//! Vault depth (docs/plan/21-vault-depth.md): the session's half of the
//! template variants and the person's own templates (D231, D232), fields
//! that link to another item's field (D235), the sites' password rules
//! (D236), the Wi-Fi guest card (D236), sign-ins logged on someone else's
//! screen (D237), the card's security-code re-prompt (D238) and files
//! attached from disk a chunk at a time (D233).
//!
//! Nothing here hands a secret out on its own: a linked field is resolved
//! to its target and the caller (`Core`) runs the target's gates; the
//! Wi-Fi card's QR leaves only through `App::wifi_qr` behind the `Reveal`
//! gate; a re-prompt is decided here and asked by the caller.

use std::collections::BTreeMap;

use lockdown_crypto::generator::{Recipe, SiteRule};
use lockdown_vault::{
    Change, FieldChange, FieldInput, FieldKind, FieldValue, Id, ItemClass, ItemKind, ItemState,
    PlainField, UserField, UserTemplate, parse_link, template_of,
};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use super::{Session, is_secret_kind, parse_id, text_of};
use crate::Revealed;
use crate::types::{
    AwayReminder, AwayView, LinkFieldView, LinkTargetView, LinkView, SiteRuleView, UserFieldView,
    UserTemplateInput, UserTemplateView,
};

/// The reserved field holding the sign-ins logged elsewhere (D237).
pub(crate) const AWAY_FIELD: &str = "lockdown.away";
const AWAY_LABEL: &str = "Signed in elsewhere";
/// The most sign-ins one item keeps logged.
pub(crate) const MAX_AWAY: usize = 20;
/// A Login's own password rules (D236).
pub(crate) const RULE_FIELD: &str = "pw.rule";
/// The warning a row carries while a sign-in elsewhere is due.
pub(crate) const AWAY_WARNING: &str = "signed-in-elsewhere";

/// One sign-in on someone else's screen.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct AwayEntry {
    pub place: String,
    pub at: i64,
    pub remind_at: i64,
}

/// The sign-ins an item has logged, oldest first; a malformed record reads
/// as none (it is Lockdown's own field, and a newer build may extend it).
pub(crate) fn away_of(fields: &[PlainField]) -> Vec<AwayEntry> {
    text_of(fields, AWAY_FIELD)
        .and_then(|t| serde_json::from_str::<Vec<AwayEntry>>(&t).ok())
        .unwrap_or_default()
}

/// The views of an item's sign-ins elsewhere, at `now`.
pub(crate) fn away_views(fields: &[PlainField], now: i64) -> Vec<AwayView> {
    away_of(fields)
        .into_iter()
        .map(|a| AwayView {
            due: a.remind_at <= now,
            place: a.place,
            at: a.at,
            remind_at: a.remind_at,
        })
        .collect()
}

/// The template id an item is drawn from (`ItemSummary::template`).
pub(crate) fn template_id(kind: ItemKind, fields: &[PlainField]) -> String {
    if let Some(v) = lockdown_vault::variant_of(fields)
        && v.starts_with(lockdown_vault::user_template::USER_PREFIX)
        && kind == ItemKind::Custom
    {
        return v.to_string();
    }
    template_of(kind, fields).id()
}

/// Whether the card holding `fields` asks before its security code is used.
pub(crate) fn reprompts(kind: ItemKind, fields: &[PlainField]) -> bool {
    kind == ItemKind::Card
        && fields
            .iter()
            .any(|f| f.field_id == "card.reprompt" && f.value == FieldValue::Boolean(true))
}

#[derive(Deserialize)]
struct PublishedRule {
    #[serde(rename = "password-rules")]
    rules: String,
}

/// The sites' published password rules (crates/app/data, Apple's
/// password-manager-resources, MIT), by domain.
fn published() -> &'static BTreeMap<String, String> {
    static RULES: std::sync::OnceLock<BTreeMap<String, String>> = std::sync::OnceLock::new();
    RULES.get_or_init(|| {
        serde_json::from_str::<BTreeMap<String, PublishedRule>>(include_str!(
            "../../data/password-rules.json"
        ))
        .map(|m| m.into_iter().map(|(k, v)| (k, v.rules)).collect())
        .unwrap_or_default()
    })
}

/// The published rule for `host` or the nearest domain above it
/// (`accounts.example.com` → `example.com`), with the domain it matched.
pub(crate) fn published_rule(host: &str) -> Option<(String, String)> {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    let mut at = host.as_str();
    loop {
        if let Some(rule) = published().get(at) {
            return Some((at.to_string(), rule.clone()));
        }
        let (_, rest) = at.split_once('.')?;
        if !rest.contains('.') {
            return None;
        }
        at = rest;
    }
}

/// The host of a web address, if it parses.
fn host_of(url: &str) -> Option<String> {
    let url = url.trim();
    let url = if url.contains("://") {
        url.to_string()
    } else {
        format!("https://{url}")
    };
    lockdown_matcher::Origin::parse(&url).ok().map(|o| o.host)
}

/// The rule that applies to a Login's new password: its own (`pw.rule`),
/// else the published rule for its first website's site.
pub(crate) fn site_rule_of(
    rule_text: Option<&str>,
    url: Option<&str>,
) -> Option<(SiteRule, SiteRuleView)> {
    let host = url.and_then(host_of).unwrap_or_default();
    if let Some(text) = rule_text.map(str::trim).filter(|t| !t.is_empty())
        && let Ok(rule) = SiteRule::parse(text)
    {
        let view = SiteRuleView {
            host,
            rule: text.to_string(),
            words: rule.describe(),
            source: "saved".into(),
        };
        return Some((rule, view));
    }
    let (matched, text) = published_rule(&host)?;
    let rule = SiteRule::parse(&text).ok()?;
    Some((
        rule.clone(),
        SiteRuleView {
            host: matched,
            rule: text,
            words: rule.describe(),
            source: "published".into(),
        },
    ))
}

fn set_text<'a>(changes: &'a [FieldChange], field_id: &str) -> Option<&'a str> {
    changes
        .iter()
        .find(|c| c.field_id.as_deref() == Some(field_id))
        .and_then(|c| match &c.change {
            Change::Set(FieldInput::Text(t)) => Some(t.as_str()),
            Change::Set(FieldInput::Url { value, .. }) => Some(value.as_str()),
            _ => None,
        })
}

fn existing_text(fields: &[PlainField], field_id: &str) -> Option<String> {
    fields
        .iter()
        .find(|f| f.field_id == field_id)
        .and_then(|f| match &f.value {
            FieldValue::Text(t) => Some(t.clone()),
            FieldValue::Url { value, .. } => Some(value.clone()),
            _ => None,
        })
}

/// A Login's new password under its site's rules (D236): a `generate` on
/// `pw` takes the rule the edit sets, else the one the item keeps, else
/// the site's published one; a rule the edit sets must read.
pub(crate) fn apply_site_rules(
    kind: ItemKind,
    existing: &[PlainField],
    changes: &mut [FieldChange],
) -> Result<(), String> {
    if kind != ItemKind::Login {
        return Ok(());
    }
    if let Some(text) = set_text(changes, RULE_FIELD).filter(|t| !t.trim().is_empty()) {
        SiteRule::parse(text).map_err(|e| e.to_string())?;
    }
    let removed = changes
        .iter()
        .any(|c| c.field_id.as_deref() == Some(RULE_FIELD) && matches!(c.change, Change::Remove));
    let rule_text = match set_text(changes, RULE_FIELD) {
        Some(t) => Some(t.to_string()),
        None if removed => None,
        None => existing_text(existing, RULE_FIELD),
    };
    let url = set_text(changes, "url.0")
        .map(str::to_string)
        .or_else(|| existing_text(existing, "url.0"));
    let Some((rule, _)) = site_rule_of(rule_text.as_deref(), url.as_deref()) else {
        return Ok(());
    };
    for c in changes.iter_mut() {
        if c.field_id.as_deref() != Some("pw") {
            continue;
        }
        c.change = match std::mem::replace(&mut c.change, Change::Keep) {
            Change::Generate => Change::GenerateWith(
                Recipe::from(lockdown_crypto::generator::PasswordRule::STRONG),
                Some(rule.clone()),
            ),
            Change::GenerateWith(recipe, None) => Change::GenerateWith(recipe, Some(rule.clone())),
            other => other,
        };
    }
    Ok(())
}

/// The kind of field a person's template names by its storage name.
fn user_kind(name: &str) -> Result<FieldKind, String> {
    super::items::field_kind_named(name)
}

fn to_user_template(input: &UserTemplateInput) -> Result<UserTemplate, String> {
    let mut fields = Vec::with_capacity(input.fields.len());
    for f in &input.fields {
        fields.push(UserField {
            key: f.key.clone(),
            label: f.label.clone(),
            kind: user_kind(&f.kind)?,
            required: f.required,
            hint: f.hint.clone(),
            section: f.section.clone(),
        });
    }
    UserTemplate {
        name: input.name.clone(),
        summary: input.summary.clone(),
        icon: input.icon.clone(),
        fields,
    }
    .checked()
    .map_err(|e| e.to_string())
}

fn user_view(id: Id, vault_id: Id, versions: u64, t: &UserTemplate) -> UserTemplateView {
    UserTemplateView {
        id: id.to_string(),
        vault_id: vault_id.to_string(),
        name: t.name.clone(),
        summary: t.summary.clone(),
        icon: if t.icon.is_empty() {
            "category".into()
        } else {
            t.icon.clone()
        },
        fields: t
            .fields
            .iter()
            .map(|f| UserFieldView {
                key: f.key.clone(),
                field_id: UserTemplate::field_id(&f.key),
                label: f.label.clone(),
                kind: super::items::field_kind_name(f.kind).to_string(),
                required: f.required,
                hint: f.hint.clone(),
                section: f.section.clone(),
                secret: is_secret_kind(f.kind),
            })
            .collect(),
        versions,
    }
}

/// The Wi-Fi joining text phones read from a QR code
/// (`WIFI:T:WPA;S:<ssid>;P:<password>;H:true;;`, the ZXing format every
/// phone camera reads), with `\ ; , " :` escaped.
pub(crate) fn wifi_payload(
    ssid: &str,
    password: Option<&str>,
    security: Option<&str>,
    hidden: bool,
) -> Result<Zeroizing<String>, String> {
    fn esc(out: &mut String, s: &str) {
        for c in s.chars() {
            if matches!(c, '\\' | ';' | ',' | '"' | ':') {
                out.push('\\');
            }
            out.push(c);
        }
    }
    if ssid.is_empty() {
        return Err("the network has no name to put in a code".into());
    }
    let password = password.filter(|p| !p.is_empty());
    let kind = match (security, password) {
        (Some("Enterprise"), _) => {
            return Err(
                "an Enterprise network signs in with an account, which a QR code can't carry; set it up by hand"
                    .into(),
            );
        }
        (Some("Open"), _) | (_, None) => "nopass",
        (Some("WEP"), _) => "WEP",
        _ => "WPA",
    };
    let mut out = Zeroizing::new(String::with_capacity(64));
    out.push_str("WIFI:T:");
    out.push_str(kind);
    out.push_str(";S:");
    esc(&mut out, ssid);
    out.push(';');
    if let (Some(p), true) = (password, kind != "nopass") {
        out.push_str("P:");
        esc(&mut out, p);
        out.push(';');
    }
    if hidden {
        out.push_str("H:true;");
    }
    out.push(';');
    Ok(out)
}

/// A QR code of `text` as bytes: its width, then one byte per module, row
/// by row, 1 for dark. The text is a secret (a Wi-Fi password), so the
/// grid is too: it leaves only as [`Revealed`] bytes.
pub(crate) fn qr_bytes(text: &str) -> Result<Revealed, String> {
    let code = qrcode::QrCode::with_error_correction_level(text.as_bytes(), qrcode::EcLevel::M)
        .map_err(|e| format!("the code couldn't be drawn: {e}"))?;
    let width = code.width();
    let w = u8::try_from(width).map_err(|_| "the code is too large")?;
    let mut out = Vec::with_capacity(1 + width * width);
    out.push(w);
    out.extend(
        code.to_colors()
            .into_iter()
            .map(|c| u8::from(c == qrcode::Color::Dark)),
    );
    Ok(Revealed::new(out))
}

impl Session {
    // --- links (D235) -------------------------------------------------------

    /// Where a field's value lives: a linked field resolves to the field it
    /// points at, in the same vault; any other field is itself. A broken
    /// link is refused in words, never followed somewhere else.
    pub(crate) fn resolve_link(
        &self,
        id: &str,
        field_id: &str,
    ) -> Result<(String, String), String> {
        let item_id = parse_id(id)?;
        let vault = self.holding(item_id)?;
        let fields = vault.open_item(item_id).map_err(|e| e.to_string())?;
        let Some(f) = fields.iter().find(|f| f.field_id == field_id) else {
            return Ok((id.to_string(), field_id.to_string()));
        };
        if f.kind != FieldKind::Linked {
            return Ok((id.to_string(), field_id.to_string()));
        }
        let FieldValue::Text(text) = &f.value else {
            return Err("this link is broken".into());
        };
        let (target, target_field) = parse_link(text).ok_or("this link is broken")?;
        let item = vault
            .item(target)
            .map_err(|_| "the item this field points at is gone".to_string())?;
        if !matches!(item.state, ItemState::Live | ItemState::Archived) || item.kind.hidden() {
            return Err("the item this field points at is in the trash".into());
        }
        let theirs = vault.open_item(target).map_err(|e| e.to_string())?;
        let tf = theirs
            .iter()
            .find(|f| f.field_id == target_field)
            .ok_or("the item this field points at no longer has that field")?;
        if matches!(tf.kind, FieldKind::Linked | FieldKind::Passkey) {
            return Err("a link points at a value, never at another link or a passkey".into());
        }
        Ok((target.to_string(), target_field.to_string()))
    }

    /// What a linked field shows: the other item's title and the field's
    /// label, never its value.
    pub(crate) fn link_view(&self, vault_id: Id, text: &str) -> LinkView {
        let missing = |item_id: String, field_id: String| LinkView {
            item_id,
            field_id,
            title: String::new(),
            label: String::new(),
            secret: false,
            code: false,
            missing: true,
        };
        let Some((target, field_id)) = parse_link(text) else {
            return missing(String::new(), String::new());
        };
        let Ok(vault) = self.vault_by_id(vault_id) else {
            return missing(target.to_string(), field_id.to_string());
        };
        let live = vault
            .item(target)
            .is_ok_and(|i| matches!(i.state, ItemState::Live | ItemState::Archived));
        let Some(fields) = live.then(|| vault.open_item(target).ok()).flatten() else {
            return missing(target.to_string(), field_id.to_string());
        };
        match fields.iter().find(|f| f.field_id == field_id) {
            Some(f) if f.kind != FieldKind::Linked => LinkView {
                item_id: target.to_string(),
                field_id: field_id.to_string(),
                title: text_of(&fields, "title").unwrap_or_default(),
                label: f.label.clone(),
                secret: is_secret_kind(f.kind),
                code: f.kind == FieldKind::Totp,
                missing: false,
            },
            _ => missing(target.to_string(), field_id.to_string()),
        }
    }

    /// Refuses a link that points out of `vault_id`, at the item itself, at
    /// a field the other item doesn't have, or at another link or a
    /// passkey (D235). `kinds` says which changed fields are links.
    pub(crate) fn check_links(
        &self,
        vault_id: Id,
        self_id: Option<Id>,
        existing: &[PlainField],
        changes: &[FieldChange],
        user: Option<&UserTemplate>,
    ) -> Result<(), String> {
        for c in changes {
            let Change::Set(FieldInput::Text(text)) = &c.change else {
                continue;
            };
            let is_link = c.kind == Some(FieldKind::Linked)
                || c.field_id.as_deref().is_some_and(|id| {
                    existing
                        .iter()
                        .any(|f| f.field_id == id && f.kind == FieldKind::Linked)
                        || user
                            .and_then(|u| u.field(id))
                            .is_some_and(|f| f.kind == FieldKind::Linked)
                });
            if !is_link {
                continue;
            }
            let Some((target, field_id)) = parse_link(text) else {
                continue; // compose refuses it in its own words
            };
            if Some(target) == self_id {
                return Err("a field can't link to its own item".into());
            }
            let vault = self.vault_by_id(vault_id)?;
            let item = vault
                .item(target)
                .map_err(|_| "a link points at an item in the same vault only".to_string())?;
            if item.kind.hidden() || !matches!(item.state, ItemState::Live | ItemState::Archived) {
                return Err("a link points at a live item".into());
            }
            let fields = vault.open_item(target).map_err(|e| e.to_string())?;
            match fields.iter().find(|f| f.field_id == field_id) {
                None => return Err("the item a link points at has no such field".into()),
                Some(f) if matches!(f.kind, FieldKind::Linked | FieldKind::Passkey) => {
                    return Err(
                        "a link points at a value, never at another link or a passkey".into(),
                    );
                }
                Some(_) => {}
            }
        }
        Ok(())
    }

    /// The items a link in `vault_id` may point at, with their fields by
    /// label (never a value); `except` is the item being edited.
    pub fn link_targets(
        &self,
        vault_id: &str,
        except: Option<&str>,
    ) -> Result<Vec<LinkTargetView>, String> {
        let vault_id = parse_id(vault_id)?;
        let except = except.map(parse_id).transpose()?;
        let vault = self.vault_by_id(vault_id)?;
        let mut out = Vec::new();
        for item in vault.items() {
            if item.kind.hidden()
                || !matches!(item.state, ItemState::Live | ItemState::Archived)
                || Some(item.item_id) == except
            {
                continue;
            }
            let fields = vault.open_item(item.item_id).map_err(|e| e.to_string())?;
            let crown = item.class == ItemClass::CrownJewel;
            let template = template_of(item.kind, &fields);
            let pick: Vec<LinkFieldView> = fields
                .iter()
                .filter(|f| {
                    f.field_id != "title"
                        && !lockdown_vault::is_org_field(&f.field_id)
                        && !lockdown_vault::is_reserved(&f.field_id)
                        && !matches!(f.kind, FieldKind::Linked | FieldKind::Passkey)
                })
                // A crown jewel names only its template's fields: never in
                // search previews (02-model §3), not even a label of its own.
                .filter(|f| !crown || template.field(&f.field_id).is_some())
                .map(|f| LinkFieldView {
                    field_id: f.field_id.clone(),
                    label: template
                        .field(&f.field_id)
                        .map(|t| t.label.to_string())
                        .unwrap_or_else(|| f.label.clone()),
                    secret: is_secret_kind(f.kind),
                    code: f.kind == FieldKind::Totp,
                })
                .collect();
            if pick.is_empty() {
                continue;
            }
            out.push(LinkTargetView {
                item_id: item.item_id.to_string(),
                title: text_of(&fields, "title").unwrap_or_default(),
                kind: item.kind.as_str().to_string(),
                crown_jewel: crown,
                fields: pick,
            });
        }
        out.sort_by(|a, b| {
            a.title
                .to_lowercase()
                .cmp(&b.title.to_lowercase())
                .then(a.item_id.cmp(&b.item_id))
        });
        Ok(out)
    }

    // --- the card's re-prompt (D238) ------------------------------------------

    /// Whether using `field_id` of item `id` asks first: a card's security
    /// code when the card says so.
    pub(crate) fn needs_reprompt(&self, id: &str, field_id: &str) -> Result<bool, String> {
        if field_id != "card.cvc" {
            return Ok(false);
        }
        let item_id = parse_id(id)?;
        let vault = self.holding(item_id)?;
        let kind = vault.item(item_id).map_err(|e| e.to_string())?.kind;
        Ok(reprompts(
            kind,
            &vault.open_item(item_id).map_err(|e| e.to_string())?,
        ))
    }

    // --- sign-ins elsewhere (D237) --------------------------------------------

    fn write_away(&mut self, item_id: Id, entries: &[AwayEntry]) -> Result<u64, String> {
        let mut fields = self
            .holding(item_id)?
            .open_item(item_id)
            .map_err(|e| e.to_string())?;
        fields.retain(|f| f.field_id != AWAY_FIELD);
        if !entries.is_empty() {
            let text = serde_json::to_string(entries).map_err(|e| e.to_string())?;
            fields.push(PlainField::text(AWAY_FIELD, AWAY_LABEL, &text));
        }
        let n = self
            .holding_mut(item_id)?
            .update_item(item_id, fields)
            .map_err(|e| e.to_string())?;
        self.save()?;
        Ok(n)
    }

    /// Logs a sign-in on someone else's screen (a hotel or rental TV), to
    /// be reminded at `remind_at` to sign out there. A new version of the
    /// item, so every device reminds.
    pub fn log_away(
        &mut self,
        id: &str,
        place: &str,
        at: i64,
        remind_at: i64,
    ) -> Result<u64, String> {
        let item_id = parse_id(id)?;
        let item = self
            .holding(item_id)?
            .item(item_id)
            .map_err(|e| e.to_string())?;
        if item.kind.hidden() || !matches!(item.state, ItemState::Live | ItemState::Archived) {
            return Err("only a live item logs a sign-in".into());
        }
        let place = place.trim();
        if place.is_empty() || place.contains(['\n', '\r']) || place.chars().count() > 60 {
            return Err("say where in one line, up to 60 characters (\"Hotel room TV\")".into());
        }
        if remind_at < at {
            return Err("the reminder needs to be later than now".into());
        }
        let fields = self
            .holding(item_id)?
            .open_item(item_id)
            .map_err(|e| e.to_string())?;
        let mut entries = away_of(&fields);
        if entries.len() >= MAX_AWAY {
            return Err(format!(
                "{MAX_AWAY} sign-ins are logged on this item already; mark some signed out first"
            ));
        }
        // `at` is the entry's id: a second log in the same second moves on one.
        let mut at = at;
        while entries.iter().any(|e| e.at == at) {
            at += 1;
        }
        entries.push(AwayEntry {
            place: place.to_string(),
            at,
            remind_at,
        });
        self.write_away(item_id, &entries)
    }

    /// Marks a logged sign-in as signed out: it leaves the item.
    pub fn clear_away(&mut self, id: &str, at: i64) -> Result<u64, String> {
        let item_id = parse_id(id)?;
        let fields = self
            .holding(item_id)?
            .open_item(item_id)
            .map_err(|e| e.to_string())?;
        let mut entries = away_of(&fields);
        let before = entries.len();
        entries.retain(|e| e.at != at);
        if entries.len() == before {
            return Err("that sign-in isn't logged on this item".into());
        }
        self.write_away(item_id, &entries)
    }

    /// Every sign-in logged elsewhere across the persona's vaults, the
    /// soonest reminder first.
    pub fn away_reminders(&self, now: i64) -> Result<Vec<AwayReminder>, String> {
        let mut out = Vec::new();
        for vault in self.vaults() {
            for item in vault.items() {
                if item.kind.hidden()
                    || !matches!(item.state, ItemState::Live | ItemState::Archived)
                {
                    continue;
                }
                let fields = vault.open_item(item.item_id).map_err(|e| e.to_string())?;
                for a in away_of(&fields) {
                    out.push(AwayReminder {
                        item_id: item.item_id.to_string(),
                        title: text_of(&fields, "title").unwrap_or_default(),
                        place: a.place,
                        at: a.at,
                        remind_at: a.remind_at,
                        due: a.remind_at <= now,
                    });
                }
            }
        }
        out.sort_by(|a, b| {
            a.remind_at
                .cmp(&b.remind_at)
                .then(a.item_id.cmp(&b.item_id))
                .then(a.at.cmp(&b.at))
        });
        Ok(out)
    }

    // --- the Wi-Fi guest card (D236) ------------------------------------------

    /// A Wi-Fi network's joining code as a QR grid. The caller gates it
    /// (`Reveal` for a crown jewel) and counts it as a use.
    pub fn wifi_qr(&self, id: &str) -> Result<Revealed, String> {
        let item_id = parse_id(id)?;
        let vault = self.holding(item_id)?;
        let item = vault.item(item_id).map_err(|e| e.to_string())?;
        if item.kind != ItemKind::WiFi {
            return Err("only a Wi-Fi network has a joining code".into());
        }
        if !matches!(item.state, ItemState::Live | ItemState::Archived) {
            return Err("this network is in the trash; restore it first".into());
        }
        let fields = vault.open_item(item_id).map_err(|e| e.to_string())?;
        let ssid = text_of(&fields, "wifi.ssid").unwrap_or_default();
        let password = text_of(&fields, "wifi.passphrase").map(Zeroizing::new);
        let security = text_of(&fields, "wifi.security");
        let hidden = fields
            .iter()
            .any(|f| f.field_id == "wifi.hidden" && f.value == FieldValue::Boolean(true));
        let payload = wifi_payload(
            &ssid,
            password.as_deref().map(String::as_str),
            security.as_deref(),
            hidden,
        )?;
        qr_bytes(&payload)
    }

    // --- a person's own templates (D232) ----------------------------------------

    /// Every template the person made, in every vault they can change, by name.
    pub fn user_templates(&self) -> Result<Vec<UserTemplateView>, String> {
        let mut out = Vec::new();
        for vault in self.vaults() {
            let vault_id = vault.context().vault_id;
            if self.is_received_vault(vault_id) {
                continue;
            }
            for (id, t) in vault.user_templates().map_err(|e| e.to_string())? {
                let versions = vault.item(id).map(|i| i.versions.len() as u64).unwrap_or(0);
                out.push(user_view(id, vault_id, versions, &t));
            }
        }
        out.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then(a.id.cmp(&b.id))
        });
        Ok(out)
    }

    /// Saves a template: new in `vault_id` (the first vault when `None`),
    /// or a new version of `id`.
    pub fn save_user_template(
        &mut self,
        vault_id: Option<&str>,
        id: Option<&str>,
        input: &UserTemplateInput,
    ) -> Result<String, String> {
        let def = to_user_template(input)?;
        let id = id.map(parse_id).transpose()?;
        let vault_id = match (id, vault_id) {
            (Some(id), _) => self.holding(id)?.context().vault_id,
            (None, Some(v)) => parse_id(v)?,
            (None, None) => self.vault().context().vault_id,
        };
        if self.is_received_vault(vault_id) {
            return Err("this was shared with you on its own; you can't add to it".into());
        }
        self.require_edit(vault_id)?;
        let saved = self
            .vault_by_id_mut(vault_id)?
            .save_user_template(id, &def)
            .map_err(|e| e.to_string())?;
        self.save()?;
        Ok(saved.to_string())
    }

    /// Moves a template to the trash: items made from it keep their fields.
    pub fn delete_user_template(&mut self, id: &str) -> Result<(), String> {
        let item_id = parse_id(id)?;
        let vault = self.holding(item_id)?;
        if vault.item(item_id).map_err(|e| e.to_string())?.kind != ItemKind::Template {
            return Err("that isn't a template".into());
        }
        self.holding_mut(item_id)?
            .trash_item(item_id)
            .map_err(|e| e.to_string())?;
        self.save()
    }

    /// The person's template `id` in `vault_id`, to make an item from.
    pub(crate) fn user_template_in(&self, vault_id: Id, id: Id) -> Result<UserTemplate, String> {
        self.vault_by_id(vault_id)?
            .user_template(id)
            .map_err(|_| "that template isn't in this vault (it may be in the trash)".to_string())
    }

    // --- files from disk, a chunk at a time (D233) ------------------------------

    /// Attaches the file at `path`, sealed straight from the disk into the
    /// store a chunk at a time (never whole in memory), up to
    /// [`lockdown_vault::MAX_ATTACHMENT_BYTES`].
    #[cfg(not(target_os = "wasi"))]
    pub fn attach_file(
        &mut self,
        id: &str,
        path: &std::path::Path,
        mime: Option<&str>,
    ) -> Result<Id, String> {
        let item_id = parse_id(id)?;
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("the file needs a plain name")?
            .to_string();
        if name.contains(['/', '\\', '\0']) || name.trim().is_empty() {
            return Err("the file needs a plain name".into());
        }
        let mut file =
            std::fs::File::open(path).map_err(|e| format!("couldn't open the file: {e}"))?;
        let size = file
            .metadata()
            .map_err(|e| format!("couldn't read the file: {e}"))?
            .len();
        if size > lockdown_vault::MAX_ATTACHMENT_BYTES {
            return Err(format!(
                "files up to {} attach; this one is {}",
                lockdown_sync::bytes_words(lockdown_vault::MAX_ATTACHMENT_BYTES),
                lockdown_sync::bytes_words(size)
            ));
        }
        let mime = mime
            .map(str::trim)
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| lockdown_cxf::convert::mime_for(&name))
            .to_string();
        {
            let vault = self.holding_mut(item_id)?;
            let item = vault.item(item_id).map_err(|e| e.to_string())?;
            if !matches!(item.state, ItemState::Live | ItemState::Archived) || item.kind.hidden() {
                return Err("files attach to a live item; restore it first".into());
            }
        }
        let mut sink =
            lockdown_store::Store::blob_sink(&mut self.store).map_err(|e| e.to_string())?;
        let prepared = self
            .holding(item_id)?
            .seal_stream(item_id, &name, &mime, size, &mut file, &mut sink);
        let prepared = match prepared {
            Ok(p) => p,
            Err(e) => {
                if let lockdown_vault::BlobSink::File { path, .. } = sink {
                    let _ = std::fs::remove_file(path);
                }
                return Err(e.to_string());
            }
        };
        lockdown_store::Store::keep_blob(&mut self.store, prepared.blob(), sink)
            .map_err(|e| e.to_string())?;
        let att = self
            .holding_mut(item_id)?
            .add_prepared(prepared)
            .map_err(|e| e.to_string())?;
        self.save()?;
        Ok(att)
    }

    /// Writes an attached file to `path`, opened a chunk at a time: into a
    /// partial file first, renamed into place only once every chunk opened,
    /// so a changed or broken file never lands under the name chosen.
    #[cfg(not(target_os = "wasi"))]
    pub fn save_attachment_to(
        &self,
        id: &str,
        att_id: &str,
        path: &std::path::Path,
    ) -> Result<(), String> {
        self.refuse_fill_only(id)?;
        let item_id = parse_id(id)?;
        let att_id = Id::parse(att_id).map_err(|_| format!("'{att_id}' isn't an attachment id"))?;
        let vault = self.holding(item_id)?;
        let info = vault
            .attachments(item_id)
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|a| a.att_id == att_id)
            .ok_or("that file isn't attached to this item")?;
        let name = path
            .file_name()
            .ok_or_else(|| format!("{} isn't a file name", path.display()))?;
        let mut partial = name.to_os_string();
        partial.push(".partial");
        let tmp = path.with_file_name(partial);
        let write = || -> Result<(), String> {
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create(true).truncate(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut out = std::io::BufWriter::new(
                options
                    .open(&tmp)
                    .map_err(|e| format!("couldn't write {}: {e}", path.display()))?,
            );
            match vault.pending_blob(info.blob) {
                Some(b) => {
                    let mut src = std::io::Cursor::new(b.to_vec());
                    vault
                        .open_attachment_to(item_id, att_id, &mut src, &mut out)
                        .map_err(|e| e.to_string())?;
                }
                None => {
                    let mut src = lockdown_store::Store::open_blob(&self.store, info.blob)
                        .map_err(|e| e.to_string())?;
                    vault
                        .open_attachment_to(item_id, att_id, &mut src, &mut out)
                        .map_err(|e| e.to_string())?;
                }
            }
            let file = out.into_inner().map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
            std::fs::rename(&tmp, path)
                .map_err(|e| format!("couldn't write {}: {e}", path.display()))
        };
        write().inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp);
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wifi_code_is_the_format_phones_read() {
        let p = wifi_payload("home", Some("pa;ss"), Some("WPA2"), false).unwrap();
        assert_eq!(p.as_str(), "WIFI:T:WPA;S:home;P:pa\\;ss;;");
        let p = wifi_payload("caf\u{e9}: \"2\"", Some("a,b\\c"), Some("WPA3"), true).unwrap();
        assert_eq!(
            p.as_str(),
            "WIFI:T:WPA;S:caf\u{e9}\\: \\\"2\\\";P:a\\,b\\\\c;H:true;;"
        );
        assert_eq!(
            wifi_payload("guest", None, Some("WPA2"), false)
                .unwrap()
                .as_str(),
            "WIFI:T:nopass;S:guest;;"
        );
        assert_eq!(
            wifi_payload("guest", Some("x"), Some("Open"), false)
                .unwrap()
                .as_str(),
            "WIFI:T:nopass;S:guest;;"
        );
        assert_eq!(
            wifi_payload("old", Some("12345"), Some("WEP"), false)
                .unwrap()
                .as_str(),
            "WIFI:T:WEP;S:old;P:12345;;"
        );
        assert!(wifi_payload("corp", Some("x"), Some("Enterprise"), false).is_err());
        assert!(wifi_payload("", Some("x"), None, false).is_err());
    }

    #[test]
    fn a_qr_grid_is_its_width_then_its_modules() {
        let r = qr_bytes("WIFI:T:WPA;S:home;P:k7Hq3mWx9rTa4nPe6uYc;;").unwrap();
        let b = r.expose();
        let w = usize::from(b[0]);
        assert_eq!(b.len(), 1 + w * w);
        assert!(
            (21..=177).contains(&w) && (w - 21) % 4 == 0,
            "a QR version's width"
        );
        assert!(b[1..].iter().all(|m| *m <= 1));
        // The three finder patterns: a dark 7x7 ring at three corners.
        let at = |x: usize, y: usize| b[1 + y * w + x];
        for (x0, y0) in [(0, 0), (w - 7, 0), (0, w - 7)] {
            for i in 0..7 {
                assert_eq!(at(x0 + i, y0), 1);
                assert_eq!(at(x0, y0 + i), 1);
            }
            assert_eq!(at(x0 + 1, y0 + 1), 0);
            assert_eq!(at(x0 + 3, y0 + 3), 1);
        }
    }

    #[test]
    fn a_sites_rule_comes_from_the_login_or_the_published_list() {
        assert!(published().len() > 400);
        // The bundled list matches the nearest listed domain above a host.
        let (host, rule) = published_rule("www.1800flowers.com").unwrap();
        assert_eq!(host, "1800flowers.com");
        assert!(rule.contains("minlength"));
        assert!(published_rule("unlisted.example").is_none());
        assert!(published_rule("com").is_none());
        let (rule, view) = site_rule_of(None, Some("https://www.1800flowers.com/login")).unwrap();
        assert_eq!(view.source, "published");
        assert!(rule.min_length.is_some());
        // The login's own rule wins over the published one.
        let (rule, view) = site_rule_of(
            Some("minlength: 6; maxlength: 6; required: digit;"),
            Some("1800flowers.com"),
        )
        .unwrap();
        assert_eq!((view.source.as_str(), rule.max_length), ("saved", Some(6)));
        assert!(site_rule_of(None, None).is_none());
        // A generate on a Login's password takes the rule; elsewhere nothing moves.
        let mut changes = vec![
            FieldChange::text("url.0", "https://example.com"),
            FieldChange::text(RULE_FIELD, "minlength: 6; maxlength: 6; required: digit;"),
            FieldChange::new("pw", Change::Generate),
        ];
        apply_site_rules(ItemKind::Login, &[], &mut changes).unwrap();
        assert!(
            matches!(&changes[2].change, Change::GenerateWith(_, Some(r)) if r.max_length == Some(6))
        );
        let mut bad = vec![FieldChange::text(RULE_FIELD, "minlength: lots;")];
        assert!(apply_site_rules(ItemKind::Login, &[], &mut bad).is_err());
        let mut server = vec![FieldChange::new("pw", Change::Generate)];
        apply_site_rules(ItemKind::Server, &[], &mut server).unwrap();
        assert!(matches!(server[0].change, Change::Generate));
    }

    #[test]
    fn sign_ins_elsewhere_read_back_and_bad_records_read_as_none() {
        let entries = vec![AwayEntry {
            place: "Hotel TV".into(),
            at: 100,
            remind_at: 200,
        }];
        let fields = vec![PlainField::text(
            AWAY_FIELD,
            AWAY_LABEL,
            &serde_json::to_string(&entries).unwrap(),
        )];
        assert_eq!(away_of(&fields), entries);
        let views = away_views(&fields, 150);
        assert!(!views[0].due);
        assert!(away_views(&fields, 200)[0].due);
        let junk = vec![PlainField::text(AWAY_FIELD, AWAY_LABEL, "{nope")];
        assert!(away_of(&junk).is_empty());
    }
}
