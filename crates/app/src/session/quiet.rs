//! What doesn't verify is held back, quietly (docs/plan/49-shared-vault-quiet.md).
//!
//! A record another person made that fails its check (a shared vault's
//! head, membership change or vault key grant; an item shared on its own;
//! an emergency grant) no longer stops the sync or the vault. It is held
//! back: never taken, shown, opened or acted on. The vault opens at the
//! newest state that checks out, and the person sees one calm line that
//! needs nothing from them. Every sync fetches the same records again and
//! checks them afresh; once they verify they are taken and the line goes.
//!
//! - **Heads** chain hash to hash, so a head that fails holds back every
//!   head after it (`lockdown_sync::sync_vault_holding`). The heads before
//!   it are taken, as far as the server still serves their items.
//! - **Membership changes** chain too: the changes up to the one that
//!   fails are taken; it and every later one wait.
//! - **A vault key grant** that fails leaves the vault where it was: not
//!   joined yet, or at the key generation it had.
//! - **While anything in a shared vault is held back**, this device sends
//!   nothing to it. Changes made here wait on this device, as offline ones
//!   do, and go out once the vault's records verify. Who is in the vault
//!   can't be changed from this device meanwhile: such a change would have
//!   to follow the one that didn't verify (the read-only words below).
//! - **Items shared on their own and emergency grants** that fail keep
//!   whatever this device already held of them.
//! - **This identity's own records** fail as they always did: a refusal
//!   that names the check. Only records other people made are held back.
//!
//! The state is per session, like the split's (D426): worked out again by
//! every sync. The technical detail (which record, which check) goes to
//! the Status screen for support ([`Session::held_back_details`]); the
//! line names the person, never a device or a key.

use std::collections::{BTreeMap, BTreeSet};

use lockdown_vault::Id;

use super::Session;

/// What was held back.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Kind {
    /// A shared vault's head (and every head after it).
    Head,
    /// A shared vault's membership change (and every change after it).
    Membership,
    /// A vault key grant: the vault isn't joined, or stays at its key.
    Grant,
    /// An item shared on its own.
    ItemShare,
    /// An emergency grant, or a key it released.
    Emergency,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Head => "head",
            Kind::Membership => "membership change",
            Kind::Grant => "vault key grant",
            Kind::ItemShare => "item share",
            Kind::Emergency => "emergency grant",
        }
    }
}

/// One record held back, with the detail support needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Held {
    pub(crate) kind: Kind,
    /// The shared vault it belongs to, for heads, changes and grants.
    pub(crate) vault: Option<Id>,
    /// Whose record it is by its own word: an account this device pinned,
    /// or `None` when it names nobody this device knows.
    pub(crate) who: Option<Id>,
    /// Which record, in words ("manifest 7", "membership change 4").
    pub(crate) record: String,
    /// Which check failed, in the check's own words.
    pub(crate) why: String,
}

/// Everything held back since the last sync worked it out.
#[derive(Clone, Debug, Default)]
pub(crate) struct Unverified {
    held: Vec<Held>,
}

impl Unverified {
    pub(crate) fn push(&mut self, held: Held) {
        if !self.held.contains(&held) {
            self.held.push(held);
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.held.is_empty()
    }

    /// Whether anything of `vault` is held back.
    pub(crate) fn vault_held(&self, vault: Id) -> bool {
        self.held.iter().any(|h| h.vault == Some(vault))
    }

    /// Whether anything of `vault` other than its heads is held back: then
    /// this device sends nothing to it.
    pub(crate) fn blocks_push(&self, vault: Id) -> bool {
        self.held
            .iter()
            .any(|h| h.vault == Some(vault) && h.kind != Kind::Head)
    }

    /// Forgets what was held back of one record, before it is checked again.
    pub(crate) fn forget(&mut self, kind: Kind, record: &str) {
        self.held
            .retain(|h| !(h.kind == kind && h.record == record));
    }

    /// Whether the record of `kind` that `record` names was held back.
    pub(crate) fn has(&self, kind: Kind, record: &str) -> bool {
        self.held
            .iter()
            .any(|h| h.kind == kind && h.record == record)
    }

    fn of_vault(&self, vault: Id) -> impl Iterator<Item = &Held> {
        self.held.iter().filter(move |h| h.vault == Some(vault))
    }

    /// The vaults with something held back, in id order.
    fn vaults(&self) -> BTreeSet<Id> {
        self.held.iter().filter_map(|h| h.vault).collect()
    }
}

/// Names joined the way a sentence says them: "Bob", "Bob and Carol",
/// "Bob, Carol and Dan".
pub(crate) fn joined(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

fn from(names: &[String]) -> String {
    if names.is_empty() {
        String::new()
    } else {
        format!(" from {}", joined(names))
    }
}

/// The part that ends every line: nothing to do.
pub(crate) const NOTHING_TO_DO: &str = "You don't need to do anything.";

/// The line on a shared vault with something held back.
pub fn vault_line(names: &[String]) -> String {
    format!(
        "Some recent changes{} couldn't be verified, so Lockdown left them out. {NOTHING_TO_DO}",
        from(names)
    )
}

/// The same, where this person could change who is in the vault: that
/// waits too (the read-only variant).
pub(crate) fn read_only_line(names: &[String]) -> String {
    format!(
        "Some recent changes{} couldn't be verified, so Lockdown left them out. Until they can be, people can't be added or removed here. {NOTHING_TO_DO}",
        from(names)
    )
}

/// The line for a shared vault this device couldn't join yet.
pub(crate) fn join_line(names: &[String]) -> String {
    format!(
        "A shared vault{} couldn't be verified, so Lockdown left it out for now. {NOTHING_TO_DO}",
        from(names)
    )
}

/// The line for items shared on their own.
pub(crate) fn item_line(names: &[String], count: usize) -> String {
    if count == 1 {
        format!(
            "A shared item{} couldn't be verified, so Lockdown left it out. {NOTHING_TO_DO}",
            from(names)
        )
    } else {
        format!(
            "Some shared items{} couldn't be verified, so Lockdown left them out. {NOTHING_TO_DO}",
            from(names)
        )
    }
}

/// The line for emergency access.
pub(crate) fn emergency_line(names: &[String]) -> String {
    format!(
        "Emergency access{} couldn't be verified, so Lockdown left it out for now. {NOTHING_TO_DO}",
        from(names)
    )
}

/// What a shared vault that can't open yet says: no vault of the persona
/// could open at unlock because nothing this device holds vouches for the
/// people who wrote to it yet (a new device before its first sync). It
/// opens by itself once the sync meets them (§6).
pub(crate) const WAITING: &str = "This shared vault opens by itself once Lockdown has checked who changed it, at the next sync. You don't need to do anything.";

/// What such a vault is called until it opens (its name is sealed inside it).
pub(crate) const WAITING_NAME: &str = "Shared vault";

/// The session's first vault while no vault of the persona could open: an
/// empty vault in memory, under the persona's own keys and a fresh id. It
/// holds nothing, is never saved, synced, listed or written to (the
/// session refuses each), and the first vault that opens takes its place.
/// Nothing unverified is in it, and no vault is made in the waiting one's
/// place.
pub(crate) fn stand_in(
    identity: &ecosystem_identity::Identity,
    persona_id: Id,
    keys: lockdown_vault::identity::PersonaKeys,
    keystore: &std::rc::Rc<crate::local_unlock::DeviceKeys>,
) -> Result<lockdown_vault::identity::OpenVault, String> {
    let ctx = lockdown_vault::VaultContext {
        profile_id: identity.profile().profile_id,
        persona_id,
        vault_id: Id::new(),
        app_data_key_id: keys.app_data_key_id,
        crown_jewel_key_id: keys.crown_jewel_key_id,
    };
    let mut vault =
        lockdown_vault::Vault::create(ctx, identity.device_id(), Box::new(keystore.clone()), None)
            .map_err(|e| e.to_string())?;
    vault.commit().map_err(|e| e.to_string())?;
    Ok(lockdown_vault::identity::OpenVault {
        vault,
        keys,
        created: false,
    })
}

impl Session {
    /// Records something held back.
    pub(crate) fn hold_unverified(&mut self, held: Held) {
        self.unverified.push(held);
    }

    /// The account a record names as its maker, kept only when it is
    /// someone this device pinned (never this identity: its own records
    /// are refused, not held back).
    pub(crate) fn known_other(&self, account: Id) -> Option<Id> {
        (Some(account) != self.sharing.account_id() && self.sharing.contact(account).is_some())
            .then_some(account)
    }

    /// The names of the people behind `held`, sorted, each once.
    fn names<'a>(&self, held: impl Iterator<Item = &'a Held>) -> Vec<String> {
        let who: BTreeSet<Id> = held.filter_map(|h| h.who).collect();
        let mut names: Vec<String> = who
            .into_iter()
            .filter_map(|a| self.sharing.contact(a).map(|c| c.login.clone()))
            .collect();
        names.sort();
        names.dedup();
        names
    }

    /// The line a vault shows while something of it is held back.
    pub(crate) fn unverified_line(&self, vault: Id) -> Option<String> {
        if !self.unverified.vault_held(vault) {
            return None;
        }
        Some(vault_line(&self.names(self.unverified.of_vault(vault))))
    }

    /// The same where the person could change who is in the vault.
    pub(crate) fn unverified_read_only_line(&self, vault: Id) -> Option<String> {
        if !self.unverified.vault_held(vault) {
            return None;
        }
        Some(read_only_line(&self.names(self.unverified.of_vault(vault))))
    }

    /// Refuses a change to who is in `vault` while something of it is held
    /// back, with the read-only words.
    pub(crate) fn refuse_while_unverified(&self, vault: Id) -> Result<(), String> {
        match self.unverified_read_only_line(vault) {
            Some(line) => Err(line),
            None => Ok(()),
        }
    }

    fn vault_name(&self, vault: Id) -> Option<String> {
        self.vault_by_id(vault)
            .ok()
            .and_then(|v| v.info().ok())
            .and_then(|i| i.name)
    }

    /// The lines a sync adds: one per vault (named, when this device has
    /// it open), one for items, one for emergency access.
    pub(crate) fn unverified_lines(&self) -> Vec<String> {
        let mut out = Vec::new();
        for vault in self.unverified.vaults() {
            let held: Vec<&Held> = self.unverified.of_vault(vault).collect();
            let names = self.names(held.iter().copied());
            let open = self.is_open(vault);
            if !open {
                out.push(join_line(&names));
                continue;
            }
            let line = vault_line(&names);
            out.push(match self.vault_name(vault) {
                Some(name) => format!("In {name}: {line}"),
                None => line,
            });
        }
        let items: Vec<&Held> = self
            .unverified
            .held
            .iter()
            .filter(|h| h.kind == Kind::ItemShare)
            .collect();
        if !items.is_empty() {
            out.push(item_line(&self.names(items.iter().copied()), items.len()));
        }
        let emergency: Vec<&Held> = self
            .unverified
            .held
            .iter()
            .filter(|h| h.kind == Kind::Emergency)
            .collect();
        if !emergency.is_empty() {
            out.push(emergency_line(&self.names(emergency.into_iter())));
        }
        out.dedup();
        out
    }

    /// For support, on the Status screen: one line per record held back,
    /// saying which record and which check. Never shown as the line.
    pub fn held_back_details(&self) -> Vec<String> {
        let mut by: BTreeMap<(Kind, String), Vec<String>> = BTreeMap::new();
        for h in &self.unverified.held {
            let place = match h.vault {
                Some(v) => self
                    .vault_name(v)
                    .unwrap_or_else(|| format!("shared vault {v}")),
                None => "shared with you".into(),
            };
            let who = h
                .who
                .and_then(|a| self.sharing.contact(a).map(|c| c.login.clone()))
                .unwrap_or_else(|| "someone this device hasn't met".into());
            by.entry((h.kind, h.record.clone()))
                .or_default()
                .push(format!(
                    "{place}: {} {} by {who} held back: {}",
                    h.kind.name(),
                    h.record,
                    h.why
                ));
        }
        by.into_values().flatten().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lines_name_people_and_ask_nothing() {
        let bob = ["bob@example.com".to_string()];
        assert_eq!(
            vault_line(&bob),
            "Some recent changes from bob@example.com couldn't be verified, so Lockdown left them out. You don't need to do anything."
        );
        assert_eq!(
            read_only_line(&bob),
            "Some recent changes from bob@example.com couldn't be verified, so Lockdown left them out. Until they can be, people can't be added or removed here. You don't need to do anything."
        );
        assert_eq!(
            vault_line(&[]),
            "Some recent changes couldn't be verified, so Lockdown left them out. You don't need to do anything."
        );
        assert_eq!(
            item_line(&bob, 1),
            "A shared item from bob@example.com couldn't be verified, so Lockdown left it out. You don't need to do anything."
        );
        assert!(item_line(&bob, 2).starts_with("Some shared items from bob@example.com"));
        assert!(join_line(&bob).starts_with("A shared vault from bob@example.com"));
        assert!(emergency_line(&bob).starts_with("Emergency access from bob@example.com"));
        let three = ["a".to_string(), "b".to_string(), "c".to_string()];
        assert_eq!(joined(&three), "a, b and c");
        assert_eq!(joined(&three[..2]), "a and b");
        for line in [
            vault_line(&bob),
            read_only_line(&bob),
            join_line(&bob),
            item_line(&bob, 1),
            emergency_line(&bob),
        ] {
            assert!(line.ends_with(NOTHING_TO_DO), "{line}");
            for word in ["device", "key", "signature", "manifest"] {
                assert!(!line.contains(word), "{line}");
            }
        }
    }

    #[test]
    fn heads_alone_still_let_this_device_pull_other_holds_stop_its_pushes() {
        let vault = Id::new();
        let mut u = Unverified::default();
        let held = |kind| Held {
            kind,
            vault: Some(vault),
            who: None,
            record: "x".into(),
            why: "y".into(),
        };
        u.push(held(Kind::Head));
        u.push(held(Kind::Head));
        assert_eq!(u.held.len(), 1, "each record once");
        assert!(u.vault_held(vault) && !u.blocks_push(vault));
        u.push(held(Kind::Membership));
        assert!(u.blocks_push(vault));
        assert!(!u.vault_held(Id::new()));
    }
}
