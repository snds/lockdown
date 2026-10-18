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
//! | `GET /v1/identity?since=N` | | [`IdentitySince`] | yes |
//! | `PUT /v1/identity` | [`IdentityPush`] | [`HeadAccepted`] | yes |
//! | `GET /v1/vaults` | | [`VaultList`] | yes |
//! | `PUT /v1/vaults/{vault}` | vault record bytes | empty | yes |
//! | `GET /v1/vaults/{vault}/heads?since=N` | | [`HeadsSince`] | yes |
//! | `PUT /v1/vaults/{vault}/heads` | manifest head bytes | [`HeadAccepted`] | yes |
//! | `GET /v1/vaults/{vault}/items?since=R` | | [`ItemsSince`] | yes |
//! | `GET /v1/vaults/{vault}/items/{item}` | | [`ItemWire`] | yes |
//! | `PUT /v1/vaults/{vault}/items/{item}` | [`ItemPush`] | [`PushReply`] | yes |
//! | `PUT /v1/vaults/{vault}/blobs/{hash}` | the blob bytes (`application/octet-stream`) | [`BlobAccepted`] | yes |
//! | `GET /v1/vaults/{vault}/blobs/{hash}` | | the blob bytes | yes |
//! | `HEAD /v1/vaults/{vault}/blobs/{hash}` | | 200 or 404 | yes |
//! | `GET /v1/account/alerts?since=N` | | [`AlertsSince`] | yes |
//! | `POST /v1/account/alerts/{seq}/dismiss` | | empty | yes |
//! | `GET /v1/account/sessions` | | [`SessionList`] | yes |
//! | `POST /v1/account/sessions/{id}/end` | | empty | yes |
//! | `POST /v1/account/sessions/end-others` | | [`SessionsEnded`] | yes |
//! | `GET /v1/events` | | `text/event-stream` of [`LiveEvent`] (JSON) | yes |
//!
//! The live channel (`/v1/events`) is server-sent events: `event: lockdown`
//! with a JSON [`LiveEvent`] as its data, and a comment line every 20 s to
//! keep the connection open. An event says only *that* something changed,
//! never what: the device then syncs (or re-reads its alerts or sessions)
//! exactly as if it had polled, with every check that implies.
//!
//! Attachment blobs (02-model §6) are content-addressed: the path names
//! the BLAKE3 of the bytes, a `PUT` of a name already held is a no-op, a
//! body that hashes to another name is `blob_mismatch`, one over the
//! server's cap is `blob_too_large`. A blob is pushed **before** the item
//! whose signed attachment list names it; an item push naming a blob the
//! server lacks is refused with `unknown_blob`. A blob nothing names any
//! more (never named, or named only by shredded attachments after a
//! purge) is collected after a grace period.
//!
//! The identity's signed record set travels the same way as vault heads
//! (`lockdown_vault::identity_set`): the manifest bytes as the Identity
//! Signing Key signed them, each record as bytes, and the server accepts a
//! manifest only as the next page of the chain it holds (D71).
//!
//! A refusal is an HTTP error status with an [`ErrorBody`] whose
//! [`ErrorCode`] a client can act on: `gap`, `fork` and `older_seq` for a
//! head or an identity manifest; `stale` for an item; `revoked_device`
//! for anything signed by a device the identity revoked; `token_expired`
//! to log in again.

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
    /// Head, item or identity manifest: signed by a device the identity
    /// manifest the server holds says is revoked (D73).
    RevokedDevice,
    /// Identity manifest: an entry names a record the server does not
    /// hold and the push did not carry.
    UnknownRecord,
    /// Identity manifest: a new revocation's vault boundary isn't the
    /// server's current head for that vault (another device wrote in
    /// between), or a vault of the account has no boundary. List the
    /// vaults again and re-sign.
    BoundaryMoved,
    /// Item: its attachment list names a blob the server doesn't hold.
    /// Push the blob first.
    UnknownBlob,
    /// Blob: larger than this server takes.
    BlobTooLarge,
    /// Blob: the bytes don't hash to the name in the path.
    BlobMismatch,
    /// The server failed.
    Internal,
    /// Head: a shared vault moved to a newer Vault Key generation (a member
    /// left); fetch the new key, move the items to it, and sync again.
    KeyRotated,
    /// Blob: the vault's owner has no room left for attachments on this
    /// server (D234).
    QuotaExceeded,
}

/// `GET /v1/account/storage`: how much of this account's attachment room
/// its vaults use (D234). `quota` is `None` on a server with no limit.
/// Counted from the padded blob sizes the server holds, so it is what the
/// server can see, never a file's real size.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct StorageUsage {
    /// The format version, 1.
    pub v: u16,
    /// Bytes of attachment blobs in the vaults this account owns.
    pub used: u64,
    /// The account's limit in bytes, if the server sets one.
    pub quota: Option<u64>,
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

// --- the identity's record set ----------------------------------------------

/// One record of the identity, as the Identity Signing Key signed it.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct RecordWire {
    /// Its path (`lockdown_vault::identity_set::RecordRef::path`).
    pub path: String,
    /// The signed bytes.
    #[serde(with = "serde_bytes")]
    pub body: Vec<u8>,
}

/// `GET /v1/identity?since=N`.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, Default)]
pub struct IdentitySince {
    /// Every identity manifest with `seq > N`, oldest first, as signed bytes.
    pub manifests: Vec<serde_bytes::ByteBuf>,
    /// Every record the newest manifest lists.
    pub records: Vec<RecordWire>,
    /// The server's current `seq` (zero when it holds none).
    pub current_seq: u64,
    /// The server's current manifest hash.
    pub current_hash: Option<Hash32>,
}

/// `PUT /v1/identity`: the next identity manifest and whichever records
/// it lists that the server may not hold yet.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct IdentityPush {
    /// The manifest, as signed.
    #[serde(with = "serde_bytes")]
    pub manifest: Vec<u8>,
    /// Records to store alongside it.
    pub records: Vec<RecordWire>,
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

/// The server's answer to a blob push.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct BlobAccepted {
    /// The name.
    pub hash: Hash32,
    /// The length in bytes.
    pub size: u64,
    /// Whether the server lacked it before this push.
    pub new: bool,
}

/// `GET /v1/vaults/{vault}/items?since=R`.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ItemsSince {
    /// Every item with `rev > R`, lowest revision first.
    pub items: Vec<ItemWire>,
    /// The newest revision assigned in this vault.
    pub current_rev: u64,
}

// --- account alerts (docs/plan/13-security-center.md) ----------------------

/// What an account alert is about.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertKind {
    /// A device the account had never seen logged in with it.
    NewDevice,
    /// The account's email address appeared in a newly known breach.
    EmailBreach,
}

/// A breach an address appeared in, as the breach feed names it.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct BreachNotice {
    /// The address that was found (the account's login).
    pub address: String,
    /// The breach's name.
    pub name: String,
    /// The breached site's domain, when the feed names one.
    pub domain: Option<String>,
    /// When the breach happened (`YYYY-MM-DD`), when known.
    pub breach_date: Option<String>,
    /// What was taken, in the feed's words ("Email addresses", "Passwords").
    pub data: Vec<String>,
}

/// One alert. `seq` orders an account's alerts and names one to dismiss.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct AccountAlert {
    /// Its number within the account.
    pub seq: u64,
    /// What it is about.
    pub kind: AlertKind,
    /// When the server recorded it (seconds since the Unix epoch).
    pub at: i64,
    /// For a new device: which one.
    pub device_id: Option<Id>,
    /// For an email breach: the breach.
    pub breach: Option<BreachNotice>,
    /// Whether a device of the account dismissed it.
    pub dismissed: bool,
}

/// `GET /v1/account/alerts?since=N`: the account's alerts after `N` that
/// concern the asking device (never its own sign-in, never one from
/// before it joined).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct AlertsSince {
    /// The alerts, oldest first.
    pub alerts: Vec<AccountAlert>,
    /// The newest `seq` the account has.
    pub current_seq: u64,
}

// --- sessions (docs/plan/20-self-host-server.md §6) ---------------------------

/// One signed-in session of the account.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SessionInfo {
    /// The session's id (32 hex digits); names it to end it.
    pub id: String,
    /// The device that logged in.
    pub device_id: Id,
    /// When it started (seconds since the Unix epoch).
    pub created_at: i64,
    /// When it was last used, to the minute.
    pub last_seen: i64,
    /// When it runs out.
    pub expires_at: i64,
    /// The address it was last used from, as the server saw it.
    pub address: Option<String>,
    /// The client's name for itself (`lockdown/0.1.0 (macos)`).
    pub client: Option<String>,
    /// Whether this is the session asking.
    pub current: bool,
}

/// `GET /v1/account/sessions`: the account's live sessions, oldest first.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SessionList {
    /// The sessions.
    pub sessions: Vec<SessionInfo>,
}

/// `POST /v1/account/sessions/end-others`: how many were ended.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SessionsEnded {
    /// The number of sessions ended.
    pub ended: u64,
}

/// What a live event is about. Readers treat a kind they don't know as
/// [`LiveKind::Other`] and ignore it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveKind {
    /// A vault or the identity's records changed on the server: sync.
    Sync,
    /// The account's alerts changed: read them again.
    Alerts,
    /// The account's session list changed.
    Sessions,
    /// This very session was ended; the stream closes after it.
    SessionEnded,
    /// A kind this build doesn't know.
    #[serde(other)]
    Other,
}

/// One event on the live channel. Content-free by design: no vault, item,
/// count or device is named.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct LiveEvent {
    /// The event format version.
    pub v: u16,
    /// What changed.
    pub kind: LiveKind,
}

/// The live event format this crate writes.
pub const LIVE_V: u16 = 1;

/// Encodes a body.
pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, lockdown_crypto::Error> {
    Ok(lockdown_crypto::cbor::to_canonical(value)?)
}

/// Decodes a body.
pub fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, lockdown_crypto::Error> {
    Ok(lockdown_crypto::cbor::from_slice(bytes)?)
}

// --- monitored addresses (docs/plan/23-bridges.md §9) -------------------------

/// `GET` / `PUT /v1/account/monitored`: the addresses beside the account's
/// own login that the server's breach job asks its feed about (the
/// person's email aliases). The client sends the whole list each time; an
/// empty list stops it.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct MonitoredAddresses {
    /// Lowercase email addresses, sorted, each at most 254 bytes.
    pub addresses: Vec<String>,
}

/// The most addresses one account may have monitored.
pub const MAX_MONITORED: usize = 200;

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
