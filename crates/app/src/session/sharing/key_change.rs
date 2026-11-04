//! When another person's keys change (docs/plan/55-member-key-change.md).
//!
//! Every lookup of someone this device pinned goes through
//! [`Session::look_up`], which lets `Contact::take_lookup` decide:
//!
//! - **The same keys** continue the pin, as always.
//! - **A hand-over** the pinned keys signed, listed on the new identity's
//!   profile (`lockdown_vault::key_change`), moves the pin silently. What
//!   that person signed with the new keys then verifies, and anything of
//!   theirs held back clears at the sync (`quiet`).
//! - **The owner's word**: a membership change the owner of a shared vault
//!   this persona is in signed, naming exactly the new keys
//!   (`Roster::readmitted`), moves the pin too. Only the owner's signature
//!   roots the other members' trust in the vault, so a manager's can't.
//! - **Anything else** (a reset after a lost kit, or a server faking one)
//!   leaves the pin as it was. Nothing is wrapped to the new keys, and what
//!   they sign is held back with slice 48's quiet line. When this persona
//!   owns something the person is in (a shared vault, an item shared with
//!   them on its own), it is asked, once and calmly, whether to let them
//!   back in; the question stays until it is answered.
//!
//! The owner's yes ([`Session::let_back_in`]) is a membership change per
//! vault naming the new keys, with the vault key wrapped to them, and a new
//! share of each item shared with them on its own. Every other member's
//! device follows the change ([`Session::follow_keys`]).

use lockdown_sync::sharing::verify_directory_entry_unpinned;
use lockdown_vault::contact::{KeyMove, Lookup};

use super::*;
use crate::sharing::{KeyResetView, reset_question};

/// What a lookup came to.
pub(in crate::session) struct Looked {
    /// The pin as it stands now.
    pub(in crate::session) contact: Contact,
    /// Whether the server's log shows other keys nothing vouches for (the
    /// pin stayed).
    pub(in crate::session) changed: bool,
}

impl Session {
    /// Looks a person up through the log and decides against the pin
    /// (`Contact::take_lookup`): first sight pins them; the same keys
    /// continue; a hand-over or the owner's word moves the pin; other keys
    /// are only noted. Refused for what the log check refuses, for keys
    /// this device moved on from, and for an older identity.
    pub(in crate::session) fn look_up(
        &mut self,
        transport: &HttpTransport,
        token: &str,
        sth: &SignedTreeHead,
        who: Result<Id, &str>,
    ) -> Result<Looked, String> {
        let entry: DirectoryEntry = match who {
            Ok(account) => transport.directory_account(token, account),
            Err(login) => transport.directory(token, login),
        }
        .map_err(|e| e.to_string())?;
        let account = entry.account_id;
        let pinned = self.sharing.contact(account).cloned();
        let (fresh, handovers) = verify_directory_entry_unpinned(
            transport,
            sth,
            &entry,
            pinned.as_ref().map(|p| p.persona_id),
        )
        .map_err(transparency_err)?;
        let Some(pinned) = pinned else {
            self.sharing.put_contact(fresh.clone());
            self.save_sharing()?;
            return Ok(Looked {
                contact: fresh,
                changed: false,
            });
        };
        let lookup = pinned
            .take_lookup(&fresh, &handovers, &|c| self.vouched_for(c))
            .map_err(|e| e.to_string())?;
        match lookup {
            Lookup::Continues(contact) => {
                let mut dirty = contact != pinned;
                if dirty {
                    self.sharing.put_contact(contact.clone());
                }
                self.changed_keys.remove(&account);
                dirty |= self.sharing.clear_key_reset(account);
                if dirty {
                    self.save_sharing()?;
                }
                if contact != pinned {
                    // New devices, or a post-quantum key published: every
                    // open shared vault trusts and holds them to it now.
                    self.retrust()?;
                }
                Ok(Looked {
                    contact,
                    changed: false,
                })
            }
            Lookup::Moved(contact, _) => {
                self.move_pin(&pinned, contact.clone())?;
                Ok(Looked {
                    contact,
                    changed: false,
                })
            }
            Lookup::Changed(fresh) => {
                self.note_key_change(transport, token, &pinned, fresh)?;
                Ok(Looked {
                    contact: pinned,
                    changed: true,
                })
            }
        }
    }

    /// Whether a membership change the owner of a shared vault this
    /// persona is in signed names exactly `fresh`'s keys for them. The
    /// rosters hold only chains that checked out.
    fn vouched_for(&self, fresh: &Contact) -> bool {
        self.rosters.values().any(|r| {
            r.owner.account_id != fresh.account_id
                && r.readmitted
                    .get(&fresh.account_id)
                    .is_some_and(|(party, keys, _)| fresh.is_named_by(party, keys))
        })
    }

    /// Moves the pin from `old` to `new`: the record, what this device
    /// remembered of their post-quantum heads (now their former identity's),
    /// and every open shared vault's trust.
    fn move_pin(&mut self, old: &Contact, new: Contact) -> Result<(), String> {
        let account = new.account_id;
        self.sharing.put_contact(new);
        self.changed_keys.remove(&account);
        self.sharing.clear_key_reset(account);
        self.save_sharing()?;
        self.memory
            .retire_member_pq_since(account, old.profile_id)
            .map_err(|e| e.to_string())?;
        self.retrust()
    }

    /// Every open shared vault takes the pins as they are now.
    fn retrust(&mut self) -> Result<(), String> {
        let open: Vec<Id> = self
            .rosters
            .keys()
            .copied()
            .filter(|v| self.is_open(*v))
            .collect();
        for vault_id in open {
            self.apply_roster(vault_id)?;
        }
        Ok(())
    }

    /// Notes keys that changed with nothing to vouch for them; when this
    /// persona owns something the person is in, asks (persisted until
    /// answered).
    fn note_key_change(
        &mut self,
        transport: &HttpTransport,
        token: &str,
        pinned: &Contact,
        fresh: Contact,
    ) -> Result<(), String> {
        let account = pinned.account_id;
        self.changed_keys.insert(account, fresh.clone());
        if self.owns_with(transport, token, account)
            && self.sharing.put_key_reset(
                account,
                pinned.login.clone(),
                fresh,
                lockdown_vault::Timestamp::now().0,
            )
        {
            self.save_sharing()?;
        }
        Ok(())
    }

    /// The vaults this persona owns that `account` is in, in id order.
    fn owned_with(&self, account: Id) -> Vec<Id> {
        let me = self.sharing.account_id();
        self.rosters
            .iter()
            .filter(|(_, r)| Some(r.owner.account_id) == me && r.members.contains_key(&account))
            .map(|(v, _)| *v)
            .collect()
    }

    /// Whether this persona owns something `account` is in: a shared vault,
    /// or an item shared with them on its own.
    fn owns_with(&self, transport: &HttpTransport, token: &str, account: Id) -> bool {
        if !self.owned_with(account).is_empty() {
            return true;
        }
        transport
            .sent_items(token)
            .is_ok_and(|sent| sent.items.iter().any(|s| s.recipient.account_id == account))
    }

    /// The refusal of a deliberate step with someone whose keys changed.
    pub(super) fn changed_words(&self, pinned: &Contact) -> String {
        if self.sharing.key_resets().contains_key(&pinned.account_id) {
            format!(
                "{} reset their account. Let them back in from Sharing first; nothing is shared with their new keys until then.",
                pinned.login
            )
        } else {
            format!(
                "{}'s keys changed and nothing vouches for the new ones yet, so nothing is shared with them.",
                pinned.login
            )
        }
    }

    /// The devices of people whose keys changed with nothing to vouch for
    /// them, with whose they are: what a shared vault's sync holds back
    /// and names, instead of stopping at a signer nobody vouches for.
    pub(in crate::session) fn changed_devices(&self) -> BTreeMap<Id, Id> {
        let mut out = BTreeMap::new();
        for (account, fresh) in &self.changed_keys {
            for device in fresh.devices.keys() {
                out.insert(*device, *account);
            }
        }
        out
    }

    /// After a vault's membership pull: looks a member up again when the
    /// server's hint says their identity moved past the pin, and when the
    /// owner's signed change names keys for them this device hasn't moved
    /// to yet. Not during a split, when nobody is looked up (D423).
    pub(super) fn follow_keys(
        &mut self,
        transport: &HttpTransport,
        token: &str,
        sth: &SignedTreeHead,
        vault_id: Id,
        hints: &BTreeMap<Id, (u64, lockdown_vault::Hash32)>,
    ) -> Result<(), String> {
        if self.split_view() || self.held_vaults.contains_key(&vault_id) {
            return Ok(());
        }
        let Some(roster) = self.rosters.get(&vault_id).cloned() else {
            return Ok(());
        };
        let me = self.sharing.account_id();
        let mut again: BTreeSet<Id> = BTreeSet::new();
        for (account, (seq, hash)) in hints {
            if Some(*account) == me || roster.right_of(*account).is_none() {
                continue;
            }
            if let Some(c) = self.sharing.contact(*account)
                && (c.identity_seq, c.identity_hash) != (*seq, *hash)
            {
                again.insert(*account);
            }
        }
        for (account, (party, keys, _)) in &roster.readmitted {
            if Some(*account) == me {
                continue;
            }
            if let Some(c) = self.sharing.contact(*account)
                && !c.is_named_by(party, keys)
                && !c.former.iter().any(|f| f.isk_pub == keys.isk_pub)
            {
                again.insert(*account);
            }
        }
        for account in again {
            // A lookup that fails leaves the pin as it was; the next sync
            // tries again.
            let _ = self.pin_quiet(transport, token, sth, account);
        }
        Ok(())
    }

    /// The people waiting on this persona, as an owner, to let them back in.
    pub fn key_resets(&self) -> Vec<KeyResetView> {
        let mut out: Vec<KeyResetView> = self
            .sharing
            .key_resets()
            .iter()
            .map(|(account, r)| KeyResetView {
                account_id: account.to_string(),
                login: r.login.clone(),
                words: reset_question(&r.login),
                seen_at: r.seen_at,
            })
            .collect();
        out.sort_by(|a, b| a.login.cmp(&b.login));
        out
    }

    /// The owner's yes: lets `account` back in under the new keys the
    /// server's log shows, the keys this persona was asked about. Each
    /// vault this persona owns that they are in gets a membership change
    /// naming those keys, signed here, with the vault key wrapped to them;
    /// each item shared with them on its own is shared again to them; the
    /// pin moves. The keys must still be the ones asked about. Answers
    /// their login.
    pub fn let_back_in(&mut self, account: &str) -> Result<String, String> {
        self.refuse_while_split(Paused::ShareVault)?;
        let account = parse_id(account)?;
        let asked = self
            .sharing
            .key_resets()
            .get(&account)
            .cloned()
            .ok_or("nobody with that account is waiting to be let back in")?;
        let pinned = self
            .sharing
            .contact(account)
            .cloned()
            .ok_or("that person isn't pinned here")?;
        let (transport, token, server) = self.online()?;
        let sth = self.verified_sth(&transport, &token, &server)?;
        let me = self.learn_account(&transport, &token, &sth)?;
        let entry = transport
            .directory_account(&token, account)
            .map_err(|e| e.to_string())?;
        let (fresh, _) =
            verify_directory_entry_unpinned(&transport, &sth, &entry, Some(pinned.persona_id))
                .map_err(transparency_err)?;
        if fresh.pinned_keys() != asked.fresh.pinned_keys() {
            self.sharing
                .put_key_reset(account, pinned.login.clone(), fresh, now_secs());
            self.save_sharing()?;
            return Err(format!(
                "{}'s account changed again since you were asked; look at the question again.",
                pinned.login
            ));
        }
        let moved = match pinned
            .take_lookup(&fresh, &[], &|_| true)
            .map_err(|e| e.to_string())?
        {
            Lookup::Moved(c, KeyMove::LetBackIn) | Lookup::Moved(c, KeyMove::HandedOver) => c,
            Lookup::Continues(_) | Lookup::Changed(_) => {
                self.sharing.clear_key_reset(account);
                self.save_sharing()?;
                return Ok(pinned.login);
            }
        };
        lockdown_vault::pq::require_hybrid(&fresh.kem_pub, &fresh.login)
            .map_err(|e| e.to_string())?;
        // Each vault this persona owns that they are in, from the server's
        // newest membership.
        for vault_id in self.owned_with(account) {
            self.pull_membership(&transport, &token, &sth, vault_id)?;
        }
        for vault_id in self.owned_with(account) {
            // Held back records of the vault stop a change to who is in it
            // (D572), unless they are only heads the person's new keys
            // signed: the yes is what lets those in.
            let only_theirs = self
                .unverified
                .of_vault(vault_id)
                .all(|h| h.kind == crate::session::quiet::Kind::Head && h.who == Some(account));
            if !only_theirs {
                self.refuse_while_unverified(vault_id)?;
            }
            let roster = self.rosters.get(&vault_id).cloned().ok_or("not shared")?;
            let right = roster
                .right_of(account)
                .ok_or("they aren't in that vault any more")?;
            let mut envelope = self
                .vault_by_id(vault_id)?
                .seal_vault_key(fresh.party(), &fresh.kem_pub, me, SharePurpose::Vault, None)
                .map_err(|e| e.to_string())?;
            self.pq_sign_envelope(&mut envelope, Some(vault_id))?;
            let event = self.sign_event(roster.propose(
                Change::Add {
                    member: fresh.party(),
                    right,
                    keys: Some(fresh.member_keys()),
                },
                None,
                me,
                self.identity.device_id(),
            ))?;
            self.post_change(&transport, &token, vault_id, event, vec![envelope])?;
        }
        self.move_pin(&pinned, moved)?;
        // Items shared with them on their own: shared again to the new
        // keys, and the share to the old ones withdrawn (the share's owner
        // re-shares; the server keeps one recipient per share).
        let sent = transport.sent_items(&token).map_err(|e| e.to_string())?;
        for s in sent
            .items
            .iter()
            .filter(|s| s.recipient.account_id == account)
        {
            if self.holding(s.item_id).is_err() {
                continue;
            }
            let share_id = Id::new();
            let mut env = self
                .holding(s.item_id)?
                .seal_item_key(s.item_id, fresh.party(), &fresh.kem_pub, me, share_id)
                .map_err(|e| e.to_string())?;
            self.pq_sign_envelope(&mut env, None)?;
            transport
                .share_item(
                    &token,
                    &ItemShareCreate {
                        share_id,
                        vault_id: s.vault_id,
                        item_id: s.item_id,
                        recipient: fresh.party(),
                        right: s.right,
                        envelope: ByteBuf::from(env.to_bytes().map_err(|e| e.to_string())?),
                    },
                )
                .map_err(|e| e.to_string())?;
            transport
                .unshare_item(&token, s.share_id)
                .map_err(|e| e.to_string())?;
        }
        self.save()?;
        Ok(pinned.login)
    }
}
