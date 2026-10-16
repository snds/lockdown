//! CXF ↔ Lockdown items.
//!
//! Each CXF item becomes one [`ImportedItem`]: a kind, a title, plain
//! fields, and a report of anything that did not map. Going the other way,
//! an [`ImportedItem`] becomes a CXF item with one credential per thing it
//! holds. The "never lost in a round trip" list in 02-model §11 is the
//! target; what this slice guarantees is in the tests.

use lockdown_vault::{FieldKind, FieldValue, ItemKind, PlainField};

use crate::model::{
    Account, Address, BasicAuth, CXF_VERSION, Credential, CredentialScope, CreditCard,
    EditableField, Header, Item, Note, Passkey, PersonName, Totp, b64url, from_b64url,
};
use crate::{CxfError, Result};

/// An item in Lockdown's shape, before it is sealed into a vault.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ImportedItem {
    /// A stable id from the source, base64url, so a re-import merges.
    pub source_id: String,
    /// The type.
    pub kind: ItemKind,
    /// The title.
    pub title: String,
    /// The fields.
    pub fields: Vec<PlainField>,
    /// Favourite.
    pub favourite: bool,
    /// Tags.
    pub tags: Vec<String>,
    /// Created and modified, seconds since the epoch.
    pub created_at: Option<u64>,
    /// Modified.
    pub modified_at: Option<u64>,
    /// Credentials or values that did not map, in words.
    pub report: Vec<String>,
}

fn text(id: &str, label: &str, f: &Option<EditableField>) -> Option<PlainField> {
    f.as_ref().map(|f| PlainField::text(id, label, &f.value))
}

fn hidden(id: &str, label: &str, f: &Option<EditableField>) -> Option<PlainField> {
    f.as_ref().map(|f| PlainField::hidden(id, label, &f.value))
}

fn kind_of(creds: &[Credential]) -> ItemKind {
    if creds.iter().any(|c| matches!(c, Credential::BasicAuth(_))) {
        return ItemKind::Login;
    }
    if creds.iter().any(|c| matches!(c, Credential::Passkey(_))) {
        return ItemKind::Login;
    }
    match creds.first() {
        Some(Credential::CreditCard(_)) => ItemKind::Card,
        Some(Credential::Note(_)) => ItemKind::Note,
        Some(Credential::Totp(_)) => ItemKind::Code,
        Some(Credential::PersonName(_)) | Some(Credential::Address(_)) => ItemKind::Identity,
        _ => ItemKind::Custom,
    }
}

/// Turns a CXF document into Lockdown items.
pub fn to_items(header: &Header) -> Result<Vec<ImportedItem>> {
    let mut out = Vec::new();
    for account in &header.accounts {
        for item in &account.items {
            out.push(to_item(item)?);
        }
    }
    Ok(out)
}

fn to_item(item: &Item) -> Result<ImportedItem> {
    let mut fields = vec![PlainField::text("title", "title", &item.title)];
    let mut report = Vec::new();
    if let Some(scope) = &item.scope {
        for (i, url) in scope.urls.iter().enumerate() {
            fields.push(PlainField::url(&format!("url.{i}"), url, "exact_host"));
        }
        if !scope.android_apps.is_empty() {
            report.push(format!(
                "{} Android app scopes not kept yet",
                scope.android_apps.len()
            ));
        }
    }
    let mut passkeys = 0;
    let mut codes = 0;
    for cred in &item.credentials {
        match cred {
            Credential::BasicAuth(b) => {
                fields.extend(text("user", "username", &b.username));
                fields.extend(hidden("pw", "password", &b.password));
            }
            Credential::Passkey(p) => {
                let n = passkeys;
                passkeys += 1;
                let key = from_b64url(&p.key)?;
                let credential_id = from_b64url(&p.credential_id)?;
                let user_handle = from_b64url(&p.user_handle)?;
                fields.push(PlainField::text(
                    &format!("passkey.{n}.rp_id"),
                    "relying party",
                    &p.rp_id,
                ));
                fields.push(PlainField::text(
                    &format!("passkey.{n}.user_name"),
                    "user name",
                    &p.user_name,
                ));
                fields.push(PlainField::text(
                    &format!("passkey.{n}.user_display_name"),
                    "display name",
                    &p.user_display_name,
                ));
                fields.push(PlainField::new(
                    format!("passkey.{n}.credential_id"),
                    FieldKind::Passkey,
                    "credential id",
                    FieldValue::Bytes(credential_id),
                ));
                fields.push(PlainField::new(
                    format!("passkey.{n}.user_handle"),
                    FieldKind::Passkey,
                    "user handle",
                    FieldValue::Bytes(user_handle),
                ));
                fields.push(PlainField::new(
                    format!("passkey.{n}.key"),
                    FieldKind::Passkey,
                    "private key",
                    FieldValue::Bytes(key),
                ));
                if p.fido2_extensions.is_some() {
                    report.push(format!(
                        "passkey {n}: FIDO2 extension data kept only in the CXF copy"
                    ));
                }
            }
            Credential::Note(n) => fields.push(PlainField::text("body", "note", &n.content.value)),
            Credential::CreditCard(c) => {
                fields.push(PlainField::hidden("card.number", "number", &c.number.value));
                fields.extend(text("card.holder", "holder", &c.full_name));
                fields.extend(text("card.type", "type", &c.card_type));
                fields.extend(hidden("card.cvc", "security code", &c.verification_number));
                fields.extend(hidden("card.pin", "PIN", &c.pin));
                if let Some(e) = &c.expiry_date {
                    fields.push(PlainField::new(
                        "card.expiry",
                        FieldKind::Date,
                        "expiry",
                        FieldValue::Text(e.value.clone()),
                    ));
                }
                if let Some(v) = &c.valid_from {
                    fields.push(PlainField::new(
                        "card.valid_from",
                        FieldKind::Date,
                        "valid from",
                        FieldValue::Text(v.value.clone()),
                    ));
                }
            }
            Credential::Totp(t) => {
                let n = codes;
                codes += 1;
                let uri = totp_to_uri(t);
                fields.push(PlainField::new(
                    format!("code.{n}"),
                    FieldKind::Totp,
                    "one-time code",
                    FieldValue::Text(uri),
                ));
            }
            Credential::PersonName(p) => {
                fields.extend(text("name.title", "title", &p.title));
                fields.extend(text("name.given", "given name", &p.given));
                fields.extend(text("name.middle", "middle name", &p.middle));
                fields.extend(text("name.family", "family name", &p.family));
                fields.extend(text("name.generation", "suffix", &p.generation));
            }
            Credential::Address(a) => {
                fields.extend(text("address.street", "street", &a.street_address));
                fields.extend(text("address.city", "city", &a.city));
                fields.extend(text("address.territory", "state or region", &a.territory));
                fields.extend(text("address.country", "country", &a.country));
                fields.extend(text("address.postal_code", "postal code", &a.postal_code));
                fields.extend(text("address.tel", "phone", &a.tel));
            }
            Credential::Other { kind, .. } => {
                report.push(format!("credential type '{kind}' isn't imported yet"))
            }
        }
    }
    Ok(ImportedItem {
        source_id: item.id.clone(),
        kind: kind_of(&item.credentials),
        title: item.title.clone(),
        fields,
        favourite: item.favorite.unwrap_or(false),
        tags: item.tags.clone(),
        created_at: item.creation_at,
        modified_at: item.modified_at,
        report,
    })
}

fn totp_to_uri(t: &Totp) -> String {
    let mut code = match lockdown_codes::Secret::from_base32(&t.secret) {
        Ok(s) => lockdown_codes::Code::totp(s),
        Err(_) => lockdown_codes::Code::totp(lockdown_codes::Secret::from_bytes(
            t.secret.clone().into_bytes(),
        )),
    };
    if let Some(a) = t
        .algorithm
        .as_deref()
        .and_then(|a| lockdown_codes::Algorithm::parse(a).ok())
    {
        code.algorithm = a;
    }
    if let Some(d) = t.digits {
        code.digits = d;
    }
    if let Some(p) = t.period {
        code.period = p;
    }
    code.issuer = t.issuer.clone();
    code.account = t.username.clone();
    code.to_otpauth()
}

fn uri_to_totp(uri: &str) -> Option<Totp> {
    let code = lockdown_codes::parse_uri(uri).ok()?;
    Some(Totp {
        secret: code.secret.to_base32(),
        period: Some(code.period),
        digits: Some(code.digits),
        username: code.account.clone(),
        algorithm: Some(code.algorithm.as_str().to_ascii_lowercase()),
        issuer: code.issuer.clone(),
    })
}

fn field<'a>(fields: &'a [PlainField], id: &str) -> Option<&'a PlainField> {
    fields.iter().find(|f| f.field_id == id)
}

fn field_text(fields: &[PlainField], id: &str) -> Option<String> {
    match &field(fields, id)?.value {
        FieldValue::Text(t) => Some(t.clone()),
        _ => None,
    }
}

fn field_bytes(fields: &[PlainField], id: &str) -> Option<Vec<u8>> {
    match &field(fields, id)?.value {
        FieldValue::Bytes(b) => Some(b.clone()),
        _ => None,
    }
}

/// Turns Lockdown items into a CXF document under one account.
pub fn from_items(
    items: &[ImportedItem],
    exporter_rp_id: &str,
    exporter_display_name: &str,
    now: u64,
) -> Result<Header> {
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        out.push(from_item(item)?);
    }
    Ok(Header {
        version: CXF_VERSION,
        exporter_rp_id: exporter_rp_id.into(),
        exporter_display_name: exporter_display_name.into(),
        timestamp: now,
        accounts: vec![Account {
            id: b64url(b"lockdown"),
            items: out,
            ..Account::default()
        }],
    })
}

fn from_item(item: &ImportedItem) -> Result<Item> {
    let f = &item.fields;
    let mut credentials = Vec::new();
    let urls: Vec<String> = f
        .iter()
        .filter_map(|p| match &p.value {
            FieldValue::Url { value, .. } => Some(value.clone()),
            _ => None,
        })
        .collect();
    if field(f, "user").is_some() || field(f, "pw").is_some() {
        credentials.push(Credential::BasicAuth(BasicAuth {
            username: field_text(f, "user").map(EditableField::string),
            password: field_text(f, "pw").map(EditableField::concealed),
        }));
    }
    for n in 0.. {
        let prefix = format!("passkey.{n}.");
        let Some(key) = field_bytes(f, &format!("{prefix}key")) else {
            break;
        };
        let credential_id =
            field_bytes(f, &format!("{prefix}credential_id")).ok_or_else(|| CxfError::Parse {
                format: "item",
                why: "passkey without a credential id".into(),
            })?;
        let user_handle = field_bytes(f, &format!("{prefix}user_handle")).unwrap_or_default();
        credentials.push(Credential::Passkey(Passkey {
            credential_id: b64url(&credential_id),
            rp_id: field_text(f, &format!("{prefix}rp_id")).unwrap_or_default(),
            user_name: field_text(f, &format!("{prefix}user_name")).unwrap_or_default(),
            user_display_name: field_text(f, &format!("{prefix}user_display_name"))
                .unwrap_or_default(),
            user_handle: b64url(&user_handle),
            key: b64url(&key),
            fido2_extensions: None,
        }));
    }
    for n in 0.. {
        let Some(uri) = field_text(f, &format!("code.{n}")) else {
            break;
        };
        if let Some(t) = uri_to_totp(&uri) {
            credentials.push(Credential::Totp(t));
        }
    }
    if let Some(body) = field_text(f, "body") {
        credentials.push(Credential::Note(Note {
            content: EditableField::string(body),
        }));
    }
    if let Some(number) = field_text(f, "card.number") {
        credentials.push(Credential::CreditCard(CreditCard {
            number: EditableField::concealed(number),
            full_name: field_text(f, "card.holder").map(EditableField::string),
            card_type: field_text(f, "card.type").map(EditableField::string),
            verification_number: field_text(f, "card.cvc").map(EditableField::concealed),
            pin: field_text(f, "card.pin").map(EditableField::concealed),
            expiry_date: field_text(f, "card.expiry")
                .map(|v| EditableField::typed("year-month", v)),
            valid_from: field_text(f, "card.valid_from")
                .map(|v| EditableField::typed("year-month", v)),
        }));
    }
    if f.iter().any(|p| p.field_id.starts_with("name.")) {
        credentials.push(Credential::PersonName(PersonName {
            title: field_text(f, "name.title").map(EditableField::string),
            given: field_text(f, "name.given").map(EditableField::string),
            middle: field_text(f, "name.middle").map(EditableField::string),
            family: field_text(f, "name.family").map(EditableField::string),
            generation: field_text(f, "name.generation").map(EditableField::string),
        }));
    }
    if f.iter().any(|p| p.field_id.starts_with("address.")) {
        credentials.push(Credential::Address(Address {
            street_address: field_text(f, "address.street").map(EditableField::string),
            city: field_text(f, "address.city").map(EditableField::string),
            territory: field_text(f, "address.territory")
                .map(|v| EditableField::typed("subdivision-code", v)),
            country: field_text(f, "address.country")
                .map(|v| EditableField::typed("country-code", v)),
            postal_code: field_text(f, "address.postal_code").map(EditableField::string),
            tel: field_text(f, "address.tel").map(EditableField::string),
        }));
    }
    Ok(Item {
        id: item.source_id.clone(),
        creation_at: item.created_at,
        modified_at: item.modified_at,
        title: item.title.clone(),
        subtitle: None,
        favorite: if item.favourite { Some(true) } else { None },
        scope: if urls.is_empty() {
            None
        } else {
            Some(CredentialScope {
                urls,
                android_apps: vec![],
            })
        },
        credentials,
        tags: item.tags.clone(),
        extensions: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Limits;
    use crate::model::{parse, write};

    fn sample() -> Header {
        Header {
            version: 1,
            exporter_rp_id: "other.example".into(),
            exporter_display_name: "Other Manager".into(),
            timestamp: 1_760_000_000,
            accounts: vec![Account {
                id: b64url(b"acct"),
                username: "me".into(),
                email: "me@example.com".into(),
                items: vec![
                    Item {
                        id: b64url(b"login-1"),
                        title: "Example".into(),
                        favorite: Some(true),
                        scope: Some(CredentialScope {
                            urls: vec!["https://accounts.example.com".into()],
                            android_apps: vec![],
                        }),
                        credentials: vec![
                            Credential::BasicAuth(BasicAuth {
                                username: Some(EditableField::string("me@example.com")),
                                password: Some(EditableField::concealed("correct horse")),
                            }),
                            Credential::Passkey(Passkey {
                                credential_id: b64url(&[1, 2, 3, 4]),
                                rp_id: "example.com".into(),
                                user_name: "me".into(),
                                user_display_name: "Me".into(),
                                user_handle: b64url(&[9, 9]),
                                key: b64url(&[7u8; 48]),
                                fido2_extensions: None,
                            }),
                            Credential::Totp(Totp {
                                secret: "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ".into(),
                                period: Some(30),
                                digits: Some(6),
                                username: Some("me".into()),
                                algorithm: Some("sha1".into()),
                                issuer: Some("Example".into()),
                            }),
                        ],
                        tags: vec!["work".into()],
                        ..Item::default()
                    },
                    Item {
                        id: b64url(b"note-1"),
                        title: "Wi-Fi guest".into(),
                        credentials: vec![Credential::Note(Note {
                            content: EditableField::string("the guest code is 1234"),
                        })],
                        ..Item::default()
                    },
                    Item {
                        id: b64url(b"card-1"),
                        title: "Visa".into(),
                        credentials: vec![Credential::CreditCard(CreditCard {
                            number: EditableField::concealed("4111111111111111"),
                            full_name: Some(EditableField::string("A Person")),
                            card_type: Some(EditableField::string("Visa")),
                            verification_number: Some(EditableField::concealed("123")),
                            pin: None,
                            expiry_date: Some(EditableField::typed("year-month", "2030-01")),
                            valid_from: None,
                        })],
                        ..Item::default()
                    },
                    Item {
                        id: b64url(b"identity-1"),
                        title: "Me".into(),
                        credentials: vec![
                            Credential::PersonName(PersonName {
                                given: Some(EditableField::string("Ada")),
                                family: Some(EditableField::string("Lovelace")),
                                ..PersonName::default()
                            }),
                            Credential::Address(Address {
                                street_address: Some(EditableField::string("1 Example Street")),
                                city: Some(EditableField::string("London")),
                                country: Some(EditableField::typed("country-code", "GB")),
                                postal_code: Some(EditableField::string("N1 1AA")),
                                ..Address::default()
                            }),
                        ],
                        ..Item::default()
                    },
                ],
                ..Account::default()
            }],
        }
    }

    #[test]
    fn five_types_round_trip_through_lockdown_items() {
        let original = sample();
        let bytes = write(&original).unwrap();
        let parsed = parse(&bytes, &Limits::default()).unwrap();
        assert_eq!(parsed, original);

        let items = to_items(&parsed).unwrap();
        assert_eq!(items.len(), 4);
        assert_eq!(items[0].kind, ItemKind::Login);
        assert_eq!(items[1].kind, ItemKind::Note);
        assert_eq!(items[2].kind, ItemKind::Card);
        assert_eq!(items[3].kind, ItemKind::Identity);
        assert!(
            items.iter().all(|i| i.report.is_empty()),
            "{:?}",
            items.iter().map(|i| &i.report).collect::<Vec<_>>()
        );
        assert_eq!(
            field_bytes(&items[0].fields, "passkey.0.key").unwrap(),
            vec![7u8; 48]
        );
        assert!(
            field_text(&items[0].fields, "code.0")
                .unwrap()
                .starts_with("otpauth://totp/Example:me?")
        );
        assert!(items[0].favourite);

        let back = from_items(&items, "lockdown.app", "Lockdown", 1_760_000_001).unwrap();
        let round = &back.accounts[0].items;
        let orig = &original.accounts[0].items;
        for (a, b) in round.iter().zip(orig) {
            assert_eq!(a.id, b.id);
            assert_eq!(a.title, b.title);
            assert_eq!(a.scope, b.scope);
            assert_eq!(a.credentials, b.credentials, "item {}", b.title);
            assert_eq!(a.tags, b.tags);
            assert_eq!(a.favorite, b.favorite);
        }
        // And the whole thing parses again as CXF.
        let again = parse(&write(&back).unwrap(), &Limits::default()).unwrap();
        assert_eq!(again.accounts[0].items, back.accounts[0].items);
        assert_eq!(again.exporter_rp_id, "lockdown.app");
    }

    #[test]
    fn unmapped_credentials_are_reported_not_dropped() {
        let mut h = sample();
        h.accounts[0].items[1].credentials.push(Credential::Other {
            kind: "ssh-key".into(),
            raw: serde_json::json!({"type": "ssh-key", "privateKey": {"fieldType": "concealed-string", "value": "x"}}),
        });
        let items = to_items(&h).unwrap();
        assert_eq!(
            items[1].report,
            vec!["credential type 'ssh-key' isn't imported yet".to_string()]
        );
    }
}
