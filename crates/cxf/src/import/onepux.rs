//! 1Password's unencrypted export (`.1pux`): a ZIP with `export.data`
//! (JSON: accounts → vaults → items) and `files/<documentId>__<name>` for
//! documents and attachments. Each item has a category number, an overview
//! (title, URLs, tags) and details (login fields, sections of typed
//! fields, notes, password history).

use std::collections::BTreeMap;

use lockdown_vault::{FieldKind, ItemKind, PlainField};
use serde::Deserialize;
use serde_json::Value;
use zeroize::Zeroizing;

use super::{Parsed, json_err};
use crate::build::{Builder, date_from_unix, month_from};
use crate::{CxfError, Limits, Result, unzip};

#[derive(Deserialize)]
struct Export {
    #[serde(default)]
    accounts: Vec<Account>,
}

#[derive(Deserialize)]
struct Account {
    #[serde(default)]
    vaults: Vec<Vault>,
}

#[derive(Deserialize)]
struct Vault {
    #[serde(default)]
    attrs: VaultAttrs,
    #[serde(default)]
    items: Vec<Item>,
}

#[derive(Deserialize, Default)]
struct VaultAttrs {
    #[serde(default)]
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Item {
    #[serde(default)]
    uuid: String,
    #[serde(default)]
    fav_index: u64,
    #[serde(default)]
    created_at: Option<u64>,
    #[serde(default)]
    updated_at: Option<u64>,
    #[serde(default)]
    state: String,
    #[serde(default)]
    category_uuid: String,
    #[serde(default)]
    details: Details,
    #[serde(default)]
    overview: Overview,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Details {
    #[serde(default)]
    login_fields: Vec<LoginField>,
    #[serde(default)]
    notes_plain: Option<String>,
    #[serde(default)]
    sections: Vec<Section>,
    #[serde(default)]
    password_history: Vec<PasswordHistory>,
    #[serde(default)]
    document_attributes: Option<FileRef>,
    #[serde(default)]
    password: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoginField {
    #[serde(default)]
    value: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    field_type: String,
    #[serde(default)]
    designation: String,
}

#[derive(Deserialize)]
struct Section {
    #[serde(default)]
    title: String,
    #[serde(default)]
    fields: Vec<SectionField>,
}

#[derive(Deserialize)]
struct SectionField {
    #[serde(default)]
    title: String,
    #[serde(default)]
    id: String,
    #[serde(default)]
    value: Value,
}

#[derive(Deserialize)]
struct PasswordHistory {
    #[serde(default)]
    value: String,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct FileRef {
    #[serde(default)]
    file_name: String,
    #[serde(default)]
    document_id: String,
}

#[derive(Deserialize, Default)]
struct Overview {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    urls: Vec<OverviewUrl>,
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(Deserialize)]
struct OverviewUrl {
    #[serde(default)]
    url: String,
}

/// The Lockdown kind for a 1Password category number.
fn kind_of(category: &str) -> ItemKind {
    match category {
        "001" => ItemKind::Login,
        "002" => ItemKind::Card,
        "003" => ItemKind::Note,
        "004" => ItemKind::Identity,
        "005" => ItemKind::Login,
        "006" | "103" | "104" | "106" | "108" => ItemKind::Document,
        "100" => ItemKind::SoftwareLicence,
        "101" => ItemKind::BankAccount,
        "102" => ItemKind::Database,
        "105" | "107" => ItemKind::Membership,
        "109" => ItemKind::WiFi,
        "110" | "111" => ItemKind::Server,
        "112" => ItemKind::ApiCredential,
        "114" => ItemKind::SshKey,
        "115" => ItemKind::CryptoWallet,
        _ => ItemKind::Custom,
    }
}

/// Where a 1Password section field lands for a kind: the Lockdown field id.
fn field_for(kind: ItemKind, id: &str) -> Option<&'static str> {
    Some(match (kind, id) {
        (ItemKind::Card, "cardholder") => "card.holder",
        (ItemKind::Card, "ccnum") => "card.number",
        (ItemKind::Card, "cvv") => "card.cvc",
        (ItemKind::Card, "expiry") => "card.expiry",
        (ItemKind::Card, "type") => "card.type",
        (ItemKind::Card, "pin") => "card.pin",
        (ItemKind::Identity, "firstname") => "name.given",
        (ItemKind::Identity, "initial") => "name.middle",
        (ItemKind::Identity, "lastname") => "name.family",
        (ItemKind::Identity, "email") => "identity.email",
        (ItemKind::Identity, "defphone" | "cellphone") => "address.tel",
        (ItemKind::BankAccount, "bankName") => "bank.name",
        (ItemKind::BankAccount, "owner") => "bank.holder",
        (ItemKind::BankAccount, "accountNo") => "bank.account_number",
        (ItemKind::BankAccount, "routingNo") => "bank.routing",
        (ItemKind::BankAccount, "swift") => "bank.swift",
        (ItemKind::BankAccount, "iban") => "bank.iban",
        (ItemKind::BankAccount, "telephonePin") => "bank.pin",
        (ItemKind::BankAccount, "branchAddress") => "bank.branch",
        (ItemKind::Database, "database_type") => "db.type",
        (ItemKind::Database, "hostname") => "db.host",
        (ItemKind::Database, "port") => "db.port",
        (ItemKind::Database, "database") => "db.name",
        (ItemKind::Database, "username") => "user",
        (ItemKind::Database, "password") => "pw",
        (ItemKind::Database, "options") => "db.options",
        (ItemKind::Server, "url" | "pop_server" | "smtp_server") => "server.host",
        (ItemKind::Server, "pop_port" | "smtp_port") => "server.port",
        (ItemKind::Server, "username" | "pop_username") => "user",
        (ItemKind::Server, "password" | "pop_password") => "pw",
        (ItemKind::Server, "admin_console_url") => "server.admin",
        (ItemKind::SoftwareLicence, "reg_code") => "licence.key",
        (ItemKind::SoftwareLicence, "reg_name") => "licence.licensed_to",
        (ItemKind::SoftwareLicence, "reg_email") => "licence.email",
        (ItemKind::SoftwareLicence, "product_version") => "licence.version",
        (ItemKind::SoftwareLicence, "publisher_name") => "licence.publisher",
        (ItemKind::SoftwareLicence, "download_link") => "licence.url",
        (ItemKind::SoftwareLicence, "order_date") => "licence.purchased",
        (ItemKind::Membership, "org_name" | "company_name") => "member.organization",
        (ItemKind::Membership, "member_name") => "member.name",
        (ItemKind::Membership, "membership_no" | "member_id") => "member.number",
        (ItemKind::Membership, "pin") => "member.pin",
        (ItemKind::Membership, "member_since") => "member.since",
        (ItemKind::Membership, "expiry_date") => "expires_at",
        (ItemKind::Membership, "website") => "member.url",
        (ItemKind::WiFi, "network_name") => "wifi.ssid",
        (ItemKind::WiFi, "wireless_password") => "wifi.passphrase",
        (ItemKind::WiFi, "wireless_security") => "wifi.security",
        (ItemKind::ApiCredential, "credential") => "api.key",
        (ItemKind::ApiCredential, "type") => "api.type",
        (ItemKind::ApiCredential, "hostname") => "api.host",
        (ItemKind::ApiCredential, "expires") => "expires_at",
        (ItemKind::SshKey, "private_key") => "ssh.private_key",
        (ItemKind::CryptoWallet, "recoveryPhrase") => "seed",
        (ItemKind::CryptoWallet, "password") => "wallet.password",
        (ItemKind::CryptoWallet, "walletAddress") => "wallet.address",
        (ItemKind::Document, "number") => "doc.number",
        (ItemKind::Document, "issuing_authority" | "issuing_country" | "state") => "doc.issuer",
        (ItemKind::Document, "issue_date") => "doc.issued",
        (ItemKind::Document, "expiry_date") => "expires_at",
        _ => return None,
    })
}

/// A section value as text, the kind a field of your own would take, and
/// whether it is a code set-up.
fn value_of(v: &Value) -> Option<(String, FieldKind)> {
    let obj = v.as_object()?;
    let (key, inner) = obj.iter().next()?;
    let text = |x: &Value| match x {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    };
    Some(match key.as_str() {
        "concealed" | "creditCardNumber" => (text(inner)?, FieldKind::Hidden),
        "totp" => (text(inner)?, FieldKind::Totp),
        "url" => (text(inner)?, FieldKind::Url),
        "phone" => (text(inner)?, FieldKind::Phone),
        "email" => match inner {
            Value::Object(o) => (
                o.get("email_address").and_then(text).unwrap_or_default(),
                FieldKind::Email,
            ),
            other => (text(other)?, FieldKind::Email),
        },
        "date" => (
            inner.as_i64().map(date_from_unix).or_else(|| text(inner))?,
            FieldKind::Date,
        ),
        "monthYear" => {
            let n = text(inner)?;
            let m = if n.len() == 6 {
                format!("{}-{}", &n[..4], &n[4..])
            } else {
                n
            };
            (m, FieldKind::Text)
        }
        "address" => {
            let o = inner.as_object()?;
            let part = |k: &str| o.get(k).and_then(text).unwrap_or_default();
            let joined = [
                part("street"),
                part("city"),
                part("state"),
                part("zip"),
                part("country"),
            ]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(", ");
            (joined, FieldKind::Text)
        }
        "sshKey" => {
            let o = inner.as_object()?;
            (
                o.get("privateKey").and_then(text).unwrap_or_default(),
                FieldKind::Hidden,
            )
        }
        "file" => return None,
        _ => (text(inner)?, FieldKind::Text),
    })
}

/// Reads a `.1pux` file.
pub fn read(bytes: &[u8], limits: &Limits) -> Result<Parsed> {
    let entries = unzip::read(bytes, limits)?;
    let data = entries
        .iter()
        .find(|e| e.name == "export.data")
        .ok_or_else(|| CxfError::Parse {
            format: "1PUX",
            why: "there is no export.data inside; is this a 1Password export?".into(),
        })?;
    let export: Export = serde_json::from_slice(&data.bytes).map_err(json_err("1PUX"))?;
    let mut files: BTreeMap<&str, &unzip::Entry> = BTreeMap::new();
    for e in &entries {
        if let Some(rest) = e.name.strip_prefix("files/")
            && let Some((doc_id, _)) = rest.split_once("__")
        {
            files.insert(doc_id, e);
        }
    }
    let mut parsed = Parsed::default();
    let mut trashed = 0;
    for vault in &export
        .accounts
        .iter()
        .flat_map(|a| &a.vaults)
        .collect::<Vec<_>>()
    {
        for item in &vault.items {
            if item.state == "trashed" || item.state == "deleted" {
                trashed += 1;
                continue;
            }
            parsed.items.push(one(item, &vault.attrs.name, &files));
        }
    }
    if trashed > 0 {
        parsed.problems.push(format!(
            "{trashed} items in 1Password's trash were left out"
        ));
    }
    Ok(parsed)
}

fn one(item: &Item, vault: &str, files: &BTreeMap<&str, &unzip::Entry>) -> crate::ImportedItem {
    let kind = kind_of(&item.category_uuid);
    let mut b = Builder::new(item.uuid.clone(), kind, &item.overview.title);
    b.folder(vault)
        .tags(item.overview.tags.iter().map(String::as_str))
        .favourite(item.fav_index > 0)
        .archived(item.state == "archived")
        .times(item.created_at, item.updated_at);
    if kind == ItemKind::Custom && !item.category_uuid.is_empty() {
        b.report(format!(
            "1Password's category {} has no Lockdown kind; it came in as a custom item",
            item.category_uuid
        ));
    }
    let urls: Vec<&str> = std::iter::once(item.overview.url.as_str())
        .chain(item.overview.urls.iter().map(|u| u.url.as_str()))
        .collect();
    for u in urls {
        if kind == ItemKind::Login {
            b.url(u);
        } else if !u.trim().is_empty() {
            b.custom("Website", FieldKind::Url, u);
        }
    }
    for f in &item.details.login_fields {
        match (f.designation.as_str(), f.field_type.as_str()) {
            ("username", _) => b.field("user", &f.value),
            ("password", _) => b.field("pw", &f.value),
            (_, "P") => b.custom(&f.name, FieldKind::Hidden, &f.value),
            (_, "E") => b.custom(&f.name, FieldKind::Email, &f.value),
            _ => b.custom(&f.name, FieldKind::Text, &f.value),
        };
    }
    if let Some(pw) = &item.details.password {
        b.field("pw", pw);
    }
    for s in &item.details.sections {
        for f in &s.fields {
            if let Some(Value::Object(o)) = Some(&f.value)
                && let Some(file) = o.get("file")
            {
                if let Ok(r) = serde_json::from_value::<FileRef>(file.clone()) {
                    attach(&mut b, &r, files);
                }
                continue;
            }
            let Some((text, fkind)) = value_of(&f.value) else {
                continue;
            };
            let label = if f.title.is_empty() {
                if s.title.is_empty() {
                    f.id.as_str()
                } else {
                    s.title.as_str()
                }
            } else {
                f.title.as_str()
            };
            match field_for(kind, &f.id) {
                Some("card.expiry") => {
                    let m = month_from(&text).unwrap_or(text);
                    b.field_labelled("card.expiry", &m, Some(label));
                }
                Some(id) => {
                    b.field_labelled(id, &text, Some(label));
                }
                None if fkind == FieldKind::Totp => {
                    b.totp(&text);
                }
                None => {
                    b.custom(label, fkind, &text);
                }
            }
        }
    }
    if let Some(doc) = &item.details.document_attributes {
        attach(&mut b, doc, files);
    }
    if let Some(n) = &item.details.notes_plain {
        b.note(n);
    }
    let older: Vec<&str> = item
        .details
        .password_history
        .iter()
        .map(|p| p.value.as_str())
        .filter(|v| !v.is_empty())
        .collect();
    let item_out = b.finish();
    finish_with_history(item_out, &older)
}

fn attach(b: &mut Builder, r: &FileRef, files: &BTreeMap<&str, &unzip::Entry>) {
    match files.get(r.document_id.as_str()) {
        Some(e) => {
            b.attach(&r.file_name, None, Zeroizing::new(e.bytes.to_vec()));
        }
        None => {
            b.report(format!(
                "the file '{}' wasn't in the export and was left out",
                r.file_name
            ));
        }
    }
}

/// 1Password keeps old passwords, not old versions: each becomes an
/// earlier version of the item that differs only in its password, oldest
/// first, so Lockdown's history shows them.
fn finish_with_history(mut item: crate::ImportedItem, older: &[&str]) -> crate::ImportedItem {
    if older.is_empty() || !item.fields.iter().any(|f| f.field_id == "pw") {
        return item;
    }
    for pw in older.iter().rev() {
        let fields: Vec<PlainField> = item
            .fields
            .iter()
            .map(|f| {
                if f.field_id == "pw" {
                    PlainField::hidden("pw", &f.label, pw)
                } else {
                    f.clone()
                }
            })
            .collect();
        item.history.push(fields);
    }
    item
}
