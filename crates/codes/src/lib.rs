//! `lockdown-codes`: one-time codes (03-architecture §2 `codes` row;
//! 02-model §4 "Code"; brief §2.3 on Steam).
//!
//! - **HOTP** (RFC 4226): HMAC over a counter, dynamic truncation, 6–8 digits.
//! - **TOTP** (RFC 6238): HOTP with the counter `floor(now / period)`,
//!   SHA-1, SHA-256 or SHA-512, 6 or 8 digits, any period.
//! - **Steam**: the TOTP clock (30 s) and HMAC-SHA-1, but five characters
//!   from the alphabet `23456789BCDFGHJKMNPQRTVWXY`, each taken as the
//!   running value modulo 26.
//!
//! Secrets are held in zeroizing buffers and never printed. URI parsing is
//! in [`uri`]; it accepts `otpauth://` and `steam://` and converts the
//! base64 `shared_secret` of a Steam file to base32 (brief §2.3: the one
//! engineering trap). Clock skew is handled by [`Code::verify`], which
//! accepts a code from a window of neighbouring steps and reports which.

pub mod skew;
pub mod uri;

use std::fmt;
use std::time::Duration;

use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

pub use uri::parse_uri;

/// The Steam code alphabet (26 characters; no 0, 1, A, E, I, L, O, S, U, Z).
pub const STEAM_ALPHABET: &[u8; 26] = b"23456789BCDFGHJKMNPQRTVWXY";
/// Steam codes are five characters.
pub const STEAM_DIGITS: u32 = 5;

/// What can go wrong with a code.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CodeError {
    /// The URI or secret did not parse.
    #[error("the code's set-up didn't read correctly: {0}")]
    Invalid(String),
    /// Digits outside 6–8 (or 5 for Steam) are not standard.
    #[error("{0} digits isn't a standard code length (6, 7 or 8; Steam uses 5)")]
    BadDigits(u32),
    /// A zero period makes no clock.
    #[error("a code's period must be at least one second")]
    BadPeriod,
    /// The feature exists in the plan but not in this build yet.
    #[error("not supported here: {0}")]
    Unsupported(String),
}

/// The crate's result type.
pub type Result<T> = std::result::Result<T, CodeError>;

/// The HMAC hash.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Algorithm {
    /// HMAC-SHA-1, the common default and the only one Steam uses.
    #[default]
    Sha1,
    /// HMAC-SHA-256.
    Sha256,
    /// HMAC-SHA-512.
    Sha512,
}

impl Algorithm {
    /// Parses `SHA1`, `SHA256`, `SHA512` (case-insensitive, dash optional).
    pub fn parse(text: &str) -> Result<Self> {
        match text.to_ascii_uppercase().replace('-', "").as_str() {
            "SHA1" => Ok(Self::Sha1),
            "SHA256" => Ok(Self::Sha256),
            "SHA512" => Ok(Self::Sha512),
            other => Err(CodeError::Invalid(format!("unknown algorithm '{other}'"))),
        }
    }

    /// The name as `otpauth://` writes it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sha1 => "SHA1",
            Self::Sha256 => "SHA256",
            Self::Sha512 => "SHA512",
        }
    }
}

/// Which kind of code (02-model §4 `variant`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Variant {
    /// Time-based.
    #[default]
    Totp,
    /// Counter-based.
    Hotp,
    /// Steam's five-character time-based code.
    Steam,
}

/// A shared secret for a code, wiped on drop. Debug prints only the length.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(Zeroizing<Vec<u8>>);

impl Secret {
    /// Raw key bytes.
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self(Zeroizing::new(bytes))
    }

    /// Decodes base32 (RFC 4648, padding optional, case-insensitive, spaces ignored).
    pub fn from_base32(text: &str) -> Result<Self> {
        let cleaned: String = text
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '-')
            .map(|c| c.to_ascii_uppercase())
            .filter(|c| *c != '=')
            .collect();
        data_encoding::BASE32_NOPAD
            .decode(cleaned.as_bytes())
            .map(Self::from_bytes)
            .map_err(|_| CodeError::Invalid("the secret isn't base32".into()))
    }

    /// Decodes standard base64 (what a Steam file's `shared_secret` uses).
    pub fn from_base64(text: &str) -> Result<Self> {
        let cleaned: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        let decoder = if cleaned.ends_with('=') {
            data_encoding::BASE64
        } else {
            data_encoding::BASE64_NOPAD
        };
        decoder
            .decode(cleaned.as_bytes())
            .map(Self::from_bytes)
            .map_err(|_| CodeError::Invalid("the secret isn't base64".into()))
    }

    /// The secret as unpadded base32, for `otpauth://` export.
    pub fn to_base32(&self) -> String {
        data_encoding::BASE32_NOPAD.encode(&self.0)
    }

    /// The raw bytes. Use them at once.
    pub fn expose(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Secret({} bytes)", self.0.len())
    }
}

/// A code's set-up: everything but the current time or counter.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Code {
    /// Which kind.
    pub variant: Variant,
    /// The key.
    pub secret: Secret,
    /// The hash (Steam is always SHA-1).
    pub algorithm: Algorithm,
    /// 6, 7 or 8; 5 for Steam.
    pub digits: u32,
    /// Seconds per step for TOTP and Steam.
    pub period: u32,
    /// The next counter for HOTP.
    pub counter: u64,
    /// Who issued it.
    pub issuer: Option<String>,
    /// The account name.
    pub account: Option<String>,
}

impl Code {
    /// A TOTP code with the common defaults (SHA-1, 6 digits, 30 s).
    pub fn totp(secret: Secret) -> Self {
        Self {
            variant: Variant::Totp,
            secret,
            algorithm: Algorithm::Sha1,
            digits: 6,
            period: 30,
            counter: 0,
            issuer: None,
            account: None,
        }
    }

    /// A Steam code (SHA-1, 5 characters, 30 s).
    pub fn steam(secret: Secret) -> Self {
        Self {
            variant: Variant::Steam,
            algorithm: Algorithm::Sha1,
            digits: STEAM_DIGITS,
            period: 30,
            issuer: Some("Steam".into()),
            ..Self::totp(secret)
        }
    }

    /// Checks the set-up is one this crate generates.
    pub fn validate(&self) -> Result<()> {
        match self.variant {
            Variant::Steam => {
                if self.digits != STEAM_DIGITS {
                    return Err(CodeError::BadDigits(self.digits));
                }
            }
            Variant::Totp | Variant::Hotp => {
                if !(6..=8).contains(&self.digits) {
                    return Err(CodeError::BadDigits(self.digits));
                }
            }
        }
        if self.variant != Variant::Hotp && self.period == 0 {
            return Err(CodeError::BadPeriod);
        }
        Ok(())
    }

    /// The time step for `now` (seconds since the epoch).
    pub fn step_at(&self, now: u64) -> u64 {
        now / u64::from(self.period.max(1))
    }

    /// The code for a given counter or time step.
    pub fn code_for_counter(&self, counter: u64) -> Result<String> {
        self.validate()?;
        let truncated = dynamic_truncation(self.algorithm, self.secret.expose(), counter);
        Ok(match self.variant {
            Variant::Steam => steam_from_truncated(truncated),
            Variant::Totp | Variant::Hotp => digits_from_truncated(truncated, self.digits),
        })
    }

    /// The code at `now` (TOTP, Steam) or for the stored counter (HOTP).
    pub fn generate_at(&self, now: u64) -> Result<String> {
        match self.variant {
            Variant::Hotp => self.code_for_counter(self.counter),
            Variant::Totp | Variant::Steam => self.code_for_counter(self.step_at(now)),
        }
    }

    /// How long the code shown at `now` stays valid. `None` for HOTP.
    pub fn next_code_in(&self, now: u64) -> Option<Duration> {
        if self.variant == Variant::Hotp {
            return None;
        }
        let period = u64::from(self.period.max(1));
        Some(Duration::from_secs(period - now % period))
    }

    /// Checks `candidate` against the codes for `now` and up to `window`
    /// steps either side (clock skew). Returns the offset in steps that
    /// matched, `0` for the current one. HOTP checks the stored counter and
    /// `window` counters ahead, returning the offset ahead.
    pub fn verify(&self, candidate: &str, now: u64, window: u32) -> Result<Option<i64>> {
        let candidate = candidate.trim().to_ascii_uppercase().replace(' ', "");
        let base = match self.variant {
            Variant::Hotp => self.counter,
            _ => self.step_at(now),
        };
        let (lo, hi) = match self.variant {
            Variant::Hotp => (0i64, i64::from(window)),
            _ => (-i64::from(window), i64::from(window)),
        };
        for offset in lo..=hi {
            let Some(counter) = base.checked_add_signed(offset) else {
                continue;
            };
            if constant_time_eq(
                self.code_for_counter(counter)?.as_bytes(),
                candidate.as_bytes(),
            ) {
                return Ok(Some(offset));
            }
        }
        Ok(None)
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// RFC 4226 §5.3: HMAC over the 8-byte big-endian counter, then the 31-bit
/// value at the offset the low nibble of the last byte names.
pub fn dynamic_truncation(algorithm: Algorithm, key: &[u8], counter: u64) -> u32 {
    let msg = counter.to_be_bytes();
    let mac: Vec<u8> = match algorithm {
        Algorithm::Sha1 => {
            let mut m = <Hmac<sha1::Sha1> as KeyInit>::new_from_slice(key)
                .expect("HMAC takes any key length");
            m.update(&msg);
            m.finalize().into_bytes().to_vec()
        }
        Algorithm::Sha256 => {
            let mut m = <Hmac<sha2::Sha256> as KeyInit>::new_from_slice(key)
                .expect("HMAC takes any key length");
            m.update(&msg);
            m.finalize().into_bytes().to_vec()
        }
        Algorithm::Sha512 => {
            let mut m = <Hmac<sha2::Sha512> as KeyInit>::new_from_slice(key)
                .expect("HMAC takes any key length");
            m.update(&msg);
            m.finalize().into_bytes().to_vec()
        }
    };
    let offset = usize::from(mac[mac.len() - 1] & 0x0f);
    (u32::from(mac[offset] & 0x7f) << 24)
        | (u32::from(mac[offset + 1]) << 16)
        | (u32::from(mac[offset + 2]) << 8)
        | u32::from(mac[offset + 3])
}

/// The last `digits` decimal digits of the truncated value, zero-padded.
pub fn digits_from_truncated(truncated: u32, digits: u32) -> String {
    let modulus = 10u64.pow(digits);
    let value = u64::from(truncated) % modulus;
    format!("{value:0width$}", width = digits as usize)
}

/// Five characters from [`STEAM_ALPHABET`], each the running value modulo 26.
pub fn steam_from_truncated(truncated: u32) -> String {
    let mut value = truncated;
    let mut out = String::with_capacity(STEAM_DIGITS as usize);
    for _ in 0..STEAM_DIGITS {
        out.push(STEAM_ALPHABET[(value % 26) as usize] as char);
        value /= 26;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const RFC4226_SECRET: &[u8] = b"12345678901234567890";

    #[test]
    fn hotp_rfc4226_vectors() {
        let expected = [
            (0u64, 1_284_755_224u32, "755224"),
            (1, 1_094_287_082, "287082"),
            (2, 137_359_152, "359152"),
            (3, 1_726_969_429, "969429"),
            (4, 1_640_338_314, "338314"),
            (5, 868_254_676, "254676"),
            (6, 1_918_287_922, "287922"),
            (7, 82_162_583, "162583"),
            (8, 673_399_871, "399871"),
            (9, 645_520_489, "520489"),
        ];
        let mut code = Code::totp(Secret::from_bytes(RFC4226_SECRET.to_vec()));
        code.variant = Variant::Hotp;
        for (counter, truncated, digits) in expected {
            assert_eq!(
                dynamic_truncation(Algorithm::Sha1, RFC4226_SECRET, counter),
                truncated
            );
            assert_eq!(
                code.code_for_counter(counter).unwrap(),
                digits,
                "counter {counter}"
            );
        }
        code.counter = 3;
        assert_eq!(code.generate_at(0).unwrap(), "969429");
        assert_eq!(code.next_code_in(0), None);
        // HOTP verify looks ahead, never behind.
        assert_eq!(code.verify("338314", 0, 2).unwrap(), Some(1));
        assert_eq!(code.verify("359152", 0, 2).unwrap(), None);
    }

    #[test]
    fn totp_rfc6238_vectors() {
        // Appendix B: 8 digits, 30 s; the seed repeats to the hash's block size.
        let seed_sha1 = b"12345678901234567890".to_vec();
        let seed_sha256 = b"12345678901234567890123456789012".to_vec();
        let seed_sha512 =
            b"1234567890123456789012345678901234567890123456789012345678901234".to_vec();
        let table: [(u64, &str, &str, &str); 6] = [
            (59, "94287082", "46119246", "90693936"),
            (1_111_111_109, "07081804", "68084774", "25091201"),
            (1_111_111_111, "14050471", "67062674", "99943326"),
            (1_234_567_890, "89005924", "91819424", "93441116"),
            (2_000_000_000, "69279037", "90698825", "38618901"),
            (20_000_000_000, "65353130", "77737706", "47863826"),
        ];
        for (alg, seed) in [
            (Algorithm::Sha1, seed_sha1),
            (Algorithm::Sha256, seed_sha256),
            (Algorithm::Sha512, seed_sha512),
        ] {
            let mut code = Code::totp(Secret::from_bytes(seed));
            code.algorithm = alg;
            code.digits = 8;
            for (t, s1, s256, s512) in table {
                let want = match alg {
                    Algorithm::Sha1 => s1,
                    Algorithm::Sha256 => s256,
                    Algorithm::Sha512 => s512,
                };
                assert_eq!(code.generate_at(t).unwrap(), want, "{alg:?} at {t}");
            }
        }
    }

    #[test]
    fn six_digits_window_and_skew() {
        let code = Code::totp(Secret::from_bytes(RFC4226_SECRET.to_vec()));
        assert_eq!(code.generate_at(59).unwrap(), "287082"); // step 1 = HOTP counter 1
        assert_eq!(code.next_code_in(59), Some(Duration::from_secs(1)));
        assert_eq!(code.next_code_in(60), Some(Duration::from_secs(30)));
        // At t = 60 (step 2) the previous step's code is accepted with offset -1.
        assert_eq!(code.verify("287082", 60, 1).unwrap(), Some(-1));
        assert_eq!(code.verify("359152", 60, 1).unwrap(), Some(0));
        assert_eq!(code.verify("969429", 60, 1).unwrap(), Some(1));
        assert_eq!(code.verify("969429", 60, 0).unwrap(), None);
        assert_eq!(code.verify("000000", 60, 5).unwrap(), None);
    }

    #[test]
    fn steam_vector_by_construction() {
        // RFC 4226 counter 0 truncates to 1284755224. Walking it through the
        // alphabet modulo 26 five times:
        //   1284755224 % 26 = 12 → 'G', 49413662 % 26 = 12 → 'G',
        //   1900525 % 26 = 3 → '5', 73097 % 26 = 11 → 'F', 2811 % 26 = 3 → '5'.
        assert_eq!(steam_from_truncated(1_284_755_224), "GG5F5");
        let mut value = 1_284_755_224u32;
        let mut by_hand = String::new();
        for _ in 0..5 {
            by_hand.push(STEAM_ALPHABET[(value % 26) as usize] as char);
            value /= 26;
        }
        assert_eq!(by_hand, "GG5F5");
        // The same secret as a Steam code at step 0 gives that string.
        let code = Code::steam(Secret::from_bytes(RFC4226_SECRET.to_vec()));
        assert_eq!(code.generate_at(0).unwrap(), "GG5F5");
        assert_eq!(code.digits, 5);
        assert!(
            code.generate_at(29)
                .unwrap()
                .bytes()
                .all(|b| STEAM_ALPHABET.contains(&b))
        );
        assert_eq!(code.verify("gg5f5", 0, 0).unwrap(), Some(0));
    }

    #[test]
    fn bad_setups_are_refused() {
        let mut code = Code::totp(Secret::from_bytes(RFC4226_SECRET.to_vec()));
        code.digits = 5;
        assert_eq!(code.generate_at(0), Err(CodeError::BadDigits(5)));
        code.digits = 6;
        code.period = 0;
        assert_eq!(code.generate_at(0), Err(CodeError::BadPeriod));
        let mut steam = Code::steam(Secret::from_bytes(RFC4226_SECRET.to_vec()));
        steam.digits = 6;
        assert_eq!(steam.generate_at(0), Err(CodeError::BadDigits(6)));
    }

    #[test]
    fn secrets_decode_both_encodings_and_never_print() {
        let s = Secret::from_base32("gezd gnbv gy3t qojq gezd gnbv gy3t qojq").unwrap();
        assert_eq!(s.expose(), RFC4226_SECRET);
        assert_eq!(s.to_base32(), "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ");
        let b = Secret::from_base64("MTIzNDU2Nzg5MDEyMzQ1Njc4OTA=").unwrap();
        assert_eq!(b, s);
        assert_eq!(format!("{s:?}"), "Secret(20 bytes)");
        assert!(Secret::from_base32("not base32!").is_err());
    }
}
