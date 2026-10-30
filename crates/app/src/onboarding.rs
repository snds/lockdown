//! What the first run remembers between its screens (`onboarding.cbor`,
//! docs/plan/39-first-run.md §1.2 and §1.13, D478).
//!
//! Nothing secret is here: how far the first run got, the name the lock
//! screen shows, when the kit was made (the kit test waits a day, Q4), and
//! whether this first run created the ecosystem identity root, so a
//! developer reset can tell a root Lockdown made from one another app
//! shares. The file sits in Lockdown's data directory beside
//! `settings.cbor`, so a reset that erases that directory catches it.

use std::path::Path;

use lockdown_store::write_private;
use lockdown_vault::Id;
use serde::{Deserialize, Serialize};

/// The file, in the data directory.
pub const FILE: &str = "onboarding.cbor";
const FILE_V: u16 = 1;
/// The most characters a name may have.
pub const NAME_MAX: usize = 64;
/// How long after the kit is made the kit test waits (Q4: a day or more).
pub const KIT_TEST_AFTER_SECS: i64 = 86_400;

/// How far the first run got.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    /// The account and the master password are made; the kit is shown next.
    AccountMade,
    /// The person said the kit is saved.
    KitSaved,
    /// Touch ID or Windows Hello was offered.
    QuickOffered,
    /// Passwords were brought in, or the step skipped.
    ImportDone,
    /// The first run is over.
    Done,
}

impl Stage {
    /// The word for a shell.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AccountMade => "account_made",
            Self::KitSaved => "kit_saved",
            Self::QuickOffered => "quick_offered",
            Self::ImportDone => "import_done",
            Self::Done => "done",
        }
    }

    /// The stage a shell names, if it is one.
    pub fn parse(word: &str) -> Option<Self> {
        Some(match word {
            "account_made" => Self::AccountMade,
            "kit_saved" => Self::KitSaved,
            "quick_offered" => Self::QuickOffered,
            "import_done" => Self::ImportDone,
            "done" => Self::Done,
            _ => return None,
        })
    }
}

/// The identity root a first run created.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatedIdentity {
    /// Its path.
    pub root: String,
    /// Its profile.
    pub profile_id: Id,
}

/// What the first run remembers.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Onboarding {
    #[serde(default)]
    v: u16,
    /// How far it got; `None` before it began (or on an older install).
    #[serde(default)]
    pub stage: Option<Stage>,
    /// The name the lock screen shows.
    #[serde(default)]
    pub name: Option<String>,
    /// When the first run made the kit (seconds since the epoch).
    #[serde(default)]
    pub kit_made_at: Option<i64>,
    /// The identity root this first run created, if it created one.
    #[serde(default)]
    pub created_identity: Option<CreatedIdentity>,
}

impl Onboarding {
    /// What `data` holds; empty when the file is missing, unreadable or
    /// from a newer build.
    pub fn load(data: &Path) -> Self {
        std::fs::read(data.join(FILE))
            .ok()
            .and_then(|b| lockdown_crypto::cbor::from_slice::<Self>(&b).ok())
            .filter(|o| o.v <= FILE_V)
            .unwrap_or_default()
    }

    /// Writes it to `data`.
    pub fn save(&self, data: &Path) -> Result<(), String> {
        let mut kept = self.clone();
        kept.v = FILE_V;
        crate::paths::make_dir(data)?;
        let bytes = lockdown_crypto::cbor::to_canonical(&kept).map_err(|e| e.to_string())?;
        write_private(&data.join(FILE), &bytes)
            .map_err(|e| format!("couldn't write {}: {e}", data.join(FILE).display()))
    }

    /// Whether the kit test may be asked now: the kit was made a day ago
    /// or more, or before this file knew (an older install).
    pub fn kit_test_due(&self, now: i64) -> bool {
        self.kit_made_at
            .is_none_or(|at| now >= at.saturating_add(KIT_TEST_AFTER_SECS))
    }
}

/// The identity root the first run in `data` created, if it did: what a
/// developer reset reads to decide whether that root is Lockdown's alone
/// (docs/plan/40-devtools.md §2.4).
pub fn created_identity(data: &Path) -> Option<CreatedIdentity> {
    Onboarding::load(data).created_identity
}

/// A name as typed, trimmed, or the words for why it can't be one: 1 to 64
/// characters, no control characters (D463).
pub fn check_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Type a name. It's what the lock screen shows.".into());
    }
    if name.chars().count() > NAME_MAX {
        return Err("Keep the name to 64 characters or fewer.".into());
    }
    if name.chars().any(char::is_control) {
        return Err("A name can't hold tabs, line breaks or other control characters.".into());
    }
    Ok(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_trimmed_and_bounded() {
        assert_eq!(check_name("  Sean ").unwrap(), "Sean");
        assert!(check_name("   ").unwrap_err().starts_with("Type a name"));
        assert!(check_name(&"x".repeat(65)).unwrap_err().contains("64"));
        assert_eq!(check_name(&"é".repeat(64)).unwrap().chars().count(), 64);
        assert!(check_name("a\nb").is_err());
    }

    #[test]
    fn it_round_trips_and_the_kit_test_waits_a_day() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Onboarding::load(dir.path()), Onboarding::default());
        let o = Onboarding {
            stage: Some(Stage::KitSaved),
            name: Some("Sean".into()),
            kit_made_at: Some(1_000),
            created_identity: Some(CreatedIdentity {
                root: "/r".into(),
                profile_id: Id::new(),
            }),
            ..Onboarding::default()
        };
        o.save(dir.path()).unwrap();
        let back = Onboarding::load(dir.path());
        assert_eq!(back.stage, Some(Stage::KitSaved));
        assert_eq!(back.name.as_deref(), Some("Sean"));
        assert_eq!(created_identity(dir.path()), o.created_identity);
        assert!(!back.kit_test_due(1_000 + 3_600));
        assert!(back.kit_test_due(1_000 + KIT_TEST_AFTER_SECS));
        assert!(Onboarding::default().kit_test_due(0));
        for s in [
            Stage::AccountMade,
            Stage::KitSaved,
            Stage::QuickOffered,
            Stage::ImportDone,
            Stage::Done,
        ] {
            assert_eq!(Stage::parse(s.as_str()), Some(s));
        }
    }
}
