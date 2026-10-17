//! The identity's signed record set, as it travels between machines
//! (07-workspace §4 step 7; closes D59): every signed record the identity
//! crate keeps for a profile, listed by one Lockdown **identity manifest**
//! that the Identity Signing Key signs and that chains by hash, so the
//! server can order it exactly as it orders vault heads and no machine
//! ever has to take the server's word for who the devices are.
//!
//! The identity crate's own on-disk manifest is per root: a device that
//! recovered from a backup starts a fresh chain, so two machines never
//! share one. This manifest sits above those: it names the *records*
//! (profile, personas, devices, key envelopes other than device wraps,
//! consents) by path and hash, exactly the files a root would hold, plus
//! the one thing the records cannot say on their own: for each revoked
//! device, the newest vault head it may have signed (`Revocation`). A
//! head after that point from that device is refused by the server and
//! by every client (04-security §7 "signer is a current member").
//!
//! What crosses the wire is the records' bytes as the ISK signed them and
//! this manifest's bytes. The server stores both, checks the chain and the
//! signatures, and reads the device records to know who may still write;
//! it never sees a key.

use std::collections::{BTreeMap, BTreeSet};

use ecosystem_identity::records::{Consent, Device, KeyEnvelope, KeyRefKind, Persona, Profile};
use ecosystem_identity::{Backup, FORMAT, Record};
use lockdown_crypto::cbor::to_canonical;
use lockdown_crypto::hash::hash;
use lockdown_crypto::{SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::{Error, Hash32, Id, Result, Timestamp};

/// The identity manifest format version.
pub const IDENTITY_MANIFEST_V: u16 = 1;

/// The path of the profile record, as the identity crate stores it.
pub const PROFILE_PATH: &str = "profile.cbor";

/// One record the manifest lists.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct RecordRef {
    /// Path relative to an identity root, with `/` separators
    /// (`profile.cbor`, `personas/<id>.cbor`, `devices/<id>.cbor`,
    /// `envelopes/<name>`, `consents/<name>`).
    pub path: String,
    /// BLAKE3 of the record's bytes.
    pub hash: Hash32,
}

/// Where a revoked device's writes stop in one vault.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct VaultBoundary {
    /// The vault.
    pub vault_id: Id,
    /// The newest head `seq` the device may have signed in it. A head by
    /// that device with a higher `seq` is refused everywhere.
    pub seq: u64,
}

/// A revoked device and where its writes stop.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Revocation {
    /// The device.
    pub device_id: Id,
    /// Per vault the account held when the device was revoked. A vault not
    /// listed has boundary zero: nothing by that device is taken in it.
    pub vaults: Vec<VaultBoundary>,
}

impl Revocation {
    /// The boundary for a vault: zero when the vault isn't listed.
    pub fn boundary(&self, vault_id: Id) -> u64 {
        self.vaults
            .iter()
            .find(|b| b.vault_id == vault_id)
            .map(|b| b.seq)
            .unwrap_or(0)
    }
}

/// The signed, hash-chained list of an identity's records and revocations.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct IdentityManifest {
    /// Format version.
    pub v: u16,
    /// The profile.
    pub profile_id: Id,
    /// Rises by one on every manifest; the first is 1.
    pub seq: u64,
    /// BLAKE3 of the manifest before; zero for the first.
    pub prev_hash: Hash32,
    /// Every record, sorted and unique by path.
    pub entries: Vec<RecordRef>,
    /// Every revoked device, sorted and unique by id. Only ever grows, and
    /// a boundary once written never changes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub revocations: Vec<Revocation>,
    /// The device that wrote it.
    pub written_by: Id,
    /// When.
    pub written_at: Timestamp,
    /// Ed25519 by the Identity Signing Key over the canonical manifest
    /// with `sig` empty.
    #[serde(with = "serde_bytes")]
    pub sig: Vec<u8>,
}

impl IdentityManifest {
    /// The bytes the signature covers.
    pub fn signable(&self) -> Result<Vec<u8>> {
        let mut copy = self.clone();
        copy.sig.clear();
        Ok(to_canonical(&copy)?)
    }

    /// Signs with the Identity Signing Key.
    pub fn sign(&mut self, isk: &SigningKey) -> Result<()> {
        self.sig = isk.sign(&self.signable()?).to_vec();
        Ok(())
    }

    /// Checks the signature.
    pub fn verify_signature(&self, isk_pub: &VerifyingKey) -> Result<()> {
        isk_pub
            .verify(
                &self.signable()?,
                &self.sig,
                &format!("identity manifest {}", self.seq),
            )
            .map_err(|_| Error::ChainBroken {
                seq: self.seq,
                why: "the identity manifest's signature didn't check out".into(),
            })
    }

    /// BLAKE3 of the signed manifest: what the next names as `prev_hash`.
    pub fn hash(&self) -> Result<Hash32> {
        Ok(hash(&self.to_bytes()?))
    }

    /// As canonical CBOR.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        Ok(to_canonical(self)?)
    }

    /// From CBOR.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        Ok(lockdown_crypto::cbor::from_slice(bytes)?)
    }

    /// The revocation of a device, if it is revoked.
    pub fn revocation(&self, device_id: Id) -> Option<&Revocation> {
        self.revocations.iter().find(|r| r.device_id == device_id)
    }

    /// Checks the shape: version, sorted unique entries and revocations.
    pub fn check_shape(&self) -> Result<()> {
        let broken = |why: &str| Error::ChainBroken {
            seq: self.seq,
            why: why.into(),
        };
        if self.v > IDENTITY_MANIFEST_V {
            return Err(broken("newer identity manifest format than this app reads"));
        }
        if self.seq == 0 {
            return Err(broken("an identity manifest's seq starts at 1"));
        }
        let paths: Vec<&str> = self.entries.iter().map(|e| e.path.as_str()).collect();
        let mut sorted = paths.clone();
        sorted.dedup();
        if sorted.len() != paths.len() || !paths.is_sorted() {
            return Err(broken("entries aren't sorted and unique by path"));
        }
        let devices: Vec<Id> = self.revocations.iter().map(|r| r.device_id).collect();
        let mut sorted = devices.clone();
        sorted.dedup();
        if sorted.len() != devices.len() || !devices.is_sorted() {
            return Err(broken("revocations aren't sorted and unique by device"));
        }
        for r in &self.revocations {
            let vaults: Vec<Id> = r.vaults.iter().map(|b| b.vault_id).collect();
            let mut sorted = vaults.clone();
            sorted.dedup();
            if sorted.len() != vaults.len() || !vaults.is_sorted() {
                return Err(broken("a revocation's vaults aren't sorted and unique"));
            }
        }
        if !self.entries.iter().any(|e| e.path == PROFILE_PATH) {
            return Err(broken("the manifest doesn't list the profile"));
        }
        Ok(())
    }

    /// Whether this manifest is the next page after `prev` (`None` for the
    /// first): `seq` one higher and `prev_hash` naming it.
    pub fn continues(&self, prev: Option<(u64, Hash32)>) -> Result<()> {
        let broken = |why: String| Error::ChainBroken { seq: self.seq, why };
        match prev {
            None => {
                if self.seq != 1 {
                    return Err(broken(format!(
                        "the first identity manifest must be seq 1, not {}",
                        self.seq
                    )));
                }
                if self.prev_hash != Hash32::ZERO {
                    return Err(broken(
                        "the first identity manifest names a predecessor".into(),
                    ));
                }
            }
            Some((seq, hash)) => {
                if self.seq != seq + 1 {
                    return Err(broken(format!(
                        "identity manifest {} doesn't follow {seq}",
                        self.seq
                    )));
                }
                if self.prev_hash != hash {
                    return Err(broken(
                        "prev_hash doesn't match the identity manifest before it".into(),
                    ));
                }
            }
        }
        Ok(())
    }

    /// Revocation only ever grows (D73): every device `prev` revoked is
    /// still revoked here with the same boundaries for the vaults `prev`
    /// named. New vaults may be added; nothing is taken back.
    pub fn revocations_extend(&self, prev: &IdentityManifest) -> Result<()> {
        for old in &prev.revocations {
            let Some(new) = self.revocation(old.device_id) else {
                return Err(Error::Invalid(format!(
                    "device {} was revoked and a revocation can't be undone",
                    old.device_id
                )));
            };
            for b in &old.vaults {
                if new.boundary(b.vault_id) != b.seq {
                    return Err(Error::Invalid(format!(
                        "the revocation of device {} already stops at head {} in vault {}; that can't change",
                        old.device_id, b.seq, b.vault_id
                    )));
                }
            }
        }
        Ok(())
    }
}

/// A manifest with the bytes of every record it lists.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RecordSet {
    /// The manifest.
    pub manifest: IdentityManifest,
    /// The records by path, as the ISK signed them.
    pub records: BTreeMap<String, Vec<u8>>,
}

/// A record set, decoded and verified.
#[derive(Clone, Debug)]
pub struct Verified {
    /// The manifest.
    pub manifest: IdentityManifest,
    /// The profile.
    pub profile: Profile,
    /// Personas by id.
    pub personas: BTreeMap<Id, Persona>,
    /// Devices by id.
    pub devices: BTreeMap<Id, Device>,
    /// Envelopes by file name.
    pub envelopes: BTreeMap<String, KeyEnvelope>,
    /// Consents by file name.
    pub consents: BTreeMap<String, Consent>,
}

impl Verified {
    /// The public key of every device, by id, revoked ones included
    /// (history they signed before revocation still verifies).
    pub fn device_keys(&self) -> BTreeMap<Id, VerifyingKey> {
        self.devices
            .values()
            .filter_map(|d| {
                VerifyingKey::from_bytes(&d.device_pub)
                    .ok()
                    .map(|k| (d.device_id, k))
            })
            .collect()
    }

    /// For each revoked device, its boundary in `vault_id`.
    pub fn revoked_after(&self, vault_id: Id) -> BTreeMap<Id, u64> {
        self.manifest
            .revocations
            .iter()
            .map(|r| (r.device_id, r.boundary(vault_id)))
            .collect()
    }

    /// Whether a device is revoked.
    pub fn is_revoked(&self, device_id: Id) -> bool {
        self.manifest.revocation(device_id).is_some()
    }

    /// The set as the identity crate's `Backup`, for `recover_with_kit`
    /// and `recover_with_account` on an empty device.
    pub fn to_backup(&self) -> Backup {
        Backup {
            format: FORMAT.to_string(),
            profile: self.profile.clone(),
            personas: self.personas.values().cloned().collect(),
            devices: self.devices.values().cloned().collect(),
            envelopes: self.envelopes.values().cloned().collect(),
            consents: self.consents.values().cloned().collect(),
        }
    }
}

fn record_path(dir: &str, name: &str) -> String {
    format!("{dir}/{name}")
}

impl RecordSet {
    /// Builds the set an open identity would publish: every record of the
    /// identity but the device wraps, under a manifest that follows `prev`
    /// (or starts a chain) and carries `revocations`. Unsigned until
    /// [`Self::sign`].
    #[allow(clippy::too_many_arguments)]
    pub fn from_records(
        profile: &Profile,
        personas: impl IntoIterator<Item = Persona>,
        devices: impl IntoIterator<Item = Device>,
        envelopes: impl IntoIterator<Item = KeyEnvelope>,
        consents: impl IntoIterator<Item = Consent>,
        prev: Option<&IdentityManifest>,
        mut revocations: Vec<Revocation>,
        written_by: Id,
    ) -> Result<Self> {
        let mut records = BTreeMap::new();
        records.insert(PROFILE_PATH.to_string(), profile.to_bytes()?);
        for p in personas {
            records.insert(
                record_path("personas", &format!("{}.cbor", p.persona_id)),
                p.to_bytes()?,
            );
        }
        for d in devices {
            records.insert(
                record_path("devices", &format!("{}.cbor", d.device_id)),
                d.to_bytes()?,
            );
        }
        for e in envelopes {
            if e.wrapped_by.kind == KeyRefKind::Device {
                continue;
            }
            records.insert(record_path("envelopes", &e.file_name()), e.to_bytes()?);
        }
        for c in consents {
            records.insert(record_path("consents", &c.file_name()), c.to_bytes()?);
        }
        let entries = records
            .iter()
            .map(|(path, bytes)| RecordRef {
                path: path.clone(),
                hash: hash(bytes),
            })
            .collect();
        revocations.sort_by_key(|r| r.device_id);
        for r in &mut revocations {
            r.vaults.sort_by_key(|b| b.vault_id);
        }
        let (seq, prev_hash) = match prev {
            Some(p) => (p.seq + 1, p.hash()?),
            None => (1, Hash32::ZERO),
        };
        let manifest = IdentityManifest {
            v: IDENTITY_MANIFEST_V,
            profile_id: profile.profile_id,
            seq,
            prev_hash,
            entries,
            revocations,
            written_by,
            written_at: Timestamp::now(),
            sig: Vec::new(),
        };
        Ok(Self { manifest, records })
    }

    /// Signs the manifest.
    pub fn sign(&mut self, isk: &SigningKey) -> Result<()> {
        self.manifest.sign(isk)
    }

    /// Whether this set lists the same records (by hash) and the same
    /// revocations as `other`: nothing to publish if so.
    pub fn same_contents(&self, other: &IdentityManifest) -> bool {
        self.manifest.entries == other.entries && self.manifest.revocations == other.revocations
    }

    /// Decodes and checks everything: the manifest's shape and signature,
    /// every listed record's hash, decoding, name and signature under the
    /// profile's Identity Signing Key, and that the profile's key is
    /// `expected_isk` when the caller already knows it (a device that holds
    /// the identity; a brand-new device anchors on the kit or the password
    /// instead). Revocations must name devices in the set whose record says
    /// revoked, and every such device must be named.
    pub fn verify(&self, expected_isk: Option<&[u8; 32]>) -> Result<Verified> {
        let manifest = &self.manifest;
        manifest.check_shape()?;
        let broken = |why: String| Error::ChainBroken {
            seq: manifest.seq,
            why,
        };
        let listed: BTreeSet<&str> = manifest.entries.iter().map(|e| e.path.as_str()).collect();
        for path in self.records.keys() {
            if !listed.contains(path.as_str()) {
                return Err(broken(format!("{path} isn't in the identity manifest")));
            }
        }
        let bytes_of = |path: &str| -> Result<&Vec<u8>> {
            self.records
                .get(path)
                .ok_or_else(|| broken(format!("{path} is listed but missing")))
        };
        for entry in &manifest.entries {
            if hash(bytes_of(&entry.path)?) != entry.hash {
                return Err(broken(format!("{} doesn't match its hash", entry.path)));
            }
        }
        let profile = Profile::from_bytes(bytes_of(PROFILE_PATH)?)?;
        if let Some(expected) = expected_isk
            && profile.isk_pub != *expected
        {
            return Err(broken(
                "the profile's signing key isn't this identity's".into(),
            ));
        }
        let isk = VerifyingKey::from_bytes(&profile.isk_pub)?;
        let isk_dalek =
            ecosystem_identity::ed25519_dalek::VerifyingKey::from_bytes(&profile.isk_pub)
                .map_err(|_| broken("the profile's signing key is malformed".into()))?;
        profile.verify(&isk_dalek)?;
        manifest.verify_signature(&isk)?;
        if manifest.profile_id != profile.profile_id {
            return Err(broken(
                "the identity manifest names another profile than its profile record".into(),
            ));
        }
        profile.kdf.check_floor()?;

        let mut out = Verified {
            manifest: manifest.clone(),
            profile,
            personas: BTreeMap::new(),
            devices: BTreeMap::new(),
            envelopes: BTreeMap::new(),
            consents: BTreeMap::new(),
        };
        for entry in &manifest.entries {
            let bytes = bytes_of(&entry.path)?;
            let Some((dir, name)) = entry.path.split_once('/') else {
                if entry.path == PROFILE_PATH {
                    continue;
                }
                return Err(broken(format!("{} isn't a place records live", entry.path)));
            };
            let misnamed = || broken(format!("{} doesn't hold the record it names", entry.path));
            match dir {
                "personas" => {
                    let r = Persona::from_bytes(bytes)?;
                    r.verify(&isk_dalek)?;
                    if name != format!("{}.cbor", r.persona_id) {
                        return Err(misnamed());
                    }
                    out.personas.insert(r.persona_id, r);
                }
                "devices" => {
                    let r = Device::from_bytes(bytes)?;
                    r.verify(&isk_dalek)?;
                    if name != format!("{}.cbor", r.device_id) {
                        return Err(misnamed());
                    }
                    out.devices.insert(r.device_id, r);
                }
                "envelopes" => {
                    let r = KeyEnvelope::from_bytes(bytes)?;
                    r.verify(&isk_dalek)?;
                    if name != r.file_name() {
                        return Err(misnamed());
                    }
                    if r.wrapped_by.kind == KeyRefKind::Device {
                        return Err(broken(format!(
                            "{} is a device wrap; those never leave their device",
                            entry.path
                        )));
                    }
                    out.envelopes.insert(r.file_name(), r);
                }
                "consents" => {
                    let r = Consent::from_bytes(bytes)?;
                    r.verify(&isk_dalek)?;
                    if name != r.file_name() {
                        return Err(misnamed());
                    }
                    out.consents.insert(r.file_name(), r);
                }
                _ => {
                    return Err(broken(format!("{} isn't a place records live", entry.path)));
                }
            }
        }
        for r in &manifest.revocations {
            match out.devices.get(&r.device_id) {
                Some(d) if d.revoked_at.is_some() => {}
                Some(_) => {
                    return Err(broken(format!(
                        "device {} is listed as revoked but its record says it isn't",
                        r.device_id
                    )));
                }
                None => {
                    return Err(broken(format!(
                        "device {} is listed as revoked but has no record",
                        r.device_id
                    )));
                }
            }
        }
        for d in out.devices.values() {
            if d.revoked_at.is_some() && manifest.revocation(d.device_id).is_none() {
                return Err(broken(format!(
                    "device {} is revoked but the manifest doesn't say where its writes stop",
                    d.device_id
                )));
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ecosystem_identity::{FileKeystore, Identity};

    fn make_identity(dir: &std::path::Path, name: &str) -> (Identity, FileKeystore) {
        let keystore = FileKeystore::open_or_create(&dir.join(format!("{name}.cbor"))).unwrap();
        let identity = Identity::create(&dir.join(name), "lockdown", name, &keystore).unwrap();
        (identity, keystore)
    }

    fn set_of(identity: &Identity, prev: Option<&IdentityManifest>) -> RecordSet {
        RecordSet::from_records(
            identity.profile(),
            identity.personas().cloned(),
            identity.devices().cloned(),
            identity.envelopes().cloned(),
            identity.consents().cloned(),
            prev,
            Vec::new(),
            identity.device_id(),
        )
        .unwrap()
    }

    #[test]
    fn a_set_round_trips_verifies_and_refuses_tampering() {
        let dir = tempfile::tempdir().unwrap();
        let (identity, keystore) = make_identity(dir.path(), "a");
        let isk = crate::identity::identity_signing_key(&identity, &keystore).unwrap();
        let mut set = set_of(&identity, None);
        assert!(set.verify(None).is_err(), "unsigned manifests don't verify");
        set.sign(&isk).unwrap();
        let verified = set.verify(Some(&identity.profile().isk_pub)).unwrap();
        assert_eq!(verified.devices.len(), 1);
        assert_eq!(verified.personas.len(), 1);
        assert!(
            verified
                .envelopes
                .values()
                .all(|e| e.wrapped_by.kind != KeyRefKind::Device),
            "device wraps stay home"
        );
        assert!(set.same_contents(&verified.manifest));
        let backup = verified.to_backup();
        assert_eq!(backup.profile.profile_id, identity.profile().profile_id);

        // Under another identity's key: refused.
        let (other, _) = make_identity(dir.path(), "b");
        assert!(set.verify(Some(&other.profile().isk_pub)).is_err());

        // A changed record fails its hash; a record swapped with another
        // signed one fails its name; an unlisted record is refused.
        let mut changed = set.clone();
        let path = changed
            .records
            .keys()
            .find(|p| p.starts_with("devices/"))
            .cloned()
            .unwrap();
        changed.records.get_mut(&path).unwrap()[0] ^= 1;
        assert!(changed.verify(None).is_err());
        let mut extra = set.clone();
        extra.records.insert("devices/x.cbor".into(), vec![1]);
        assert!(extra.verify(None).is_err());

        // The chain.
        let next = {
            let mut n = set_of(&identity, Some(&set.manifest));
            n.sign(&isk).unwrap();
            n
        };
        assert_eq!(next.manifest.seq, 2);
        next.manifest
            .continues(Some((1, set.manifest.hash().unwrap())))
            .unwrap();
        assert!(next.manifest.continues(None).is_err());
        assert!(next.manifest.continues(Some((1, Hash32([9; 32])))).is_err());
        set.manifest.continues(None).unwrap();
        let bytes = next.manifest.to_bytes().unwrap();
        assert_eq!(IdentityManifest::from_bytes(&bytes).unwrap(), next.manifest);
    }

    #[test]
    fn revocations_are_consistent_and_only_grow() {
        let dir = tempfile::tempdir().unwrap();
        let (identity, keystore) = make_identity(dir.path(), "a");
        let isk = crate::identity::identity_signing_key(&identity, &keystore).unwrap();
        let vault = Id::new();
        let device = identity.device_id();
        // A revocation naming a device whose record isn't revoked: refused.
        let mut set = RecordSet::from_records(
            identity.profile(),
            identity.personas().cloned(),
            identity.devices().cloned(),
            identity.envelopes().cloned(),
            identity.consents().cloned(),
            None,
            vec![Revocation {
                device_id: device,
                vaults: vec![VaultBoundary {
                    vault_id: vault,
                    seq: 3,
                }],
            }],
            device,
        )
        .unwrap();
        set.sign(&isk).unwrap();
        let err = set.verify(None).unwrap_err();
        assert!(err.to_string().contains("record says it isn't"), "{err}");

        // A revoked record without a revocation: refused.
        let mut revoked = identity.devices().next().unwrap().clone();
        revoked.revoked_at = Some(Timestamp::now());
        revoked.sign(isk.inner()).unwrap();
        let mut set = RecordSet::from_records(
            identity.profile(),
            identity.personas().cloned(),
            [revoked.clone()],
            identity.envelopes().cloned(),
            identity.consents().cloned(),
            None,
            Vec::new(),
            device,
        )
        .unwrap();
        set.sign(&isk).unwrap();
        let err = set.verify(None).unwrap_err();
        assert!(err.to_string().contains("where its writes stop"), "{err}");

        // Both present: fine, and the boundary reads back.
        let revocation = Revocation {
            device_id: device,
            vaults: vec![VaultBoundary {
                vault_id: vault,
                seq: 3,
            }],
        };
        let mut set = RecordSet::from_records(
            identity.profile(),
            identity.personas().cloned(),
            [revoked],
            identity.envelopes().cloned(),
            identity.consents().cloned(),
            None,
            vec![revocation.clone()],
            device,
        )
        .unwrap();
        set.sign(&isk).unwrap();
        let verified = set.verify(None).unwrap();
        assert!(verified.is_revoked(device));
        assert_eq!(verified.revoked_after(vault)[&device], 3);
        assert_eq!(verified.revoked_after(Id::new())[&device], 0);

        // Growth: the same or more passes; undoing or moving is refused.
        let mut next = set.manifest.clone();
        next.seq += 1;
        next.revocations_extend(&set.manifest).unwrap();
        next.revocations[0].vaults.push(VaultBoundary {
            vault_id: Id::new(),
            seq: 1,
        });
        next.revocations_extend(&set.manifest).unwrap();
        next.revocations[0].vaults[0].seq = 4;
        assert!(next.revocations_extend(&set.manifest).is_err());
        next.revocations.clear();
        assert!(next.revocations_extend(&set.manifest).is_err());
    }
}
