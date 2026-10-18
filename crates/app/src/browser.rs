//! What the browser extension may ask of the core, through the desktop
//! app's local endpoint (docs/plan/16-browser-extension.md). The core
//! decides; the extension measures and renders.
//!
//! - **Which logins fit a page** ([`App::page_matches`]): every live login's
//!   every saved address through the matcher under the built-in signed data,
//!   strongest decision first. A crown jewel shows by title only (D113).
//! - **May this fill happen, and with what** ([`App::fill`]): the item's
//!   addresses decided against the frame again, `fill_check` on the page's
//!   measured facts, then the values. Nothing is read from the vault until
//!   the verdict is `fill`, and a crown jewel's values go through the `Use`
//!   gate as a copy does (D101).
//! - **Save or update after a sign-in** ([`App::save_proposal`],
//!   [`App::save_login`]): the password the person typed is compared in the
//!   core and never sent back.
//!
//! Two rules sit beside `fill_check` here, because they need the saved
//! address's scheme, which the matcher's decision does not carry: a login
//! saved for `https` is never filled into a plain `http` frame, and cards
//! and identities fill only into a secure frame (`https`, or the machine
//! itself).

use std::time::Instant;

use lockdown_matcher::{
    FillContext, FillKind, FillVerdict, MatchDecision, MatchRule, MatcherData, Origin, SavedUrl,
    decide_with, fill_check,
};
use lockdown_vault::{FieldKind, FieldValue, Id, ItemClass, ItemKind, ItemState, PlainField};
use serde::{Deserialize, Serialize};

use crate::session::Session;
use crate::{App, AppError, AppResult, ApprovalKind, Core, GateInput, Revealed};

/// The most logins a page's match list carries.
pub const MAX_CANDIDATES: usize = 20;

/// What the extension measured about the field and where it sits. `top`
/// comes from the browser's own record of the tab, not from the page.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageFacts {
    /// The top-level page's address.
    pub top: String,
    /// The address of the frame that holds the field.
    pub frame: String,
    /// The field and every ancestor are visible, on screen and not covered
    /// (D67).
    pub field_visible: bool,
    /// The person already took a second deliberate action for this fill.
    #[serde(default)]
    pub user_confirmed: bool,
}

/// What is being filled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FillWhat {
    /// A login's username and password.
    Login,
    /// A login's one-time code.
    Code,
    /// A payment card.
    Card,
    /// Name, email, phone and address.
    Identity,
}

impl FillWhat {
    fn fill_kind(self) -> FillKind {
        match self {
            Self::Login => FillKind::Login,
            Self::Code => FillKind::Code,
            Self::Card => FillKind::Card,
            Self::Identity => FillKind::Identity,
        }
    }

    fn item_kind(self) -> ItemKind {
        match self {
            Self::Login | Self::Code => ItemKind::Login,
            Self::Card => ItemKind::Card,
            Self::Identity => ItemKind::Identity,
        }
    }
}

/// One login that fits the page, as the extension's menu and popup show it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LoginCandidate {
    pub item_id: String,
    pub title: String,
    /// Empty for a crown jewel (D113).
    pub username: String,
    /// The saved address that matched, so the extension can run the same
    /// decision in its own matcher; `None` for a crown jewel.
    pub saved: Option<String>,
    /// That address's rule (`exact_host`, `base_domain`, …).
    pub rule: Option<String>,
    /// `exact`, `equivalent` or `base_domain`.
    pub decision: MatchDecision,
    pub crown_jewel: bool,
    /// Whether it holds a password to fill.
    pub password: bool,
    /// The field id of its first one-time code, when it holds one.
    pub code_field: Option<String>,
    /// Seconds since the Unix epoch.
    pub edited_at: i64,
}

/// Which logins fit a page, and what the page imitates.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PageMatches {
    /// The page's host as the matcher read it.
    pub host: String,
    /// Logins whose saved address matches, strongest decision first, then
    /// the most recently edited.
    pub logins: Vec<LoginCandidate>,
    /// Saved hosts this page looks like but is not (a hard stop). A crown
    /// jewel's host is never named; it counts as "a protected login".
    pub lookalike_of: Vec<String>,
    /// `https`, or the machine itself.
    pub secure: bool,
}

/// A card or an identity, for the extension's form menu.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct WalletEntry {
    pub item_id: String,
    /// `card` or `identity`.
    pub kind: String,
    pub title: String,
    /// The card's holder and brand, or the identity's email; empty for a
    /// crown jewel.
    pub subtitle: String,
    pub crown_jewel: bool,
}

/// One value to put into one field: `role` is the template's field id
/// (`user`, `pw`, `code`, `card.number`, `name.given`, …).
pub struct FillValue {
    pub role: String,
    pub value: Revealed,
}

impl std::fmt::Debug for FillValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "FillValue({}, {} bytes)", self.role, self.value.len())
    }
}

/// The core's answer to a fill.
#[derive(Debug)]
pub enum FillOutcome {
    /// Fill these values now.
    Fill(Vec<FillValue>),
    /// Ask the person first; the words say why.
    Confirm(String),
    /// Do not fill; the words say why.
    Refuse(String),
}

/// What a sign-in the person just made would do to the vault.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SaveProposal {
    /// No login for this site and username: offer to save one.
    New { title: String },
    /// A login exists with another password: offer to update it.
    Update { item_id: String, title: String },
    /// The vault already has exactly this: offer nothing.
    Same { item_id: String, title: String },
}

/// The signed matcher data built into this build, verified once.
pub(crate) fn matcher_data() -> AppResult<MatcherData> {
    MatcherData::builtin()
        .map_err(|e| AppError::refused(format!("the matcher's data didn't verify: {e}")))
}

fn rule_named(name: &str) -> MatchRule {
    match name {
        "base_domain" => MatchRule::BaseDomain,
        "equivalent" => MatchRule::Equivalent,
        "starts_with" => MatchRule::StartsWith,
        "never" => MatchRule::Never,
        _ => MatchRule::ExactHost,
    }
}

/// Lower is stronger.
fn strength(d: MatchDecision) -> u8 {
    match d {
        MatchDecision::Exact => 0,
        MatchDecision::Equivalent => 1,
        MatchDecision::BaseDomain => 2,
        MatchDecision::Lookalike => 3,
        MatchDecision::NoMatch => 4,
    }
}

pub(crate) fn is_match(d: MatchDecision) -> bool {
    strength(d) <= 2
}

/// Whether an address is `https`, or names this machine.
pub fn is_secure(origin: &Origin) -> bool {
    origin.scheme == "https"
        || matches!(
            origin.host.as_str(),
            "localhost" | "127.0.0.1" | "[::1]" | "::1"
        )
        || origin.host.ends_with(".localhost")
}

fn parse_page(text: &str) -> AppResult<Origin> {
    Origin::parse(text).map_err(|e| AppError::refused(e.to_string()))
}

/// The saved addresses of an item with their rules.
pub(crate) fn saved_urls(fields: &[PlainField]) -> Vec<(String, MatchRule)> {
    fields
        .iter()
        .filter(|f| f.kind == FieldKind::Url)
        .filter_map(|f| match &f.value {
            FieldValue::Url { value, match_rule } => Some((value.clone(), rule_named(match_rule))),
            _ => None,
        })
        .collect()
}

/// The strongest decision any of `urls` gives for `page`, with the address
/// that gave it.
pub(crate) fn best_decision(
    urls: &[(String, MatchRule)],
    page: &Origin,
    data: &MatcherData,
) -> (MatchDecision, Option<(String, MatchRule, Origin)>) {
    let mut best = (MatchDecision::NoMatch, None);
    for (text, rule) in urls {
        let Ok(saved) = SavedUrl::parse(text, *rule) else {
            continue;
        };
        let d = decide_with(&saved, page, data);
        if strength(d) < strength(best.0) {
            best = (d, Some((text.clone(), *rule, saved.origin)));
        }
    }
    best
}

/// Every live login's saved address, crown jewels left out (their sites
/// are never named, D113), for the security center's lookalike rule.
fn saved_sites(session: &Session) -> AppResult<Vec<String>> {
    let mut out = Vec::new();
    for (vault, item) in every_item(session) {
        if item.kind != ItemKind::Login
            || item.state != ItemState::Live
            || item.class == ItemClass::CrownJewel
        {
            continue;
        }
        let fields = vault
            .open_item(item.item_id)
            .map_err(|e| AppError::refused(e.to_string()))?;
        out.extend(saved_urls(&fields).into_iter().map(|(u, _)| u));
    }
    Ok(out)
}

/// Every item of every vault the persona has open (several may be, D151's
/// slice), with the vault that holds it.
fn every_item(
    session: &Session,
) -> impl Iterator<Item = (&lockdown_vault::Vault, &lockdown_vault::Item)> {
    session
        .vaults()
        .flat_map(|v| v.items().map(move |i| (v, i)))
}

fn rule_name(rule: MatchRule) -> &'static str {
    match rule {
        MatchRule::ExactHost => "exact_host",
        MatchRule::BaseDomain => "base_domain",
        MatchRule::Equivalent => "equivalent",
        MatchRule::StartsWith => "starts_with",
        MatchRule::Never => "never",
    }
}

pub(crate) fn text(fields: &[PlainField], field_id: &str) -> Option<String> {
    fields
        .iter()
        .find(|f| f.field_id == field_id)
        .and_then(|f| match &f.value {
            FieldValue::Text(t) if !t.is_empty() => Some(t.clone()),
            _ => None,
        })
}

fn has_field(fields: &[PlainField], field_id: &str) -> bool {
    fields.iter().any(|f| f.field_id == field_id)
}

fn first_code(fields: &[PlainField]) -> Option<String> {
    fields
        .iter()
        .find(|f| f.kind == FieldKind::Totp)
        .map(|f| f.field_id.clone())
}

fn parse_item_id(id: &str) -> AppResult<Id> {
    Id::parse(id).map_err(|_| AppError::refused(format!("'{id}' isn't an item id")))
}

/// The plain fields an identity fills, in the template's order.
const IDENTITY_FIELDS: [&str; 9] = [
    "name.given",
    "name.middle",
    "name.family",
    "identity.email",
    "address.tel",
    "address.street",
    "address.city",
    "address.territory",
    "address.postal_code",
];

impl App {
    /// Which live logins fit the page at `page`, and which saved hosts it
    /// imitates. Nothing secret is in the answer.
    pub fn page_matches(&self, page: &str) -> AppResult<PageMatches> {
        let page = page.to_string();
        self.call(move |core| {
            core.clock.touch(Instant::now());
            let current = parse_page(&page)?;
            let data = matcher_data()?;
            let session = core.session()?;
            let mut logins = Vec::new();
            let mut lookalike_of = Vec::new();
            for (vault, item) in every_item(session) {
                if item.kind != ItemKind::Login || item.state != ItemState::Live {
                    continue;
                }
                let fields = vault
                    .open_item(item.item_id)
                    .map_err(|e| AppError::refused(e.to_string()))?;
                let crown_jewel = item.class == ItemClass::CrownJewel;
                let (decision, matched) = best_decision(&saved_urls(&fields), &current, &data);
                if decision == MatchDecision::Lookalike {
                    let name = match (&matched, crown_jewel) {
                        (Some((_, _, o)), false) => o.host.clone(),
                        _ => "a protected login".to_string(),
                    };
                    if !lookalike_of.contains(&name) {
                        lookalike_of.push(name);
                    }
                    continue;
                }
                if !is_match(decision) {
                    continue;
                }
                let edited_at = item
                    .latest()
                    .map(|v| v.edited_at.0)
                    .map_err(|e| AppError::refused(e.to_string()))?;
                let (saved, rule) = match (&matched, crown_jewel) {
                    (Some((t, r, _)), false) => (Some(t.clone()), Some(rule_name(*r).to_string())),
                    _ => (None, None),
                };
                logins.push(LoginCandidate {
                    item_id: item.item_id.to_string(),
                    title: text(&fields, "title").unwrap_or_default(),
                    username: if crown_jewel {
                        String::new()
                    } else {
                        text(&fields, "user").unwrap_or_default()
                    },
                    saved,
                    rule,
                    decision,
                    crown_jewel,
                    password: has_field(&fields, "pw"),
                    code_field: first_code(&fields),
                    edited_at,
                });
            }
            // Beyond your own logins: a page that imitates a well-known site,
            // by the security center's own rule (lockdown_app::security).
            if logins.is_empty() && lookalike_of.is_empty() {
                let saved = saved_sites(session)?;
                if let Some(domain) = crate::security::imitated_site(&page, &saved) {
                    lookalike_of.push(domain);
                }
            }
            logins.sort_by(|a, b| {
                strength(a.decision)
                    .cmp(&strength(b.decision))
                    .then(b.edited_at.cmp(&a.edited_at))
                    .then(a.title.cmp(&b.title))
            });
            logins.truncate(MAX_CANDIDATES);
            Ok(PageMatches {
                host: current.host.clone(),
                logins,
                lookalike_of,
                secure: is_secure(&current),
            })
        })
    }

    /// Every live card and identity, for a checkout or address form's menu.
    pub fn wallet(&self) -> AppResult<Vec<WalletEntry>> {
        self.with_session(|s| {
            let mut out = Vec::new();
            for (vault, item) in every_item(s) {
                let kind = match item.kind {
                    ItemKind::Card => "card",
                    ItemKind::Identity => "identity",
                    _ => continue,
                };
                if item.state != ItemState::Live {
                    continue;
                }
                let fields = vault.open_item(item.item_id).map_err(|e| e.to_string())?;
                let crown_jewel = item.class == ItemClass::CrownJewel;
                let subtitle = if crown_jewel {
                    String::new()
                } else if item.kind == ItemKind::Card {
                    [text(&fields, "card.holder"), text(&fields, "card.type")]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join(" · ")
                } else {
                    text(&fields, "identity.email").unwrap_or_default()
                };
                out.push(WalletEntry {
                    item_id: item.item_id.to_string(),
                    kind: kind.into(),
                    title: text(&fields, "title").unwrap_or_default(),
                    subtitle,
                    crown_jewel,
                });
            }
            out.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.title.cmp(&b.title)));
            Ok(out)
        })
    }

    /// Decides one fill and, when the verdict is `fill`, reads the values.
    /// `field` names which code to fill for [`FillWhat::Code`] (the item's
    /// first when absent). The item's addresses are decided against
    /// `facts.frame` here, whatever the extension's own matcher said.
    pub fn fill(
        &self,
        item_id: &str,
        what: FillWhat,
        field: Option<&str>,
        facts: PageFacts,
    ) -> AppResult<FillOutcome> {
        let (item_id, field) = (item_id.to_string(), field.map(str::to_string));
        self.call(move |core| {
            core.clock.touch(Instant::now());
            match decide_fill(core, &item_id, what, field.as_deref(), &facts)? {
                Ok(plan) => {
                    let values = read_values(core, &item_id, plan)?;
                    // A password this core filled: the site may upgrade that
                    // login to a passkey for a while (passkeys.rs).
                    if what == FillWhat::Login && values.iter().any(|v| v.role == "pw") {
                        core.note_password_fill(&item_id, &facts.frame);
                    }
                    Ok(FillOutcome::Fill(values))
                }
                Err(outcome) => Ok(outcome),
            }
        })
    }

    /// What saving the sign-in the person just made at `page` would do:
    /// new, update, or nothing. The password is compared here and goes no
    /// further.
    pub fn save_proposal(
        &self,
        page: &str,
        username: &str,
        password: &[u8],
    ) -> AppResult<SaveProposal> {
        let (page, username) = (page.to_string(), username.to_string());
        let password = zeroize::Zeroizing::new(password.to_vec());
        self.call(move |core| {
            core.clock.touch(Instant::now());
            let current = parse_page(&page)?;
            let data = matcher_data()?;
            let session = core.session()?;
            for (vault, item) in every_item(session) {
                if item.kind != ItemKind::Login || item.state != ItemState::Live {
                    continue;
                }
                let fields = vault
                    .open_item(item.item_id)
                    .map_err(|e| AppError::refused(e.to_string()))?;
                let (decision, _) = best_decision(&saved_urls(&fields), &current, &data);
                if !matches!(decision, MatchDecision::Exact | MatchDecision::Equivalent) {
                    continue;
                }
                let user = text(&fields, "user").unwrap_or_default();
                if !user.eq_ignore_ascii_case(&username) {
                    continue;
                }
                let title = text(&fields, "title").unwrap_or_default();
                let item_id = item.item_id.to_string();
                let same = fields.iter().any(|f| {
                    f.field_id == "pw"
                        && matches!(&f.value, FieldValue::Text(t) if t.as_bytes() == password.as_slice())
                });
                return Ok(if same {
                    SaveProposal::Same { item_id, title }
                } else {
                    SaveProposal::Update { item_id, title }
                });
            }
            Ok(SaveProposal::New {
                title: current.host.clone(),
            })
        })
    }

    /// Saves the sign-in: a new login for `page`'s origin, or, with
    /// `update`, a new version of that login with the new password (its
    /// `Use` gate first when it is a crown jewel). Returns the item id.
    pub fn save_login(
        &self,
        page: &str,
        username: &str,
        password: Vec<u8>,
        update: Option<&str>,
    ) -> AppResult<String> {
        let current = parse_page(page)?;
        let site = match current.port {
            Some(port) => format!("{}://{}:{port}", current.scheme, current.host),
            None => format!("{}://{}", current.scheme, current.host),
        };
        match update {
            Some(id) => {
                self.edit_login(id, None, None, Some(password), GateInput::default())?;
                Ok(id.to_string())
            }
            None => self.add_login(&site, username, password, None),
        }
    }
}

/// What to read once the verdict is `fill`.
struct FillPlan {
    what: FillWhat,
    code_field: Option<String>,
}

/// Every check before a value is read. `Ok(Err(outcome))` is a verdict
/// other than `fill`.
fn decide_fill(
    core: &mut Core,
    item_id: &str,
    what: FillWhat,
    field: Option<&str>,
    facts: &PageFacts,
) -> AppResult<Result<FillPlan, FillOutcome>> {
    let top = parse_page(&facts.top)?;
    let frame = parse_page(&facts.frame)?;
    let data = matcher_data()?;
    let id = parse_item_id(item_id)?;
    let session = core.session()?;
    let vault = session.holding(id).map_err(AppError::refused)?;
    let item = vault
        .item(id)
        .map_err(|e| AppError::refused(e.to_string()))?;
    if item.state != ItemState::Live {
        return Ok(Err(FillOutcome::Refuse(
            "this item is in the trash or archived".into(),
        )));
    }
    if item.kind != what.item_kind() || !item.kind.autofill_allowed() {
        return Ok(Err(FillOutcome::Refuse(format!(
            "a {} isn't filled as a {}",
            item.kind.as_str(),
            match what {
                FillWhat::Login => "login",
                FillWhat::Code => "code",
                FillWhat::Card => "card",
                FillWhat::Identity => "identity",
            }
        ))));
    }
    let fields = vault
        .open_item(id)
        .map_err(|e| AppError::refused(e.to_string()))?;
    let decision = match what {
        FillWhat::Login | FillWhat::Code => {
            let (decision, matched) = best_decision(&saved_urls(&fields), &frame, &data);
            if let Some((_, _, saved)) = &matched
                && is_match(decision)
                && saved.scheme == "https"
                && frame.scheme != "https"
                && !is_secure(&frame)
            {
                return Ok(Err(FillOutcome::Refuse(format!(
                    "this login was saved for a secure (https) site and {} isn't secure; Lockdown won't fill here",
                    frame.host
                ))));
            }
            decision
        }
        FillWhat::Card | FillWhat::Identity => {
            if let Some(domain) =
                crate::security::imitated_site(&facts.frame, &saved_sites(session)?)
            {
                return Ok(Err(FillOutcome::Refuse(format!(
                    "{} looks like {domain} but isn't; Lockdown won't fill here",
                    frame.host
                ))));
            }
            if !is_secure(&frame) {
                return Ok(Err(FillOutcome::Refuse(format!(
                    "{} isn't a secure (https) page; Lockdown fills cards and addresses only on secure pages",
                    frame.host
                ))));
            }
            // A card or identity is not tied to a site: the person chose it
            // from the menu, and the frame and visibility rules still apply.
            MatchDecision::Exact
        }
    };
    let ctx = FillContext {
        top,
        frame,
        field_visible: facts.field_visible,
        user_confirmed: facts.user_confirmed,
        kind: what.fill_kind(),
    };
    match fill_check(&ctx, decision) {
        FillVerdict::Fill => {}
        FillVerdict::Confirm(r) => return Ok(Err(FillOutcome::Confirm(r))),
        FillVerdict::Refuse(r) => return Ok(Err(FillOutcome::Refuse(r))),
    }
    let code_field = match what {
        FillWhat::Code => {
            let wanted = field.map(str::to_string).or_else(|| first_code(&fields));
            match wanted {
                Some(f)
                    if fields
                        .iter()
                        .any(|x| x.field_id == f && x.kind == FieldKind::Totp) =>
                {
                    Some(f)
                }
                _ => {
                    return Ok(Err(FillOutcome::Refuse(
                        "this login has no one-time code".into(),
                    )));
                }
            }
        }
        _ => None,
    };
    Ok(Ok(FillPlan { what, code_field }))
}

/// Reads what a decided fill puts into the page. Secrets come through the
/// same gated paths a copy uses.
fn read_values(core: &mut Core, item_id: &str, plan: FillPlan) -> AppResult<Vec<FillValue>> {
    let id = parse_item_id(item_id)?;
    let fields = core
        .session()?
        .holding(id)
        .map_err(AppError::refused)?
        .open_item(id)
        .map_err(|e| AppError::refused(e.to_string()))?;
    let gate = GateInput::default();
    let mut out = Vec::new();
    let plain = |out: &mut Vec<FillValue>, role: &str| {
        if let Some(t) = text(&fields, role) {
            out.push(FillValue {
                role: role.into(),
                value: Revealed::from_text(&t),
            });
        }
    };
    match plan.what {
        FillWhat::Login => {
            plain(&mut out, "user");
            if has_field(&fields, "pw") {
                let value = core.reveal_for_fill(item_id, "pw", ApprovalKind::Use, &gate)?;
                out.push(FillValue {
                    role: "pw".into(),
                    value,
                });
            }
        }
        FillWhat::Code => {
            let field = plan.code_field.as_deref().unwrap_or("code.0");
            let value = core.code_for_fill(item_id, field, false, &gate)?;
            out.push(FillValue {
                role: "code".into(),
                value,
            });
        }
        FillWhat::Card => {
            for role in ["card.number", "card.cvc"] {
                if has_field(&fields, role) {
                    let value = core.reveal_for_fill(item_id, role, ApprovalKind::Use, &gate)?;
                    out.push(FillValue {
                        role: role.into(),
                        value,
                    });
                }
            }
            plain(&mut out, "card.holder");
            plain(&mut out, "card.expiry");
            plain(&mut out, "card.type");
        }
        FillWhat::Identity => {
            for role in IDENTITY_FIELDS {
                plain(&mut out, role);
            }
            plain(&mut out, "address.country");
            // A fill is a use (D151); the secret-bearing kinds above note it
            // through the reveal and code paths, an identity has none.
            core.note_use(item_id);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FieldChange, LockPolicy, Paths};

    fn fresh() -> (tempfile::TempDir, App) {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::resolve(Some(dir.path().join("snds/identity")), None).unwrap();
        let app = App::start(paths, None, LockPolicy::default());
        app.open_or_create(true).unwrap();
        (dir, app)
    }

    fn facts(top: &str, frame: &str) -> PageFacts {
        PageFacts {
            top: top.into(),
            frame: frame.into(),
            field_visible: true,
            user_confirmed: false,
        }
    }

    fn values(outcome: FillOutcome) -> Vec<(String, String)> {
        match outcome {
            FillOutcome::Fill(v) => v
                .into_iter()
                .map(|f| {
                    (
                        f.role.clone(),
                        String::from_utf8(f.value.expose().to_vec()).unwrap(),
                    )
                })
                .collect(),
            other => panic!("expected a fill, got {other:?}"),
        }
    }

    #[test]
    fn a_page_lists_its_logins_and_names_what_it_imitates() {
        let (_dir, app) = fresh();
        let paypal = app
            .add_login(
                "https://www.paypal.com/signin",
                "sean",
                b"pw-1".to_vec(),
                None,
            )
            .unwrap();
        app.add_login("https://example.com", "other", b"pw-2".to_vec(), None)
            .unwrap();
        let m = app
            .page_matches("https://www.paypal.com/myaccount")
            .unwrap();
        assert_eq!(m.host, "www.paypal.com");
        assert!(m.secure);
        assert_eq!(m.logins.len(), 1);
        let c = &m.logins[0];
        assert_eq!(c.item_id, paypal);
        assert_eq!(c.username, "sean");
        assert_eq!(c.decision, MatchDecision::Exact);
        assert_eq!(c.saved.as_deref(), Some("https://www.paypal.com/signin"));
        assert_eq!(c.rule.as_deref(), Some("exact_host"));
        assert!(c.password && c.code_field.is_none());
        let evil = app.page_matches("https://paypal.com.evil.net/").unwrap();
        assert!(evil.logins.is_empty());
        assert_eq!(evil.lookalike_of, vec!["www.paypal.com".to_string()]);
        // A well-known site you have no login for: the security center's rule.
        let fake = app.page_matches("https://githuh.com/login").unwrap();
        assert!(fake.logins.is_empty());
        assert_eq!(fake.lookalike_of, vec!["github.com".to_string()]);
        let none = app.page_matches("http://unrelated.test/").unwrap();
        assert!(none.logins.is_empty() && none.lookalike_of.is_empty());
        assert!(!none.secure);
    }

    #[test]
    fn a_fill_is_decided_again_in_the_core_before_anything_is_read() {
        let (_dir, app) = fresh();
        let id = app
            .add_login("https://example.com", "me", b"s3cret".to_vec(), None)
            .unwrap();
        let page = "https://example.com/login";
        let got = values(
            app.fill(&id, FillWhat::Login, None, facts(page, page))
                .unwrap(),
        );
        assert_eq!(
            got,
            vec![("user".into(), "me".into()), ("pw".into(), "s3cret".into())]
        );
        // Another site: the extension's word that it matched counts for nothing.
        let other = "https://example.org/login";
        assert!(matches!(
            app.fill(&id, FillWhat::Login, None, facts(other, other)).unwrap(),
            FillOutcome::Refuse(r) if r.contains("no saved login")
        ));
        // Covered: refused even when confirmed.
        let mut hidden = facts(page, page);
        hidden.field_visible = false;
        hidden.user_confirmed = true;
        assert!(matches!(
            app.fill(&id, FillWhat::Login, None, hidden).unwrap(),
            FillOutcome::Refuse(r) if r.contains("clickjacking")
        ));
        // In a frame from the same site inside another site's page: ask.
        let framed = facts("https://news.test/article", page);
        assert!(matches!(
            app.fill(&id, FillWhat::Login, None, framed.clone()).unwrap(),
            FillOutcome::Confirm(r) if r.contains("inside a page from news.test")
        ));
        let confirmed = PageFacts {
            user_confirmed: true,
            ..framed
        };
        assert_eq!(
            values(app.fill(&id, FillWhat::Login, None, confirmed).unwrap()).len(),
            2
        );
        // An https login never goes into a plain http frame.
        let plain = "http://example.com/login";
        assert!(matches!(
            app.fill(&id, FillWhat::Login, None, facts(plain, plain)).unwrap(),
            FillOutcome::Refuse(r) if r.contains("isn't secure")
        ));
        // A lookalike is a hard stop.
        let evil = "https://examp1e.com/login";
        assert!(matches!(
            app.fill(&id, FillWhat::Login, None, facts(evil, evil))
                .unwrap(),
            FillOutcome::Refuse(_)
        ));
    }

    #[test]
    fn a_code_needs_the_second_action_and_comes_from_the_core() {
        let (_dir, app) = fresh();
        let id = app
            .add_login("https://example.com", "me", b"pw".to_vec(), None)
            .unwrap();
        app.add_code(
            "otpauth://totp/Example:me?secret=JBSWY3DPEHPK3PXP&issuer=Example",
            None,
            Some(&id),
        )
        .unwrap();
        let page = "https://example.com/2fa";
        let m = app.page_matches(page).unwrap();
        let field = m.logins[0].code_field.clone().unwrap();
        assert!(matches!(
            app.fill(&id, FillWhat::Code, Some(&field), facts(page, page)).unwrap(),
            FillOutcome::Confirm(r) if r.contains("deliberate")
        ));
        let confirmed = PageFacts {
            user_confirmed: true,
            ..facts(page, page)
        };
        let got = values(
            app.fill(&id, FillWhat::Code, Some(&field), confirmed)
                .unwrap(),
        );
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].0, "code");
        assert_eq!(got[0].1.len(), 6);
        assert!(got[0].1.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn cards_ask_first_and_fill_only_on_secure_pages() {
        let (_dir, app) = fresh();
        let card = app
            .add_item(
                "card",
                Some("Everyday card"),
                false,
                vec![
                    FieldChange::secret("card.number", b"4242424242424242"),
                    FieldChange::text("card.holder", "Sean Sands"),
                    FieldChange::text("card.expiry", "2030-08"),
                    FieldChange::secret("card.cvc", b"123"),
                ],
            )
            .unwrap();
        let wallet = app.wallet().unwrap();
        assert_eq!(wallet.len(), 1);
        assert_eq!(wallet[0].kind, "card");
        assert_eq!(wallet[0].subtitle, "Sean Sands · Visa");
        let page = "https://shop.test/checkout";
        assert!(matches!(
            app.fill(&card, FillWhat::Card, None, facts(page, page))
                .unwrap(),
            FillOutcome::Confirm(_)
        ));
        let fake = "https://paypa1.com/checkout";
        assert!(matches!(
            app.fill(&card, FillWhat::Card, None, PageFacts { user_confirmed: true, ..facts(fake, fake) }).unwrap(),
            FillOutcome::Refuse(r) if r.contains("looks like paypal.com")
        ));
        let insecure = PageFacts {
            user_confirmed: true,
            ..facts("http://shop.test/", "http://shop.test/")
        };
        assert!(matches!(
            app.fill(&card, FillWhat::Card, None, insecure).unwrap(),
            FillOutcome::Refuse(r) if r.contains("secure")
        ));
        let confirmed = PageFacts {
            user_confirmed: true,
            ..facts(page, page)
        };
        let got = values(app.fill(&card, FillWhat::Card, None, confirmed).unwrap());
        let roles: Vec<&str> = got.iter().map(|(r, _)| r.as_str()).collect();
        assert_eq!(
            roles,
            [
                "card.number",
                "card.cvc",
                "card.holder",
                "card.expiry",
                "card.type"
            ]
        );
        // A card is not a login.
        assert!(matches!(
            app.fill(&card, FillWhat::Login, None, facts(page, page))
                .unwrap(),
            FillOutcome::Refuse(_)
        ));
    }

    #[test]
    fn an_identity_fills_its_plain_fields() {
        let (_dir, app) = fresh();
        let id = app
            .add_item(
                "identity",
                Some("Home"),
                false,
                vec![
                    FieldChange::text("name.given", "Sean"),
                    FieldChange::text("name.family", "Sands"),
                    FieldChange::text("identity.email", "sean@example.com"),
                    FieldChange::text("address.city", "Springfield"),
                ],
            )
            .unwrap();
        let page = "https://shop.test/address";
        let got = values(
            app.fill(&id, FillWhat::Identity, None, facts(page, page))
                .unwrap(),
        );
        assert_eq!(
            got,
            vec![
                ("name.given".into(), "Sean".into()),
                ("name.family".into(), "Sands".into()),
                ("identity.email".into(), "sean@example.com".into()),
                ("address.city".into(), "Springfield".into()),
            ]
        );
    }

    #[test]
    fn a_sign_in_proposes_save_update_or_nothing() {
        let (_dir, app) = fresh();
        let page = "https://example.com/login";
        assert_eq!(
            app.save_proposal(page, "me", b"first").unwrap(),
            SaveProposal::New {
                title: "example.com".into()
            }
        );
        let id = app.save_login(page, "me", b"first".to_vec(), None).unwrap();
        let rows = app.list().unwrap();
        assert_eq!(rows[0].site, "https://example.com");
        assert!(matches!(
            app.save_proposal(page, "ME", b"first").unwrap(),
            SaveProposal::Same { item_id, .. } if item_id == id
        ));
        let SaveProposal::Update { item_id, .. } =
            app.save_proposal(page, "me", b"second").unwrap()
        else {
            panic!("expected an update");
        };
        assert_eq!(item_id, id);
        app.save_login(page, "me", b"second".to_vec(), Some(&id))
            .unwrap();
        assert!(matches!(
            app.save_proposal(page, "me", b"second").unwrap(),
            SaveProposal::Same { .. }
        ));
        // Another username on the same site is a new login.
        assert!(matches!(
            app.save_proposal(page, "you", b"second").unwrap(),
            SaveProposal::New { .. }
        ));
    }

    #[test]
    fn every_website_of_every_open_vault_is_considered_and_a_fill_counts_as_a_use() {
        let (_dir, app) = fresh();
        let work = app.create_vault("Work").unwrap();
        let id = app
            .add_item_in(
                &work,
                "login",
                Some("Intranet"),
                false,
                vec![
                    FieldChange::text("url.0", "https://intranet.example.com"),
                    FieldChange::text("url.1", "https://sso.example.org"),
                    FieldChange::text("user", "me"),
                    FieldChange::secret("pw", b"work-pass"),
                ],
            )
            .unwrap();
        let m = app.page_matches("https://sso.example.org/login").unwrap();
        assert_eq!(m.logins.len(), 1);
        assert_eq!(m.logins[0].item_id, id);
        assert_eq!(
            m.logins[0].saved.as_deref(),
            Some("https://sso.example.org")
        );
        let page = "https://sso.example.org/login";
        assert_eq!(
            values(
                app.fill(&id, FillWhat::Login, None, facts(page, page))
                    .unwrap()
            )[1]
            .1,
            "work-pass"
        );
        let row = app
            .list()
            .unwrap()
            .into_iter()
            .find(|r| r.id == id)
            .unwrap();
        assert_eq!(row.uses, 1);
        assert!(row.last_used.is_some());
    }

    #[test]
    fn locked_answers_locked() {
        let (_dir, app) = fresh();
        app.lock().unwrap();
        assert_eq!(
            app.page_matches("https://example.com").unwrap_err().code,
            crate::ErrorCode::Locked
        );
    }
}
