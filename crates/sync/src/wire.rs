//! The sync protocol on the wire: what `lockdown-server` accepts and
//! answers, and what the client sends. One set of types for both sides so
//! they cannot drift (03-architecture §5, §6).
//!
//! Every body is canonical CBOR (`application/cbor`); the discovery
//! document at `/.well-known/lockdown` is the one JSON exception. Item
//! envelopes, manifest heads and vault records travel as the bytes the
//! vault crate already produces (`Item::to_bytes`, `ManifestHead::to_bytes`,
//! `VaultRecord::to_bytes`), so the server stores exactly what a device
//! signed and hands it back byte for byte. Everything binary is
//! ciphertext or public structure: the server never sees a key.
//!
//! | Method and path | Body in | Body out | Needs a session |
//! |---|---|---|---|
//! | `POST /v1/account/register/start` | [`RegisterStart`] | [`RegisterStartReply`] | no |
//! | `POST /v1/account/register/finish` | [`RegisterFinish`] | empty | no |
//! | `POST /v1/account/login/start` | [`LoginStart`] | [`LoginStartReply`] | no |
//! | `POST /v1/account/login/finish` | [`LoginFinish`] | [`LoginFinishReply`] | no |
//! | `PUT /v1/account/identity` | identity backup bytes | empty | yes |
//! | `GET /v1/account/identity` | | identity backup bytes | yes |
//! | `GET /v1/vaults` | | [`VaultList`] | yes |
//! | `PUT /v1/vaults/{vault}` | vault record bytes | empty | yes |
//! | `GET /v1/vaults/{vault}/heads?since=N` | | [`HeadsSince`] | yes |
//! | `PUT /v1/vaults/{vault}/heads` | manifest head bytes | [`HeadAccepted`] | yes |
//! | `GET /v1/vaults/{vault}/items?since=R` | | [`ItemsSince`] | yes |
//! | `GET /v1/vaults/{vault}/items/{item}` | | [`ItemWire`] | yes |
//! | `PUT /v1/vaults/{vault}/items/{item}` | [`ItemPush`] | [`PushReply`] | yes |
//!
//! A refusal is an HTTP error status with an [`ErrorBody`] whose
//! [`ErrorCode`] a client can act on: `gap`, `fork` and `older_seq` for a
//! head; `stale` for an item; `token_expired` to log in again.

use lockdown_vault::{Hash32, Id};
use serde::{Deserialize, Serialize};

/// The API version this crate speaks.
pub const API_V: u16 = 1;

/// The content type of every body but the discovery document.
pub const CONTENT_TYPE: &str = "application/cbor";

/// The scheme in the `Authorization` header.
pub const BEARER: &str = "Bearer ";

/// Why the server refused, in a form the client can act on.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// The body or a path part did not decode.
    BadRequest,
    /// No session token, or one that does not check out.
    Unauthenticated,
    /// The session token has run out; log in again.
    TokenExpired,
    /// A valid session, but not this account's vault.
    Forbidden,
    /// No such vault, item or account.
    NotFound,
    /// Too many attempts; the `Retry-After` header says when.
    RateLimited,
    /// Registration: the login name is already an account.
    LoginTaken,
    /// Registration is closed on this server.
    SignupsClosed,
    /// Login: wrong password, unknown login, or a stale attempt. One code
    /// for all of them.
    LoginFailed,
    /// Head: `seq` is more than one past the server's head.
    Gap,
    /// Head: `seq` is the next one but `prev_hash` is not the server's head.
    Fork,
    /// Head: `seq` is at or below the server's head.
    OlderSeq,
    /// Head or item: signed by a device this account has not logged in with.
    UnknownDevice,
    /// Head or item: the device signature did not check out.
    BadSignature,
    /// Head: an entry names an item, revision or version the server does
    /// not hold. Push the item first.
    UnknownItem,
    /// The body names another vault or item than the path.
    WrongVault,
    /// Item: the revision moved; the reply carries the server's copy.
    Stale,
    /// The server failed.
    Internal,
}

/// The body of every error response.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ErrorBody {
    /// What to do about it.
    pub code: ErrorCode,
    /// For the person, in plain words.
    pub message: String,
}

// --- account ---------------------------------------------------------------

/// Registration, first round trip.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct RegisterStart {
    /// The login name the account is filed under.
    pub login: String,
    /// The OPAQUE registration request.
    #[serde(with = "serde_bytes")]
    pub request: Vec<u8>,
}

/// The server's answer to [`RegisterStart`].
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct RegisterStartReply {
    /// The OPAQUE registration response.
    #[serde(with = "serde_bytes")]
    pub response: Vec<u8>,
}

/// Registration, second round trip. The account exists afterwards.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct RegisterFinish {
    /// The login name.
    pub login: String,
    /// The profile this account is for.
    pub profile_id: Id,
    /// The OPAQUE registration upload (the password file in the making).
    #[serde(with = "serde_bytes")]
    pub upload: Vec<u8>,
}

/// Login, first round trip.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct LoginStart {
    /// The login name.
    pub login: String,
    /// The OPAQUE credential request.
    #[serde(with = "serde_bytes")]
    pub request: Vec<u8>,
}

/// The server's answer to [`LoginStart`].
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct LoginStartReply {
    /// The server's one-time id for this attempt.
    pub login_id: Id,
    /// The OPAQUE credential response (a decoy for an unknown login).
    #[serde(with = "serde_bytes")]
    pub response: Vec<u8>,
}

/// The device's proof at login, as sent.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct DeviceProofWire {
    /// The device.
    pub device_id: Id,
    /// Its Ed25519 public key.
    #[serde(with = "serde_bytes")]
    pub device_pub: [u8; 32],
    /// Ed25519 over `lockdown_crypto::auth::device_pop_message`.
    #[serde(with = "serde_bytes")]
    pub pop: Vec<u8>,
}

/// Login, second round trip.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct LoginFinish {
    /// From [`LoginStartReply`].
    pub login_id: Id,
    /// The OPAQUE credential finalization.
    #[serde(with = "serde_bytes")]
    pub finalization: Vec<u8>,
    /// Which device this is.
    pub device: DeviceProofWire,
}

/// A session.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct LoginFinishReply {
    /// The bearer token.
    pub token: String,
    /// When it stops working (seconds since the Unix epoch).
    pub expires_at: i64,
    /// The profile the account was registered for.
    pub profile_id: Id,
}

// --- vaults ----------------------------------------------------------------

/// One vault of the account, as the server knows it.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct VaultSummary {
    /// The vault.
    pub vault_id: Id,
    /// Its `VaultRecord` bytes (the Vault Key wrapped under the persona's
    /// App Data Key, and the public context).
    #[serde(with = "serde_bytes")]
    pub record: Vec<u8>,
    /// The newest accepted head's `seq`; zero before the first.
    pub head_seq: u64,
    /// The newest accepted head's hash.
    pub head_hash: Option<Hash32>,
    /// The newest item revision assigned in this vault.
    pub item_rev: u64,
}

/// `GET /v1/vaults`.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, Default)]
pub struct VaultList {
    /// Every vault the account owns, in id order.
    pub vaults: Vec<VaultSummary>,
}

/// The server's answer to an accepted head.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct HeadAccepted {
    /// The accepted `seq`.
    pub seq: u64,
    /// Its hash: what the next head must name as `prev_hash`.
    pub hash: Hash32,
}

/// A head refusal's detail, alongside the [`ErrorCode`].
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct HeadRefused {
    /// Why.
    pub code: ErrorCode,
    /// For the person.
    pub message: String,
    /// The server's current `seq` (zero when it holds none).
    pub current_seq: u64,
    /// The server's current head hash.
    pub current_hash: Option<Hash32>,
}

/// `GET /v1/vaults/{vault}/heads?since=N`.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct HeadsSince {
    /// Every head with `seq > N`, oldest first, as signed bytes.
    pub heads: Vec<serde_bytes::ByteBuf>,
    /// The server's current `seq`.
    pub current_seq: u64,
    /// The server's current head hash.
    pub current_hash: Option<Hash32>,
}

/// `PUT /v1/vaults/{vault}/items/{item}`.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ItemPush {
    /// The revision the client last saw; zero for a new item.
    pub if_match: u64,
    /// The item envelope bytes.
    #[serde(with = "serde_bytes")]
    pub item: Vec<u8>,
}

/// One item as the server holds it. The envelope bytes are exactly what
/// the device signed and pushed, so the `rev` *inside* them is whatever
/// that device had at the time; the revision that counts is the one
/// beside them, and a client sets `Item::rev` from it when it adopts the
/// item.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ItemWire {
    /// Its revision, assigned by the server.
    pub rev: u64,
    /// The envelope bytes.
    #[serde(with = "serde_bytes")]
    pub item: Vec<u8>,
}

/// The server's answer to a push.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum PushReply {
    /// Stored at this revision.
    Accepted {
        /// The new revision.
        rev: u64,
    },
    /// The revision moved. Sent with status 409 and code `stale`.
    Stale {
        /// What the server holds now.
        current: ItemWire,
    },
}

/// `GET /v1/vaults/{vault}/items?since=R`.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ItemsSince {
    /// Every item with `rev > R`, lowest revision first.
    pub items: Vec<ItemWire>,
    /// The newest revision assigned in this vault.
    pub current_rev: u64,
}

/// Encodes a body.
pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, lockdown_crypto::Error> {
    Ok(lockdown_crypto::cbor::to_canonical(value)?)
}

/// Decodes a body.
pub fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, lockdown_crypto::Error> {
    Ok(lockdown_crypto::cbor::from_slice(bytes)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bodies_round_trip_and_binary_stays_binary() {
        let push = ItemPush {
            if_match: 3,
            item: vec![1, 2, 3],
        };
        let bytes = encode(&push).unwrap();
        assert_eq!(decode::<ItemPush>(&bytes).unwrap(), push);
        // The item bytes are a CBOR byte string (major type 2), not an array.
        assert!(bytes.windows(4).any(|w| w == [0x43, 1, 2, 3]));

        let reply = PushReply::Stale {
            current: ItemWire {
                rev: 4,
                item: vec![9],
            },
        };
        assert_eq!(
            decode::<PushReply>(&encode(&reply).unwrap()).unwrap(),
            reply
        );
        let err = ErrorBody {
            code: ErrorCode::OlderSeq,
            message: "m".into(),
        };
        assert_eq!(decode::<ErrorBody>(&encode(&err).unwrap()).unwrap(), err);
    }
}
