//! ML-DSA-65 (FIPS 204), the post-quantum signature that sits beside
//! Ed25519 where the plan reserved a slot for it (01-identity §3 "ISK,
//! Ed25519; ML-DSA slot reserved"; 03-architecture §2; 04-security §1,
//! "Quantum adversary, later").
//!
//! A [`PqSigningKey`] is made from a 32-byte seed (FIPS 204 `ξ`), so it is
//! derived the way the identity crate derives the Identity Signing Key
//! (D19): never stored, always rebuilt from the key above it. Signing is
//! the deterministic variant with a context string, so the same message
//! under the same key and context gives the same bytes, and a signature
//! made for one purpose never verifies for another.
//!
//! [`HybridSignature`] is the pair Lockdown writes where both apply: an
//! Ed25519 signature and an ML-DSA-65 signature over the same bytes. A
//! verifier that has pinned the ML-DSA key requires both, so a forger needs
//! to break both schemes; one that has not (an older build, or a record
//! from before the key existed) checks Ed25519 alone, as before.

use std::fmt;

use ml_dsa::{EncodedVerifyingKey, ExpandedSigningKey, MlDsa65, Seed, Signature, VerifyingKey};
use zeroize::Zeroize;

use crate::{Error, Result};

/// Bytes in an encoded ML-DSA-65 public key.
pub const PUBLIC_KEY_LEN: usize = 1952;
/// Bytes in an encoded ML-DSA-65 signature.
pub const SIGNATURE_LEN: usize = 3309;
/// The longest context FIPS 204 allows.
const MAX_CONTEXT: usize = 255;

/// An ML-DSA-65 signing key. Debug prints nothing of it; the seed is wiped
/// when it drops.
pub struct PqSigningKey {
    seed: [u8; 32],
    key: Box<ExpandedSigningKey<MlDsa65>>,
}

/// An ML-DSA-65 public key.
#[derive(Clone, PartialEq, Eq)]
pub struct PqVerifyingKey(Vec<u8>);

impl PqSigningKey {
    /// The key for a 32-byte seed (FIPS 204 `ML-DSA.KeyGen_internal(ξ)`).
    pub fn from_seed(seed: [u8; 32]) -> Self {
        let xi = Seed::from(seed);
        let key = Box::new(ExpandedSigningKey::<MlDsa65>::from_seed(&xi));
        Self { seed, key }
    }

    /// The public half.
    pub fn verifying_key(&self) -> PqVerifyingKey {
        PqVerifyingKey(self.key.verifying_key().encode().to_vec())
    }

    /// Signs `message` under `context` (at most 255 bytes, never empty in
    /// Lockdown: every use names itself). 3309 bytes out.
    pub fn sign(&self, message: &[u8], context: &[u8]) -> Result<Vec<u8>> {
        check_context(context)?;
        let sig = self
            .key
            .sign_deterministic(message, context)
            .map_err(|_| Error::Invalid("ML-DSA refused the message".into()))?;
        Ok(sig.encode().to_vec())
    }
}

impl Drop for PqSigningKey {
    fn drop(&mut self) {
        self.seed.zeroize();
    }
}

impl fmt::Debug for PqSigningKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PqSigningKey(…)")
    }
}

impl PqVerifyingKey {
    /// A key from its 1952 encoded bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != PUBLIC_KEY_LEN {
            return Err(Error::Invalid(format!(
                "an ML-DSA-65 public key is {PUBLIC_KEY_LEN} bytes, not {}",
                bytes.len()
            )));
        }
        Ok(Self(bytes.to_vec()))
    }

    /// The encoded bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Checks `signature` over `message` under `context`; `what` names the
    /// thing checked, for the error.
    pub fn verify(
        &self,
        message: &[u8],
        context: &[u8],
        signature: &[u8],
        what: &str,
    ) -> Result<()> {
        check_context(context)?;
        let bad = || Error::BadSignature(format!("{what} (post-quantum signature)"));
        let encoded = EncodedVerifyingKey::<MlDsa65>::try_from(self.0.as_slice())
            .map_err(|_| Error::Invalid("not an ML-DSA-65 public key".into()))?;
        let key = VerifyingKey::<MlDsa65>::decode(&encoded);
        let sig = Signature::<MlDsa65>::try_from(signature).map_err(|_| bad())?;
        if key.verify_with_context(message, context, &sig) {
            Ok(())
        } else {
            Err(bad())
        }
    }
}

impl fmt::Debug for PqVerifyingKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let fp = crate::hash::hash(&self.0);
        write!(f, "PqVerifyingKey({}…)", &fp.to_hex()[..16])
    }
}

fn check_context(context: &[u8]) -> Result<()> {
    if context.is_empty() || context.len() > MAX_CONTEXT {
        return Err(Error::Invalid(
            "an ML-DSA context names its use in 1 to 255 bytes".into(),
        ));
    }
    Ok(())
}

/// Ed25519 and ML-DSA-65 over the same bytes.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct HybridSignature {
    /// The Ed25519 signature (64 bytes).
    pub ed25519: [u8; 64],
    /// The ML-DSA-65 signature (3309 bytes).
    pub ml_dsa: Vec<u8>,
}

/// Signs `message` with both keys: the classical signature as before, and
/// the post-quantum one under `context`.
pub fn sign_hybrid(
    classical: &crate::SigningKey,
    post_quantum: &PqSigningKey,
    message: &[u8],
    context: &[u8],
) -> Result<HybridSignature> {
    Ok(HybridSignature {
        ed25519: classical.sign(message),
        ml_dsa: post_quantum.sign(message, context)?,
    })
}

/// Checks both halves; either failing is a failure.
pub fn verify_hybrid(
    classical: &crate::VerifyingKey,
    post_quantum: &PqVerifyingKey,
    message: &[u8],
    context: &[u8],
    signature: &HybridSignature,
    what: &str,
) -> Result<()> {
    classical.verify(message, &signature.ed25519, what)?;
    post_quantum.verify(message, context, &signature.ml_dsa, what)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_seed_gives_one_key_and_signatures_check_under_their_context_only() {
        let a = PqSigningKey::from_seed([7; 32]);
        let again = PqSigningKey::from_seed([7; 32]);
        let other = PqSigningKey::from_seed([8; 32]);
        let pk = a.verifying_key();
        assert_eq!(pk, again.verifying_key(), "derived, not random");
        assert_ne!(pk, other.verifying_key());
        assert_eq!(pk.as_bytes().len(), PUBLIC_KEY_LEN);

        let sig = a.sign(b"identity manifest 4", b"lockdown/test/v1").unwrap();
        assert_eq!(sig.len(), SIGNATURE_LEN);
        assert_eq!(
            sig,
            again
                .sign(b"identity manifest 4", b"lockdown/test/v1")
                .unwrap()
        );
        pk.verify(b"identity manifest 4", b"lockdown/test/v1", &sig, "x")
            .unwrap();
        // Another message, another context, another key, a bent byte: no.
        assert!(
            pk.verify(b"identity manifest 5", b"lockdown/test/v1", &sig, "x")
                .is_err()
        );
        assert!(
            pk.verify(b"identity manifest 4", b"lockdown/other/v1", &sig, "x")
                .is_err()
        );
        assert!(
            other
                .verifying_key()
                .verify(b"identity manifest 4", b"lockdown/test/v1", &sig, "x")
                .is_err()
        );
        let mut bent = sig.clone();
        bent[100] ^= 1;
        assert!(
            pk.verify(b"identity manifest 4", b"lockdown/test/v1", &bent, "x")
                .is_err()
        );
        assert!(
            pk.verify(b"identity manifest 4", b"lockdown/test/v1", &sig[..10], "x")
                .is_err()
        );
        assert!(a.sign(b"m", b"").is_err(), "a context is required");
        assert!(PqVerifyingKey::from_bytes(&[0; 32]).is_err());
        assert_eq!(format!("{a:?}"), "PqSigningKey(…)");
    }

    #[test]
    fn a_hybrid_signature_needs_both_halves() {
        let ed = crate::SigningKey::from_bytes([1; 32]);
        let pq = PqSigningKey::from_seed([2; 32]);
        let sig = sign_hybrid(&ed, &pq, b"head", b"lockdown/test/v1").unwrap();
        verify_hybrid(
            &ed.verifying_key(),
            &pq.verifying_key(),
            b"head",
            b"lockdown/test/v1",
            &sig,
            "head",
        )
        .unwrap();
        let mut classical_bent = sig.clone();
        classical_bent.ed25519[0] ^= 1;
        assert!(
            verify_hybrid(
                &ed.verifying_key(),
                &pq.verifying_key(),
                b"head",
                b"lockdown/test/v1",
                &classical_bent,
                "head",
            )
            .is_err()
        );
        let mut pq_bent = sig;
        pq_bent.ml_dsa[0] ^= 1;
        assert!(
            verify_hybrid(
                &ed.verifying_key(),
                &pq.verifying_key(),
                b"head",
                b"lockdown/test/v1",
                &pq_bent,
                "head",
            )
            .is_err()
        );
    }
}
