//! Making and changing items of every kind through one generic path: the
//! vault's templates ([`lockdown_vault::template`]) say what each kind
//! holds, [`lockdown_vault::compose`] checks what a shell sent against
//! them, and this module adds the two things only the session knows: a
//! title worked out from the item when the person left it empty (a Login's
//! site, a code's issuer), and a one-time code typed as a bare set-up key
//! turned into a link the codes crate has checked.

use lockdown_codes::{Code, Secret, parse_uri};
use lockdown_vault::{
    Change, Control, FieldChange, FieldInput, FieldKind, Id, ItemKind, PlainField, template_for,
};
use zeroize::Zeroizing;

use super::{Session, parse_id, text_of};
use crate::types::{CopyTarget, CustomKindView, FieldTemplateView, GeneratorView, TemplateView};

/// The kind a shell names (`ItemKind::as_str`).
pub(crate) fn kind_named(name: &str) -> Result<ItemKind, String> {
    ItemKind::ALL
        .into_iter()
        .find(|k| k.as_str() == name)
        .ok_or_else(|| format!("'{name}' isn't a kind of item"))
}

/// Every template, as a shell draws its "New item" list, editor and detail.
pub fn templates() -> Vec<TemplateView> {
    lockdown_vault::TEMPLATES
        .iter()
        .filter(|t| !t.kind.hidden())
        .map(|t| TemplateView {
            kind: t.kind.as_str().to_string(),
            name: t.name.to_string(),
            summary: t.summary.to_string(),
            creatable: t.creatable,
            crown_jewel: t.crown_jewel_default() == lockdown_vault::CrownJewelDefault::Default,
            autofill: t.kind.autofill_allowed(),
            primary: t.primary.map(str::to_string),
            subtitle: t.subtitle.map(str::to_string),
            fields: t
                .fields
                .iter()
                .map(|f| FieldTemplateView {
                    field_id: f.field_id.to_string(),
                    label: f.label.to_string(),
                    control: f.control.as_str().to_string(),
                    secret: f.control.is_secret(),
                    required: f.required,
                    crown_jewel: f.crown_jewel,
                    fill: f.fill.as_str().to_string(),
                    generator: f.generator.map(|g| GeneratorView {
                        length: g.length as u32,
                        symbols: g.symbols,
                        unambiguous: g.unambiguous,
                        words: g.describe(),
                    }),
                    options: f.options.iter().map(|o| o.to_string()).collect(),
                    section: f.section.to_string(),
                    grouped: f.grouped,
                    hint: f.hint.to_string(),
                    repeats: f.repeats,
                })
                .collect(),
        })
        .collect()
}

/// The kinds a person may give a field they add by hand.
pub fn custom_kinds() -> Vec<CustomKindView> {
    lockdown_vault::CUSTOM_KINDS
        .iter()
        .map(|(kind, words)| CustomKindView {
            kind: field_kind_name(*kind).to_string(),
            label: words.to_string(),
        })
        .collect()
}

/// The storage kind's name as `show` writes it (`FieldView.kind`).
pub(crate) fn field_kind_name(kind: FieldKind) -> &'static str {
    match kind {
        FieldKind::Text => "text",
        FieldKind::Hidden => "hidden",
        FieldKind::Boolean => "boolean",
        FieldKind::Date => "date",
        FieldKind::Url => "url",
        FieldKind::Email => "email",
        FieldKind::Phone => "phone",
        FieldKind::Address => "address",
        FieldKind::Totp => "totp",
        FieldKind::Passkey => "passkey",
        FieldKind::Linked => "linked",
        FieldKind::File => "file",
    }
}

/// A storage kind from its name, for a field added by hand.
pub fn field_kind_named(name: &str) -> Result<FieldKind, String> {
    lockdown_vault::CUSTOM_KINDS
        .iter()
        .map(|(k, _)| *k)
        .find(|k| field_kind_name(*k) == name)
        .ok_or_else(|| format!("'{name}' isn't a kind of field you can add"))
}

/// What a row's quick copy takes (the palette and the copy button): the
/// template's primary secret where the item holds it, else the item's
/// first code. A crown jewel's copy still asks first; the core decides.
pub(crate) fn copy_target(kind: ItemKind, fields: &[PlainField]) -> Option<CopyTarget> {
    let template = template_for(kind);
    let has = |id: &str| fields.iter().any(|f| f.field_id == id);
    if let Some(p) = template.primary.filter(|p| has(p)) {
        let f = template
            .field(p)
            .expect("tested: primary is a template field");
        return Some(if f.control == Control::Totp {
            CopyTarget {
                field_id: p.to_string(),
                how: "code".into(),
                label: "code".into(),
            }
        } else {
            CopyTarget {
                field_id: p.to_string(),
                how: "secret".into(),
                label: match kind {
                    ItemKind::WiFi => "Wi-Fi password".into(),
                    _ => f.label.to_lowercase(),
                },
            }
        });
    }
    fields
        .iter()
        .find(|f| f.kind == FieldKind::Totp)
        .map(|f| CopyTarget {
            field_id: f.field_id.clone(),
            how: "code".into(),
            label: "code".into(),
        })
}

/// A code's set-up as the person typed it, as a checked link: an
/// `otpauth://` or `steam://` link as it is, or a bare base32 key as a
/// standard six-digit, thirty-second code named after the item.
fn normalize_code(text: &str, issuer: Option<&str>) -> Result<String, String> {
    let text = text.trim();
    let code = if text.starts_with("otpauth://") || text.starts_with("steam://") {
        parse_uri(text).map_err(|e| e.to_string())?
    } else {
        let secret = Secret::from_base32(text).map_err(|_| {
            "that isn't a set-up link or a set-up key; paste the otpauth:// link or the key the site showed".to_string()
        })?;
        if secret.expose().len() < 10 {
            return Err("that set-up key is too short to be a real one".into());
        }
        let mut code = Code::totp(secret);
        code.issuer = issuer.map(str::to_string).filter(|i| !i.is_empty());
        code
    };
    code.validate().map_err(|e| e.to_string())?;
    if text.starts_with("otpauth://") || text.starts_with("steam://") {
        Ok(text.to_string())
    } else {
        Ok(code.to_otpauth())
    }
}

/// Turns every code set-up in `changes` into a checked link.
fn normalize_codes(
    kind: ItemKind,
    changes: &mut [FieldChange],
    issuer: Option<&str>,
) -> Result<(), String> {
    let template = template_for(kind);
    for change in changes.iter_mut() {
        let Some(id) = change.field_id.as_deref() else {
            continue;
        };
        let is_code =
            template.field(id).map(|f| f.control) == Some(Control::Totp) || id.starts_with("code.");
        if !is_code {
            continue;
        }
        if let Change::Set(FieldInput::Secret(bytes)) = &change.change {
            let text = std::str::from_utf8(bytes).map_err(|_| "the set-up isn't valid text")?;
            if text.trim().is_empty() {
                continue;
            }
            let uri = normalize_code(text, issuer)?;
            change.change = Change::Set(FieldInput::Secret(Zeroizing::new(uri.into_bytes())));
        }
    }
    Ok(())
}

/// The plain text a change sets for `field_id`, if it sets one.
fn set_text<'a>(changes: &'a [FieldChange], field_id: &str) -> Option<&'a str> {
    changes
        .iter()
        .find(|c| c.field_id.as_deref() == Some(field_id))
        .and_then(|c| match &c.change {
            Change::Set(FieldInput::Text(t)) => Some(t.trim()),
            _ => None,
        })
        .filter(|t| !t.is_empty())
}

/// The title when the person left it empty: a Login's site host, a code's
/// issuer and account. Anything else needs one.
fn derived_title(kind: ItemKind, changes: &[FieldChange]) -> Option<String> {
    match kind {
        ItemKind::Login | ItemKind::Server => {
            let url = set_text(changes, "url.0").or_else(|| set_text(changes, "server.host"))?;
            let url = if url.contains("://") {
                url.to_string()
            } else {
                format!("https://{url}")
            };
            lockdown_matcher::Origin::parse(&url).ok().map(|o| o.host)
        }
        ItemKind::Code => changes
            .iter()
            .find(|c| c.field_id.as_deref() == Some("code.0"))
            .and_then(|c| match &c.change {
                Change::Set(FieldInput::Secret(b)) => std::str::from_utf8(b).ok(),
                _ => None,
            })
            .and_then(|uri| parse_uri(uri).ok())
            .and_then(|code| match (code.issuer, code.account) {
                (Some(i), Some(a)) => Some(format!("{i} ({a})")),
                (Some(i), None) => Some(i),
                (None, Some(a)) => Some(a),
                (None, None) => None,
            }),
        ItemKind::WiFi => set_text(changes, "wifi.ssid").map(str::to_string),
        _ => None,
    }
}

impl Session {
    /// Makes an item of `kind` (its `as_str` name) from what a shell sent.
    /// A Login's site, a code's issuer or a network's name stands in for
    /// an empty title.
    pub fn add_item(
        &mut self,
        kind: &str,
        title: Option<&str>,
        crown_jewel: bool,
        changes: Vec<FieldChange>,
    ) -> Result<Id, String> {
        let first = self.vault().context().vault_id;
        self.add_item_in(first, kind, title, crown_jewel, changes)
    }

    /// [`Self::add_item`] into the vault `vault_id` of the persona.
    pub fn add_item_in(
        &mut self,
        vault_id: Id,
        kind: &str,
        title: Option<&str>,
        crown_jewel: bool,
        mut changes: Vec<FieldChange>,
    ) -> Result<Id, String> {
        self.check_folder(vault_id, &changes)?;
        let kind = kind_named(kind)?;
        if crown_jewel
            || lockdown_vault::template_for(kind).crown_jewel_default()
                == lockdown_vault::CrownJewelDefault::Default
        {
            self.refuse_crown_in_shared(vault_id)?;
        }
        let given = title.map(str::trim).filter(|t| !t.is_empty());
        normalize_codes(kind, &mut changes, given)?;
        let title = match given {
            Some(t) => t.to_string(),
            None => derived_title(kind, &changes).unwrap_or_default(),
        };
        let id = self
            .vault_by_id_mut(vault_id)?
            .add_item(kind, &title, crown_jewel, changes)
            .map_err(|e| e.to_string())?;
        self.save()?;
        Ok(id)
    }

    /// Saves a new version of an item from an edit that names every field
    /// (Keep for a secret the person did not retype). `title: None` keeps it.
    pub fn edit_item(
        &mut self,
        id: &str,
        title: Option<&str>,
        mut changes: Vec<FieldChange>,
    ) -> Result<u64, String> {
        let item_id = parse_id(id)?;
        let vault_id = self.holding(item_id)?.context().vault_id;
        self.check_folder(vault_id, &changes)?;
        let kind = self
            .holding(item_id)?
            .item(item_id)
            .map_err(|e| e.to_string())?
            .kind;
        let existing_title = text_of(
            &self
                .holding(item_id)?
                .open_item(item_id)
                .map_err(|e| e.to_string())?,
            "title",
        );
        let issuer = title
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_string)
            .or(existing_title);
        normalize_codes(kind, &mut changes, issuer.as_deref())?;
        let n = self
            .holding_mut(item_id)?
            .edit_item(item_id, title, changes)
            .map_err(|e| e.to_string())?;
        self.save()?;
        Ok(n)
    }

    /// Every field of an item as Keep, except those `set` names: how a
    /// narrow edit (the old `edit_login`) rides on the generic path.
    pub(crate) fn keep_all_but(
        &self,
        item_id: Id,
        set: Vec<FieldChange>,
    ) -> Result<Vec<FieldChange>, String> {
        let fields = self
            .holding(item_id)?
            .open_item(item_id)
            .map_err(|e| e.to_string())?;
        let mut changes: Vec<FieldChange> = fields
            .iter()
            .filter(|f| f.field_id != "title")
            // Lockdown's own fields are kept by every edit without being named.
            .filter(|f| !lockdown_vault::is_reserved(&f.field_id))
            .filter(|f| {
                !set.iter()
                    .any(|c| c.field_id.as_deref() == Some(f.field_id.as_str()))
            })
            .map(|f| FieldChange::keep(&f.field_id))
            .collect();
        changes.extend(set);
        Ok(changes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lockdown_vault::FieldValue;

    const SEED: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

    #[test]
    fn a_bare_key_becomes_a_link_and_a_link_stays_as_it_was() {
        let uri = normalize_code(&format!("{} ", SEED.to_lowercase()), Some("Example")).unwrap();
        assert!(uri.starts_with("otpauth://totp/"), "{uri}");
        let code = parse_uri(&uri).unwrap();
        assert_eq!(code.issuer.as_deref(), Some("Example"));
        assert_eq!(code.digits, 6);
        let link = format!("otpauth://totp/Ex:me?secret={SEED}&digits=8");
        assert_eq!(normalize_code(&link, None).unwrap(), link);
        assert!(normalize_code("not a key!", None).is_err());
        assert!(
            normalize_code("GEZDGNBV", None)
                .unwrap_err()
                .contains("too short")
        );
        assert!(
            normalize_code("otpauth://totp/x", None).is_err(),
            "no secret"
        );
    }

    #[test]
    fn kinds_and_field_kinds_are_named_as_shells_name_them() {
        assert_eq!(kind_named("wi-fi").unwrap(), ItemKind::WiFi);
        assert!(kind_named("spaceship").is_err());
        assert_eq!(field_kind_named("hidden").unwrap(), FieldKind::Hidden);
        assert!(
            field_kind_named("passkey").is_err(),
            "not one a person adds"
        );
        let views = templates();
        assert_eq!(views.len(), 17, "every kind but the hidden consent record");
        assert_eq!(views.iter().filter(|t| t.creatable).count(), 16);
        let wallet = views.iter().find(|t| t.kind == "crypto-wallet").unwrap();
        assert!(wallet.crown_jewel);
        assert!(
            wallet
                .fields
                .iter()
                .any(|f| f.field_id == "seed" && f.crown_jewel && f.secret)
        );
        let wifi = views.iter().find(|t| t.kind == "wi-fi").unwrap();
        let pw = wifi
            .fields
            .iter()
            .find(|f| f.field_id == "wifi.passphrase")
            .unwrap();
        assert_eq!(
            pw.generator.as_ref().unwrap().words,
            "20 characters: letters and digits, none that look alike"
        );
        assert_eq!(custom_kinds().len(), 7);
    }

    #[test]
    fn the_quick_copy_is_the_primary_secret_or_the_first_code() {
        let pw = PlainField::hidden("pw", "Password", "x");
        let code = PlainField::new(
            "code.0",
            FieldKind::Totp,
            "One-time code",
            FieldValue::Text("otpauth://".into()),
        );
        let t = copy_target(ItemKind::Login, &[pw.clone(), code.clone()]).unwrap();
        assert_eq!(
            (t.field_id.as_str(), t.how.as_str(), t.label.as_str()),
            ("pw", "secret", "password")
        );
        let t = copy_target(ItemKind::Login, std::slice::from_ref(&code)).unwrap();
        assert_eq!((t.field_id.as_str(), t.how.as_str()), ("code.0", "code"));
        let wifi = PlainField::hidden("wifi.passphrase", "Password", "x");
        assert_eq!(
            copy_target(ItemKind::WiFi, &[wifi]).unwrap().label,
            "Wi-Fi password"
        );
        let card = PlainField::hidden("card.number", "Card number", "4111");
        assert_eq!(
            copy_target(ItemKind::Card, &[card]).unwrap().label,
            "card number"
        );
        assert!(copy_target(ItemKind::Note, &[PlainField::text("body", "Note", "x")]).is_none());
    }
}
