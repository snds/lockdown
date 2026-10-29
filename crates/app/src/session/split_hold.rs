//! New members of a shared vault wait during a split (D413 and D414's
//! follow-up; docs/plan/35-revoke-split.md §2).
//!
//! While the account server's public log disagrees with a mirror (a split
//! view, `split_pause`), the server could be showing this device one set of
//! keys and the world another. The pause already refuses every new look-up
//! and share; what was left open is the sync of a vault someone else
//! shares, which pinned anybody its membership chain named. Now, during a
//! split, a shared vault's sync takes in nobody it hasn't met:
//!
//! - **Membership changes** are taken in order up to the first one that
//!   names an account this device hasn't pinned, or that is signed by a
//!   device of a pinned member this device doesn't know (that member's
//!   keys changed). That change and every one after it wait unapplied: the
//!   chain is a chain, so nothing is skipped. The vault says
//!   [`HELD`]; a vault shared with this person whose chain waits is not
//!   joined yet, and its key generation doesn't move.
//! - **Vault heads** signed by a device this vault doesn't trust (a member
//!   who waits, or a known member's new device) stop that one vault's sync
//!   for now, with the same sentence; every other vault keeps syncing, and
//!   the items of members already pinned keep arriving until such a head.
//!
//! When the split clears (the mirror agrees again, or the person removes a
//! mirror they added), the next sync takes the waiting changes: the new
//! accounts are looked up and checked against the log as ever, a known
//! member whose devices changed is looked up again and must continue the
//! pin (`Contact::check_continues`), and a vault stopped at an unknown
//! device refreshes its writers' pins and syncs once more. The sync that
//! sees the split clear runs that next round itself.
//!
//! The state is per session and recomputed by every sync, so a restart
//! forgets nothing that matters: the next sync holds the same changes again.

use std::collections::BTreeSet;

use lockdown_vault::sharing::{Change, MembershipEvent};
use lockdown_vault::{Id, ManifestHead};

use super::Session;

/// What a shared vault says while changes from people this device hasn't
/// met wait for the split to clear.
pub(crate) const HELD: &str = "New members wait until the server's log matches its mirrors again.";

/// The line a sync adds while any shared vault waits.
pub(crate) const HELD_LINE: &str = "A shared vault has changes from people this device hasn't met yet. New members wait until the server's log matches its mirrors again. Items from members you already have keep syncing.";

/// Every account a membership event names: who made it and whom it is about.
pub(crate) fn named(e: &MembershipEvent) -> Vec<Id> {
    let about = match &e.change {
        Change::Create { owner, .. } => owner.account_id,
        Change::Add { member, .. } => member.account_id,
        Change::SetRight { account_id, .. } | Change::Remove { account_id } => *account_id,
    };
    if about == e.by_account {
        vec![e.by_account]
    } else {
        vec![e.by_account, about]
    }
}

/// How much of a batch of new membership events to take now.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Take {
    /// The first this many events are taken; the rest wait.
    pub(crate) count: usize,
    /// Accounts the taken events name that this device hasn't pinned yet
    /// (looked up and checked against the log before they apply).
    pub(crate) meet: BTreeSet<Id>,
    /// Pinned accounts that signed a taken event with a device this device
    /// doesn't know: looked up again, and must continue the pin.
    pub(crate) refresh: BTreeSet<Id>,
}

impl Session {
    /// Whether a shared vault's changes wait for the split to clear.
    pub(crate) fn held_words(&self, vault_id: Id) -> Option<String> {
        self.held_vaults
            .get(&vault_id)
            .map(|h| h.words().to_string())
    }

    /// Takes `events` (new membership changes, oldest first) in order, up
    /// to the first one that would make this device meet someone while the
    /// account server's log is split. Outside a split everything is taken.
    pub(crate) fn membership_take(&self, events: &[MembershipEvent]) -> Take {
        let split = self.split_view();
        let me = self.sharing.account_id();
        let devices = self.device_table();
        let mut take = Take::default();
        for (i, e) in events.iter().enumerate() {
            let unmet: Vec<Id> = named(e)
                .into_iter()
                .filter(|a| {
                    Some(*a) != me && self.sharing.contact(*a).is_none() && !take.meet.contains(a)
                })
                .collect();
            let new_key = Some(e.by_account) != me
                && !take.meet.contains(&e.by_account)
                && !unmet.contains(&e.by_account)
                && !devices.contains_key(&e.by_device);
            if split && (!unmet.is_empty() || new_key) {
                take.count = i;
                return take;
            }
            take.meet.extend(unmet);
            if new_key {
                take.refresh.insert(e.by_account);
            }
        }
        take.count = events.len();
        take
    }

    /// Whether the server's heads of `vault_id` past what this device
    /// synced name a signer the vault doesn't trust: someone this device
    /// hasn't met, or a member's device it doesn't know yet.
    pub(crate) fn unmet_head_signer(
        &self,
        transport: &dyn lockdown_sync::Transport,
        token: &str,
        vault_id: Id,
    ) -> Result<bool, String> {
        let synced = self
            .sync
            .as_ref()
            .and_then(|c| c.vaults.get(&vault_id))
            .map(|s| s.synced_seq)
            .unwrap_or(0);
        let since = transport
            .heads_since(token, vault_id, synced)
            .map_err(|e| e.to_string())?;
        let vault = self.vault_by_id(vault_id)?;
        Ok(since
            .heads
            .iter()
            .filter_map(|b| ManifestHead::from_bytes(b).ok())
            .any(|h| vault.key_for(h.signer).is_none()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lockdown_vault::Timestamp;
    use lockdown_vault::sharing::{Party, Right, VaultKind};

    fn event(change: Change, by: Id) -> MembershipEvent {
        MembershipEvent {
            v: 1,
            vault_id: Id::new(),
            seq: 1,
            prev_hash: lockdown_vault::Hash32::ZERO,
            change,
            key_ref: Id::new(),
            by_account: by,
            by_device: Id::new(),
            at: Timestamp(0),
            sig: Vec::new(),
        }
    }

    fn party(account_id: Id) -> Party {
        Party {
            account_id,
            profile_id: Id::new(),
            persona_id: Id::new(),
        }
    }

    #[test]
    fn an_event_names_who_made_it_and_whom_it_is_about() {
        let (owner, carol) = (Id::new(), Id::new());
        let create = event(
            Change::Create {
                owner: party(owner),
                kind: VaultKind::Shared,
            },
            owner,
        );
        assert_eq!(named(&create), vec![owner]);
        let add = event(
            Change::Add {
                member: party(carol),
                right: Right::Edit,
            },
            owner,
        );
        assert_eq!(named(&add), vec![owner, carol]);
        let remove = event(Change::Remove { account_id: carol }, owner);
        assert_eq!(named(&remove), vec![owner, carol]);
    }

    #[test]
    fn the_words_say_who_waits_and_until_when() {
        assert!(HELD.starts_with("New members wait"));
        assert!(HELD.contains("matches its mirrors again"));
        assert!(HELD_LINE.contains(HELD));
    }
}
