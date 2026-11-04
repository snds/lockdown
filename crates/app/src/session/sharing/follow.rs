//! An item shared on its own follows other people's edits
//! (docs/plan/54-share-follows-edits.md).
//!
//! A person holding one item someone shared on its own trusts only the
//! owner's devices for it (docs/plan/18-sharing.md): their device opens the
//! item only when one of the owner's devices signed its newest version. In
//! a shared vault, a member with edit can change the item, and a manager
//! can re-key the vault; either way the newest version is someone else's,
//! and the person kept the last copy that opened (doc 51, D596).
//!
//! The owner's device closes that gap. Each sync, after a vault's round, it
//! looks at the owner's items shared on their own from that vault. Any
//! whose newest version none of the owner's devices signed gets a follow
//! version (`Vault::follow_edits`): the same fields and files, under the
//! item's current Item Key, signed by this device. The vault then syncs
//! once more in the same sync, so the follow version is on the server
//! before the sync ends, and before the end of the sync seals any new keys
//! (`rekey`). The person's next sync takes it like any edit of the owner's.
//!
//! Only what the owner's vault took is followed, and a vault takes a
//! version only once it verified under a writer's pinned key; a vault with
//! anything held back is left alone (`quiet`). So nothing that didn't
//! verify is ever signed again. While the owner's device is offline, the
//! person's copy stays as it was and catches up later; nothing is shown.

use super::*;

impl Session {
    /// The owner's items shared on their own, by vault, for every vault of
    /// this persona's own that is shared and open here: the vaults where
    /// someone else can write. Asks the server only when there is such a
    /// vault. A list the server doesn't send now answers nothing, and the
    /// items are followed at a later sync.
    pub(in crate::session) fn shared_on_their_own(
        &self,
        transport: &HttpTransport,
        token: &str,
    ) -> BTreeMap<Id, Vec<Id>> {
        let stand_in = self.stand_in.then(|| self.opened.vault.context().vault_id);
        let own: BTreeSet<Id> = self
            .open_vaults()
            .map(|v| v.context().vault_id)
            .filter(|v| Some(*v) != stand_in)
            .filter(|v| self.rosters.contains_key(v) && self.vault_right(*v).is_none())
            .collect();
        let mut out: BTreeMap<Id, Vec<Id>> = BTreeMap::new();
        if own.is_empty() {
            return out;
        }
        let Ok(sent) = transport.sent_items(token) else {
            return out;
        };
        for s in sent.items.iter().filter(|s| own.contains(&s.vault_id)) {
            let items = out.entry(s.vault_id).or_default();
            if !items.contains(&s.item_id) {
                items.push(s.item_id);
            }
        }
        for items in out.values_mut() {
            items.sort();
        }
        out
    }

    /// Writes the follow version of each of `items` in `vault_id` whose
    /// newest version none of this persona's devices signed, and saves the
    /// vault. Answers whether anything was written, so the vault syncs
    /// again and the follow versions reach the server in this sync.
    pub(in crate::session) fn follow_edits(
        &mut self,
        vault_id: Id,
        items: &[Id],
    ) -> Result<bool, String> {
        let own = self.own_device_keys();
        let wrote = self
            .vault_slot(vault_id)?
            .follow_edits(items, &|d| own.contains_key(&d))
            .map_err(|e| e.to_string())?;
        if wrote.is_empty() {
            return Ok(false);
        }
        let open = if self.opened.vault.context().vault_id == vault_id {
            &mut self.opened
        } else {
            self.more
                .iter_mut()
                .find(|o| o.vault.context().vault_id == vault_id)
                .ok_or_else(|| format!("vault {vault_id} isn't open here"))?
        };
        open.save(&mut self.store).map_err(|e| e.to_string())?;
        Ok(true)
    }
}
