//! The web vault's part of security depth (docs/plan/22-security-depth.md
//! §2–§4):
//!
//! - **A sign-in that waits for a security key.** Every sign-in enrols
//!   this tab as a device the account has never seen, so an account with
//!   keys answers OPAQUE with a waiting sign-in. The tab keeps the export
//!   key and its sealed device key in memory, the page uses the key with
//!   the browser's own WebAuthn (the server is the relying party: the page
//!   comes from it), and [`WebVault::sign_in_continue`] collects the
//!   session and finishes the enrolment, without asking for the password
//!   again.
//! - **Joining from another device.** A tab with nothing in it joins a
//!   link the desktop opened: the code typed, or the QR code read with the
//!   camera. The other device approves, the identity (and the Secret Key,
//!   when the other device keeps it) arrive over the handshake, and the
//!   tab signs in with the password; its device key is sealed under that
//!   sign-in's export key as every tab's is.
//! - **The pages the desktop opens in the system browser.** Adding a key
//!   to the account, and using one for a desktop's waiting sign-in, need a
//!   browser at the server's origin: `#/security-key/<id>` and
//!   `#/second-factor/<id>`. The id is the capability; no account is
//!   signed in on that page. [`page_key_request`], [`page_key_finish`],
//!   [`page_sign_in_request`] and [`page_sign_in_answer`] carry them,
//!   to the page's own origin only.
//!
//! What the browser's WebAuthn hands back is hostile input to the server,
//! which checks it (`lockdown_passkey::verify`); this module only moves it.

use ecosystem_identity::ExportKey;
use lockdown_app::{AppError, AppResult, ErrorCode, Status};
use lockdown_crypto::auth::SecondFactorNeeded;
use lockdown_sync::HttpTransport;
use lockdown_sync::account_security::{
    KeyRegisterFinish, SecondFactorAnswer, SecurityKeyInfo, session_of,
};
use lockdown_vault::Id;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use zeroize::Zeroizing;

use crate::host::{SealedDevice, WebVault, sign_in_round, trimmed};

fn refused(message: impl Into<String>) -> AppError {
    AppError::refused(message)
}

/// A sign-in that passed the password and waits for a security key.
pub(crate) struct Waiting {
    server: String,
    login: String,
    export_key: ExportKey,
    sealed: SealedDevice,
    needed: SecondFactorNeeded,
    secret_key: Option<Zeroizing<String>>,
}

/// What `navigator.credentials.get` needs, with bytes as arrays of numbers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GetRequest {
    /// The waiting sign-in.
    pub pending_id: String,
    /// The challenge.
    pub challenge: Vec<u8>,
    /// The relying party (the server's host).
    pub rp_id: String,
    /// The credential ids that may answer.
    pub allow: Vec<Vec<u8>>,
    /// How long the browser may wait.
    pub timeout_ms: u32,
    /// When the sign-in stops waiting.
    pub expires_at: i64,
    /// Which device is signing in, for the page to say.
    pub device_label: String,
}

/// What `navigator.credentials.create` needs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CreateRequest {
    /// The registration.
    pub pending_id: String,
    /// The challenge.
    pub challenge: Vec<u8>,
    /// The relying party.
    pub rp_id: String,
    /// Its name.
    pub rp_name: String,
    /// The user handle.
    pub user_id: Vec<u8>,
    /// The login name.
    pub user_name: String,
    /// Keys already on the account.
    pub exclude: Vec<Vec<u8>>,
    /// How long the browser may wait.
    pub timeout_ms: u32,
}

/// The browser's answer to `get`, as the page hands it over.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetAnswer {
    /// `rawId`.
    pub credential_id: Vec<u8>,
    /// `response.clientDataJSON`.
    pub client_data_json: Vec<u8>,
    /// `response.authenticatorData`.
    pub authenticator_data: Vec<u8>,
    /// `response.signature`.
    pub signature: Vec<u8>,
}

/// The browser's answer to `create`.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAnswer {
    /// What the person calls the key.
    pub name: String,
    /// `response.clientDataJSON`.
    pub client_data_json: Vec<u8>,
    /// `response.attestationObject`.
    pub attestation_object: Vec<u8>,
}

fn bufs(v: Vec<ByteBuf>) -> Vec<Vec<u8>> {
    v.into_iter().map(ByteBuf::into_vec).collect()
}

fn parse_id(id: &str) -> AppResult<Id> {
    Id::parse(id).map_err(|_| refused("that isn't a sign-in or registration id"))
}

fn transport(origin: &str) -> AppResult<HttpTransport> {
    HttpTransport::new(origin).map_err(|e| refused(e.to_string()))
}

fn answer_of(pending_id: Id, a: GetAnswer) -> SecondFactorAnswer {
    SecondFactorAnswer {
        pending_id,
        credential_id: ByteBuf::from(a.credential_id),
        client_data_json: ByteBuf::from(a.client_data_json),
        authenticator_data: ByteBuf::from(a.authenticator_data),
        signature: ByteBuf::from(a.signature),
    }
}

/// A waiting sign-in's WebAuthn request, by its id (the page the desktop
/// opened in the system browser, or this tab's own sign-in).
pub fn page_sign_in_request(origin: &str, pending_id: &str) -> AppResult<GetRequest> {
    let r = transport(origin)?
        .second_factor_request(parse_id(pending_id)?)
        .map_err(|e| refused(e.to_string()))?;
    Ok(GetRequest {
        pending_id: r.pending_id.to_string(),
        challenge: r.challenge.into_vec(),
        rp_id: r.rp_id,
        allow: bufs(r.allow),
        timeout_ms: r.timeout_ms,
        expires_at: r.expires_at,
        device_label: r.device_label,
    })
}

/// Hands the key's answer for a waiting sign-in to the server, which
/// checks it.
pub fn page_sign_in_answer(origin: &str, pending_id: &str, answer: GetAnswer) -> AppResult<()> {
    transport(origin)?
        .second_factor_answer(&answer_of(parse_id(pending_id)?, answer))
        .map_err(|e| refused(e.to_string()))
}

/// A key registration's WebAuthn request, by its id.
pub fn page_key_request(origin: &str, pending_id: &str) -> AppResult<CreateRequest> {
    let r = transport(origin)?
        .security_key_request(parse_id(pending_id)?)
        .map_err(|e| refused(e.to_string()))?;
    Ok(CreateRequest {
        pending_id: r.pending_id.to_string(),
        challenge: r.challenge.into_vec(),
        rp_id: r.rp_id,
        rp_name: r.rp_name,
        user_id: r.user_id.into_vec(),
        user_name: r.user_name,
        exclude: bufs(r.exclude),
        timeout_ms: r.timeout_ms,
    })
}

/// Hands a new key to the server, which checks it and adds it.
pub fn page_key_finish(
    origin: &str,
    pending_id: &str,
    answer: CreateAnswer,
) -> AppResult<SecurityKeyInfo> {
    transport(origin)?
        .security_key_finish(&KeyRegisterFinish {
            pending_id: parse_id(pending_id)?,
            name: answer.name,
            client_data_json: ByteBuf::from(answer.client_data_json),
            attestation_object: ByteBuf::from(answer.attestation_object),
        })
        .map_err(|e| refused(e.to_string()))
}

impl WebVault {
    /// Keeps a sign-in that passed the password until a key answers, and
    /// says so as the error the page knows (`security_key`).
    pub(crate) fn wait_for_key(
        &mut self,
        server: &str,
        login: &str,
        export_key: ExportKey,
        sealed: SealedDevice,
        needed: SecondFactorNeeded,
        secret_key: Option<&str>,
    ) -> AppError {
        let pending = needed.pending_id;
        let email_to =
            lockdown_sync::account_security::request_of(&needed).and_then(|r| r.email_to);
        self.waiting = Some(Waiting {
            server: server.to_string(),
            login: login.to_string(),
            export_key,
            sealed,
            needed,
            secret_key: secret_key.map(|s| Zeroizing::new(s.to_string())),
        });
        // A server with email approval mailed a link instead of asking for
        // a key (docs/plan/25-self-host-2.md §5).
        if let Some(to) = email_to {
            return AppError {
                code: ErrorCode::ApprovalNeeded,
                message: format!(
                    "the server emailed a link to {to}: open it and choose Approve, and this tab carries on by itself"
                ),
                detail: Some("email".into()),
            };
        }
        AppError {
            code: ErrorCode::ApprovalNeeded,
            message: format!(
                "your account asks for a security key on a new device; use it now (sign-in {pending})"
            ),
            detail: Some("security_key".into()),
        }
    }

    /// The waiting sign-in's WebAuthn request, for this tab's own page.
    pub fn sign_in_waiting(&self) -> AppResult<GetRequest> {
        let w = self
            .waiting
            .as_ref()
            .ok_or_else(|| refused("no sign-in is waiting for a security key"))?;
        page_sign_in_request(&w.server, &w.needed.pending_id.to_string())
    }

    /// The key answered in this tab: the server checks it, the session is
    /// collected and the sign-in finishes. A refused answer leaves the
    /// sign-in waiting, so the person can try the key again.
    pub fn sign_in_answer(&mut self, answer: GetAnswer) -> AppResult<Status> {
        let (server, pending) = match self.waiting.as_ref() {
            Some(w) => (w.server.clone(), w.needed.pending_id),
            None => return Err(refused("no sign-in is waiting for a security key")),
        };
        transport(&server)?
            .second_factor_answer(&answer_of(pending, answer))
            .map_err(|e| refused(e.to_string()))?;
        self.sign_in_continue()?.ok_or_else(|| {
            refused("the server took the key but hasn't let the sign-in on; try again")
        })
    }

    /// Asks whether the key answered (here or anywhere); finishes the
    /// sign-in once it has. `None` while it still waits.
    pub fn sign_in_continue(&mut self) -> AppResult<Option<Status>> {
        let (server, pending, poll_key) = match self.waiting.as_ref() {
            Some(w) => (
                w.server.clone(),
                w.needed.pending_id,
                w.needed.poll_key.clone(),
            ),
            None => return Err(refused("no sign-in is waiting for a security key")),
        };
        let status = match transport(&server)?.second_factor_poll(pending, &poll_key) {
            Ok(s) => s,
            Err(e) => {
                let _ = self.start_empty(true);
                return Err(refused(e.to_string()));
            }
        };
        let Some(reply) = status.session else {
            return Ok(None);
        };
        let w = self.waiting.take().expect("checked above");
        let session = session_of(reply);
        let out = self.finish_sign_in(
            &w.server,
            &w.login,
            w.export_key,
            session,
            w.sealed,
            w.secret_key.as_deref().map(String::as_str),
        );
        if out.is_err() {
            let _ = self.start_empty(true);
        }
        out.map(Some)
    }

    /// Joins a link another device opened (the code typed, or the QR code
    /// read with the camera), on this page's own server only.
    pub fn join_start(&mut self, invite: &str) -> AppResult<serde_json::Value> {
        let origin = trimmed(self.origin_for_join()?);
        self.start_empty(true)?;
        let view = self.app().join_start(&origin, invite)?;
        serde_json::to_value(view).map_err(|e| refused(e.to_string()))
    }

    /// Where the join stands.
    pub fn join_poll(&mut self) -> AppResult<serde_json::Value> {
        let view = self.app().join_poll()?;
        serde_json::to_value(view).map_err(|e| refused(e.to_string()))
    }

    /// Finishes an approved join: a sign-in with the account password,
    /// whose export key seals this tab's device key, then the vault opens.
    pub fn join_finish(&mut self, login: &str, password: &str) -> AppResult<Status> {
        let server = trimmed(self.origin_for_join()?);
        let (export_key, session, sealed) =
            sign_in_round(self.paths(), &server, login, password, None)?;
        if session.second_factor.is_some() {
            return Err(refused(
                "the server didn't take the other device's approval; start the join again",
            ));
        }
        self.app()
            .join_finish_signed_in(login, export_key, session)?;
        self.opened(&server, login, sealed)
    }

    fn origin_for_join(&self) -> AppResult<&str> {
        self.serving_origin()
            .ok_or_else(|| refused("the web vault doesn't know which server it came from"))
    }
}
