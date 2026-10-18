//! The sync step itself (03-architecture §6), over any [`Transport`].
//!
//! One run of [`sync_vault`]:
//!
//! 1. **Pull.** Ask for every head after the last one the server confirmed
//!    to this device (`synced_seq`). A server that is now *behind* that is
//!    refused outright ("Rollback attempt blocked"); one whose head at
//!    `synced_seq` is not the one this device accepted is a fork. New heads
//!    are verified as a chain continuing this device's copy, signed by
//!    trusted devices, and replace whatever this device had after
//!    `synced_seq` (heads it committed offline were never on the server;
//!    their changes live on in the items). With a key-transparency session
//!    ([`sync_vault_with`]), the tree head each pulled head cites is
//!    checked against this device's view with a consistency proof: a
//!    server that showed the other device another log is a split view,
//!    refused.
//! 2. **Merge.** Items the new heads list are pulled, with the blobs their
//!    attachments name that this device lacks (each checked against its
//!    name before it is kept). One this device did not touch is taken as is. One both sides changed is merged field by
//!    field against the last version the two histories share; a field
//!    both sides changed differently keeps the server's value in the
//!    item and this device's whole version in a new *conflict copy*
//!    item, so nothing is overwritten silently.
//! 3. **Push.** Every item that differs from the synced head is sent with
//!    the revision this device last saw, after the attachment blobs its
//!    versions still hold keys for are on the server (content-addressed,
//!    so a blob already there costs one `HEAD`). `stale` means another
//!    device got there between the pull and the push: merge against the
//!    copy the server returned and go round again.
//! 4. **Head.** A fresh head is committed over the items with their
//!    server revisions, citing the tree head this device verified, and sent. `fork`, `gap` or `older_seq` means
//!    another device moved first: go round again. A few rounds settle any
//!    race; more than that is reported, not retried forever.
//!
//! [`bootstrap_vault`] is the first pull on a device that holds nothing
//! yet: it writes the record, heads and items into the store, and the
//! caller opens the vault the ordinary way, which verifies all of it.

use std::collections::BTreeSet;

use lockdown_vault::store::{Store, StoreError};
use lockdown_vault::{
    FieldValue, Hash32, Id, Item, ItemState, ManifestHead, PlainField, Vault, VaultRecord,
};
use serde::{Deserialize, Serialize};

use crate::kt::KtSession;
use crate::transport::{Transport, TransportError};
use crate::wire::{ErrorCode, ItemPush, PushReply, VaultSummary};
use crate::{SyncError, merge_fields};

/// How far one vault is synced on this device. Kept per device, never
/// synced itself.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct VaultSyncState {
    /// The newest manifest `seq` the server confirmed to this device.
    pub synced_seq: u64,
    /// The newest item revision this device has pulled or pushed.
    pub synced_rev: u64,
}

/// What one run did.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct SyncReport {
    /// Items sent and accepted.
    pub pushed: usize,
    /// Items taken from the server as they were.
    pub pulled: usize,
    /// Items merged field by field.
    pub merged: usize,
    /// Conflict copies made, by id.
    pub conflict_copies: Vec<Id>,
    /// Attachment blobs sent.
    pub blobs_pushed: usize,
    /// Attachment blobs fetched.
    pub blobs_pulled: usize,
    /// The vault's head `seq` afterwards.
    pub head_seq: u64,
    /// Whether heads this device had committed were replaced by the
    /// server's chain (the caller then resets what it remembers).
    pub chain_rewritten: bool,
    /// How many rounds it took.
    pub rounds: u32,
}

/// The most rounds one run tries before reporting a race it cannot settle.
pub const MAX_ROUNDS: u32 = 6;

/// Where this device keeps attachment blobs, as sync needs them: read one
/// to push it, write one that was pulled. Every [`Store`] is one.
pub trait BlobStore {
    /// A blob's bytes, if this device holds it.
    fn blob(&self, hash: Hash32) -> Result<Option<Vec<u8>>, StoreError>;
    /// Keeps a blob pulled from the server (already checked against its name).
    fn store_blob(&mut self, hash: Hash32, bytes: &[u8]) -> Result<(), StoreError>;
}

impl<S: Store + ?Sized> BlobStore for S {
    fn blob(&self, hash: Hash32) -> Result<Option<Vec<u8>>, StoreError> {
        match self.get_blob(hash) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(StoreError::NotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn store_blob(&mut self, hash: Hash32, bytes: &[u8]) -> Result<(), StoreError> {
        self.put_blob(hash, bytes)
    }
}

/// The blobs an item's versions still hold a key for.
fn blobs_of(item: &Item) -> Vec<Hash32> {
    let mut out: Vec<Hash32> = Vec::new();
    for a in item
        .versions
        .iter()
        .flat_map(|v| v.attachments.iter())
        .filter(|a| !a.is_shredded())
    {
        if !out.contains(&a.blob) {
            out.push(a.blob);
        }
    }
    out
}

/// Pulls the blobs `item` names that this device lacks, each checked
/// against its name before it is kept.
fn pull_blobs<B: BlobStore + ?Sized>(
    syncer: &Syncer<'_>,
    vault_id: Id,
    item: &Item,
    blobs: &mut B,
) -> Result<usize, SyncError> {
    let mut pulled = 0;
    for hash in blobs_of(item) {
        if blobs.blob(hash)?.is_some() {
            continue;
        }
        let bytes = syncer
            .transport
            .get_blob(syncer.token, vault_id, hash)
            .map_err(session_or)?
            .ok_or(SyncError::BlobMissing(hash))?;
        if lockdown_crypto::hash::hash(&bytes) != hash {
            return Err(SyncError::BlobCorrupt(hash));
        }
        blobs.store_blob(hash, &bytes)?;
        pulled += 1;
    }
    Ok(pulled)
}

/// Pushes the blobs `item` names that the server lacks: the ones sealed in
/// this run and not yet saved, or the store's.
fn push_blobs(
    syncer: &Syncer<'_>,
    vault: &Vault,
    item: &Item,
    blobs: &dyn BlobStore,
    done: &mut BTreeSet<Hash32>,
) -> Result<usize, SyncError> {
    let vault_id = vault.context().vault_id;
    let mut pushed = 0;
    for hash in blobs_of(item) {
        if done.contains(&hash) {
            continue;
        }
        if syncer
            .transport
            .has_blob(syncer.token, vault_id, hash)
            .map_err(session_or)?
        {
            done.insert(hash);
            continue;
        }
        let bytes = match vault.pending_blob(hash) {
            Some(b) => b.to_vec(),
            None => blobs.blob(hash)?.ok_or(SyncError::BlobMissing(hash))?,
        };
        syncer
            .transport
            .put_blob(syncer.token, vault_id, hash, &bytes)
            .map_err(session_or)?;
        done.insert(hash);
        pushed += 1;
    }
    Ok(pushed)
}

/// Who is syncing: the transport, the session, and the name used on
/// conflict copies.
pub struct Syncer<'a> {
    /// The server.
    pub transport: &'a dyn Transport,
    /// The session token.
    pub token: &'a str,
    /// What this device is called, for conflict copies.
    pub device_label: &'a str,
}

impl std::fmt::Debug for Syncer<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Syncer")
            .field("device_label", &self.device_label)
            .finish_non_exhaustive()
    }
}

pub(crate) fn session_or(e: TransportError) -> SyncError {
    match e {
        TransportError::Refused {
            code: ErrorCode::TokenExpired | ErrorCode::Unauthenticated,
            ..
        } => SyncError::SessionExpired,
        other => SyncError::Transport(other),
    }
}

/// Syncs `vault` with the server with no key-transparency session: what
/// [`sync_vault_with`] does with `kt` `None`. For callers with no log in
/// sight; an app syncs through [`sync_vault_with`].
pub fn sync_vault(
    syncer: &Syncer<'_>,
    vault: &mut Vault,
    record: &[u8],
    state: &mut VaultSyncState,
    blobs: &mut dyn BlobStore,
) -> Result<SyncReport, SyncError> {
    sync_vault_with(syncer, vault, record, state, blobs, None)
}

/// Syncs `vault` with the server. `record` is the vault's record bytes
/// (sent on the first contact so another device can find the vault);
/// `blobs` is where this device keeps attachment blobs (its store); `kt`
/// is the key-transparency session for this run, when the caller began
/// one: every pulled head's citation is checked against it and the head
/// committed here cites it.
pub fn sync_vault_with(
    syncer: &Syncer<'_>,
    vault: &mut Vault,
    record: &[u8],
    state: &mut VaultSyncState,
    blobs: &mut dyn BlobStore,
    kt: Option<&KtSession<'_>>,
) -> Result<SyncReport, SyncError> {
    let vault_id = vault.context().vault_id;
    let t = syncer.transport;
    let token = syncer.token;
    let mut report = SyncReport::default();
    let mut blobs_done: BTreeSet<Hash32> = BTreeSet::new();
    if state.synced_seq == 0 {
        t.put_vault(token, vault_id, record).map_err(session_or)?;
    }
    for round in 1..=MAX_ROUNDS {
        report.rounds = round;

        // 1. Pull heads.
        let remote = t
            .heads_since(token, vault_id, state.synced_seq)
            .map_err(session_or)?;
        if remote.current_seq < state.synced_seq {
            return Err(SyncError::Rollback {
                offered: remote.current_seq,
                remembered: state.synced_seq,
            });
        }
        let mut remote_heads = Vec::with_capacity(remote.heads.len());
        for bytes in &remote.heads {
            let head = ManifestHead::from_bytes(bytes)?;
            if let (Some(kt), Some(cited)) = (kt, head.sth) {
                kt.verify_citation(cited)?;
            }
            remote_heads.push(head);
        }
        if state.synced_seq > 0 {
            let ours = vault
                .head_at(state.synced_seq)
                .ok_or_else(|| {
                    SyncError::Unsupported(format!(
                        "this device's copy of manifest {} is missing",
                        state.synced_seq
                    ))
                })?
                .hash()?;
            let server_has = match remote_heads.first() {
                Some(first) => first.prev_hash == ours,
                None => remote.current_hash == Some(ours),
            };
            if !server_has {
                return Err(SyncError::Fork {
                    seq: state.synced_seq,
                });
            }
        }

        // 2. Merge what the new heads bring.
        if let Some(newest) = remote_heads.last().cloned() {
            let dirty_before: BTreeSet<Id> =
                vault.changed_since(state.synced_seq)?.into_iter().collect();
            if vault.seq() > state.synced_seq {
                report.chain_rewritten = true;
            }
            vault.rebase(state.synced_seq, remote_heads)?;
            let pulled = t
                .items_since(token, vault_id, state.synced_rev)
                .map_err(session_or)?;
            for wire_item in pulled.items {
                let mut theirs = Item::from_bytes(&wire_item.item)?;
                theirs.rev = wire_item.rev;
                let their_hash = theirs.latest_hash()?;
                let listed = newest.entries.iter().any(|e| {
                    e.item_id == theirs.item_id
                        && e.rev == theirs.rev
                        && e.version_hash == their_hash
                });
                if !listed {
                    // Pushed by a device whose head isn't in yet (or not
                    // what the signed head describes); next time.
                    continue;
                }
                report.blobs_pulled += pull_blobs(syncer, vault_id, &theirs, blobs)?;
                let local = vault.item(theirs.item_id).ok().cloned();
                match local {
                    Some(ours) if dirty_before.contains(&ours.item_id) => {
                        let copies = merge_item(vault, syncer.device_label, ours, theirs)?;
                        report.merged += 1;
                        report.conflict_copies.extend(copies);
                    }
                    _ => {
                        vault.adopt_item(theirs)?;
                        report.pulled += 1;
                    }
                }
            }
            state.synced_seq = newest.seq;
            state.synced_rev = newest
                .entries
                .iter()
                .map(|e| e.rev)
                .max()
                .unwrap_or(state.synced_rev)
                .max(state.synced_rev);
        }

        // 3. Push what changed here.
        let dirty = vault.changed_since(state.synced_seq)?;
        let mut stale = false;
        for item_id in &dirty {
            let ours = vault.item(*item_id)?.clone();
            report.blobs_pushed += push_blobs(syncer, vault, &ours, blobs, &mut blobs_done)?;
            let push = ItemPush {
                if_match: ours.rev,
                item: ours.to_bytes()?,
            };
            match t
                .push_item(token, vault_id, *item_id, &push)
                .map_err(session_or)?
            {
                PushReply::Accepted { rev } => {
                    vault.set_item_rev(*item_id, rev)?;
                    state.synced_rev = state.synced_rev.max(rev);
                    report.pushed += 1;
                }
                PushReply::Stale { current } => {
                    let mut theirs = Item::from_bytes(&current.item)?;
                    theirs.rev = current.rev;
                    report.blobs_pulled += pull_blobs(syncer, vault_id, &theirs, blobs)?;
                    let copies = merge_item(vault, syncer.device_label, ours, theirs)?;
                    report.merged += 1;
                    report.conflict_copies.extend(copies);
                    stale = true;
                }
            }
        }
        if stale {
            continue;
        }

        // 4. Publish a head if there is anything the server's chain lacks.
        if dirty.is_empty() && vault.seq() == state.synced_seq {
            report.head_seq = vault.seq();
            return Ok(report);
        }
        if vault.seq() > state.synced_seq {
            report.chain_rewritten = true;
        }
        vault.truncate_heads_after(state.synced_seq);
        if let Some(kt) = kt {
            vault.cite_sth(Some(kt.citation()));
        }
        let seq = vault.commit()?;
        let head = vault.head()?;
        match t
            .put_head(token, vault_id, &head.to_bytes()?)
            .map_err(session_or)?
        {
            Ok(accepted) => {
                state.synced_seq = accepted.seq;
                debug_assert_eq!(accepted.seq, seq);
                report.head_seq = seq;
                return Ok(report);
            }
            Err(refused) => match refused.code {
                ErrorCode::Fork | ErrorCode::Gap | ErrorCode::OlderSeq | ErrorCode::UnknownItem => {
                    continue;
                }
                other => {
                    return Err(SyncError::Transport(TransportError::Refused {
                        status: 409,
                        code: other,
                        message: refused.message.clone(),
                        head: Some(refused),
                    }));
                }
            },
        }
    }
    Err(SyncError::Unsettled(MAX_ROUNDS))
}

/// Pulls a vault this device has never held into `store`: the record, the
/// whole head chain, every item the newest head lists and the blobs
/// those items name. With a key-transparency session, every head's cited
/// tree head is checked against this device's view first. Returns the
/// sync state to continue from, or `None` when the server holds no head
/// yet. The caller opens the vault afterwards, which verifies everything
/// against the devices it trusts.
pub fn bootstrap_vault(
    syncer: &Syncer<'_>,
    store: &mut dyn Store,
    summary: &VaultSummary,
) -> Result<Option<VaultSyncState>, SyncError> {
    bootstrap_vault_with(syncer, store, summary, None)
}

/// [`bootstrap_vault`] with a key-transparency session: every head's cited
/// tree head is checked against this device's view before anything is
/// written to the store.
pub fn bootstrap_vault_with(
    syncer: &Syncer<'_>,
    store: &mut dyn Store,
    summary: &VaultSummary,
    kt: Option<&KtSession<'_>>,
) -> Result<Option<VaultSyncState>, SyncError> {
    let t = syncer.transport;
    let token = syncer.token;
    let record = VaultRecord::from_bytes(&summary.record)?;
    if record.context.vault_id != summary.vault_id {
        return Err(SyncError::Unsupported(format!(
            "the server's record for vault {} names vault {}",
            summary.vault_id, record.context.vault_id
        )));
    }
    let remote = t
        .heads_since(token, summary.vault_id, 0)
        .map_err(session_or)?;
    let mut heads = Vec::with_capacity(remote.heads.len());
    for bytes in &remote.heads {
        let head = ManifestHead::from_bytes(bytes)?;
        if let (Some(kt), Some(cited)) = (kt, head.sth) {
            kt.verify_citation(cited)?;
        }
        heads.push(head);
    }
    let Some(newest) = heads.last().cloned() else {
        return Ok(None);
    };
    let pulled = t
        .items_since(token, summary.vault_id, 0)
        .map_err(session_or)?;
    let mut items = Vec::new();
    for wire_item in pulled.items {
        let mut item = Item::from_bytes(&wire_item.item)?;
        item.rev = wire_item.rev;
        let item_hash = item.latest_hash()?;
        if newest
            .entries
            .iter()
            .any(|e| e.item_id == item.item_id && e.rev == item.rev && e.version_hash == item_hash)
        {
            items.push(item);
        }
    }
    let synced_rev = newest.entries.iter().map(|e| e.rev).max().unwrap_or(0);
    for item in &items {
        pull_blobs(syncer, summary.vault_id, item, store)?;
    }
    store.save_all(record, items, heads)?;
    Ok(Some(VaultSyncState {
        synced_seq: newest.seq,
        synced_rev,
    }))
}

/// Merges `theirs` (the server's copy) into the vault where `ours` is the
/// local copy both sides changed. Returns the conflict copies made.
fn merge_item(
    vault: &mut Vault,
    device_label: &str,
    ours: Item,
    theirs: Item,
) -> Result<Vec<Id>, SyncError> {
    let item_id = ours.item_id;
    let mut copies = Vec::new();

    // A purge on the server is final: keys are gone everywhere. What this
    // device still had readable becomes a conflict copy, if anything.
    if theirs.state == ItemState::Tombstone || theirs.is_shredded() {
        let our_fields = if ours.is_shredded() {
            None
        } else {
            Some(vault.open_envelope(&ours, ours.latest()?.n)?)
        };
        vault.adopt_item(theirs)?;
        if let Some(fields) = our_fields {
            copies.push(vault.create_item(
                ours.kind,
                ours.class,
                conflict_copy_fields(fields, device_label),
            )?);
        }
        return Ok(copies);
    }
    if ours.is_shredded() {
        // Purged here: carry the purge to the server's copy.
        vault.adopt_item(theirs)?;
        vault.purge_item(item_id)?;
        return Ok(copies);
    }

    // The last version both histories share is the base.
    let their_header = theirs.header();
    let their_hashes: Vec<Hash32> = theirs
        .versions
        .iter()
        .map(|v| v.hash(&their_header))
        .collect::<Result<_, _>>()?;
    let our_header = ours.header();
    let mut base_n = None;
    for v in ours.versions.iter().rev() {
        if their_hashes.contains(&v.hash(&our_header)?) {
            base_n = Some(v.n);
            break;
        }
    }
    let base_fields = match base_n {
        Some(n) => vault.open_envelope(&ours, n)?,
        None => Vec::new(),
    };
    let our_fields = vault.open_envelope(&ours, ours.latest()?.n)?;
    let their_fields = vault.open_envelope(&theirs, theirs.latest()?.n)?;
    let result = merge_fields(&base_fields, &our_fields, &their_fields);

    // The server's history wins the slot; the merge goes on top of it.
    let (our_state, their_state) = (ours.state, theirs.state);
    vault.adopt_item(theirs)?;
    let mut merged = result.merged.clone();
    for conflict in &result.conflicts {
        if let Some(slot) = merged.iter_mut().find(|f| f.field_id == conflict.field_id) {
            *slot = conflict.theirs.clone();
        }
    }
    if merged != their_fields {
        vault.update_item(item_id, merged)?;
    }
    if !result.conflicts.is_empty() {
        copies.push(vault.create_item(
            ours.kind,
            ours.class,
            conflict_copy_fields(our_fields, device_label),
        )?);
    }
    // A trash on either side sticks; so does an archive over a live copy.
    if our_state == ItemState::Trashed && their_state != ItemState::Trashed {
        vault.trash_item(item_id)?;
    } else if our_state == ItemState::Archived && their_state == ItemState::Live {
        vault.archive_item(item_id)?;
    }
    Ok(copies)
}

/// The fields of a conflict copy: this device's version with its title
/// marked, so the person can tell the two apart in a list.
fn conflict_copy_fields(mut fields: Vec<PlainField>, device_label: &str) -> Vec<PlainField> {
    let suffix = format!(
        " (conflict copy from {device_label}, {})",
        civil_date(lockdown_vault::Timestamp::now().0)
    );
    match fields.iter_mut().find(|f| f.field_id == "title") {
        Some(title) => {
            if let FieldValue::Text(t) = &mut title.value {
                t.push_str(&suffix);
            }
        }
        None => fields.insert(0, PlainField::text("title", "title", suffix.trim_start())),
    }
    fields
}

/// `YYYY-MM-DD` for seconds since the Unix epoch (UTC), with no date
/// library (the civil-from-days algorithm).
pub fn civil_date(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates_are_right() {
        assert_eq!(civil_date(0), "1970-01-01");
        assert_eq!(civil_date(951_782_400), "2000-02-29");
        assert_eq!(civil_date(1_792_206_090), "2026-10-17");
        assert_eq!(civil_date(-86_400), "1969-12-31");
    }

    #[test]
    fn a_conflict_copy_is_named_after_the_device_and_day() {
        let fields =
            conflict_copy_fields(vec![PlainField::text("title", "title", "Bank")], "laptop");
        match &fields[0].value {
            FieldValue::Text(t) => {
                assert!(t.starts_with("Bank (conflict copy from laptop, "), "{t}");
            }
            other => panic!("{other:?}"),
        }
        let fields =
            conflict_copy_fields(vec![PlainField::text("user", "username", "me")], "laptop");
        assert_eq!(fields[0].field_id, "title");
    }
}
