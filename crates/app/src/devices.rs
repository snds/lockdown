//! The device and account calls of the security-depth slice, as a shell
//! makes them (docs/plan/22-security-depth.md §2–§4):
//!
//! - **Approve a new device from this one**: [`App::link_offer`] makes a
//!   code and a QR payload, [`App::link_poll`] watches for the new device
//!   and shows the six digits, [`App::link_approve`] enrols it after the
//!   `Approve` gate (D99: the platform check confirms, the digits are the
//!   check), [`App::link_decline`] writes nothing.
//! - **Join from another device** (this one empty): [`App::join_start`]
//!   with the typed code or the scanned payload, [`App::join_poll`] until
//!   the digits and then the approval, [`App::join_finish`] with the
//!   account password. No kit is typed.
//! - **The Secret Key**: [`App::secret_key_status`] says whether this
//!   device keeps it (so it can hand it on); [`App::account_register_opted_out`]
//!   is the documented opt-out, refused until the person confirms the
//!   warning.
//! - **Security keys on the account**: [`App::security_keys`],
//!   [`App::security_key_add_page`] (the system browser adds one on the
//!   server's own page), [`App::security_key_remove`] (`Approve` gate); a
//!   new device's sign-in that waits for one comes back as
//!   `approval_needed` with detail `security_key`, [`App::second_factor_page`]
//!   names the page to open and [`App::second_factor_wait`] finishes once
//!   the key answered.

use std::time::Instant;

use lockdown_sync::HttpTransport;
pub use lockdown_sync::account_security::{SecurityKeyInfo, SecurityKeyList};
use serde::Serialize;
use zeroize::Zeroizing;

pub use crate::session::devices_link::{JoinView, LinkView};
use crate::session::devices_link::{Joining, SignIn, finish_joined};
use crate::session::second_factor::{Then, WaitingSignIn, sign_in_new_device};
use crate::{App, AppError, AppResult, ApprovalKind, ErrorCode, GateInput, Registered, Status};

/// What a core keeps between a shell's calls for these flows. Dropped with
/// everything else when the app stops; a lock leaves a join alone (no
/// vault is open during one) but a waiting sign-in holds an export key, so
/// [`Depth::forget`] drops it.
#[derive(Default)]
pub(crate) struct Depth {
    joining: Option<Joining>,
    waiting: Option<WaitingSignIn>,
    /// This device's unlock is the account password (the web vault), so the
    /// password reminder never applies (`reminders`).
    pub(crate) unlock_asks_password: bool,
}

impl Depth {
    /// Drops what holds a key.
    pub(crate) fn forget(&mut self) {
        self.waiting = None;
    }
}

/// Whether this device keeps the Secret Key, in a word a shell switches on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SecretKeyView {
    /// `kept`, `opted-out` or `not-kept`.
    pub state: String,
    /// In plain words.
    pub words: String,
}

fn waiting_error(w: &WaitingSignIn) -> AppError {
    AppError {
        code: ErrorCode::ApprovalNeeded,
        message: format!(
            "your account asks for a security key on a new device: use it on the page that opens ({})",
            w.page_url()
        ),
        detail: Some("security_key".into()),
    }
}

impl App {
    // --- approving a new device from this one ------------------------------------

    /// Opens a link for a new device: the code to type there and the QR
    /// payload to scan (needs the account on this device).
    pub fn link_offer(&self) -> AppResult<LinkView> {
        self.with_session(|s| s.link_offer())
    }

    /// Where the link stands; a new device's request is answered here.
    pub fn link_poll(&self) -> AppResult<LinkView> {
        self.call(|core| core.session()?.link_poll().map_err(AppError::refused))
    }

    /// Enrols the new device, after the `Approve` gate: the person compared
    /// the digits; the platform check (or the password) confirms it is them.
    pub fn link_approve(&self, gate: GateInput) -> AppResult<Vec<String>> {
        self.call(move |core| {
            core.clock.touch(Instant::now());
            core.approve(
                ApprovalKind::Approve,
                "add that device to your account",
                &gate,
            )?;
            core.session()?.link_approve().map_err(AppError::refused)
        })
    }

    /// Says no to the new device (or stops waiting for one).
    pub fn link_decline(&self) -> AppResult<()> {
        self.with_session(|s| s.link_decline())
    }

    // --- joining from another device ------------------------------------------------

    /// On a device with nothing on it: joins the link `invite` names (the
    /// typed code, or the QR payload, which also names the server and pins
    /// the other device's key).
    pub fn join_start(&self, server: &str, invite: &str) -> AppResult<JoinView> {
        let (server, invite) = (server.to_string(), Zeroizing::new(invite.to_string()));
        self.call(move |core| {
            let joining =
                Joining::start(&core.paths, &server, &invite).map_err(AppError::refused)?;
            let view = joining.view();
            core.depth.joining = Some(joining);
            Ok(view)
        })
    }

    /// Where the join stands: the digits to compare, then the approval
    /// (the identity is restored here the moment the other device approves).
    pub fn join_poll(&self) -> AppResult<JoinView> {
        self.call(|core| {
            let joining = core
                .depth
                .joining
                .as_mut()
                .ok_or_else(|| AppError::refused("no join is under way"))?;
            match joining.poll() {
                Ok(v) => Ok(v),
                Err(e) => {
                    core.depth.joining = None;
                    Err(AppError::refused(e))
                }
            }
        })
    }

    /// Finishes an approved join: signs in with the account password, then
    /// opens the vault (pulling it from the server).
    pub fn join_finish(&self, login: &str, password: &str) -> AppResult<Status> {
        let login = login.to_string();
        let password = Zeroizing::new(password.to_string());
        self.call(move |core| {
            let (server, sk) = match core.depth.joining.as_ref() {
                Some(j) => (j.server().to_string(), j.secret_key()),
                None => return Err(AppError::refused("no join is under way")),
            };
            finish_joined(
                &core.paths,
                &server,
                &login,
                SignIn::Password(&password),
                sk,
            )
            .map_err(AppError::refused)?;
            core.depth.joining = None;
            core.open(crate::session::Mode::Init)
        })
    }

    /// [`App::join_finish`] after a sign-in the shell ran itself with this
    /// device's keystore (the web vault, which seals its device key with
    /// the export key). Does not open the vault; the shell does.
    pub fn join_finish_signed_in(
        &self,
        login: &str,
        export_key: ecosystem_identity::ExportKey,
        session: lockdown_crypto::auth::Session,
    ) -> AppResult<Vec<String>> {
        let login = login.to_string();
        self.call(move |core| {
            let (server, sk) = match core.depth.joining.as_ref() {
                Some(j) => (j.server().to_string(), j.secret_key()),
                None => return Err(AppError::refused("no join is under way")),
            };
            let lines = finish_joined(
                &core.paths,
                &server,
                &login,
                SignIn::Done(export_key, session),
                sk,
            )
            .map_err(AppError::refused)?;
            core.depth.joining = None;
            Ok(lines)
        })
    }

    /// Stops a join (nothing is kept unless the other device already approved).
    pub fn join_cancel(&self) -> AppResult<()> {
        self.call(|core| {
            core.depth.joining = None;
            Ok(())
        })
    }

    // --- the Secret Key ------------------------------------------------------------

    /// Whether this device keeps the Secret Key, so an approval can hand it on.
    pub fn secret_key_status(&self) -> AppResult<SecretKeyView> {
        self.with_session(|s| {
            let state = crate::session::secret_key::state_word(&s.secret_key_state());
            let words = match state {
                "kept" => "This device keeps your Secret Key, sealed by its own key, and hands it to a new device you approve.",
                "opted-out" => "Your account has no Secret Key: the password alone opens it on a new device. Someone who steals the server's database can try guessing your password offline.",
                _ => "This device doesn't keep your Secret Key. A device you approve from here asks for the kit if it ever needs it; type the Secret Key in a recovery drill to keep it here.",
            };
            Ok(SecretKeyView {
                state: state.into(),
                words: words.into(),
            })
        })
    }

    /// Makes the account **without** a Secret Key (04-security §3's
    /// opt-out): refused unless `understood`, because then the password
    /// alone protects the account against a stolen server database.
    pub fn account_register_opted_out(
        &self,
        server: &str,
        login: &str,
        password: &str,
        understood: bool,
    ) -> AppResult<Registered> {
        if !understood {
            return Err(AppError {
                code: ErrorCode::Refused,
                message: "Without a Secret Key, your password alone protects your account: someone who copies the server's database can try guessing it offline, and nothing but its length slows them. Confirm that you understand to make the account this way.".into(),
                detail: Some("confirm_opt_out".into()),
            });
        }
        let (server, login) = (server.to_string(), login.to_string());
        let password = Zeroizing::new(password.to_string());
        self.with_session(move |s| s.account_register_opted_out(&server, &login, &password))
    }

    // --- security keys on the account ------------------------------------------------

    /// The account's security keys and passkeys.
    pub fn security_keys(&self) -> AppResult<SecurityKeyList> {
        self.with_session(|s| {
            let (transport, token) = s.account_transport()?;
            transport.security_keys(&token).map_err(|e| e.to_string())
        })
    }

    /// Starts adding a key and returns the server's page to open in the
    /// system browser, where the browser's WebAuthn talks to the key (the
    /// desktop has no WebAuthn of its own without the OS-level work D122
    /// defers). The page finishes the registration itself.
    pub fn security_key_add_page(&self) -> AppResult<String> {
        self.with_session(|s| {
            let (transport, token) = s.account_transport()?;
            let request = transport
                .security_key_start(&token)
                .map_err(|e| e.to_string())?;
            Ok(format!(
                "{}/vault/#/security-key/{}",
                transport.base().trim_end_matches('/'),
                request.pending_id
            ))
        })
    }

    /// Removes a key from the account, after the `Approve` gate.
    pub fn security_key_remove(&self, id: &str, gate: GateInput) -> AppResult<SecurityKeyList> {
        let id = id.to_string();
        self.call(move |core| {
            core.clock.touch(Instant::now());
            core.approve(
                ApprovalKind::Approve,
                "remove that security key from your account",
                &gate,
            )?;
            let s = core.session()?;
            let (transport, token) = s.account_transport().map_err(AppError::refused)?;
            transport
                .security_key_remove(&token, &id)
                .map_err(|e| AppError::refused(e.to_string()))
        })
    }

    /// Signs this empty device in to enrol it (password and Secret Key);
    /// when the account wants a security key, `approval_needed` with detail
    /// `security_key`, and the shell opens [`App::second_factor_page`].
    pub(crate) fn enrol_new_device(
        &self,
        server: &str,
        login: &str,
        password: &str,
        secret_key: Option<&str>,
    ) -> AppResult<Vec<String>> {
        let (server, login) = (server.to_string(), login.to_string());
        let password = Zeroizing::new(password.to_string());
        let secret_key = secret_key.map(|s| Zeroizing::new(s.to_string()));
        self.call(move |core| {
            start_waiting(
                core,
                sign_in_new_device(
                    &core.paths,
                    &server,
                    &login,
                    &password,
                    Then::Enrol { secret_key },
                ),
            )
        })
    }

    /// The recovery with the kit, with the same wait for a security key.
    pub(crate) fn recover_new_device(
        &self,
        server: &str,
        login: &str,
        password: &str,
        secret_key: &str,
        recovery_code: &str,
    ) -> AppResult<Vec<String>> {
        let (server, login) = (server.to_string(), login.to_string());
        let password = Zeroizing::new(password.to_string());
        let secret_key = Zeroizing::new(secret_key.to_string());
        let recovery_code = Zeroizing::new(recovery_code.to_string());
        self.call(move |core| {
            start_waiting(
                core,
                sign_in_new_device(
                    &core.paths,
                    &server,
                    &login,
                    &password,
                    Then::Recover {
                        secret_key,
                        recovery_code,
                    },
                ),
            )
        })
    }

    /// The page where the waiting sign-in's security key is used.
    pub fn second_factor_page(&self) -> AppResult<String> {
        self.call(|core| {
            core.depth
                .waiting
                .as_ref()
                .map(WaitingSignIn::page_url)
                .ok_or_else(|| AppError::refused("no sign-in is waiting for a security key"))
        })
    }

    /// Whether the key answered; `Some(lines)` once the enrolment or
    /// recovery finished, `None` while it still waits.
    pub fn second_factor_wait(&self) -> AppResult<Option<Vec<String>>> {
        self.call(|core| {
            let w = core
                .depth
                .waiting
                .as_mut()
                .ok_or_else(|| AppError::refused("no sign-in is waiting for a security key"))?;
            match w.poll() {
                Ok(None) => Ok(None),
                Ok(Some(lines)) => {
                    core.depth.waiting = None;
                    Ok(Some(lines))
                }
                Err(e) => {
                    core.depth.waiting = None;
                    Err(AppError::refused(e))
                }
            }
        })
    }
}

fn start_waiting(
    core: &mut crate::Core,
    step: Result<Result<Vec<String>, WaitingSignIn>, String>,
) -> AppResult<Vec<String>> {
    match step.map_err(AppError::refused)? {
        Ok(lines) => Ok(lines),
        Err(waiting) => {
            let err = waiting_error(&waiting);
            core.depth.waiting = Some(waiting);
            Err(err)
        }
    }
}

impl crate::session::Session {
    /// The account's transport and session token.
    pub(crate) fn account_transport(&self) -> Result<(HttpTransport, String), String> {
        let (transport, token, _) = self.account_online()?;
        Ok((transport, token))
    }
}
