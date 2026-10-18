//! Steam Guard files (docs/plan/23-bridges.md §2): Steam Desktop
//! Authenticator's maFiles (one file, or the `maFiles` folder as a ZIP
//! with its `manifest.json`, including the passkey-encrypted kind),
//! WinAuth's text export, and the Steam app's own JSON (the "v2" files
//! with `shared_secret`, `identity_secret` and the rest, as one object, a
//! list, or keyed by Steam ID).
//!
//! Each account becomes two items:
//!
//! - a **Code** item with the Steam code (`shared_secret`), an everyday
//!   item like any other code;
//! - a **crown jewel** holding the `identity_secret` and the revocation
//!   code (02-model §3: both are crown-jewel by default). The identity
//!   secret is what signs trade and market confirmations; Lockdown keeps
//!   it so the authenticator can move again later, and **never uses it**:
//!   there is no code path that confirms a trade (a standing rule, D8).
//!
//! SDA's encryption is AES-256-CBC with no authentication (PBKDF2-SHA-1,
//! 50,000 rounds, the salt and IV per file in the manifest); a wrong
//! passkey usually shows as bad padding or text that isn't JSON, and is
//! reported as a wrong password. Once in Lockdown the secrets are sealed
//! like every other field. Session tokens in a maFile (`Session`) are live
//! sign-ins and are left out on purpose.

use lockdown_vault::{FieldKind, ItemKind};
use serde::Deserialize;
use serde_json::Value;
use zeroize::Zeroizing;

use super::{Parsed, json_err};
use crate::build::Builder;
use crate::model::b64url;
use crate::{CxfError, Limits, Result, unzip};

/// The most accounts one file may hold.
const MAX_ACCOUNTS: usize = 1_000;
/// SDA's PBKDF2 rounds.
const SDA_ROUNDS: u32 = 50_000;

fn parse_err(why: impl Into<String>) -> CxfError {
    CxfError::Parse {
        format: "Steam",
        why: why.into(),
    }
}

/// One Steam account's secrets, as any of the files hold them.
#[derive(Deserialize, Default)]
struct SteamAccount {
    #[serde(default)]
    shared_secret: Option<String>,
    #[serde(default)]
    identity_secret: Option<String>,
    #[serde(default)]
    revocation_code: Option<String>,
    #[serde(default)]
    account_name: Option<String>,
    #[serde(default, deserialize_with = "id_text")]
    steamid: Option<String>,
    #[serde(default)]
    device_id: Option<String>,
    #[serde(default)]
    serial_number: Option<Value>,
    #[serde(default, rename = "Session")]
    session: Option<Value>,
}

/// A Steam ID as text, whether the file wrote a number or a string.
fn id_text<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<Option<String>, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::String(s) => Some(s),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    })
}

#[derive(Deserialize)]
struct Manifest {
    #[serde(default)]
    encrypted: bool,
    #[serde(default)]
    entries: Vec<ManifestEntry>,
}

#[derive(Deserialize)]
struct ManifestEntry {
    #[serde(default)]
    encryption_iv: Option<String>,
    #[serde(default)]
    encryption_salt: Option<String>,
    filename: String,
}

/// Decrypts one SDA maFile body: base64 of AES-256-CBC under
/// PBKDF2-HMAC-SHA1(passkey, salt, 50,000), PKCS#7 padding.
pub fn sda_decrypt(
    passkey: &[u8],
    salt_b64: &str,
    iv_b64: &str,
    body_b64: &str,
) -> Result<Zeroizing<Vec<u8>>> {
    use aes::cipher::{BlockModeDecrypt, KeyIvInit, block_padding::Pkcs7};
    let b64 = |t: &str| {
        data_encoding::BASE64
            .decode(t.trim().as_bytes())
            .map_err(|_| parse_err("an encrypted maFile part isn't base64"))
    };
    let (salt, iv, body) = (b64(salt_b64)?, b64(iv_b64)?, b64(body_b64)?);
    if iv.len() != 16 || body.is_empty() || body.len() % 16 != 0 {
        return Err(CxfError::WrongPassword);
    }
    let mut key = Zeroizing::new([0u8; 32]);
    pbkdf2::pbkdf2_hmac::<sha1::Sha1>(passkey, &salt, SDA_ROUNDS, key.as_mut());
    let dec = cbc::Decryptor::<aes::Aes256>::new_from_slices(key.as_ref(), &iv)
        .map_err(|_| CxfError::WrongPassword)?;
    let plain = dec
        .decrypt_padded_vec::<Pkcs7>(&body)
        .map_err(|_| CxfError::WrongPassword)?;
    Ok(Zeroizing::new(plain))
}

/// The two items for one account, or what was wrong with it.
fn items_for(acc: &SteamAccount, n: usize, parsed: &mut Parsed) {
    let name = acc
        .account_name
        .clone()
        .filter(|a| !a.trim().is_empty())
        .unwrap_or_else(|| format!("account {n}"));
    let id = acc.steamid.clone().unwrap_or_else(|| name.clone());
    let Some(shared) = acc.shared_secret.as_deref().filter(|s| !s.is_empty()) else {
        parsed.problems.push(format!(
            "Steam {name} has no shared secret, so no code was made"
        ));
        return;
    };
    let uri = format!(
        "steam://auth?secret={}&account={}",
        shared
            .replace('+', "%2B")
            .replace('/', "%2F")
            .replace('=', "%3D"),
        name.chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
            .collect::<String>()
    );
    let mut code = Builder::new(
        b64url(format!("steam:{id}").as_bytes()),
        ItemKind::Code,
        &format!("Steam ({name})"),
    );
    code.totp(&uri);
    let code = code.finish();
    if code.fields.iter().all(|f| f.field_id != "code.0") {
        parsed.problems.push(format!(
            "Steam {name}'s shared secret didn't read, so no code was made"
        ));
        return;
    }
    parsed.items.push(code);

    let identity = acc.identity_secret.as_deref().filter(|s| !s.is_empty());
    let revocation = acc.revocation_code.as_deref().filter(|s| !s.is_empty());
    if identity.is_some() || revocation.is_some() {
        let mut b = Builder::new(
            b64url(format!("steam-recovery:{id}").as_bytes()),
            ItemKind::Note,
            &format!("Steam Guard recovery ({name})"),
        );
        b.field(
            "body",
            "The Steam Guard secrets that came with this account's authenticator. Lockdown keeps them so the authenticator can be moved later. It never uses the identity secret: Lockdown does not confirm trades or market listings.",
        );
        if let Some(r) = revocation {
            b.custom("Revocation code", FieldKind::Hidden, r);
        }
        if let Some(i) = identity {
            b.custom("Identity secret", FieldKind::Hidden, i);
        }
        if let Some(sid) = &acc.steamid {
            b.custom("Steam ID", FieldKind::Text, sid);
        }
        if let Some(d) = acc.device_id.as_deref().filter(|d| !d.is_empty()) {
            b.custom("Device ID", FieldKind::Text, d);
        }
        if let Some(serial) = &acc.serial_number {
            let s = match serial {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            b.custom("Serial number", FieldKind::Text, &s);
        }
        let mut item = b.finish();
        item.crown_jewel = true;
        parsed.items.push(item);
    }
    if acc.session.as_ref().is_some_and(|s| !s.is_null()) {
        parsed.problems.push(format!(
            "Steam {name}'s saved sign-in session was left out: it is a live sign-in, not a secret to keep"
        ));
    }
}

fn accounts_in(value: Value) -> Result<Vec<SteamAccount>> {
    let take =
        |v: Value| -> Result<SteamAccount> { serde_json::from_value(v).map_err(json_err("Steam")) };
    let list: Vec<Value> = match value {
        Value::Array(items) => items,
        Value::Object(map)
            if map.contains_key("shared_secret") || map.contains_key("identity_secret") =>
        {
            vec![Value::Object(map)]
        }
        Value::Object(mut map) => match map.remove("accounts") {
            Some(Value::Array(items)) => items,
            Some(Value::Object(by_id)) => by_id.into_values().collect(),
            Some(_) => return Err(parse_err("'accounts' isn't a list")),
            // Keyed by Steam ID: every value that looks like an account.
            None => map
                .into_values()
                .filter(|v| v.get("shared_secret").is_some())
                .collect(),
        },
        _ => return Err(parse_err("it isn't a Steam Guard file")),
    };
    if list.len() > MAX_ACCOUNTS {
        return Err(CxfError::TooMany {
            what: "Steam accounts",
            found: list.len(),
            limit: MAX_ACCOUNTS,
        });
    }
    list.into_iter().map(take).collect()
}

/// Reads one maFile, the Steam app's JSON, or a ZIP of SDA's `maFiles`
/// folder (`password` is the SDA passkey when the manifest says the files
/// are encrypted).
pub fn read(bytes: &[u8], password: Option<&[u8]>, limits: &Limits) -> Result<Parsed> {
    let mut parsed = Parsed::default();
    if unzip::is_zip(bytes) {
        let entries = unzip::read(bytes, limits)?;
        let manifest = entries
            .iter()
            .find(|e| e.name.rsplit('/').next() == Some("manifest.json"))
            .map(|e| serde_json::from_slice::<Manifest>(&e.bytes))
            .transpose()
            .map_err(json_err("Steam"))?;
        let mut n = 0;
        for e in entries.iter().filter(|e| e.name.ends_with(".maFile")) {
            let file = e.name.rsplit('/').next().unwrap_or(&e.name);
            let entry = manifest
                .as_ref()
                .and_then(|m| m.entries.iter().find(|x| x.filename == file));
            let body: Zeroizing<Vec<u8>> =
                match (manifest.as_ref().is_some_and(|m| m.encrypted), entry) {
                    (
                        true,
                        Some(ManifestEntry {
                            encryption_iv: Some(iv),
                            encryption_salt: Some(salt),
                            ..
                        }),
                    ) => {
                        let pass = password.ok_or(CxfError::PasswordNeeded)?;
                        let text = std::str::from_utf8(&e.bytes)
                            .map_err(|_| parse_err("a maFile isn't text"))?;
                        let plain = sda_decrypt(pass, salt, iv, text)?;
                        if serde_json::from_slice::<Value>(&plain).is_err() {
                            return Err(CxfError::WrongPassword);
                        }
                        plain
                    }
                    (true, _) => {
                        parsed.problems.push(format!(
                        "{file} is encrypted but the manifest doesn't list it, so it was skipped"
                    ));
                        continue;
                    }
                    (false, _) => Zeroizing::new(e.bytes.to_vec()),
                };
            let value: Value = serde_json::from_slice(&body).map_err(json_err("Steam"))?;
            for acc in accounts_in(value)? {
                n += 1;
                items_for(&acc, n, &mut parsed);
            }
        }
        if n == 0 {
            return Err(parse_err("the ZIP holds no .maFile"));
        }
        return Ok(parsed);
    }
    let first = bytes
        .iter()
        .find(|b| !b.is_ascii_whitespace() && !matches!(b, 0xef | 0xbb | 0xbf));
    if !matches!(first, Some(b'{' | b'[' | b'"')) {
        // WinAuth's text export: one otpauth:// link per line.
        return super::otp::read_otpauth_lines(bytes, "WinAuth");
    }
    let value: Value = serde_json::from_slice(bytes).map_err(json_err("Steam"))?;
    if value.is_string() {
        return Err(CxfError::Unsupported(
            "this maFile is encrypted; import the whole maFiles folder as a ZIP so Lockdown can read its manifest".into(),
        ));
    }
    for (n, acc) in accounts_in(value)?.into_iter().enumerate() {
        items_for(&acc, n + 1, &mut parsed);
    }
    if parsed.items.is_empty() && parsed.problems.is_empty() {
        return Err(parse_err("there's no Steam account in it"));
    }
    Ok(parsed)
}

/// Whether a JSON file looks like a Steam Guard file.
pub fn is_steam_json(keys: &std::collections::BTreeMap<String, serde::de::IgnoredAny>) -> bool {
    keys.contains_key("shared_secret")
        || (keys.contains_key("identity_secret") && keys.contains_key("account_name"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHARED: &str = "MTIzNDU2Nzg5MDEyMzQ1Njc4OTA="; // "12345678901234567890"

    fn mafile() -> String {
        format!(
            r#"{{"shared_secret":"{SHARED}","serial_number":"1234","revocation_code":"R12345","uri":"otpauth://totp/Steam:gaben?secret=X&issuer=Steam","server_time":1700000000,"account_name":"gaben","token_gid":"abc","identity_secret":"aWRlbnRpdHktc2VjcmV0LTEyMzQ=","secret_1":"c2VjcmV0","status":1,"device_id":"android:1111-2222","fully_enrolled":true,"Session":{{"SteamID":76561197960287930,"AccessToken":"live-token","RefreshToken":"refresh"}}}}"#
        )
    }

    #[test]
    fn a_mafile_becomes_a_code_and_a_crown_jewel_and_the_session_stays_out() {
        let parsed = read(mafile().as_bytes(), None, &Limits::default()).unwrap();
        assert_eq!(parsed.items.len(), 2);
        let code = &parsed.items[0];
        assert_eq!(code.kind, ItemKind::Code);
        assert!(!code.crown_jewel);
        let recovery = &parsed.items[1];
        assert!(recovery.crown_jewel);
        assert_eq!(recovery.title, "Steam Guard recovery (gaben)");
        let labels: Vec<&str> = recovery.fields.iter().map(|f| f.label.as_str()).collect();
        assert!(labels.contains(&"Identity secret") && labels.contains(&"Revocation code"));
        assert!(
            parsed
                .problems
                .iter()
                .any(|p| p.contains("session was left out"))
        );
        let all = format!("{:?}", parsed.items);
        assert!(!all.contains("live-token") && !all.contains("refresh"));
        // The code is the Steam variant with the shared secret's bytes.
        let lockdown_vault::FieldValue::Text(uri) = &code
            .fields
            .iter()
            .find(|f| f.field_id == "code.0")
            .unwrap()
            .value
        else {
            panic!()
        };
        let c = lockdown_codes::parse_uri(uri).unwrap();
        assert_eq!(c.variant, lockdown_codes::Variant::Steam);
        assert_eq!(c.secret.expose(), b"12345678901234567890");
    }

    #[test]
    fn the_steam_apps_json_reads_as_one_a_list_or_keyed_by_id() {
        let one = r#"{"shared_secret":"MTIzNDU2Nzg5MDEyMzQ1Njc4OTA=","account_name":"a","steamid":"7656"}"#;
        for text in [
            one.to_string(),
            format!("[{one},{one}]"),
            format!(r#"{{"accounts":{{"7656":{one}}}}}"#),
            format!(r#"{{"7656":{one}}}"#),
        ] {
            let parsed = read(text.as_bytes(), None, &Limits::default()).unwrap();
            assert!(!parsed.items.is_empty(), "{text}");
            assert!(
                parsed.items.iter().all(|i| !i.crown_jewel),
                "no identity secret, no crown jewel"
            );
        }
        let no_secret = r#"{"identity_secret":"eA==","account_name":"b"}"#;
        let parsed = read(no_secret.as_bytes(), None, &Limits::default()).unwrap();
        assert!(parsed.items.is_empty());
        assert!(parsed.problems[0].contains("no shared secret"));
        assert!(matches!(
            read(br#""c29tZSBjaXBoZXJ0ZXh0""#, None, &Limits::default()),
            Err(CxfError::Unsupported(_))
        ));
    }
}
