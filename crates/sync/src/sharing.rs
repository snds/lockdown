//! The sharing half of the protocol (`docs/plan/18-sharing.md`): finding
//! another person's keys, shared-vault membership, items shared on their
//! own, share links, request links and emergency access. One set of wire
//! types for `lockdown-server` and the client, and the client's calls as
//! inherent methods on [`HttpTransport`] (like the account alerts), so the
//! sync engine's test transports need nothing new.
//!
//! | Method and path | Body in | Body out | Session |
//! |---|---|---|---|
//! | `GET /v1/directory?login=L` | | [`DirectoryEntry`] | yes |
//! | `GET /v1/directory/accounts/{account}` | | [`DirectoryEntry`] | yes |
//! | `GET /v1/shared` | | [`SharedVaults`] | yes |
//! | `GET /v1/vaults/{vault}/members?since=N` | | [`MembershipSince`] | yes |
//! | `POST /v1/vaults/{vault}/members` | [`MembershipPush`] | [`MembershipAccepted`] | yes |
//! | `GET /v1/vaults/{vault}/key` | | [`EnvelopeWire`] | yes |
//! | `POST /v1/shares/items` | [`ItemShareCreate`] | empty | yes |
//! | `GET /v1/shares/items` | | [`ReceivedItems`] | yes |
//! | `GET /v1/shares/items/sent` | | [`SentItems`] | yes |
//! | `DELETE /v1/shares/items/{share}` | | empty | yes |
//! | `POST /v1/sends` | [`SendCreate`] | empty | yes |
//! | `GET /v1/sends` | | [`SendList`] | yes |
//! | `DELETE /v1/sends/{send}` | | empty | yes |
//! | `GET /v1/sends/{send}/meta` | | [`SendMeta`] | no |
//! | `POST /v1/sends/{send}/open` | [`SendOpen`] | [`SendOpened`] | no |
//! | `POST /v1/requests` | [`RequestCreate`] | empty | yes |
//! | `GET /v1/requests` | | [`RequestList`] | yes |
//! | `DELETE /v1/requests/{request}` | | empty | yes |
//! | `GET /v1/requests/{request}/key` | | [`RequestKey`] | no |
//! | `POST /v1/requests/{request}/answer` | [`RequestAnswer`] | empty | no |
//! | `POST /v1/emergency` | [`EmergencyCreate`] | empty | yes |
//! | `GET /v1/emergency` | | [`EmergencyList`] | yes |
//! | `POST /v1/emergency/{grant}/step` | [`EmergencyStepPush`] | [`EmergencyWire`] | yes |
//! | `POST /v1/emergency/{grant}/envelopes` | [`EmergencyEnvelopes`] | empty | yes |
//! | `GET /v1/emergency/{grant}/release` | | [`EmergencyRelease`] | yes |
//! | `GET /s/{send}`, `GET /r/{request}` | | a small HTML page | no |

use std::collections::BTreeMap;

use lockdown_crypto::merkle;
use lockdown_vault::contact::{Contact, PublicIdentity};
use lockdown_vault::identity_set::IdentityManifest;
use lockdown_vault::send::SendPassword;
use lockdown_vault::sharing::{Party, Right, VaultKind};
use lockdown_vault::transparency::{LogEntry, LogLeaf, SignedTreeHead};
use lockdown_vault::{Hash32, Id};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

use crate::SyncError;
use crate::engine::session_or;
use crate::transport::{HttpTransport, Method, Result, Transport};
use crate::wire::{ItemWire, RecordWire, VaultSummary};

// --- the directory -----------------------------------------------------------

/// Another account's public identity as the server holds it, with where
/// its key-transparency log recorded the identity manifest.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct DirectoryEntry {
    /// The account.
    pub account_id: Id,
    /// Its login name.
    pub login: String,
    /// The newest identity manifest, as signed.
    pub manifest: ByteBuf,
    /// The profile, persona and device records it lists, as signed.
    pub records: Vec<RecordWire>,
    /// The log entry that recorded that manifest.
    pub log_index: u64,
    /// That entry's bytes exactly as hashed.
    pub leaf: ByteBuf,
}

/// Checks a directory entry against the key-transparency log the caller
/// verified this sync (`sth`, from [`crate::KtSession`]): the leaf is in
/// the tree the server signed, names the account, and records exactly
/// this identity manifest; the manifest and records then verify under the
/// profile's own signing key ([`PublicIdentity::verify`]). With `pinned`,
/// the keys must be the ones pinned before. A server that made up keys for
/// someone would have to log them, where that person's own audit catches
/// it (04-security §6; "refuse to share to a key that doesn't verify
/// against the log").
pub fn verify_directory_entry(
    transport: &dyn Transport,
    sth: &SignedTreeHead,
    entry: &DirectoryEntry,
    pinned: Option<&Contact>,
    persona: Option<Id>,
) -> std::result::Result<Contact, SyncError> {
    let refuse = |msg: String| SyncError::Vault(lockdown_vault::Error::Transparency(msg));
    let manifest = IdentityManifest::from_bytes(&entry.manifest)?;
    let leaf = LogLeaf::from_bytes(&entry.leaf)?;
    if leaf.account_id != entry.account_id {
        return Err(refuse(format!(
            "the log entry the server named for {} belongs to another account",
            entry.login
        )));
    }
    match leaf.entry {
        LogEntry::IdentityHead { seq, hash }
            if seq == manifest.seq && hash == manifest.hash()? => {}
        _ => {
            return Err(refuse(format!(
                "the server's log doesn't record the identity it showed for {}; nothing is shared with it",
                entry.login
            )));
        }
    }
    if entry.log_index >= sth.size {
        return Err(refuse(format!(
            "the log entry for {} is past the tree head this device verified; sync and try again",
            entry.login
        )));
    }
    let proof = transport
        .kt_inclusion(entry.log_index, sth.size)
        .map_err(session_or)?;
    if proof.index != entry.log_index
        || proof.size != sth.size
        || !merkle::verify_inclusion(
            merkle::leaf_hash(&entry.leaf),
            entry.log_index,
            sth.size,
            &proof.path,
            sth.root,
        )
    {
        return Err(refuse(format!(
            "the identity the server showed for {} isn't in the log it signed; nothing is shared with it",
            entry.login
        )));
    }
    let public = PublicIdentity {
        account_id: entry.account_id,
        login: entry.login.clone(),
        manifest,
        records: entry
            .records
            .iter()
            .map(|r| (r.path.clone(), r.body.clone()))
            .collect::<BTreeMap<_, _>>(),
    };
    let verified = public.verify(pinned.map(|p| &p.isk_pub))?;
    let persona = persona.or(pinned.map(|p| p.persona_id));
    let fresh = Contact::from_public(&public, &verified, persona, entry.log_index)?;
    match pinned {
        Some(p) => Ok(p.check_continues(&fresh)?),
        None => Ok(fresh),
    }
}

// --- shared vaults ---------------------------------------------------------------

/// A vault another account shared with this one.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SharedVaultWire {
    /// The vault as the owner's account holds it (the record carries the
    /// public context every field is bound to).
    pub summary: VaultSummary,
    /// A group or a household.
    pub kind: VaultKind,
    /// The owner.
    pub owner: Party,
    /// The owner's login name.
    pub owner_login: String,
    /// What this account may do.
    pub right: Right,
    /// The Vault Key generation in force.
    pub key_ref: Id,
    /// The newest membership event.
    pub membership_seq: u64,
}

/// `GET /v1/shared`.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct SharedVaults {
    /// Vaults shared with this account, by vault id.
    pub vaults: Vec<SharedVaultWire>,
    /// Vaults this account owns and shares, with their newest event.
    pub sharing: Vec<OwnSharedVault>,
}

/// A vault of this account's that is shared.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct OwnSharedVault {
    /// The vault.
    pub vault_id: Id,
    /// A group or a household.
    pub kind: VaultKind,
    /// The Vault Key generation in force.
    pub key_ref: Id,
    /// The newest membership event.
    pub membership_seq: u64,
}

/// `GET /v1/vaults/{vault}/members?since=N`.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct MembershipSince {
    /// Every event after `N`, oldest first, as signed.
    pub events: Vec<ByteBuf>,
    /// The newest event's `seq` (zero for a vault that isn't shared).
    pub current_seq: u64,
    /// The login name of every account the chain names, for display.
    pub logins: BTreeMap<Id, String>,
}

/// `POST /v1/vaults/{vault}/members`: the next membership event and the
/// share envelopes it needs (the new member's, for an addition; every
/// remaining member's at the new generation, for a removal).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct MembershipPush {
    /// The event, as signed.
    pub event: ByteBuf,
    /// Share envelopes, as signed.
    pub envelopes: Vec<ByteBuf>,
}

/// The server's answer to an accepted event.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct MembershipAccepted {
    /// Its `seq`.
    pub seq: u64,
    /// Its hash.
    pub hash: Hash32,
}

/// One share envelope.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct EnvelopeWire {
    /// The envelope, as signed.
    pub envelope: ByteBuf,
}

// --- items shared on their own ---------------------------------------------------

/// `POST /v1/shares/items`.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ItemShareCreate {
    /// The share (reusing one replaces its envelope after a re-key).
    pub share_id: Id,
    /// The vault.
    pub vault_id: Id,
    /// The item.
    pub item_id: Id,
    /// Who.
    pub recipient: Party,
    /// `view` or `fill-only`.
    pub right: Right,
    /// The Item Key wrapped to them, as signed.
    pub envelope: ByteBuf,
}

/// One item shared with this account.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ReceivedItemWire {
    /// The share.
    pub share_id: Id,
    /// Who shared it.
    pub owner: Party,
    /// Their login name.
    pub owner_login: String,
    /// What this account may do.
    pub right: Right,
    /// The Item Key, as signed.
    pub envelope: ByteBuf,
    /// The owner's vault record (for its public context).
    pub record: ByteBuf,
    /// The item as the server holds it now.
    pub item: ItemWire,
}

/// `GET /v1/shares/items`.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct ReceivedItems {
    /// By share id.
    pub items: Vec<ReceivedItemWire>,
}

/// One item this account shared.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SentItemWire {
    /// The share.
    pub share_id: Id,
    /// The vault.
    pub vault_id: Id,
    /// The item.
    pub item_id: Id,
    /// Who.
    pub recipient: Party,
    /// Their login name.
    pub recipient_login: String,
    /// What they may do.
    pub right: Right,
    /// When it was shared.
    pub created_at: i64,
}

/// `GET /v1/shares/items/sent`.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct SentItems {
    /// By share id.
    pub items: Vec<SentItemWire>,
}

// --- share links -------------------------------------------------------------------

/// `POST /v1/sends`.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SendCreate {
    /// The link.
    pub send_id: Id,
    /// The sealed payload.
    pub ciphertext: ByteBuf,
    /// When it stops opening (seconds since the epoch).
    pub expires_at: i64,
    /// How many times it may be opened; `None` for no limit before expiry.
    pub max_views: Option<u32>,
    /// The password half, for a locked link.
    pub password: Option<SendPassword>,
}

/// One link this account made.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SendInfo {
    /// The link.
    pub send_id: Id,
    /// When it was made.
    pub created_at: i64,
    /// When it stops opening.
    pub expires_at: i64,
    /// The view limit.
    pub max_views: Option<u32>,
    /// How many times it was opened.
    pub views: u32,
    /// Whether it needs a password.
    pub password: bool,
}

/// `GET /v1/sends`.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct SendList {
    /// Newest first.
    pub sends: Vec<SendInfo>,
}

/// What a viewer learns before opening: whether to ask for a password
/// (and its salt and cost; never the verifier), and how long it lasts.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SendMeta {
    /// The salt and cost, for a locked link.
    pub password: Option<SendPasswordMeta>,
    /// When it stops opening.
    pub expires_at: i64,
    /// Opens left, when limited.
    pub views_left: Option<u32>,
}

/// The public half of a link's password settings.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SendPasswordMeta {
    /// The Argon2id salt.
    #[serde(with = "serde_bytes")]
    pub salt: [u8; 16],
    /// The Argon2id cost.
    pub params: lockdown_crypto::Argon2Params,
}

/// `POST /v1/sends/{send}/open`.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct SendOpen {
    /// The access proof, for a locked link.
    pub access: Option<ByteBuf>,
}

/// The sealed payload, handed out once per view.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SendOpened {
    /// The ciphertext.
    pub ciphertext: ByteBuf,
}

// --- request links -------------------------------------------------------------------

/// `POST /v1/requests`.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct RequestCreate {
    /// The request.
    pub request_id: Id,
    /// The asking persona's receiving key.
    pub kem_pub: ecosystem_identity::kem::KemPublicKey,
    /// The asking profile.
    pub profile_id: Id,
    /// The asking persona.
    pub persona_id: Id,
    /// When it stops taking an answer.
    pub expires_at: i64,
}

/// One request this account made.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct RequestInfo {
    /// The request.
    pub request_id: Id,
    /// When it was made.
    pub created_at: i64,
    /// When it stops taking an answer.
    pub expires_at: i64,
    /// When it was answered.
    pub answered_at: Option<i64>,
    /// The sealed answer.
    pub answer: Option<ByteBuf>,
}

/// `GET /v1/requests`.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct RequestList {
    /// Newest first.
    pub requests: Vec<RequestInfo>,
}

/// What someone answering a request needs: the key to seal to.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct RequestKey {
    /// The asking persona's receiving key (checked against the link).
    pub kem_pub: ecosystem_identity::kem::KemPublicKey,
    /// The asking profile.
    pub profile_id: Id,
    /// The asking persona.
    pub persona_id: Id,
    /// When it stops taking an answer.
    pub expires_at: i64,
    /// Whether it still takes one.
    pub open: bool,
}

/// `POST /v1/requests/{request}/answer`.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct RequestAnswer {
    /// The sealed answer.
    pub answer: ByteBuf,
}

// --- emergency access -------------------------------------------------------------------

/// `POST /v1/emergency`: a signed grant and the Vault Keys it covers,
/// wrapped to the contact with purpose `emergency`.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct EmergencyCreate {
    /// The grant, as signed.
    pub grant: ByteBuf,
    /// One envelope per vault, as signed.
    pub envelopes: Vec<ByteBuf>,
}

/// `POST /v1/emergency/{grant}/envelopes`: fresh envelopes after a
/// covered vault moved to a new key generation.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct EmergencyEnvelopes {
    /// One per vault, as signed.
    pub envelopes: Vec<ByteBuf>,
}

/// `POST /v1/emergency/{grant}/step`.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct EmergencyStepPush {
    /// The step, as signed.
    pub step: ByteBuf,
}

/// Where a grant stands.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EmergencyState {
    /// Named; the contact hasn't answered.
    Invited,
    /// The contact agreed.
    Accepted,
    /// The contact asked; the waiting period runs.
    Requested,
    /// The waiting period ran out; the keys may be released.
    Ready,
    /// The keys were handed to the contact.
    Released,
    /// The contact said no or stepped down.
    Declined,
    /// The owner ended it.
    Revoked,
}

/// One recorded step, with the server's own time.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct EmergencyEventWire {
    /// Its number within the grant.
    pub seq: u64,
    /// When the server recorded it.
    pub at: i64,
    /// What happened: a step's word, `created`, `released` or `envelopes`.
    pub kind: String,
    /// Who.
    pub by_account: Id,
    /// The signed step, when a person took it.
    pub step: Option<ByteBuf>,
}

/// One grant, as either side sees it.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct EmergencyWire {
    /// The grant, as signed.
    pub grant: ByteBuf,
    /// The owner's login.
    pub owner_login: String,
    /// The contact's login.
    pub contact_login: String,
    /// Where it stands now.
    pub state: EmergencyState,
    /// When the waiting request was made.
    pub requested_at: Option<i64>,
    /// When the keys may be released.
    pub release_at: Option<i64>,
    /// Every step, oldest first.
    pub events: Vec<EmergencyEventWire>,
}

/// `GET /v1/emergency`.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct EmergencyList {
    /// Grants this account made.
    pub as_owner: Vec<EmergencyWire>,
    /// Grants naming this account as the contact.
    pub as_contact: Vec<EmergencyWire>,
}

/// One vault released to a contact.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ReleasedVault {
    /// The vault as the owner's account holds it.
    pub summary: VaultSummary,
    /// The Vault Key wrapped to the contact, as signed.
    pub envelope: ByteBuf,
}

/// `GET /v1/emergency/{grant}/release`.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct EmergencyRelease {
    /// The released vaults.
    pub vaults: Vec<ReleasedVault>,
}

// --- the client's calls -----------------------------------------------------------------

fn path_escape(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

impl HttpTransport {
    fn get_cbor<R: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        token: Option<&str>,
    ) -> Result<R> {
        self.call::<(), R>(Method::Get, path, token, None)
    }

    fn post_cbor<T: Serialize, R: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        token: Option<&str>,
        body: &T,
    ) -> Result<R> {
        self.call(Method::Post, path, token, Some(body))
    }

    fn post_empty<T: Serialize>(&self, path: &str, token: Option<&str>, body: &T) -> Result<()> {
        let bytes = crate::wire::encode(body)
            .map_err(|e| crate::transport::TransportError::Decode(e.to_string()))?;
        self.call_raw(Method::Post, path, token, Some(bytes))
            .map(|_| ())
    }

    fn delete(&self, path: &str, token: Option<&str>) -> Result<()> {
        self.call_raw(Method::Delete, path, token, None).map(|_| ())
    }

    /// `GET /v1/directory?login=L`.
    pub fn directory(&self, token: &str, login: &str) -> Result<DirectoryEntry> {
        self.get_cbor(
            &format!("/v1/directory?login={}", path_escape(login)),
            Some(token),
        )
    }

    /// `GET /v1/directory/accounts/{account}`.
    pub fn directory_account(&self, token: &str, account: Id) -> Result<DirectoryEntry> {
        self.get_cbor(&format!("/v1/directory/accounts/{account}"), Some(token))
    }

    /// `GET /v1/shared`.
    pub fn shared_vaults(&self, token: &str) -> Result<SharedVaults> {
        self.get_cbor("/v1/shared", Some(token))
    }

    /// `GET /v1/vaults/{vault}/members?since=N`.
    pub fn members_since(&self, token: &str, vault: Id, since: u64) -> Result<MembershipSince> {
        self.get_cbor(
            &format!("/v1/vaults/{vault}/members?since={since}"),
            Some(token),
        )
    }

    /// `POST /v1/vaults/{vault}/members`.
    pub fn push_membership(
        &self,
        token: &str,
        vault: Id,
        push: &MembershipPush,
    ) -> Result<MembershipAccepted> {
        self.post_cbor(&format!("/v1/vaults/{vault}/members"), Some(token), push)
    }

    /// `GET /v1/vaults/{vault}/key`: this account's envelope for the
    /// generation in force.
    pub fn vault_envelope(&self, token: &str, vault: Id) -> Result<EnvelopeWire> {
        self.get_cbor(&format!("/v1/vaults/{vault}/key"), Some(token))
    }

    /// `POST /v1/shares/items`.
    pub fn share_item(&self, token: &str, create: &ItemShareCreate) -> Result<()> {
        self.post_empty("/v1/shares/items", Some(token), create)
    }

    /// `GET /v1/shares/items`.
    pub fn received_items(&self, token: &str) -> Result<ReceivedItems> {
        self.get_cbor("/v1/shares/items", Some(token))
    }

    /// `GET /v1/shares/items/sent`.
    pub fn sent_items(&self, token: &str) -> Result<SentItems> {
        self.get_cbor("/v1/shares/items/sent", Some(token))
    }

    /// `DELETE /v1/shares/items/{share}`.
    pub fn unshare_item(&self, token: &str, share: Id) -> Result<()> {
        self.delete(&format!("/v1/shares/items/{share}"), Some(token))
    }

    /// `POST /v1/sends`.
    pub fn create_send(&self, token: &str, create: &SendCreate) -> Result<()> {
        self.post_empty("/v1/sends", Some(token), create)
    }

    /// `GET /v1/sends`.
    pub fn sends(&self, token: &str) -> Result<SendList> {
        self.get_cbor("/v1/sends", Some(token))
    }

    /// `DELETE /v1/sends/{send}`.
    pub fn delete_send(&self, token: &str, send: Id) -> Result<()> {
        self.delete(&format!("/v1/sends/{send}"), Some(token))
    }

    /// `GET /v1/sends/{send}/meta`, no session.
    pub fn send_meta(&self, send: Id) -> Result<SendMeta> {
        self.get_cbor(&format!("/v1/sends/{send}/meta"), None)
    }

    /// `POST /v1/sends/{send}/open`, no session; counts a view.
    pub fn open_send(&self, send: Id, open: &SendOpen) -> Result<SendOpened> {
        self.post_cbor(&format!("/v1/sends/{send}/open"), None, open)
    }

    /// `POST /v1/requests`.
    pub fn create_request(&self, token: &str, create: &RequestCreate) -> Result<()> {
        self.post_empty("/v1/requests", Some(token), create)
    }

    /// `GET /v1/requests`.
    pub fn requests(&self, token: &str) -> Result<RequestList> {
        self.get_cbor("/v1/requests", Some(token))
    }

    /// `DELETE /v1/requests/{request}`.
    pub fn delete_request(&self, token: &str, request: Id) -> Result<()> {
        self.delete(&format!("/v1/requests/{request}"), Some(token))
    }

    /// `GET /v1/requests/{request}/key`, no session.
    pub fn request_key(&self, request: Id) -> Result<RequestKey> {
        self.get_cbor(&format!("/v1/requests/{request}/key"), None)
    }

    /// `POST /v1/requests/{request}/answer`, no session.
    pub fn answer_request(&self, request: Id, answer: &RequestAnswer) -> Result<()> {
        self.post_empty(&format!("/v1/requests/{request}/answer"), None, answer)
    }

    /// `POST /v1/emergency`.
    pub fn create_emergency(&self, token: &str, create: &EmergencyCreate) -> Result<()> {
        self.post_empty("/v1/emergency", Some(token), create)
    }

    /// `GET /v1/emergency`.
    pub fn emergency(&self, token: &str) -> Result<EmergencyList> {
        self.get_cbor("/v1/emergency", Some(token))
    }

    /// `POST /v1/emergency/{grant}/step`.
    pub fn emergency_step(
        &self,
        token: &str,
        grant: Id,
        step: &EmergencyStepPush,
    ) -> Result<EmergencyWire> {
        self.post_cbor(&format!("/v1/emergency/{grant}/step"), Some(token), step)
    }

    /// `POST /v1/emergency/{grant}/envelopes`.
    pub fn emergency_envelopes(
        &self,
        token: &str,
        grant: Id,
        envelopes: &EmergencyEnvelopes,
    ) -> Result<()> {
        self.post_empty(
            &format!("/v1/emergency/{grant}/envelopes"),
            Some(token),
            envelopes,
        )
    }

    /// `GET /v1/emergency/{grant}/release`.
    pub fn emergency_release(&self, token: &str, grant: Id) -> Result<EmergencyRelease> {
        self.get_cbor(&format!("/v1/emergency/{grant}/release"), Some(token))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::{decode, encode};

    #[test]
    fn the_sharing_bodies_round_trip() {
        let party = Party {
            account_id: Id::new(),
            profile_id: Id::new(),
            persona_id: Id::new(),
        };
        let push = ItemShareCreate {
            share_id: Id::new(),
            vault_id: Id::new(),
            item_id: Id::new(),
            recipient: party,
            right: Right::FillOnly,
            envelope: ByteBuf::from(vec![1, 2, 3]),
        };
        assert_eq!(
            decode::<ItemShareCreate>(&encode(&push).unwrap()).unwrap(),
            push
        );
        let meta = SendMeta {
            password: Some(SendPasswordMeta {
                salt: [3; 16],
                params: lockdown_vault::send::send_password_floor(),
            }),
            expires_at: 9,
            views_left: Some(2),
        };
        assert_eq!(decode::<SendMeta>(&encode(&meta).unwrap()).unwrap(), meta);
        let state = EmergencyState::Requested;
        assert_eq!(
            decode::<EmergencyState>(&encode(&state).unwrap()).unwrap(),
            state
        );
        assert_eq!(path_escape("a b@c.d"), "a%20b%40c.d");
    }

    use crate::kt::{AccountEntries, ConsistencyProofWire, InclusionProofWire, SthWire};
    use crate::transport::TransportError;
    use crate::wire;
    use lockdown_crypto::SigningKey;
    use lockdown_vault::identity_set::{PROFILE_PATH, RecordSet};
    use lockdown_vault::transparency::TreeHead;

    /// A log of leaves and nothing else: inclusion proofs from the tree.
    struct Log(Vec<Vec<u8>>);

    fn nope<T>() -> Result<T> {
        Err(TransportError::Network("not in this test".into()))
    }

    impl Transport for Log {
        fn identity_since(&self, _: &str, _: u64) -> Result<wire::IdentitySince> {
            nope()
        }
        fn put_identity(
            &self,
            _: &str,
            _: &wire::IdentityPush,
        ) -> Result<std::result::Result<wire::HeadAccepted, wire::HeadRefused>> {
            nope()
        }
        fn list_vaults(&self, _: &str) -> Result<wire::VaultList> {
            nope()
        }
        fn put_vault(&self, _: &str, _: Id, _: &[u8]) -> Result<()> {
            nope()
        }
        fn heads_since(&self, _: &str, _: Id, _: u64) -> Result<wire::HeadsSince> {
            nope()
        }
        fn put_head(
            &self,
            _: &str,
            _: Id,
            _: &[u8],
        ) -> Result<std::result::Result<wire::HeadAccepted, wire::HeadRefused>> {
            nope()
        }
        fn items_since(&self, _: &str, _: Id, _: u64) -> Result<wire::ItemsSince> {
            nope()
        }
        fn push_item(&self, _: &str, _: Id, _: Id, _: &wire::ItemPush) -> Result<wire::PushReply> {
            nope()
        }
        fn put_blob(&self, _: &str, _: Id, _: Hash32, _: &[u8]) -> Result<wire::BlobAccepted> {
            nope()
        }
        fn get_blob(&self, _: &str, _: Id, _: Hash32) -> Result<Option<Vec<u8>>> {
            nope()
        }
        fn has_blob(&self, _: &str, _: Id, _: Hash32) -> Result<bool> {
            nope()
        }
        fn kt_sth(&self) -> Result<SthWire> {
            nope()
        }
        fn kt_inclusion(&self, index: u64, size: u64) -> Result<InclusionProofWire> {
            let hashes: Vec<Hash32> = self.0.iter().map(|b| merkle::leaf_hash(b)).collect();
            Ok(InclusionProofWire {
                index,
                size,
                path: merkle::inclusion_proof(&hashes[..size as usize], index).unwrap(),
            })
        }
        fn kt_consistency(&self, _: u64, _: u64) -> Result<ConsistencyProofWire> {
            nope()
        }
        fn kt_entries(&self, _: &str, _: u64) -> Result<AccountEntries> {
            nope()
        }
    }

    #[test]
    fn a_directory_entry_counts_only_when_the_log_records_its_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let keystore =
            ecosystem_identity::FileKeystore::open_or_create(&dir.path().join("k.cbor")).unwrap();
        let identity = ecosystem_identity::Identity::create(
            &dir.path().join("bob"),
            "lockdown",
            "bob",
            &keystore,
        )
        .unwrap();
        let mut set = RecordSet::from_records(
            identity.profile(),
            identity.personas().cloned(),
            identity.devices().cloned(),
            identity.envelopes().cloned(),
            identity.consents().cloned(),
            None,
            Vec::new(),
            identity.device_id(),
        )
        .unwrap();
        let isk = lockdown_vault::identity::identity_signing_key(&identity, &keystore).unwrap();
        set.sign(&isk).unwrap();
        let account = Id::new();
        let other = Id::new();
        let leaf = |account: Id, hash: Hash32| {
            LogLeaf::new(account, LogEntry::IdentityHead { seq: 1, hash })
                .to_bytes()
                .unwrap()
        };
        let good = leaf(account, set.manifest.hash().unwrap());
        let log = Log(vec![
            leaf(other, Hash32::ZERO),
            good.clone(),
            leaf(other, Hash32::ZERO),
        ]);
        let hashes: Vec<Hash32> = log.0.iter().map(|b| merkle::leaf_hash(b)).collect();
        let sth = SignedTreeHead::sign(
            TreeHead {
                size: 3,
                root: merkle::root(&hashes),
                time: 1,
            },
            &SigningKey::generate().unwrap(),
        )
        .unwrap();
        let records: Vec<RecordWire> = set
            .records
            .iter()
            .filter(|(p, _)| {
                p.as_str() == PROFILE_PATH
                    || p.starts_with("personas/")
                    || p.starts_with("devices/")
            })
            .map(|(p, b)| RecordWire {
                path: p.clone(),
                body: b.clone(),
            })
            .collect();
        let entry = DirectoryEntry {
            account_id: account,
            login: "bob@example.com".into(),
            manifest: ByteBuf::from(set.manifest.to_bytes().unwrap()),
            records,
            log_index: 1,
            leaf: ByteBuf::from(good),
        };
        let contact = verify_directory_entry(&log, &sth, &entry, None, None).unwrap();
        assert_eq!(contact.isk_pub, identity.profile().isk_pub);
        assert_eq!(contact.log_index, 1);
        // Pinned, the same answer continues.
        verify_directory_entry(&log, &sth, &entry, Some(&contact), None).unwrap();

        // The leaf of another account, or one naming another manifest.
        let mut wrong = entry.clone();
        wrong.log_index = 0;
        wrong.leaf = ByteBuf::from(log.0[0].clone());
        assert!(verify_directory_entry(&log, &sth, &wrong, None, None).is_err());
        let mut forged = entry.clone();
        forged.leaf = ByteBuf::from(leaf(account, Hash32::ZERO));
        assert!(verify_directory_entry(&log, &sth, &forged, None, None).is_err());
        // The right leaf at the wrong index: not in the signed tree there.
        let mut moved = entry.clone();
        moved.log_index = 2;
        assert!(verify_directory_entry(&log, &sth, &moved, None, None).is_err());
        // Past the verified tree head.
        let mut late = entry.clone();
        late.log_index = 3;
        assert!(verify_directory_entry(&log, &sth, &late, None, None).is_err());
        // A device hidden from the answer.
        let mut hidden = entry.clone();
        hidden.records.retain(|r| !r.path.starts_with("devices/"));
        assert!(verify_directory_entry(&log, &sth, &hidden, None, None).is_err());
    }
}
