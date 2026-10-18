//! Authenticator apps' exports (docs/plan/23-bridges.md §1): Google
//! Authenticator's transfer batches (`otpauth-migration://`), Aegis (plain
//! and password-encrypted), 2FAS (plain and encrypted), Ente Auth (plain
//! and encrypted), Proton Authenticator and Bitwarden Authenticator, plus
//! any file of `otpauth://` lines.
//!
//! Every one lands as Code items through the same [`Builder`], so a secret
//! is checked by `lockdown-codes` before it is kept and a code that
//! doesn't parse is reported by name, never kept half-read. The files are
//! hostile like any import: sizes are capped by the caller, a key
//! derivation a file asks for is capped before it runs, and the
//! authenticated ciphers (AES-GCM, XChaCha20-Poly1305) refuse a changed
//! byte before anything is parsed.

use lockdown_codes::{Algorithm, Code, Secret, Variant};
use lockdown_vault::ItemKind;
use serde::Deserialize;
use serde_json::Value;
use zeroize::Zeroizing;

use super::{Parsed, json_err};
use crate::build::Builder;
use crate::model::b64url;
use crate::{CxfError, Limits, Result};

/// The most codes one file may hold.
const MAX_CODES: usize = 10_000;

fn parse_err(format: &'static str, why: impl Into<String>) -> CxfError {
    CxfError::Parse {
        format,
        why: why.into(),
    }
}

/// One code as an item: titled by its issuer (else its account), the
/// set-up checked and written as an `otpauth://` link.
#[allow(clippy::too_many_arguments)]
fn code_item(
    source: &str,
    id: &str,
    code: &Code,
    name: Option<&str>,
    note: Option<&str>,
    folder: Option<&str>,
    favourite: bool,
) -> crate::ImportedItem {
    let title = name
        .filter(|n| !n.trim().is_empty())
        .map(str::to_string)
        .or_else(|| code.issuer.clone())
        .or_else(|| code.account.clone())
        .unwrap_or_else(|| "One-time code".into());
    let mut b = Builder::new(
        b64url(format!("{source}:{id}").as_bytes()),
        ItemKind::Code,
        &title,
    );
    b.totp(&code.to_otpauth());
    if let Some(n) = note {
        b.note(n);
    }
    if let Some(f) = folder.filter(|f| !f.trim().is_empty()) {
        b.folder(f);
    }
    b.favourite(favourite);
    b.finish()
}

fn algorithm(name: Option<&str>) -> std::result::Result<Algorithm, String> {
    match name.map(str::to_ascii_uppercase).as_deref() {
        None | Some("") | Some("SHA1") | Some("SHA-1") => Ok(Algorithm::Sha1),
        Some("SHA256") | Some("SHA-256") => Ok(Algorithm::Sha256),
        Some("SHA512") | Some("SHA-512") => Ok(Algorithm::Sha512),
        Some(other) => Err(format!(
            "its hash ({other}) isn't one Lockdown makes codes with"
        )),
    }
}

/// A code from its parts, checked.
#[allow(clippy::too_many_arguments)]
fn build_code(
    kind: &str,
    secret: Secret,
    algo: Option<&str>,
    digits: Option<u32>,
    period: Option<u32>,
    counter: Option<u64>,
    issuer: Option<&str>,
    account: Option<&str>,
) -> std::result::Result<Code, String> {
    let mut code = match kind.to_ascii_lowercase().as_str() {
        "totp" | "" => Code::totp(secret),
        "hotp" => {
            let mut c = Code::totp(secret);
            c.variant = Variant::Hotp;
            c.counter = counter.unwrap_or(0);
            c
        }
        "steam" => Code::steam(secret),
        other => return Err(format!("it is a {other} code, which Lockdown doesn't make")),
    };
    if code.variant != Variant::Steam {
        code.algorithm = algorithm(algo)?;
        if let Some(d) = digits {
            code.digits = d;
        }
    }
    if let Some(p) = period.filter(|p| *p > 0) {
        code.period = p;
    }
    code.issuer = issuer
        .map(str::trim)
        .filter(|i| !i.is_empty())
        .map(str::to_string)
        .or(code.issuer);
    code.account = account
        .map(str::trim)
        .filter(|a| !a.is_empty())
        .map(str::to_string);
    code.validate().map_err(|e| e.to_string())?;
    Ok(code)
}

fn label_of(issuer: Option<&str>, account: Option<&str>) -> String {
    match (issuer, account) {
        (Some(i), Some(a)) if !i.is_empty() && !a.is_empty() => format!("{i} ({a})"),
        (Some(i), _) if !i.is_empty() => i.to_string(),
        (_, Some(a)) if !a.is_empty() => a.to_string(),
        _ => "a code".into(),
    }
}

// --- Google Authenticator transfer batches ------------------------------------

/// A protobuf field as read: its number and its value.
enum Wire<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
}

/// Reads one protobuf message's fields (varint, 64-bit, length-delimited
/// and 32-bit; groups are refused). Hostile input: every length is
/// checked against what is left.
fn protobuf_fields(mut data: &[u8]) -> std::result::Result<Vec<(u32, Wire<'_>)>, String> {
    fn varint(data: &mut &[u8]) -> std::result::Result<u64, String> {
        let mut out = 0u64;
        for i in 0..10 {
            let (&b, rest) = data.split_first().ok_or("cut short")?;
            *data = rest;
            out |= u64::from(b & 0x7f) << (7 * i);
            if b & 0x80 == 0 {
                return Ok(out);
            }
        }
        Err("a number is too long".into())
    }
    let mut fields = Vec::new();
    while !data.is_empty() {
        let key = varint(&mut data)?;
        let number = u32::try_from(key >> 3).map_err(|_| "a field number is too large")?;
        let value = match key & 7 {
            0 => Wire::Varint(varint(&mut data)?),
            1 => {
                if data.len() < 8 {
                    return Err("cut short".into());
                }
                data = &data[8..];
                continue;
            }
            2 => {
                let len = usize::try_from(varint(&mut data)?).map_err(|_| "too long")?;
                if len > data.len() {
                    return Err("a field runs past the end".into());
                }
                let (v, rest) = data.split_at(len);
                data = rest;
                Wire::Bytes(v)
            }
            5 => {
                if data.len() < 4 {
                    return Err("cut short".into());
                }
                data = &data[4..];
                continue;
            }
            other => return Err(format!("wire type {other} isn't used by this format")),
        };
        fields.push((number, value));
    }
    Ok(fields)
}

/// One batch's header and its codes, still as raw parameters.
struct Batch {
    size: u64,
    index: u64,
    id: u64,
    codes: Vec<Vec<u8>>,
}

fn read_batch(payload: &[u8]) -> std::result::Result<Batch, String> {
    let mut b = Batch {
        size: 1,
        index: 0,
        id: 0,
        codes: Vec::new(),
    };
    for (n, v) in protobuf_fields(payload)? {
        match (n, v) {
            (1, Wire::Bytes(p)) => b.codes.push(p.to_vec()),
            (3, Wire::Varint(v)) => b.size = v.max(1),
            (4, Wire::Varint(v)) => b.index = v,
            (5, Wire::Varint(v)) => b.id = v,
            _ => {}
        }
    }
    Ok(b)
}

fn google_code(params: &[u8]) -> std::result::Result<Code, String> {
    let mut secret = Zeroizing::new(Vec::new());
    let (mut name, mut issuer) = (String::new(), String::new());
    let (mut algo, mut digits, mut kind, mut counter) = (1u64, 1u64, 2u64, 0u64);
    for (n, v) in protobuf_fields(params)? {
        match (n, v) {
            (1, Wire::Bytes(s)) => secret = Zeroizing::new(s.to_vec()),
            (2, Wire::Bytes(s)) => name = String::from_utf8_lossy(s).into_owned(),
            (3, Wire::Bytes(s)) => issuer = String::from_utf8_lossy(s).into_owned(),
            (4, Wire::Varint(v)) => algo = v,
            (5, Wire::Varint(v)) => digits = v,
            (6, Wire::Varint(v)) => kind = v,
            (7, Wire::Varint(v)) => counter = v,
            _ => {}
        }
    }
    if secret.is_empty() {
        return Err("it has no secret".into());
    }
    let algo = match algo {
        0 | 1 => "SHA1",
        2 => "SHA256",
        3 => "SHA512",
        4 => "MD5",
        _ => "unknown",
    };
    let digits = match digits {
        2 => 8,
        _ => 6,
    };
    let kind = match kind {
        1 => "hotp",
        _ => "totp",
    };
    // The name is "Issuer:account" or just the account.
    let account = match name.split_once(':') {
        Some((i, a)) if issuer.is_empty() || i.trim() == issuer => a.trim().to_string(),
        _ => name.trim().to_string(),
    };
    build_code(
        kind,
        Secret::from_bytes(secret.to_vec()),
        Some(algo),
        Some(digits),
        None,
        Some(counter),
        Some(&issuer),
        Some(&account),
    )
}

fn percent_decoded(text: &str) -> Vec<u8> {
    let b = text.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && let Some(Ok(v)) = text.get(i + 1..i + 3).map(|h| u8::from_str_radix(h, 16))
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    out
}

/// Reads one or more `otpauth-migration://offline?data=…` links (one per
/// line, as a QR reader or the app's own export gives them). Batches that
/// belong together are put back in order; a missing one is reported.
pub fn read_google(bytes: &[u8]) -> Result<Parsed> {
    const F: &str = "Google Authenticator";
    let text = std::str::from_utf8(bytes).map_err(|_| parse_err(F, "the file isn't text"))?;
    let mut parsed = Parsed::default();
    let mut batches: Vec<Batch> = Vec::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let Some(rest) = line.strip_prefix("otpauth-migration://offline?") else {
            parsed
                .problems
                .push("a line that isn't a transfer link was skipped".into());
            continue;
        };
        let data = rest
            .split('&')
            .find_map(|kv| kv.strip_prefix("data="))
            .ok_or_else(|| parse_err(F, "a transfer link has no data"))?;
        let raw = percent_decoded(data);
        let raw: Vec<u8> = raw
            .into_iter()
            .filter(|b| !b.is_ascii_whitespace())
            .collect();
        let payload = data_encoding::BASE64
            .decode(&raw)
            .or_else(|_| data_encoding::BASE64_NOPAD.decode(&raw))
            .map_err(|_| parse_err(F, "a transfer link's data isn't base64"))?;
        batches.push(read_batch(&payload).map_err(|e| parse_err(F, e))?);
    }
    if batches.is_empty() {
        return Err(parse_err(F, "there's no otpauth-migration:// link in it"));
    }
    batches.sort_by_key(|b| (b.id, b.index));
    batches.dedup_by_key(|b| (b.id, b.index));
    let mut n = 0usize;
    let mut groups: Vec<(u64, u64, Vec<u64>)> = Vec::new();
    for b in &batches {
        match groups.iter_mut().find(|g| g.0 == b.id) {
            Some(g) => g.2.push(b.index),
            None => groups.push((b.id, b.size, vec![b.index])),
        }
        for params in &b.codes {
            n += 1;
            if n > MAX_CODES {
                return Err(CxfError::TooMany {
                    what: "codes",
                    found: n,
                    limit: MAX_CODES,
                });
            }
            match google_code(params) {
                Ok(code) => parsed.items.push(code_item(
                    "google",
                    &format!("{}-{}-{n}", b.id, b.index),
                    &code,
                    None,
                    None,
                    None,
                    false,
                )),
                Err(why) => parsed
                    .problems
                    .push(format!("code {n} wasn't brought in: {why}")),
            }
        }
    }
    for (_, size, have) in groups {
        if (have.len() as u64) < size {
            parsed.problems.push(format!(
                "this transfer had {size} QR codes and {} were read; scan the rest and import them too",
                have.len()
            ));
        }
    }
    Ok(parsed)
}

/// One code as a transfer batch carries it: secret, name, issuer,
/// algorithm, digits, type and counter (the protobuf's numbers).
#[cfg(test)]
type GoogleParams<'a> = (&'a [u8], &'a str, &'a str, u8, u8, u8, u64);

/// Writes a transfer batch, for the tests: the protobuf Google
/// Authenticator's export carries.
#[cfg(test)]
fn google_batch(codes: &[GoogleParams<'_>], index: u8, size: u8, id: u32) -> String {
    fn varint(mut v: u64, out: &mut Vec<u8>) {
        loop {
            let b = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                out.push(b);
                return;
            }
            out.push(b | 0x80);
        }
    }
    fn bytes(n: u32, v: &[u8], out: &mut Vec<u8>) {
        varint(u64::from(n << 3 | 2), out);
        varint(v.len() as u64, out);
        out.extend_from_slice(v);
    }
    fn num(n: u32, v: u64, out: &mut Vec<u8>) {
        varint(u64::from(n << 3), out);
        varint(v, out);
    }
    let mut payload = Vec::new();
    for (secret, name, issuer, algo, digits, kind, counter) in codes {
        let mut p = Vec::new();
        bytes(1, secret, &mut p);
        bytes(2, name.as_bytes(), &mut p);
        bytes(3, issuer.as_bytes(), &mut p);
        num(4, u64::from(*algo), &mut p);
        num(5, u64::from(*digits), &mut p);
        num(6, u64::from(*kind), &mut p);
        num(7, *counter, &mut p);
        bytes(1, &p, &mut payload);
    }
    num(2, 1, &mut payload);
    num(3, u64::from(size), &mut payload);
    num(4, u64::from(index), &mut payload);
    num(5, u64::from(id), &mut payload);
    let data = data_encoding::BASE64.encode(&payload);
    let data: String = data
        .chars()
        .map(|c| match c {
            '+' => "%2B".to_string(),
            '/' => "%2F".to_string(),
            '=' => "%3D".to_string(),
            c => c.to_string(),
        })
        .collect();
    format!("otpauth-migration://offline?data={data}")
}

// --- plain otpauth:// lines (Ente's plain export, WinAuth, any list) ---------

/// Reads every `otpauth://` (or `steam://`) link in a text file, one per
/// line; anything else is skipped and counted.
pub fn read_otpauth_lines(bytes: &[u8], source: &'static str) -> Result<Parsed> {
    let text = std::str::from_utf8(bytes).map_err(|_| parse_err(source, "the file isn't text"))?;
    let mut parsed = Parsed::default();
    let mut skipped = 0;
    for (n, line) in text.lines().map(str::trim).enumerate() {
        if line.is_empty() {
            continue;
        }
        if !(line.starts_with("otpauth://") || line.starts_with("steam://")) {
            skipped += 1;
            continue;
        }
        if parsed.items.len() >= MAX_CODES {
            return Err(CxfError::TooMany {
                what: "codes",
                found: parsed.items.len() + 1,
                limit: MAX_CODES,
            });
        }
        match lockdown_codes::parse_uri(line) {
            Ok(code) => parsed.items.push(code_item(
                source,
                &format!("line-{}", n + 1),
                &code,
                None,
                None,
                None,
                false,
            )),
            Err(e) => parsed
                .problems
                .push(format!("line {} wasn't brought in: {e}", n + 1)),
        }
    }
    if parsed.items.is_empty() && parsed.problems.is_empty() {
        return Err(parse_err(source, "there's no otpauth:// link in it"));
    }
    if skipped > 0 {
        parsed.problems.push(format!(
            "{skipped} lines that aren't code links were skipped"
        ));
    }
    Ok(parsed)
}

// --- Aegis --------------------------------------------------------------------

#[derive(Deserialize)]
struct AegisFile {
    header: AegisHeader,
    db: Value,
}

#[derive(Deserialize)]
struct AegisHeader {
    slots: Option<Vec<AegisSlot>>,
    params: Option<AegisParams>,
}

#[derive(Deserialize)]
struct AegisSlot {
    #[serde(rename = "type")]
    kind: u32,
    key: String,
    key_params: AegisParams,
    #[serde(default)]
    n: u64,
    #[serde(default)]
    r: u32,
    #[serde(default)]
    p: u32,
    #[serde(default)]
    salt: String,
}

#[derive(Deserialize)]
struct AegisParams {
    nonce: String,
    tag: String,
}

#[derive(Deserialize)]
struct AegisDb {
    #[serde(default)]
    entries: Vec<AegisEntry>,
    #[serde(default)]
    groups: Vec<AegisGroup>,
}

#[derive(Deserialize)]
struct AegisGroup {
    uuid: String,
    name: String,
}

#[derive(Deserialize)]
struct AegisEntry {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    uuid: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    issuer: String,
    #[serde(default)]
    note: String,
    #[serde(default)]
    favorite: bool,
    #[serde(default)]
    group: Option<String>,
    #[serde(default)]
    groups: Vec<String>,
    info: AegisInfo,
}

#[derive(Deserialize)]
struct AegisInfo {
    secret: String,
    #[serde(default)]
    algo: Option<String>,
    #[serde(default)]
    digits: Option<u32>,
    #[serde(default)]
    period: Option<u32>,
    #[serde(default)]
    counter: Option<u64>,
}

fn hex(text: &str, what: &str) -> Result<Zeroizing<Vec<u8>>> {
    data_encoding::HEXLOWER_PERMISSIVE
        .decode(text.as_bytes())
        .map(Zeroizing::new)
        .map_err(|_| parse_err("Aegis", format!("its {what} isn't hex")))
}

/// AES-256-GCM, the tag apart (as Aegis and 2FAS keep it, or appended).
fn aes_gcm_open(key: &[u8], nonce: &[u8], ciphertext_and_tag: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    use aes_gcm::aead::{Aead, KeyInit};
    let cipher = aes_gcm::Aes256Gcm::new_from_slice(key).map_err(|_| CxfError::WrongPassword)?;
    let nonce = aes_gcm::Nonce::try_from(nonce).map_err(|_| CxfError::WrongPassword)?;
    cipher
        .decrypt(&nonce, ciphertext_and_tag)
        .map(Zeroizing::new)
        .map_err(|_| CxfError::WrongPassword)
}

/// The most scrypt a file may ask for: 2^20 rounds, r 32, p 16, 1 GiB.
fn capped_scrypt(
    password: &[u8],
    salt: &[u8],
    n: u64,
    r: u32,
    p: u32,
) -> Result<Zeroizing<[u8; 32]>> {
    if !n.is_power_of_two()
        || !(2..=1 << 20).contains(&n)
        || !(1..=32).contains(&r)
        || !(1..=16).contains(&p)
    {
        return Err(parse_err(
            "Aegis",
            format!(
                "its key-derivation settings (N={n}, r={r}, p={p}) are outside what Lockdown runs"
            ),
        ));
    }
    if 128u64 * n * u64::from(r) > 1 << 30 {
        return Err(parse_err(
            "Aegis",
            "its key derivation would need more than 1 GiB",
        ));
    }
    let params = scrypt::Params::new(n.trailing_zeros() as u8, r, p)
        .map_err(|_| parse_err("Aegis", "its key-derivation settings are invalid"))?;
    let mut out = Zeroizing::new([0u8; 32]);
    scrypt::scrypt(password, salt, &params, out.as_mut())
        .map_err(|_| parse_err("Aegis", "its key derivation failed"))?;
    Ok(out)
}

/// Reads an Aegis vault export, plain or encrypted with a password.
pub fn read_aegis(bytes: &[u8], password: Option<&[u8]>, limits: &Limits) -> Result<Parsed> {
    const F: &str = "Aegis";
    let _ = limits;
    let file: AegisFile = serde_json::from_slice(bytes).map_err(json_err(F))?;
    let db: AegisDb = match (&file.header.slots, &file.db) {
        (None, db) | (Some(_), db) if db.is_object() => {
            serde_json::from_value(file.db.clone()).map_err(json_err(F))?
        }
        (Some(slots), Value::String(sealed)) => {
            let password = password.ok_or(CxfError::PasswordNeeded)?;
            let params = file
                .header
                .params
                .as_ref()
                .ok_or_else(|| parse_err(F, "the encrypted file has no parameters"))?;
            let mut master = None;
            for slot in slots.iter().filter(|s| s.kind == 1) {
                let key =
                    capped_scrypt(password, &hex(&slot.salt, "salt")?, slot.n, slot.r, slot.p)?;
                let mut wrapped = hex(&slot.key, "slot key")?.to_vec();
                wrapped.extend_from_slice(&hex(&slot.key_params.tag, "slot tag")?);
                if let Ok(m) = aes_gcm_open(
                    key.as_ref(),
                    &hex(&slot.key_params.nonce, "slot nonce")?,
                    &wrapped,
                ) {
                    master = Some(m);
                    break;
                }
            }
            if master.is_none() && !slots.iter().any(|s| s.kind == 1) {
                return Err(parse_err(
                    F,
                    "this file opens only with a fingerprint or a security key on the phone; export it again with a password",
                ));
            }
            let master = master.ok_or(CxfError::WrongPassword)?;
            let mut sealed = data_encoding::BASE64
                .decode(sealed.as_bytes())
                .map_err(|_| parse_err(F, "the encrypted vault isn't base64"))?;
            sealed.extend_from_slice(&hex(&params.tag, "tag")?);
            let plain = aes_gcm_open(&master, &hex(&params.nonce, "nonce")?, &sealed)?;
            serde_json::from_slice(&plain).map_err(json_err(F))?
        }
        _ => return Err(parse_err(F, "the vault is neither plain nor encrypted")),
    };
    let mut parsed = Parsed::default();
    if db.entries.len() > MAX_CODES {
        return Err(CxfError::TooMany {
            what: "codes",
            found: db.entries.len(),
            limit: MAX_CODES,
        });
    }
    for (i, e) in db.entries.iter().enumerate() {
        let folder = e.group.clone().or_else(|| {
            e.groups
                .first()
                .and_then(|g| db.groups.iter().find(|x| &x.uuid == g))
                .map(|g| g.name.clone())
        });
        if !matches!(
            e.kind.to_ascii_lowercase().as_str(),
            "totp" | "hotp" | "steam"
        ) {
            parsed.problems.push(format!(
                "{} wasn't brought in: it is a {} code, which Lockdown doesn't make",
                label_of(Some(&e.issuer), Some(&e.name)),
                e.kind
            ));
            continue;
        }
        let secret = match Secret::from_base32(&e.info.secret) {
            Ok(s) => s,
            Err(_) => {
                parsed.problems.push(format!(
                    "{} wasn't brought in: its secret isn't base32",
                    label_of(Some(&e.issuer), Some(&e.name))
                ));
                continue;
            }
        };
        match build_code(
            &e.kind,
            secret,
            e.info.algo.as_deref(),
            e.info.digits,
            e.info.period,
            e.info.counter,
            Some(&e.issuer),
            Some(&e.name),
        ) {
            Ok(code) => parsed.items.push(code_item(
                "aegis",
                if e.uuid.is_empty() { "" } else { &e.uuid },
                &code,
                None,
                Some(&e.note).filter(|n| !n.is_empty()).map(String::as_str),
                folder.as_deref(),
                e.favorite,
            )),
            Err(why) => parsed.problems.push(format!(
                "{} wasn't brought in: {why}",
                label_of(Some(&e.issuer), Some(&e.name))
            )),
        }
        let _ = i;
    }
    Ok(parsed)
}

// --- 2FAS ---------------------------------------------------------------------

#[derive(Deserialize)]
struct TwoFasFile {
    #[serde(default)]
    services: Vec<TwoFasService>,
    #[serde(default, rename = "servicesEncrypted")]
    services_encrypted: Option<String>,
    #[serde(default)]
    groups: Vec<TwoFasGroup>,
    #[serde(default, rename = "schemaVersion")]
    schema_version: u32,
}

#[derive(Deserialize)]
struct TwoFasGroup {
    id: String,
    name: String,
}

#[derive(Deserialize)]
struct TwoFasService {
    #[serde(default)]
    name: String,
    secret: String,
    #[serde(default)]
    otp: TwoFasOtp,
    #[serde(default, rename = "groupId")]
    group_id: Option<String>,
    #[serde(default, rename = "updatedAt")]
    updated_at: Option<u64>,
}

#[derive(Deserialize, Default)]
struct TwoFasOtp {
    #[serde(default)]
    account: Option<String>,
    #[serde(default)]
    issuer: Option<String>,
    #[serde(default)]
    digits: Option<u32>,
    #[serde(default)]
    period: Option<u32>,
    #[serde(default)]
    algorithm: Option<String>,
    #[serde(default)]
    counter: Option<u64>,
    #[serde(default, rename = "tokenType")]
    token_type: Option<String>,
}

/// 2FAS's PBKDF2 rounds.
const TWOFAS_ROUNDS: u32 = 10_000;

/// Reads a 2FAS backup (`.2fas`), plain or encrypted with a password.
pub fn read_2fas(bytes: &[u8], password: Option<&[u8]>) -> Result<Parsed> {
    const F: &str = "2FAS";
    let file: TwoFasFile = serde_json::from_slice(bytes).map_err(json_err(F))?;
    let _ = file.schema_version;
    let services: Vec<TwoFasService> = match &file.services_encrypted {
        Some(sealed) if !sealed.is_empty() => {
            let password = password.ok_or(CxfError::PasswordNeeded)?;
            let mut parts = sealed.split(':');
            let (Some(data), Some(salt), Some(iv), None) =
                (parts.next(), parts.next(), parts.next(), parts.next())
            else {
                return Err(parse_err(F, "the encrypted services aren't data:salt:iv"));
            };
            let b64 = |t: &str| {
                data_encoding::BASE64
                    .decode(t.as_bytes())
                    .map_err(|_| parse_err(F, "the encrypted services aren't base64"))
            };
            let (data, salt, iv) = (b64(data)?, b64(salt)?, b64(iv)?);
            let mut key = Zeroizing::new([0u8; 32]);
            pbkdf2::pbkdf2_hmac::<sha2::Sha256>(password, &salt, TWOFAS_ROUNDS, key.as_mut());
            let plain = aes_gcm_open(key.as_ref(), &iv, &data)?;
            serde_json::from_slice(&plain).map_err(json_err(F))?
        }
        _ => file.services,
    };
    if services.len() > MAX_CODES {
        return Err(CxfError::TooMany {
            what: "codes",
            found: services.len(),
            limit: MAX_CODES,
        });
    }
    let mut parsed = Parsed::default();
    for (i, s) in services.iter().enumerate() {
        let issuer = s
            .otp
            .issuer
            .clone()
            .filter(|i| !i.is_empty())
            .unwrap_or(s.name.clone());
        let label = label_of(Some(&issuer), s.otp.account.as_deref());
        let secret = match Secret::from_base32(&s.secret) {
            Ok(x) => x,
            Err(_) => {
                parsed.problems.push(format!(
                    "{label} wasn't brought in: its secret isn't base32"
                ));
                continue;
            }
        };
        let folder = s
            .group_id
            .as_ref()
            .and_then(|g| file.groups.iter().find(|x| &x.id == g))
            .map(|g| g.name.clone());
        match build_code(
            s.otp.token_type.as_deref().unwrap_or("totp"),
            secret,
            s.otp.algorithm.as_deref(),
            s.otp.digits,
            s.otp.period,
            s.otp.counter,
            Some(&issuer),
            s.otp.account.as_deref(),
        ) {
            Ok(code) => {
                let mut item = code_item(
                    "2fas",
                    &format!("{i}"),
                    &code,
                    Some(&s.name),
                    None,
                    folder.as_deref(),
                    false,
                );
                item.modified_at = s.updated_at.map(|t| t / 1000);
                parsed.items.push(item);
            }
            Err(why) => parsed
                .problems
                .push(format!("{label} wasn't brought in: {why}")),
        }
    }
    Ok(parsed)
}

// --- Ente Auth ------------------------------------------------------------------

#[derive(Deserialize)]
struct EnteFile {
    #[serde(default)]
    version: u32,
    #[serde(rename = "kdfParams")]
    kdf: EnteKdf,
    #[serde(rename = "encryptedData")]
    data: String,
    #[serde(rename = "encryptionNonce")]
    nonce: String,
}

#[derive(Deserialize)]
struct EnteKdf {
    #[serde(rename = "memLimit")]
    mem: u64,
    #[serde(rename = "opsLimit")]
    ops: u32,
    salt: String,
}

/// libsodium's `crypto_secretstream_xchacha20poly1305`, one message
/// pulled (Ente's `decryptData`): the header is the 24-byte nonce, the
/// message is a tag byte, the text and a 16-byte Poly1305 MAC. The MAC is
/// checked before a byte is decrypted.
pub fn secretstream_open(
    key: &[u8; 32],
    header: &[u8],
    sealed: &[u8],
) -> Result<Zeroizing<Vec<u8>>> {
    use chacha20::cipher::{KeyIvInit, StreamCipher, StreamCipherSeek};
    use poly1305::universal_hash::KeyInit;
    if header.len() != 24 || sealed.len() < 17 {
        return Err(CxfError::WrongPassword);
    }
    let first: [u8; 16] = header[..16]
        .try_into()
        .map_err(|_| CxfError::WrongPassword)?;
    let mut subkey = Zeroizing::new([0u8; 32]);
    subkey.copy_from_slice(&chacha20::hchacha::<chacha20::R20>(
        key.into(),
        &first.into(),
    ));
    let mut nonce = [0u8; 12];
    nonce[0] = 1; // the stream's counter starts at 1, little-endian
    nonce[4..].copy_from_slice(&header[16..24]);
    let keystream = |ic: u64, data: &mut [u8]| {
        let k: &[u8; 32] = &subkey;
        let mut c = chacha20::ChaCha20::new(k.into(), &nonce.into());
        c.seek(64 * ic);
        c.apply_keystream(data);
    };
    let mut poly_key = Zeroizing::new([0u8; 64]);
    keystream(0, poly_key.as_mut());
    let mlen = sealed.len() - 17;
    let (body, mac) = sealed.split_at(1 + mlen);
    let mut block = [0u8; 64];
    block[0] = body[0];
    keystream(1, &mut block);
    let tag = block[0];
    block[0] = body[0];
    // What Poly1305 covers: (no associated data) the 64-byte block, the
    // text, padding as libsodium pads it, then the two lengths.
    let mut mac_input = Vec::with_capacity(64 + mlen + 32);
    mac_input.extend_from_slice(&block);
    mac_input.extend_from_slice(&body[1..]);
    let pad = (0x10usize.wrapping_sub(64).wrapping_add(mlen)) & 0xf;
    mac_input.extend(std::iter::repeat_n(0u8, pad));
    mac_input.extend_from_slice(&0u64.to_le_bytes());
    mac_input.extend_from_slice(&((64 + mlen) as u64).to_le_bytes());
    let poly =
        poly1305::Poly1305::new_from_slice(&poly_key[..32]).map_err(|_| CxfError::WrongPassword)?;
    let computed = poly.compute_unpadded(&mac_input);
    let same = computed
        .iter()
        .zip(mac)
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0;
    if !same {
        return Err(CxfError::WrongPassword);
    }
    // TAG_FINAL (3) or TAG_MESSAGE (0) are what a one-message stream holds.
    if tag != 3 && tag != 0 {
        return Err(CxfError::Tampered(
            "the stream's tag is not a final message".into(),
        ));
    }
    let mut plain = Zeroizing::new(body[1..].to_vec());
    keystream(2, &mut plain);
    Ok(plain)
}

/// Reads an Ente Auth export: the plain text of `otpauth://` links, or the
/// encrypted JSON (Argon2id over the password, then libsodium's
/// secretstream).
pub fn read_ente(bytes: &[u8], password: Option<&[u8]>) -> Result<Parsed> {
    const F: &str = "Ente Auth";
    let trimmed = bytes
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .map_or(&bytes[..0], |i| &bytes[i..]);
    if !trimmed.starts_with(b"{") {
        return read_otpauth_lines(bytes, F);
    }
    let file: EnteFile = serde_json::from_slice(bytes).map_err(json_err(F))?;
    if file.version > 1 {
        return Err(parse_err(
            F,
            format!(
                "it is export version {}, newer than Lockdown reads",
                file.version
            ),
        ));
    }
    let password = password.ok_or(CxfError::PasswordNeeded)?;
    // libsodium's limits: memory in bytes, at least 8 KiB; capped here at
    // 1 GiB and 20 passes before anything runs.
    if file.kdf.mem < 8192 || file.kdf.mem > 1 << 30 || file.kdf.ops == 0 || file.kdf.ops > 20 {
        return Err(parse_err(
            F,
            "its key-derivation settings are outside what Lockdown runs",
        ));
    }
    let b64 = |t: &str| {
        data_encoding::BASE64
            .decode(t.as_bytes())
            .map_err(|_| parse_err(F, "a part of the file isn't base64"))
    };
    let salt = b64(&file.kdf.salt)?;
    let params = argon2::Params::new((file.kdf.mem / 1024) as u32, file.kdf.ops, 1, Some(32))
        .map_err(|_| parse_err(F, "its key-derivation settings are invalid"))?;
    let argon = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let mut key = Zeroizing::new([0u8; 32]);
    argon
        .hash_password_into(password, &salt, key.as_mut())
        .map_err(|_| parse_err(F, "its key derivation failed"))?;
    let plain = secretstream_open(&key, &b64(&file.nonce)?, &b64(&file.data)?)?;
    read_otpauth_lines(&plain, F)
}

// --- Proton Authenticator ---------------------------------------------------------

#[derive(Deserialize)]
struct ProtonAuthFile {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    entries: Option<Vec<ProtonAuthEntry>>,
    #[serde(default)]
    salt: Option<String>,
    #[serde(default)]
    content: Option<Value>,
}

#[derive(Deserialize)]
struct ProtonAuthEntry {
    #[serde(default)]
    id: String,
    content: ProtonAuthContent,
    #[serde(default)]
    note: Option<String>,
}

#[derive(Deserialize)]
struct ProtonAuthContent {
    uri: String,
    #[serde(default)]
    name: Option<String>,
}

/// Reads a Proton Authenticator export (JSON, unencrypted).
pub fn read_proton_authenticator(bytes: &[u8]) -> Result<Parsed> {
    const F: &str = "Proton Authenticator";
    let file: ProtonAuthFile = serde_json::from_slice(bytes).map_err(json_err(F))?;
    let Some(entries) = file.entries else {
        if file.salt.is_some() || file.content.is_some() {
            return Err(CxfError::Unsupported(
                "this Proton Authenticator export is locked with a password; export it again without one and import that".into(),
            ));
        }
        return Err(parse_err(F, "it has no entries"));
    };
    if file.version > 1 {
        return Err(parse_err(
            F,
            format!(
                "it is export version {}, newer than Lockdown reads",
                file.version
            ),
        ));
    }
    if entries.len() > MAX_CODES {
        return Err(CxfError::TooMany {
            what: "codes",
            found: entries.len(),
            limit: MAX_CODES,
        });
    }
    let mut parsed = Parsed::default();
    for (i, e) in entries.iter().enumerate() {
        match lockdown_codes::parse_uri(&e.content.uri) {
            Ok(code) => parsed.items.push(code_item(
                "proton-authenticator",
                if e.id.is_empty() { "" } else { &e.id },
                &code,
                e.content.name.as_deref(),
                e.note.as_deref().filter(|n| !n.is_empty()),
                None,
                false,
            )),
            Err(err) => parsed.problems.push(format!(
                "{} wasn't brought in: {err}",
                e.content
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("entry {}", i + 1))
            )),
        }
    }
    Ok(parsed)
}

// --- Bitwarden Authenticator --------------------------------------------------------

#[derive(Deserialize)]
struct BwAuthFile {
    #[serde(default)]
    encrypted: bool,
    #[serde(default)]
    items: Vec<BwAuthItem>,
}

#[derive(Deserialize)]
struct BwAuthItem {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    favorite: bool,
    #[serde(default)]
    login: Option<BwAuthLogin>,
}

#[derive(Deserialize)]
struct BwAuthLogin {
    #[serde(default)]
    totp: Option<String>,
    #[serde(default)]
    username: Option<String>,
}

/// Reads a Bitwarden Authenticator export: its JSON (`items` with
/// `login.totp`) or its CSV (`name`, `totp` or `login_totp` columns).
pub fn read_bitwarden_authenticator(bytes: &[u8], limits: &Limits) -> Result<Parsed> {
    const F: &str = "Bitwarden Authenticator";
    let mut parsed = Parsed::default();
    let add = |id: &str, name: &str, totp: &str, favourite: bool, parsed: &mut Parsed| {
        let uri = if totp.starts_with("otpauth://") || totp.starts_with("steam://") {
            totp.to_string()
        } else {
            format!(
                "otpauth://totp/{}?secret={}",
                name.chars()
                    .filter(|c| c.is_ascii_alphanumeric())
                    .collect::<String>(),
                totp.chars()
                    .filter(|c| !c.is_whitespace())
                    .collect::<String>()
            )
        };
        match lockdown_codes::parse_uri(&uri) {
            Ok(code) => parsed.items.push(code_item(
                "bitwarden-authenticator",
                id,
                &code,
                Some(name),
                None,
                None,
                favourite,
            )),
            Err(e) => parsed.problems.push(format!(
                "{} wasn't brought in: {e}",
                if name.is_empty() { "a code" } else { name }
            )),
        }
    };
    if super::looks_like_json(bytes) {
        let file: BwAuthFile = serde_json::from_slice(bytes).map_err(json_err(F))?;
        if file.encrypted {
            return Err(CxfError::Unsupported(
                "this Bitwarden Authenticator export is encrypted; export it again as unencrypted JSON".into(),
            ));
        }
        for (i, item) in file.items.iter().take(MAX_CODES).enumerate() {
            let Some(totp) = item.login.as_ref().and_then(|l| l.totp.as_deref()) else {
                continue;
            };
            let name = if item.name.is_empty() {
                item.login
                    .as_ref()
                    .and_then(|l| l.username.clone())
                    .unwrap_or_default()
            } else {
                item.name.clone()
            };
            let id = if item.id.is_empty() {
                i.to_string()
            } else {
                item.id.clone()
            };
            add(&id, &name, totp, item.favorite, &mut parsed);
        }
    } else {
        let table = crate::csv::parse(bytes, limits)?;
        let col = |names: &[&str]| {
            table
                .headers
                .iter()
                .position(|h| names.contains(&h.as_str()))
        };
        let (Some(totp), name) = (
            col(&["login_totp", "totp", "secret"]),
            col(&["name", "issuer"]),
        ) else {
            return Err(parse_err(F, "the CSV has no totp column"));
        };
        for (i, row) in table.rows.iter().take(MAX_CODES).enumerate() {
            let value = row.get(totp).map(String::as_str).unwrap_or("").trim();
            if value.is_empty() {
                continue;
            }
            let n = name
                .and_then(|c| row.get(c))
                .map(String::as_str)
                .unwrap_or("");
            add(&format!("csv-{}", i + 1), n, value, false, &mut parsed);
        }
    }
    if parsed.items.is_empty() && parsed.problems.is_empty() {
        return Err(parse_err(F, "there are no codes in it"));
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &[u8] = b"12345678901234567890";

    #[test]
    fn a_google_transfer_reads_back_every_code_and_says_when_a_batch_is_missing() {
        let one = google_batch(
            &[
                (KEY, "Example:sean@example.com", "Example", 1, 1, 2, 0),
                (KEY, "counter@example.com", "Counter", 2, 2, 1, 7),
            ],
            0,
            2,
            42,
        );
        let parsed = read_google(one.as_bytes()).unwrap();
        assert_eq!(parsed.items.len(), 2);
        assert_eq!(parsed.items[0].title, "Example");
        assert!(
            parsed
                .problems
                .iter()
                .any(|p| p.contains("2 QR codes and 1 were read"))
        );
        let uri = |i: usize| match &parsed.items[i]
            .fields
            .iter()
            .find(|f| f.field_id == "code.0")
            .unwrap()
            .value
        {
            lockdown_vault::FieldValue::Text(t) => t.clone(),
            _ => panic!(),
        };
        let first = lockdown_codes::parse_uri(&uri(0)).unwrap();
        assert_eq!(first.secret.expose(), KEY);
        assert_eq!(first.account.as_deref(), Some("sean@example.com"));
        // RFC 6238 test vector: 59 s → 94287082 at 8 digits, 287082 at six.
        assert_eq!(first.generate_at(59).unwrap(), "287082");
        let second = lockdown_codes::parse_uri(&uri(1)).unwrap();
        assert_eq!(second.variant, Variant::Hotp);
        assert_eq!((second.digits, second.counter), (8, 7));
        assert_eq!(second.algorithm, Algorithm::Sha256);
        // Both halves of the batch: nothing missing.
        let two = google_batch(&[(KEY, "c", "C", 1, 1, 2, 0)], 1, 2, 42);
        let parsed = read_google(format!("{one}\n{two}\n").as_bytes()).unwrap();
        assert_eq!(parsed.items.len(), 3);
        assert!(parsed.problems.is_empty(), "{:?}", parsed.problems);
    }

    #[test]
    fn a_google_code_the_app_cant_make_is_reported_not_kept() {
        let batch = google_batch(
            &[
                (KEY, "md5", "Old", 4, 1, 2, 0),
                (b"", "empty", "E", 1, 1, 2, 0),
            ],
            0,
            1,
            1,
        );
        let parsed = read_google(batch.as_bytes()).unwrap();
        assert!(parsed.items.is_empty());
        assert_eq!(parsed.problems.len(), 2);
        assert!(parsed.problems[0].contains("MD5"));
        for bad in [
            "",
            "otpauth-migration://offline?data=%%%",
            "otpauth-migration://offline?nodata",
        ] {
            assert!(read_google(bad.as_bytes()).is_err(), "{bad}");
        }
        // A length that runs past the end is an error, never a panic.
        assert!(protobuf_fields(&[0x0a, 0xff, 0x01, 0x00]).is_err());
        assert!(
            protobuf_fields(&[
                0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01
            ])
            .is_err()
        );
    }

    #[test]
    fn otpauth_lines_read_and_skip_what_isnt_a_link() {
        let text = "# my codes\notpauth://totp/A:me?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&issuer=A\n\
                    otpauth://totp/B?secret=not-base32!\nsteam://auth?secret=MTIzNDU2Nzg5MDEyMzQ1Njc4OTA%3D&account=gaben\n";
        let parsed = read_otpauth_lines(text.as_bytes(), "WinAuth").unwrap();
        assert_eq!(parsed.items.len(), 2);
        assert_eq!(parsed.problems.len(), 2);
        assert_eq!(parsed.items[1].title, "Steam");
    }
}
