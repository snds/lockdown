//! Enpass's JSON export: `{ "folders": [ { "uuid", "title" } ], "items": [
//! { "uuid", "title", "note", "category", "template_type", "favorite",
//! "trashed", "archived", "folders": [uuid], "fields": [ { "label", "type",
//! "value", "sensitive", "deleted" } ], "attachments": [ { "name", "kind",
//! "data" (base64) } ], "createdAt", "updated_at" } ] }`. Field `type`s are
//! Enpass's (`username`, `email`, `password`, `url`, `totp`, `ccNumber`,
//! `ccCvc`, `ccExpiry`, `ccName`, `ccPin`, `section`, …).

use std::collections::BTreeMap;

use lockdown_vault::{FieldKind, ItemKind};
use serde::Deserialize;
use zeroize::Zeroizing;

use super::{Parsed, json_err};
use crate::build::{Builder, month_from};
use crate::{CxfError, Limits, Result};

#[derive(Deserialize)]
struct Export {
    #[serde(default)]
    folders: Vec<Folder>,
    #[serde(default)]
    items: Vec<Item>,
}

#[derive(Deserialize)]
struct Folder {
    uuid: String,
    #[serde(default)]
    title: String,
}

#[derive(Deserialize)]
struct Item {
    #[serde(default)]
    uuid: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    note: String,
    #[serde(default)]
    category: String,
    #[serde(default)]
    template_type: String,
    #[serde(default)]
    favorite: u8,
    #[serde(default)]
    trashed: u8,
    #[serde(default)]
    archived: u8,
    #[serde(default)]
    folders: Vec<String>,
    #[serde(default)]
    fields: Vec<Field>,
    #[serde(default)]
    attachments: Vec<Attachment>,
    #[serde(default, rename = "createdAt")]
    created_at: Option<u64>,
    #[serde(default)]
    updated_at: Option<u64>,
}

#[derive(Deserialize)]
struct Field {
    #[serde(default)]
    label: String,
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    value: String,
    #[serde(default)]
    sensitive: u8,
    #[serde(default)]
    deleted: u8,
}

#[derive(Deserialize)]
struct Attachment {
    #[serde(default)]
    name: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    data: String,
}

fn kind_of(category: &str, template: &str) -> ItemKind {
    match (category, template) {
        (_, "computer.wifi" | "misc.wifi") => ItemKind::WiFi,
        (_, "computer.database") => ItemKind::Database,
        (_, "computer.server" | "computer.webhosting" | "computer.ftp") => ItemKind::Server,
        (_, "finance.bankaccount" | "finance.bank") => ItemKind::BankAccount,
        (_, "license.software") => ItemKind::SoftwareLicence,
        (_, t) if t.starts_with("travel.") => ItemKind::Document,
        (_, "misc.membership" | "misc.rewards") => ItemKind::Membership,
        (_, "misc.crypto" | "finance.crypto") => ItemKind::CryptoWallet,
        ("login" | "password", _) => ItemKind::Login,
        ("creditcard", _) => ItemKind::Card,
        ("note", _) => ItemKind::Note,
        ("identity", _) => ItemKind::Identity,
        ("license", _) => ItemKind::SoftwareLicence,
        ("travel", _) => ItemKind::Document,
        ("finance", _) => ItemKind::BankAccount,
        _ => ItemKind::Custom,
    }
}

/// Where an Enpass field lands for a kind, by its type and then its label.
fn field_for(kind: ItemKind, ty: &str, label: &str) -> Option<&'static str> {
    let l = label.to_ascii_lowercase();
    Some(match (kind, ty, l.as_str()) {
        (ItemKind::Login | ItemKind::Server | ItemKind::Database, "username", _) => "user",
        (ItemKind::Login | ItemKind::Server | ItemKind::Database, "password", _) => "pw",
        (ItemKind::Login, "email", _) => "user",
        (ItemKind::Card, "ccName", _) => "card.holder",
        (ItemKind::Card, "ccNumber", _) => "card.number",
        (ItemKind::Card, "ccCvc", _) => "card.cvc",
        (ItemKind::Card, "ccExpiry", _) => "card.expiry",
        (ItemKind::Card, "ccPin", _) => "card.pin",
        (ItemKind::Card, "ccType", _) => "card.type",
        (ItemKind::WiFi, _, "ssid" | "network name" | "station name") => "wifi.ssid",
        (ItemKind::WiFi, "password", _) => "wifi.passphrase",
        (ItemKind::WiFi, _, "security" | "security type") => "wifi.security",
        (ItemKind::Server, _, "hostname" | "server" | "url") => "server.host",
        (ItemKind::Server | ItemKind::Database, _, "port") => {
            if kind == ItemKind::Server {
                "server.port"
            } else {
                "db.port"
            }
        }
        (ItemKind::Database, _, "hostname" | "server") => "db.host",
        (ItemKind::Database, _, "database") => "db.name",
        (ItemKind::Database, _, "type") => "db.type",
        (ItemKind::BankAccount, _, "bank name") => "bank.name",
        (ItemKind::BankAccount, _, "account holder") => "bank.holder",
        (ItemKind::BankAccount, _, "account number") => "bank.account_number",
        (ItemKind::BankAccount, _, "routing number" | "sort code") => "bank.routing",
        (ItemKind::BankAccount, _, "iban") => "bank.iban",
        (ItemKind::BankAccount, _, "swift" | "bic" | "swift code") => "bank.swift",
        (ItemKind::BankAccount, _, "pin") => "bank.pin",
        (ItemKind::SoftwareLicence, _, "license key" | "licence key" | "key") => "licence.key",
        (ItemKind::SoftwareLicence, _, "version") => "licence.version",
        (ItemKind::SoftwareLicence, _, "publisher") => "licence.publisher",
        (ItemKind::Identity, _, "first name") => "name.given",
        (ItemKind::Identity, _, "middle name") => "name.middle",
        (ItemKind::Identity, _, "last name") => "name.family",
        (ItemKind::Identity, "email", _) => "identity.email",
        (ItemKind::Identity, "phone", _) => "address.tel",
        (ItemKind::Document, _, "number" | "passport number" | "license number") => "doc.number",
        (ItemKind::Document, _, "expiry date" | "expiration date") => "expires_at",
        (ItemKind::Document, _, "issue date" | "issued on") => "doc.issued",
        (ItemKind::Membership, _, "membership no" | "member id" | "number") => "member.number",
        (ItemKind::Membership, _, "member name" | "name") => "member.name",
        (ItemKind::CryptoWallet, _, "seed" | "recovery phrase" | "mnemonic") => "seed",
        (ItemKind::CryptoWallet, _, "address" | "wallet address") => "wallet.address",
        _ => return None,
    })
}

/// Reads an Enpass JSON export.
pub fn read(bytes: &[u8], limits: &Limits) -> Result<Parsed> {
    limits.check_size(bytes)?;
    let export: Export = serde_json::from_slice(bytes).map_err(json_err("Enpass JSON"))?;
    if export.items.len() > limits.max_items {
        return Err(CxfError::TooMany {
            what: "items",
            found: export.items.len(),
            limit: limits.max_items,
        });
    }
    let folders: BTreeMap<&str, &str> = export
        .folders
        .iter()
        .map(|f| (f.uuid.as_str(), f.title.as_str()))
        .collect();
    let mut parsed = Parsed::default();
    let mut trashed = 0;
    for item in &export.items {
        if item.trashed != 0 {
            trashed += 1;
            continue;
        }
        parsed.items.push(one(item, &folders, limits));
    }
    if trashed > 0 {
        parsed
            .problems
            .push(format!("{trashed} items in Enpass's trash were left out"));
    }
    Ok(parsed)
}

fn one(item: &Item, folders: &BTreeMap<&str, &str>, limits: &Limits) -> crate::ImportedItem {
    let kind = kind_of(&item.category, &item.template_type);
    let mut b = Builder::new(item.uuid.clone(), kind, &item.title);
    b.favourite(item.favorite != 0)
        .archived(item.archived != 0)
        .times(item.created_at, item.updated_at);
    if let Some(f) = item.folders.first().and_then(|id| folders.get(id.as_str())) {
        b.folder(f);
    }
    if kind == ItemKind::Custom {
        b.report(format!(
            "Enpass's '{}' category has no Lockdown kind; it came in as a custom item",
            item.category
        ));
    }
    for f in item
        .fields
        .iter()
        .filter(|f| f.deleted == 0 && f.kind != "section")
    {
        match (field_for(kind, &f.kind, &f.label), f.kind.as_str()) {
            (Some("card.expiry"), _) => {
                let m = month_from(&f.value).unwrap_or_else(|| f.value.clone());
                b.field_labelled("card.expiry", &m, Some(&f.label));
            }
            (Some(id), _) => {
                b.field_labelled(id, &f.value, Some(&f.label));
            }
            (None, "totp") => {
                b.totp(&f.value);
            }
            (None, "url") if kind == ItemKind::Login => {
                b.url(&f.value);
            }
            (None, ty) => {
                let fk = match ty {
                    "url" => FieldKind::Url,
                    "email" => FieldKind::Email,
                    "phone" => FieldKind::Phone,
                    "date" => FieldKind::Date,
                    _ if f.sensitive != 0 || ty == "password" || ty == "pin" => FieldKind::Hidden,
                    _ => FieldKind::Text,
                };
                b.custom(&f.label, fk, &f.value);
            }
        }
    }
    for a in &item.attachments {
        let raw = a.data.trim();
        match data_encoding::BASE64.decode(raw.as_bytes()) {
            Ok(bytes) if bytes.len() <= limits.max_attachment_bytes => {
                let mime = (!a.kind.is_empty()).then_some(a.kind.as_str());
                b.attach(&a.name, mime, Zeroizing::new(bytes));
            }
            Ok(_) => {
                b.report(format!(
                    "the file '{}' is over the size limit and was left out",
                    a.name
                ));
            }
            Err(_) => {
                b.report(format!(
                    "the file '{}' didn't read and was left out",
                    a.name
                ));
            }
        }
    }
    b.note(&item.note);
    b.finish()
}
