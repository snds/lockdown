//! The device's half of session renewal (docs/plan/33-session-renewal.md):
//! every call that needs the server takes its token from here, and a token
//! that has run out, or is about to, is renewed first with this device's
//! key (`lockdown_sync::renew`). The account password is needed only when
//! the server says the session was ended, the device was revoked, or the
//! session went unused past the server's idle limit; the words say which,
//! and stay with the account (`sync.cbor`) until the next sign-in.
//!
//! The data directory is shared with the `lockdown` command line, so
//! before renewing, a newer token another process of this device already
//! saved is taken instead (two renewals of one session would end each
//! other's).

use std::path::Path;

use ecosystem_identity::FileKeystore;
use ecosystem_identity::keystore::DeviceKeystore;
use lockdown_sync::HttpTransport;
use lockdown_sync::renew::{RenewError, SignInReason, renew, renewal_due};

use super::{Session, SyncConfig};

/// When a token was refused or found missing, and nothing says why.
const RUN_OUT: &str = "the session with the server has run out; log in again";

/// Whether to renew only when due, or now because the server refused the
/// token.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Need {
    /// Renew when the token has run out or is about to.
    WhenDue,
    /// The server refused this token: renew now (or take a newer one).
    Refused,
}

/// Takes the token another process of this device saved after renewing,
/// when it is newer than the one in hand.
fn adopt_newer(cfg: &mut SyncConfig, path: &Path) {
    if let Ok(Some(disk)) = SyncConfig::load(path)
        && disk.server == cfg.server
        && disk.login == cfg.login
        && !disk.token.is_empty()
        && disk.token != cfg.token
        && disk.expires_at > cfg.expires_at
    {
        cfg.token = disk.token;
        cfg.expires_at = disk.expires_at;
        cfg.sign_in = None;
    }
}

/// The token for the next call: `cfg`'s own, a newer one this device
/// already saved, or a renewed one. Fails with the words of
/// [`SignInReason`] when only the password helps.
pub(crate) fn fresh_token(
    cfg: &mut SyncConfig,
    keystore: &FileKeystore,
    path: &Path,
    need: Need,
) -> Result<String, String> {
    let now = lockdown_vault::Timestamp::now().0;
    // The usual case: a token in hand, far from running out. Nothing is
    // read or sent.
    if need == Need::WhenDue && !cfg.token.is_empty() && !renewal_due(cfg.expires_at, now) {
        return Ok(cfg.token.clone());
    }
    let refused_token = cfg.token.clone();
    adopt_newer(cfg, path);
    if need == Need::Refused && cfg.token != refused_token && cfg.expires_at > now {
        return Ok(cfg.token.clone());
    }
    if cfg.token.is_empty() {
        return Err(match cfg.sign_in.as_deref().and_then(SignInReason::parse) {
            Some(reason) => reason.message().to_string(),
            None => RUN_OUT.to_string(),
        });
    }
    let alive = cfg.expires_at > now;
    if need == Need::WhenDue && !renewal_due(cfg.expires_at, now) {
        return Ok(cfg.token.clone());
    }
    let outcome = HttpTransport::new(&cfg.server)
        .map_err(|e| RenewError::Failed(e.to_string()))
        .and_then(|transport| {
            renew(&transport, &cfg.token, &|m| {
                DeviceKeystore::sign(keystore, m).map_err(lockdown_crypto::Error::from)
            })
        });
    match outcome {
        Ok(renewed) if renewed.profile_id == cfg.profile_id => {
            cfg.token = renewed.token;
            cfg.expires_at = renewed.expires_at;
            cfg.sign_in = None;
            cfg.save(path)?;
            Ok(cfg.token.clone())
        }
        Ok(_) => Err(
            "the server renewed the session for another profile than this account's; nothing was kept"
                .into(),
        ),
        // A token that still works keeps working until it runs out, unless
        // the server said the session itself was ended.
        Err(RenewError::SignIn(reason))
            if alive && need == Need::WhenDue && reason != SignInReason::Ended =>
        {
            Ok(cfg.token.clone())
        }
        Err(RenewError::SignIn(reason)) => {
            cfg.token = String::new();
            cfg.expires_at = 0;
            cfg.sign_in = Some(reason.as_str().to_string());
            cfg.save(path)?;
            Err(reason.message().to_string())
        }
        Err(RenewError::Failed(_)) if alive && need == Need::WhenDue => Ok(cfg.token.clone()),
        Err(e @ RenewError::Failed(_)) => Err(e.to_string()),
    }
}

impl Session {
    /// The token for the next call to the account's server, renewed with
    /// this device's key when it has run out or is about to.
    pub(crate) fn token(&mut self) -> Result<String, String> {
        self.token_for(Need::WhenDue)
    }

    /// The server refused the token a call carried: a newer one, or a
    /// renewal, for one more try.
    pub(crate) fn token_after_refusal(&mut self) -> Result<String, String> {
        self.token_for(Need::Refused)
    }

    fn token_for(&mut self, need: Need) -> Result<String, String> {
        let path = self.paths.sync_file();
        let Some(cfg) = self.sync.as_mut() else {
            return Err("no account on this device; register or log in first".into());
        };
        fresh_token(cfg, self.keystore.as_ref(), &path, need)
    }

    /// The account's server, with no call to it.
    pub(crate) fn account_server(&self) -> Option<String> {
        self.sync.as_ref().map(|c| c.server.clone())
    }

    /// Why this device must sign in with the password, when it must.
    pub(crate) fn sign_in_needed(&self) -> Option<SignInReason> {
        let cfg = self.sync.as_ref()?;
        if !cfg.token.is_empty() {
            return None;
        }
        cfg.sign_in.as_deref().and_then(SignInReason::parse)
    }
}
