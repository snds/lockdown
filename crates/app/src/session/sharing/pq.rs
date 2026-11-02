//! Post-quantum signatures between people (docs/plan/47-shared-vault-pq.md):
//! what this device signs with the identity's ML-DSA-65 key beside its
//! Ed25519 one when it writes a shared vault or a sharing record, and what
//! it holds other people's heads, membership changes and grants to.
//!
//! **Whose key.** This identity's own key is derived from the root key and
//! checked against the profile (D339, `vault_pq`). Another person's is the
//! one their pinned contact holds (`Contact::pq_isk`): pinned on first
//! sight, and a later lookup that drops or changes it is refused
//! (`Contact::check_continues`, D248). A person whose profile publishes no
//! key yet has none pinned: their records carry nothing this device can
//! check, and a `pq_sig` from them counts for nothing.
//!
//! **The rules, per person.** Each identity follows doc 28's rollout for
//! what it signs itself, wherever it signs: once its key is published and
//! every one of its devices reads post-quantum heads, or once the chain it
//! extends already carries one of its own. A verifier holds each person's
//! heads (per vault, `MemberPq`, with the first confirmed one remembered)
//! and membership changes (per chain) to that person's own signatures:
//! once one carries a verified signature, every later one by them must.
//! Nobody waits for anybody else: the signature sits outside every signed
//! byte and hash, so an older build ignores it, and a member who can't sign
//! yet is never held to it.

use std::sync::Arc;

use lockdown_crypto::pq_sign::{PqSigningKey, PqVerifyingKey};
use lockdown_vault::pq::MemberPq;

use super::*;

impl Session {
    /// This identity's post-quantum head setting (`vault_pq`), from its
    /// records as this device holds them now.
    pub(super) fn own_pq(&self) -> Result<Option<lockdown_vault::pq::HeadPq>, String> {
        let signer: Box<dyn lockdown_vault::Signer> = Box::new(self.keystore.clone());
        lockdown_vault::identity::vault_pq(
            &self.identity,
            signer.as_ref(),
            self.own_device_keys().into_keys(),
        )
        .map_err(|e| e.to_string())
    }

    /// The key a sharing record this device writes is signed with beside
    /// Ed25519, if any: the identity's rollout is done, or (for a shared
    /// vault's membership chain, `chain`) one of this account's changes in
    /// it already carries one.
    pub(super) fn sharing_pq_key(
        &self,
        chain: Option<Id>,
    ) -> Result<Option<Arc<PqSigningKey>>, String> {
        let Some(pq) = self.own_pq()? else {
            return Ok(None);
        };
        let Some(key) = pq.signer else {
            return Ok(None);
        };
        if pq.ready {
            return Ok(Some(key));
        }
        let me = self.sharing.account_id();
        let carried = chain.and_then(|v| self.sharing.vault(v)).is_some_and(|s| {
            s.events.iter().any(|b| {
                MembershipEvent::from_bytes(b)
                    .is_ok_and(|e| Some(e.by_account) == me && e.pq_sig.is_some())
            })
        });
        Ok(carried.then_some(key))
    }

    /// Signs `envelope` post-quantum when [`Self::sharing_pq_key`] says so.
    pub(super) fn pq_sign_envelope(
        &self,
        envelope: &mut ShareEnvelope,
        chain: Option<Id>,
    ) -> Result<(), String> {
        if let Some(key) = self.sharing_pq_key(chain)? {
            envelope
                .sign_post_quantum(&key)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// An account's post-quantum key as this device knows it: this
    /// identity's published one for its own account, else the pinned
    /// contact's. `None` when they publish none (as far as this device saw).
    pub(super) fn pq_key_of(&self, account: Id) -> Option<PqVerifyingKey> {
        if Some(account) == self.sharing.account_id() {
            return lockdown_vault::pq::published_key(self.identity.profile())
                .ok()
                .flatten();
        }
        self.sharing.contact(account)?.pq_key().ok().flatten()
    }

    /// Checks a shared vault's membership chain, as the sharing record
    /// holds it, against each maker's post-quantum key (`pq::check_events`).
    pub(super) fn check_membership_pq(&self, vault_id: Id) -> Result<(), String> {
        let Some(share) = self.sharing.vault(vault_id) else {
            return Ok(());
        };
        let events = share
            .events
            .iter()
            .map(|b| MembershipEvent::from_bytes(b))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        lockdown_vault::pq::check_events(&events, &|a| self.pq_key_of(a))
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    /// Whether the vault key envelope for `key_ref` that `from` sent this
    /// persona must carry a post-quantum signature: the membership change
    /// that handed it over (this persona's latest addition, or the removal
    /// that made the generation, by `from`) carries one.
    pub(super) fn envelope_needs_pq(&self, vault_id: Id, key_ref: Id, from: Id) -> bool {
        let Some(me) = self.sharing.account_id() else {
            return false;
        };
        let Some(share) = self.sharing.vault(vault_id) else {
            return false;
        };
        share
            .events
            .iter()
            .rev()
            .filter_map(|b| MembershipEvent::from_bytes(b).ok())
            .find(|e| {
                e.key_ref == key_ref
                    && match &e.change {
                        Change::Add { member, .. } => member.account_id == me,
                        Change::Remove { .. } => true,
                        _ => false,
                    }
            })
            .is_some_and(|e| e.by_account == from && e.pq_sig.is_some())
    }

    /// The other writers of a shared vault, by account, for its heads'
    /// post-quantum rules: their devices as pinned, their pinned key, and
    /// where this device remembers their first confirmed post-quantum head.
    pub(super) fn member_pq(&self, roster: &Roster) -> BTreeMap<Id, MemberPq> {
        let me = self.sharing.account_id();
        let own = self.own_device_keys();
        let remembered = self.memory.member_pq_since(roster.vault_id);
        roster
            .writers
            .iter()
            .filter(|a| Some(**a) != me)
            .filter_map(|account| {
                let contact = self.sharing.contact(*account)?;
                Some((
                    *account,
                    MemberPq {
                        devices: contact
                            .devices
                            .keys()
                            .filter(|d| !own.contains_key(d))
                            .copied()
                            .collect(),
                        key: contact.pq_key().ok().flatten(),
                        since: remembered.get(account).copied(),
                    },
                ))
            })
            .collect()
    }

    /// Holds an open shared vault's other writers' heads to their
    /// post-quantum rules. Refused when the chain held breaks one.
    pub(super) fn hold_members_to_pq(&mut self, vault_id: Id) -> Result<(), String> {
        let Some(roster) = self.rosters.get(&vault_id).cloned() else {
            return Ok(());
        };
        let members = self.member_pq(&roster);
        let vault = self.vault_slot(vault_id)?;
        vault
            .set_member_post_quantum(members)
            .map_err(|e| e.to_string())
    }

    /// Looks up again, through the log, every person in a shared vault this
    /// device doesn't yet see as signing post-quantum (no key pinned, or a
    /// device below level 2), so their key is pinned once they publish it
    /// and the vault's rollout can be told. A failed lookup leaves the pin
    /// as it was. Not during a split, when nobody is looked up (D423).
    pub(super) fn refresh_unready(
        &mut self,
        transport: &HttpTransport,
        token: &str,
        sth: &SignedTreeHead,
        vault_id: Id,
    ) -> Result<(), String> {
        if self.split_view() {
            return Ok(());
        }
        let Some(roster) = self.rosters.get(&vault_id).cloned() else {
            return Ok(());
        };
        let me = self.sharing.account_id();
        let unready: Vec<Id> = roster
            .parties()
            .into_iter()
            .map(|(p, _)| p.account_id)
            .filter(|a| Some(*a) != me)
            .filter(|a| {
                self.sharing
                    .contact(*a)
                    .is_some_and(|c| !c.signs_heads_post_quantum())
            })
            .collect();
        if unready.is_empty() {
            return Ok(());
        }
        for account in unready {
            let _ = self.pin(transport, token, sth, Ok(account));
        }
        if self.vaults().any(|v| v.context().vault_id == vault_id) {
            self.apply_roster(vault_id)?;
        }
        Ok(())
    }
}
