//! Fields: the unit of encryption (02-model §5), and the associated data
//! that binds each one to its place (02-model §2, 04-security §5).

use lockdown_crypto::aead::{Sealed, open, seal};
use lockdown_crypto::cbor::{from_slice, to_canonical};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::keys::VersionKey;
use crate::{Error, Id, Result};

/// The item schema version this crate writes and the highest it reads. It
/// is part of every field's associated data, so an old format cannot be
/// forced back (04-security §5).
pub const SCHEMA_V: u16 = 1;

/// What kind of value a field holds, and so how it is filled and shown.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FieldKind {
    /// Plain text.
    Text,
    /// Masked; reveal on tap; copy with a clipboard timer.
    Hidden,
    /// True or false.
    Boolean,
    /// A date, month-year or year; drives expiry reminders when named `expires_at`.
    Date,
    /// A URL with a match rule.
    Url,
    /// An email address.
    Email,
    /// A phone number.
    Phone,
    /// A postal address.
    Address,
    /// A one-time code set-up; second deliberate action before fill.
    Totp,
    /// A passkey's private material and metadata.
    Passkey,
    /// A reference to another item; the link itself is encrypted.
    Linked,
    /// A reference to an attachment.
    File,
}

/// A field's value. Wiped when dropped.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(tag = "t", content = "v", rename_all = "kebab-case")]
pub enum FieldValue {
    /// Text, hidden text, email, phone, address, date: all strings.
    Text(String),
    /// True or false.
    Boolean(bool),
    /// A URL and its match rule name (`exact_host`, `base_domain`, …).
    Url {
        /// The address.
        value: String,
        /// The match rule, as the matcher crate names it.
        match_rule: String,
    },
    /// Raw bytes: a passkey's COSE key, a code's packed set-up.
    Bytes(Vec<u8>),
    /// A reference to another item or an attachment.
    Ref(#[zeroize(skip)] Id),
}

/// A field in the clear: what the person sees and edits.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct PlainField {
    /// Stable id, so history can diff it (`title`, `pw`, `url.0`, `custom.3`).
    pub field_id: String,
    /// What kind of value.
    pub kind: FieldKind,
    /// The label shown.
    pub label: String,
    /// An optional section heading.
    pub section: Option<String>,
    /// The value.
    pub value: FieldValue,
}

impl PlainField {
    /// A field with no section.
    pub fn new(
        field_id: impl Into<String>,
        kind: FieldKind,
        label: impl Into<String>,
        value: FieldValue,
    ) -> Self {
        Self {
            field_id: field_id.into(),
            kind,
            label: label.into(),
            section: None,
            value,
        }
    }

    /// A text field.
    pub fn text(field_id: &str, label: &str, value: &str) -> Self {
        Self::new(
            field_id,
            FieldKind::Text,
            label,
            FieldValue::Text(value.into()),
        )
    }

    /// A hidden (masked) text field.
    pub fn hidden(field_id: &str, label: &str, value: &str) -> Self {
        Self::new(
            field_id,
            FieldKind::Hidden,
            label,
            FieldValue::Text(value.into()),
        )
    }

    /// A URL field with a match rule.
    pub fn url(field_id: &str, value: &str, match_rule: &str) -> Self {
        Self::new(
            field_id,
            FieldKind::Url,
            "website",
            FieldValue::Url {
                value: value.into(),
                match_rule: match_rule.into(),
            },
        )
    }
}

/// What is sealed: everything a reader needs that is not already in the
/// associated data.
#[derive(Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
struct FieldBody {
    label: String,
    section: Option<String>,
    value: FieldValue,
}

/// The associated data for one field: where it belongs. Canonical CBOR of
/// this is handed to the AEAD (02-model §2).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct FieldAad {
    /// The profile.
    pub profile_id: Id,
    /// The persona.
    pub persona_id: Id,
    /// The vault.
    pub vault_id: Id,
    /// The item.
    pub item_id: Id,
    /// The version.
    pub version_n: u64,
    /// The field.
    pub field_id: String,
    /// The field kind.
    pub kind: FieldKind,
    /// The item schema version.
    pub schema_v: u16,
}

impl FieldAad {
    /// The bytes handed to the AEAD.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        Ok(to_canonical(self)?)
    }
}

/// A field as stored and synced: the id and kind in the clear (both are in
/// the associated data, so they cannot be changed unnoticed), everything
/// else sealed.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SealedField {
    /// The field id.
    pub field_id: String,
    /// The field kind.
    pub kind: FieldKind,
    /// The sealed label, section and value.
    pub sealed: Sealed,
}

impl SealedField {
    /// Seals `field` under `key` at `aad`. `aad.field_id` and `aad.kind`
    /// must be the field's own.
    pub fn seal(key: &VersionKey, aad: &FieldAad, field: &PlainField) -> Result<Self> {
        if aad.field_id != field.field_id || aad.kind != field.kind {
            return Err(Error::Invalid(
                "the associated data must name this field".into(),
            ));
        }
        if aad.schema_v > SCHEMA_V {
            return Err(Error::SchemaTooNew {
                found: aad.schema_v,
                supported: SCHEMA_V,
            });
        }
        let body = FieldBody {
            label: field.label.clone(),
            section: field.section.clone(),
            value: field.value.clone(),
        };
        let plain = zeroize::Zeroizing::new(to_canonical(&body)?);
        Ok(Self {
            field_id: field.field_id.clone(),
            kind: field.kind,
            sealed: seal(key, &aad.to_bytes()?, &plain)?,
        })
    }

    /// Opens the field at the place `aad` names. A field moved to another
    /// item, version or field id fails as [`Error::FieldTampered`].
    pub fn open(&self, key: &VersionKey, aad: &FieldAad) -> Result<PlainField> {
        if aad.schema_v > SCHEMA_V {
            return Err(Error::SchemaTooNew {
                found: aad.schema_v,
                supported: SCHEMA_V,
            });
        }
        let tampered = || Error::FieldTampered {
            field_id: self.field_id.clone(),
        };
        if aad.field_id != self.field_id || aad.kind != self.kind {
            return Err(tampered());
        }
        let plain = open(key, &aad.to_bytes()?, &self.sealed).map_err(|_| tampered())?;
        let body: FieldBody = from_slice(&plain).map_err(|_| tampered())?;
        Ok(PlainField {
            field_id: self.field_id.clone(),
            kind: self.kind,
            label: body.label.clone(),
            section: body.section.clone(),
            value: body.value.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ecosystem_identity::SymmetricKey;

    fn aad(item_id: Id, version_n: u64, field: &PlainField) -> FieldAad {
        FieldAad {
            profile_id: Id::new(),
            persona_id: Id::new(),
            vault_id: Id::new(),
            item_id,
            version_n,
            field_id: field.field_id.clone(),
            kind: field.kind,
            schema_v: SCHEMA_V,
        }
    }

    #[test]
    fn a_moved_field_fails_its_tag() {
        let key = VersionKey::from_bytes(lockdown_crypto::rand::random_32().unwrap());
        let pw = PlainField::hidden("pw", "password", "correct horse");
        let item = Id::new();
        let place = aad(item, 1, &pw);
        let sealed = SealedField::seal(&key, &place, &pw).unwrap();
        assert_eq!(sealed.open(&key, &place).unwrap(), pw);

        // Same item, next version: another place.
        let mut next = place.clone();
        next.version_n = 2;
        assert!(matches!(
            sealed.open(&key, &next),
            Err(Error::FieldTampered { .. })
        ));
        // Another item.
        let mut other_item = place.clone();
        other_item.item_id = Id::new();
        assert!(matches!(
            sealed.open(&key, &other_item),
            Err(Error::FieldTampered { .. })
        ));
        // Re-labelled as another field: refused by name, and the tag would
        // fail anyway.
        let mut relabelled = sealed.clone();
        relabelled.field_id = "user".into();
        let mut as_user = place.clone();
        as_user.field_id = "user".into();
        assert!(matches!(
            relabelled.open(&key, &as_user),
            Err(Error::FieldTampered { .. })
        ));
        // A newer schema is refused, not guessed at.
        let mut future = place.clone();
        future.schema_v = SCHEMA_V + 1;
        assert!(matches!(
            sealed.open(&key, &future),
            Err(Error::SchemaTooNew { .. })
        ));
    }

    #[test]
    fn the_aad_is_the_eight_named_fields() {
        let f = PlainField::text("title", "title", "Bank");
        let a = aad(Id::new(), 7, &f);
        let bytes = a.to_bytes().unwrap();
        // A CBOR map of exactly eight entries.
        assert_eq!(bytes[0], 0xa8);
        let back: FieldAad = from_slice(&bytes).unwrap();
        assert_eq!(back, a);
    }
}
