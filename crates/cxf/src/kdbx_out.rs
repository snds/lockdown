//! The exit path (02-model §11, matrix `open-format`): the vault as a
//! KeePass KDBX 4 database any KeePass-family app opens, written with the
//! `keepass` crate under a password the person picks. One group per kind;
//! each item an entry with Title, UserName, Password, URL, Notes and `otp`
//! where it has them, every other field as a named string (masked when
//! secret), passkeys in KeePassXC's `KPEX_PASSKEY_*` fields, attachments as
//! entry attachments. What KDBX has no place for (earlier versions,
//! match rules beyond the address) is listed by the caller's report.

use keepass::db::{EntryMut, Value, fields};
use keepass::{Database, DatabaseKey};
use lockdown_vault::template::template_for;
use lockdown_vault::{FieldKind, FieldValue, ItemKind, PlainField};

use crate::convert::ImportedItem;
use crate::model::b64url;
use crate::{CxfError, Result};

fn text(v: &FieldValue) -> Option<String> {
    match v {
        FieldValue::Text(t) => Some(t.clone()),
        FieldValue::Url { value, .. } => Some(value.clone()),
        FieldValue::Boolean(b) => Some(if *b { "yes" } else { "no" }.into()),
        FieldValue::Bytes(b) => Some(b64url(b)),
        FieldValue::Ref(id) => Some(id.to_string()),
    }
}

fn pem(der: &[u8]) -> String {
    let b64 = data_encoding::BASE64.encode(der);
    let mut out = String::from("-----BEGIN PRIVATE KEY-----\n");
    for line in b64.as_bytes().chunks(64) {
        out.push_str(&String::from_utf8_lossy(line));
        out.push('\n');
    }
    out.push_str("-----END PRIVATE KEY-----\n");
    out
}

fn put(e: &mut EntryMut<'_>, name: &str, value: String, secret: bool) {
    let mut key = name.to_string();
    let mut n = 2;
    while e.fields.contains_key(&key) {
        key = format!("{name} ({n})");
        n += 1;
    }
    if secret {
        e.set_protected(key, value);
    } else {
        e.set_unprotected(key, value);
    }
}

fn entry(e: &mut EntryMut<'_>, item: &ImportedItem) {
    e.set_unprotected(fields::TITLE, item.title.clone());
    let primary = template_for(item.kind).primary;
    let mut urls = 0;
    let mut passkey: Option<usize> = None;
    for f in item.fields.iter().filter(|f| f.field_id != "title") {
        let Some(value) = text(&f.value) else {
            continue;
        };
        let secret = matches!(
            f.kind,
            FieldKind::Hidden | FieldKind::Totp | FieldKind::Passkey
        );
        match f.field_id.as_str() {
            "user" => e.set_unprotected(fields::USERNAME, value),
            "pw" => e.set_protected(fields::PASSWORD, value),
            "notes" | "body" => e.set_unprotected(fields::NOTES, value),
            "code.0" => e.set_protected(fields::OTP, value),
            "lockdown.export" => {}
            id if Some(id) == primary && !e.fields.contains_key(fields::PASSWORD) => {
                e.set_protected(fields::PASSWORD, value)
            }
            id if id.starts_with("passkey.") => {
                let n: usize = id
                    .split('.')
                    .nth(1)
                    .and_then(|x| x.parse().ok())
                    .unwrap_or(0);
                if n == 0 {
                    passkey = Some(0);
                    let part = id.rsplit('.').next().unwrap_or("");
                    match (part, &f.value) {
                        ("rp_id", _) => e.set_unprotected("KPEX_PASSKEY_RELYING_PARTY", value),
                        ("user_name", _) => e.set_unprotected("KPEX_PASSKEY_USERNAME", value),
                        ("credential_id", _) => {
                            e.set_protected("KPEX_PASSKEY_CREDENTIAL_ID", value)
                        }
                        ("user_handle", _) => e.set_protected("KPEX_PASSKEY_USER_HANDLE", value),
                        ("key", FieldValue::Bytes(der)) => {
                            e.set_protected("KPEX_PASSKEY_PRIVATE_KEY_PEM", pem(der))
                        }
                        _ => put(e, &f.label, value, secret),
                    }
                } else {
                    put(e, &format!("Passkey {} {}", n + 1, f.label), value, secret);
                }
            }
            _ if f.kind == FieldKind::Url && item.kind == ItemKind::Login => {
                if urls == 0 && !e.fields.contains_key(fields::URL) {
                    e.set_unprotected(fields::URL, value);
                } else {
                    put(e, &format!("KP2A_URL_{urls}"), value, false);
                }
                urls += 1;
            }
            _ => put(e, &f.label, value, secret),
        }
    }
    if passkey.is_some() {
        e.tags.push("Passkey".into());
    }
    for a in &item.attachments {
        e.add_attachment(a.name.clone(), Value::protected(a.bytes.to_vec()));
    }
}

/// The group name for a kind.
fn group_name(kind: ItemKind) -> String {
    let name = template_for(kind).name;
    match kind {
        ItemKind::Login => "Logins".into(),
        ItemKind::Code => "One-time codes".into(),
        ItemKind::Identity => "Identities".into(),
        ItemKind::Membership => "Memberships".into(),
        _ => format!("{name}s"),
    }
}

/// The items as a KDBX 4 file locked with `password`.
pub fn write(items: &[ImportedItem], password: &str, name: &str) -> Result<Vec<u8>> {
    let mut db = Database::new();
    db.meta.database_name = Some(name.into());
    let floor = lockdown_crypto::kdf::floor();
    if let keepass::config::KdfConfig::Argon2 {
        iterations,
        memory,
        parallelism,
        ..
    } = &mut db.config.kdf_config
    {
        *iterations = u64::from(floor.t_cost);
        *memory = u64::from(floor.m_cost_kib) * 1024;
        *parallelism = floor.p_cost;
    }
    let mut kinds: Vec<ItemKind> = Vec::new();
    for i in items {
        if !kinds.contains(&i.kind) {
            kinds.push(i.kind);
        }
    }
    for kind in kinds {
        let mut root = db.root_mut();
        let mut group = root.add_group();
        group.name = group_name(kind);
        for item in items.iter().filter(|i| i.kind == kind) {
            let mut e = group.add_entry();
            entry(&mut e, item);
        }
    }
    let mut out = Vec::new();
    db.save(&mut out, DatabaseKey::new().with_password(password))
        .map_err(|e| CxfError::Unsupported(format!("couldn't write the KeePass file: {e}")))?;
    Ok(out)
}

/// What a KDBX file can't hold of an item, in words, for the export report.
pub fn dropped(item: &ImportedItem) -> Vec<String> {
    let mut out = Vec::new();
    if !item.history.is_empty() {
        out.push(format!(
            "{} earlier versions (KeePass keeps its own history from now on)",
            item.history.len()
        ));
    }
    let rules = item
        .fields
        .iter()
        .filter(|f: &&PlainField| {
            matches!(&f.value, FieldValue::Url { match_rule, .. } if match_rule != "exact_host")
        })
        .count();
    if rules > 0 {
        out.push(format!("{rules} website match rules"));
    }
    out
}
