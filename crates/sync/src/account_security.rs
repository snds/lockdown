//! The wire for the security-depth slice (docs/plan/22-security-depth.md):
//! the relay two devices meet through to approve a new one, the relay two
//! people meet through to check each other's keys with a phrase, raising an
//! account's password stretching, and security keys as a second factor.
//! Inherent on [`HttpTransport`], like the alerts and sessions, so the sync
//! engine's test transports need nothing new.
//!
//! | Method and path | Body in | Body out | Needs a session |
//! |---|---|---|---|
//! | `POST /v1/links` | [`LinkOpen`] | [`LinkOpened`] | yes (the existing device) |
//! | `POST /v1/links/join` | [`LinkJoin`] | [`LinkJoined`] | no: the code is the capability |
//! | `POST /v1/links/{id}/frames` | [`FramePush`] | empty | yes (the existing device) |
//! | `POST /v1/links/{id}/poll` | [`FramePoll`] | [`Frames`] | yes (the existing device) |
//! | `POST /v1/links/{id}/joiner/frames` | [`FramePush`] | empty | the joiner's key |
//! | `POST /v1/links/{id}/joiner/poll` | [`FramePoll`] | [`Frames`] | the joiner's key |
//! | `POST /v1/links/{id}/device` | [`LinkDevice`] | empty | yes: binds the approved device |
//! | `POST /v1/links/{id}/close` | | empty | yes |
//! | `POST /v1/account/password/start` | [`PasswordStart`] | [`PasswordStartReply`] | yes |
//! | `POST /v1/account/password/finish` | [`PasswordFinish`] | empty | yes |
//! | `GET /v1/account/security-keys` | | [`SecurityKeyList`] | yes |
//! | `POST /v1/account/security-keys/start` | | [`CreationRequest`] | yes |
//! | `POST /v1/account/security-keys/request` | [`SecondFactorLookup`] | [`CreationRequest`] | no: the registration's id |
//! | `POST /v1/account/security-keys/finish` | [`KeyRegisterFinish`] | [`SecurityKeyInfo`] | no: the registration's id |
//! | `POST /v1/account/security-keys/{id}/remove` | | [`SecurityKeyList`] | yes |
//! | `POST /v1/account/login/second-factor` | [`SecondFactorAnswer`] | empty | no: the pending id |
//! | `POST /v1/account/login/second-factor/request` | [`SecondFactorLookup`] | [`SecondFactorRequest`] | no |
//! | `POST /v1/account/login/second-factor/poll` | [`SecondFactorPoll`] | [`SecondFactorStatus`] | no: the poll key |
//! | `POST /v1/emergency/{grant}/items` | [`EmergencyItems`] | empty | yes (the owner) |
//! | `GET /v1/emergency/{grant}/items` | | [`EmergencyItems`] | yes (the contact, once released) |
//! | `POST /v1/verifications` | [`VerifyOpen`] | [`VerifyOpened`] | yes |
//! | `GET /v1/verifications` | | [`VerifyList`] | yes |
//! | `POST /v1/verifications/{id}/frames` | [`FramePush`] | empty | yes (either person) |
//! | `POST /v1/verifications/{id}/poll` | [`FramePoll`] | [`Frames`] | yes (either person) |
//!
//! The relays carry frames they can't read the meaning of: a link's frames
//! are the identity crate's handshake (`lockdown_vault::device_link`), a
//! verification's are an OPAQUE exchange keyed by the phrase. A frame is at
//! most [`MAX_FRAME`] bytes and each side sends at most [`MAX_FRAMES`].

use lockdown_crypto::Argon2Params;
use lockdown_vault::Id;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

use crate::transport::{HttpTransport, Method, Result, TransportError};

/// The largest frame either relay carries.
pub const MAX_FRAME: usize = 256 * 1024;
/// How many frames each side of a relay may send.
pub const MAX_FRAMES: usize = 8;

// --- device links ------------------------------------------------------------------

/// The existing device opens a link: the code's hash (never the code) and
/// its first frame.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct LinkOpen {
    /// `LinkCode::token_hash`.
    #[serde(with = "serde_bytes")]
    pub token_hash: [u8; 32],
    /// The Hello frame.
    pub hello: ByteBuf,
}

/// The link, as the server filed it.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct LinkOpened {
    /// Its id.
    pub link_id: Id,
    /// When it closes on its own.
    pub expires_at: i64,
}

/// The new device joins with the code it was shown.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct LinkJoin {
    /// The ten characters, as `LinkCode::as_str`.
    pub code: String,
}

/// What joining gives the new device: the link and a key only it holds.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct LinkJoined {
    /// The link.
    pub link_id: Id,
    /// The new device's side of the relay; the server keeps only its hash.
    pub key: ByteBuf,
    /// When the link closes.
    pub expires_at: i64,
}

/// One frame up. `key` is the joiner's; the existing device uses its session.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct FramePush {
    /// The joiner's key, or none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<ByteBuf>,
    /// The frame.
    pub frame: ByteBuf,
}

/// Frames from the other side after the first `after`.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct FramePoll {
    /// The joiner's key, or none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<ByteBuf>,
    /// How many frames the asker already read.
    pub after: u32,
}

/// The other side's frames.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, Default)]
pub struct Frames {
    /// Frames `after + 1` onward.
    pub frames: Vec<ByteBuf>,
    /// The link was closed (by either side, or it ran out).
    pub closed: bool,
    /// When it closes on its own.
    pub expires_at: i64,
}

/// The existing device tells the server which device it approved, so that
/// device's first sign-in counts as a device the account already knows.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct LinkDevice {
    /// The new device.
    pub device_id: Id,
    /// Its Ed25519 public key.
    #[serde(with = "serde_bytes")]
    pub device_pub: [u8; 32],
}

// --- raising the password stretching ----------------------------------------------

/// Re-registration, first round trip, for the signed-in account.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct PasswordStart {
    /// The OPAQUE registration request.
    pub request: ByteBuf,
}

/// The OPAQUE registration response.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct PasswordStartReply {
    /// The response.
    pub response: ByteBuf,
}

/// Re-registration, second round trip: the new password file and the
/// stretching it was made under. Replaces the old file at once.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct PasswordFinish {
    /// The OPAQUE upload.
    pub upload: ByteBuf,
    /// The stretching.
    pub ksf: Argon2Params,
}

// --- security keys -------------------------------------------------------------------

/// What `navigator.credentials.create` needs to add a security key.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct CreationRequest {
    /// The server's id for this registration.
    pub pending_id: Id,
    /// The random challenge.
    pub challenge: ByteBuf,
    /// The relying party: the server's host name.
    pub rp_id: String,
    /// The relying party's name, shown by the browser.
    pub rp_name: String,
    /// The account, as WebAuthn's user handle.
    pub user_id: ByteBuf,
    /// The login name, shown by the browser.
    pub user_name: String,
    /// Keys already registered, so the same one isn't added twice.
    pub exclude: Vec<ByteBuf>,
    /// How long the browser may wait, in milliseconds.
    pub timeout_ms: u32,
}

/// The browser's answer to a creation.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct KeyRegisterFinish {
    /// From [`CreationRequest`].
    pub pending_id: Id,
    /// What the person calls it.
    pub name: String,
    /// `response.clientDataJSON`.
    pub client_data_json: ByteBuf,
    /// `response.attestationObject`.
    pub attestation_object: ByteBuf,
}

/// One registered key.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SecurityKeyInfo {
    /// The credential id, base64url.
    pub id: String,
    /// What the person calls it.
    pub name: String,
    /// `es256` or `ed25519`.
    pub alg: String,
    /// When it was added.
    pub created_at: i64,
    /// When it last signed a sign-in in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used: Option<i64>,
}

/// The account's keys.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, Default)]
pub struct SecurityKeyList {
    /// Oldest first.
    pub keys: Vec<SecurityKeyInfo>,
    /// The relying party id a key is registered under (the server's host).
    pub rp_id: String,
}

/// What a sign-in waiting for a second factor needs: how to ask the key,
/// and how this device collects the session afterwards.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SecondFactorPending {
    /// The waiting sign-in.
    pub pending_id: Id,
    /// Only the signing-in device holds it: the session is handed out
    /// against it, never to the page that used the key.
    pub poll_key: ByteBuf,
    /// The WebAuthn `get` request.
    pub request: SecondFactorRequest,
}

/// What `navigator.credentials.get` needs.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SecondFactorRequest {
    /// The waiting sign-in.
    pub pending_id: Id,
    /// The random challenge.
    pub challenge: ByteBuf,
    /// The server's host name.
    pub rp_id: String,
    /// The keys that may answer (credential ids).
    pub allow: Vec<ByteBuf>,
    /// How long the browser may wait, in milliseconds.
    pub timeout_ms: u32,
    /// When the waiting sign-in is dropped.
    pub expires_at: i64,
    /// The device that is signing in, as it named itself (for the page in
    /// the system browser to say which sign-in it confirms).
    pub device_label: String,
    /// When the account's email approves the sign-in instead of a key: the
    /// address the link went to, masked (`s•••@example.com`). `allow` is
    /// then empty and no key can answer (docs/plan/25-self-host-2.md §5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email_to: Option<String>,
}

/// Looks a waiting sign-in up by id (the system browser's page, which has
/// only the id from the address).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SecondFactorLookup {
    /// The waiting sign-in.
    pub pending_id: Id,
}

/// A key's answer.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SecondFactorAnswer {
    /// The waiting sign-in.
    pub pending_id: Id,
    /// `rawId`.
    pub credential_id: ByteBuf,
    /// `response.clientDataJSON`.
    pub client_data_json: ByteBuf,
    /// `response.authenticatorData`.
    pub authenticator_data: ByteBuf,
    /// `response.signature`.
    pub signature: ByteBuf,
}

/// The signing-in device asks whether the key answered.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SecondFactorPoll {
    /// The waiting sign-in.
    pub pending_id: Id,
    /// From [`SecondFactorPending`].
    pub poll_key: ByteBuf,
}

/// Not yet, or the session.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SecondFactorStatus {
    /// The session, once a key answered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<crate::wire::LoginFinishReply>,
}

// --- crown jewels in emergency access ---------------------------------------------

/// The crown jewels an owner opted in to an emergency grant: their Item
/// Keys sealed to the contact (`lockdown_vault::sharing::ShareEnvelope`,
/// purpose `emergency`, naming the item), held back with the grant.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, Default)]
pub struct EmergencyItems {
    /// One per item, as signed.
    pub envelopes: Vec<ByteBuf>,
}

// --- phrase checks between two people ----------------------------------------------

/// One person asks another to check keys with a phrase.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct VerifyOpen {
    /// Whom.
    pub to_account: Id,
}

/// The check, as filed.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct VerifyOpened {
    /// Its id.
    pub verify_id: Id,
    /// When it closes on its own.
    pub expires_at: i64,
}

/// One open check.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct VerifyEntry {
    /// Its id.
    pub verify_id: Id,
    /// Who asked.
    pub from_account: Id,
    /// Who was asked.
    pub to_account: Id,
    /// When it closes.
    pub expires_at: i64,
}

/// The checks this account is part of.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, Default)]
pub struct VerifyList {
    /// Open checks, oldest first.
    pub open: Vec<VerifyEntry>,
}

fn path_id(id: Id) -> String {
    id.to_string()
}

/// Which end of a link is calling.
#[derive(Clone, Copy, Debug)]
pub enum LinkSide<'a> {
    /// The existing device, with its session token.
    Opener(&'a str),
    /// The new device, with the key joining gave it.
    Joiner(&'a [u8]),
}

impl<'a> LinkSide<'a> {
    fn path(self, link: Id, what: &str) -> (String, Option<&'a str>, Option<ByteBuf>) {
        match self {
            Self::Opener(token) => (
                format!("/v1/links/{}/{what}", path_id(link)),
                Some(token),
                None,
            ),
            Self::Joiner(key) => (
                format!("/v1/links/{}/joiner/{what}", path_id(link)),
                None,
                Some(ByteBuf::from(key.to_vec())),
            ),
        }
    }
}

impl HttpTransport {
    /// `POST /v1/links`.
    pub fn link_open(&self, token: &str, open: &LinkOpen) -> Result<LinkOpened> {
        self.call(Method::Post, "/v1/links", Some(token), Some(open))
    }

    /// `POST /v1/links/join`.
    pub fn link_join(&self, code: &str) -> Result<LinkJoined> {
        self.call(
            Method::Post,
            "/v1/links/join",
            None,
            Some(&LinkJoin { code: code.into() }),
        )
    }

    /// Sends one frame on a link: the existing device with its session, the
    /// new device with its key.
    pub fn link_push(&self, link: Id, side: LinkSide<'_>, frame: &[u8]) -> Result<()> {
        if frame.len() > MAX_FRAME {
            return Err(TransportError::Network(
                "that frame is larger than the relay carries".into(),
            ));
        }
        let (path, token, key) = side.path(link, "frames");
        self.call_raw(
            Method::Post,
            &path,
            token,
            Some(
                crate::wire::encode(&FramePush {
                    key,
                    frame: ByteBuf::from(frame.to_vec()),
                })
                .map_err(|e| TransportError::Decode(e.to_string()))?,
            ),
        )
        .map(|_| ())
    }

    /// Reads the other side's frames after the first `after`.
    pub fn link_poll(&self, link: Id, side: LinkSide<'_>, after: u32) -> Result<Frames> {
        let (path, token, key) = side.path(link, "poll");
        self.call(Method::Post, &path, token, Some(&FramePoll { key, after }))
    }

    /// `POST /v1/links/{id}/device`.
    pub fn link_device(&self, token: &str, link: Id, device: &LinkDevice) -> Result<()> {
        self.call_raw(
            Method::Post,
            &format!("/v1/links/{}/device", path_id(link)),
            Some(token),
            Some(crate::wire::encode(device).map_err(|e| TransportError::Decode(e.to_string()))?),
        )
        .map(|_| ())
    }

    /// `POST /v1/links/{id}/close`.
    pub fn link_close(&self, token: &str, link: Id) -> Result<()> {
        self.call_raw(
            Method::Post,
            &format!("/v1/links/{}/close", path_id(link)),
            Some(token),
            None,
        )
        .map(|_| ())
    }

    /// `POST /v1/account/password/start`.
    pub fn password_start(&self, token: &str, request: &[u8]) -> Result<PasswordStartReply> {
        self.call(
            Method::Post,
            "/v1/account/password/start",
            Some(token),
            Some(&PasswordStart {
                request: ByteBuf::from(request.to_vec()),
            }),
        )
    }

    /// `POST /v1/account/password/finish`.
    pub fn password_finish(&self, token: &str, finish: &PasswordFinish) -> Result<()> {
        self.call_raw(
            Method::Post,
            "/v1/account/password/finish",
            Some(token),
            Some(crate::wire::encode(finish).map_err(|e| TransportError::Decode(e.to_string()))?),
        )
        .map(|_| ())
    }

    /// `GET /v1/account/security-keys`.
    pub fn security_keys(&self, token: &str) -> Result<SecurityKeyList> {
        self.call::<(), _>(Method::Get, "/v1/account/security-keys", Some(token), None)
    }

    /// `POST /v1/account/security-keys/start`.
    pub fn security_key_start(&self, token: &str) -> Result<CreationRequest> {
        self.call::<(), _>(
            Method::Post,
            "/v1/account/security-keys/start",
            Some(token),
            None,
        )
    }

    /// `POST /v1/account/security-keys/request`: a registration's request
    /// by its id.
    pub fn security_key_request(&self, pending_id: Id) -> Result<CreationRequest> {
        self.call(
            Method::Post,
            "/v1/account/security-keys/request",
            None,
            Some(&SecondFactorLookup { pending_id }),
        )
    }

    /// `POST /v1/account/security-keys/finish`.
    pub fn security_key_finish(&self, finish: &KeyRegisterFinish) -> Result<SecurityKeyInfo> {
        self.call(
            Method::Post,
            "/v1/account/security-keys/finish",
            None,
            Some(finish),
        )
    }

    /// `POST /v1/account/security-keys/{id}/remove`.
    pub fn security_key_remove(&self, token: &str, id: &str) -> Result<SecurityKeyList> {
        if id.is_empty()
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(TransportError::Network(format!("{id} isn't a key id")));
        }
        self.call::<(), _>(
            Method::Post,
            &format!("/v1/account/security-keys/{id}/remove"),
            Some(token),
            None,
        )
    }

    /// `POST /v1/account/login/second-factor/request`.
    pub fn second_factor_request(&self, pending_id: Id) -> Result<SecondFactorRequest> {
        self.call(
            Method::Post,
            "/v1/account/login/second-factor/request",
            None,
            Some(&SecondFactorLookup { pending_id }),
        )
    }

    /// `POST /v1/account/login/second-factor`.
    pub fn second_factor_answer(&self, answer: &SecondFactorAnswer) -> Result<()> {
        self.call_raw(
            Method::Post,
            "/v1/account/login/second-factor",
            None,
            Some(crate::wire::encode(answer).map_err(|e| TransportError::Decode(e.to_string()))?),
        )
        .map(|_| ())
    }

    /// `POST /v1/account/login/second-factor/poll`.
    pub fn second_factor_poll(
        &self,
        pending_id: Id,
        poll_key: &[u8],
    ) -> Result<SecondFactorStatus> {
        self.call(
            Method::Post,
            "/v1/account/login/second-factor/poll",
            None,
            Some(&SecondFactorPoll {
                pending_id,
                poll_key: ByteBuf::from(poll_key.to_vec()),
            }),
        )
    }

    /// `POST /v1/emergency/{grant}/items`: the owner's whole set.
    pub fn emergency_items_put(
        &self,
        token: &str,
        grant: Id,
        items: &EmergencyItems,
    ) -> Result<()> {
        self.call_raw(
            Method::Post,
            &format!("/v1/emergency/{}/items", path_id(grant)),
            Some(token),
            Some(crate::wire::encode(items).map_err(|e| TransportError::Decode(e.to_string()))?),
        )
        .map(|_| ())
    }

    /// `GET /v1/emergency/{grant}/items`: the contact's, after the release.
    pub fn emergency_items_get(&self, token: &str, grant: Id) -> Result<EmergencyItems> {
        self.call::<(), _>(
            Method::Get,
            &format!("/v1/emergency/{}/items", path_id(grant)),
            Some(token),
            None,
        )
    }

    /// `POST /v1/verifications`.
    pub fn verify_open(&self, token: &str, to_account: Id) -> Result<VerifyOpened> {
        self.call(
            Method::Post,
            "/v1/verifications",
            Some(token),
            Some(&VerifyOpen { to_account }),
        )
    }

    /// `GET /v1/verifications`.
    pub fn verify_list(&self, token: &str) -> Result<VerifyList> {
        self.call::<(), _>(Method::Get, "/v1/verifications", Some(token), None)
    }

    /// `POST /v1/verifications/{id}/frames`.
    pub fn verify_push(&self, token: &str, verify: Id, frame: &[u8]) -> Result<()> {
        self.call_raw(
            Method::Post,
            &format!("/v1/verifications/{}/frames", path_id(verify)),
            Some(token),
            Some(
                crate::wire::encode(&FramePush {
                    key: None,
                    frame: ByteBuf::from(frame.to_vec()),
                })
                .map_err(|e| TransportError::Decode(e.to_string()))?,
            ),
        )
        .map(|_| ())
    }

    /// `POST /v1/verifications/{id}/poll`.
    pub fn verify_poll(&self, token: &str, verify: Id, after: u32) -> Result<Frames> {
        self.call(
            Method::Post,
            &format!("/v1/verifications/{}/poll", path_id(verify)),
            Some(token),
            Some(&FramePoll { key: None, after }),
        )
    }
}

/// The session out of a second-factor wait, in the shape the OPAQUE client
/// hands back.
pub fn session_of(reply: crate::wire::LoginFinishReply) -> lockdown_crypto::auth::Session {
    lockdown_crypto::auth::Session {
        token: reply.token,
        expires_at: reply.expires_at,
        profile_id: reply.profile_id,
        second_factor: reply.second_factor.map(pending_of),
    }
}

/// A wire pending second factor as the crypto crate's.
pub fn pending_of(p: SecondFactorPending) -> lockdown_crypto::auth::SecondFactorNeeded {
    lockdown_crypto::auth::SecondFactorNeeded {
        pending_id: p.pending_id,
        poll_key: p.poll_key.into_vec(),
        request: crate::wire::encode(&p.request).unwrap_or_default(),
    }
}

/// The WebAuthn request back out of [`pending_of`]'s bytes.
pub fn request_of(
    needed: &lockdown_crypto::auth::SecondFactorNeeded,
) -> Option<SecondFactorRequest> {
    crate::wire::decode(&needed.request).ok()
}
