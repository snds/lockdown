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
use ecosystem_identity::{Head, Identity, Record as _};
use lockdown_crypto::pq_sign::{PqSigningKey, PqVerifyingKey};

use crate::identity_set::IdentityManifest;
use crate::{Error, Result};
#[cfg(test)]
use ecosystem_identity::records::Id;

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
pub fn identity_pq_key<K: DeviceKeystore + ?Sized>(
    identity: &Identity,
    keystore: &K,
) -> Result<PqSigningKey> {
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

/// The ML-DSA-65 key a profile record's bytes publish, if any: for a
/// reader that holds the record but not the identity (the server).
pub fn published_key_in(profile_record: &[u8]) -> Result<Option<PqVerifyingKey>> {
    published_key(&Profile::from_bytes(profile_record)?)
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

// --- the rollout rule (D248's audit item; docs/plan/26-desktop-finish.md §10) ----
//
// A client from before the post-quantum signature decodes a manifest that
// carries `pq_sig` but drops the field when it re-encodes one, so its hash
// of the manifest differs and its chain check fails. So the key is
// published (and with it, the second signature required) only once every
// device of the identity says it reads and keeps the field: each device
// marks its own `Device` record (`lockdown.pq`, signed by the identity key
// like a rename; the identity crate keeps fields it doesn't know), and a
// device publishes the key only when every device not revoked carries the
// mark. Once published it stays; a device added later is a build that
// already reads it, since it is this build or newer.

/// The `Device` record field a device marks to say what it supports.
pub const SUPPORT_FIELD: &str = "lockdown.pq";
/// Level 1: reads, keeps and checks `pq_sig` on identity manifests.
pub const MANIFEST_LEVEL: u64 = 1;
/// Level 2: also on vault heads, the log's tree heads and checkpoints
/// (docs/plan/28-pq-signatures.md).
pub const HEAD_LEVEL: u64 = 2;
/// What this build supports, and marks its own record with.
pub const SUPPORT_LEVEL: u64 = HEAD_LEVEL;

/// Whether `device` says it supports the post-quantum signatures at
/// `level` or above.
pub fn supports_level(device: &ecosystem_identity::records::Device, level: u64) -> bool {
    matches!(device.extra.get(SUPPORT_FIELD), Some(ciborium::Value::Integer(n)) if i128::from(*n) >= i128::from(level))
}

/// Whether `device` says it supports the post-quantum signature on
/// identity manifests (level 1).
pub fn supports(device: &ecosystem_identity::records::Device) -> bool {
    supports_level(device, MANIFEST_LEVEL)
}

/// The devices not revoked that don't say so yet, by name: who holds the
/// post-quantum signature on identity manifests back.
pub fn devices_without_support(identity: &Identity) -> Vec<String> {
    devices_below(identity, MANIFEST_LEVEL)
}

/// The devices not revoked that don't read post-quantum vault heads yet,
/// by name: who holds them back.
pub fn devices_without_head_support(identity: &Identity) -> Vec<String> {
    devices_below(identity, HEAD_LEVEL)
}

fn devices_below(identity: &Identity, level: u64) -> Vec<String> {
    let mut names: Vec<String> = identity
        .devices()
        .filter(|d| d.revoked_at.is_none() && !supports_level(d, level))
        .map(|d| d.name.clone())
        .collect();
    names.sort();
    names
}

/// Marks this device's record as supporting the post-quantum signatures,
/// once. Returns the new head when it wrote one.
pub fn mark_this_device(
    identity: &mut Identity,
    keystore: &dyn DeviceKeystore,
) -> Result<Option<Head>> {
    crate::identity::check_manager_public(identity)?;
    let device_id = identity.device_id();
    crate::identity::write_identity_store(identity, keystore, |contents, isk| {
        let device = contents
            .devices
            .get_mut(&device_id)
            .ok_or_else(|| Error::NotFound(format!("device {device_id} in this identity")))?;
        if supports_level(device, SUPPORT_LEVEL) {
            return Ok(false);
        }
        device.extra.insert(
            SUPPORT_FIELD.to_string(),
            ciborium::Value::Integer(SUPPORT_LEVEL.into()),
        );
        device.sign(isk.inner())?;
        Ok(true)
    })
}

/// The rollout rule: marks this device, then publishes the key only when
/// every device not revoked supports it (or it is published already).
/// Answers whether the key is published now.
pub fn roll_out(identity: &mut Identity, keystore: &dyn DeviceKeystore) -> Result<bool> {
    mark_this_device(identity, keystore)?;
    if published_key(identity.profile())?.is_some() {
        return Ok(true);
    }
    if !devices_without_support(identity).is_empty() {
        return Ok(false);
    }
    publish_key(identity, keystore)?;
    Ok(true)
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

// --- post-quantum vault heads (docs/plan/28-pq-signatures.md) -------------------
//
// A vault head is signed by the device that made it (Ed25519) and, beside
// that, by the identity's ML-DSA-65 key over the same bytes
// (`ManifestHead.pq_sig`). Every item version is pinned by its BLAKE3 hash
// in the head, and every head names the one before by hash, so one
// post-quantum signature on a head covers the whole history behind it:
// the items need none of their own. The rules:
//
// - **Starting.** A device signs heads post-quantum once the profile
//   publishes the key and every device not revoked marks level 2
//   ([`HEAD_LEVEL`]), or once the chain it extends already carries one
//   (another device saw the rollout finish first).
// - **Checking.** Every `pq_sig` on a head by one of the identity's devices
//   must verify under the published key. Once a head in a chain carries
//   one, every later head by the identity's devices must too, and so must
//   every head from the `seq` this device remembers seeing the first one
//   at (`HeadMemory::pq_since`), so a server that strips the signatures
//   from the whole chain is refused, not just ignored.
// - **Not covered.** Heads signed by other people's devices in a shared
//   vault stay Ed25519 (their keys would come from pinned contacts; doc 28
//   §8).

/// What a vault needs to sign and check post-quantum heads.
#[derive(Clone)]
pub struct HeadPq {
    /// The identity's published ML-DSA-65 key.
    pub key: PqVerifyingKey,
    /// This device's copy of the signing half, when it holds the root key.
    pub signer: Option<std::sync::Arc<PqSigningKey>>,
    /// Whether every device not revoked reads post-quantum heads, so this
    /// device starts signing them.
    pub ready: bool,
    /// The identity's devices: the heads the rules apply to.
    pub own: std::collections::BTreeSet<ecosystem_identity::records::Id>,
}

impl std::fmt::Debug for HeadPq {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HeadPq")
            .field("key", &self.key)
            .field("signs", &self.signer.is_some())
            .field("ready", &self.ready)
            .field("own", &self.own.len())
            .finish()
    }
}

/// Whether the identity's devices all read post-quantum heads and its
/// profile publishes the key: the rollout rule for heads.
pub fn heads_ready(identity: &Identity) -> Result<bool> {
    Ok(published_key(identity.profile())?.is_some()
        && devices_without_head_support(identity).is_empty())
}

/// The post-quantum setting for the identity's vaults: `None` while the
/// profile publishes no key. `signing` is this device's derivation of the
/// identity's key (`None` where the signer holds no root key); it must be
/// the key the profile publishes, or the profile was changed under this
/// device. `own` names the identity's devices.
pub fn head_pq(
    identity: &Identity,
    signing: Option<PqSigningKey>,
    own: impl IntoIterator<Item = ecosystem_identity::records::Id>,
) -> Result<Option<HeadPq>> {
    let Some(key) = published_key(identity.profile())? else {
        return Ok(None);
    };
    if let Some(signing) = &signing
        && signing.verifying_key() != key
    {
        return Err(Error::Invalid(
            "the profile's post-quantum key isn't the one this identity's root key derives; the profile was changed under this device"
                .into(),
        ));
    }
    let mut own: std::collections::BTreeSet<_> = own.into_iter().collect();
    own.extend(identity.devices().map(|d| d.device_id));
    Ok(Some(HeadPq {
        key,
        signer: signing.map(std::sync::Arc::new),
        ready: heads_ready(identity)?,
        own,
    }))
}

/// Checks the post-quantum rules over `heads` (oldest first), and answers
/// the `seq` of the first head carrying a verified post-quantum signature.
/// `key` is the identity's published key (`None` when this device knows
/// none: then a `pq_sig` can't be checked and counts for nothing); `own`
/// says which signers are the identity's devices; `since` is the `seq`
/// this device remembers the first post-quantum head at.
pub fn check_heads(
    heads: &[crate::ManifestHead],
    key: Option<&PqVerifyingKey>,
    own: &dyn Fn(ecosystem_identity::records::Id) -> bool,
    since: Option<u64>,
) -> Result<Option<u64>> {
    if since.is_some() && key.is_none() {
        return Err(Error::Invalid(
            "this device saw post-quantum signatures on this vault but knows no post-quantum key for the identity now; refusing it"
                .into(),
        ));
    }
    let mut first: Option<u64> = None;
    for head in heads {
        if !own(head.signer) {
            continue;
        }
        match (key, &head.pq_sig) {
            (Some(key), Some(_)) => {
                head.verify_post_quantum(key)?;
                first.get_or_insert(head.seq);
            }
            (_, None) => {
                let required = first.is_some() || since.is_some_and(|s| head.seq >= s);
                if required {
                    return Err(Error::ChainBroken {
                        seq: head.seq,
                        why: "the head lost its post-quantum signature: heads before it carry one, so every later head must; refusing it".into(),
                    });
                }
            }
            (None, Some(_)) => {}
        }
    }
    Ok(first)
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

    #[test]
    fn the_key_is_published_only_once_every_device_says_it_keeps_the_signature() {
        use ecosystem_identity::FileKeystore;
        let dir = tempfile::tempdir().unwrap();
        let keystore = FileKeystore::open_or_create(&dir.path().join("a.cbor")).unwrap();
        let mut identity =
            Identity::create(&dir.path().join("a"), "lockdown", "a", &keystore).unwrap();
        // A second device from an older build: its record has no mark.
        let older = Id::new();
        crate::identity::write_identity_store(&mut identity, &keystore, |contents, isk| {
            let mut d = contents.devices.values().next().unwrap().clone();
            d.device_id = older;
            d.name = "Lockdown on the old laptop".into();
            d.extra.remove(SUPPORT_FIELD);
            d.sign(isk.inner())?;
            contents.devices.insert(older, d);
            Ok(true)
        })
        .unwrap();
        assert!(!roll_out(&mut identity, &keystore).unwrap(), "held back");
        assert!(published_key(identity.profile()).unwrap().is_none());
        assert_eq!(
            devices_without_support(&identity),
            ["Lockdown on the old laptop"]
        );
        assert!(supports(
            identity
                .devices()
                .find(|d| d.device_id == identity.device_id())
                .unwrap()
        ));
        // The older device updates (its record gets the mark): now it goes out.
        crate::identity::write_identity_store(&mut identity, &keystore, |contents, isk| {
            let d = contents.devices.get_mut(&older).unwrap();
            d.extra
                .insert(SUPPORT_FIELD.into(), ciborium::Value::Integer(1.into()));
            d.sign(isk.inner())?;
            Ok(true)
        })
        .unwrap();
        assert!(roll_out(&mut identity, &keystore).unwrap());
        assert!(published_key(identity.profile()).unwrap().is_some());
        // That older device reads manifests but not vault heads (level 1):
        // heads stay Ed25519 until it says level 2.
        assert!(!heads_ready(&identity).unwrap());
        assert_eq!(
            devices_without_head_support(&identity),
            ["Lockdown on the old laptop"]
        );
        let pq = head_pq(
            &identity,
            Some(identity_pq_key(&identity, &keystore).unwrap()),
            [],
        )
        .unwrap()
        .unwrap();
        assert!(!pq.ready && pq.signer.is_some());
        assert!(pq.own.contains(&older) && pq.own.contains(&identity.device_id()));
        crate::identity::write_identity_store(&mut identity, &keystore, |contents, isk| {
            let d = contents.devices.get_mut(&older).unwrap();
            d.extra.insert(
                SUPPORT_FIELD.into(),
                ciborium::Value::Integer(HEAD_LEVEL.into()),
            );
            d.sign(isk.inner())?;
            Ok(true)
        })
        .unwrap();
        assert!(heads_ready(&identity).unwrap());
        assert!(devices_without_head_support(&identity).is_empty());
        // A derivation that isn't the published key: the profile changed
        // under this device.
        assert!(head_pq(&identity, Some(PqSigningKey::from_seed([1; 32])), []).is_err());
        // Once out, it stays out.
        assert!(roll_out(&mut identity, &keystore).unwrap());
        assert!(
            mark_this_device(&mut identity, &keystore)
                .unwrap()
                .is_none(),
            "marked once"
        );
    }
}
