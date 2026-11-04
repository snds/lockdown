//! Re-keyed items reach the server before anyone is handed their new keys
//! (docs/plan/51-rekey-push.md).
//!
//! Removing a member (`rotate_vault_key`) or withdrawing a share of one
//! item (`rekey_item`) moves items to fresh Item Keys. The vault writes a
//! re-key version for each (`lockdown_vault`, `share`), so sync sees the
//! item changed and pushes it like an edit. What this module adds:
//!
//! - **Order.** The envelopes that hand the new keys out (each item shared
//!   on its own from the vault, and, after a new Vault Key, each emergency
//!   grant covering it) are sealed only once the vault's re-keyed items are
//!   on the server. The vault is marked in the sharing record when it is
//!   re-keyed (`SharingState::mark_reseal`) and sealed at the end of the
//!   first sync that leaves nothing of it unpushed, so a failed sync or a
//!   restart only delays it.
//! - **Repair.** An earlier build moved keys without a version, so those
//!   items never left the device that re-keyed them. Once per vault, this
//!   device compares its copies with the server's: an item at the same
//!   revision and version whose Item Key envelope differs was re-keyed here
//!   and never pushed. It gets its re-key version now, and its shares are
//!   sealed again after the push.
//! - **The other side.** A person holding an item shared on its own keeps
//!   the copy that opens when a new share's key doesn't open the copy the
//!   server serves yet (`received_opens`), and takes the new one at a later
//!   sync. Nothing is held back over it and no line is shown.

use lockdown_sync::wire::ItemsSince;

use super::*;

impl Session {
    /// Writes the re-key version of every item an earlier build re-keyed
    /// on this device and never pushed, in each open vault this persona
    /// may change that this device hasn't compared with the server yet.
    /// Answers the vaults compared, to be recorded once their sync leaves
    /// nothing unpushed ([`Self::after_rekey_push`]). A vault whose items
    /// the server doesn't send now is compared at a later sync.
    pub(in crate::session) fn repair_rekeyed(
        &mut self,
        transport: &HttpTransport,
        token: &str,
    ) -> Result<Vec<Id>, String> {
        let stand_in = self.stand_in.then(|| self.opened.vault.context().vault_id);
        let due: Vec<Id> = self
            .open_vaults()
            .map(|v| v.context().vault_id)
            .filter(|v| Some(*v) != stand_in && !self.sharing.rekey_checked(*v))
            .filter(|v| self.vault_right(*v).is_none_or(Right::can_edit))
            .collect();
        let mut compared = Vec::new();
        for vault_id in due {
            let Ok(served) = transport.items_since(token, vault_id, 0) else {
                continue;
            };
            let (rotated, items) = self.reseal_unpushed(vault_id, &served)?;
            if rotated || !items.is_empty() {
                self.sharing.mark_reseal(vault_id, rotated, &items);
                self.save_sharing()?;
            }
            compared.push(vault_id);
        }
        Ok(compared)
    }

    /// The comparison for one vault (`Vault::reseal_unpushed`) against
    /// what the server serves. Answers whether any item had moved to
    /// another Vault Key generation, and the items re-sealed.
    fn reseal_unpushed(
        &mut self,
        vault_id: Id,
        served: &ItemsSince,
    ) -> Result<(bool, Vec<Id>), String> {
        let served: Vec<Item> = served
            .items
            .iter()
            .filter_map(|w| {
                let mut item = Item::from_bytes(&w.item).ok()?;
                item.rev = w.rev;
                Some(item)
            })
            .collect();
        let (items, rotated) = self
            .vault_slot(vault_id)?
            .reseal_unpushed(&served)
            .map_err(|e| e.to_string())?;
        Ok((rotated, items))
    }

    /// After a sync's vaults: records the vaults compared whose items are
    /// all on the server now, and seals again the shares of every marked
    /// vault whose re-keyed items are. A vault no longer open here has
    /// nothing to seal from and is forgotten; one whose sealing fails waits
    /// for the next sync.
    pub(in crate::session) fn after_rekey_push(
        &mut self,
        transport: &HttpTransport,
        token: &str,
        compared: &[Id],
    ) -> Result<(), String> {
        let mut changed = false;
        for vault_id in compared {
            if self.all_pushed(*vault_id) {
                self.sharing.mark_rekey_checked(*vault_id);
                changed = true;
            }
        }
        let marked: Vec<(Id, crate::sharing::Reseal)> = self
            .sharing
            .reseal()
            .iter()
            .map(|(v, r)| (*v, r.clone()))
            .collect();
        for (vault_id, reseal) in marked {
            if !self.is_open(vault_id) {
                self.sharing.resealed(vault_id);
                changed = true;
                continue;
            }
            if !self.all_pushed(vault_id) {
                continue;
            }
            if let Ok(waiting) = self.reseal_shares(transport, token, vault_id, &reseal) {
                self.sharing.resealed(vault_id);
                if !waiting.is_empty() {
                    self.sharing.mark_reseal(vault_id, false, &waiting);
                }
                changed = true;
            }
        }
        if changed {
            self.save_sharing()?;
        }
        Ok(())
    }

    /// Whether every item of an open vault is on the server as the head the
    /// server last confirmed lists it.
    fn all_pushed(&self, vault_id: Id) -> bool {
        let Some(state) = self.sync.as_ref().and_then(|c| c.vaults.get(&vault_id)) else {
            return false;
        };
        if self.stand_in && self.opened.vault.context().vault_id == vault_id {
            return false;
        }
        self.open_by_id(vault_id)
            .ok()
            .and_then(|o| o.vault.changed_since(state.synced_seq).ok())
            .is_some_and(|dirty| dirty.is_empty() && state.synced_seq > 0)
    }

    /// Seals the new keys for what `reseal` names: each item shared on its
    /// own from the vault (all of them after a new Vault Key, else the
    /// re-keyed ones), and after a new Vault Key each emergency grant
    /// covering the vault. A share whose recipient isn't pinned here, or
    /// whose item is gone, is passed over; one whose item is in the trash
    /// waits, and its item is answered, to be sealed once it is restored.
    /// A refusal from the server stops it all, to be tried again.
    fn reseal_shares(
        &mut self,
        transport: &HttpTransport,
        token: &str,
        vault_id: Id,
        reseal: &crate::sharing::Reseal,
    ) -> Result<Vec<Id>, String> {
        let me = self.me()?;
        let mut waiting = Vec::new();
        let sent = transport.sent_items(token).map_err(|e| e.to_string())?;
        for s in sent
            .items
            .iter()
            .filter(|s| s.vault_id == vault_id)
            .filter(|s| reseal.rotated || reseal.items.contains(&s.item_id))
        {
            let Some(contact) = self.sharing.contact(s.recipient.account_id).cloned() else {
                continue;
            };
            let vault = self.vault_by_id(vault_id)?;
            let Ok(mut env) = vault.seal_item_key(
                s.item_id,
                s.recipient,
                &contact.kem_pub,
                me.account_id,
                s.share_id,
            ) else {
                if vault
                    .item(s.item_id)
                    .is_ok_and(|i| i.state == ItemState::Trashed)
                    && !waiting.contains(&s.item_id)
                {
                    waiting.push(s.item_id);
                }
                continue;
            };
            self.pq_sign_envelope(&mut env, None)?;
            transport
                .share_item(
                    token,
                    &ItemShareCreate {
                        share_id: s.share_id,
                        vault_id,
                        item_id: s.item_id,
                        recipient: s.recipient,
                        right: s.right,
                        envelope: ByteBuf::from(env.to_bytes().map_err(|e| e.to_string())?),
                    },
                )
                .map_err(|e| e.to_string())?;
        }
        if !reseal.rotated {
            return Ok(waiting);
        }
        let list = transport.emergency(token).map_err(|e| e.to_string())?;
        for w in &list.as_owner {
            let Ok(grant) = EmergencyGrant::from_bytes(&w.grant) else {
                continue;
            };
            if !grant.vaults.contains(&vault_id)
                || matches!(w.state, EmergencyState::Declined | EmergencyState::Revoked)
            {
                continue;
            }
            let Some(contact) = self.sharing.contact(grant.contact.account_id).cloned() else {
                continue;
            };
            let mut env = self
                .vault_by_id(vault_id)?
                .seal_vault_key(
                    grant.contact,
                    &contact.kem_pub,
                    me.account_id,
                    SharePurpose::Emergency,
                    Some(grant.grant_id),
                )
                .map_err(|e| e.to_string())?;
            self.pq_sign_envelope(&mut env, None)?;
            transport
                .emergency_envelopes(
                    token,
                    grant.grant_id,
                    &EmergencyEnvelopes {
                        envelopes: vec![ByteBuf::from(env.to_bytes().map_err(|e| e.to_string())?)],
                    },
                )
                .map_err(|e| e.to_string())?;
        }
        Ok(waiting)
    }

    /// Whether an item shared on its own can be held as this device has it:
    /// its Item Key opens the newest version, and one of its owner's devices
    /// this device knows signed that version.
    pub(super) fn received_opens(&self, r: &ReceivedItem) -> bool {
        let Ok(item) = Item::from_bytes(&r.item) else {
            return false;
        };
        let Ok(bytes): Result<[u8; 32], _> = r.item_key.as_slice().try_into() else {
            return false;
        };
        if !lockdown_vault::received_key_opens(&r.ctx, &item, &ItemKey::from_bytes(bytes)) {
            return false;
        }
        let Ok(latest) = item.latest() else {
            return false;
        };
        self.device_keys_of(r.owner.account_id)
            .get(&latest.edited_by)
            .is_some_and(|k| latest.verify(&item.header(), k).is_ok())
    }
}
