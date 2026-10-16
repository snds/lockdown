//! Seal and open with XChaCha20-Poly1305 and associated data (04-security §5).
//!
//! Associated data (AAD) is authenticated but not encrypted. It glues a
//! ciphertext to the one place it belongs: the vault binds every field to
//! `{profile_id, persona_id, vault_id, item_id, version_n, field_id, kind,
//! schema_v}` (02-model §2). A server that moves a ciphertext gets a tag
//! failure, not a swapped value. Empty AAD is refused, so no unbound path
//! exists.

use chacha20poly1305::aead::{Aead, Payload};
use chacha20poly1305::{KeyInit, XChaCha20Poly1305, XNonce};
use ecosystem_identity::SymmetricKey;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::rand::random_nonce;
use crate::{Error, Result};

/// A ciphertext and the nonce it was sealed under. The associated data is
/// not stored here: the reader recomputes it from where it found the
/// ciphertext, which is the whole point.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Sealed {
    /// The 24-byte random nonce.
    #[serde(with = "serde_bytes")]
    pub nonce: Vec<u8>,
    /// The ciphertext followed by the 16-byte tag.
    #[serde(with = "serde_bytes")]
    pub ct: Vec<u8>,
}

fn cipher<K: SymmetricKey>(key: &K) -> XChaCha20Poly1305 {
    XChaCha20Poly1305::new(&chacha20poly1305::Key::from(*key.expose()))
}

/// Seals `plaintext` under `key`, bound to `aad`. A fresh random nonce is
/// drawn every time.
pub fn seal<K: SymmetricKey>(key: &K, aad: &[u8], plaintext: &[u8]) -> Result<Sealed> {
    if aad.is_empty() {
        return Err(Error::EmptyAad);
    }
    let nonce = random_nonce()?;
    let ct = cipher(key)
        .encrypt(
            &XNonce::from(nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| Error::Invalid("couldn't seal".into()))?;
    Ok(Sealed {
        nonce: nonce.to_vec(),
        ct,
    })
}

/// Opens `sealed` under `key` at the place `aad` names. Any change to the
/// key, the nonce, the ciphertext or the place fails as [`Error::Tampered`].
pub fn open<K: SymmetricKey>(key: &K, aad: &[u8], sealed: &Sealed) -> Result<Zeroizing<Vec<u8>>> {
    if aad.is_empty() {
        return Err(Error::EmptyAad);
    }
    let nonce = XNonce::try_from(sealed.nonce.as_slice()).map_err(|_| Error::Tampered)?;
    let plain = cipher(key)
        .decrypt(
            &nonce,
            Payload {
                msg: &sealed.ct,
                aad,
            },
        )
        .map_err(|_| Error::Tampered)?;
    Ok(Zeroizing::new(plain))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Key32;

    #[test]
    fn round_trip_and_every_tamper_fails() {
        let key = Key32::generate().unwrap();
        let aad = b"field:pw/item:1";
        let sealed = seal(&key, aad, b"correct horse").unwrap();
        assert_eq!(
            open(&key, aad, &sealed).unwrap().as_slice(),
            b"correct horse"
        );

        // Moved to another place.
        assert!(matches!(
            open(&key, b"field:pw/item:2", &sealed),
            Err(Error::Tampered)
        ));
        // Another key.
        let other = Key32::generate().unwrap();
        assert!(matches!(open(&other, aad, &sealed), Err(Error::Tampered)));
        // A flipped ciphertext byte.
        let mut bent = sealed.clone();
        bent.ct[0] ^= 1;
        assert!(matches!(open(&key, aad, &bent), Err(Error::Tampered)));
        // A short nonce.
        let mut short = sealed.clone();
        short.nonce.pop();
        assert!(matches!(open(&key, aad, &short), Err(Error::Tampered)));
    }

    #[test]
    fn empty_aad_is_refused_both_ways() {
        let key = Key32::generate().unwrap();
        assert!(matches!(seal(&key, b"", b"x"), Err(Error::EmptyAad)));
        let sealed = seal(&key, b"place", b"x").unwrap();
        assert!(matches!(open(&key, b"", &sealed), Err(Error::EmptyAad)));
    }

    #[test]
    fn nonces_are_fresh() {
        let key = Key32::generate().unwrap();
        let a = seal(&key, b"p", b"same").unwrap();
        let b = seal(&key, b"p", b"same").unwrap();
        assert_ne!(a.nonce, b.nonce);
        assert_ne!(a.ct, b.ct);
    }
}
