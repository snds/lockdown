//! Per-item exposure tiers (`exposure-tiers`; feature matrix "three tiers
//! (never in extension, phone approval, free fill) per item, with
//! high-risk items defaulting to approval"; 02-model §13 q.3, "a middle
//! protection tier for mailbox and identity-provider logins that still
//! allows a fill with a second action"; docs/plan/22-security-depth.md §1).
//!
//! Three tiers, strongest first:
//!
//! - [`Exposure::AppOnly`]: the item never reaches the browser extension.
//!   It is not offered on any page, a fill naming it is refused, and its
//!   passkeys are never offered. In the app it behaves as before.
//! - [`Exposure::Ask`]: every use of a secret (a fill, a copy, a reveal, a
//!   code) asks through the `Use` gate first, as a crown jewel's copy does.
//! - [`Exposure::Fill`]: the everyday tier; nothing is asked beyond what
//!   the item's kind already asks (a card's second deliberate action).
//!
//! The person's choice is a reserved field on the item
//! ([`EXPOSURE_FIELD`], `app-only` | `ask` | `fill`), so it syncs, sits in
//! history and export, and an edit keeps it without naming it (D132's
//! rule for `lockdown.*` fields). With no choice, [`default_for`] decides:
//! a crown jewel and a **high-risk** login ([`high_risk`]) start at `Ask`,
//! everything else at `Fill`.
//!
//! Crown jewels keep every rule they had (02-model §3, D101, D113): their
//! tier is never below `Ask` and can be `AppOnly`; `Fill` is refused for
//! them. A tier only ever adds a check; it never removes one a kind or the
//! crown-jewel class already imposes.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{Error, FieldKind, FieldValue, ItemClass, ItemKind, PlainField, Result};

/// The reserved field holding the person's choice.
pub const EXPOSURE_FIELD: &str = "lockdown.exposure";

/// How far an item's secrets may travel without asking.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Exposure {
    /// Kept to the Lockdown app: never offered to or filled in a browser.
    AppOnly,
    /// Each fill, copy or reveal asks first (the `Use` gate).
    Ask,
    /// Fills and copies freely.
    Fill,
}

impl Exposure {
    /// Every tier, strongest first.
    pub const ALL: [Exposure; 3] = [Self::AppOnly, Self::Ask, Self::Fill];

    /// The word on the wire and in the field.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AppOnly => "app-only",
            Self::Ask => "ask",
            Self::Fill => "fill",
        }
    }

    /// The tier a word names.
    pub fn parse(text: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|t| t.as_str() == text.trim())
            .ok_or_else(|| {
                Error::Invalid(format!(
                    "'{text}' isn't a protection level; use app-only, ask or fill"
                ))
            })
    }

    /// What a person reads, in plain words.
    pub fn describe(self) -> &'static str {
        match self {
            Self::AppOnly => "Only in the Lockdown app: never offered or filled in a browser",
            Self::Ask => "Ask each time: a fill, copy or reveal needs your confirmation first",
            Self::Fill => "Fill freely",
        }
    }

    /// Whether the browser extension may see the item at all.
    pub fn reaches_browser(self) -> bool {
        self != Self::AppOnly
    }

    /// Whether every use of a secret asks first.
    pub fn asks(self) -> bool {
        self != Self::Fill
    }

    /// Whether `self` protects at least as much as `other`.
    pub fn at_least(self, other: Exposure) -> bool {
        self <= other
    }
}

/// Why an item is at the tier it is.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExposureSource {
    /// The person chose it.
    Chosen,
    /// A crown jewel starts at `Ask`.
    CrownJewel,
    /// A high-risk login starts at `Ask` ([`high_risk`] says why).
    HighRisk,
    /// The everyday default.
    Default,
}

/// What the default rule needs to know beyond the item itself.
#[derive(Clone, Debug, Default)]
pub struct ExposureContext {
    /// The account's own login name (the mailbox Lockdown recovers through).
    pub account_login: Option<String>,
    /// Every "Signed in with" name the persona's logins give, lowercased:
    /// the accounts other sign-ins depend on.
    pub providers: BTreeSet<String>,
}

impl ExposureContext {
    /// Collects the "Signed in with" names from every login's fields.
    pub fn with_providers<'a>(
        mut self,
        logins: impl IntoIterator<Item = &'a [PlainField]>,
    ) -> Self {
        for fields in logins {
            if let Some(p) = text(fields, "sign_in_provider") {
                let p = p.trim().to_lowercase();
                if !p.is_empty() {
                    self.providers.insert(p);
                }
            }
        }
        self
    }
}

/// Mail and identity providers whose login opens other accounts: a reset
/// link goes to the mailbox, a "Sign in with" button to the provider.
/// Matched as the host or a parent domain of a login's website.
pub const IDENTITY_PROVIDERS: &[&str] = &[
    "accounts.google.com",
    "google.com",
    "gmail.com",
    "appleid.apple.com",
    "apple.com",
    "icloud.com",
    "login.microsoftonline.com",
    "microsoft.com",
    "live.com",
    "outlook.com",
    "yahoo.com",
    "aol.com",
    "proton.me",
    "protonmail.com",
    "fastmail.com",
    "zoho.com",
    "gmx.com",
    "yandex.com",
    "okta.com",
];

fn text(fields: &[PlainField], field_id: &str) -> Option<String> {
    fields.iter().find_map(|f| match &f.value {
        FieldValue::Text(t) if f.field_id == field_id => Some(t.clone()),
        _ => None,
    })
}

/// The hosts of a login's websites, lowercased, without scheme, port or path.
fn hosts(fields: &[PlainField]) -> Vec<String> {
    fields
        .iter()
        .filter(|f| f.kind == FieldKind::Url || f.field_id.starts_with("url."))
        .filter_map(|f| match &f.value {
            FieldValue::Text(t) => Some(t.clone()),
            _ => None,
        })
        .filter_map(|t| {
            let t = t.trim();
            let rest = t.split_once("://").map_or(t, |(_, r)| r);
            let host = rest
                .split(['/', '?', '#'])
                .next()?
                .rsplit('@')
                .next()?
                .split(':')
                .next()?
                .trim_end_matches('.')
                .to_lowercase();
            (!host.is_empty()).then_some(host)
        })
        .collect()
}

fn under(host: &str, domain: &str) -> bool {
    host == domain || host.ends_with(&format!(".{domain}"))
}

/// Why a login is high-risk, in plain words, or `None`: its website is a
/// mail or identity provider, other logins sign in through it, or its
/// username is the account's own login name. Only logins are judged; every
/// other kind keeps the rules its kind has.
pub fn high_risk(kind: ItemKind, fields: &[PlainField], ctx: &ExposureContext) -> Option<String> {
    if kind != ItemKind::Login {
        return None;
    }
    let hosts = hosts(fields);
    if let Some(domain) = IDENTITY_PROVIDERS
        .iter()
        .find(|d| hosts.iter().any(|h| under(h, d)))
    {
        return Some(format!(
            "{domain} is a mail or sign-in provider: whoever holds this login can reset or open other accounts"
        ));
    }
    let title = text(fields, "title").unwrap_or_default().to_lowercase();
    let first_labels: Vec<String> = hosts
        .iter()
        .map(|h| {
            let parts: Vec<&str> = h.split('.').collect();
            if parts.len() >= 2 {
                parts[parts.len() - 2].to_string()
            } else {
                h.clone()
            }
        })
        .collect();
    if let Some(p) = ctx.providers.iter().find(|p| {
        (!title.is_empty() && title == **p)
            || first_labels.iter().any(|l| l == *p)
            || hosts.iter().any(|h| h == *p)
    }) {
        return Some(format!(
            "other logins sign in through {p}, so this one opens them too"
        ));
    }
    if let (Some(login), Some(user)) = (&ctx.account_login, text(fields, "user"))
        && !login.trim().is_empty()
        && user.trim().eq_ignore_ascii_case(login.trim())
    {
        return Some(
            "this is the mailbox your Lockdown account signs in as, the one a recovery goes through"
                .into(),
        );
    }
    None
}

/// The person's choice, if the item carries one.
pub fn chosen(fields: &[PlainField]) -> Option<Exposure> {
    text(fields, EXPOSURE_FIELD).and_then(|t| Exposure::parse(&t).ok())
}

/// The tier with no choice made: `Ask` for a crown jewel or a high-risk
/// login, `Fill` otherwise.
pub fn default_for(
    kind: ItemKind,
    class: ItemClass,
    fields: &[PlainField],
    ctx: &ExposureContext,
) -> (Exposure, ExposureSource) {
    if class == ItemClass::CrownJewel {
        return (Exposure::Ask, ExposureSource::CrownJewel);
    }
    if high_risk(kind, fields, ctx).is_some() {
        return (Exposure::Ask, ExposureSource::HighRisk);
    }
    (Exposure::Fill, ExposureSource::Default)
}

/// The tier in force: the choice when there is one (never weaker than
/// `Ask` for a crown jewel, whatever a hand-edited field says), else the
/// default.
pub fn effective(
    kind: ItemKind,
    class: ItemClass,
    fields: &[PlainField],
    ctx: &ExposureContext,
) -> (Exposure, ExposureSource) {
    match chosen(fields) {
        Some(Exposure::Fill) if class == ItemClass::CrownJewel => {
            (Exposure::Ask, ExposureSource::CrownJewel)
        }
        Some(t) => (t, ExposureSource::Chosen),
        None => default_for(kind, class, fields, ctx),
    }
}

/// Checks a choice against the item: `Fill` is refused for a crown jewel,
/// and a kind the browser never fills (an SSH key, a bank account) takes
/// no choice at all.
pub fn check_choice(kind: ItemKind, class: ItemClass, choice: Exposure) -> Result<()> {
    if !kind.autofill_allowed() {
        return Err(Error::Invalid(format!(
            "a {} never reaches the browser, so it has no protection level to choose",
            kind.as_str()
        )));
    }
    if class == ItemClass::CrownJewel && choice == Exposure::Fill {
        return Err(Error::Invalid(
            "a crown jewel always asks first; take it out of the crown-jewel class to let it fill freely"
                .into(),
        ));
    }
    Ok(())
}

/// `fields` with the choice set (or cleared with `None`), for a new version.
pub fn with_choice(mut fields: Vec<PlainField>, choice: Option<Exposure>) -> Vec<PlainField> {
    fields.retain(|f| f.field_id != EXPOSURE_FIELD);
    if let Some(t) = choice {
        fields.push(PlainField::new(
            EXPOSURE_FIELD,
            FieldKind::Text,
            "Protection level",
            FieldValue::Text(t.as_str().to_string()),
        ));
    }
    fields
}

impl crate::Vault {
    /// Sets (or with `None` clears) an item's protection level as a new
    /// version, so it syncs; the current version when nothing changes.
    /// The gate in front of a weakening is the app's.
    pub fn set_exposure(&mut self, item_id: crate::Id, choice: Option<Exposure>) -> Result<u64> {
        let item = self.item(item_id)?;
        if let Some(c) = choice {
            check_choice(item.kind, item.class, c)?;
        }
        if !matches!(
            item.state,
            crate::ItemState::Live | crate::ItemState::Archived
        ) {
            return Err(Error::Invalid(
                "this item is in the trash; restore it before changing it".into(),
            ));
        }
        let current = item.latest()?.n;
        let fields = self.open_item(item_id)?;
        if chosen(&fields) == choice {
            return Ok(current);
        }
        self.update_item(item_id, with_choice(fields, choice))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn login(title: &str, user: &str, url: &str) -> Vec<PlainField> {
        vec![
            PlainField::new(
                "title",
                FieldKind::Text,
                "Title",
                FieldValue::Text(title.into()),
            ),
            PlainField::new(
                "user",
                FieldKind::Text,
                "Username",
                FieldValue::Text(user.into()),
            ),
            PlainField::new(
                "url.0",
                FieldKind::Url,
                "Website",
                FieldValue::Text(url.into()),
            ),
        ]
    }

    #[test]
    fn mail_and_identity_providers_default_to_asking() {
        let ctx = ExposureContext::default();
        let gmail = login(
            "Google",
            "me@gmail.com",
            "https://accounts.google.com/signin",
        );
        assert!(high_risk(ItemKind::Login, &gmail, &ctx).is_some());
        assert_eq!(
            default_for(ItemKind::Login, ItemClass::Standard, &gmail, &ctx),
            (Exposure::Ask, ExposureSource::HighRisk)
        );
        // A lookalike that only ends in the name is not the provider.
        let fake = login("Not Google", "me", "https://evilgoogle.com/");
        assert!(high_risk(ItemKind::Login, &fake, &ctx).is_none());
        let shop = login("Shop", "me", "https://shop.example/");
        assert_eq!(
            default_for(ItemKind::Login, ItemClass::Standard, &shop, &ctx),
            (Exposure::Fill, ExposureSource::Default)
        );
        // Only logins are judged.
        assert!(high_risk(ItemKind::Card, &gmail, &ctx).is_none());
    }

    #[test]
    fn a_login_others_sign_in_through_and_the_accounts_own_mailbox_are_high_risk() {
        let mut airbnb = login("Airbnb", "me", "https://airbnb.com");
        airbnb.push(PlainField::new(
            "sign_in_provider",
            FieldKind::Text,
            "Signed in with",
            FieldValue::Text("Acme SSO".into()),
        ));
        let ctx = ExposureContext {
            account_login: Some("sean@example.org".into()),
            ..Default::default()
        }
        .with_providers([airbnb.as_slice()]);
        let sso = login("Acme SSO", "sean", "https://login.acme.example");
        let why = high_risk(ItemKind::Login, &sso, &ctx).unwrap();
        assert!(why.contains("acme sso"), "{why}");
        let mailbox = login("Mail", "Sean@Example.org", "https://mail.example.org");
        assert!(
            high_risk(ItemKind::Login, &mailbox, &ctx)
                .unwrap()
                .contains("mailbox")
        );
        assert!(high_risk(ItemKind::Login, &airbnb, &ctx).is_none());
    }

    #[test]
    fn a_choice_wins_but_never_weakens_a_crown_jewel() {
        let ctx = ExposureContext::default();
        let shop = login("Shop", "me", "https://shop.example/");
        let app_only = with_choice(shop.clone(), Some(Exposure::AppOnly));
        assert_eq!(chosen(&app_only), Some(Exposure::AppOnly));
        assert_eq!(
            effective(ItemKind::Login, ItemClass::Standard, &app_only, &ctx),
            (Exposure::AppOnly, ExposureSource::Chosen)
        );
        let cleared = with_choice(app_only, None);
        assert_eq!(chosen(&cleared), None);
        // A crown jewel asks by default, can be kept to the app, never fills freely.
        assert_eq!(
            effective(ItemKind::Login, ItemClass::CrownJewel, &shop, &ctx).0,
            Exposure::Ask
        );
        let forced = with_choice(shop.clone(), Some(Exposure::Fill));
        assert_eq!(
            effective(ItemKind::Login, ItemClass::CrownJewel, &forced, &ctx),
            (Exposure::Ask, ExposureSource::CrownJewel)
        );
        assert!(check_choice(ItemKind::Login, ItemClass::CrownJewel, Exposure::Fill).is_err());
        check_choice(ItemKind::Login, ItemClass::CrownJewel, Exposure::AppOnly).unwrap();
        assert!(check_choice(ItemKind::SshKey, ItemClass::Standard, Exposure::Ask).is_err());
        // Order: app-only protects most.
        assert!(
            Exposure::AppOnly.at_least(Exposure::Ask) && !Exposure::Fill.at_least(Exposure::Ask)
        );
        assert!(!Exposure::AppOnly.reaches_browser() && Exposure::Ask.reaches_browser());
        assert_eq!(Exposure::parse(" ask ").unwrap(), Exposure::Ask);
        assert!(Exposure::parse("sometimes").is_err());
    }
}
