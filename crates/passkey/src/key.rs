//! A passkey's private key: made here (ES256), read from the vault as the
//! PKCS#8 document CXF carries (ES256 or Ed25519, so imported passkeys
//! sign too), and used only to sign. The private half never leaves this
//! type except as the PKCS#8 bytes the vault seals; the public half is
//! given out as a COSE key (for the attestation) and as SPKI DER (for the
//! page's `getPublicKey()`).

use ring::rand::SystemRandom;
use ring::signature::{ECDSA_P256_SHA256_ASN1_SIGNING, EcdsaKeyPair, Ed25519KeyPair, KeyPair};
use zeroize::Zeroizing;

use crate::encode::Cbor;
use crate::{Refusal, RefusalName};

/// COSE algorithm identifiers (IANA "COSE Algorithms").
pub const COSE_ES256: i64 = -7;
/// EdDSA (Ed25519 here).
pub const COSE_EDDSA: i64 = -8;

/// The algorithms this authenticator signs with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alg {
    /// ECDSA over P-256 with SHA-256; what Lockdown makes.
    Es256,
    /// Ed25519; signed with when an imported passkey holds one.
    Ed25519,
}

impl Alg {
    /// The COSE identifier.
    pub fn cose(self) -> i64 {
        match self {
            Self::Es256 => COSE_ES256,
            Self::Ed25519 => COSE_EDDSA,
        }
    }

    /// Its name in plain words.
    pub fn name(self) -> &'static str {
        match self {
            Self::Es256 => "ES256 (P-256)",
            Self::Ed25519 => "EdDSA (Ed25519)",
        }
    }
}

/// The DER prefix of a P-256 `SubjectPublicKeyInfo` (id-ecPublicKey,
/// prime256v1, then a 66-byte BIT STRING holding the uncompressed point).
const P256_SPKI_PREFIX: [u8; 26] = [
    0x30, 0x59, 0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, 0x06, 0x08, 0x2a,
    0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07, 0x03, 0x42, 0x00,
];
/// The DER prefix of an Ed25519 `SubjectPublicKeyInfo`.
const ED25519_SPKI_PREFIX: [u8; 12] = [
    0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
];

enum Pair {
    Es256(EcdsaKeyPair),
    Ed25519(Ed25519KeyPair),
}

/// A key ready to sign.
pub struct CredentialKey {
    pair: Pair,
}

impl std::fmt::Debug for CredentialKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CredentialKey({})", self.alg().name())
    }
}

fn bad_key(why: &str) -> Refusal {
    Refusal::new(
        RefusalName::NotSupported,
        format!("this passkey's key can't be used here ({why})"),
    )
}

impl CredentialKey {
    /// Makes a new ES256 key. Returns the key and its PKCS#8 document (v1,
    /// with the public key, as RFC 5915 and CXF expect), which the caller
    /// seals into the vault and wipes.
    pub fn generate() -> Result<(Self, Zeroizing<Vec<u8>>), Refusal> {
        let rng = SystemRandom::new();
        let doc =
            EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &rng).map_err(|_| {
                Refusal::new(RefusalName::NotAllowed, "the system's random source failed")
            })?;
        let pkcs8 = Zeroizing::new(doc.as_ref().to_vec());
        let key = Self::from_pkcs8(&pkcs8)?;
        Ok((key, pkcs8))
    }

    /// Reads a PKCS#8 document: P-256 (which must carry its public key, as
    /// every WebCrypto and CXF export does) or Ed25519.
    pub fn from_pkcs8(pkcs8: &[u8]) -> Result<Self, Refusal> {
        let rng = SystemRandom::new();
        if let Ok(p) = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, pkcs8, &rng) {
            return Ok(Self {
                pair: Pair::Es256(p),
            });
        }
        if let Ok(p) = Ed25519KeyPair::from_pkcs8_maybe_unchecked(pkcs8) {
            return Ok(Self {
                pair: Pair::Ed25519(p),
            });
        }
        Err(bad_key(
            "only P-256 keys that carry their public half, and Ed25519 keys, are supported",
        ))
    }

    /// Which algorithm it signs with.
    pub fn alg(&self) -> Alg {
        match self.pair {
            Pair::Es256(_) => Alg::Es256,
            Pair::Ed25519(_) => Alg::Ed25519,
        }
    }

    /// The raw public key: the 65-byte uncompressed point, or 32 bytes.
    pub fn public_raw(&self) -> &[u8] {
        match &self.pair {
            Pair::Es256(p) => p.public_key().as_ref(),
            Pair::Ed25519(p) => p.public_key().as_ref(),
        }
    }

    /// The public key as a COSE_Key, keys in CTAP2 canonical order.
    pub fn cose_public_key(&self) -> Vec<u8> {
        let raw = self.public_raw();
        let mut c = Cbor::new();
        match self.alg() {
            Alg::Es256 => {
                // {1: 2 (EC2), 3: -7, -1: 1 (P-256), -2: x, -3: y}
                c.map(5)
                    .int(1)
                    .int(2)
                    .int(3)
                    .int(COSE_ES256)
                    .int(-1)
                    .int(1)
                    .int(-2)
                    .bytes(&raw[1..33])
                    .int(-3)
                    .bytes(&raw[33..65]);
            }
            Alg::Ed25519 => {
                // {1: 1 (OKP), 3: -8, -1: 6 (Ed25519), -2: x}
                c.map(4)
                    .int(1)
                    .int(1)
                    .int(3)
                    .int(COSE_EDDSA)
                    .int(-1)
                    .int(6)
                    .int(-2)
                    .bytes(raw);
            }
        }
        c.finish()
    }

    /// The public key as `SubjectPublicKeyInfo` DER.
    pub fn spki(&self) -> Vec<u8> {
        let prefix: &[u8] = match self.alg() {
            Alg::Es256 => &P256_SPKI_PREFIX,
            Alg::Ed25519 => &ED25519_SPKI_PREFIX,
        };
        [prefix, self.public_raw()].concat()
    }

    /// Signs `message`: ASN.1 DER for ES256 (what WebAuthn asks for), the
    /// 64 raw bytes for Ed25519.
    pub fn sign(&self, message: &[u8]) -> Result<Vec<u8>, Refusal> {
        match &self.pair {
            Pair::Es256(p) => p
                .sign(&SystemRandom::new(), message)
                .map(|s| s.as_ref().to_vec())
                .map_err(|_| Refusal::new(RefusalName::NotAllowed, "signing failed")),
            Pair::Ed25519(p) => Ok(p.sign(message).as_ref().to_vec()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::{ECDSA_P256_SHA256_ASN1, ED25519, UnparsedPublicKey};

    #[test]
    fn a_new_key_is_es256_and_its_signature_verifies() {
        let (key, pkcs8) = CredentialKey::generate().unwrap();
        assert_eq!(key.alg(), Alg::Es256);
        assert_eq!(key.public_raw().len(), 65);
        assert_eq!(key.spki().len(), 91);
        let sig = key.sign(b"hello").unwrap();
        UnparsedPublicKey::new(&ECDSA_P256_SHA256_ASN1, key.public_raw())
            .verify(b"hello", &sig)
            .unwrap();
        // The PKCS#8 bytes read back to the same key.
        let again = CredentialKey::from_pkcs8(&pkcs8).unwrap();
        assert_eq!(again.public_raw(), key.public_raw());
    }

    #[test]
    fn the_cose_key_names_ec2_p256_and_its_coordinates() {
        let (key, _) = CredentialKey::generate().unwrap();
        let v: ciborium::Value = ciborium::from_reader(key.cose_public_key().as_slice()).unwrap();
        let map = v.as_map().unwrap();
        let get = |k: i64| {
            map.iter()
                .find(|(a, _)| a.as_integer() == Some(k.into()))
                .map(|(_, b)| b.clone())
                .unwrap()
        };
        assert_eq!(get(1), ciborium::Value::from(2));
        assert_eq!(get(3), ciborium::Value::from(-7));
        assert_eq!(get(-1), ciborium::Value::from(1));
        let mut point = vec![4u8];
        point.extend(get(-2).as_bytes().unwrap());
        point.extend(get(-3).as_bytes().unwrap());
        assert_eq!(point, key.public_raw());
        // Canonical order: 1, 3, -1, -2, -3.
        let keys: Vec<i128> = map
            .iter()
            .map(|(k, _)| k.as_integer().unwrap().into())
            .collect();
        assert_eq!(keys, [1, 3, -1, -2, -3]);
    }

    #[test]
    fn an_imported_ed25519_key_signs() {
        let rng = SystemRandom::new();
        let doc = Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
        let key = CredentialKey::from_pkcs8(doc.as_ref()).unwrap();
        assert_eq!(key.alg(), Alg::Ed25519);
        assert_eq!(key.alg().cose(), -8);
        let sig = key.sign(b"x").unwrap();
        UnparsedPublicKey::new(&ED25519, key.public_raw())
            .verify(b"x", &sig)
            .unwrap();
        assert_eq!(&key.spki()[..12], &ED25519_SPKI_PREFIX);
    }

    #[test]
    fn something_else_is_refused_with_words() {
        let e = CredentialKey::from_pkcs8(b"not a key").unwrap_err();
        assert_eq!(e.name, RefusalName::NotSupported);
        assert!(e.message.contains("P-256"));
    }
}
