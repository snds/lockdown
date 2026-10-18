//! Keeper's exports: JSON and CSV.
//!
//! JSON: `{ "shared_folders": [...], "records": [ { "uid", "title",
//! "$type", "login", "password", "login_url", "notes", "custom_fields": {
//! "<$type>[:<label>]": value }, "folders": [ { "folder": "A\\B" } or {
//! "shared_folder": "S", "folder": "F" } ] } ] }`. Typed records keep their
//! structured values in `custom_fields` under `$`-names (`$paymentCard`,
//! `$keyPair`, `$bankAccount`, `$oneTimeCode`, …).
//!
//! CSV: no header; each row is `folder, title, login, password, url,
//! notes, shared folder`, then name/value pairs of custom fields.

use lockdown_vault::{FieldKind, ItemKind};
use serde::Deserialize;
use serde_json::{Map, Value};

use super::{Parsed, json_err};
use crate::build::{Builder, month_from};
use crate::{CxfError, Limits, Result};

#[derive(Deserialize)]
struct Export {
    #[serde(default)]
    records: Vec<Record>,
}

#[derive(Deserialize)]
struct Record {
    #[serde(default)]
    uid: Option<String>,
    #[serde(default)]
    title: String,
    #[serde(rename = "$type", default)]
    kind: String,
    #[serde(default)]
    login: Option<String>,
    #[serde(default)]
    password: Option<String>,
    #[serde(default)]
    login_url: Option<String>,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default)]
    custom_fields: Map<String, Value>,
    #[serde(default)]
    folders: Vec<Map<String, Value>>,
}

fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Array(a) => a
            .iter()
            .map(text)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(", "),
        Value::Object(o) => o
            .values()
            .map(text)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(", "),
        Value::Null => String::new(),
    }
}

fn part(v: &Value, key: &str) -> String {
    let v = match v {
        Value::Array(a) => a.first().unwrap_or(&Value::Null),
        other => other,
    };
    v.get(key).map(text).unwrap_or_default()
}

/// Reads a Keeper JSON export.
pub fn read_json(bytes: &[u8], limits: &Limits) -> Result<Parsed> {
    limits.check_size(bytes)?;
    let export: Export = serde_json::from_slice(bytes).map_err(json_err("Keeper JSON"))?;
    if export.records.len() > limits.max_items {
        return Err(CxfError::TooMany {
            what: "records",
            found: export.records.len(),
            limit: limits.max_items,
        });
    }
    let mut parsed = Parsed::default();
    for (n, r) in export.records.iter().enumerate() {
        parsed.items.push(record(r, n));
    }
    Ok(parsed)
}

fn record(r: &Record, n: usize) -> crate::ImportedItem {
    let kind = match r.kind.as_str() {
        "" | "login" => ItemKind::Login,
        "bankCard" => ItemKind::Card,
        "bankAccount" => ItemKind::BankAccount,
        "encryptedNotes" => ItemKind::Note,
        "sshKeys" => ItemKind::SshKey,
        "serverCredentials" => ItemKind::Server,
        "databaseCredentials" => ItemKind::Database,
        "wifiCredentials" => ItemKind::WiFi,
        "softwareLicense" => ItemKind::SoftwareLicence,
        "membership" => ItemKind::Membership,
        "address" | "contact" => ItemKind::Identity,
        "passport" | "driverLicense" | "birthCertificate" | "ssnCard" | "file" | "photo" => {
            ItemKind::Document
        }
        _ => ItemKind::Custom,
    };
    let id = r.uid.clone().unwrap_or_else(|| format!("keeper-{n}"));
    let mut b = Builder::new(id, kind, &r.title);
    if kind == ItemKind::Custom {
        b.report(format!(
            "Keeper's '{}' record type has no Lockdown kind; it came in as a custom item",
            r.kind
        ));
    }
    let user = r.login.clone().unwrap_or_default();
    let pw = r.password.clone().unwrap_or_default();
    let url = r.login_url.clone().unwrap_or_default();
    match kind {
        ItemKind::Login | ItemKind::Server | ItemKind::Database => {
            b.field("user", &user).field("pw", &pw);
            if kind == ItemKind::Login {
                b.url(&url);
            } else {
                b.custom("Website", FieldKind::Url, &url);
            }
        }
        _ => {
            b.custom("Login", FieldKind::Text, &user)
                .custom("Password", FieldKind::Hidden, &pw)
                .custom("Website", FieldKind::Url, &url);
        }
    }
    for (key, v) in &r.custom_fields {
        let (ty, label) = match key.split_once(':') {
            Some((t, l)) => (t, l),
            None => (key.as_str(), key.trim_start_matches('$')),
        };
        let ty = ty.split("::").next().unwrap_or(ty);
        match ty {
            "$note" => {
                b.note(&text(v));
            }
            "$oneTimeCode" => {
                b.totp(&text(v));
            }
            "$paymentCard" => {
                b.field("card.number", &part(v, "cardNumber"))
                    .field("card.cvc", &part(v, "cardSecurityCode"));
                let exp = part(v, "cardExpirationDate");
                match month_from(&exp) {
                    Some(m) => b.field("card.expiry", &m),
                    None => b.custom("Expires", FieldKind::Text, &exp),
                };
            }
            "$text" if kind == ItemKind::Card && label == "cardholderName" => {
                b.field("card.holder", &text(v));
            }
            "$pinCode" if kind == ItemKind::Card => {
                b.field("card.pin", &text(v));
            }
            "$bankAccount" => {
                b.field("bank.account_number", &part(v, "accountNumber"))
                    .field("bank.routing", &part(v, "routingNumber"))
                    .custom("Account type", FieldKind::Text, &part(v, "accountType"));
            }
            "$keyPair" => {
                b.field("ssh.private_key", &part(v, "privateKey"))
                    .field("ssh.public_key", &part(v, "publicKey"));
            }
            "$host" => {
                let host = part(v, "hostName");
                let port = part(v, "port");
                match kind {
                    ItemKind::Server => b.field("server.host", &host).field("server.port", &port),
                    ItemKind::Database => b.field("db.host", &host).field("db.port", &port),
                    _ => b.custom("Host", FieldKind::Text, &format!("{host}:{port}")),
                };
            }
            "$name" => {
                if kind == ItemKind::Identity {
                    b.field("name.given", &part(v, "first"))
                        .field("name.middle", &part(v, "middle"))
                        .field("name.family", &part(v, "last"));
                } else {
                    b.custom(label, FieldKind::Text, &text(v));
                }
            }
            "$address" if kind == ItemKind::Identity => {
                let street = [part(v, "street1"), part(v, "street2")]
                    .into_iter()
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(", ");
                b.field("address.street", &street)
                    .field("address.city", &part(v, "city"))
                    .field("address.territory", &part(v, "state"))
                    .field("address.postal_code", &part(v, "zip"))
                    .field("address.country", &part(v, "country"));
            }
            "$email" if kind == ItemKind::Identity => {
                b.field("identity.email", &text(v));
            }
            "$phone" if kind == ItemKind::Identity => {
                b.field("address.tel", &part(v, "number"));
            }
            "$passkey" => {
                b.report("a passkey in a Keeper export isn't in a form Lockdown can use and was left out");
            }
            "$secret" | "$pinCode" | "$securityQuestion" | "$licenseNumber" | "$accountNumber" => {
                b.custom(label, FieldKind::Hidden, &text(v));
            }
            "$url" => {
                b.custom(label, FieldKind::Url, &text(v));
            }
            "$date" | "$birthDate" | "$expirationDate" => {
                let t = text(v);
                let t = t
                    .parse::<i64>()
                    .map(|ms| crate::build::date_from_unix(ms / 1000))
                    .unwrap_or(t);
                b.custom(label, FieldKind::Date, &t);
            }
            _ => {
                b.custom(label, FieldKind::Text, &text(v));
            }
        }
    }
    for f in &r.folders {
        let shared = f.get("shared_folder").map(text).unwrap_or_default();
        let folder = f.get("folder").map(text).unwrap_or_default();
        let path = [shared, folder]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("/")
            .replace('\\', "/");
        b.folder(&path);
    }
    b.note(r.notes.as_deref().unwrap_or(""));
    b.finish()
}

/// Reads Keeper's CSV rows (the file has no header row).
pub fn read_csv(rows: &[Vec<String>]) -> Parsed {
    let mut parsed = Parsed::default();
    for (n, r) in rows.iter().enumerate() {
        let get = |i: usize| r.get(i).cloned().unwrap_or_default();
        let mut b = Builder::new(format!("keeper-csv-{n}"), ItemKind::Login, &get(1));
        b.folder(
            &[get(6), get(0)]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("/")
                .replace('\\', "/"),
        )
        .field("user", &get(2))
        .field("pw", &get(3))
        .url(&get(4));
        let mut i = 7;
        while i + 1 < r.len() {
            let (name, value) = (get(i), get(i + 1));
            if name == "TFC:Keeper" || name.eq_ignore_ascii_case("$oneTimeCode") {
                b.totp(&value);
            } else {
                b.custom(&name, FieldKind::Text, &value);
            }
            i += 2;
        }
        b.note(&get(5));
        parsed.items.push(b.finish());
    }
    parsed
}
