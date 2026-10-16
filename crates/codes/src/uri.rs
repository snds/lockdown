//! `otpauth://` and `steam://` parsing.
//!
//! `otpauth://TYPE/LABEL?secret=BASE32&issuer=…&algorithm=…&digits=…&period=…&counter=…`
//! is the Google Authenticator key URI format every app exports. `TYPE` is
//! `totp` or `hotp`; some apps write `steam` as the type, others write
//! `totp` with `issuer=Steam` and `encoder=steam`; all three are read as
//! a Steam code.
//!
//! `steam://` is Lockdown's own short form for a Steam file's
//! `shared_secret`, which is **base64**, not base32 (brief §2.3):
//! `steam://auth?secret=BASE64&account=NAME`. It is converted on read, so
//! the stored secret and any `otpauth://` export are base32 like every
//! other code.

use url::Url;

use crate::{Algorithm, Code, CodeError, Result, Secret, Variant};

fn percent_decode(text: &str) -> String {
    let mut out = Vec::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Some(hex) = text.get(i + 1..i + 3)
            && let Ok(v) = u8::from_str_radix(hex, 16)
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Splits `Issuer:account` (or `Issuer: account`) into its two parts.
fn split_label(label: &str) -> (Option<String>, Option<String>) {
    match label.split_once(':') {
        Some((issuer, account)) => (
            Some(issuer.trim().to_string()).filter(|s| !s.is_empty()),
            Some(account.trim().to_string()).filter(|s| !s.is_empty()),
        ),
        None => (
            None,
            Some(label.trim().to_string()).filter(|s| !s.is_empty()),
        ),
    }
}

/// Parses an `otpauth://` or `steam://` URI into a [`Code`].
pub fn parse_uri(text: &str) -> Result<Code> {
    let url = Url::parse(text.trim()).map_err(|_| CodeError::Invalid("not a URI".into()))?;
    match url.scheme() {
        "otpauth" => parse_otpauth(&url),
        "steam" => parse_steam(&url),
        other => Err(CodeError::Invalid(format!(
            "'{other}://' isn't a code URI; expected otpauth:// or steam://"
        ))),
    }
}

fn query(url: &Url) -> Vec<(String, String)> {
    url.query_pairs()
        .map(|(k, v)| (k.to_ascii_lowercase(), v.into_owned()))
        .collect()
}

fn get<'a>(q: &'a [(String, String)], key: &str) -> Option<&'a str> {
    q.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

fn parse_otpauth(url: &Url) -> Result<Code> {
    let kind = url
        .host_str()
        .ok_or_else(|| CodeError::Invalid("otpauth:// needs a type (totp or hotp)".into()))?
        .to_ascii_lowercase();
    let label = percent_decode(url.path().trim_start_matches('/'));
    let (label_issuer, account) = split_label(&label);
    let q = query(url);
    let secret_text =
        get(&q, "secret").ok_or_else(|| CodeError::Invalid("the URI has no secret".into()))?;
    let issuer = get(&q, "issuer")
        .map(str::to_string)
        .filter(|s| !s.is_empty())
        .or(label_issuer);
    let is_steam = kind == "steam"
        || get(&q, "encoder").is_some_and(|e| e.eq_ignore_ascii_case("steam"))
        || issuer
            .as_deref()
            .is_some_and(|i| i.eq_ignore_ascii_case("steam"));

    let secret = Secret::from_base32(secret_text)?;
    let mut code = if is_steam {
        Code::steam(secret)
    } else {
        Code::totp(secret)
    };
    code.account = account;
    if issuer.is_some() {
        code.issuer = issuer;
    }
    match kind.as_str() {
        "totp" | "steam" => {}
        "hotp" => {
            code.variant = Variant::Hotp;
            code.counter = get(&q, "counter")
                .map(|c| c.parse::<u64>())
                .transpose()
                .map_err(|_| CodeError::Invalid("the counter isn't a number".into()))?
                .unwrap_or(0);
        }
        other => return Err(CodeError::Invalid(format!("unknown code type '{other}'"))),
    }
    if !is_steam {
        if let Some(a) = get(&q, "algorithm") {
            code.algorithm = Algorithm::parse(a)?;
        }
        if let Some(d) = get(&q, "digits") {
            code.digits = d
                .parse()
                .map_err(|_| CodeError::Invalid("digits isn't a number".into()))?;
        }
    }
    if let Some(p) = get(&q, "period") {
        code.period = p
            .parse()
            .map_err(|_| CodeError::Invalid("period isn't a number".into()))?;
    }
    code.validate()?;
    Ok(code)
}

fn parse_steam(url: &Url) -> Result<Code> {
    let q = query(url);
    let secret_text = get(&q, "secret")
        .or_else(|| get(&q, "shared_secret"))
        .ok_or_else(|| CodeError::Invalid("steam:// needs a secret".into()))?;
    // Steam files hold base64; a base32 secret is accepted too, in case a
    // link was built from another app's export.
    let secret = Secret::from_base64(secret_text).or_else(|_| Secret::from_base32(secret_text))?;
    let mut code = Code::steam(secret);
    code.account = get(&q, "account")
        .or_else(|| get(&q, "account_name"))
        .map(str::to_string);
    code.validate()?;
    Ok(code)
}

impl Code {
    /// The `otpauth://` form of this code. Steam codes are written as
    /// `otpauth://steam/…` with `encoder=steam`, which Aegis, 2FAS and
    /// KeePassXC all read.
    pub fn to_otpauth(&self) -> String {
        let kind = match self.variant {
            Variant::Totp => "totp",
            Variant::Hotp => "hotp",
            Variant::Steam => "steam",
        };
        let label = match (&self.issuer, &self.account) {
            (Some(i), Some(a)) => format!("{i}:{a}"),
            (None, Some(a)) => a.clone(),
            (Some(i), None) => i.clone(),
            (None, None) => String::new(),
        };
        let mut url = Url::parse(&format!("otpauth://{kind}/")).expect("fixed scheme parses");
        url.set_path(&format!("/{label}"));
        {
            let mut q = url.query_pairs_mut();
            q.append_pair("secret", &self.secret.to_base32());
            if let Some(i) = &self.issuer {
                q.append_pair("issuer", i);
            }
            match self.variant {
                Variant::Steam => {
                    q.append_pair("encoder", "steam");
                    q.append_pair("period", &self.period.to_string());
                }
                Variant::Totp => {
                    q.append_pair("algorithm", self.algorithm.as_str());
                    q.append_pair("digits", &self.digits.to_string());
                    q.append_pair("period", &self.period.to_string());
                }
                Variant::Hotp => {
                    q.append_pair("algorithm", self.algorithm.as_str());
                    q.append_pair("digits", &self.digits.to_string());
                    q.append_pair("counter", &self.counter.to_string());
                }
            }
        }
        url.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn otpauth_totp_and_hotp() {
        let c = parse_uri(
            "otpauth://totp/Example%3Aalice%40example.com?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&issuer=Example&algorithm=SHA256&digits=8&period=60",
        )
        .unwrap();
        assert_eq!(c.variant, Variant::Totp);
        assert_eq!(c.issuer.as_deref(), Some("Example"));
        assert_eq!(c.account.as_deref(), Some("alice@example.com"));
        assert_eq!(c.algorithm, Algorithm::Sha256);
        assert_eq!((c.digits, c.period), (8, 60));
        assert_eq!(c.secret.expose(), b"12345678901234567890");

        let h = parse_uri("otpauth://hotp/acct?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&counter=3")
            .unwrap();
        assert_eq!(h.variant, Variant::Hotp);
        assert_eq!(h.counter, 3);
        assert_eq!(h.generate_at(0).unwrap(), "969429");
        assert_eq!(h.issuer, None);

        // Round trip through to_otpauth.
        let again = parse_uri(&c.to_otpauth()).unwrap();
        assert_eq!(again, c);
    }

    #[test]
    fn steam_in_three_spellings_and_base64_conversion() {
        let b32 = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
        let a = parse_uri(&format!("otpauth://steam/Steam:me?secret={b32}")).unwrap();
        let b = parse_uri(&format!(
            "otpauth://totp/Steam:me?secret={b32}&issuer=Steam"
        ))
        .unwrap();
        let c = parse_uri(&format!("otpauth://totp/me?secret={b32}&encoder=steam")).unwrap();
        // A Steam file's shared_secret is base64 of the same bytes.
        let d = parse_uri("steam://auth?secret=MTIzNDU2Nzg5MDEyMzQ1Njc4OTA%3D&account=me").unwrap();
        for code in [&a, &b, &c, &d] {
            assert_eq!(code.variant, Variant::Steam);
            assert_eq!(code.digits, 5);
            assert_eq!(code.secret.expose(), b"12345678901234567890");
            assert_eq!(code.generate_at(0).unwrap(), "GG5F5");
        }
        assert_eq!(d.account.as_deref(), Some("me"));
        // Exported, it is base32 and reads back as Steam.
        let exported = d.to_otpauth();
        assert!(exported.starts_with("otpauth://steam/"));
        assert!(exported.contains(&format!("secret={b32}")));
        assert_eq!(parse_uri(&exported).unwrap().variant, Variant::Steam);
    }

    #[test]
    fn bad_uris_are_refused_plainly() {
        assert!(
            matches!(parse_uri("https://example.com"), Err(CodeError::Invalid(m)) if m.contains("otpauth"))
        );
        assert!(parse_uri("otpauth://totp/x").is_err());
        assert!(parse_uri("otpauth://totp/x?secret=GEZDGNBV&digits=4").is_err());
        assert!(parse_uri("otpauth://magic/x?secret=GEZDGNBV").is_err());
        assert!(parse_uri("steam://auth?account=me").is_err());
        assert!(parse_uri("not a uri at all").is_err());
    }
}
