//! BLAKE3: the hash in version hashes, manifest chains and blob names
//! (02-model §8, §12). Fast, 256-bit, and one function for plain, keyed and
//! derive-key use.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A 32-byte BLAKE3 output. Displays as lowercase hex.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Hash32(#[serde(with = "serde_bytes")] pub [u8; 32]);

impl Hash32 {
    /// The all-zero hash, used as `prev_hash` of the first manifest head.
    pub const ZERO: Hash32 = Hash32([0u8; 32]);

    /// The raw bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Parses 64 hex characters (either case). Anything else is `None`.
    pub fn from_hex(text: &str) -> Option<Self> {
        let text = text.trim();
        if text.len() != 64 || !text.is_ascii() {
            return None;
        }
        let mut out = [0u8; 32];
        for (i, pair) in text.as_bytes().chunks(2).enumerate() {
            let s = std::str::from_utf8(pair).ok()?;
            out[i] = u8::from_str_radix(s, 16).ok()?;
        }
        Some(Hash32(out))
    }

    /// Lowercase hex, 64 characters.
    pub fn to_hex(self) -> String {
        let mut s = String::with_capacity(64);
        for b in self.0 {
            s.push_str(&format!("{b:02x}"));
        }
        s
    }
}

impl fmt::Debug for Hash32 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Hash32({})", &self.to_hex()[..16])
    }
}

impl std::str::FromStr for Hash32 {
    type Err = crate::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_hex(s).ok_or_else(|| crate::Error::Invalid("not a 64-character hex hash".into()))
    }
}

impl fmt::Display for Hash32 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

/// BLAKE3 of `bytes`.
pub fn hash(bytes: &[u8]) -> Hash32 {
    Hash32(*blake3::hash(bytes).as_bytes())
}

/// Keyed BLAKE3 (a MAC) of `bytes` under a 32-byte key.
pub fn keyed_hash(key: &[u8; 32], bytes: &[u8]) -> Hash32 {
    Hash32(*blake3::keyed_hash(key, bytes).as_bytes())
}

/// BLAKE3 in derive-key mode: a 32-byte key for `context` from `material`.
/// `context` is a fixed, globally unique string such as
/// `"lockdown 2026-10-16 blob name"`.
pub fn derive_key(context: &str, material: &[u8]) -> [u8; 32] {
    blake3::derive_key(context, material)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_answer_and_modes_differ() {
        // BLAKE3 of the empty input, from the reference test vectors.
        assert_eq!(
            hash(b"").to_hex(),
            "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
        );
        let k = [7u8; 32];
        assert_ne!(hash(b"x"), keyed_hash(&k, b"x"));
        assert_ne!(derive_key("a", b"x"), derive_key("b", b"x"));
        assert_eq!(Hash32::ZERO.to_hex(), "0".repeat(64));
        let bytes = crate::cbor::to_canonical(&hash(b"x")).unwrap();
        let back: Hash32 = crate::cbor::from_slice(&bytes).unwrap();
        assert_eq!(back, hash(b"x"));
        let h = hash(b"hex");
        assert_eq!(Hash32::from_hex(&h.to_hex()), Some(h));
        assert_eq!(h.to_hex().to_uppercase().parse::<Hash32>().unwrap(), h);
        assert!(Hash32::from_hex("abc").is_none());
        assert!(Hash32::from_hex(&"zz".repeat(32)).is_none());
    }
}
