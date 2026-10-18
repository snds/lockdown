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

use ecosystem_identity::SymmetricKey as _;
use ecosystem_identity::kem::{KemAlg, KemPublicKey};
use ecosystem_identity::keystore::DeviceKeystore;
use ecosystem_identity::records::Profile;
use ecosystem_identity::{Head, Identity};
use lockdown_crypto::pq_sign::{PqSigningKey, PqVerifyingKey};

use crate::identity_set::IdentityManifest;
use crate::{Error, Result};

/// The profile field publishing the identity's ML-DSA-65 public key.
pub const PQ_ISK_FIELD: &str = "lockdown.pq_isk";
/// HKDF info for the post-quantum signing key's seed, from the root key.
const PQ_ISK_INFO: &[u8] = b"lockdown/pq-isk/ml-dsa-65/v1";
/// The ML-DSA context string of an identity manifest signature.
const MANIFEST_CONTEXT: &[u8] = b"lockdown identity manifest";

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

// --- the identity's post-quantum signature ---------------------------------------
//
// Beside the Ed25519 Identity Signing Key the identity crate derives from
// the root key, Lockdown derives an ML-DSA-65 key from the same root key
// (its own HKDF label), publishes the public half on the profile
// ([`PQ_ISK_FIELD`], signed with the rest of the profile), and signs every
// identity manifest with both. A reader holding a profile that publishes
// the key requires the second signature; a contact pins the key when first
// seen and refuses an answer that drops or changes it
// (`Contact::check_continues`). Device signatures, vault heads and the
// key-transparency log stay Ed25519 (docs/plan/22-security-depth.md §6).

/// The identity's ML-DSA-65 signing key, derived from the root key.
pub fn identity_pq_key(identity: &Identity, keystore: &dyn DeviceKeystore) -> Result<PqSigningKey> {
    let (irk, _) = crate::handover::root_key(identity, keystore)?;
    let seed = ecosystem_identity::kdf::subkey(irk.expose(), PQ_ISK_INFO)?;
    Ok(PqSigningKey::from_seed(*seed))
}

/// The ML-DSA-65 key a profile publishes, if any. A malformed one is an
/// error, never "none".
pub fn published_key(profile: &Profile) -> Result<Option<PqVerifyingKey>> {
    match profile.extra.get(PQ_ISK_FIELD) {
        None => Ok(None),
        Some(ciborium::Value::Bytes(b)) => Ok(Some(PqVerifyingKey::from_bytes(b)?)),
        Some(_) => Err(Error::Invalid(
            "the profile's post-quantum signing key is malformed".into(),
        )),
    }
}

/// Publishes this identity's ML-DSA-65 key on its profile, once. Returns
/// the new head when it wrote one.
pub fn publish_key(identity: &mut Identity, keystore: &dyn DeviceKeystore) -> Result<Option<Head>> {
    let key = identity_pq_key(identity, keystore)?.verifying_key();
    if published_key(identity.profile())?.as_ref() == Some(&key) {
        return Ok(None);
    }
    crate::handover::set_profile_extra(
        identity,
        keystore,
        &[(
            PQ_ISK_FIELD,
            Some(ciborium::Value::Bytes(key.as_bytes().to_vec())),
        )],
    )
}

/// Adds the ML-DSA-65 signature to a manifest already signed with Ed25519.
pub fn sign_manifest(manifest: &mut IdentityManifest, key: &PqSigningKey) -> Result<()> {
    let sig = key.sign(&manifest.signable()?, MANIFEST_CONTEXT)?;
    manifest.pq_sig = Some(serde_bytes::ByteBuf::from(sig));
    Ok(())
}

/// Checks a manifest's ML-DSA-65 signature when its profile publishes the
/// key: then it must be there and verify. A profile without the key (an
/// identity from before this) needs none.
pub fn verify_manifest(profile: &Profile, manifest: &IdentityManifest) -> Result<()> {
    let Some(key) = published_key(profile)? else {
        return Ok(());
    };
    let broken = |why: &str| Error::ChainBroken {
        seq: manifest.seq,
        why: why.into(),
    };
    let sig = manifest.pq_sig.as_ref().ok_or_else(|| {
        broken("the identity publishes a post-quantum key but the manifest isn't signed with it")
    })?;
    key.verify(
        &manifest.signable()?,
        MANIFEST_CONTEXT,
        sig,
        &format!("identity manifest {}", manifest.seq),
    )
    .map_err(|_| broken("the identity manifest's post-quantum signature didn't check out"))
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
