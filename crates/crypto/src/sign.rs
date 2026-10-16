//! Ed25519 signing and verification, the same implementation the identity
//! crate signs its records with. The ML-DSA slot named in 03-architecture §2
//! is reserved for a later format version; nothing here precludes it.

use std::fmt;

use ed25519_dalek::{Signer, Verifier};

use crate::{Error, Result};

/// An Ed25519 signing key. Debug prints nothing of the key.
pub struct SigningKey(ed25519_dalek::SigningKey);

/// An Ed25519 public key.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct VerifyingKey(ed25519_dalek::VerifyingKey);

impl SigningKey {
    /// A fresh random key.
    pub fn generate() -> Result<Self> {
        Ok(Self::from_bytes(crate::rand::random_32()?))
    }

    /// The key for a 32-byte seed.
    pub fn from_bytes(seed: [u8; 32]) -> Self {
        Self(ed25519_dalek::SigningKey::from_bytes(&seed))
    }

    /// The public half.
    pub fn verifying_key(&self) -> VerifyingKey {
        VerifyingKey(self.0.verifying_key())
    }

    /// Signs `message`. Sixty-four bytes out.
    pub fn sign(&self, message: &[u8]) -> [u8; 64] {
        self.0.sign(message).to_bytes()
    }

    /// The underlying key, for the identity crate's record signing.
    pub fn inner(&self) -> &ed25519_dalek::SigningKey {
        &self.0
    }
}

impl From<ed25519_dalek::SigningKey> for SigningKey {
    fn from(k: ed25519_dalek::SigningKey) -> Self {
        Self(k)
    }
}

impl fmt::Debug for SigningKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SigningKey(…)")
    }
}

impl VerifyingKey {
    /// The key for its 32 encoded bytes.
    pub fn from_bytes(bytes: &[u8; 32]) -> Result<Self> {
        ed25519_dalek::VerifyingKey::from_bytes(bytes)
            .map(Self)
            .map_err(|_| Error::Invalid("not an Ed25519 public key".into()))
    }

    /// The 32 encoded bytes.
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_bytes()
    }

    /// Checks `signature` over `message`. `what` names the thing being
    /// checked, for the error message.
    pub fn verify(&self, message: &[u8], signature: &[u8], what: &str) -> Result<()> {
        let sig = ed25519_dalek::Signature::from_slice(signature)
            .map_err(|_| Error::BadSignature(what.to_string()))?;
        self.0
            .verify(message, &sig)
            .map_err(|_| Error::BadSignature(what.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_verify_and_reject() {
        let sk = SigningKey::generate().unwrap();
        let vk = sk.verifying_key();
        let sig = sk.sign(b"head 7");
        vk.verify(b"head 7", &sig, "head").unwrap();
        assert!(matches!(
            vk.verify(b"head 8", &sig, "head"),
            Err(Error::BadSignature(w)) if w == "head"
        ));
        let other = SigningKey::generate().unwrap().verifying_key();
        assert!(other.verify(b"head 7", &sig, "head").is_err());
        assert!(vk.verify(b"head 7", &sig[..63], "head").is_err());
        let again = VerifyingKey::from_bytes(&vk.to_bytes()).unwrap();
        assert_eq!(again, vk);
        assert_eq!(format!("{sk:?}"), "SigningKey(…)");
    }
}
