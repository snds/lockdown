//! What a shell sends to make or change an item, checked against the
//! kind's [`Template`](crate::template::Template) and turned into the
//! plain fields a version is sealed from.
//!
//! An edit names **every** field the item has, each with what to do:
//! [`Change::Keep`] (left as it was: a secret the person did not retype is
//! never sent back), [`Change::Set`], [`Change::Generate`] (the core makes
//! the value; the shell never sees it until a reveal) or
//! [`Change::Remove`]. A field the edit does not name is refused rather
//! than guessed at, so a stale editor cannot drop a secret by leaving it
//! out. Secrets arrive as bytes ([`FieldInput::Secret`]) and plain values
//! as text; a secret sent as text is refused.

use lockdown_crypto::generator::{PasswordRule, generate_password};
use zeroize::Zeroizing;

use crate::crown_jewel::CrownJewelDefault;
use crate::field::{FieldKind, FieldValue, PlainField};
use crate::item::{ItemClass, ItemKind};
use crate::organization::{ORG_FOLDER, ORG_TAGS, ensure_org_fields, is_org_field, normalize_tags};
use crate::template::{CUSTOM_KINDS, Control, FieldTemplate, Template, card_brand, template_for};
use crate::{Error, Result};

/// The match rule a new web address gets (the importers' default).
pub const DEFAULT_MATCH_RULE: &str = "exact_host";

/// A value as a shell sends it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum FieldInput {
    /// A plain value (text, a date, an address, a choice).
    Text(String),
    /// A secret, as the bytes the shell read from its own control; wiped
    /// when dropped.
    Secret(Zeroizing<Vec<u8>>),
    /// Yes or no.
    Flag(bool),
    /// A web address with the rule it matches pages by (02-model §10; one
    /// of [`MATCH_RULES`]). Plain [`FieldInput::Text`] keeps a stored rule
    /// on an edit and takes [`DEFAULT_MATCH_RULE`] on a new address.
    Url {
        /// The address.
        value: String,
        /// The rule, as the matcher names it.
        match_rule: String,
    },
}

/// The match rules a web address may carry, as `lockdown_matcher::MatchRule`
/// names them on the wire.
pub const MATCH_RULES: &[&str] = &[
    "exact_host",
    "base_domain",
    "equivalent",
    "starts_with",
    "never",
];

/// What to do with one field.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Change {
    /// Leave it as it is (an edit only).
    Keep,
    /// Give it this value.
    Set(FieldInput),
    /// Make a new value with the field's generator.
    Generate,
    /// Take it off the item (an edit only).
    Remove,
}

/// One field of an add or an edit.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FieldChange {
    /// The field, or `None` for a field the person is adding by hand.
    pub field_id: Option<String>,
    /// For a field added by hand (or renaming one): its label.
    pub label: Option<String>,
    /// For a field added by hand: one of [`CUSTOM_KINDS`].
    pub kind: Option<FieldKind>,
    /// What to do.
    pub change: Change,
}

impl FieldChange {
    /// `field_id` gets `change`.
    pub fn new(field_id: &str, change: Change) -> Self {
        Self {
            field_id: Some(field_id.into()),
            label: None,
            kind: None,
            change,
        }
    }

    /// Plain text for `field_id`.
    pub fn text(field_id: &str, value: &str) -> Self {
        Self::new(field_id, Change::Set(FieldInput::Text(value.into())))
    }

    /// A secret for `field_id`, as bytes.
    pub fn secret(field_id: &str, value: &[u8]) -> Self {
        Self::new(
            field_id,
            Change::Set(FieldInput::Secret(Zeroizing::new(value.to_vec()))),
        )
    }

    /// Leave `field_id` as it is.
    pub fn keep(field_id: &str) -> Self {
        Self::new(field_id, Change::Keep)
    }

    /// A field the person adds by hand.
    pub fn custom(label: &str, kind: FieldKind, value: FieldInput) -> Self {
        Self {
            field_id: None,
            label: Some(label.into()),
            kind: Some(kind),
            change: Change::Set(value),
        }
    }
}

fn invalid(why: impl Into<String>) -> Error {
    Error::Invalid(why.into())
}

/// How a field's value is checked: its template's control, or for a field
/// outside the template, the control its stored kind implies.
fn control_for(template: Option<&FieldTemplate>, kind: FieldKind) -> Option<Control> {
    if let Some(t) = template {
        return Some(t.control);
    }
    Some(match kind {
        FieldKind::Text | FieldKind::Address => Control::Text,
        FieldKind::Hidden => Control::Secret,
        FieldKind::Url => Control::Url,
        FieldKind::Email => Control::Email,
        FieldKind::Phone => Control::Phone,
        FieldKind::Date => Control::Date,
        FieldKind::Boolean => Control::Boolean,
        FieldKind::Totp => Control::Totp,
        FieldKind::Passkey | FieldKind::Linked | FieldKind::File => return None,
    })
}

fn is_date(text: &str, with_day: bool) -> bool {
    let parts: Vec<&str> = text.split('-').collect();
    let n = |s: &str, len: usize| s.len() == len && s.bytes().all(|b| b.is_ascii_digit());
    match parts.as_slice() {
        [y, m] if !with_day => {
            n(y, 4) && n(m, 2) && (1..=12).contains(&m.parse::<u32>().unwrap_or(0))
        }
        [y, m, d] if with_day => {
            n(y, 4)
                && n(m, 2)
                && n(d, 2)
                && (1..=12).contains(&m.parse::<u32>().unwrap_or(0))
                && (1..=31).contains(&d.parse::<u32>().unwrap_or(0))
        }
        _ => false,
    }
}

/// Checks `input` for a field of `control` and makes its stored value.
/// `None` means the value was empty: the caller decides whether that is a
/// removal or a missing required field.
fn value_of(
    control: Control,
    template: Option<&FieldTemplate>,
    label: &str,
    input: FieldInput,
    match_rule: Option<&str>,
) -> Result<Option<FieldValue>> {
    let (input, match_rule) = match input {
        FieldInput::Url { value, match_rule } => {
            if control != Control::Url {
                return Err(invalid(format!("{label} isn't a web address")));
            }
            if !MATCH_RULES.contains(&match_rule.as_str()) {
                return Err(invalid(format!(
                    "'{match_rule}' isn't a way to match a website; use one of: {}",
                    MATCH_RULES.join(", ")
                )));
            }
            (FieldInput::Text(value), Some(match_rule))
        }
        other => (other, match_rule.map(str::to_string)),
    };
    let match_rule = match_rule.as_deref();
    let text = match input {
        FieldInput::Url { .. } => unreachable!("taken apart above"),
        FieldInput::Flag(b) => {
            return if control == Control::Boolean {
                Ok(Some(FieldValue::Boolean(b)))
            } else {
                Err(invalid(format!("{label} takes text, not yes or no")))
            };
        }
        FieldInput::Secret(bytes) => {
            if !control.is_secret() {
                return Err(invalid(format!(
                    "{label} isn't a secret; send it as plain text"
                )));
            }
            let text = std::str::from_utf8(&bytes)
                .map_err(|_| invalid(format!("{label} isn't valid text")))?
                .to_string();
            if text.trim().is_empty() {
                return Ok(None);
            }
            return match control {
                Control::Totp => {
                    let uri = text.trim();
                    if uri.starts_with("otpauth://") || uri.starts_with("steam://") {
                        Ok(Some(FieldValue::Text(uri.to_string())))
                    } else {
                        Err(invalid(format!(
                            "{label} needs an otpauth:// or steam:// link"
                        )))
                    }
                }
                Control::Passkey => Err(invalid(format!(
                    "{label} is made by a passkey provider and can't be typed in"
                ))),
                _ => Ok(Some(FieldValue::Text(text))),
            };
        }
        FieldInput::Text(text) => text,
    };
    if control.is_secret() {
        return Err(invalid(format!(
            "{label} is a secret and must arrive as bytes, never as text"
        )));
    }
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let one_line = || -> Result<String> {
        if trimmed.contains('\n') {
            Err(invalid(format!("{label} is one line")))
        } else {
            Ok(trimmed.to_string())
        }
    };
    let value = match control {
        Control::Text => FieldValue::Text(one_line()?),
        Control::Multiline => FieldValue::Text(text.trim_end().to_string()),
        Control::Url => {
            let t = one_line()?;
            if t.chars().any(char::is_whitespace) {
                return Err(invalid(format!("{label} can't have spaces")));
            }
            let value = if t.contains("://") {
                t
            } else {
                format!("https://{t}")
            };
            FieldValue::Url {
                value,
                match_rule: match_rule.unwrap_or(DEFAULT_MATCH_RULE).to_string(),
            }
        }
        Control::Email => {
            let t = one_line()?;
            let ok = t
                .split_once('@')
                .is_some_and(|(a, b)| !a.is_empty() && !b.is_empty() && !b.contains('@'));
            if !ok || t.contains(' ') {
                return Err(invalid(format!(
                    "{label} doesn't look like an email address"
                )));
            }
            FieldValue::Text(t)
        }
        Control::Phone => {
            let t = one_line()?;
            let digits = t.chars().filter(char::is_ascii_digit).count();
            if digits < 3
                || !t
                    .chars()
                    .all(|c| c.is_ascii_digit() || " +-().".contains(c))
            {
                return Err(invalid(format!("{label} doesn't look like a phone number")));
            }
            FieldValue::Text(t)
        }
        Control::Number => {
            let t = one_line()?;
            if !t.bytes().all(|b| b.is_ascii_digit()) {
                return Err(invalid(format!("{label} takes digits only")));
            }
            FieldValue::Text(t)
        }
        Control::Date => {
            let t = one_line()?;
            // A field outside the template may hold a month or a year (an import).
            let loose = template.is_none()
                && (is_date(&t, false) || (t.len() == 4 && t.bytes().all(|b| b.is_ascii_digit())));
            if !is_date(&t, true) && !loose {
                return Err(invalid(format!("{label} needs a date as YYYY-MM-DD")));
            }
            FieldValue::Text(t)
        }
        Control::Month => {
            let t = one_line()?;
            if !is_date(&t, false) {
                return Err(invalid(format!("{label} needs a month as YYYY-MM")));
            }
            FieldValue::Text(t)
        }
        Control::Select => {
            let t = one_line()?;
            let options = template.map(|f| f.options).unwrap_or(&[]);
            if !options.contains(&t.as_str()) {
                return Err(invalid(format!(
                    "{label} is one of: {}",
                    options.join(", ")
                )));
            }
            FieldValue::Text(t)
        }
        Control::Boolean => match trimmed {
            "true" | "yes" => FieldValue::Boolean(true),
            "false" | "no" => FieldValue::Boolean(false),
            _ => return Err(invalid(format!("{label} is yes or no"))),
        },
        Control::Secret | Control::SecretMultiline | Control::Totp | Control::Passkey => {
            unreachable!("secret controls returned above")
        }
    };
    Ok(Some(value))
}

/// The rule a `Generate` uses for a field, if any: the template's, or the
/// strong default for a secret the person added by hand.
fn generator_for(template: Option<&FieldTemplate>, kind: FieldKind) -> Option<PasswordRule> {
    match template {
        Some(t) => t.generator,
        None if kind == FieldKind::Hidden => Some(PasswordRule::STRONG),
        None => None,
    }
}

fn generated(rule: PasswordRule) -> Result<FieldValue> {
    let pw = generate_password(&rule)?;
    Ok(FieldValue::Text(pw.as_str().to_string()))
}

fn check_title(title: &str) -> Result<String> {
    let t = title.trim();
    if t.is_empty() {
        return Err(invalid("give it a title"));
    }
    if t.contains('\n') {
        return Err(invalid("a title is one line"));
    }
    Ok(t.to_string())
}

fn custom_kind(kind: Option<FieldKind>) -> Result<FieldKind> {
    let kind = kind.ok_or_else(|| invalid("a field you add needs a kind"))?;
    if CUSTOM_KINDS.iter().any(|(k, _)| *k == kind) {
        Ok(kind)
    } else {
        Err(invalid("that kind of field can't be added by hand"))
    }
}

fn custom_label(label: Option<&str>) -> Result<String> {
    let l = label.map(str::trim).unwrap_or("");
    if l.is_empty() {
        return Err(invalid("a field you add needs a label"));
    }
    Ok(l.to_string())
}

fn next_custom_index(fields: &[PlainField]) -> usize {
    fields
        .iter()
        .filter_map(|f| f.field_id.strip_prefix("custom.")?.parse::<usize>().ok())
        .max()
        .map_or(0, |n| n + 1)
}

/// A new custom field from a change, numbered `custom.<n>`.
fn custom_field(change: FieldChange, n: usize) -> Result<Option<PlainField>> {
    let kind = custom_kind(change.kind)?;
    let label = custom_label(change.label.as_deref())?;
    let control = control_for(None, kind).expect("custom kinds all have a control");
    let value = match change.change {
        Change::Set(input) => value_of(control, None, &label, input, None)?,
        Change::Generate => match generator_for(None, kind) {
            Some(rule) => Some(generated(rule)?),
            None => return Err(invalid(format!("{label} can't be generated"))),
        },
        Change::Keep | Change::Remove => {
            return Err(invalid(format!(
                "{label} is new; there is nothing to keep or remove"
            )));
        }
    };
    Ok(value.map(|value| PlainField::new(format!("custom.{n}"), kind, label, value)))
}

fn missing_required(template: &Template, fields: &[PlainField]) -> Result<()> {
    for f in template.fields.iter().filter(|f| f.required) {
        if !fields.iter().any(|p| p.field_id == f.field_id) {
            return Err(invalid(format!("{} is needed", f.label)));
        }
    }
    Ok(())
}

/// Puts `title` first, the template's fields in its order, then every
/// other field in the order it came.
fn ordered(template: &Template, title: String, mut fields: Vec<PlainField>) -> Vec<PlainField> {
    let mut out = vec![PlainField::text("title", "title", &title)];
    for t in template.fields {
        if let Some(i) = fields.iter().position(|f| f.field_id == t.field_id) {
            out.push(fields.remove(i));
        }
    }
    out.extend(fields);
    out
}

/// Fills in a card's brand from its number when the person left it empty.
fn derive(kind: ItemKind, fields: &mut Vec<PlainField>) {
    if kind != ItemKind::Card || fields.iter().any(|f| f.field_id == "card.type") {
        return;
    }
    let brand = fields
        .iter()
        .find(|f| f.field_id == "card.number")
        .and_then(|f| match &f.value {
            FieldValue::Text(t) => card_brand(t),
            _ => None,
        });
    if let Some(brand) = brand {
        fields.push(PlainField::text("card.type", "Brand", brand));
    }
}

fn no_repeats(changes: &[FieldChange]) -> Result<()> {
    let mut seen = std::collections::BTreeSet::new();
    for id in changes.iter().filter_map(|c| c.field_id.as_deref()) {
        if id == "title" {
            return Err(invalid("the title is passed on its own, not as a field"));
        }
        if !seen.insert(id) {
            return Err(invalid(format!("'{id}' is named twice")));
        }
    }
    Ok(())
}

/// A new item of `kind`: its class and its fields. `crown_jewel` asks for
/// the crown-jewel class; a kind (or a field) that is a crown jewel by
/// default is one whatever is asked, since the class is never
/// configurable downward (02-model §3).
pub fn compose_new(
    kind: ItemKind,
    title: &str,
    crown_jewel: bool,
    changes: Vec<FieldChange>,
) -> Result<(ItemClass, Vec<PlainField>)> {
    let template = template_for(kind);
    if !template.creatable {
        return Err(invalid(format!(
            "a {} isn't made by hand",
            template.name.to_lowercase()
        )));
    }
    let title = check_title(title)?;
    no_repeats(&changes)?;
    let mut fields = Vec::new();
    let mut customs = Vec::new();
    for change in changes {
        let Some(field_id) = change.field_id.clone() else {
            customs.push(change);
            continue;
        };
        let t = template.field_or_common(&field_id).ok_or_else(|| {
            invalid(format!(
                "'{field_id}' isn't a field of a {}; add it as your own field instead",
                template.name.to_lowercase()
            ))
        })?;
        let value = match change.change {
            Change::Set(input) => org_value(
                &field_id,
                value_of(t.control, Some(t), t.label, input, None)?,
            )?,
            Change::Generate => match t.generator {
                Some(rule) => Some(generated(rule)?),
                None => return Err(invalid(format!("{} can't be generated", t.label))),
            },
            Change::Keep | Change::Remove => {
                return Err(invalid(format!(
                    "a new item has nothing to keep or remove ({})",
                    t.label
                )));
            }
        };
        if let Some(value) = value {
            fields.push(PlainField::new(
                field_id.as_str(),
                t.control.storage(),
                t.label,
                value,
            ));
        }
    }
    let mut n = 0;
    for change in customs {
        if let Some(f) = custom_field(change, n)? {
            fields.push(f);
            n += 1;
        }
    }
    missing_required(template, &fields)?;
    derive(kind, &mut fields);
    ensure_org_fields(&mut fields);
    let by_default = template.crown_jewel_default() == CrownJewelDefault::Default
        || fields
            .iter()
            .any(|f| template.field(&f.field_id).is_some_and(|t| t.crown_jewel));
    let class = if crown_jewel || by_default {
        ItemClass::CrownJewel
    } else {
        ItemClass::Standard
    };
    Ok((class, ordered(template, title, fields)))
}

/// The next version's fields for an item of `kind` that holds `existing`.
/// `title: None` keeps the title. Every existing field but the title must
/// be named in `changes` (see the module docs).
pub fn compose_edit(
    kind: ItemKind,
    existing: &[PlainField],
    title: Option<&str>,
    changes: Vec<FieldChange>,
) -> Result<Vec<PlainField>> {
    let template = template_for(kind);
    no_repeats(&changes)?;
    let title = match title {
        Some(t) => check_title(t)?,
        None => existing
            .iter()
            .find(|f| f.field_id == "title")
            .and_then(|f| match &f.value {
                FieldValue::Text(t) => Some(t.clone()),
                _ => None,
            })
            .unwrap_or_default(),
    };
    // The organization fields are filed by `Vault::organize`, not drawn in
    // an editor: one the edit doesn't name is kept as it is.
    let mut changes = changes;
    for f in existing.iter().filter(|f| is_org_field(&f.field_id)) {
        if !changes
            .iter()
            .any(|c| c.field_id.as_deref() == Some(f.field_id.as_str()))
        {
            changes.push(FieldChange::keep(&f.field_id));
        }
    }
    for f in existing.iter().filter(|f| f.field_id != "title") {
        if !changes
            .iter()
            .any(|c| c.field_id.as_deref() == Some(f.field_id.as_str()))
        {
            return Err(invalid(format!(
                "the edit doesn't say what to do with {}; send keep to leave it as it is",
                f.label
            )));
        }
    }
    let mut fields: Vec<PlainField> = Vec::new();
    let mut customs = Vec::new();
    // Existing fields first, in their order, so an untouched item keeps its shape.
    for f in existing.iter().filter(|f| f.field_id != "title") {
        let change = changes
            .iter()
            .find(|c| c.field_id.as_deref() == Some(f.field_id.as_str()))
            .expect("checked above")
            .clone();
        let t = template.field_or_common(&f.field_id);
        let label = match (t, change.label.as_deref().map(str::trim)) {
            (None, Some(l)) if !l.is_empty() => l.to_string(),
            _ => f.label.clone(),
        };
        match change.change {
            Change::Keep => fields.push(PlainField { label, ..f.clone() }),
            Change::Remove => {
                if t.is_some_and(|t| t.required) {
                    return Err(invalid(format!(
                        "{} is needed and can't be removed",
                        f.label
                    )));
                }
            }
            Change::Set(input) => {
                let control = control_for(t, f.kind)
                    .ok_or_else(|| invalid(format!("{} can only be kept or removed", f.label)))?;
                let match_rule = match &f.value {
                    FieldValue::Url { match_rule, .. } => Some(match_rule.as_str()),
                    _ => None,
                };
                match org_value(
                    &f.field_id,
                    value_of(control, t, &label, input, match_rule)?,
                )? {
                    Some(value) => fields.push(PlainField {
                        label,
                        value,
                        ..f.clone()
                    }),
                    None if t.is_some_and(|t| t.required) => {
                        return Err(invalid(format!("{} is needed", f.label)));
                    }
                    None => {}
                }
            }
            Change::Generate => {
                let rule = generator_for(t, f.kind)
                    .ok_or_else(|| invalid(format!("{} can't be generated", f.label)))?;
                fields.push(PlainField {
                    label,
                    value: generated(rule)?,
                    ..f.clone()
                });
            }
        }
    }
    // Then what the edit adds: template fields the item lacked, and fields by hand.
    for change in changes {
        let Some(field_id) = change.field_id.clone() else {
            customs.push(change);
            continue;
        };
        if existing.iter().any(|f| f.field_id == field_id) {
            continue;
        }
        let t = template
            .field_or_common(&field_id)
            .ok_or_else(|| invalid(format!("there is no field '{field_id}' on this item")))?;
        let value = match change.change {
            Change::Set(input) => org_value(
                &field_id,
                value_of(t.control, Some(t), t.label, input, None)?,
            )?,
            Change::Generate => match t.generator {
                Some(rule) => Some(generated(rule)?),
                None => return Err(invalid(format!("{} can't be generated", t.label))),
            },
            Change::Keep | Change::Remove => {
                return Err(invalid(format!(
                    "this item has no {} to keep or remove",
                    t.label
                )));
            }
        };
        if let Some(value) = value {
            fields.push(PlainField::new(
                field_id.as_str(),
                t.control.storage(),
                t.label,
                value,
            ));
        }
    }
    let mut n = next_custom_index(existing);
    for change in customs {
        if let Some(f) = custom_field(change, n)? {
            fields.push(f);
            n += 1;
        }
    }
    missing_required(template, &fields)?;
    derive(kind, &mut fields);
    ensure_org_fields(&mut fields);
    Ok(ordered(template, title, fields))
}

/// The stored value of an organization field, checked: tags normalized one
/// per line (`crate::organization::normalize_tags`), a folder an id. Any
/// other field passes through.
fn org_value(field_id: &str, value: Option<FieldValue>) -> Result<Option<FieldValue>> {
    let text = match &value {
        Some(FieldValue::Text(t)) if field_id == ORG_TAGS || field_id == ORG_FOLDER => t,
        _ => return Ok(value),
    };
    if field_id == ORG_TAGS {
        let tags: Vec<&str> = text.lines().collect();
        let tags = normalize_tags(&tags)?;
        return Ok((!tags.is_empty()).then(|| FieldValue::Text(tags.join("\n"))));
    }
    crate::Id::parse(text.trim()).map_err(|_| invalid("that isn't a folder of this vault"))?;
    Ok(Some(FieldValue::Text(text.trim().to_string())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_of(fields: &[PlainField], id: &str) -> Option<String> {
        fields
            .iter()
            .find(|f| f.field_id == id)
            .map(|f| match &f.value {
                FieldValue::Text(t) => t.clone(),
                FieldValue::Url { value, .. } => value.clone(),
                FieldValue::Boolean(b) => b.to_string(),
                other => format!("{other:?}"),
            })
    }

    /// The field ids, leaving out the three organization fields every
    /// composed item carries (tested on their own below).
    fn ids(fields: &[PlainField]) -> Vec<&str> {
        fields
            .iter()
            .map(|f| f.field_id.as_str())
            .filter(|id| !is_org_field(id))
            .collect()
    }

    #[test]
    fn a_login_takes_several_websites_each_with_its_match_rule() {
        let url = |id: &str, value: &str, rule: &str| {
            FieldChange::new(
                id,
                Change::Set(FieldInput::Url {
                    value: value.into(),
                    match_rule: rule.into(),
                }),
            )
        };
        let (_, fields) = compose_new(
            ItemKind::Login,
            "Shop",
            false,
            vec![
                url("url.0", "shop.example.com", "base_domain"),
                FieldChange::text("url.1", "https://checkout.example.net"),
                url("url.2", "https://old.example.org/login", "never"),
                FieldChange::text("sign_in_method", "Another account"),
                FieldChange::text("sign_in_provider", "Google"),
            ],
        )
        .unwrap();
        let rule = |fields: &[PlainField], id: &str| {
            fields
                .iter()
                .find(|f| f.field_id == id)
                .map(|f| match &f.value {
                    FieldValue::Url { value, match_rule } => (value.clone(), match_rule.clone()),
                    other => panic!("{other:?}"),
                })
        };
        assert_eq!(
            rule(&fields, "url.0").unwrap(),
            ("https://shop.example.com".into(), "base_domain".into())
        );
        assert_eq!(rule(&fields, "url.1").unwrap().1, DEFAULT_MATCH_RULE);
        assert_eq!(rule(&fields, "url.2").unwrap().1, "never");
        assert_eq!(
            text_of(&fields, "sign_in_method").unwrap(),
            "Another account"
        );
        assert!(
            compose_new(
                ItemKind::Login,
                "x",
                false,
                vec![url("url.0", "a.example", "anywhere")]
            )
            .is_err(),
            "an unknown rule is refused"
        );
        assert!(
            compose_new(
                ItemKind::Login,
                "x",
                false,
                vec![FieldChange::text("sign_in_method", "Telepathy")]
            )
            .is_err()
        );
        assert!(
            compose_new(
                ItemKind::Note,
                "x",
                false,
                vec![url("body", "a.example", "never")]
            )
            .is_err(),
            "only a web address takes a rule"
        );

        // An edit changes one rule, keeps another's, drops a third and adds a fourth.
        let edited = compose_edit(
            ItemKind::Login,
            &fields,
            None,
            vec![
                url("url.0", "shop.example.com", "exact_host"),
                FieldChange::text("url.1", "https://checkout.example.net/pay"),
                FieldChange::new("url.2", Change::Remove),
                FieldChange::text("url.3", "https://m.example.com"),
                FieldChange::keep("sign_in_method"),
                FieldChange::keep("sign_in_provider"),
            ],
        )
        .unwrap();
        assert_eq!(rule(&edited, "url.0").unwrap().1, "exact_host");
        assert_eq!(
            rule(&edited, "url.1").unwrap(),
            (
                "https://checkout.example.net/pay".into(),
                DEFAULT_MATCH_RULE.into()
            )
        );
        assert!(rule(&edited, "url.2").is_none());
        assert_eq!(rule(&edited, "url.3").unwrap().0, "https://m.example.com");
    }

    #[test]
    fn every_composed_item_carries_its_organization_and_an_edit_keeps_it() {
        let folder = crate::Id::new();
        let (_, fields) = compose_new(
            ItemKind::Note,
            "Plans",
            false,
            vec![
                FieldChange::text("body", "x"),
                FieldChange::text(ORG_TAGS, "work / clients\nfamily\nFamily"),
                FieldChange::text(ORG_FOLDER, &folder.to_string()),
                FieldChange::new(
                    crate::organization::ORG_FAVOURITE,
                    Change::Set(FieldInput::Flag(true)),
                ),
            ],
        )
        .unwrap();
        let org = crate::Organization::of(&fields);
        assert_eq!(org.folder, Some(folder));
        assert_eq!(
            org.tags,
            vec!["family".to_string(), "work/clients".to_string()]
        );
        assert!(org.favourite);
        // A bare item gets the three fields too, empty.
        let (_, bare) = compose_new(ItemKind::Note, "Bare", false, vec![]).unwrap();
        assert_eq!(bare.iter().filter(|f| is_org_field(&f.field_id)).count(), 3);
        assert_eq!(
            crate::Organization::of(&bare),
            crate::Organization::default()
        );
        // An editor that never heard of them leaves them as they were.
        let edited = compose_edit(
            ItemKind::Note,
            &fields,
            Some("Plans 2"),
            vec![FieldChange::text("body", "y")],
        )
        .unwrap();
        assert_eq!(crate::Organization::of(&edited), org);
        // A folder that isn't an id, and a tag with an empty part, are refused.
        assert!(
            compose_new(
                ItemKind::Note,
                "x",
                false,
                vec![FieldChange::text(ORG_FOLDER, "Work")]
            )
            .is_err()
        );
        assert!(
            compose_new(
                ItemKind::Note,
                "x",
                false,
                vec![FieldChange::text(ORG_TAGS, "a//b")]
            )
            .is_err()
        );
    }

    #[test]
    fn a_wifi_network_is_checked_and_ordered_by_its_template() {
        let (class, fields) = compose_new(
            ItemKind::WiFi,
            " Home ",
            false,
            vec![
                FieldChange::new("wifi.hidden", Change::Set(FieldInput::Flag(true))),
                FieldChange::text("wifi.security", "WPA3"),
                FieldChange::new("wifi.passphrase", Change::Generate),
                FieldChange::text("wifi.ssid", "home-net"),
                FieldChange::text("notes", ""),
            ],
        )
        .unwrap();
        assert_eq!(class, ItemClass::Standard);
        assert_eq!(
            ids(&fields),
            [
                "title",
                "wifi.ssid",
                "wifi.passphrase",
                "wifi.security",
                "wifi.hidden"
            ],
            "template order; the empty note is left out"
        );
        assert_eq!(text_of(&fields, "title").unwrap(), "Home");
        let pw = text_of(&fields, "wifi.passphrase").unwrap();
        assert_eq!(pw.len(), 20, "the readable rule");
        assert_eq!(fields[2].kind, FieldKind::Hidden);
        assert_eq!(fields[4].value, FieldValue::Boolean(true));

        let bad = |change: FieldChange| {
            compose_new(
                ItemKind::WiFi,
                "x",
                false,
                vec![FieldChange::text("wifi.ssid", "n"), change],
            )
            .unwrap_err()
            .to_string()
        };
        assert!(bad(FieldChange::text("wifi.security", "Strong")).contains("one of"));
        assert!(
            bad(FieldChange::text("wifi.passphrase", "plain")).contains("must arrive as bytes")
        );
        assert!(
            bad(FieldChange::secret("wifi.ssid2", b"x"))
                .contains("isn't a field of a wi-fi network")
        );
        assert!(bad(FieldChange::keep("notes")).contains("nothing to keep"));
        assert!(
            bad(FieldChange::new("wifi.security", Change::Generate)).contains("can't be generated")
        );
        assert!(
            compose_new(ItemKind::WiFi, "x", false, vec![])
                .unwrap_err()
                .to_string()
                .contains("Network name is needed")
        );
        assert!(
            compose_new(ItemKind::WiFi, "  ", false, vec![])
                .unwrap_err()
                .to_string()
                .contains("title")
        );
    }

    #[test]
    fn the_wallet_is_a_crown_jewel_whatever_is_asked_and_others_may_opt_in() {
        let (class, _) = compose_new(
            ItemKind::CryptoWallet,
            "Cold wallet",
            false,
            vec![FieldChange::secret("seed", b"abandon ability able about above absent absorb abstract absurd abuse access accident")],
        )
        .unwrap();
        assert_eq!(class, ItemClass::CrownJewel, "never configurable downward");
        let (class, _) = compose_new(
            ItemKind::Note,
            "Plans",
            true,
            vec![FieldChange::text("body", "# hi")],
        )
        .unwrap();
        assert_eq!(class, ItemClass::CrownJewel, "a person may opt in");
        let (class, _) = compose_new(ItemKind::Note, "Plans", false, vec![]).unwrap();
        assert_eq!(class, ItemClass::Standard);
        assert!(
            compose_new(ItemKind::Passkey, "x", false, vec![]).is_err(),
            "made by a provider"
        );
        assert!(
            compose_new(ItemKind::Consent, "x", false, vec![]).is_err(),
            "the app's own"
        );
    }

    #[test]
    fn values_are_checked_by_control() {
        let card = |c: FieldChange| {
            compose_new(
                ItemKind::Card,
                "Card",
                false,
                vec![
                    FieldChange::secret("card.number", b"4111 1111 1111 1111"),
                    c,
                ],
            )
        };
        let ok = card(FieldChange::text("card.expiry", "2027-09")).unwrap().1;
        assert_eq!(
            text_of(&ok, "card.type").unwrap(),
            "Visa",
            "the brand is worked out"
        );
        assert!(card(FieldChange::text("card.expiry", "09/27")).is_err());
        let own_brand = card(FieldChange::text("card.type", "Store card"))
            .unwrap()
            .1;
        assert_eq!(text_of(&own_brand, "card.type").unwrap(), "Store card");

        let server = |c: FieldChange| {
            compose_new(
                ItemKind::Server,
                "box",
                false,
                vec![FieldChange::text("server.host", "10.0.0.2"), c],
            )
        };
        assert!(server(FieldChange::text("server.port", "22")).is_ok());
        assert!(server(FieldChange::text("server.port", "twenty-two")).is_err());
        let url = server(FieldChange::text("server.admin", "admin.example.com"))
            .unwrap()
            .1;
        assert_eq!(
            text_of(&url, "server.admin").unwrap(),
            "https://admin.example.com"
        );
        assert!(server(FieldChange::text("server.admin", "a b")).is_err());

        let doc = |c: FieldChange| compose_new(ItemKind::Document, "Passport", false, vec![c]);
        assert!(doc(FieldChange::text("expires_at", "2030-01-31")).is_ok());
        assert!(doc(FieldChange::text("expires_at", "2030-13-01")).is_err());
        assert!(doc(FieldChange::text("expires_at", "next year")).is_err());

        let id = |c: FieldChange| compose_new(ItemKind::Identity, "Me", false, vec![c]);
        assert!(id(FieldChange::text("identity.email", "me@example.com")).is_ok());
        assert!(id(FieldChange::text("identity.email", "me at example")).is_err());
        assert!(id(FieldChange::text("address.tel", "+1 (555) 010-0199")).is_ok());
        assert!(id(FieldChange::text("address.tel", "call me")).is_err());
        assert!(id(FieldChange::text("name.given", "two\nlines")).is_err());

        let code = |uri: &[u8]| {
            compose_new(
                ItemKind::Code,
                "x",
                false,
                vec![FieldChange::secret("code.0", uri)],
            )
        };
        assert!(code(b"otpauth://totp/x?secret=GEZDGNBV").is_ok());
        assert!(
            code(b"GEZDGNBV").is_err(),
            "the session turns a bare secret into a link first"
        );
    }

    #[test]
    fn fields_by_hand_are_numbered_and_kinds_are_limited() {
        let (_, fields) = compose_new(
            ItemKind::Custom,
            "Locker",
            false,
            vec![
                FieldChange::custom(
                    "Combination",
                    FieldKind::Hidden,
                    FieldInput::Secret(Zeroizing::new(b"12-34-56".to_vec())),
                ),
                FieldChange::custom("Floor", FieldKind::Text, FieldInput::Text("3".into())),
            ],
        )
        .unwrap();
        assert_eq!(ids(&fields), ["title", "custom.0", "custom.1"]);
        assert_eq!(fields[1].label, "Combination");
        assert_eq!(fields[1].kind, FieldKind::Hidden);
        let refused = compose_new(
            ItemKind::Custom,
            "x",
            false,
            vec![FieldChange::custom(
                "Key",
                FieldKind::Passkey,
                FieldInput::Text("x".into()),
            )],
        );
        assert!(
            refused
                .unwrap_err()
                .to_string()
                .contains("can't be added by hand")
        );
        let unlabelled = compose_new(
            ItemKind::Custom,
            "x",
            false,
            vec![FieldChange::custom(
                " ",
                FieldKind::Text,
                FieldInput::Text("x".into()),
            )],
        );
        assert!(unlabelled.unwrap_err().to_string().contains("label"));
    }

    fn login() -> Vec<PlainField> {
        compose_new(
            ItemKind::Login,
            "Example",
            false,
            vec![
                FieldChange::text("user", "me"),
                FieldChange::secret("pw", b"hunter2"),
                FieldChange::text("url.0", "https://example.com"),
                FieldChange::custom(
                    "PIN",
                    FieldKind::Hidden,
                    FieldInput::Secret(Zeroizing::new(b"0000".to_vec())),
                ),
            ],
        )
        .unwrap()
        .1
    }

    #[test]
    fn an_edit_keeps_untouched_secrets_without_their_bytes() {
        let before = login();
        let after = compose_edit(
            ItemKind::Login,
            &before,
            Some("Example (work)"),
            vec![
                FieldChange::text("user", "me@work"),
                FieldChange::keep("pw"),
                FieldChange::keep("url.0"),
                FieldChange {
                    label: Some("Door PIN".into()),
                    ..FieldChange::keep("custom.0")
                },
                FieldChange::text("notes", "rotated in May"),
                FieldChange::custom(
                    "Recovery email",
                    FieldKind::Email,
                    FieldInput::Text("me@example.org".into()),
                ),
            ],
        )
        .unwrap();
        assert_eq!(text_of(&after, "title").unwrap(), "Example (work)");
        assert_eq!(
            text_of(&after, "pw").unwrap(),
            "hunter2",
            "kept without being sent"
        );
        assert_eq!(text_of(&after, "user").unwrap(), "me@work");
        let pin = after.iter().find(|f| f.field_id == "custom.0").unwrap();
        assert_eq!(pin.label, "Door PIN");
        assert_eq!(pin.value, FieldValue::Text("0000".into()));
        assert_eq!(
            ids(&after),
            [
                "title", "user", "pw", "url.0", "notes", "custom.0", "custom.1"
            ]
        );
        let url = after.iter().find(|f| f.field_id == "url.0").unwrap();
        assert!(
            matches!(&url.value, FieldValue::Url { match_rule, .. } if match_rule == DEFAULT_MATCH_RULE)
        );
    }

    #[test]
    fn an_edit_must_name_every_field_and_can_generate_and_remove() {
        let before = login();
        let forgot = compose_edit(
            ItemKind::Login,
            &before,
            None,
            vec![
                FieldChange::keep("user"),
                FieldChange::keep("url.0"),
                FieldChange::keep("custom.0"),
            ],
        );
        assert!(
            forgot
                .unwrap_err()
                .to_string()
                .contains("doesn't say what to do with Password"),
            "a field left out is refused, never dropped"
        );
        let after = compose_edit(
            ItemKind::Login,
            &before,
            None,
            vec![
                FieldChange::keep("user"),
                FieldChange::new("pw", Change::Generate),
                FieldChange::new("url.0", Change::Remove),
                FieldChange::new("custom.0", Change::Generate),
            ],
        )
        .unwrap();
        assert_eq!(
            text_of(&after, "title").unwrap(),
            "Example",
            "None keeps the title"
        );
        assert!(after.iter().all(|f| f.field_id != "url.0"));
        let pw = text_of(&after, "pw").unwrap();
        assert_eq!(pw.len(), 20);
        assert_ne!(pw, "hunter2");
        assert_ne!(
            text_of(&after, "custom.0").unwrap(),
            "0000",
            "a secret of your own takes the strong rule"
        );

        let wifi = compose_new(
            ItemKind::WiFi,
            "Home",
            false,
            vec![FieldChange::text("wifi.ssid", "net")],
        )
        .unwrap()
        .1;
        let removed = compose_edit(
            ItemKind::WiFi,
            &wifi,
            None,
            vec![FieldChange::new("wifi.ssid", Change::Remove)],
        );
        assert!(
            removed
                .unwrap_err()
                .to_string()
                .contains("can't be removed")
        );
        let emptied = compose_edit(
            ItemKind::WiFi,
            &wifi,
            None,
            vec![FieldChange::text("wifi.ssid", " ")],
        );
        assert!(emptied.unwrap_err().to_string().contains("is needed"));
        let ghost = compose_edit(
            ItemKind::WiFi,
            &wifi,
            None,
            vec![
                FieldChange::keep("wifi.ssid"),
                FieldChange::keep("wifi.passphrase"),
            ],
        );
        assert!(
            ghost
                .unwrap_err()
                .to_string()
                .contains("no Password to keep")
        );
        let twice = compose_edit(
            ItemKind::WiFi,
            &wifi,
            None,
            vec![
                FieldChange::keep("wifi.ssid"),
                FieldChange::keep("wifi.ssid"),
            ],
        );
        assert!(twice.unwrap_err().to_string().contains("named twice"));
    }

    #[test]
    fn fields_outside_the_template_are_kept_or_checked_by_their_kind() {
        // An imported login: a second website and passkey material the template doesn't list.
        let mut before = login();
        before.push(PlainField::url(
            "url.1",
            "https://m.example.com",
            "base_domain",
        ));
        before.push(PlainField::new(
            "passkey.0.key",
            FieldKind::Passkey,
            "private key",
            FieldValue::Bytes(vec![1, 2, 3]),
        ));
        let mut changes = vec![
            FieldChange::keep("user"),
            FieldChange::keep("pw"),
            FieldChange::keep("url.0"),
            FieldChange::keep("custom.0"),
            FieldChange::text("url.1", "https://mobile.example.com"),
            FieldChange::keep("passkey.0.key"),
        ];
        let after = compose_edit(ItemKind::Login, &before, None, changes.clone()).unwrap();
        let url1 = after.iter().find(|f| f.field_id == "url.1").unwrap();
        assert_eq!(
            url1.value,
            FieldValue::Url {
                value: "https://mobile.example.com".into(),
                match_rule: "base_domain".into()
            },
            "an address keeps its match rule"
        );
        assert!(
            after
                .iter()
                .any(|f| f.field_id == "passkey.0.key"
                    && f.value == FieldValue::Bytes(vec![1, 2, 3]))
        );
        changes[5] = FieldChange::secret("passkey.0.key", b"x");
        assert!(
            compose_edit(ItemKind::Login, &before, None, changes)
                .unwrap_err()
                .to_string()
                .contains("only be kept or removed")
        );
    }
}
