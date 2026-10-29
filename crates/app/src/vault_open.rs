//! Opening a persona's vaults, one code path for the unlocked session and
//! the command line (docs/plan/37-cli-popup.md §1; docs/plan/36-shared-sync.md).
//!
//! A shared vault's heads and versions are signed by every member's
//! devices, not only this identity's, and vault ids are random, so neither
//! "open the first vault by id" nor "trust only my devices" holds for a
//! persona that shares a vault. The rules, as `Session::open` has run them
//! since slice 35 and `bins/cli` now runs them too:
//!
//! 1. Every vault of the persona is tried with this identity's devices
//!    ([`open_own_vaults`]). One that names a head or version signer the
//!    identity doesn't know waits ([`Waiting`]); any other refusal stops
//!    the open.
//! 2. The persona's first vault (where new items go) is the first of the
//!    ones that opened, in the app's order: the person's own order, then
//!    the oldest, then the id ([`order_vaults`]). The id only breaks ties,
//!    so a shared vault can't push the person's first vault aside.
//! 3. A vault that waited is tried again trusting, apart from the
//!    identity's devices, the devices of every writer its signed membership
//!    chain names, as this device pinned them in the sharing record
//!    ([`pinned_writers`], [`open_one`]); one that still names a signer
//!    nobody vouches for keeps waiting until a sync meets its writers.
//!
//! The writers' devices verify heads and versions but never count as the
//! identity's own (`open_shared_vault_trusting`, D431).

use std::collections::BTreeMap;
use std::rc::Rc;

use crate::local_unlock::DeviceKeys;
use ecosystem_identity::Identity;
use lockdown_crypto::VerifyingKey;
use lockdown_store::{HeadMemory, SyncedIdentity};
use lockdown_vault::identity::{
    DeviceTrust, OpenVault, device_keys, open_shared_vault_trusting, persona_vaults,
};
use lockdown_vault::sharing::{MembershipEvent, Roster};
use lockdown_vault::{Id, Store};

use crate::session::shared_sync::unknown_signers;
use crate::sharing::SharingState;

/// A vault of the persona that couldn't open with the trust it has yet.
#[derive(Clone, Debug)]
pub struct Waiting {
    /// The vault.
    pub vault_id: Id,
    /// The load's own refusal, said if nothing more can vouch for it.
    pub why: String,
}

/// What [`open_own_vaults`] found.
#[derive(Debug, Default)]
pub struct PersonaVaults {
    /// The vaults that opened, the persona's first vault first.
    pub opened: Vec<OpenVault>,
    /// The vaults that name a signer this identity doesn't know, in the
    /// store's order.
    pub waiting: Vec<Waiting>,
}

/// The vaults in the order they were made: by the place each one's info
/// names (none for the first), then its first head's time, then its id.
/// Every device orders the same vaults the same way, the command line too.
pub fn order_vaults(all: &mut [OpenVault]) {
    all.sort_by_cached_key(|o| {
        (
            o.vault.info().ok().and_then(|i| i.order).unwrap_or(0),
            o.vault.heads().first().map(|h| h.signed_at.0).unwrap_or(0),
            o.vault.context().vault_id,
        )
    });
}

/// This identity's devices: its own records, and the synced record set's
/// for devices the local records don't hold yet.
pub fn own_device_keys(identity: &Identity, synced: &SyncedIdentity) -> BTreeMap<Id, VerifyingKey> {
    let mut keys = synced.device_keys();
    for (id, key) in device_keys(identity) {
        keys.entry(id).or_insert(key);
    }
    keys
}

/// The trust a vault of this persona opens with: this identity's devices
/// (and the boundaries of revoked ones), and apart from them `writers`
/// less any device that is the identity's own.
pub fn vault_trust(
    identity: &Identity,
    synced: &SyncedIdentity,
    vault_id: Id,
    writers: BTreeMap<Id, VerifyingKey>,
) -> (DeviceTrust, BTreeMap<Id, VerifyingKey>) {
    let keys = own_device_keys(identity, synced);
    let members = writers
        .into_iter()
        .filter(|(id, _)| !keys.contains_key(id))
        .collect();
    (
        DeviceTrust {
            keys,
            revoked_after: synced.revoked_after(vault_id),
        },
        members,
    )
}

/// Opens one vault of the persona with `trust` and the pinned `members`,
/// held to post-quantum heads from where this device first saw them
/// (doc 28). `Ok(Err(_))` when it names a signer neither vouches for, so
/// it waits; `Err` for any other refusal.
pub fn open_one(
    identity: &Identity,
    vault_id: Id,
    store: &dyn Store,
    keystore: &Rc<DeviceKeys>,
    memory: &HeadMemory,
    trust: &DeviceTrust,
    members: &BTreeMap<Id, VerifyingKey>,
) -> Result<Result<OpenVault, Waiting>, String> {
    match open_shared_vault_trusting(
        identity,
        vault_id,
        store,
        Box::new(keystore.clone()),
        memory.last_accepted_seq(vault_id),
        trust,
        members,
    ) {
        Ok(mut opened) => {
            opened
                .vault
                .require_post_quantum_from(memory.pq_since(vault_id))
                .map_err(|e| e.to_string())?;
            Ok(Ok(opened))
        }
        Err(e) => {
            let known = |d: Id| trust.keys.contains_key(&d) || members.contains_key(&d);
            let unmet = unknown_signers(store, vault_id, identity.device_id(), &known)?;
            if unmet.is_empty() {
                return Err(e.to_string());
            }
            Ok(Err(Waiting {
                vault_id,
                why: e.to_string(),
            }))
        }
    }
}

/// Step 1 and 2: every vault of the persona tried with this identity's
/// devices, the ones that opened in the app's order.
pub fn open_own_vaults(
    identity: &Identity,
    persona_id: Id,
    store: &dyn Store,
    keystore: &Rc<DeviceKeys>,
    memory: &HeadMemory,
    synced: &SyncedIdentity,
) -> Result<PersonaVaults, String> {
    let mut out = PersonaVaults::default();
    for vault_id in persona_vaults(identity, persona_id, store).map_err(|e| e.to_string())? {
        let (trust, none) = vault_trust(identity, synced, vault_id, BTreeMap::new());
        match open_one(identity, vault_id, store, keystore, memory, &trust, &none)? {
            Ok(opened) => out.opened.push(opened),
            Err(waiting) => out.waiting.push(waiting),
        }
    }
    order_vaults(&mut out.opened);
    Ok(out)
}

/// Every device the sharing record knows the owner of: this identity's
/// (under the persona's account) and every pinned contact's.
pub(crate) fn device_table(
    sharing: &SharingState,
    own: &BTreeMap<Id, VerifyingKey>,
) -> BTreeMap<Id, (Id, VerifyingKey)> {
    let mut out = BTreeMap::new();
    for c in sharing.contacts().values() {
        for id in c.devices.keys() {
            if let Some(key) = c.device_key(*id) {
                out.insert(*id, (c.account_id, key));
            }
        }
    }
    if let Some(me) = sharing.account_id() {
        for (id, key) in own {
            out.insert(*id, (me, key.clone()));
        }
    }
    out
}

/// The devices of one account, as pinned (this persona's own account's are
/// the identity's).
pub(crate) fn device_keys_of(
    sharing: &SharingState,
    own: &BTreeMap<Id, VerifyingKey>,
    account: Id,
) -> BTreeMap<Id, VerifyingKey> {
    if Some(account) == sharing.account_id() {
        return own.clone();
    }
    sharing
        .contact(account)
        .map(|c| {
            c.devices
                .keys()
                .filter_map(|id| c.device_key(*id).map(|k| (*id, k)))
                .collect()
        })
        .unwrap_or_default()
}

/// A shared vault's membership as its signed chain in the sharing record
/// says, or `None` for a vault the record doesn't share.
pub(crate) fn roster(
    sharing: &SharingState,
    own: &BTreeMap<Id, VerifyingKey>,
    vault_id: Id,
) -> Result<Option<Roster>, String> {
    let Some(share) = sharing.vault(vault_id) else {
        return Ok(None);
    };
    if share.events.is_empty() {
        return Ok(None);
    }
    let events = share
        .events
        .iter()
        .map(|b| MembershipEvent::from_bytes(b))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let table = device_table(sharing, own);
    let lookup = |d: Id| table.get(&d).cloned();
    Roster::replay(&events, &lookup)
        .map(Some)
        .map_err(|e| e.to_string())
}

/// The keys of every device whose heads and versions a shared vault takes:
/// the devices of every account its roster says ever held `edit`.
pub(crate) fn writer_keys(
    sharing: &SharingState,
    own: &BTreeMap<Id, VerifyingKey>,
    roster: &Roster,
) -> BTreeMap<Id, VerifyingKey> {
    let mut out = BTreeMap::new();
    for account in &roster.writers {
        out.extend(device_keys_of(sharing, own, *account));
    }
    out
}

/// Step 3's trust for one vault: its writers' devices as this device
/// pinned them, from the sharing record. Empty for a vault the record
/// doesn't share, or whose chain doesn't replay here.
pub fn pinned_writers(
    sharing: &SharingState,
    own: &BTreeMap<Id, VerifyingKey>,
    vault_id: Id,
) -> BTreeMap<Id, VerifyingKey> {
    match roster(sharing, own, vault_id) {
        Ok(Some(roster)) => writer_keys(sharing, own, &roster),
        _ => BTreeMap::new(),
    }
}
