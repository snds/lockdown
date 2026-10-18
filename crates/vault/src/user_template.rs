//! A person's own templates (matrix `typed-templates`: "user-defined
//! templates"; 02-model §4 `Custom`; D232).
//!
//! A template a person makes is an item of the hidden kind
//! [`ItemKind::Template`]: its name is the item's title and its definition
//! one sealed field, so it syncs, merges, keeps a history and goes to the
//! trash like any item, and a shared vault's members share its templates.
//!
//! An item made from one is a [`ItemKind::Custom`] item whose fields the
//! template named are `tpl.<key>` (each with the template's label and
//! kind, and checked by that kind), plus any fields of its own
//! (`custom.<n>`); [`TEMPLATE_FIELD`] holds `user:<template item id>`. The
//! item carries its own labels, so it reads the same when the template is
//! changed, deleted or left behind in another vault.

use lockdown_crypto::cbor::{from_slice, to_canonical};
use serde::{Deserialize, Serialize};

use crate::field::{FieldKind, FieldValue, PlainField};
use crate::template::{CUSTOM_KINDS, TEMPLATE_FIELD};
use crate::{Error, Id, Result};

/// The definition's field on a template item.
pub const DEFINITION_FIELD: &str = "template.definition";

/// The prefix of a field a person's template named on an item made from it.
pub const TEMPLATE_FIELD_PREFIX: &str = "tpl.";

/// What [`TEMPLATE_FIELD`] holds on an item made from a person's template.
pub const USER_PREFIX: &str = "user:";

/// The most fields a template may name.
pub const MAX_FIELDS: usize = 40;

/// The definition's format version.
pub const DEFINITION_V: u16 = 1;

/// One field of a person's template.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct UserField {
    /// Its key: `a`–`z`, `0`–`9` and `_`, up to 32; the item's field is `tpl.<key>`.
    pub key: String,
    /// Its label.
    pub label: String,
    /// One of the kinds a field of your own may take ([`CUSTOM_KINDS`]).
    pub kind: FieldKind,
    /// An item made from the template is refused without it.
    #[serde(default)]
    pub required: bool,
    /// One line of help, or empty.
    #[serde(default)]
    pub hint: String,
    /// The heading it is shown under, or empty for "Details".
    #[serde(default)]
    pub section: String,
}

/// A person's template.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct UserTemplate {
    /// Its name ("Gym locker"): the item's title.
    pub name: String,
    /// One line on what it is for, or empty.
    #[serde(default)]
    pub summary: String,
    /// A Material Symbols icon name, or empty for the custom item's.
    #[serde(default)]
    pub icon: String,
    /// Its fields, in order.
    pub fields: Vec<UserField>,
}

#[derive(Serialize, Deserialize)]
struct Stored {
    v: u16,
    summary: String,
    icon: String,
    fields: Vec<UserField>,
}

fn invalid(why: impl Into<String>) -> Error {
    Error::Invalid(why.into())
}

fn one_line(what: &str, text: &str, max: usize) -> Result<String> {
    let t = text.trim();
    if t.contains('\n') || t.contains('\r') {
        return Err(invalid(format!("{what} is one line")));
    }
    if t.chars().count() > max {
        return Err(invalid(format!("{what} is at most {max} characters")));
    }
    Ok(t.to_string())
}

fn is_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 32
        && key
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

fn is_icon(icon: &str) -> bool {
    icon.len() <= 40
        && icon
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

impl UserTemplate {
    /// Checks and tidies the definition: a name, at most [`MAX_FIELDS`]
    /// fields with unique keys, labels on one line, kinds a field of your
    /// own may take.
    pub fn checked(&self) -> Result<Self> {
        let name = one_line("a template's name", &self.name, 60)?;
        if name.is_empty() {
            return Err(invalid("give the template a name"));
        }
        let summary = one_line("a template's summary", &self.summary, 120)?;
        let icon = self.icon.trim().to_string();
        if !is_icon(&icon) {
            return Err(invalid("that isn't an icon's name"));
        }
        if self.fields.is_empty() {
            return Err(invalid("a template needs at least one field"));
        }
        if self.fields.len() > MAX_FIELDS {
            return Err(invalid(format!(
                "a template has at most {MAX_FIELDS} fields"
            )));
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut fields = Vec::with_capacity(self.fields.len());
        for f in &self.fields {
            let key = f.key.trim().to_string();
            if !is_key(&key) {
                return Err(invalid(format!(
                    "'{key}' isn't a field key: use a to z, 0 to 9 and _, up to 32"
                )));
            }
            if !seen.insert(key.clone()) {
                return Err(invalid(format!("the key '{key}' is used twice")));
            }
            let label = one_line("a field's label", &f.label, 60)?;
            if label.is_empty() {
                return Err(invalid(format!("the field '{key}' needs a label")));
            }
            if !CUSTOM_KINDS.iter().any(|(k, _)| *k == f.kind) {
                return Err(invalid(format!(
                    "{label} can't be that kind of field; use one a field of your own may be"
                )));
            }
            fields.push(UserField {
                key,
                label,
                kind: f.kind,
                required: f.required,
                hint: one_line("a field's hint", &f.hint, 160)?,
                section: one_line("a field's section", &f.section, 40)?,
            });
        }
        Ok(Self {
            name,
            summary,
            icon,
            fields,
        })
    }

    /// The template item's fields: the title and the sealed definition.
    pub fn to_fields(&self) -> Result<Vec<PlainField>> {
        let t = self.checked()?;
        let stored = Stored {
            v: DEFINITION_V,
            summary: t.summary,
            icon: t.icon,
            fields: t.fields,
        };
        Ok(vec![
            PlainField::text("title", "title", &t.name),
            PlainField::new(
                DEFINITION_FIELD,
                FieldKind::Text,
                "Definition",
                FieldValue::Bytes(to_canonical(&stored)?),
            ),
        ])
    }

    /// Reads a template item's fields back. A definition from a newer
    /// build, or one that doesn't read, is refused rather than guessed at.
    pub fn from_fields(fields: &[PlainField]) -> Result<Self> {
        let name = fields
            .iter()
            .find(|f| f.field_id == "title")
            .and_then(|f| match &f.value {
                FieldValue::Text(t) => Some(t.clone()),
                _ => None,
            })
            .unwrap_or_default();
        let bytes = fields
            .iter()
            .find(|f| f.field_id == DEFINITION_FIELD)
            .and_then(|f| match &f.value {
                FieldValue::Bytes(b) => Some(b.as_slice()),
                _ => None,
            })
            .ok_or_else(|| invalid("this template has no definition"))?;
        let stored: Stored =
            from_slice(bytes).map_err(|_| invalid("this template's definition doesn't read"))?;
        if stored.v > DEFINITION_V {
            return Err(invalid(
                "this template was made by a newer Lockdown; update to use it",
            ));
        }
        Ok(Self {
            name,
            summary: stored.summary,
            icon: stored.icon,
            fields: stored.fields,
        })
    }

    /// The field on an item for `key`.
    pub fn field_id(key: &str) -> String {
        format!("{TEMPLATE_FIELD_PREFIX}{key}")
    }

    /// One field by its item field id (`tpl.<key>`).
    pub fn field(&self, field_id: &str) -> Option<&UserField> {
        let key = field_id.strip_prefix(TEMPLATE_FIELD_PREFIX)?;
        self.fields.iter().find(|f| f.key == key)
    }

    /// The reserved field naming template `id` on an item made from it.
    pub fn marker(id: Id) -> PlainField {
        PlainField::text(TEMPLATE_FIELD, "Template", &format!("{USER_PREFIX}{id}"))
    }
}

/// The person's template an item names, if it names one.
pub fn user_template_of(fields: &[PlainField]) -> Option<Id> {
    let v = crate::template::variant_of(fields)?;
    Id::parse(v.strip_prefix(USER_PREFIX)?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gym() -> UserTemplate {
        UserTemplate {
            name: " Gym locker ".into(),
            summary: "Locker and membership".into(),
            icon: "fitness_center".into(),
            fields: vec![
                UserField {
                    key: "locker".into(),
                    label: "Locker number".into(),
                    kind: FieldKind::Text,
                    required: true,
                    hint: String::new(),
                    section: String::new(),
                },
                UserField {
                    key: "combo".into(),
                    label: "Combination".into(),
                    kind: FieldKind::Hidden,
                    required: false,
                    hint: "Three numbers".into(),
                    section: "Lock".into(),
                },
            ],
        }
    }

    #[test]
    fn a_template_round_trips_through_its_item_fields() {
        let fields = gym().to_fields().unwrap();
        assert_eq!(fields[0].field_id, "title");
        let back = UserTemplate::from_fields(&fields).unwrap();
        assert_eq!(back, gym().checked().unwrap());
        assert_eq!(back.name, "Gym locker");
        assert_eq!(back.field("tpl.combo").unwrap().label, "Combination");
        assert!(back.field("custom.0").is_none());
        let id = Id::new();
        assert_eq!(user_template_of(&[UserTemplate::marker(id)]), Some(id));
        assert_eq!(
            user_template_of(&[PlainField::text(TEMPLATE_FIELD, "x", "passport")]),
            None
        );
    }

    #[test]
    fn a_bad_definition_is_refused() {
        let mut t = gym();
        t.name = "  ".into();
        assert!(t.checked().is_err());
        let mut t = gym();
        t.fields[1].key = "locker".into();
        assert!(t.checked().unwrap_err().to_string().contains("twice"));
        let mut t = gym();
        t.fields[0].key = "Locker No".into();
        assert!(t.checked().is_err());
        let mut t = gym();
        t.fields[0].kind = FieldKind::Passkey;
        assert!(t.checked().is_err());
        let mut t = gym();
        t.fields[0].label = "two\nlines".into();
        assert!(t.checked().is_err());
        let mut t = gym();
        t.icon = "<svg>".into();
        assert!(t.checked().is_err());
        let mut t = gym();
        t.fields.clear();
        assert!(t.checked().is_err());
        let mut t = gym();
        t.fields = (0..=MAX_FIELDS)
            .map(|i| UserField {
                key: format!("f{i}"),
                ..gym().fields[0].clone()
            })
            .collect();
        assert!(t.checked().is_err());
    }

    #[test]
    fn a_newer_definition_is_refused_not_guessed() {
        let mut fields = gym().to_fields().unwrap();
        let newer = Stored {
            v: DEFINITION_V + 1,
            summary: String::new(),
            icon: String::new(),
            fields: vec![],
        };
        fields[1].value = FieldValue::Bytes(to_canonical(&newer).unwrap());
        assert!(
            UserTemplate::from_fields(&fields)
                .unwrap_err()
                .to_string()
                .contains("newer")
        );
        fields[1].value = FieldValue::Bytes(vec![0xff, 0x00]);
        assert!(UserTemplate::from_fields(&fields).is_err());
    }

    #[test]
    fn a_template_lives_in_the_vault_as_an_item_with_a_history() {
        use crate::compose::FieldChange;
        let mut vault = crate::test_support::sample_vault();
        let tid = vault.save_user_template(None, &gym()).unwrap();
        let item = vault.item(tid).unwrap();
        assert_eq!(item.kind, crate::ItemKind::Template);
        assert!(item.kind.hidden());
        let listed = vault.user_templates().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(
            (listed[0].0, listed[0].1.name.as_str()),
            (tid, "Gym locker")
        );

        let made = vault
            .add_item_from_user(
                tid,
                "Leisure centre",
                false,
                vec![FieldChange::text("tpl.locker", "12")],
            )
            .unwrap();
        let fields = vault.open_item(made).unwrap();
        assert_eq!(vault.item(made).unwrap().kind, crate::ItemKind::Custom);
        assert_eq!(user_template_of(&fields), Some(tid));
        let locker = fields.iter().find(|f| f.field_id == "tpl.locker").unwrap();
        assert_eq!(locker.label, "Locker number");

        // A new version of the template: the history keeps the old one, and
        // an item already made keeps its own labels.
        let mut v2 = gym();
        v2.fields[0].label = "Locker".into();
        assert_eq!(vault.save_user_template(Some(tid), &v2).unwrap(), tid);
        assert_eq!(vault.item(tid).unwrap().versions.len(), 2);
        assert_eq!(vault.user_template(tid).unwrap().fields[0].label, "Locker");
        let again = vault.open_item(made).unwrap();
        assert_eq!(
            again
                .iter()
                .find(|f| f.field_id == "tpl.locker")
                .unwrap()
                .label,
            "Locker number"
        );

        // A trashed template makes nothing and is no longer listed.
        vault.trash_item(tid).unwrap();
        assert!(vault.user_templates().unwrap().is_empty());
        assert!(vault.add_item_from_user(tid, "x", false, vec![]).is_err());
        // Saving over something that isn't a template is refused.
        assert!(vault.save_user_template(Some(made), &gym()).is_err());
    }
}
