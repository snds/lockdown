//! A new device's sign-in when the account asks for a security key
//! (`account-security-keys`; docs/plan/22-security-depth.md §4).
//!
//! The password goes through OPAQUE as always; the server then answers with
//! a waiting sign-in instead of a session. On the desktop the key is used
//! in the system browser, on the server's own page (`/vault/#/second-factor/<id>`,
//! where the browser's WebAuthn and the key's site both are the server), and
//! this side polls with the key only it holds until the session comes. The
//! export key the password produced is kept in memory meanwhile, so the
//! enrolment or recovery goes on without asking for the password again.

use std::rc::Rc;

use ecosystem_identity::{ExportKey, FileKeystore};
use lockdown_crypto::auth::{OpaqueAccountAuth, SecondFactorNeeded, Session as Signed};
use lockdown_sync::HttpTransport;
use lockdown_sync::account_security::session_of;
use zeroize::Zeroizing;

use super::{
    KEYSTORE_FILE, Paths, binding_of, check_root_empty, enrol_signed_in, recover_signed_in,
};

/// What the sign-in was for.
pub(crate) enum Then {
    /// Enrolling this device through the account slot (password and Secret Key).
    Enrol {
        /// The Secret Key, when the person gave it.
        secret_key: Option<Zeroizing<String>>,
    },
    /// Recovering with the kit.
    Recover {
        /// The kit's Secret Key.
        secret_key: Zeroizing<String>,
        /// The kit's recovery code.
        recovery_code: Zeroizing<String>,
    },
}

/// A sign-in waiting for a security key.
pub struct WaitingSignIn {
    paths: Paths,
    login: String,
    transport: HttpTransport,
    keystore: Rc<FileKeystore>,
    export_key: Option<ExportKey>,
    needed: SecondFactorNeeded,
    profile_id: lockdown_vault::Id,
    then: Then,
}

impl std::fmt::Debug for WaitingSignIn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WaitingSignIn")
            .field("login", &self.login)
            .field("pending", &self.needed.pending_id)
            .finish_non_exhaustive()
    }
}

/// Signs this empty device in; `Ok(Ok(lines))` when no key was needed and
/// the enrolment or recovery is done, `Ok(Err(waiting))` when a key is.
pub(crate) fn sign_in_new_device(
    paths: &Paths,
    server: &str,
    login: &str,
    password: &str,
    then: Then,
) -> Result<Result<Vec<String>, WaitingSignIn>, String> {
    check_root_empty(paths)?;
    let keystore = Rc::new(
        FileKeystore::open_or_create(&paths.data.join(KEYSTORE_FILE)).map_err(|e| e.to_string())?,
    );
    let transport = HttpTransport::new(server).map_err(|e| e.to_string())?;
    let mut auth = OpaqueAccountAuth::new(transport, login, binding_of(&keystore));
    let (export_key, session) = auth.sign_in(password).map_err(|e| e.to_string())?;
    let transport = auth.into_rounds();
    match session.second_factor.clone() {
        Some(needed) => Ok(Err(WaitingSignIn {
            paths: paths.clone(),
            login: login.to_string(),
            transport,
            keystore,
            export_key: Some(export_key),
            profile_id: session.profile_id,
            needed,
            then,
        })),
        None => finish(
            paths, &transport, &keystore, login, export_key, session, &then,
        )
        .map(Ok),
    }
}

fn finish(
    paths: &Paths,
    transport: &HttpTransport,
    keystore: &Rc<FileKeystore>,
    login: &str,
    export_key: ExportKey,
    session: Signed,
    then: &Then,
) -> Result<Vec<String>, String> {
    match then {
        Then::Enrol { secret_key } => enrol_signed_in(
            paths,
            transport,
            keystore,
            login,
            export_key,
            session,
            secret_key.as_deref().map(String::as_str),
        ),
        Then::Recover {
            secret_key,
            recovery_code,
        } => recover_signed_in(
            paths,
            transport,
            keystore,
            login,
            export_key,
            session,
            secret_key,
            recovery_code,
        ),
    }
}

impl WaitingSignIn {
    /// The page to open in the system browser, where the key is used.
    pub fn page_url(&self) -> String {
        format!(
            "{}/vault/#/second-factor/{}",
            self.transport.base().trim_end_matches('/'),
            self.needed.pending_id
        )
    }

    /// Asks the server whether the key answered; once it has, finishes what
    /// the sign-in was for and returns its lines.
    pub fn poll(&mut self) -> Result<Option<Vec<String>>, String> {
        let status = self
            .transport
            .second_factor_poll(self.needed.pending_id, &self.needed.poll_key)
            .map_err(|e| e.to_string())?;
        let Some(reply) = status.session else {
            return Ok(None);
        };
        let session = session_of(reply);
        if session.profile_id != self.profile_id || session.token.is_empty() {
            return Err("the server answered for another sign-in; sign in again".into());
        }
        let export_key = self
            .export_key
            .take()
            .ok_or("this sign-in already finished")?;
        finish(
            &self.paths,
            &self.transport,
            &self.keystore,
            &self.login,
            export_key,
            session,
            &self.then,
        )
        .map(Some)
    }
}
