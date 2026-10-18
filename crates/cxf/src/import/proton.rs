//! Proton Pass exports: the ZIP (`Proton Pass/data.json`), the bare JSON,
//! and the CSV. The JSON is `{ "version", "userId", "encrypted": false,
//! "vaults": { "<shareId>": { "name", "items": [ { "itemId", "state",
//! "createTime", "modifyTime", "pinned", "aliasEmail", "data": { "type",
//! "metadata": { "name", "note" }, "extraFields": [...], "content": {...}
//! } } ] } } }`. A PGP-encrypted export (`data.pgp`) is refused with how to
//! make an unencrypted one: Lockdown doesn't read OpenPGP.

use lockdown_vault::{FieldKind, ItemKind};
use serde::Deserialize;
use serde_json::{Map, Value};

use super::{Parsed, json_err};
use crate::build::{Builder, month_from};
use crate::csv::Table;
use crate::{CxfError, Limits, Result, unzip};

#[derive(Deserialize)]
struct Export {
    #[serde(default)]
    encrypted: bool,
    #[serde(default)]
    vaults: Map<String, Value>,
}

#[derive(Deserialize)]
struct Vault {
    #[serde(default)]
    name: String,
    #[serde(default)]
    items: Vec<Item>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Item {
    #[serde(default)]
    item_id: String,
    #[serde(default)]
    state: u8,
    #[serde(default)]
    create_time: Option<u64>,
    #[serde(default)]
    modify_time: Option<u64>,
    #[serde(default)]
    pinned: bool,
    #[serde(default)]
    alias_email: Option<String>,
    data: Data,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Data {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    metadata: Metadata,
    #[serde(default)]
    extra_fields: Vec<ExtraField>,
    #[serde(default)]
    content: Map<String, Value>,
}

#[derive(Deserialize, Default)]
struct Metadata {
    #[serde(default)]
    name: String,
    #[serde(default)]
    note: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExtraField {
    #[serde(default)]
    field_name: String,
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    data: Map<String, Value>,
}

fn s(map: &Map<String, Value>, key: &str) -> String {
    match map.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

/// Reads a Proton Pass ZIP or JSON export.
pub fn read(bytes: &[u8], limits: &Limits) -> Result<Parsed> {
    let json: Vec<u8> = if unzip::is_zip(bytes) {
        let entries = unzip::read(bytes, limits)?;
        if entries.iter().any(|e| e.name.ends_with(".pgp")) {
            return Err(CxfError::Unsupported(
                "this Proton Pass export is encrypted with PGP; export again from Proton Pass without encryption (JSON or ZIP), import it here, then delete the file".into(),
            ));
        }
        entries
            .iter()
            .find(|e| e.name.ends_with("data.json"))
            .map(|e| e.bytes.to_vec())
            .ok_or_else(|| CxfError::Parse {
                format: "Proton Pass",
                why: "there is no data.json inside; is this a Proton Pass export?".into(),
            })?
    } else {
        limits.check_size(bytes)?;
        bytes.to_vec()
    };
    let export: Export = serde_json::from_slice(&json).map_err(json_err("Proton Pass"))?;
    let json = zeroize::Zeroizing::new(json);
    drop(json);
    if export.encrypted {
        return Err(CxfError::Unsupported(
            "this Proton Pass export is encrypted; export again without encryption".into(),
        ));
    }
    let mut parsed = Parsed::default();
    let mut trashed = 0;
    for (_, v) in export.vaults {
        let vault: Vault = serde_json::from_value(v).map_err(json_err("Proton Pass"))?;
        for item in &vault.items {
            if item.state == 2 {
                trashed += 1;
                continue;
            }
            parsed.items.push(one(item, &vault.name));
            if parsed.items.len() > limits.max_items {
                return Err(CxfError::TooMany {
                    what: "items",
                    found: parsed.items.len(),
                    limit: limits.max_items,
                });
            }
        }
    }
    if trashed > 0 {
        parsed.problems.push(format!(
            "{trashed} items in Proton Pass's trash were left out"
        ));
    }
    Ok(parsed)
}

fn one(item: &Item, vault: &str) -> crate::ImportedItem {
    let c = &item.data.content;
    let kind = match item.data.kind.as_str() {
        "login" | "alias" => ItemKind::Login,
        "note" => ItemKind::Note,
        "creditCard" => ItemKind::Card,
        "identity" => ItemKind::Identity,
        "wifi" => ItemKind::WiFi,
        "sshKey" => ItemKind::SshKey,
        _ => ItemKind::Custom,
    };
    let mut b = Builder::new(item.item_id.clone(), kind, &item.data.metadata.name);
    b.folder(vault)
        .favourite(item.pinned)
        .times(item.create_time, item.modify_time);
    match item.data.kind.as_str() {
        "login" => {
            let email = s(c, "itemEmail");
            let user = s(c, "itemUsername");
            let user = if user.is_empty() {
                s(c, "username")
            } else {
                user
            };
            if user.is_empty() {
                b.field("user", &email);
            } else {
                b.field("user", &user);
                b.custom("Email", FieldKind::Email, &email);
            }
            b.field("pw", &s(c, "password")).totp(&s(c, "totpUri"));
            for u in c
                .get("urls")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                b.url(u.as_str().unwrap_or(""));
            }
            if let Some(n) = c.get("passkeys").and_then(Value::as_array)
                && !n.is_empty()
            {
                b.report(format!(
                    "{} passkeys aren't in a form Lockdown reads from a Proton Pass export and were left out",
                    n.len()
                ));
            }
        }
        "alias" => {
            b.field("user", item.alias_email.as_deref().unwrap_or(""));
            b.report("a hide-my-email alias came in as a login with its address; the alias itself stays with Proton");
        }
        "creditCard" => {
            b.field("card.holder", &s(c, "cardholderName"))
                .field("card.number", &s(c, "number"))
                .field("card.cvc", &s(c, "verificationNumber"))
                .field("card.pin", &s(c, "pin"));
            let exp = s(c, "expirationDate");
            match month_from(&exp) {
                Some(m) => b.field("card.expiry", &m),
                None => b.custom("Expires", FieldKind::Text, &exp),
            };
        }
        "identity" => {
            let full = s(c, "fullName");
            b.field("name.given", &s(c, "firstName"))
                .field("name.middle", &s(c, "middleName"))
                .field("name.family", &s(c, "lastName"))
                .field("identity.email", &s(c, "email"))
                .field("address.tel", &s(c, "phoneNumber"))
                .field("address.street", &s(c, "streetAddress"))
                .field("address.city", &s(c, "city"))
                .field("address.territory", &s(c, "stateOrProvince"))
                .field("address.postal_code", &s(c, "zipOrPostalCode"))
                .field("address.country", &s(c, "countryOrRegion"));
            if !b.has("name.given") {
                b.field("name.given", &full);
            }
            let skip = [
                "fullName",
                "firstName",
                "middleName",
                "lastName",
                "email",
                "phoneNumber",
                "streetAddress",
                "city",
                "stateOrProvince",
                "zipOrPostalCode",
                "countryOrRegion",
            ];
            for (k, v) in c.iter().filter(|(k, _)| !skip.contains(&k.as_str())) {
                if let Some(t) = v.as_str() {
                    let kind = if k.contains("Number") || k.contains("security") {
                        FieldKind::Hidden
                    } else {
                        FieldKind::Text
                    };
                    b.custom(&words(k), kind, t);
                }
            }
        }
        "wifi" => {
            b.field("wifi.ssid", &s(c, "ssid"))
                .field("wifi.passphrase", &s(c, "password"))
                .field("wifi.security", &s(c, "security"));
        }
        "sshKey" => {
            b.field("ssh.private_key", &s(c, "privateKey"))
                .field("ssh.public_key", &s(c, "publicKey"));
        }
        "note" => {}
        other => {
            b.report(format!(
                "Proton Pass's '{other}' type has no Lockdown kind; it came in as a custom item"
            ));
        }
    }
    for f in &item.data.extra_fields {
        let v = s(&f.data, "content");
        match f.kind.as_str() {
            "hidden" => b.custom(&f.field_name, FieldKind::Hidden, &v),
            "totp" => b.totp(&v),
            "timestamp" => b.custom(&f.field_name, FieldKind::Date, &v),
            _ => b.custom(&f.field_name, FieldKind::Text, &v),
        };
    }
    b.note(&item.data.metadata.note);
    b.finish()
}

/// `socialSecurityNumber` → `Social security number`.
fn words(key: &str) -> String {
    let mut out = String::new();
    for (i, c) in key.chars().enumerate() {
        if c.is_ascii_uppercase() && i > 0 {
            out.push(' ');
            out.push(c.to_ascii_lowercase());
        } else if i == 0 {
            out.push(c.to_ascii_uppercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// Whether a CSV's headers are Proton Pass's.
pub fn is_proton_csv(t: &Table) -> bool {
    [
        "type", "name", "url", "email", "username", "password", "note", "totp", "vault",
    ]
    .iter()
    .all(|h| t.headers.iter().any(|x| x == h))
}

/// Reads Proton Pass's CSV: `type,name,url,email,username,password,note,
/// totp,createTime,modifyTime,vault`.
pub fn read_csv(t: &Table) -> Parsed {
    let col = |name: &str| t.column(&[name]);
    let (ty, name, url, email, user, pw, note, totp, vault) = (
        col("type"),
        col("name"),
        col("url"),
        col("email"),
        col("username"),
        col("password"),
        col("note"),
        col("totp"),
        col("vault"),
    );
    let get =
        |r: &[String], c: Option<usize>| c.and_then(|i| r.get(i)).cloned().unwrap_or_default();
    let mut parsed = Parsed::default();
    for (n, r) in t.rows.iter().enumerate() {
        let kind = match get(r, ty).as_str() {
            "note" => ItemKind::Note,
            _ => ItemKind::Login,
        };
        let mut b = Builder::new(format!("proton-csv-{n}"), kind, &get(r, name));
        b.folder(&get(r, vault));
        if kind == ItemKind::Login {
            let (e, u) = (get(r, email), get(r, user));
            if u.is_empty() {
                b.field("user", &e);
            } else {
                b.field("user", &u).custom("Email", FieldKind::Email, &e);
            }
            b.field("pw", &get(r, pw)).totp(&get(r, totp));
            for one in get(r, url).split(',') {
                b.url(one);
            }
        }
        b.note(&get(r, note));
        parsed.items.push(b.finish());
    }
    parsed
}
