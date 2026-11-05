//! Ed25519 signing and verification, the same implementation the identity
//! crate signs its records with. The ML-DSA slot named in 03-architecture §2
//! is reserved for a later format version; nothing here precludes it.
//!
//! A [`SigningKey`] sits in one heap block that never moves, and the pages
//! under that block are locked out of swap while the key lives, on the
//! process's one page table ([`crate::locked`], doc 53). That covers the
//! server's token and log keys, which live as long as the server, and the
//! Identity Signing Key a record change rebuilds for one operation.
//! `ed25519-dalek` wipes the key on drop (its `zeroize` feature is on),
//! before the pages are unlocked.
//!
//! The key is built by value before it reaches its block
//! (`ed25519_dalek::SigningKey::from_bytes` has no in-place form), so it is
//! built on the thread's secret stack ([`crate::secret_stack`], doc 58):
//! locked pages that are wiped before the constructor returns. Each
//! signature runs there too, so the expanded secret and the nonce it
//! computes never sit on an ordinary stack page.

use std::fmt;

use ed25519_dalek::{Signer, Verifier};
use zeroize::Zeroize;

use crate::locked::PageLock;
use crate::secret_stack;
use crate::{Error, Result};

/// An Ed25519 signing key. Debug prints nothing of the key. Its pages are
/// locked out of swap while it lives, and it is wiped when it drops.
pub struct SigningKey {
    // Field order is drop order: `ed25519-dalek` wipes the key while its
    // pages are still locked, and they are unlocked after.
    key: Box<ed25519_dalek::SigningKey>,
    pages: Option<PageLock>,
}

/// An Ed25519 public key.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct VerifyingKey(ed25519_dalek::VerifyingKey);

impl SigningKey {
    /// A fresh random key.
    pub fn generate() -> Result<Self> {
        Ok(Self::from_bytes(crate::rand::random_32()?))
    }

    /// The key for a 32-byte seed, built on the secret stack. The seed is
    /// wiped once the key is made.
    pub fn from_bytes(mut seed: [u8; 32]) -> Self {
        let key = secret_stack::run(|| Box::new(ed25519_dalek::SigningKey::from_bytes(&seed)));
        seed.zeroize();
        Self::locked(key)
    }

    fn locked(key: Box<ed25519_dalek::SigningKey>) -> Self {
        let pages = PageLock::of_value(&*key);
        Self { key, pages }
    }

    /// Whether the pages under the key are locked out of swap now (`false`
    /// when the system refused, counted in [`crate::locked::report`], or
    /// this build cannot lock).
    pub fn is_locked(&self) -> bool {
        self.pages.is_some()
    }

    /// How many pages the lock holds: 0 when not locked, else 1, or 2 when
    /// the key's block crosses a page boundary.
    pub fn locked_pages(&self) -> usize {
        self.pages.as_ref().map_or(0, PageLock::pages)
    }

    /// The public half.
    pub fn verifying_key(&self) -> VerifyingKey {
        VerifyingKey(self.key.verifying_key())
    }

    /// Signs `message`, on the secret stack. Sixty-four bytes out.
    pub fn sign(&self, message: &[u8]) -> [u8; 64] {
        secret_stack::run(|| self.key.sign(message).to_bytes())
    }

    /// The underlying key, for the identity crate's record signing.
    pub fn inner(&self) -> &ed25519_dalek::SigningKey {
        &self.key
    }
}

impl From<ed25519_dalek::SigningKey> for SigningKey {
    /// Takes a key the identity crate made (the Identity Signing Key). The
    /// copy is made on the secret stack and goes into the locked block, and
    /// the key passed in is dropped, which wipes it where the caller passed
    /// it; a plain move would leave it there.
    fn from(k: ed25519_dalek::SigningKey) -> Self {
        let key = secret_stack::run(|| Box::new(k.clone()));
        drop(k);
        Self::locked(key)
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

    #[test]
    fn the_key_is_locked_out_of_swap_or_counted_as_refused() {
        let before = crate::locked::report();
        let seeded = SigningKey::from_bytes([5; 32]);
        let taken = SigningKey::from(ed25519_dalek::SigningKey::from_bytes(&[5; 32]));
        for key in [&seeded, &taken] {
            if key.is_locked() {
                // The key is a couple of hundred bytes: one page, or two
                // where its block crosses a boundary.
                assert!((1..=2).contains(&key.locked_pages()));
                assert!(crate::locked::report().pages_locked >= 1);
            } else {
                assert_eq!(key.locked_pages(), 0);
                assert!(crate::locked::report().refused > before.refused);
            }
        }
        // Locked or refused, the two sign the same bytes.
        assert_eq!(seeded.sign(b"m"), taken.sign(b"m"));
        assert_eq!(seeded.inner().to_bytes(), [5; 32]);
        seeded
            .verifying_key()
            .verify(b"m", &taken.sign(b"m"), "m")
            .unwrap();
    }

    /// A seed whose bytes appear nowhere by chance.
    const SEED: [u8; 32] = *b"an ed25519 seed for a wipe test.";

    /// The half of SHA-512(seed) that Ed25519 signs with (RFC 8032 §5.1.6,
    /// the nonce's prefix): a known slice of the expanded key.
    fn prefix(seed: &[u8; 32]) -> Vec<u8> {
        use sha2_010::Digest;
        sha2_010::Sha512::digest(seed)[32..].to_vec()
    }

    #[test]
    fn the_key_and_each_signature_leave_nothing_on_the_secret_stack() {
        use crate::secret_stack::testing::{holds, pinned, unwiped, wipe};
        pinned(|| {
            let prefix = prefix(&SEED);
            // Made, taken from the identity crate, and signed with: the
            // block holds neither the seed nor the expanded key after.
            let key = SigningKey::from_bytes(SEED);
            assert_eq!(holds(&SEED), Some(false));
            let taken = SigningKey::from(ed25519_dalek::SigningKey::from_bytes(&SEED));
            assert_eq!(holds(&SEED), Some(false));
            assert_eq!(key.sign(b"a vault head"), taken.sign(b"a vault head"));
            assert_eq!(holds(&SEED), Some(false));
            assert_eq!(holds(&prefix), Some(false));

            // The control: the same calls with the wipe off leave both.
            let again = unwiped(|| SigningKey::from_bytes(SEED));
            assert_eq!(holds(&SEED), Some(true), "the constructor left the seed");
            wipe();
            let _ = unwiped(|| SigningKey::from(ed25519_dalek::SigningKey::from_bytes(&SEED)));
            assert_eq!(holds(&SEED), Some(true), "the clone left the key");
            wipe();
            unwiped(|| again.sign(b"a vault head"));
            assert_eq!(
                holds(&prefix),
                Some(true),
                "a signature left the expanded key"
            );
            wipe();
            assert_eq!(holds(&SEED), Some(false));
            assert_eq!(holds(&prefix), Some(false));
        });
    }
}
