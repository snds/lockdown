//! Bitwarden's JSON export, plain or password-protected.
//!
//! Plain: `{ "encrypted": false, "folders": [...], "items": [...] }`, each
//! item typed 1 login, 2 secure note, 3 card, 4 identity, 5 SSH key, with
//! custom `fields` (0 text, 1 hidden, 2 boolean, 3 linked) and, on a
//! login, `fido2Credentials` (passkeys).
//!
//! Password-protected: `{ "encrypted": true, "passwordProtected": true,
//! "salt", "kdfType" (0 PBKDF2-SHA-256, 1 Argon2id), "kdfIterations",
//! "kdfMemory" (MiB), "kdfParallelism", "encKeyValidation_DO_NOT_EDIT",
//! "data" }`. The key is PBKDF2 or Argon2id of the file password (Argon2id
//! salts with SHA-256 of the salt string), stretched with HKDF-Expand into
//! an encryption key (`info = "enc"`) and a MAC key (`"mac"`). `data` is a
//! type-2 cipher string, `2.<iv>|<ciphertext>|<mac>` in base64: AES-256-CBC
//! with HMAC-SHA-256 over `iv ‖ ciphertext`, checked before decrypting.
//! The KDF settings come from the file, so they are capped before use.
//!
//! An export locked to a Bitwarden account (`encrypted` without
//! `passwordProtected`) can't be opened outside Bitwarden; it is refused
//! with that sentence.

use std::collections::BTreeMap;

use lockdown_vault::{FieldKind, ItemKind};
use serde::Deserialize;
use serde_json::Value;
use zeroize::Zeroizing;

use super::{Parsed, json_err};
use crate::build::{Builder, month_of};
use crate::{CxfError, Limits, Result};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Envelope {
    #[serde(default)]
    encrypted: bool,
    #[serde(default)]
    password_protected: bool,
    #[serde(default)]
    salt: String,
    #[serde(default)]
    kdf_type: u8,
    #[serde(default)]
    kdf_iterations: u32,
    #[serde(default)]
    kdf_memory: Option<u32>,
    #[serde(default)]
    kdf_parallelism: Option<u32>,
    #[serde(default, rename = "encKeyValidation_DO_NOT_EDIT")]
    validation: String,
    #[serde(default)]
    data: String,
}

#[derive(Deserialize, Default)]
struct Export {
    #[serde(default)]
    folders: Vec<Folder>,
    #[serde(default)]
    items: Vec<Item>,
}

#[derive(Deserialize)]
struct Folder {
    id: String,
    #[serde(default)]
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Item {
    #[serde(default)]
    id: String,
    #[serde(default)]
    folder_id: Option<String>,
    #[serde(rename = "type", default)]
    kind: u8,
    #[serde(default)]
    name: String,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default)]
    favorite: bool,
    #[serde(default)]
    fields: Vec<CustomField>,
    #[serde(default)]
    login: Option<Login>,
    #[serde(default)]
    card: Option<BTreeMap<String, Value>>,
    #[serde(default)]
    identity: Option<BTreeMap<String, Value>>,
    #[serde(default)]
    ssh_key: Option<BTreeMap<String, Value>>,
    #[serde(default)]
    password_history: Option<Vec<PasswordHistory>>,
    #[serde(default)]
    deleted_date: Option<String>,
    #[serde(default)]
    attachments: Option<Vec<Value>>,
}

#[derive(Deserialize)]
struct CustomField {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    value: Option<String>,
    #[serde(rename = "type", default)]
    kind: u8,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Login {
    #[serde(default)]
    uris: Option<Vec<Uri>>,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    password: Option<String>,
    #[serde(default)]
    totp: Option<String>,
    #[serde(default)]
    fido2_credentials: Option<Vec<Fido2>>,
}

#[derive(Deserialize)]
struct Uri {
    #[serde(default)]
    uri: Option<String>,
    #[serde(rename = "match", default)]
    rule: Option<u8>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fido2 {
    #[serde(default)]
    credential_id: String,
    #[serde(default)]
    key_value: String,
    #[serde(default)]
    rp_id: String,
    #[serde(default)]
    user_handle: Option<String>,
    #[serde(default)]
    user_name: Option<String>,
    #[serde(default)]
    user_display_name: Option<String>,
}

#[derive(Deserialize)]
struct PasswordHistory {
    #[serde(default)]
    password: String,
}

/// The most PBKDF2 rounds, Argon2id memory (MiB), passes and lanes a file
/// may ask for: well above Bitwarden's own maximums, low enough that a
/// hostile file can't stall the app.
const MAX_PBKDF2: u32 = 2_000_000;
const MAX_ARGON_MIB: u32 = 1024;
const MAX_ARGON_T: u32 = 10;
const MAX_ARGON_P: u32 = 16;

/// Reads a Bitwarden JSON export, opening a password-protected one with
/// `password`.
pub fn read(bytes: &[u8], password: Option<&[u8]>, limits: &Limits) -> Result<Parsed> {
    limits.check_size(bytes)?;
    let env: Envelope = serde_json::from_slice(bytes).map_err(json_err("Bitwarden JSON"))?;
    let export: Export = if env.encrypted {
        if !env.password_protected {
            return Err(CxfError::Unsupported(
                "this Bitwarden export is locked to your Bitwarden account and only Bitwarden can open it; export again and choose \"Password protected\" or an unencrypted file".into(),
            ));
        }
        let password = password.ok_or(CxfError::PasswordNeeded)?;
        let plain = open_protected(&env, password)?;
        serde_json::from_slice(&plain).map_err(json_err("Bitwarden JSON"))?
    } else {
        serde_json::from_slice(bytes).map_err(json_err("Bitwarden JSON"))?
    };
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
        .map(|f| (f.id.as_str(), f.name.as_str()))
        .collect();
    let mut parsed = Parsed::default();
    let mut deleted = 0;
    let mut files = 0;
    for item in &export.items {
        if item.deleted_date.is_some() {
            deleted += 1;
            continue;
        }
        files += item.attachments.as_ref().map_or(0, Vec::len);
        parsed.items.push(one(item, &folders));
    }
    if deleted > 0 {
        parsed.problems.push(format!(
            "{deleted} items in Bitwarden's trash were left out"
        ));
    }
    if files > 0 {
        parsed.problems.push(format!(
            "{files} attachments are named in the file but Bitwarden doesn't put the files in a JSON export; download them from Bitwarden and add them to the items"
        ));
    }
    Ok(parsed)
}

fn kind_of(n: u8) -> ItemKind {
    match n {
        1 => ItemKind::Login,
        2 => ItemKind::Note,
        3 => ItemKind::Card,
        4 => ItemKind::Identity,
        5 => ItemKind::SshKey,
        _ => ItemKind::Custom,
    }
}

fn s(map: &BTreeMap<String, Value>, key: &str) -> String {
    match map.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

/// Bitwarden's URI match setting as a Lockdown rule (02-model §10): 0
/// base domain, 1 host, 2 starts with, 3 exact, 4 regular expression, 5
/// never; unset is Lockdown's default, the exact host.
fn rule_of(n: Option<u8>) -> (&'static str, Option<&'static str>) {
    match n {
        Some(0) => ("base_domain", None),
        Some(2) => ("starts_with", None),
        Some(4) => (
            "exact_host",
            Some("a website matched by a regular expression now matches its exact host"),
        ),
        Some(5) => ("never", None),
        _ => ("exact_host", None),
    }
}

fn b64_any(text: &str) -> Option<Vec<u8>> {
    let t = text.trim().trim_end_matches('=');
    data_encoding::BASE64URL_NOPAD
        .decode(t.as_bytes())
        .or_else(|_| data_encoding::BASE64_NOPAD.decode(t.as_bytes()))
        .ok()
}

/// A Bitwarden credential id: `b64.<base64>` or a GUID's 16 bytes.
fn credential_id(text: &str) -> Option<Vec<u8>> {
    if let Some(b) = text.strip_prefix("b64.") {
        return b64_any(b);
    }
    let hex: String = text.chars().filter(|c| *c != '-').collect();
    if hex.len() == 32 {
        return data_encoding::HEXLOWER_PERMISSIVE
            .decode(hex.as_bytes())
            .ok();
    }
    b64_any(text)
}

fn one(item: &Item, folders: &BTreeMap<&str, &str>) -> crate::ImportedItem {
    let kind = kind_of(item.kind);
    let mut b = Builder::new(item.id.clone(), kind, &item.name);
    b.favourite(item.favorite);
    if let Some(f) = item.folder_id.as_deref().and_then(|id| folders.get(id)) {
        b.folder(f);
    }
    if let Some(login) = &item.login {
        b.field("user", login.username.as_deref().unwrap_or(""));
        b.field("pw", login.password.as_deref().unwrap_or(""));
        if let Some(t) = &login.totp {
            b.totp(t);
        }
        for u in login.uris.iter().flatten() {
            let (rule, note) = rule_of(u.rule);
            b.url_with_rule(u.uri.as_deref().unwrap_or(""), rule);
            if let Some(n) = note {
                b.report(n);
            }
        }
        for p in login.fido2_credentials.iter().flatten() {
            match (credential_id(&p.credential_id), b64_any(&p.key_value)) {
                (Some(id), Some(key)) => {
                    let handle = p
                        .user_handle
                        .as_deref()
                        .and_then(b64_any)
                        .unwrap_or_default();
                    b.passkey(
                        &p.rp_id,
                        p.user_name.as_deref().unwrap_or(""),
                        p.user_display_name.as_deref().unwrap_or(""),
                        id,
                        handle,
                        key,
                    );
                }
                _ => {
                    b.report(format!(
                        "the passkey for {} didn't read and was left out",
                        p.rp_id
                    ));
                }
            }
        }
    }
    if let Some(c) = &item.card {
        b.field("card.holder", &s(c, "cardholderName"))
            .field("card.type", &s(c, "brand"))
            .field("card.number", &s(c, "number"))
            .field("card.cvc", &s(c, "code"));
        let (m, y) = (s(c, "expMonth"), s(c, "expYear"));
        if let Some(month) = month_of(&m, &y) {
            b.field("card.expiry", &month);
        } else if !(m.is_empty() && y.is_empty()) {
            b.custom("Expires", FieldKind::Text, &format!("{m}/{y}"));
        }
    }
    if let Some(i) = &item.identity {
        b.field("name.given", &s(i, "firstName"))
            .field("name.middle", &s(i, "middleName"))
            .field("name.family", &s(i, "lastName"))
            .field("identity.email", &s(i, "email"))
            .field("address.tel", &s(i, "phone"));
        let street = ["address1", "address2", "address3"]
            .iter()
            .map(|k| s(i, k))
            .filter(|v| !v.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        b.field("address.street", &street.replace('\n', ", "))
            .field("address.city", &s(i, "city"))
            .field("address.territory", &s(i, "state"))
            .field("address.postal_code", &s(i, "postalCode"))
            .field("address.country", &s(i, "country"));
        for (key, label, kind) in [
            ("title", "Title", FieldKind::Text),
            ("company", "Company", FieldKind::Text),
            ("username", "Username", FieldKind::Text),
            ("ssn", "Social security number", FieldKind::Hidden),
            ("passportNumber", "Passport number", FieldKind::Hidden),
            ("licenseNumber", "Licence number", FieldKind::Hidden),
        ] {
            b.custom(label, kind, &s(i, key));
        }
    }
    if let Some(k) = &item.ssh_key {
        b.field("ssh.private_key", &s(k, "privateKey"))
            .field("ssh.public_key", &s(k, "publicKey"))
            .field("ssh.fingerprint", &s(k, "keyFingerprint"));
    }
    for f in &item.fields {
        let label = f.name.as_deref().unwrap_or("Field");
        let value = f.value.as_deref().unwrap_or("");
        match f.kind {
            1 => b.custom(label, FieldKind::Hidden, value),
            2 => b.custom(label, FieldKind::Boolean, value),
            3 => b.report(format!(
                "'{label}' is a linked field in Bitwarden; it points at another field and holds no value of its own"
            )),
            _ => b.custom(label, FieldKind::Text, value),
        };
    }
    if let Some(n) = &item.notes {
        b.note(n);
    }
    let mut out = b.finish();
    if let Some(history) = &item.password_history
        && out.fields.iter().any(|f| f.field_id == "pw")
    {
        // Bitwarden lists the newest old password first.
        for old in history.iter().rev().filter(|h| !h.password.is_empty()) {
            let fields = out
                .fields
                .iter()
                .map(|f| {
                    if f.field_id == "pw" {
                        lockdown_vault::PlainField::hidden("pw", &f.label, &old.password)
                    } else {
                        f.clone()
                    }
                })
                .collect();
            out.history.push(fields);
        }
    }
    out
}

/// The two keys Bitwarden stretches its file key into.
struct Keys {
    enc: Zeroizing<[u8; 32]>,
    mac: Zeroizing<[u8; 32]>,
}

fn derive(env: &Envelope, password: &[u8]) -> Result<Keys> {
    let mut master = Zeroizing::new([0u8; 32]);
    match env.kdf_type {
        0 => {
            if env.kdf_iterations == 0 || env.kdf_iterations > MAX_PBKDF2 {
                return Err(CxfError::Unsupported(format!(
                    "the file asks for {} PBKDF2 rounds, outside 1–{MAX_PBKDF2}",
                    env.kdf_iterations
                )));
            }
            pbkdf2::pbkdf2_hmac::<sha2::Sha256>(
                password,
                env.salt.as_bytes(),
                env.kdf_iterations,
                master.as_mut(),
            );
        }
        1 => {
            let m = env.kdf_memory.unwrap_or(64);
            let p = env.kdf_parallelism.unwrap_or(4);
            let t = env.kdf_iterations;
            if !(1..=MAX_ARGON_MIB).contains(&m)
                || !(1..=MAX_ARGON_T).contains(&t)
                || !(1..=MAX_ARGON_P).contains(&p)
            {
                return Err(CxfError::Unsupported(format!(
                    "the file asks for Argon2id with {m} MiB, {t} passes, {p} lanes, outside what Lockdown accepts"
                )));
            }
            use sha2::Digest;
            let salt = sha2::Sha256::digest(env.salt.as_bytes());
            let params = argon2::Params::new(m * 1024, t, p, Some(32))
                .map_err(|e| CxfError::Unsupported(format!("Argon2id settings: {e}")))?;
            argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params)
                .hash_password_into(password, &salt, master.as_mut())
                .map_err(|e| CxfError::Unsupported(format!("Argon2id: {e}")))?;
        }
        other => {
            return Err(CxfError::Unsupported(format!(
                "Bitwarden key derivation type {other}"
            )));
        }
    }
    let hk = hkdf::Hkdf::<sha2::Sha256>::from_prk(master.as_ref())
        .map_err(|_| CxfError::WrongPassword)?;
    let mut enc = Zeroizing::new([0u8; 32]);
    let mut mac = Zeroizing::new([0u8; 32]);
    hk.expand(b"enc", enc.as_mut())
        .map_err(|_| CxfError::WrongPassword)?;
    hk.expand(b"mac", mac.as_mut())
        .map_err(|_| CxfError::WrongPassword)?;
    Ok(Keys { enc, mac })
}

/// Opens a type-2 cipher string: checks the MAC, then decrypts.
fn open_cipher_string(text: &str, keys: &Keys) -> Result<Zeroizing<Vec<u8>>> {
    use cbc::cipher::{BlockModeDecrypt, KeyIvInit, block_padding::Pkcs7};
    use hmac::{KeyInit, Mac};
    let body = text
        .strip_prefix("2.")
        .ok_or_else(|| CxfError::Unsupported("a Bitwarden cipher of a type other than 2".into()))?;
    let parts: Vec<&str> = body.split('|').collect();
    let [iv, ct, tag] = parts.as_slice() else {
        return Err(CxfError::Parse {
            format: "Bitwarden JSON",
            why: "a cipher string isn't iv|data|mac".into(),
        });
    };
    let bad = || CxfError::Parse {
        format: "Bitwarden JSON",
        why: "a cipher string isn't valid base64".into(),
    };
    let iv = data_encoding::BASE64
        .decode(iv.as_bytes())
        .map_err(|_| bad())?;
    let ct = data_encoding::BASE64
        .decode(ct.as_bytes())
        .map_err(|_| bad())?;
    let tag = data_encoding::BASE64
        .decode(tag.as_bytes())
        .map_err(|_| bad())?;
    let mut m = <hmac::Hmac<sha2::Sha256> as KeyInit>::new_from_slice(keys.mac.as_ref())
        .map_err(|_| CxfError::WrongPassword)?;
    m.update(&iv);
    m.update(&ct);
    m.verify_slice(&tag).map_err(|_| CxfError::WrongPassword)?;
    let dec = cbc::Decryptor::<aes::Aes256>::new_from_slices(keys.enc.as_ref(), &iv)
        .map_err(|_| CxfError::WrongPassword)?;
    let mut out = Zeroizing::new(vec![0u8; ct.len()]);
    let n = dec
        .decrypt_padded_b2b::<Pkcs7>(&ct, &mut out)
        .map_err(|_| CxfError::WrongPassword)?
        .len();
    out.truncate(n);
    Ok(out)
}

fn open_protected(env: &Envelope, password: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    let keys = derive(env, password)?;
    open_cipher_string(&env.validation, &keys)?;
    open_cipher_string(&env.data, &keys)
}

/// Seals `plain` the way Bitwarden's password-protected export does, for
/// fixtures and tests (PBKDF2, `iterations` rounds).
pub fn seal_for_tests(plain: &[u8], password: &[u8], salt: &str, iterations: u32) -> String {
    use cbc::cipher::{BlockModeEncrypt, KeyIvInit, block_padding::Pkcs7};
    use hmac::{KeyInit, Mac};
    let env = Envelope {
        encrypted: true,
        password_protected: true,
        salt: salt.into(),
        kdf_type: 0,
        kdf_iterations: iterations,
        kdf_memory: None,
        kdf_parallelism: None,
        validation: String::new(),
        data: String::new(),
    };
    let keys = derive(&env, password).expect("pbkdf2");
    let seal = |data: &[u8], iv: [u8; 16]| {
        let enc = cbc::Encryptor::<aes::Aes256>::new_from_slices(keys.enc.as_ref(), &iv).unwrap();
        let ct = enc.encrypt_padded_vec::<Pkcs7>(data);
        let mut m =
            <hmac::Hmac<sha2::Sha256> as KeyInit>::new_from_slice(keys.mac.as_ref()).unwrap();
        m.update(&iv);
        m.update(&ct);
        let tag = m.finalize().into_bytes();
        format!(
            "2.{}|{}|{}",
            data_encoding::BASE64.encode(&iv),
            data_encoding::BASE64.encode(&ct),
            data_encoding::BASE64.encode(&tag)
        )
    };
    serde_json::json!({
        "encrypted": true,
        "passwordProtected": true,
        "salt": salt,
        "kdfType": 0,
        "kdfIterations": iterations,
        "kdfMemory": null,
        "kdfParallelism": null,
        "encKeyValidation_DO_NOT_EDIT": seal(b"0b6c2a5e-4b9e-4a39-9b1f-6a1b3d5e7f90", [7u8; 16]),
        "data": seal(plain, [9u8; 16]),
    })
    .to_string()
}
