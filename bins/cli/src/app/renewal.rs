//! The command line's half of session renewal
//! (docs/plan/33-session-renewal.md), over the CLI's own `sync.cbor`: a
//! token that has run out, or is about to, is renewed with this device's
//! key before the command talks to the server, and one the server refuses
//! anyway is renewed and the call tried once more. Only an ended session,
//! a revoked device or the idle limit stop it, with words that say which
//! and the command to sign in again.

use std::path::Path;

use ecosystem_identity::FileKeystore;
use ecosystem_identity::keystore::DeviceKeystore;
use lockdown_sync::HttpTransport;
use lockdown_sync::renew::{RenewError, SignInReason, renew, renewal_due};

use super::{App, SyncConfig};

/// What to add to the words of a refused renewal: how to sign in here.
const HOW: &str = " (`lockdown account login`)";

/// The token for the next call, renewed first when due; with `refused`,
/// the server refused the one in hand: a newer one another process of this
/// device saved, or a renewal now.
pub(super) fn fresh_token(
    cfg: &mut SyncConfig,
    keystore: &FileKeystore,
    path: &Path,
    refused: bool,
) -> Result<String, String> {
    let now = lockdown_vault::Timestamp::now().0;
    if !refused && !cfg.token.is_empty() && !renewal_due(cfg.expires_at, now) {
        return Ok(cfg.token.clone());
    }
    let had = cfg.token.clone();
    // The desktop app shares this data directory: take a newer token it
    // saved rather than renewing the same session twice.
    if let Ok(Some(disk)) = SyncConfig::load(path)
        && disk.server == cfg.server
        && disk.login == cfg.login
        && !disk.token.is_empty()
        && disk.expires_at > cfg.expires_at
    {
        cfg.token = disk.token;
        cfg.expires_at = disk.expires_at;
    }
    if refused && cfg.token != had && cfg.expires_at > now {
        return Ok(cfg.token.clone());
    }
    if cfg.token.is_empty() {
        return Err(
            "the session with the server has run out; run `lockdown account login` again".into(),
        );
    }
    let alive = cfg.expires_at > now;
    if !refused && !renewal_due(cfg.expires_at, now) {
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
            cfg.save(path)?;
            Ok(cfg.token.clone())
        }
        Ok(_) => Err(
            "the server renewed the session for another profile than this account's; nothing was kept"
                .into(),
        ),
        Err(RenewError::SignIn(reason)) if alive && !refused && reason != SignInReason::Ended => {
            Ok(cfg.token.clone())
        }
        Err(RenewError::SignIn(reason)) => {
            cfg.token = String::new();
            cfg.expires_at = 0;
            cfg.save(path)?;
            Err(format!("{}{HOW}", reason.message()))
        }
        Err(RenewError::Failed(_)) if alive && !refused => Ok(cfg.token.clone()),
        Err(e @ RenewError::Failed(_)) => Err(e.to_string()),
    }
}

impl App {
    /// The token for the next call to the account's server.
    pub(super) fn token(&mut self) -> Result<String, String> {
        self.token_for(false)
    }

    /// The server refused the token a call carried: another, for one more
    /// try.
    pub(super) fn token_after_refusal(&mut self) -> Result<String, String> {
        self.token_for(true)
    }

    fn token_for(&mut self, refused: bool) -> Result<String, String> {
        let path = self.paths.sync_file();
        let Some(cfg) = self.sync.as_mut() else {
            return Err("no account on this device; run `lockdown account register` or `lockdown account login` first".into());
        };
        fresh_token(cfg, self.keystore.as_ref(), &path, refused)
    }
}
