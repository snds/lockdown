//! The sharing handlers (`docs/plan/18-sharing.md` §4); the wire types are
//! `lockdown_sync::sharing`, the tables `crate::sharing`.
//!
//! Rules enforced here, in plain words:
//! - A shared vault is read by its owner, its current members and, after
//!   an emergency release, the contact; it is written (items, heads,
//!   blobs) by its owner and members with `edit` or `manage` only.
//! - A membership change is accepted when it continues the vault's chain,
//!   is signed by a device of the account making it, that account may
//!   manage the vault, and it brings the envelopes it needs: the new
//!   member's for an addition, every remaining member's at the new key
//!   generation for a removal. Afterwards the vault's heads must name that
//!   generation and its items be wrapped under it (`key_rotated`).
//! - Another person's public identity is handed out only to a signed-in
//!   account, rate-limited, and never with a key envelope.
//! - A share link opens only before it expires, within its view limit,
//!   and with the password's proof when it has one; a request link takes
//!   one answer. Neither needs a session; both are rate-limited.
//! - An emergency grant moves invited → accepted → requested → (after the
//!   waiting period) released, each step signed by the person taking it;
//!   the owner can stop a waiting request or revoke the grant at any time,
//!   and every step is recorded with the server's time.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use lockdown_crypto::VerifyingKey;
use lockdown_sync::sharing::{
    DirectoryEntry, EmergencyCreate, EmergencyEnvelopes, EmergencyEventWire, EmergencyList,
    EmergencyRelease, EmergencyState, EmergencyStepPush, EmergencyWire, EnvelopeWire,
    ItemShareCreate, MembershipAccepted, MembershipPush, MembershipSince, OwnSharedVault,
    ReceivedItemWire, ReceivedItems, ReleasedVault, RequestAnswer, RequestCreate, RequestInfo,
    RequestKey, RequestList, SendCreate, SendInfo, SendList, SendMeta, SendOpen, SendOpened,
    SendPasswordMeta, SentItemWire, SentItems, SharedVaultWire, SharedVaults,
};
use lockdown_sync::wire::{ErrorCode, ItemWire, RecordWire, VaultSummary};
use lockdown_vault::identity_set::IdentityManifest;
use lockdown_vault::send::send_password_floor;
use lockdown_vault::sharing::{
    Change, EmergencyAction, EmergencyGrant, EmergencyStep, MembershipEvent, Party, Right, Roster,
    ShareEnvelope, SharePurpose,
};
use lockdown_vault::transparency::{LogEntry, LogLeaf};
use lockdown_vault::{Id, Item, ItemClass, VaultRecord};
use serde::Deserialize;
use serde_bytes::ByteBuf;

use crate::AppState;
use crate::api::{
    ApiError, ApiResult, Cbor, Peer, Session, Since, brake, check_not_revoked, device_key,
};
use crate::sharing::{
    GrantRow, ItemShareRow, MemberRow, MembershipWrite, RequestRow, SendRow, SharedRow,
    SharingStore,
};
use crate::storage::VaultRow;

/// The longest a share link or a request may stay open: thirty days.
pub const MAX_OPEN_SECS: i64 = 30 * 86_400;
/// The most views a share link may allow.
pub const MAX_VIEWS: u32 = 1_000;
/// The most open share links and requests one account keeps.
pub const MAX_OPEN_PER_ACCOUNT: usize = 200;
/// The largest answer to a request the server takes.
pub const MAX_ANSWER_BYTES: usize = 128 << 10;

fn store(state: &AppState) -> ApiResult<&dyn SharingStore> {
    state
        .storage
        .sharing()
        .ok_or_else(|| ApiError::not_found("sharing on this server"))
}

fn decode_body<T: serde::de::DeserializeOwned>(bytes: &[u8], what: &str) -> ApiResult<T> {
    lockdown_crypto::cbor::from_slice(bytes)
        .map_err(|_| ApiError::bad_request(format!("that isn't {what}")))
}

fn login_of(state: &AppState, account: Id) -> String {
    state
        .storage
        .account(account)
        .ok()
        .flatten()
        .map(|a| a.login)
        .unwrap_or_else(|| account.to_string())
}

fn summary_of(row: &VaultRow) -> VaultSummary {
    VaultSummary {
        vault_id: row.vault_id,
        record: row.record.clone(),
        head_seq: row.head_seq,
        head_hash: row.head_hash,
        item_rev: row.item_rev,
    }
}

fn owner_party(row: &VaultRow) -> ApiResult<Party> {
    let record = VaultRecord::from_bytes(&row.record)
        .map_err(|_| ApiError::internal("a stored vault record didn't decode"))?;
    Ok(Party {
        account_id: row.account_id,
        profile_id: record.context.profile_id,
        persona_id: record.context.persona_id,
    })
}

// --- who may read and write a vault -------------------------------------------------

/// How the session reaches a vault.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Access {
    /// It owns it.
    Owner,
    /// A current member, with this right.
    Member(Right),
    /// An emergency contact after release: read only.
    Emergency,
}

fn released_to(state: &AppState, account: Id, vault_id: Id) -> ApiResult<bool> {
    let Some(store) = state.storage.sharing() else {
        return Ok(false);
    };
    for grant in store.grants_for_contact(account)? {
        if grant.state != "released" {
            continue;
        }
        let g: EmergencyGrant = decode_body(&grant.body, "an emergency grant")?;
        if g.vaults.contains(&vault_id) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// How the session's account reaches `vault_id`, or `forbidden`.
pub fn vault_access(
    state: &AppState,
    session: &Session,
    vault_id: Id,
) -> ApiResult<(VaultRow, Access)> {
    let row = state
        .storage
        .vault(vault_id)?
        .ok_or_else(|| ApiError::not_found(format!("vault {vault_id} isn't on this server")))?;
    if row.account_id == session.account_id {
        return Ok((row, Access::Owner));
    }
    if let Some(store) = state.storage.sharing()
        && let Some(right) = store
            .shared_with(session.account_id)?
            .into_iter()
            .find(|(v, _)| *v == vault_id)
            .map(|(_, r)| r)
    {
        return Ok((row, Access::Member(right)));
    }
    if released_to(state, session.account_id, vault_id)? {
        return Ok((row, Access::Emergency));
    }
    Err(ApiError::forbidden("that vault belongs to another account"))
}

/// The vault, when the session may read it.
pub fn readable(state: &AppState, session: &Session, vault_id: Id) -> ApiResult<VaultRow> {
    vault_access(state, session, vault_id).map(|(row, _)| row)
}

/// The vault, when the session may write to it: its owner, or a member
/// with `edit` or `manage`.
pub fn writable(state: &AppState, session: &Session, vault_id: Id) -> ApiResult<VaultRow> {
    match vault_access(state, session, vault_id)? {
        (row, Access::Owner) => Ok(row),
        (row, Access::Member(r)) if r.can_edit() => Ok(row),
        (_, Access::Member(r)) => Err(ApiError::forbidden(format!(
            "this account can only {} in that shared vault; it can't change anything",
            if r == Right::FillOnly { "fill" } else { "view" }
        ))),
        (_, Access::Emergency) => Err(ApiError::forbidden(
            "emergency access reads a vault; it can't change it",
        )),
    }
}

/// The key of the device that saved an item version in `vault`: a device
/// of the owner, or of an account that is or was a member that could
/// write. A device its own identity revoked can't sign anything new.
pub fn writer_key(state: &AppState, vault: &VaultRow, device_id: Id) -> ApiResult<VerifyingKey> {
    let mut accounts = vec![vault.account_id];
    if let Some(store) = state.storage.sharing() {
        for m in store.members(vault.vault_id)? {
            accounts.push(m.party.account_id);
        }
    }
    for account in accounts {
        if let Some(key) = state.storage.device_pub(account, device_id)? {
            let as_session = Session {
                account_id: account,
                device_id,
                session_id: [0; 16],
                expires_at: 0,
            };
            check_not_revoked(state, &as_session, device_id, vault.vault_id)?;
            return VerifyingKey::from_bytes(&key)
                .map_err(|_| ApiError::internal("a stored device key is unusable"));
        }
    }
    Err(ApiError::new(
        StatusCode::FORBIDDEN,
        ErrorCode::UnknownDevice,
        format!("device {device_id} isn't one of this vault's people; it can't sign for it"),
    ))
}

fn key_rotated(message: impl Into<String>) -> ApiError {
    ApiError::new(StatusCode::CONFLICT, ErrorCode::KeyRotated, message)
}

/// For a shared vault: a head must name the key generation in force.
pub fn check_head_generation(state: &AppState, vault_id: Id, key_ref: Id) -> ApiResult<()> {
    if let Some(store) = state.storage.sharing()
        && let Some(shared) = store.shared_vault(vault_id)?
        && shared.key_ref != key_ref
    {
        return Err(key_rotated(
            "this shared vault moved to a newer key; fetch it, move the items to it and sync again",
        ));
    }
    Ok(())
}

/// For a shared vault: an item must be wrapped under the key generation
/// in force, and never be a crown jewel.
pub fn check_item_generation(state: &AppState, vault_id: Id, item: &Item) -> ApiResult<()> {
    let Some(store) = state.storage.sharing() else {
        return Ok(());
    };
    let Some(shared) = store.shared_vault(vault_id)? else {
        return Ok(());
    };
    if item.class == ItemClass::CrownJewel {
        return Err(ApiError::forbidden(
            "a crown jewel can't be in a shared vault",
        ));
    }
    if let Some(key) = &item.item_key
        && key.wrapped_by.id != shared.key_ref
    {
        return Err(key_rotated(
            "this item is still under an older key of the shared vault; move it to the new key and push again",
        ));
    }
    Ok(())
}

// --- the directory ------------------------------------------------------------------

fn directory_entry(state: &AppState, account_id: Id) -> ApiResult<DirectoryEntry> {
    let account = state
        .storage
        .account(account_id)?
        .ok_or_else(|| ApiError::not_found("that person on this server"))?;
    let identity = state.storage.identity_state(account_id)?;
    if identity.seq == 0 {
        return Err(ApiError::not_found(
            "that person hasn't published their identity on this server yet",
        ));
    }
    let head = state
        .storage
        .identity_heads_since(account_id, identity.seq - 1)?
        .into_iter()
        .find(|h| h.seq == identity.seq)
        .ok_or_else(|| ApiError::internal("the newest identity manifest is missing"))?;
    let manifest = IdentityManifest::from_bytes(&head.body)
        .map_err(|_| ApiError::internal("a stored identity manifest didn't decode"))?;
    let refs: Vec<_> = manifest
        .entries
        .iter()
        .filter(|e| {
            e.path == lockdown_vault::identity_set::PROFILE_PATH
                || e.path.starts_with("personas/")
                || e.path.starts_with("devices/")
        })
        .cloned()
        .collect();
    let records = state
        .storage
        .identity_records(account_id, &refs)?
        .into_iter()
        .map(|r| RecordWire {
            path: r.path,
            body: r.body,
        })
        .collect();
    let mut found = None;
    for row in state.storage.log_entries(account_id, 0)? {
        let leaf = LogLeaf::from_bytes(&row.body)
            .map_err(|_| ApiError::internal("a stored log entry didn't decode"))?;
        if let LogEntry::IdentityHead { seq, .. } = leaf.entry
            && seq == identity.seq
        {
            found = Some(row);
        }
    }
    let row =
        found.ok_or_else(|| ApiError::internal("the log doesn't hold the identity manifest"))?;
    Ok(DirectoryEntry {
        account_id,
        login: account.login,
        manifest: ByteBuf::from(head.body),
        records,
        log_index: row.index,
        leaf: ByteBuf::from(row.body),
    })
}

/// `?login=L`.
#[derive(Deserialize)]
pub struct LoginQuery {
    /// The login name.
    pub login: String,
}

/// `GET /v1/directory?login=L`.
pub async fn directory(
    State(state): State<Arc<AppState>>,
    session: Session,
    peer: Peer,
    Query(q): Query<LoginQuery>,
) -> ApiResult<Cbor<DirectoryEntry>> {
    brake(
        &state,
        &peer,
        Some(&format!("directory:{}", session.account_id)),
    )?;
    let account = state
        .storage
        .account_by_login(q.login.trim())?
        .ok_or_else(|| ApiError::not_found("nobody with that login name on this server"))?;
    Ok(Cbor(directory_entry(&state, account.account_id)?))
}

/// `GET /v1/directory/accounts/{account}`.
pub async fn directory_account(
    State(state): State<Arc<AppState>>,
    _session: Session,
    Path(account): Path<Id>,
) -> ApiResult<Cbor<DirectoryEntry>> {
    Ok(Cbor(directory_entry(&state, account)?))
}

// --- membership -------------------------------------------------------------------------

fn roster_of(store: &dyn SharingStore, vault_id: Id) -> ApiResult<Option<Roster>> {
    let events = store
        .membership_events(vault_id, 0)?
        .iter()
        .map(|b| MembershipEvent::from_bytes(b))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| ApiError::internal("a stored membership event didn't decode"))?;
    if events.is_empty() {
        return Ok(None);
    }
    Roster::replay_trusted(&events)
        .map(Some)
        .map_err(|e| ApiError::internal(format!("the stored membership chain broke: {e}")))
}

/// `GET /v1/shared`.
pub async fn shared_vaults(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> ApiResult<Cbor<SharedVaults>> {
    let store = store(&state)?;
    let mut out = SharedVaults::default();
    for (vault_id, right) in store.shared_with(session.account_id)? {
        let Some(row) = state.storage.vault(vault_id)? else {
            continue;
        };
        let Some(shared) = store.shared_vault(vault_id)? else {
            continue;
        };
        out.vaults.push(SharedVaultWire {
            summary: summary_of(&row),
            kind: shared.kind,
            owner: owner_party(&row)?,
            owner_login: login_of(&state, row.account_id),
            right,
            key_ref: shared.key_ref,
            membership_seq: shared.seq,
        });
    }
    for shared in store.owned_shared(session.account_id)? {
        out.sharing.push(OwnSharedVault {
            vault_id: shared.vault_id,
            kind: shared.kind,
            key_ref: shared.key_ref,
            membership_seq: shared.seq,
        });
    }
    Ok(Cbor(out))
}

/// `GET /v1/vaults/{vault}/members?since=N`.
pub async fn members_since(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(vault_id): Path<Id>,
    Query(since): Query<Since>,
) -> ApiResult<Cbor<MembershipSince>> {
    let (row, access) = vault_access(&state, &session, vault_id)?;
    if access == Access::Emergency {
        return Err(ApiError::forbidden(
            "emergency access doesn't show a vault's members",
        ));
    }
    let store = store(&state)?;
    let events = store.membership_events(vault_id, since.since)?;
    let current_seq = store.shared_vault(vault_id)?.map(|s| s.seq).unwrap_or(0);
    let mut logins = std::collections::BTreeMap::new();
    logins.insert(row.account_id, login_of(&state, row.account_id));
    for m in store.members(vault_id)? {
        logins.insert(m.party.account_id, login_of(&state, m.party.account_id));
    }
    Ok(Cbor(MembershipSince {
        events: events.into_iter().map(ByteBuf::from).collect(),
        current_seq,
        logins,
    }))
}

/// `GET /v1/vaults/{vault}/key`: the session's envelope at the generation
/// in force.
pub async fn vault_key(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(vault_id): Path<Id>,
) -> ApiResult<Cbor<EnvelopeWire>> {
    let (_, access) = vault_access(&state, &session, vault_id)?;
    if !matches!(access, Access::Member(_) | Access::Owner) {
        return Err(ApiError::not_found(
            "a vault's key envelope is for its members",
        ));
    }
    let store = store(&state)?;
    let shared = store
        .shared_vault(vault_id)?
        .ok_or_else(|| ApiError::not_found("that vault isn't shared"))?;
    let envelope = store
        .share_envelope(vault_id, shared.key_ref, session.account_id)?
        .ok_or_else(|| ApiError::not_found("no key was wrapped for this account yet"))?;
    Ok(Cbor(EnvelopeWire {
        envelope: ByteBuf::from(envelope),
    }))
}

fn check_envelope(
    env: &ShareEnvelope,
    key: &VerifyingKey,
    session: &Session,
    purpose: SharePurpose,
    vault_id: Id,
) -> ApiResult<()> {
    if env.place.purpose != purpose || env.place.vault_id != vault_id {
        return Err(ApiError::bad_request(
            "a share envelope names another vault or purpose",
        ));
    }
    if env.place.from_account != session.account_id || env.by_device != session.device_id {
        return Err(ApiError::bad_request(
            "a share envelope must come from the account and device sending it",
        ));
    }
    env.verify(key)
        .map_err(|_| ApiError::forbidden("a share envelope's signature didn't check out"))
}

/// `POST /v1/vaults/{vault}/members`.
pub async fn push_membership(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(vault_id): Path<Id>,
    Cbor(push): Cbor<MembershipPush>,
) -> ApiResult<Cbor<MembershipAccepted>> {
    let store = store(&state)?;
    let event = MembershipEvent::from_bytes(&push.event)
        .map_err(|_| ApiError::bad_request("that isn't a membership event"))?;
    if event.vault_id != vault_id {
        return Err(ApiError::bad_request("the event names another vault"));
    }
    if event.by_account != session.account_id || event.by_device != session.device_id {
        return Err(ApiError::forbidden(
            "a membership change is signed by the device that sends it",
        ));
    }
    let (row, _) = vault_access(&state, &session, vault_id)?;
    check_not_revoked(&state, &session, session.device_id, vault_id)?;
    let key = device_key(&state, &session, session.device_id)?;
    event
        .verify(&key)
        .map_err(|_| ApiError::forbidden("the membership change's signature didn't check out"))?;
    let roster = match roster_of(store, vault_id)? {
        None => {
            if row.account_id != session.account_id {
                return Err(ApiError::forbidden("only a vault's owner can share it"));
            }
            match &event.change {
                Change::Create { owner, .. } if *owner == owner_party(&row)? => {}
                _ => {
                    return Err(ApiError::bad_request(
                        "a vault is shared by its owner, as the persona that holds it",
                    ));
                }
            }
            Roster::replay_trusted(std::slice::from_ref(&event))
                .map_err(|e| ApiError::bad_request(e.to_string()))?
        }
        Some(mut roster) => {
            roster
                .apply_trusted(&event)
                .map_err(|e| ApiError::new(StatusCode::CONFLICT, ErrorCode::Fork, e.to_string()))?;
            roster
        }
    };
    for account in roster.members.keys() {
        if state.storage.account(*account)?.is_none() {
            return Err(ApiError::not_found(
                "a member names no account on this server",
            ));
        }
    }
    let mut envelopes = Vec::new();
    for bytes in &push.envelopes {
        let env = ShareEnvelope::from_bytes(bytes)
            .map_err(|_| ApiError::bad_request("that isn't a share envelope"))?;
        check_envelope(&env, &key, &session, SharePurpose::Vault, vault_id)?;
        if env.place.key_id != roster.key_ref {
            return Err(ApiError::bad_request(
                "a share envelope names another key generation than the vault's",
            ));
        }
        match roster.members.get(&env.place.to.account_id) {
            Some(m) if m.party == env.place.to => {}
            // A manager who isn't the owner hands the owner the new key too.
            None if env.place.to == roster.owner => {}
            _ => {
                return Err(ApiError::bad_request(
                    "a share envelope is for someone who isn't a member",
                ));
            }
        }
        envelopes.push((env.place.to.account_id, env.place.key_id, bytes.to_vec()));
    }
    let covered = |account: Id| envelopes.iter().any(|(a, _, _)| *a == account);
    let needed: Vec<Id> = match &event.change {
        Change::Add { member, .. } => vec![member.account_id],
        Change::Remove { .. } => {
            let mut all: Vec<Id> = roster.members.keys().copied().collect();
            if event.by_account != roster.owner.account_id {
                all.push(roster.owner.account_id);
            }
            all
        }
        _ => Vec::new(),
    };
    if let Some(missing) = needed.into_iter().find(|a| !covered(*a)) {
        return Err(ApiError::bad_request(format!(
            "the change needs the vault key wrapped for account {missing}"
        )));
    }
    let mut members: Vec<MemberRow> = roster
        .members
        .values()
        .map(|m| MemberRow {
            party: m.party,
            right: m.right,
            active: true,
        })
        .collect();
    members.extend(roster.former.values().map(|(party, _)| MemberRow {
        party: *party,
        right: Right::View,
        active: false,
    }));
    let shared = SharedRow {
        vault_id,
        owner_account: row.account_id,
        kind: roster.kind,
        key_ref: roster.key_ref,
        seq: roster.seq,
        head: roster.head,
    };
    let accepted = store.accept_membership(MembershipWrite {
        vault_id,
        shared: &shared,
        event: (&push.event, event.seq),
        members: &members,
        envelopes: &envelopes,
    })?;
    if !accepted {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            ErrorCode::Fork,
            "another membership change landed first; pull the members and try again",
        ));
    }
    Ok(Cbor(MembershipAccepted {
        seq: roster.seq,
        hash: roster.head,
    }))
}

// --- items shared on their own -----------------------------------------------------

/// `POST /v1/shares/items`.
pub async fn share_item(
    State(state): State<Arc<AppState>>,
    session: Session,
    Cbor(create): Cbor<ItemShareCreate>,
) -> ApiResult<StatusCode> {
    let store = store(&state)?;
    let (row, access) = vault_access(&state, &session, create.vault_id)?;
    if access != Access::Owner {
        return Err(ApiError::forbidden(
            "an item is shared on its own from a vault you own",
        ));
    }
    if !matches!(create.right, Right::View | Right::FillOnly) {
        return Err(ApiError::bad_request(
            "an item shared on its own can be viewed or filled, not changed",
        ));
    }
    if create.recipient.account_id == session.account_id {
        return Err(ApiError::bad_request("share it with someone else"));
    }
    state
        .storage
        .account(create.recipient.account_id)?
        .ok_or_else(|| ApiError::not_found("that person on this server"))?;
    let item = state
        .storage
        .item(row.vault_id, create.item_id)?
        .ok_or_else(|| ApiError::not_found("that item on this server; sync first"))?;
    let item = Item::from_bytes(&item.body)
        .map_err(|_| ApiError::internal("a stored item didn't decode"))?;
    if item.class == ItemClass::CrownJewel {
        return Err(ApiError::forbidden("a crown jewel is never shared"));
    }
    let key = device_key(&state, &session, session.device_id)?;
    let env = ShareEnvelope::from_bytes(&create.envelope)
        .map_err(|_| ApiError::bad_request("that isn't a share envelope"))?;
    check_envelope(&env, &key, &session, SharePurpose::Item, create.vault_id)?;
    if env.place.item_id != Some(create.item_id)
        || env.place.key_id != create.item_id
        || env.place.grant_id != Some(create.share_id)
        || env.place.to != create.recipient
    {
        return Err(ApiError::bad_request(
            "the envelope is for another item, share or person",
        ));
    }
    if let Some(existing) = store.item_share(create.share_id)?
        && (existing.owner_account != session.account_id
            || existing.item_id != create.item_id
            || existing.recipient != create.recipient)
    {
        return Err(ApiError::forbidden("that share is someone else's"));
    }
    if store.item_shares_from(session.account_id)?.len() >= 1_000 {
        return Err(ApiError::forbidden(
            "this account shares at most 1,000 items on their own",
        ));
    }
    store.put_item_share(&ItemShareRow {
        share_id: create.share_id,
        vault_id: create.vault_id,
        item_id: create.item_id,
        owner_account: session.account_id,
        recipient: create.recipient,
        right: create.right,
        envelope: create.envelope.into_vec(),
        created_at: state.now(),
    })?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /v1/shares/items`.
pub async fn received_items(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> ApiResult<Cbor<ReceivedItems>> {
    let store = store(&state)?;
    let mut out = ReceivedItems::default();
    for share in store.item_shares_to(session.account_id)? {
        let Some(vault) = state.storage.vault(share.vault_id)? else {
            continue;
        };
        if vault.account_id != share.owner_account {
            continue;
        }
        let Some(item) = state.storage.item(share.vault_id, share.item_id)? else {
            continue;
        };
        out.items.push(ReceivedItemWire {
            share_id: share.share_id,
            owner: owner_party(&vault)?,
            owner_login: login_of(&state, share.owner_account),
            right: share.right,
            envelope: ByteBuf::from(share.envelope),
            record: ByteBuf::from(vault.record.clone()),
            item: ItemWire {
                rev: item.rev,
                item: item.body,
            },
        });
    }
    Ok(Cbor(out))
}

/// `GET /v1/shares/items/sent`.
pub async fn sent_items(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> ApiResult<Cbor<SentItems>> {
    let store = store(&state)?;
    let items = store
        .item_shares_from(session.account_id)?
        .into_iter()
        .map(|s| SentItemWire {
            share_id: s.share_id,
            vault_id: s.vault_id,
            item_id: s.item_id,
            recipient: s.recipient,
            recipient_login: login_of(&state, s.recipient.account_id),
            right: s.right,
            created_at: s.created_at,
        })
        .collect();
    Ok(Cbor(SentItems { items }))
}

/// `DELETE /v1/shares/items/{share}`: by the person who shared it, or the
/// person it was shared with.
pub async fn unshare_item(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(share_id): Path<Id>,
) -> ApiResult<StatusCode> {
    let store = store(&state)?;
    let share = store
        .item_share(share_id)?
        .ok_or_else(|| ApiError::not_found("that share"))?;
    if share.owner_account != session.account_id && share.recipient.account_id != session.account_id
    {
        return Err(ApiError::forbidden("that share is someone else's"));
    }
    store.delete_item_share(share_id)?;
    Ok(StatusCode::NO_CONTENT)
}

// --- share links ---------------------------------------------------------------------

fn gone() -> ApiError {
    ApiError::not_found("this link has expired, was used up, or never existed")
}

/// `POST /v1/sends`.
pub async fn create_send(
    State(state): State<Arc<AppState>>,
    session: Session,
    body: Bytes,
) -> ApiResult<StatusCode> {
    let store = store(&state)?;
    let create: SendCreate = decode_body(&body, "a share link")?;
    let now = state.now();
    let _ = store.purge_sends(now);
    if create.expires_at <= now || create.expires_at > now + MAX_OPEN_SECS {
        return Err(ApiError::bad_request(
            "a share link lasts from a few minutes up to thirty days",
        ));
    }
    if create.max_views.is_some_and(|m| m == 0 || m > MAX_VIEWS) {
        return Err(ApiError::bad_request(format!(
            "a share link opens 1 to {MAX_VIEWS} times, or without a limit until it expires"
        )));
    }
    if create.ciphertext.len() as u64 > state.config.max_blob_bytes {
        return Err(ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            ErrorCode::BlobTooLarge,
            format!(
                "this server takes share links up to {} bytes",
                state.config.max_blob_bytes
            ),
        ));
    }
    if let Some(p) = &create.password
        && !p.params.meets(&send_password_floor())
    {
        return Err(ApiError::bad_request(
            "the link's password settings are below the floor",
        ));
    }
    if store.sends_of(session.account_id)?.len() >= MAX_OPEN_PER_ACCOUNT {
        return Err(ApiError::forbidden(format!(
            "an account keeps at most {MAX_OPEN_PER_ACCOUNT} share links open; delete some first"
        )));
    }
    if store.send(create.send_id)?.is_some() {
        return Err(ApiError::bad_request("that link id is taken"));
    }
    store.put_send(&SendRow {
        send_id: create.send_id,
        owner_account: session.account_id,
        created_at: now,
        expires_at: create.expires_at,
        max_views: create.max_views,
        views: 0,
        password: create.password,
        body: create.ciphertext.into_vec(),
    })?;
    Ok(StatusCode::CREATED)
}

/// `GET /v1/sends`.
pub async fn sends(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> ApiResult<Cbor<SendList>> {
    let store = store(&state)?;
    let _ = store.purge_sends(state.now());
    let sends = store
        .sends_of(session.account_id)?
        .into_iter()
        .map(|s| SendInfo {
            send_id: s.send_id,
            created_at: s.created_at,
            expires_at: s.expires_at,
            max_views: s.max_views,
            views: s.views,
            password: s.password.is_some(),
        })
        .collect();
    Ok(Cbor(SendList { sends }))
}

/// `DELETE /v1/sends/{send}`.
pub async fn delete_send(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(send_id): Path<Id>,
) -> ApiResult<StatusCode> {
    if store(&state)?.delete_send(session.account_id, send_id)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found("that link"))
    }
}

fn open_send_row(state: &AppState, send_id: Id) -> ApiResult<SendRow> {
    let row = store(state)?.send(send_id)?.ok_or_else(gone)?;
    if row.expires_at <= state.now() || row.max_views.is_some_and(|m| row.views >= m) {
        return Err(gone());
    }
    Ok(row)
}

/// `GET /v1/sends/{send}/meta`, no session.
pub async fn send_meta(
    State(state): State<Arc<AppState>>,
    peer: Peer,
    Path(send_id): Path<Id>,
) -> ApiResult<Cbor<SendMeta>> {
    brake(&state, &peer, None)?;
    let row = open_send_row(&state, send_id)?;
    Ok(Cbor(SendMeta {
        password: row.password.map(|p| SendPasswordMeta {
            salt: p.salt,
            params: p.params,
        }),
        expires_at: row.expires_at,
        views_left: row.max_views.map(|m| m.saturating_sub(row.views)),
    }))
}

/// `POST /v1/sends/{send}/open`, no session: counts one view.
pub async fn open_send(
    State(state): State<Arc<AppState>>,
    peer: Peer,
    Path(send_id): Path<Id>,
    Cbor(open): Cbor<SendOpen>,
) -> ApiResult<Cbor<SendOpened>> {
    brake(&state, &peer, Some(&format!("send:{send_id}")))?;
    let row = open_send_row(&state, send_id)?;
    if let Some(p) = &row.password {
        let ok = open
            .access
            .as_ref()
            .is_some_and(|a| lockdown_crypto::hash::hash(a) == p.verifier);
        if !ok {
            return Err(ApiError::forbidden("the link's password isn't right"));
        }
    }
    if !store(&state)?.count_view(send_id, state.now())? {
        return Err(gone());
    }
    Ok(Cbor(SendOpened {
        ciphertext: ByteBuf::from(row.body),
    }))
}

// --- request links -----------------------------------------------------------------------

/// `POST /v1/requests`.
pub async fn create_request(
    State(state): State<Arc<AppState>>,
    session: Session,
    Cbor(create): Cbor<RequestCreate>,
) -> ApiResult<StatusCode> {
    let store = store(&state)?;
    let now = state.now();
    if create.expires_at <= now || create.expires_at > now + MAX_OPEN_SECS {
        return Err(ApiError::bad_request(
            "a request stays open from a few minutes up to thirty days",
        ));
    }
    if store.requests_of(session.account_id)?.len() >= MAX_OPEN_PER_ACCOUNT {
        return Err(ApiError::forbidden(format!(
            "an account keeps at most {MAX_OPEN_PER_ACCOUNT} requests; delete some first"
        )));
    }
    if store.request(create.request_id)?.is_some() {
        return Err(ApiError::bad_request("that request id is taken"));
    }
    let key = RequestKey {
        kem_pub: create.kem_pub,
        profile_id: create.profile_id,
        persona_id: create.persona_id,
        expires_at: create.expires_at,
        open: true,
    };
    store.put_request(&RequestRow {
        request_id: create.request_id,
        owner_account: session.account_id,
        created_at: now,
        expires_at: create.expires_at,
        key: lockdown_crypto::cbor::to_canonical(&key)
            .map_err(|e| ApiError::internal(e.to_string()))?,
        answer: None,
        answered_at: None,
    })?;
    Ok(StatusCode::CREATED)
}

/// `GET /v1/requests`.
pub async fn requests(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> ApiResult<Cbor<RequestList>> {
    let requests = store(&state)?
        .requests_of(session.account_id)?
        .into_iter()
        .map(|r| RequestInfo {
            request_id: r.request_id,
            created_at: r.created_at,
            expires_at: r.expires_at,
            answered_at: r.answered_at,
            answer: r.answer.map(ByteBuf::from),
        })
        .collect();
    Ok(Cbor(RequestList { requests }))
}

/// `DELETE /v1/requests/{request}`.
pub async fn delete_request(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(request_id): Path<Id>,
) -> ApiResult<StatusCode> {
    if store(&state)?.delete_request(session.account_id, request_id)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found("that request"))
    }
}

/// `GET /v1/requests/{request}/key`, no session.
pub async fn request_key(
    State(state): State<Arc<AppState>>,
    peer: Peer,
    Path(request_id): Path<Id>,
) -> ApiResult<Cbor<RequestKey>> {
    brake(&state, &peer, None)?;
    let row = store(&state)?
        .request(request_id)?
        .ok_or_else(|| ApiError::not_found("this request has expired or never existed"))?;
    let mut key: RequestKey = decode_body(&row.key, "a request")?;
    key.open = row.answer.is_none() && row.expires_at > state.now();
    Ok(Cbor(key))
}

/// `POST /v1/requests/{request}/answer`, no session: the one answer.
pub async fn answer_request(
    State(state): State<Arc<AppState>>,
    peer: Peer,
    Path(request_id): Path<Id>,
    Cbor(answer): Cbor<RequestAnswer>,
) -> ApiResult<StatusCode> {
    brake(&state, &peer, Some(&format!("request:{request_id}")))?;
    if answer.answer.is_empty() || answer.answer.len() > MAX_ANSWER_BYTES {
        return Err(ApiError::bad_request("an answer is a short sealed message"));
    }
    if store(&state)?.answer_request(request_id, &answer.answer, state.now())? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found(
            "this request was already answered, has expired, or never existed",
        ))
    }
}

// --- emergency access ---------------------------------------------------------------------

fn state_of(row: &GrantRow, grant: &EmergencyGrant, now: i64) -> EmergencyState {
    match row.state.as_str() {
        "invited" => EmergencyState::Invited,
        "accepted" => EmergencyState::Accepted,
        "requested" => {
            if row
                .requested_at
                .is_some_and(|t| now >= t.saturating_add(grant.wait_secs as i64))
            {
                EmergencyState::Ready
            } else {
                EmergencyState::Requested
            }
        }
        "released" => EmergencyState::Released,
        "declined" => EmergencyState::Declined,
        _ => EmergencyState::Revoked,
    }
}

fn grant_wire(state: &AppState, row: &GrantRow) -> ApiResult<EmergencyWire> {
    let grant: EmergencyGrant = decode_body(&row.body, "an emergency grant")?;
    let events = store(state)?
        .emergency_events(row.grant_id)?
        .into_iter()
        .map(|e| EmergencyEventWire {
            seq: e.seq,
            at: e.at,
            kind: e.kind,
            by_account: e.by_account,
            step: e.step.map(ByteBuf::from),
        })
        .collect();
    Ok(EmergencyWire {
        grant: ByteBuf::from(row.body.clone()),
        owner_login: login_of(state, row.owner_account),
        contact_login: login_of(state, row.contact_account),
        state: state_of(row, &grant, state.now()),
        requested_at: row.requested_at,
        release_at: row
            .requested_at
            .map(|t| t.saturating_add(grant.wait_secs as i64)),
        events,
    })
}

fn check_emergency_envelopes(
    state: &AppState,
    session: &Session,
    key: &VerifyingKey,
    grant: &EmergencyGrant,
    envelopes: &[ByteBuf],
) -> ApiResult<Vec<(Id, Vec<u8>)>> {
    let mut out = Vec::new();
    for bytes in envelopes {
        let env = ShareEnvelope::from_bytes(bytes)
            .map_err(|_| ApiError::bad_request("that isn't a share envelope"))?;
        check_envelope(
            &env,
            key,
            session,
            SharePurpose::Emergency,
            env.place.vault_id,
        )?;
        if env.place.grant_id != Some(grant.grant_id)
            || env.place.to != grant.contact
            || !grant.vaults.contains(&env.place.vault_id)
        {
            return Err(ApiError::bad_request(
                "an emergency envelope is for another grant, person or vault",
            ));
        }
        let (_, access) = vault_access(state, session, env.place.vault_id)?;
        if access != Access::Owner {
            return Err(ApiError::forbidden(
                "emergency access covers vaults you own",
            ));
        }
        out.push((env.place.vault_id, bytes.to_vec()));
    }
    Ok(out)
}

/// `POST /v1/emergency`.
pub async fn create_emergency(
    State(state): State<Arc<AppState>>,
    session: Session,
    Cbor(create): Cbor<EmergencyCreate>,
) -> ApiResult<StatusCode> {
    let store = store(&state)?;
    let grant = EmergencyGrant::from_bytes(&create.grant)
        .map_err(|_| ApiError::bad_request("that isn't an emergency grant"))?;
    if grant.owner.account_id != session.account_id || grant.by_device != session.device_id {
        return Err(ApiError::forbidden(
            "an emergency grant is made by the owner's device that sends it",
        ));
    }
    let key = device_key(&state, &session, session.device_id)?;
    grant
        .verify(&key)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    state
        .storage
        .account(grant.contact.account_id)?
        .ok_or_else(|| ApiError::not_found("that person on this server"))?;
    if store.grant(grant.grant_id)?.is_some() {
        return Err(ApiError::bad_request("that grant id is taken"));
    }
    let envelopes = check_emergency_envelopes(&state, &session, &key, &grant, &create.envelopes)?;
    if grant
        .vaults
        .iter()
        .any(|v| !envelopes.iter().any(|(e, _)| e == v))
    {
        return Err(ApiError::bad_request(
            "every vault a grant names needs its key wrapped for the contact",
        ));
    }
    store.put_grant(
        &GrantRow {
            grant_id: grant.grant_id,
            owner_account: session.account_id,
            contact_account: grant.contact.account_id,
            body: create.grant.into_vec(),
            state: "invited".into(),
            requested_at: None,
        },
        &envelopes,
        state.now(),
    )?;
    Ok(StatusCode::CREATED)
}

/// `GET /v1/emergency`.
pub async fn emergency(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> ApiResult<Cbor<EmergencyList>> {
    let store = store(&state)?;
    let mut out = EmergencyList::default();
    for row in store.grants_by_owner(session.account_id)? {
        out.as_owner.push(grant_wire(&state, &row)?);
    }
    for row in store.grants_for_contact(session.account_id)? {
        out.as_contact.push(grant_wire(&state, &row)?);
    }
    Ok(Cbor(out))
}

/// `POST /v1/emergency/{grant}/step`.
pub async fn emergency_step(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(grant_id): Path<Id>,
    Cbor(push): Cbor<EmergencyStepPush>,
) -> ApiResult<Cbor<EmergencyWire>> {
    let store = store(&state)?;
    let step = EmergencyStep::from_bytes(&push.step)
        .map_err(|_| ApiError::bad_request("that isn't an emergency step"))?;
    if step.grant_id != grant_id
        || step.by_account != session.account_id
        || step.by_device != session.device_id
    {
        return Err(ApiError::bad_request(
            "a step names its grant and is signed by the device that sends it",
        ));
    }
    let key = device_key(&state, &session, session.device_id)?;
    step.verify(&key)
        .map_err(|e| ApiError::forbidden(e.to_string()))?;
    let row = store
        .grant(grant_id)?
        .ok_or_else(|| ApiError::not_found("that emergency grant"))?;
    let is_owner = row.owner_account == session.account_id;
    let is_contact = row.contact_account == session.account_id;
    if (step.action.by_owner() && !is_owner) || (!step.action.by_owner() && !is_contact) {
        return Err(ApiError::forbidden(
            "that step is for the other person on this grant",
        ));
    }
    let now = state.now();
    let refuse = |why: &str| ApiError::new(StatusCode::CONFLICT, ErrorCode::Stale, why.to_string());
    let (next, requested_at) = match (step.action, row.state.as_str()) {
        (_, "declined" | "revoked") => return Err(refuse("this grant has ended")),
        (EmergencyAction::Accept, "invited") => ("accepted", None),
        (EmergencyAction::Accept, _) => return Err(refuse("this grant was already accepted")),
        (EmergencyAction::Decline, _) => ("declined", None),
        (EmergencyAction::Request, "accepted") => ("requested", Some(now)),
        (EmergencyAction::Request, "invited") => {
            return Err(refuse("accept the grant before asking for access"));
        }
        (EmergencyAction::Request, _) => return Err(refuse("access was already asked for")),
        (EmergencyAction::Stop, "requested") => ("accepted", None),
        (EmergencyAction::Stop, "released") => {
            return Err(refuse(
                "the keys were already released; revoke the grant and change what matters",
            ));
        }
        (EmergencyAction::Stop, _) => return Err(refuse("nothing is waiting to stop")),
        (EmergencyAction::Revoke, _) => ("revoked", None),
    };
    store.step_grant(
        grant_id,
        next,
        requested_at,
        (step.action.as_str(), session.account_id, Some(&push.step)),
        now,
    )?;
    let row = store
        .grant(grant_id)?
        .ok_or_else(|| ApiError::not_found("that emergency grant"))?;
    Ok(Cbor(grant_wire(&state, &row)?))
}

/// `POST /v1/emergency/{grant}/envelopes`: the owner's fresh envelopes.
pub async fn emergency_envelopes(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(grant_id): Path<Id>,
    Cbor(push): Cbor<EmergencyEnvelopes>,
) -> ApiResult<StatusCode> {
    let store = store(&state)?;
    let row = store
        .grant(grant_id)?
        .ok_or_else(|| ApiError::not_found("that emergency grant"))?;
    if row.owner_account != session.account_id {
        return Err(ApiError::forbidden("that grant is someone else's"));
    }
    if matches!(row.state.as_str(), "declined" | "revoked") {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            ErrorCode::Stale,
            "this grant has ended",
        ));
    }
    let grant: EmergencyGrant = decode_body(&row.body, "an emergency grant")?;
    let key = device_key(&state, &session, session.device_id)?;
    let envelopes = check_emergency_envelopes(&state, &session, &key, &grant, &push.envelopes)?;
    store.replace_grant_envelopes(grant_id, &envelopes, session.account_id, state.now())?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /v1/emergency/{grant}/release`: the keys, once the waiting period
/// ran out with no stop; recorded once.
pub async fn emergency_release(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(grant_id): Path<Id>,
) -> ApiResult<Cbor<EmergencyRelease>> {
    let store = store(&state)?;
    let row = store
        .grant(grant_id)?
        .ok_or_else(|| ApiError::not_found("that emergency grant"))?;
    if row.contact_account != session.account_id {
        return Err(ApiError::forbidden("that grant names someone else"));
    }
    let grant: EmergencyGrant = decode_body(&row.body, "an emergency grant")?;
    match state_of(&row, &grant, state.now()) {
        EmergencyState::Released => {}
        EmergencyState::Ready => {
            store.step_grant(
                grant_id,
                "released",
                row.requested_at,
                ("released", session.account_id, None),
                state.now(),
            )?;
        }
        EmergencyState::Requested => {
            return Err(ApiError::new(
                StatusCode::FORBIDDEN,
                ErrorCode::Forbidden,
                "the waiting period hasn't run out yet",
            ));
        }
        _ => {
            return Err(ApiError::forbidden(
                "access hasn't been asked for, or the grant ended",
            ));
        }
    }
    let mut out = EmergencyRelease::default();
    for (vault_id, envelope) in store.emergency_envelopes(grant_id)? {
        let Some(vault) = state.storage.vault(vault_id)? else {
            continue;
        };
        out.vaults.push(ReleasedVault {
            summary: summary_of(&vault),
            envelope: ByteBuf::from(envelope),
        });
    }
    Ok(Cbor(out))
}

// --- the pages behind the links ---------------------------------------------------

fn page(title: &str, body: &str) -> Response {
    let html = format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>{title}</title></head><body><main><h1>{title}</h1>{body}</main></body></html>"
    );
    let mut response = (
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/html; charset=utf-8"),
        )],
        html,
    )
        .into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'none'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'",
        ),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// `GET /s/{send}`: what someone who opens a share link in a browser sees.
/// On a server that serves the web vault, the web vault's link page (the
/// fragment, the link's key, rides along without reaching the server).
pub async fn send_page(State(state): State<Arc<AppState>>, Path(send): Path<String>) -> Response {
    if let Some(to) = crate::web_vault::link_redirect(&state, "s", &send) {
        return to;
    }
    page(
        "A Lockdown share link",
        "<p>Someone shared something with you through Lockdown. It is locked on their device, and the key is in the part of this link after the <code>#</code>, which your browser never sends to this server.</p><p>To open it, paste the whole link into Lockdown: <strong>Sharing → Open a share link</strong>. Opening it in the browser arrives with the Lockdown web vault.</p>",
    )
}

/// `GET /r/{request}`: what someone who opens a request link sees; on a
/// server that serves the web vault, the web vault's answer page.
pub async fn request_page(
    State(state): State<Arc<AppState>>,
    Path(request): Path<String>,
) -> Response {
    if let Some(to) = crate::web_vault::link_redirect(&state, "r", &request) {
        return to;
    }
    page(
        "Someone asked you for a secret",
        "<p>Someone asked you to send them a password or other secret through Lockdown. What you send is locked to their key on your device, so this server can't read it, and the link takes one answer.</p><p>To answer, paste the whole link into Lockdown: <strong>Sharing → Answer a request</strong>. Answering in the browser arrives with the Lockdown web vault.</p>",
    )
}
