//! The crown-jewel class: its rules and which items land there by default
//! (02-model §3; brief §2.9 point 5).
//!
//! An item in this class has its Item Key wrapped by the persona's
//! Crown-Jewel Key instead of the Vault Key. The rules are not configurable
//! downward; the policy crate enforces them as hard rules.

use crate::ItemKind;

/// The seven rules, as (rule, what it means for the person). Shown in the
/// app when an item is classified; enforced in `lockdown-policy`.
pub const CROWN_JEWEL_RULES: &[(&str, &str)] = &[
    (
        "Fresh biometric plus a deliberate tap to use",
        "Signing a confirmation or copying a value needs Face ID, Touch ID, Hello or StrongBox right then, not a cached unlock",
    ),
    (
        "Full account password to reveal or export",
        "A fingerprint is not enough to show the secret on screen",
    ),
    (
        "Never autofilled",
        "The extension and OS providers do not see these items",
    ),
    (
        "Never in search previews",
        "Search shows the title only, never a field",
    ),
    (
        "Never given to agents",
        "Deny by default; override per item with the full password",
    ),
    (
        "Out of bulk export and legacy release",
        "Unless you opt each item in",
    ),
    (
        "\"This device only\" option",
        "The item never syncs; the manifest holds a tombstone that says so",
    ),
];

/// What the classifier says about an item or field.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CrownJewelDefault {
    /// Crown-jewel by default.
    Default,
    /// Prompt once; the person decides.
    Suggested,
    /// Standard unless the person chooses otherwise.
    Never,
}

/// What a login is for, when the app can tell (from the sign-in-with map
/// or the recovery settings). Drives the "suggested" column.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum LoginRole {
    /// An ordinary account.
    #[default]
    Everyday,
    /// The mailbox used for account recovery.
    RecoveryMailbox,
    /// An identity provider other sites sign in through.
    IdentityProvider,
    /// A bank or brokerage.
    Banking,
}

/// What the classifier looks at.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Classify<'a> {
    /// The item's kind.
    pub kind: ItemKind,
    /// The field being judged, if a single field is.
    pub field_id: Option<&'a str>,
    /// For logins, what the account is for.
    pub login_role: LoginRole,
}

/// The default table from 02-model §3.
pub fn crown_jewel_default(c: Classify<'_>) -> CrownJewelDefault {
    use CrownJewelDefault::{Default, Never, Suggested};
    let field = c.field_id.unwrap_or("");
    let is_recovery_codes =
        field.starts_with("recovery_codes") || field.starts_with("backup_codes");
    if is_recovery_codes {
        return Default;
    }
    match c.kind {
        ItemKind::CryptoWallet => {
            if field.is_empty() || field.starts_with("seed") || field.starts_with("recovery_phrase")
            {
                Default
            } else {
                Never
            }
        }
        ItemKind::Document if field.starts_with("recovery_kit") || field.is_empty() => Suggested,
        ItemKind::Login => match c.login_role {
            LoginRole::RecoveryMailbox | LoginRole::IdentityProvider | LoginRole::Banking => {
                Suggested
            }
            LoginRole::Everyday => Never,
        },
        _ => Never,
    }
}

/// A cheap check for a pasted BIP-39 or SLIP-39 phrase: 12, 15, 18, 21, 24
/// (or 20 or 33 for SLIP-39) lowercase words of three to eight letters.
/// The full word lists land with the import code; this is enough to prompt.
pub fn looks_like_seed_phrase(text: &str) -> bool {
    let words: Vec<&str> = text.split_whitespace().collect();
    if !matches!(words.len(), 12 | 15 | 18 | 20 | 21 | 24 | 33) {
        return false;
    }
    words
        .iter()
        .all(|w| (3..=8).contains(&w.len()) && w.chars().all(|c| c.is_ascii_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_table_matches_the_plan() {
        let c = |kind, field_id, login_role| Classify {
            kind,
            field_id,
            login_role,
        };
        assert_eq!(
            crown_jewel_default(c(ItemKind::CryptoWallet, Some("seed"), LoginRole::Everyday)),
            CrownJewelDefault::Default
        );
        assert_eq!(
            crown_jewel_default(c(
                ItemKind::Login,
                Some("recovery_codes"),
                LoginRole::Everyday
            )),
            CrownJewelDefault::Default
        );
        assert_eq!(
            crown_jewel_default(c(ItemKind::Login, None, LoginRole::RecoveryMailbox)),
            CrownJewelDefault::Suggested
        );
        assert_eq!(
            crown_jewel_default(c(ItemKind::Login, None, LoginRole::Banking)),
            CrownJewelDefault::Suggested
        );
        assert_eq!(
            crown_jewel_default(c(ItemKind::Login, None, LoginRole::Everyday)),
            CrownJewelDefault::Never
        );
        for kind in [
            ItemKind::Code,
            ItemKind::WiFi,
            ItemKind::Card,
            ItemKind::Note,
        ] {
            assert_eq!(
                crown_jewel_default(c(kind, None, LoginRole::Everyday)),
                CrownJewelDefault::Never,
                "{kind:?}"
            );
        }
        assert_eq!(CROWN_JEWEL_RULES.len(), 7);
    }

    #[test]
    fn seed_phrases_are_spotted() {
        let twelve =
            "abandon ability able about above absent absorb abstract absurd abuse access accident";
        assert!(looks_like_seed_phrase(twelve));
        assert!(!looks_like_seed_phrase("correct horse battery staple"));
        assert!(!looks_like_seed_phrase(
            &twelve.replace("abandon", "Abandon1")
        ));
    }
}
