//! The FIDO Credential Exchange Format v1.0 document shape, as JSON.
//! Spec: <https://fidoalliance.org/specs/cx/cxf-v1.0-ps-20250814.html>.
//!
//! Field names are the spec's camelCase. Binary values (`id`,
//! `credentialId`, `userHandle`, `key`) are base64url without padding.
//! Timestamps are seconds since the epoch. Credentials carry a `type`
//! discriminator; types this crate does not model are kept as
//! [`Credential::Other`] and written back unchanged, so a round trip through
//! Lockdown never drops a credential it did not understand.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{CxfError, Limits, Result};

/// The document version this crate writes.
pub const CXF_VERSION: u8 = 1;

/// The top of a CXF document.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Header {
    /// Format version (1).
    pub version: u8,
    /// The exporting app's relying-party id, such as `lockdown.app`.
    pub exporter_rp_id: String,
    /// The exporting app's name.
    pub exporter_display_name: String,
    /// When exported, seconds since the epoch.
    pub timestamp: u64,
    /// The accounts (one per persona, in Lockdown's case).
    pub accounts: Vec<Account>,
}

/// One account's worth of items.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    /// base64url id.
    pub id: String,
    /// The account's user name.
    #[serde(default)]
    pub username: String,
    /// Email.
    #[serde(default)]
    pub email: String,
    /// Full name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub full_name: Option<String>,
    /// Folders.
    #[serde(default)]
    pub collections: Vec<Collection>,
    /// Items.
    #[serde(default)]
    pub items: Vec<Item>,
    /// Anything else, kept as read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Value>,
}

/// A folder.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    /// base64url id.
    pub id: String,
    /// Name.
    pub title: String,
    /// Subtitle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    /// The items in it, by reference.
    #[serde(default)]
    pub items: Vec<LinkedItem>,
    /// Nested folders.
    #[serde(default)]
    pub sub_collections: Vec<Collection>,
}

/// A reference from a collection to an item.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkedItem {
    /// The item's base64url id.
    pub item: String,
    /// The account's base64url id, when the item is in another account.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
}

/// Where an item applies.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialScope {
    /// Web addresses.
    #[serde(default)]
    pub urls: Vec<String>,
    /// Android app ids.
    #[serde(default)]
    pub android_apps: Vec<Value>,
}

/// One item: a title, a scope and one or more credentials.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    /// base64url id.
    pub id: String,
    /// Created, seconds since the epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creation_at: Option<u64>,
    /// Modified, seconds since the epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<u64>,
    /// The title.
    pub title: String,
    /// A subtitle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    /// Favourite.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub favorite: Option<bool>,
    /// Where it applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<CredentialScope>,
    /// The credentials.
    #[serde(default)]
    pub credentials: Vec<Credential>,
    /// Tags.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Anything else, kept as read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Value>,
}

/// A field the person may edit: a typed string.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditableField {
    /// base64url id, optional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// `string`, `concealed-string`, `email`, `number`, `boolean`, `date`,
    /// `year-month`, `subdivision-code`, `country-code`, …
    pub field_type: String,
    /// The value, always a string.
    pub value: String,
    /// A label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl EditableField {
    /// A `string` field.
    pub fn string(value: impl Into<String>) -> Self {
        Self {
            id: None,
            field_type: "string".into(),
            value: value.into(),
            label: None,
        }
    }

    /// A `concealed-string` field.
    pub fn concealed(value: impl Into<String>) -> Self {
        Self {
            field_type: "concealed-string".into(),
            ..Self::string(value)
        }
    }

    /// A field of another type.
    pub fn typed(field_type: &str, value: impl Into<String>) -> Self {
        Self {
            field_type: field_type.into(),
            ..Self::string(value)
        }
    }
}

/// `basic-auth`: a username and password.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BasicAuth {
    /// The username.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<EditableField>,
    /// The password.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<EditableField>,
}

/// `passkey`: a WebAuthn credential.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Passkey {
    /// base64url credential id.
    pub credential_id: String,
    /// The relying party id.
    pub rp_id: String,
    /// The user name.
    #[serde(default)]
    pub user_name: String,
    /// The user display name.
    #[serde(default)]
    pub user_display_name: String,
    /// base64url user handle.
    pub user_handle: String,
    /// base64url PKCS#8 private key.
    pub key: String,
    /// FIDO2 extension data, kept as read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fido2_extensions: Option<Value>,
}

/// `note`: free text.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    /// The body.
    pub content: EditableField,
}

/// `credit-card`.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreditCard {
    /// The number.
    pub number: EditableField,
    /// The holder's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub full_name: Option<EditableField>,
    /// Visa, Mastercard, …
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_type: Option<EditableField>,
    /// The CVC / CVV.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verification_number: Option<EditableField>,
    /// The PIN.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pin: Option<EditableField>,
    /// Expiry, a `year-month` field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expiry_date: Option<EditableField>,
    /// Valid from, a `year-month` field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<EditableField>,
}

/// `totp`: a one-time code set-up.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Totp {
    /// The base32 secret.
    pub secret: String,
    /// Seconds per step.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub period: Option<u32>,
    /// Digits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digits: Option<u32>,
    /// The account name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// `sha1`, `sha256`, `sha512`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub algorithm: Option<String>,
    /// The issuer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuer: Option<String>,
}

/// `person-name`.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonName {
    /// Title (Dr, Ms, …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<EditableField>,
    /// Given name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub given: Option<EditableField>,
    /// Middle name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub middle: Option<EditableField>,
    /// Family name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<EditableField>,
    /// Suffix.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation: Option<EditableField>,
}

/// `address`.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Address {
    /// Street address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub street_address: Option<EditableField>,
    /// City.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub city: Option<EditableField>,
    /// State or region, a `subdivision-code` field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub territory: Option<EditableField>,
    /// Country, a `country-code` field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country: Option<EditableField>,
    /// Postal code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub postal_code: Option<EditableField>,
    /// Phone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tel: Option<EditableField>,
}

/// `wifi` (CXF §"Wifi"): a network's name, security type, passphrase and
/// whether it is hidden. `networkSecurityType` takes `unsecured`,
/// `wpa-personal`, `wpa2-personal`, `wpa3-personal` or `wep`.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Wifi {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssid: Option<EditableField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network_security_type: Option<EditableField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub passphrase: Option<EditableField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hidden: Option<EditableField>,
}

/// `drivers-license`.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriversLicense {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub full_name: Option<EditableField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub birth_date: Option<EditableField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue_date: Option<EditableField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expiry_date: Option<EditableField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuing_authority: Option<EditableField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub territory: Option<EditableField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country: Option<EditableField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license_number: Option<EditableField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license_class: Option<EditableField>,
}

/// `passport`.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Passport {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuing_country: Option<EditableField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub passport_type: Option<EditableField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub passport_number: Option<EditableField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub national_identification_number: Option<EditableField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nationality: Option<EditableField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub full_name: Option<EditableField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub birth_date: Option<EditableField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub birth_place: Option<EditableField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sex: Option<EditableField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue_date: Option<EditableField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expiry_date: Option<EditableField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuing_authority: Option<EditableField>,
}

/// `identity-document`: a national ID card and the like.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityDocument {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuing_country: Option<EditableField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_number: Option<EditableField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identification_number: Option<EditableField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nationality: Option<EditableField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub full_name: Option<EditableField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub birth_date: Option<EditableField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub birth_place: Option<EditableField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sex: Option<EditableField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue_date: Option<EditableField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expiry_date: Option<EditableField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuing_authority: Option<EditableField>,
}

/// `file`: a file's id, name, size and SHA-256 (`integrityHash`,
/// base64url). The format holds no bytes: they belong to the transfer
/// that carries the document (the exchange protocol, a Working Draft).
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct File {
    pub id: String,
    pub name: String,
    pub decrypted_size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integrity_hash: Option<String>,
}

/// One credential. The JSON carries a `type` field naming the variant.
#[derive(Clone, PartialEq, Debug)]
pub enum Credential {
    /// `basic-auth`.
    BasicAuth(BasicAuth),
    /// `passkey`.
    Passkey(Passkey),
    /// `note`.
    Note(Note),
    /// `credit-card`.
    CreditCard(CreditCard),
    /// `totp`.
    Totp(Totp),
    /// `person-name`.
    PersonName(PersonName),
    /// `address`.
    Address(Address),
    /// `wifi`.
    Wifi(Wifi),
    /// `drivers-license`.
    DriversLicense(DriversLicense),
    /// `passport`.
    Passport(Passport),
    /// `identity-document`.
    IdentityDocument(IdentityDocument),
    /// `file`.
    File(File),
    /// Any type this crate does not model, kept exactly as read.
    Other {
        /// The `type` value.
        kind: String,
        /// The whole object.
        raw: Value,
    },
}

impl Credential {
    /// The `type` value.
    pub fn kind(&self) -> &str {
        match self {
            Self::BasicAuth(_) => "basic-auth",
            Self::Passkey(_) => "passkey",
            Self::Note(_) => "note",
            Self::CreditCard(_) => "credit-card",
            Self::Totp(_) => "totp",
            Self::PersonName(_) => "person-name",
            Self::Address(_) => "address",
            Self::Wifi(_) => "wifi",
            Self::DriversLicense(_) => "drivers-license",
            Self::Passport(_) => "passport",
            Self::IdentityDocument(_) => "identity-document",
            Self::File(_) => "file",
            Self::Other { kind, .. } => kind,
        }
    }
}

impl Serialize for Credential {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        fn tagged<T: Serialize, S: serde::Serializer>(
            kind: &str,
            v: &T,
            s: S,
        ) -> std::result::Result<S::Ok, S::Error> {
            let mut value = serde_json::to_value(v).map_err(serde::ser::Error::custom)?;
            if let Value::Object(map) = &mut value {
                map.insert("type".into(), Value::String(kind.into()));
            }
            value.serialize(s)
        }
        match self {
            Self::BasicAuth(v) => tagged("basic-auth", v, s),
            Self::Passkey(v) => tagged("passkey", v, s),
            Self::Note(v) => tagged("note", v, s),
            Self::CreditCard(v) => tagged("credit-card", v, s),
            Self::Totp(v) => tagged("totp", v, s),
            Self::PersonName(v) => tagged("person-name", v, s),
            Self::Address(v) => tagged("address", v, s),
            Self::Wifi(v) => tagged("wifi", v, s),
            Self::DriversLicense(v) => tagged("drivers-license", v, s),
            Self::Passport(v) => tagged("passport", v, s),
            Self::IdentityDocument(v) => tagged("identity-document", v, s),
            Self::File(v) => tagged("file", v, s),
            Self::Other { raw, .. } => raw.serialize(s),
        }
    }
}

impl<'de> Deserialize<'de> for Credential {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        use serde::de::Error as _;
        let raw = Value::deserialize(d)?;
        let kind = raw
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| D::Error::custom("a credential needs a 'type'"))?
            .to_string();
        fn parse<T: serde::de::DeserializeOwned, E: serde::de::Error>(
            v: Value,
        ) -> std::result::Result<T, E> {
            serde_json::from_value(v).map_err(E::custom)
        }
        Ok(match kind.as_str() {
            "basic-auth" => Self::BasicAuth(parse(raw)?),
            "passkey" => Self::Passkey(parse(raw)?),
            "note" => Self::Note(parse(raw)?),
            "credit-card" => Self::CreditCard(parse(raw)?),
            "totp" => Self::Totp(parse(raw)?),
            "person-name" => Self::PersonName(parse(raw)?),
            "address" => Self::Address(parse(raw)?),
            "wifi" => Self::Wifi(parse(raw)?),
            "drivers-license" => Self::DriversLicense(parse(raw)?),
            "passport" => Self::Passport(parse(raw)?),
            "identity-document" => Self::IdentityDocument(parse(raw)?),
            "file" => Self::File(parse(raw)?),
            _ => Self::Other { kind, raw },
        })
    }
}

/// Encodes bytes as base64url without padding, as CXF does.
pub fn b64url(bytes: &[u8]) -> String {
    data_encoding::BASE64URL_NOPAD.encode(bytes)
}

/// Decodes base64url, padded or not.
pub fn from_b64url(text: &str) -> Result<Vec<u8>> {
    let trimmed = text.trim_end_matches('=');
    data_encoding::BASE64URL_NOPAD
        .decode(trimmed.as_bytes())
        .map_err(|_| CxfError::BadBase64(text.chars().take(16).collect()))
}

/// Parses a CXF document from bytes, within `limits`.
pub fn parse(bytes: &[u8], limits: &Limits) -> Result<Header> {
    limits.check_size(bytes)?;
    let header: Header = serde_json::from_slice(bytes).map_err(|e| CxfError::Parse {
        format: "CXF",
        why: e.to_string(),
    })?;
    let items: usize = header.accounts.iter().map(|a| a.items.len()).sum();
    if items > limits.max_items {
        return Err(CxfError::TooMany {
            what: "items",
            found: items,
            limit: limits.max_items,
        });
    }
    for item in header.accounts.iter().flat_map(|a| a.items.iter()) {
        if item.credentials.len() > limits.max_credentials_per_item {
            return Err(CxfError::TooMany {
                what: "credentials in one item",
                found: item.credentials.len(),
                limit: limits.max_credentials_per_item,
            });
        }
        for id in [&item.id]
            .into_iter()
            .chain(item.credentials.iter().filter_map(|c| match c {
                Credential::Passkey(p) => Some(&p.credential_id),
                _ => None,
            }))
        {
            from_b64url(id)?;
        }
    }
    Ok(header)
}

/// Writes a CXF document as pretty JSON.
pub fn write(header: &Header) -> Result<Vec<u8>> {
    serde_json::to_vec_pretty(header).map_err(|e| CxfError::Parse {
        format: "CXF",
        why: e.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_credential_types_survive_a_round_trip() {
        let json = r#"{"version":1,"exporterRpId":"other.example","exporterDisplayName":"Other","timestamp":1,
            "accounts":[{"id":"YWNjdA","username":"me","email":"","items":[
              {"id":"aXRlbQ","title":"X","credentials":[
                {"type":"basic-auth","username":{"fieldType":"string","value":"u"},"password":{"fieldType":"concealed-string","value":"p"}},
                {"type":"x-future","ssid":{"fieldType":"string","value":"home"},"passphrase":{"fieldType":"concealed-string","value":"s"}}
              ]}]}]}"#;
        let h = parse(json.as_bytes(), &Limits::default()).unwrap();
        let creds = &h.accounts[0].items[0].credentials;
        assert!(
            matches!(&creds[0], Credential::BasicAuth(b) if b.username.as_ref().unwrap().value == "u")
        );
        assert!(matches!(&creds[1], Credential::Other { kind, .. } if kind == "x-future"));
        let out = write(&h).unwrap();
        let again = parse(&out, &Limits::default()).unwrap();
        assert_eq!(again, h);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("\"type\": \"x-future\""));
        assert!(text.contains("\"type\": \"basic-auth\""));
    }

    #[test]
    fn caps_and_bad_ids_are_refused() {
        let small = Limits {
            max_bytes: 10,
            ..Limits::default()
        };
        assert!(matches!(
            parse(b"{\"version\":1}xxxxxxxxx", &small),
            Err(CxfError::TooLarge { .. })
        ));
        let bad_id = r#"{"version":1,"exporterRpId":"x","exporterDisplayName":"x","timestamp":1,
            "accounts":[{"id":"a","items":[{"id":"not base64url!","title":"t","credentials":[]}]}]}"#;
        assert!(matches!(
            parse(bad_id.as_bytes(), &Limits::default()),
            Err(CxfError::BadBase64(_))
        ));
        let one_item = Limits {
            max_items: 1,
            ..Limits::default()
        };
        let two = r#"{"version":1,"exporterRpId":"x","exporterDisplayName":"x","timestamp":1,
            "accounts":[{"id":"a","items":[{"id":"YQ","title":"t"},{"id":"Yg","title":"u"}]}]}"#;
        assert!(matches!(
            parse(two.as_bytes(), &one_item),
            Err(CxfError::TooMany { what: "items", .. })
        ));
        assert!(matches!(
            parse(b"not json", &Limits::default()),
            Err(CxfError::Parse { .. })
        ));
    }
}
