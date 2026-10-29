//! Sharing paused on a split view (D328 as changed by D375;
//! docs/plan/34-small-calls.md §3).
//!
//! While the account server's public log disagrees with a mirror (a split
//! view, D327), the server is showing this device one history and the
//! world another, which is the moment it could swap someone's keys. The
//! vault keeps opening and syncing (D328 stands for that), but every new
//! step that trusts the server for someone's keys waits until the split
//! clears: a contact looked up and pinned (a phrase check pins too), a vault or an item shared, a
//! share or request link made, an emergency contact named, and emergency
//! access opened after its wait. The rule is here, in the core, so every
//! shell refuses the same steps with the same sentence; a shell only says
//! so ahead of time.
//!
//! The split is read from what the last sync's mirror check kept for the
//! account server (`mirrors`), so it outlasts a restart, and it clears
//! only when a later check no longer finds it: the mirror agrees again, or
//! the person removes a mirror they added. A mirror that was split and is
//! unreachable at the next check stays a split (`mirrors::check`).

use super::{Session, mirrors};

/// A step paused while the server's log is split.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Paused {
    /// Looking someone up and pinning their keys.
    Lookup,
    /// A phrase check, which pins the other person's keys.
    PhraseCheck,
    /// Sharing a vault, or adding someone to a household.
    ShareVault,
    /// Sharing one item on its own.
    ShareItem,
    /// Making a share link.
    ShareLink,
    /// Making a request link.
    RequestLink,
    /// Naming an emergency contact.
    EmergencyContact,
    /// Opening emergency access after its wait (the keys released to you).
    EmergencyRelease,
}

impl Paused {
    /// The step, as the sentence's subject.
    pub(crate) fn words(self) -> &'static str {
        match self {
            Self::Lookup => "Looking someone up",
            Self::PhraseCheck => "Checking someone's keys with a phrase",
            Self::ShareVault => "Sharing a vault",
            Self::ShareItem => "Sharing an item",
            Self::ShareLink => "Making a share link",
            Self::RequestLink => "Making a request link",
            Self::EmergencyContact => "Naming an emergency contact",
            Self::EmergencyRelease => "Opening emergency access",
        }
    }
}

/// The refusal for `step` during a split, in words.
pub(crate) fn refusal(step: Paused) -> String {
    format!(
        "{} is paused: this server's public log doesn't match a mirror, and a server showing two histories could swap someone's keys. Your vault still opens and syncs; this works again once the split clears.",
        step.words()
    )
}

/// What the Sharing screen says while a split pauses it.
pub(crate) const BANNER: &str = "Sharing is paused: this server's public log doesn't match a mirror, and a server showing two histories could swap someone's keys. Until it clears, Lockdown won't look anyone up or check their keys with a phrase, share a vault or an item, make share or request links, name an emergency contact or open emergency access. Your vault still opens and syncs.";

impl Session {
    /// Whether the last mirror check for the account server found a split.
    pub(crate) fn split_view(&self) -> bool {
        self.account_server().is_some_and(|server| {
            mirrors::last(&self.paths.data, &server)
                .iter()
                .any(|c| c.verdict.is_split())
        })
    }

    /// Refuses `step` while the account server's log is split.
    pub(crate) fn refuse_while_split(&self, step: Paused) -> Result<(), String> {
        if self.split_view() {
            Err(refusal(step))
        } else {
            Ok(())
        }
    }

    /// The Sharing screen's banner while a split pauses it.
    pub(crate) fn sharing_paused(&self) -> Option<String> {
        self.split_view().then(|| BANNER.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_step_says_what_is_paused_and_why() {
        for step in [
            Paused::Lookup,
            Paused::PhraseCheck,
            Paused::ShareVault,
            Paused::ShareItem,
            Paused::ShareLink,
            Paused::RequestLink,
            Paused::EmergencyContact,
            Paused::EmergencyRelease,
        ] {
            let words = refusal(step);
            assert!(words.starts_with(step.words()), "{words}");
            assert!(words.contains("is paused"), "{words}");
            assert!(words.contains("doesn't match a mirror"), "{words}");
            assert!(words.contains("still opens and syncs"), "{words}");
        }
    }
}
