//! Shared vaults and items in step on every device of a person
//! (docs/plan/36-shared-sync.md; the gaps doc 35 §5 left, decided under
//! D38).
//!
//! Three rules sit here beside `split_hold`:
//!
//! - **A vault of this persona opens trusting its members.** A shared
//!   vault's heads and versions are signed by every member's devices, not
//!   only this identity's. At unlock and after each sync's sharing step, a
//!   vault of the persona that isn't open yet opens trusting this
//!   identity's devices and the devices of every writer its signed
//!   membership chain names, as this device pinned them through the log
//!   ([`Session::vault_trust`]). A vault whose store copy names a signer
//!   nobody vouches for yet waits unopened instead of failing the unlock or
//!   the sync ([`unknown_signers`]): a new device of the owner meets the
//!   vault's writers in its first sync, looks each one up, checks them
//!   against the log and pins them, and then opens it. Outside a split, a
//!   writer whose device is still unknown is looked up again and must
//!   continue the pin; during one, the vault waits as D423 says.
//! - **Items shared on their own meet nobody new during a split.** An item
//!   whose owner this device hasn't pinned, or that a pinned owner's device
//!   this device doesn't know signed, waits unapplied, with
//!   [`HELD_ITEMS_LINE`], and applies once the split clears; items from
//!   owners already pinned keep syncing ([`received_take`]). Outside a
//!   split, such an owner is looked up again and must continue the pin.
//! - **A held head stops the rest of that vault, and says so.** Heads chain
//!   hash to hash and every head lists every item's newest version, so a
//!   known member's head written after a held member's carries the held
//!   member's changes inside it: taking it would take changes signed by
//!   someone this device hasn't met. The stop stays, and the vault says
//!   [`HELD_AFTER`] ([`Hold::Heads`]).

use std::collections::{BTreeMap, BTreeSet};

use lockdown_crypto::VerifyingKey;
use lockdown_sync::HttpTransport;
use lockdown_vault::identity::DeviceTrust;
use lockdown_vault::transparency::SignedTreeHead;
use lockdown_vault::{Id, Store};

use super::Session;
use super::split_hold::{HELD, HELD_LINE};

/// What a shared vault says once a head from someone this device hasn't
/// met stopped it: the new members wait, and so does every later change.
pub(crate) const HELD_AFTER: &str = "New members wait until the server's log matches its mirrors again. Changes made after a new member's or a new device's wait too, even from members you already have.";

/// The line a sync adds while a shared vault is stopped at such a head.
pub(crate) const HELD_AFTER_LINE: &str = "A shared vault has changes from people this device hasn't met yet. They and every change made after them wait until the server's log matches its mirrors again, even from members you already have. Your other vaults keep syncing.";

/// The line a sync adds while items shared on their own by people this
/// device hasn't met wait.
pub(crate) const HELD_ITEMS_LINE: &str = "Items shared with you by people this device hasn't met yet wait until the server's log matches its mirrors again. Items from people you already have keep syncing.";

/// Why a shared vault waits during a split.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Hold {
    /// Membership changes wait from the first one that meets someone;
    /// heads from members already pinned still arrive.
    Members,
    /// A head signed by a device this vault doesn't trust stopped it, and
    /// every later head waits behind it.
    Heads,
}

impl Hold {
    /// What the vault's sharing panel says.
    pub(crate) fn words(self) -> &'static str {
        match self {
            Hold::Members => HELD,
            Hold::Heads => HELD_AFTER,
        }
    }

    /// The line the sync adds.
    pub(crate) fn line(self) -> &'static str {
        match self {
            Hold::Members => HELD_LINE,
            Hold::Heads => HELD_AFTER_LINE,
        }
    }
}

/// The devices that signed a head or an item version of `vault_id` in
/// `store` and that `known` doesn't vouch for (`own`, this device, always
/// counts as known). Empty when the vault would open with that trust as
/// far as signers go.
pub(crate) fn unknown_signers(
    store: &dyn Store,
    vault_id: Id,
    own: Id,
    known: &dyn Fn(Id) -> bool,
) -> Result<BTreeSet<Id>, String> {
    let mut signers: BTreeSet<Id> = store
        .heads(vault_id)
        .map_err(|e| e.to_string())?
        .iter()
        .map(|h| h.signer)
        .collect();
    for item_id in store.list_items(vault_id).map_err(|e| e.to_string())? {
        let item = store
            .get_item(vault_id, item_id)
            .map_err(|e| e.to_string())?;
        signers.extend(item.versions.iter().map(|v| v.edited_by));
    }
    Ok(signers
        .into_iter()
        .filter(|d| *d != own && !known(*d))
        .collect())
}

/// Which items shared on their own to take this sync.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct ReceivedTake {
    /// One flag per item, in the order of the server's list.
    pub(crate) take: Vec<bool>,
    /// Pinned owners whose taken items name a device this device doesn't
    /// know: looked up again, and they must continue the pin.
    pub(crate) refresh: BTreeSet<Id>,
}

/// Decides [`ReceivedTake`] over `items`: each one's owner and the devices
/// that signed it (the envelope's and its versions'). Outside a split every
/// item is taken, and a pinned owner with a device this device doesn't
/// know is refreshed. During one, an item waits when its owner isn't pinned
/// or one of its devices isn't known, as membership changes wait (D423).
pub(crate) fn received_take(
    split: bool,
    items: &[(Id, Vec<Id>)],
    pinned: &dyn Fn(Id) -> bool,
    known_device: &dyn Fn(Id) -> bool,
) -> ReceivedTake {
    let mut out = ReceivedTake::default();
    for (owner, devices) in items {
        let met = pinned(*owner);
        let new_device = met && devices.iter().any(|d| !known_device(*d));
        if split && (!met || new_device) {
            out.take.push(false);
            continue;
        }
        out.take.push(true);
        if new_device {
            out.refresh.insert(*owner);
        }
    }
    out
}

pub(crate) use crate::vault_open::Waiting;

impl Session {
    /// Marks a shared vault as waiting for the split to clear, keeping the
    /// stronger reason when it already waits.
    pub(crate) fn hold(&mut self, vault_id: Id, hold: Hold) {
        let at = self.held_vaults.entry(vault_id).or_insert(hold);
        *at = (*at).max(hold);
    }

    /// The lines a sync adds for what waits: the strongest vault hold, and
    /// items shared on their own.
    pub(crate) fn held_lines(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(hold) = self.held_vaults.values().max() {
            out.push(hold.line().to_string());
        }
        if self.held_items > 0 {
            out.push(HELD_ITEMS_LINE.to_string());
        }
        out
    }

    /// Whether anything waits for the split to clear.
    pub(crate) fn anything_held(&self) -> bool {
        !self.held_vaults.is_empty() || self.held_items > 0
    }

    /// The devices a vault of this persona trusts when it opens: this
    /// identity's (its records and the synced set), and apart from them,
    /// for a shared vault, every device of each account its membership
    /// chain says ever held edit, as this device pinned them. The second
    /// set verifies heads and versions but never counts as the identity's
    /// own (the post-quantum rule holds only those to its key).
    pub(crate) fn vault_trust(&self, vault_id: Id) -> (DeviceTrust, BTreeMap<Id, VerifyingKey>) {
        let writers = self
            .rosters
            .get(&vault_id)
            .map(|roster| self.writer_keys(roster))
            .unwrap_or_default();
        crate::vault_open::vault_trust(&self.identity, &self.synced, vault_id, writers)
    }

    /// After the sync's sharing step: opens every vault of the persona in
    /// the store that isn't open yet, trusting its writers. One that still
    /// names a signer nobody vouches for waits during a split (the vault
    /// says why); outside one, a shared vault's writers are looked up again
    /// (each must continue the pin) and it opens, or the sync fails with
    /// the load's own words as it always did.
    pub(crate) fn open_pulled_vaults(
        &mut self,
        transport: &HttpTransport,
        token: &str,
        sth: &SignedTreeHead,
    ) -> Result<(), String> {
        for w in self.open_new_vaults()? {
            let shared = self.rosters.contains_key(&w.vault_id);
            if shared && (self.split_view() || self.held_vaults.contains_key(&w.vault_id)) {
                self.hold(w.vault_id, Hold::Heads);
                continue;
            }
            if !shared {
                return Err(w.why);
            }
            self.refresh_writers(transport, token, sth, w.vault_id)?;
            if let Some(still) = self
                .open_new_vaults()?
                .into_iter()
                .find(|s| s.vault_id == w.vault_id)
            {
                return Err(still.why);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_head_stop_outranks_a_membership_hold_and_says_more() {
        assert!(Hold::Heads > Hold::Members);
        assert_eq!(Hold::Members.words(), HELD);
        assert!(Hold::Heads.words().starts_with(HELD));
        assert!(
            Hold::Heads
                .words()
                .contains("even from members you already have")
        );
        assert!(
            Hold::Heads
                .line()
                .contains("Your other vaults keep syncing.")
        );
        assert!(
            !Hold::Heads
                .line()
                .contains("Items from members you already have keep syncing")
        );
        assert!(
            Hold::Members
                .line()
                .contains("Items from members you already have keep syncing")
        );
    }

    #[test]
    fn a_signer_nobody_vouches_for_is_named() {
        use lockdown_vault::{ItemClass, ItemKind, PlainField, Vault, VaultContext};
        let device = Id::new();
        let ctx = VaultContext {
            profile_id: Id::new(),
            persona_id: Id::new(),
            vault_id: Id::new(),
            app_data_key_id: Id::new(),
            crown_jewel_key_id: Id::new(),
        };
        let key = lockdown_crypto::SigningKey::generate().unwrap();
        let mut vault = Vault::create(ctx, device, Box::new(key), None).unwrap();
        vault
            .create_item(
                ItemKind::Note,
                ItemClass::Standard,
                vec![PlainField::text("title", "Title", "Door code")],
            )
            .unwrap();
        vault.commit().unwrap();
        let mut store = lockdown_store::MemoryStore::new();
        use ecosystem_identity::SymmetricKey as _;
        let app_data_key = ecosystem_identity::keys::AppDataKey::generate().unwrap();
        vault.save(&mut store, &app_data_key).unwrap();
        // Signed by this device: nothing unknown.
        assert!(
            unknown_signers(&store, ctx.vault_id, device, &|_| false)
                .unwrap()
                .is_empty()
        );
        // Seen from another device that doesn't know it: named once, for
        // its head and its version alike.
        let other = Id::new();
        assert_eq!(
            unknown_signers(&store, ctx.vault_id, other, &|_| false).unwrap(),
            BTreeSet::from([device])
        );
        // Vouched for (a pinned member's device): nothing unknown.
        assert!(
            unknown_signers(&store, ctx.vault_id, other, &|d| d == device)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn during_a_split_only_items_from_pinned_owners_and_known_devices_are_taken() {
        let (alice, carol) = (Id::new(), Id::new());
        let (laptop, tablet, carols) = (Id::new(), Id::new(), Id::new());
        let items = [
            (alice, vec![laptop]),
            (carol, vec![carols]),
            (alice, vec![laptop, tablet]),
        ];
        let pinned = |a: Id| a == alice;
        let known = |d: Id| d == laptop;
        let outside = received_take(false, &items, &pinned, &known);
        assert_eq!(outside.take, vec![true, true, true]);
        assert_eq!(outside.refresh, BTreeSet::from([alice]));
        let during = received_take(true, &items, &pinned, &known);
        assert_eq!(during.take, vec![true, false, false]);
        assert!(during.refresh.is_empty());
        assert!(HELD_ITEMS_LINE.contains("matches its mirrors again"));
    }
}
