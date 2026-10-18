//! Post-quantum key wrapping, as a rule every new wrap to another key
//! follows (`pq-key-wrapping`; 04-security §1 "each wrap needs both X25519
//! and ML-KEM-768 broken", §2 "hybrid HPKE to a peer"; D23, D30;
//! docs/plan/22-security-depth.md §6).
//!
//! Every key Lockdown hands to another persona or device travels in the
//! identity crate's peer wrap, whose algorithm follows the receiving key:
//! `x-wing-xchacha20poly1305` for a hybrid X25519 + ML-KEM-768 key, the
//! classical `x25519-xchacha20poly1305` for an X25519 one. The crate makes
//! every persona and device key hybrid, so in practice every wrap already
//! is; this module makes it a refusal instead of a habit:
//!
//! - **New wraps** (a vault key to a member or a household, an item key, a
//!   held-back emergency key, the root key to an approved device, the
//!   Secret Key that rides with an approval) go only to hybrid receiving
//!   keys ([`require_hybrid`]). A classical key, whether an old record or a
//!   server's substitution, is refused by name before anything is sealed.
//! - **Old wraps still open**: the unwrap paths take either algorithm, as
//!   the envelope names it, so nothing made before this rule is stranded.
//! - Symmetric wraps (the vault key under the App Data Key, item and
//!   version keys, the kit and account slots) are already safe against a
//!   quantum computer at 256 bits and are untouched.

use ecosystem_identity::kem::{KemAlg, KemPublicKey};

use crate::{Error, Result};

/// The receiving-key scheme every new wrap needs: X-Wing.
pub const REQUIRED_KEM: KemAlg = KemAlg::X25519MlKem768;

/// Refuses a receiving key that isn't hybrid, naming `whose` it is.
pub fn require_hybrid(key: &KemPublicKey, whose: &str) -> Result<()> {
    if key.alg == REQUIRED_KEM {
        Ok(())
    } else {
        Err(Error::Invalid(format!(
            "{whose} has only a classical receiving key (X25519 without ML-KEM-768); Lockdown wraps keys only to post-quantum hybrid keys, so nothing was sealed. They need to update their app"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_hybrid_receiving_keys_take_new_wraps() {
        let (_, hybrid) = ecosystem_identity::kem::generate(KemAlg::X25519MlKem768).unwrap();
        let (_, classical) = ecosystem_identity::kem::generate(KemAlg::X25519).unwrap();
        require_hybrid(&hybrid, "Sam").unwrap();
        let err = require_hybrid(&classical, "Sam").unwrap_err().to_string();
        assert!(err.contains("Sam has only a classical"), "{err}");
    }
}
