//! CXF ↔ Lockdown items.
//!
//! Each CXF item becomes one [`ImportedItem`]: a kind, a title, plain
//! fields, and a report of anything that did not map. Going the other way,
//! an [`ImportedItem`] becomes a CXF item with one credential per thing it
//! holds. The "never lost in a round trip" list in 02-model §11 is the
//! target; what this slice guarantees is in the tests.

use lockdown_vault::{FieldKind, FieldValue, Id, ItemClass, ItemKind, ItemState, PlainField};
use serde_json::{Value, json};
use zeroize::Zeroizing;

use crate::model::{
    Account, Address, BasicAuth, CXF_VERSION, Credential, CredentialScope, CreditCard,
    DriversLicense, EditableField, File, Header, IdentityDocument, Item, Note, Passkey, Passport,
    PersonName, Totp, Wifi, b64url, from_b64url,
};
use crate::{CxfError, Result};
use lockdown_vault::TEMPLATE_FIELD;

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
    /// The folder path in the source (`Work/Servers`), if any.
    pub folder: Option<String>,
    /// Archived in the source: out of everyday lists, kept.
    pub archived: bool,
    /// Files attached in the source.
    pub attachments: Vec<ImportedAttachment>,
    /// Earlier versions in the source, oldest first; `fields` is the newest.
    pub history: Vec<Vec<PlainField>>,
    /// Only from a Lockdown backup: the item's own id, class and state, so
    /// a restore puts it back as it was and a second restore merges.
    pub kept: Option<Kept>,
    /// The source holds something crown-jewel by default that no
    /// template field names (a Steam identity secret, 02-model §3), so the
    /// item lands in the crown-jewel class.
    pub crown_jewel: bool,
}

/// What a Lockdown backup keeps about an item beyond its fields.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Kept {
    /// The item's id in the vault it came from.
    pub item_id: Id,
    /// Standard or crown jewel.
    pub class: ItemClass,
    /// Live, archived or trashed.
    pub state: ItemState,
}

/// A file that came with an item.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ImportedAttachment {
    /// Its name.
    pub name: String,
    /// Its media type, from the source or guessed from the name.
    pub mime: String,
    /// Its bytes, wiped when dropped.
    pub bytes: Zeroizing<Vec<u8>>,
}

impl ImportedItem {
    /// An item with a title field and nothing else yet.
    pub fn new(source_id: impl Into<String>, kind: ItemKind, title: impl Into<String>) -> Self {
        let title = title.into();
        Self {
            source_id: source_id.into(),
            kind,
            fields: vec![PlainField::text("title", "title", &title)],
            title,
            favourite: false,
            tags: Vec::new(),
            created_at: None,
            modified_at: None,
            report: Vec::new(),
            folder: None,
            archived: false,
            attachments: Vec::new(),
            history: Vec::new(),
            kept: None,
            crown_jewel: false,
        }
    }
}

/// A media type from a file name, for sources that don't say.
pub fn mime_for(name: &str) -> &'static str {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "heic" => "image/heic",
        "txt" | "md" => "text/plain",
        "json" => "application/json",
        "zip" => "application/zip",
        "pem" | "key" | "pub" => "application/x-pem-file",
        _ => "application/octet-stream",
    }
}

fn text(id: &str, label: &str, f: &Option<EditableField>) -> Option<PlainField> {
    f.as_ref().map(|f| PlainField::text(id, label, &f.value))
}

fn hidden(id: &str, label: &str, f: &Option<EditableField>) -> Option<PlainField> {
    f.as_ref().map(|f| PlainField::hidden(id, label, &f.value))
}

fn kind_of(creds: &[Credential]) -> ItemKind {
    if creds.iter().any(|c| matches!(c, Credential::Wifi(_))) {
        return ItemKind::WiFi;
    }
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
        Some(Credential::PersonName(_))
        | Some(Credential::Address(_))
        | Some(Credential::DriversLicense(_))
        | Some(Credential::Passport(_))
        | Some(Credential::IdentityDocument(_)) => ItemKind::Identity,
        _ => ItemKind::Custom,
    }
}

/// CXF's Wi-Fi security types and the template's choices, where one
/// matches the other exactly. A choice CXF has no word for (WPA2/WPA3,
/// Enterprise) rides in the Lockdown extension as well.
const WIFI_TYPES: [(&str, &str); 5] = [
    ("WPA3", "wpa3-personal"),
    ("WPA2", "wpa2-personal"),
    ("WPA", "wpa-personal"),
    ("WEP", "wep"),
    ("Open", "unsecured"),
];

fn date(id: &str, label: &str, f: &Option<EditableField>) -> Option<PlainField> {
    f.as_ref().map(|f| {
        PlainField::new(
            id,
            FieldKind::Date,
            label,
            FieldValue::Text(f.value.clone()),
        )
    })
}

/// A full name split for the two name fields: the last word is the surname.
fn split_name(fields: &mut Vec<PlainField>, full: &Option<EditableField>) {
    let Some(full) = full
        .as_ref()
        .map(|f| f.value.trim())
        .filter(|v| !v.is_empty())
    else {
        return;
    };
    match full.rsplit_once(' ') {
        Some((given, family)) => {
            fields.push(PlainField::text("name.given", "Given names", given.trim()));
            fields.push(PlainField::text("name.family", "Surname", family));
        }
        None => fields.push(PlainField::text("name.family", "Surname", full)),
    }
}

fn variant(fields: &mut Vec<PlainField>, name: &str) {
    if !fields.iter().any(|f| f.field_id == TEMPLATE_FIELD) {
        fields.push(PlainField::text(TEMPLATE_FIELD, "Template", name));
    }
}

/// Turns a CXF document into Lockdown items.
pub fn to_items(header: &Header) -> Result<Vec<ImportedItem>> {
    let mut out = Vec::new();
    for account in &header.accounts {
        // An item's folder: the path of the collection that lists it.
        let mut folder_of: std::collections::BTreeMap<&str, String> = Default::default();
        fn walk<'a>(
            c: &'a crate::model::Collection,
            path: &str,
            depth: usize,
            out: &mut std::collections::BTreeMap<&'a str, String>,
        ) {
            if depth > 32 {
                return;
            }
            let here = if path.is_empty() {
                c.title.clone()
            } else {
                format!("{path}/{}", c.title)
            };
            for l in &c.items {
                out.entry(l.item.as_str()).or_insert_with(|| here.clone());
            }
            for sub in &c.sub_collections {
                walk(sub, &here, depth + 1, out);
            }
        }
        for c in &account.collections {
            walk(c, "", 0, &mut folder_of);
        }
        for item in &account.items {
            let mut it = to_item(item)?;
            it.folder = folder_of.get(item.id.as_str()).cloned();
            out.push(it);
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
            Credential::Wifi(w) => {
                fields.extend(text("wifi.ssid", "Network name", &w.ssid));
                fields.extend(hidden("wifi.passphrase", "Password", &w.passphrase));
                if let Some(t) = &w.network_security_type {
                    match WIFI_TYPES.iter().find(|(_, cxf)| *cxf == t.value) {
                        Some((ours, _)) => {
                            fields.push(PlainField::text("wifi.security", "Security", ours))
                        }
                        None => report.push(format!(
                            "the Wi-Fi security type '{}' has no match here and was left out",
                            t.value
                        )),
                    }
                }
                if let Some(h) = &w.hidden {
                    fields.push(PlainField::new(
                        "wifi.hidden",
                        FieldKind::Boolean,
                        "Hidden network",
                        FieldValue::Boolean(h.value == "true"),
                    ));
                }
            }
            Credential::DriversLicense(d) => {
                variant(&mut fields, "driver-licence");
                fields.extend(hidden("dl.number", "Licence number", &d.license_number));
                fields.extend(text("dl.class", "Categories", &d.license_class));
                fields.extend(text("dl.country", "Country", &d.country));
                fields.extend(text("dl.territory", "State or region", &d.territory));
                fields.extend(text("dl.authority", "Issued by", &d.issuing_authority));
                fields.extend(date("dl.issued", "Issued", &d.issue_date));
                fields.extend(date("expires_at", "Expires", &d.expiry_date));
                fields.extend(date("dl.birth_date", "Date of birth", &d.birth_date));
                split_name(&mut fields, &d.full_name);
            }
            Credential::Passport(p) => {
                variant(&mut fields, "passport");
                fields.extend(hidden("passport.number", "Passport number", &p.passport_number));
                fields.extend(text("passport.type", "Type", &p.passport_type));
                fields.extend(text("passport.country", "Issuing country", &p.issuing_country));
                fields.extend(text("passport.authority", "Issuing authority", &p.issuing_authority));
                fields.extend(date("passport.issued", "Issued", &p.issue_date));
                fields.extend(date("expires_at", "Expires", &p.expiry_date));
                fields.extend(text("passport.nationality", "Nationality", &p.nationality));
                fields.extend(date("passport.birth_date", "Date of birth", &p.birth_date));
                fields.extend(text("passport.birth_place", "Place of birth", &p.birth_place));
                fields.extend(text("passport.sex", "Sex", &p.sex));
                fields.extend(text(
                    "passport.national_id",
                    "Personal number",
                    &p.national_identification_number,
                ));
                split_name(&mut fields, &p.full_name);
            }
            Credential::IdentityDocument(d) => {
                variant(&mut fields, "national-id");
                fields.extend(hidden("id.number", "Number", &d.document_number));
                fields.extend(text("id.personal_number", "Personal number", &d.identification_number));
                fields.extend(text("id.country", "Issuing country", &d.issuing_country));
                fields.extend(text("id.authority", "Issued by", &d.issuing_authority));
                fields.extend(date("id.issued", "Issued", &d.issue_date));
                fields.extend(date("expires_at", "Expires", &d.expiry_date));
                fields.extend(text("id.nationality", "Nationality", &d.nationality));
                fields.extend(date("id.birth_date", "Date of birth", &d.birth_date));
                if d.birth_place.is_some() || d.sex.is_some() {
                    report.push("an ID document's place of birth and sex have no field here and were left out".into());
                }
                split_name(&mut fields, &d.full_name);
            }
            Credential::File(f) => report.push(format!(
                "the file '{}' ({} bytes) was named, but a CXF file holds a file's name and checksum, not its contents",
                f.name, f.decrypted_size
            )),
            Credential::Other { kind, .. } => {
                report.push(format!("credential type '{kind}' isn't imported yet"))
            }
        }
    }
    let mut kind = kind_of(&item.credentials);
    if let Some(ext) = lockdown_extension(item.extensions.as_ref()) {
        match read_extension(ext) {
            Ok((ext_kind, extra)) => {
                if let Some(k) = ext_kind {
                    kind = k;
                }
                for f in extra {
                    if !fields.iter().any(|p| p.field_id == f.field_id) {
                        fields.push(f);
                    }
                }
            }
            Err(why) => report.push(format!(
                "Lockdown's own fields didn't read ({why}) and were left out"
            )),
        }
    }
    Ok(ImportedItem {
        fields,
        favourite: item.favorite.unwrap_or(false),
        tags: item.tags.clone(),
        created_at: item.creation_at,
        modified_at: item.modified_at,
        report,
        ..ImportedItem::new(item.id.clone(), kind, item.title.clone())
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
    let mut collections: Vec<crate::model::Collection> = Vec::new();
    for item in items {
        out.push(from_item(item)?);
        if let Some(path) = item.folder.as_deref().filter(|p| !p.is_empty()) {
            let link = crate::model::LinkedItem {
                item: item.source_id.clone(),
                account: None,
            };
            match collections.iter_mut().find(|c| c.title == path) {
                Some(c) => c.items.push(link),
                None => collections.push(crate::model::Collection {
                    id: b64url(format!("folder:{path}").as_bytes()),
                    title: path.to_string(),
                    subtitle: None,
                    items: vec![link],
                    sub_collections: vec![],
                }),
            }
        }
    }
    Ok(Header {
        version: CXF_VERSION,
        exporter_rp_id: exporter_rp_id.into(),
        exporter_display_name: exporter_display_name.into(),
        timestamp: now,
        accounts: vec![Account {
            id: b64url(b"lockdown"),
            collections,
            items: out,
            ..Account::default()
        }],
    })
}

fn from_item(item: &ImportedItem) -> Result<Item> {
    let f = &item.fields;
    let mut used: Vec<String> = vec!["title".into()];
    let mut take = |id: String| {
        if f.iter().any(|p| p.field_id == id) {
            used.push(id);
        }
    };
    let mut credentials = Vec::new();
    let urls: Vec<String> = f
        .iter()
        .filter_map(|p| match &p.value {
            FieldValue::Url { value, .. } => Some(value.clone()),
            _ => None,
        })
        .collect();
    for p in f
        .iter()
        .filter(|p| matches!(p.value, FieldValue::Url { .. }))
    {
        take(p.field_id.clone());
    }
    if field(f, "user").is_some() || field(f, "pw").is_some() {
        credentials.push(Credential::BasicAuth(BasicAuth {
            username: field_text(f, "user").map(EditableField::string),
            password: field_text(f, "pw").map(EditableField::concealed),
        }));
        take("user".into());
        take("pw".into());
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
        for part in [
            "key",
            "credential_id",
            "user_handle",
            "rp_id",
            "user_name",
            "user_display_name",
        ] {
            take(format!("{prefix}{part}"));
        }
    }
    for n in 0.. {
        let id = format!("code.{n}");
        let Some(uri) = field_text(f, &id) else {
            break;
        };
        if let Some(t) = uri_to_totp(&uri) {
            credentials.push(Credential::Totp(t));
            take(id);
        }
    }
    if let Some(body) = field_text(f, "body") {
        credentials.push(Credential::Note(Note {
            content: EditableField::string(body),
        }));
        take("body".into());
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
        for part in [
            "number",
            "holder",
            "type",
            "cvc",
            "pin",
            "expiry",
            "valid_from",
        ] {
            take(format!("card.{part}"));
        }
    }
    // A document's holder goes in its own credential's full name (below);
    // the two name fields then ride in the Lockdown extension.
    let document = matches!(
        field_text(f, TEMPLATE_FIELD).as_deref(),
        Some("passport" | "driver-licence" | "national-id")
    );
    if !document && f.iter().any(|p| p.field_id.starts_with("name.")) {
        credentials.push(Credential::PersonName(PersonName {
            title: field_text(f, "name.title").map(EditableField::string),
            given: field_text(f, "name.given").map(EditableField::string),
            middle: field_text(f, "name.middle").map(EditableField::string),
            family: field_text(f, "name.family").map(EditableField::string),
            generation: field_text(f, "name.generation").map(EditableField::string),
        }));
        for part in ["title", "given", "middle", "family", "generation"] {
            take(format!("name.{part}"));
        }
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
        for part in [
            "street",
            "city",
            "territory",
            "country",
            "postal_code",
            "tel",
        ] {
            take(format!("address.{part}"));
        }
    }
    if let Some(ssid) = field_text(f, "wifi.ssid") {
        let security = field_text(f, "wifi.security");
        let exact = security
            .as_deref()
            .and_then(|s| WIFI_TYPES.iter().find(|(ours, _)| *ours == s));
        credentials.push(Credential::Wifi(Wifi {
            ssid: Some(EditableField::string(ssid)),
            network_security_type: exact
                .map(|(_, cxf)| EditableField::typed("wifi-network-security-type", *cxf))
                .or_else(|| {
                    // "WPA2/WPA3" joins on WPA2; Enterprise has no CXF word.
                    (security.as_deref() == Some("WPA2/WPA3")).then(|| {
                        EditableField::typed("wifi-network-security-type", "wpa2-personal")
                    })
                }),
            passphrase: field_text(f, "wifi.passphrase").map(EditableField::concealed),
            hidden: field(f, "wifi.hidden").and_then(|p| match p.value {
                FieldValue::Boolean(b) => Some(EditableField::typed("boolean", b.to_string())),
                _ => None,
            }),
        }));
        take("wifi.ssid".into());
        take("wifi.passphrase".into());
        take("wifi.hidden".into());
        if exact.is_some() {
            take("wifi.security".into());
        }
    }
    let marker = field_text(f, TEMPLATE_FIELD);
    let full_name = || -> Option<EditableField> {
        let parts: Vec<String> = [field_text(f, "name.given"), field_text(f, "name.family")]
            .into_iter()
            .flatten()
            .filter(|p| !p.trim().is_empty())
            .collect();
        (!parts.is_empty()).then(|| EditableField::string(parts.join(" ")))
    };
    let d = |id: &str| field_text(f, id).map(|v| EditableField::typed("date", v));
    let t = |id: &str| field_text(f, id).map(EditableField::string);
    match marker.as_deref() {
        Some("passport") => {
            credentials.push(Credential::Passport(Passport {
                issuing_country: t("passport.country"),
                passport_type: t("passport.type"),
                passport_number: field_text(f, "passport.number").map(EditableField::concealed),
                national_identification_number: t("passport.national_id"),
                nationality: t("passport.nationality"),
                full_name: full_name(),
                birth_date: d("passport.birth_date"),
                birth_place: t("passport.birth_place"),
                sex: t("passport.sex"),
                issue_date: d("passport.issued"),
                expiry_date: d("expires_at"),
                issuing_authority: t("passport.authority"),
            }));
            for id in [
                "passport.country",
                "passport.type",
                "passport.number",
                "passport.national_id",
                "passport.nationality",
                "passport.birth_date",
                "passport.birth_place",
                "passport.sex",
                "passport.issued",
                "passport.authority",
            ] {
                take(id.into());
            }
        }
        Some("driver-licence") => {
            credentials.push(Credential::DriversLicense(DriversLicense {
                full_name: full_name(),
                birth_date: d("dl.birth_date"),
                issue_date: d("dl.issued"),
                expiry_date: d("expires_at"),
                issuing_authority: t("dl.authority"),
                territory: field_text(f, "dl.territory")
                    .map(|v| EditableField::typed("subdivision-code", v)),
                country: field_text(f, "dl.country")
                    .map(|v| EditableField::typed("country-code", v)),
                license_number: field_text(f, "dl.number").map(EditableField::concealed),
                license_class: t("dl.class"),
            }));
            for id in [
                "dl.birth_date",
                "dl.issued",
                "dl.authority",
                "dl.territory",
                "dl.country",
                "dl.number",
                "dl.class",
            ] {
                take(id.into());
            }
        }
        Some("national-id") => {
            credentials.push(Credential::IdentityDocument(IdentityDocument {
                issuing_country: t("id.country"),
                document_number: field_text(f, "id.number").map(EditableField::concealed),
                identification_number: t("id.personal_number"),
                nationality: t("id.nationality"),
                full_name: full_name(),
                birth_date: d("id.birth_date"),
                birth_place: None,
                sex: None,
                issue_date: d("id.issued"),
                expiry_date: d("expires_at"),
                issuing_authority: t("id.authority"),
            }));
            for id in [
                "id.country",
                "id.number",
                "id.personal_number",
                "id.nationality",
                "id.birth_date",
                "id.issued",
                "id.authority",
            ] {
                take(id.into());
            }
        }
        _ => {}
    }
    // Attached files as CXF `file` credentials: name, size and SHA-256.
    // The bytes have no place in a CXF document (see `File`); the export's
    // report says so, and a Lockdown backup carries them.
    for a in &item.attachments {
        let digest = lockdown_crypto::hash::sha256(&a.bytes);
        credentials.push(Credential::File(File {
            id: b64url(&lockdown_crypto::hash::hash(&digest).as_bytes()[..16]),
            name: a.name.clone(),
            decrypted_size: a.bytes.len() as u64,
            integrity_hash: Some(b64url(&digest)),
        }));
    }
    let rest: Vec<&PlainField> = f.iter().filter(|p| !used.contains(&p.field_id)).collect();
    let extensions = if rest.is_empty() && kind_of(&credentials) == item.kind {
        None
    } else {
        Some(write_extension(item.kind, &rest))
    };
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
        extensions,
    })
}

/// The name of the item extension a Lockdown export carries (CXF §
/// "extensions"): the item's kind and every field no CXF credential type
/// holds (a Wi-Fi password, an SSH key, a recovery phrase, notes, fields a
/// person added by hand), so an export never drops a field (02-model §4,
/// §11) and an import from Lockdown gets the same item back.
pub const LOCKDOWN_EXTENSION: &str = "design.snds.lockdown.fields";

fn value_json(v: &FieldValue) -> Value {
    match v {
        FieldValue::Text(t) => json!({ "text": t }),
        FieldValue::Boolean(b) => json!({ "flag": b }),
        FieldValue::Url { value, match_rule } => json!({ "url": value, "matchRule": match_rule }),
        FieldValue::Bytes(b) => json!({ "bytes": b64url(b) }),
        FieldValue::Ref(id) => json!({ "ref": id.to_string() }),
    }
}

fn write_extension(kind: ItemKind, fields: &[&PlainField]) -> Value {
    let fields: Vec<Value> = fields
        .iter()
        .map(|p| {
            json!({
                "id": p.field_id,
                "kind": serde_json::to_value(p.kind).unwrap_or(Value::Null),
                "label": p.label,
                "section": p.section,
                "value": value_json(&p.value),
            })
        })
        .collect();
    json!([{ "name": LOCKDOWN_EXTENSION, "kind": kind.as_str(), "fields": fields }])
}

fn lockdown_extension(ext: Option<&Value>) -> Option<&Value> {
    ext?.as_array()?
        .iter()
        .find(|e| e.get("name").and_then(Value::as_str) == Some(LOCKDOWN_EXTENSION))
}

fn read_value(v: &Value) -> std::result::Result<FieldValue, String> {
    let str_of = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    if let Some(t) = str_of("text") {
        return Ok(FieldValue::Text(t));
    }
    if let Some(b) = v.get("flag").and_then(Value::as_bool) {
        return Ok(FieldValue::Boolean(b));
    }
    if let Some(url) = str_of("url") {
        return Ok(FieldValue::Url {
            value: url,
            match_rule: str_of("matchRule").unwrap_or_else(|| "exact_host".into()),
        });
    }
    if let Some(b) = str_of("bytes") {
        return from_b64url(&b)
            .map(FieldValue::Bytes)
            .map_err(|e| e.to_string());
    }
    if let Some(r) = str_of("ref") {
        return lockdown_vault::Id::parse(&r)
            .map(FieldValue::Ref)
            .map_err(|_| "a link to another item isn't an id".to_string());
    }
    Err("a value of no known shape".into())
}

fn read_extension(ext: &Value) -> std::result::Result<(Option<ItemKind>, Vec<PlainField>), String> {
    let kind = match ext.get("kind").and_then(Value::as_str) {
        Some(name) => ItemKind::ALL
            .into_iter()
            .find(|k| k.as_str() == name && !k.hidden()),
        None => None,
    };
    let mut out = Vec::new();
    for f in ext
        .get("fields")
        .and_then(Value::as_array)
        .ok_or("no field list")?
    {
        let id = f
            .get("id")
            .and_then(Value::as_str)
            .ok_or("a field without an id")?;
        if id == "title" {
            continue;
        }
        let kind: FieldKind = serde_json::from_value(f.get("kind").cloned().unwrap_or(Value::Null))
            .map_err(|_| format!("'{id}' has a kind this build doesn't know"))?;
        let label = f.get("label").and_then(Value::as_str).unwrap_or(id);
        let value = read_value(f.get("value").unwrap_or(&Value::Null))
            .map_err(|e| format!("'{id}': {e}"))?;
        let mut field = PlainField::new(id, kind, label, value);
        field.section = f.get("section").and_then(Value::as_str).map(str::to_string);
        out.push(field);
    }
    Ok((kind, out))
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
    fn fields_no_credential_holds_ride_in_the_lockdown_extension_both_ways() {
        let wifi = ImportedItem {
            fields: vec![
                PlainField::text("title", "title", "Home"),
                PlainField::text("wifi.ssid", "Network name", "home-net"),
                PlainField::hidden("wifi.passphrase", "Password", "k7Hq3mWx9rTa4nPe6uYc"),
                PlainField::new(
                    "wifi.hidden",
                    FieldKind::Boolean,
                    "Hidden network",
                    FieldValue::Boolean(true),
                ),
            ],
            ..ImportedItem::new(b64url(b"wifi-1"), ItemKind::WiFi, "Home")
        };
        let mut login = wifi.clone();
        login.source_id = b64url(b"login-1");
        login.kind = ItemKind::Login;
        login.fields = vec![
            PlainField::text("title", "title", "Forum"),
            PlainField::text("user", "Username", "me"),
            PlainField::hidden("pw", "Password", "pw"),
            PlainField::text("notes", "Notes", "changed in May"),
            PlainField::new(
                "custom.0",
                FieldKind::Hidden,
                "Door code",
                FieldValue::Text("1234".into()),
            ),
        ];
        let doc = from_items(
            &[wifi.clone(), login.clone()],
            "lockdown.app",
            "Lockdown",
            1,
        )
        .unwrap();
        let written = write(&doc).unwrap();
        let text = std::str::from_utf8(&written).unwrap();
        assert!(text.contains(LOCKDOWN_EXTENSION));
        let back = to_items(&parse(&written, &Limits::default()).unwrap()).unwrap();
        assert_eq!(
            back[0].kind,
            ItemKind::WiFi,
            "the kind comes back, not 'custom'"
        );
        assert_eq!(back[0].fields, wifi.fields, "every field, in order");
        assert_eq!(back[1].kind, ItemKind::Login);
        assert_eq!(
            field_text(&back[1].fields, "notes").as_deref(),
            Some("changed in May")
        );
        let door = back[1]
            .fields
            .iter()
            .find(|f| f.field_id == "custom.0")
            .unwrap();
        assert_eq!(
            (door.kind, door.label.as_str()),
            (FieldKind::Hidden, "Door code")
        );
        assert!(back.iter().all(|i| i.report.is_empty()));
        // A login the credentials hold whole carries no extension.
        let plain = from_items(
            &[ImportedItem {
                fields: login.fields[..3].to_vec(),
                ..login
            }],
            "x",
            "x",
            1,
        )
        .unwrap();
        assert!(plain.accounts[0].items[0].extensions.is_none());
        // A broken extension is reported, never fatal, never a guess.
        let mut h = doc.clone();
        h.accounts[0].items[0].extensions = Some(
            serde_json::json!([{ "name": LOCKDOWN_EXTENSION, "kind": "wi-fi", "fields": [{ "id": "x", "kind": "spaceship", "value": { "text": "a" } }] }]),
        );
        let items = to_items(&h).unwrap();
        assert!(
            items[0].report[0].contains("didn't read"),
            "{:?}",
            items[0].report
        );
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

    fn foreign(credentials: serde_json::Value) -> Header {
        let json = serde_json::json!({
            "version": 1, "exporterRpId": "other.example", "exporterDisplayName": "Other",
            "timestamp": 1,
            "accounts": [{"id": "YQ", "items": [{"id": "aXRlbQ", "title": "From elsewhere",
                "credentials": credentials}]}]
        });
        parse(json.to_string().as_bytes(), &Limits::default()).unwrap()
    }

    fn value_of<'a>(fields: &'a [PlainField], id: &str) -> Option<&'a FieldValue> {
        fields.iter().find(|f| f.field_id == id).map(|f| &f.value)
    }

    #[test]
    fn wifi_goes_out_in_cxfs_own_type_and_comes_back() {
        let mut net = ImportedItem::new(b64url(b"net-1"), ItemKind::WiFi, "Home");
        net.fields.extend([
            PlainField::text("wifi.ssid", "Network name", "home-net"),
            PlainField::hidden("wifi.passphrase", "Password", "k7Hq3mWx9rTa4nPe6uYc"),
            PlainField::text("wifi.security", "Security", "WPA2"),
            PlainField::new(
                "wifi.hidden",
                FieldKind::Boolean,
                "Hidden network",
                FieldValue::Boolean(true),
            ),
        ]);
        let mut enterprise = ImportedItem::new(b64url(b"net-2"), ItemKind::WiFi, "Office");
        enterprise.fields.extend([
            PlainField::text("wifi.ssid", "Network name", "corp"),
            PlainField::text("wifi.security", "Security", "Enterprise"),
            PlainField::text("wifi.eap_identity", "Enterprise identity", "sean"),
        ]);
        let header = from_items(
            &[net.clone(), enterprise.clone()],
            "lockdown.app",
            "Lockdown",
            1,
        )
        .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&write(&header).unwrap()).unwrap();
        let item = &json["accounts"][0]["items"][0];
        let wifi = &item["credentials"][0];
        assert_eq!(wifi["type"], "wifi");
        assert_eq!(wifi["ssid"]["value"], "home-net");
        assert_eq!(wifi["passphrase"]["fieldType"], "concealed-string");
        assert_eq!(wifi["networkSecurityType"]["value"], "wpa2-personal");
        assert_eq!(
            wifi["networkSecurityType"]["fieldType"],
            "wifi-network-security-type"
        );
        assert_eq!(wifi["hidden"]["value"], "true");
        // Everything CXF's type holds left the Lockdown extension; the
        // item's kind is all the extension still says.
        let ext = item["extensions"]
            .as_array()
            .map(|a| a[0]["fields"].as_array().unwrap().len());
        assert!(ext.is_none() || ext == Some(0), "{item}");
        // Enterprise has no CXF word: the type is left out, the choice rides along.
        let office = &json["accounts"][0]["items"][1];
        assert!(
            office["credentials"][0]
                .get("networkSecurityType")
                .is_none()
        );
        let back = to_items(&header).unwrap();
        assert_eq!(back[0].kind, ItemKind::WiFi);
        assert_eq!(
            value_of(&back[0].fields, "wifi.security"),
            Some(&FieldValue::Text("WPA2".into()))
        );
        assert_eq!(
            value_of(&back[0].fields, "wifi.hidden"),
            Some(&FieldValue::Boolean(true))
        );
        assert_eq!(
            value_of(&back[1].fields, "wifi.security"),
            Some(&FieldValue::Text("Enterprise".into()))
        );
        assert!(value_of(&back[1].fields, "wifi.eap_identity").is_some());
        // Another manager's Wi-Fi lands as a network.
        let other = to_items(&foreign(serde_json::json!([{
            "type": "wifi",
            "ssid": {"fieldType": "string", "value": "cafe"},
            "networkSecurityType": {"fieldType": "wifi-network-security-type", "value": "wpa3-personal"},
            "passphrase": {"fieldType": "concealed-string", "value": "latte-art"},
            "hidden": {"fieldType": "boolean", "value": "false"}
        }]))).unwrap();
        assert_eq!(other[0].kind, ItemKind::WiFi);
        assert_eq!(
            value_of(&other[0].fields, "wifi.security"),
            Some(&FieldValue::Text("WPA3".into()))
        );
        let pw = other[0]
            .fields
            .iter()
            .find(|f| f.field_id == "wifi.passphrase")
            .unwrap();
        assert_eq!(pw.kind, FieldKind::Hidden);
    }

    #[test]
    fn identity_documents_map_to_their_variants_both_ways() {
        let items = to_items(&foreign(serde_json::json!([
            {"type": "passport",
             "passportNumber": {"fieldType": "concealed-string", "value": "X1234567"},
             "issuingCountry": {"fieldType": "country-code", "value": "GB"},
             "fullName": {"fieldType": "string", "value": "Sean Q Sands"},
             "expiryDate": {"fieldType": "date", "value": "2034-05-01"},
             "sex": {"fieldType": "string", "value": "X"}}
        ])))
        .unwrap();
        let p = &items[0];
        assert_eq!(p.kind, ItemKind::Identity);
        assert_eq!(
            value_of(&p.fields, TEMPLATE_FIELD),
            Some(&FieldValue::Text("passport".into()))
        );
        assert_eq!(
            value_of(&p.fields, "name.given"),
            Some(&FieldValue::Text("Sean Q".into()))
        );
        assert_eq!(
            value_of(&p.fields, "name.family"),
            Some(&FieldValue::Text("Sands".into()))
        );
        assert_eq!(
            p.fields
                .iter()
                .find(|f| f.field_id == "passport.number")
                .unwrap()
                .kind,
            FieldKind::Hidden
        );
        assert_eq!(
            p.fields
                .iter()
                .find(|f| f.field_id == "expires_at")
                .unwrap()
                .kind,
            FieldKind::Date
        );

        let dl = to_items(&foreign(serde_json::json!([
            {"type": "drivers-license",
             "licenseNumber": {"fieldType": "concealed-string", "value": "SANDS901"},
             "licenseClass": {"fieldType": "string", "value": "B"},
             "territory": {"fieldType": "subdivision-code", "value": "CA"}}
        ])))
        .unwrap();
        assert_eq!(
            value_of(&dl[0].fields, TEMPLATE_FIELD),
            Some(&FieldValue::Text("driver-licence".into()))
        );
        assert_eq!(
            value_of(&dl[0].fields, "dl.class"),
            Some(&FieldValue::Text("B".into()))
        );

        let id = to_items(&foreign(serde_json::json!([
            {"type": "identity-document",
             "documentNumber": {"fieldType": "concealed-string", "value": "ID-77"},
             "identificationNumber": {"fieldType": "string", "value": "AB123456C"},
             "birthPlace": {"fieldType": "string", "value": "York"}}
        ])))
        .unwrap();
        assert_eq!(
            value_of(&id[0].fields, TEMPLATE_FIELD),
            Some(&FieldValue::Text("national-id".into()))
        );
        assert_eq!(
            value_of(&id[0].fields, "id.personal_number"),
            Some(&FieldValue::Text("AB123456C".into()))
        );
        assert!(
            id[0].report.iter().any(|r| r.contains("place of birth")),
            "{:?}",
            id[0].report
        );

        // Out again in CXF's own types, and back to the same variant.
        let header = from_items(
            &[p.clone(), dl[0].clone(), id[0].clone()],
            "lockdown.app",
            "Lockdown",
            1,
        )
        .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&write(&header).unwrap()).unwrap();
        let types: Vec<&str> = json["accounts"][0]["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["credentials"][0]["type"].as_str().unwrap())
            .collect();
        assert_eq!(
            types,
            vec!["passport", "drivers-license", "identity-document"]
        );
        let passport = &json["accounts"][0]["items"][0]["credentials"][0];
        assert_eq!(passport["fullName"]["value"], "Sean Q Sands");
        assert_eq!(passport["passportNumber"]["value"], "X1234567");
        let back = to_items(&header).unwrap();
        assert_eq!(
            value_of(&back[0].fields, TEMPLATE_FIELD),
            Some(&FieldValue::Text("passport".into()))
        );
        assert_eq!(
            value_of(&back[0].fields, "name.given"),
            Some(&FieldValue::Text("Sean Q".into()))
        );
    }

    #[test]
    fn files_are_named_with_their_checksum_and_never_carried() {
        let mut doc = ImportedItem::new(b64url(b"doc-1"), ItemKind::Document, "Lease");
        doc.attachments.push(ImportedAttachment {
            name: "lease.pdf".into(),
            mime: "application/pdf".into(),
            bytes: Zeroizing::new(b"%PDF-1.7 the lease".to_vec()),
        });
        let header = from_items(&[doc], "lockdown.app", "Lockdown", 1).unwrap();
        let json: serde_json::Value = serde_json::from_slice(&write(&header).unwrap()).unwrap();
        let file = &json["accounts"][0]["items"][0]["credentials"][0];
        assert_eq!(file["type"], "file");
        assert_eq!(file["name"], "lease.pdf");
        assert_eq!(file["decryptedSize"], 18);
        assert_eq!(
            file["integrityHash"],
            b64url(&lockdown_crypto::hash::sha256(b"%PDF-1.7 the lease"))
        );
        assert!(
            !json.to_string().contains("the lease\""),
            "no bytes in the document"
        );
        let back = to_items(&header).unwrap();
        assert!(back[0].attachments.is_empty());
        assert!(
            back[0].report[0].contains("lease.pdf"),
            "{:?}",
            back[0].report
        );
    }
}
