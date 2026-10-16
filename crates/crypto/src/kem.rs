//! Hybrid post-quantum key encapsulation: X-Wing (X25519 + ML-KEM-768), the
//! identity crate's scheme (D23). Lockdown uses it to wrap vault keys to
//! another persona's receiving key when sharing (02-model §1, brief §4.5).
//!
//! The types are the identity crate's, re-exported, so a `Persona.kem_pub`
//! read from the identity store is directly usable here.

pub use ecosystem_identity::kem::{
    DEFAULT_KEM, Kem, KemAlg, KemPublicKey, KemSecretKey, SharedSecret, generate, kem,
};

use crate::Result;

/// A fresh hybrid key pair (X-Wing).
pub fn generate_hybrid() -> Result<(KemSecretKey, KemPublicKey)> {
    Ok(generate(KemAlg::X25519MlKem768)?)
}

/// Makes a shared secret for the holder of `public` and returns the
/// ciphertext that carries it (1120 bytes for X-Wing).
pub fn encapsulate(public: &KemPublicKey) -> Result<(Vec<u8>, SharedSecret)> {
    Ok(kem(public.alg).encapsulate(public)?)
}

/// Recovers the shared secret from `ciphertext` with the private half.
pub fn decapsulate(secret: &KemSecretKey, ciphertext: &[u8]) -> Result<SharedSecret> {
    Ok(kem(secret.alg()).decapsulate(secret, ciphertext)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x_wing_round_trip() {
        let (sk, pk) = generate_hybrid().unwrap();
        assert_eq!(pk.alg, KemAlg::X25519MlKem768);
        assert_eq!(pk.bytes.len(), 1216);
        let (ct, a) = encapsulate(&pk).unwrap();
        assert_eq!(ct.len(), 1120);
        let b = decapsulate(&sk, &ct).unwrap();
        assert_eq!(a.expose(), b.expose());
        // Another key pair does not recover the same secret.
        let (other, _) = generate_hybrid().unwrap();
        let c = decapsulate(&other, &ct).unwrap();
        assert_ne!(a.expose(), c.expose());
        // A truncated ciphertext is refused.
        assert!(decapsulate(&sk, &ct[..ct.len() - 1]).is_err());
    }
}
