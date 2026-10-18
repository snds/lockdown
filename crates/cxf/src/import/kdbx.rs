//! KeePass databases (KDBX 3.1 and 4), read with the `keepass` crate (MIT,
//! github.com/sseemayer/keepass-rs). Groups become folders, entries become
//! items (Logins, or Notes when an entry has only notes), string fields
//! beyond the five standard ones become fields of the person's own
//! (protected ones masked), `otp` (KeePassXC's otpauth:// link) becomes a
//! code, attachments come along, and every entry's history becomes the
//! item's earlier versions. Entries in the recycle bin are left out.
//!
//! The key-derivation cost lives in the file's header, so a hostile file
//! could ask for a terabyte of Argon2 memory. The header is read here first
//! and refused above the caps before the crate runs the derivation.

use lockdown_vault::{FieldKind, ItemKind};
use zeroize::Zeroizing;

use super::Parsed;
use crate::build::Builder;
use crate::{CxfError, Limits, Result};

/// The first eight bytes of every KDBX file.
pub const MAGIC: [u8; 8] = [0x03, 0xD9, 0xA2, 0x9A, 0x67, 0xFB, 0x4B, 0xB5];

/// Caps on the cost a file may ask for.
const MAX_AES_ROUNDS: u64 = 50_000_000;
const MAX_ARGON_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_ARGON_ITERATIONS: u64 = 100;
const MAX_ARGON_LANES: u64 = 64;

fn bad(why: impl Into<String>) -> CxfError {
    CxfError::Parse {
        format: "KDBX",
        why: why.into(),
    }
}

fn take<'a>(bytes: &'a [u8], at: &mut usize, n: usize) -> Result<&'a [u8]> {
    let end = at
        .checked_add(n)
        .ok_or_else(|| bad("the header is cut short"))?;
    let s = bytes
        .get(*at..end)
        .ok_or_else(|| bad("the header is cut short"))?;
    *at = end;
    Ok(s)
}

fn le_u64(b: &[u8]) -> u64 {
    let mut a = [0u8; 8];
    for (i, x) in b.iter().take(8).enumerate() {
        a[i] = *x;
    }
    u64::from_le_bytes(a)
}

/// Reads the outer header and refuses a key derivation above the caps.
pub fn check_cost(bytes: &[u8]) -> Result<()> {
    if bytes.len() < 12 || bytes[..8] != MAGIC {
        return Err(bad("this isn't a KeePass database"));
    }
    let major = u16::from_le_bytes([bytes[10], bytes[11]]);
    let mut at = 12;
    loop {
        let id = *take(bytes, &mut at, 1)?.first().unwrap_or(&0);
        let len = if major >= 4 {
            let b = take(bytes, &mut at, 4)?;
            u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize
        } else {
            let b = take(bytes, &mut at, 2)?;
            u16::from_le_bytes([b[0], b[1]]) as usize
        };
        let value = take(bytes, &mut at, len)?;
        match id {
            0 => return Ok(()),
            // KDBX 3.1: TransformRounds (AES-KDF).
            6 if major < 4 => {
                if le_u64(value) > MAX_AES_ROUNDS {
                    return Err(CxfError::Unsupported(format!(
                        "the database asks for {} key-derivation rounds; Lockdown stops at {MAX_AES_ROUNDS}",
                        le_u64(value)
                    )));
                }
            }
            // KDBX 4: KdfParameters, a variant dictionary.
            11 => check_kdf_dictionary(value)?,
            _ => {}
        }
    }
}

fn check_kdf_dictionary(d: &[u8]) -> Result<()> {
    let mut at = 2; // version
    while at < d.len() {
        let ty = *take(d, &mut at, 1)?.first().unwrap_or(&0);
        if ty == 0 {
            break;
        }
        let kl = take(d, &mut at, 4)?;
        let kl = u32::from_le_bytes([kl[0], kl[1], kl[2], kl[3]]) as usize;
        let key = take(d, &mut at, kl)?;
        let vl = take(d, &mut at, 4)?;
        let vl = u32::from_le_bytes([vl[0], vl[1], vl[2], vl[3]]) as usize;
        let val = take(d, &mut at, vl)?;
        let n = le_u64(val);
        let over = match key {
            b"R" => n > MAX_AES_ROUNDS,
            b"M" => n > MAX_ARGON_BYTES,
            b"I" => n > MAX_ARGON_ITERATIONS,
            b"P" => n > MAX_ARGON_LANES,
            _ => false,
        };
        if over {
            return Err(CxfError::Unsupported(format!(
                "the database asks for a key derivation heavier than Lockdown allows ({}={n})",
                String::from_utf8_lossy(key)
            )));
        }
    }
    Ok(())
}

/// Reads a KeePass database opened with `password`.
pub fn read(bytes: &[u8], password: Option<&[u8]>, limits: &Limits) -> Result<Parsed> {
    limits.check_size(bytes)?;
    check_cost(bytes)?;
    let password = password.ok_or(CxfError::PasswordNeeded)?;
    let password = std::str::from_utf8(password)
        .map_err(|_| CxfError::Unsupported("the password isn't valid text".into()))?;
    let key = keepass::DatabaseKey::new().with_password(password);
    let db = keepass::Database::parse(bytes, key).map_err(|e| match e {
        keepass::db::DatabaseOpenError::Key(_) => CxfError::WrongPassword,
        other => {
            let text = other.to_string();
            if text.to_ascii_lowercase().contains("key") || text.contains("HMAC") {
                CxfError::WrongPassword
            } else {
                bad(text)
            }
        }
    })?;
    let bin = db.meta.recyclebin_uuid.map(|u| u.to_string());
    let mut parsed = Parsed::default();
    let mut binned = 0;
    walk(
        &db.root(),
        &mut Vec::new(),
        bin.as_deref(),
        &mut parsed,
        &mut binned,
        limits,
    )?;
    if binned > 0 {
        parsed.problems.push(format!(
            "{binned} entries in KeePass's recycle bin were left out"
        ));
    }
    Ok(parsed)
}

fn walk(
    group: &keepass::db::GroupRef<'_>,
    path: &mut Vec<String>,
    bin: Option<&str>,
    parsed: &mut Parsed,
    binned: &mut usize,
    limits: &Limits,
) -> Result<()> {
    if bin.is_some_and(|b| group.id().uuid().to_string() == b) {
        *binned += group.entries().count();
        return Ok(());
    }
    let folder = path.join("/");
    for e in group.entries() {
        parsed.items.push(entry(&e, &folder));
        if parsed.items.len() > limits.max_items {
            return Err(CxfError::TooMany {
                what: "entries",
                found: parsed.items.len(),
                limit: limits.max_items,
            });
        }
    }
    for g in group.groups() {
        path.push(g.name.clone());
        walk(&g, path, bin, parsed, binned, limits)?;
        path.pop();
    }
    Ok(())
}

const STANDARD: [&str; 6] = ["Title", "UserName", "Password", "URL", "Notes", "otp"];

fn fields_of(e: &keepass::db::EntryRef<'_>, id: &str) -> crate::ImportedItem {
    let get = |k: &str| e.get(k).unwrap_or("").to_string();
    let user = get("UserName");
    let pw = get("Password");
    let url = get("URL");
    let others = e.fields.keys().any(|k| !STANDARD.contains(&k.as_str()));
    let kind = if user.is_empty() && pw.is_empty() && url.is_empty() && !others {
        ItemKind::Note
    } else {
        ItemKind::Login
    };
    let mut b = Builder::new(id.to_string(), kind, &get("Title"));
    if kind == ItemKind::Login {
        b.field("user", &user).field("pw", &pw).url(&url);
    }
    if let Some(otp) = e.get("otp") {
        b.totp(otp);
    } else if let Some(seed) = e.get("TOTP Seed") {
        b.totp(seed);
    }
    let mut keys: Vec<&String> = e
        .fields
        .keys()
        .filter(|k| !STANDARD.contains(&k.as_str()) && k.as_str() != "TOTP Seed")
        .collect();
    keys.sort();
    for k in keys {
        let v = &e.fields[k];
        let kind = if v.is_protected() {
            FieldKind::Hidden
        } else if k.starts_with("KP2A_URL") || k.starts_with("URL_") {
            FieldKind::Url
        } else {
            FieldKind::Text
        };
        if kind == FieldKind::Url {
            b.url(v.get());
        } else if k.as_str() == "TOTP Settings" {
            continue;
        } else {
            b.custom(k, kind, v.get());
        }
    }
    b.tags(e.tags.iter().map(String::as_str));
    if e.times.expires == Some(true)
        && let Some(t) = e.times.expiry
    {
        b.custom(
            "Expires",
            FieldKind::Date,
            &crate::build::date_from_unix(t.and_utc().timestamp()),
        );
    }
    let ts = |t: Option<i64>| t.filter(|s| *s > 0).map(|s| s as u64);
    b.times(
        ts(e.times.creation.map(|t| t.and_utc().timestamp())),
        ts(e.times.last_modification.map(|t| t.and_utc().timestamp())),
    );
    b.note(&get("Notes"));
    b.finish()
}

fn entry(e: &keepass::db::EntryRef<'_>, folder: &str) -> crate::ImportedItem {
    let id = e.id().uuid().to_string();
    let mut item = fields_of(e, &id);
    item.folder = (!folder.is_empty()).then(|| folder.to_string());
    for (name, a) in e.attachments_named() {
        let bytes = Zeroizing::new(a.data.get().clone());
        item.attachments.push(crate::ImportedAttachment {
            name: name.to_string(),
            mime: crate::convert::mime_for(name).to_string(),
            bytes,
        });
    }
    let count = e.history.as_ref().map_or(0, |h| h.get_entries().len());
    // KeePass keeps history newest first; Lockdown's is oldest first.
    for i in (0..count).rev() {
        if let Some(old) = e.historical(i) {
            item.history.push(fields_of(&old, &id).fields);
        }
    }
    item
}
