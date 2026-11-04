//! Other people, as Lockdown knows them before it shares anything
//! (04-security §6 "before wrapping any key to a peer", §8 "trusted
//! contacts"; `docs/plan/18-sharing.md` §3).
//!
//! A [`Contact`] is built only from another person's **public identity**:
//! their signed identity manifest and the profile, persona and device
//! records it lists, each signed by their Identity Signing Key. The server
//! hands these over; it cannot forge them, and the caller checks that the
//! manifest is the one the server's key-transparency log recorded for that
//! account (`lockdown_sync::sharing::verify_directory_entry`), so a key
//! the server made up for someone would show up in their own app's audit
//! as an identity manifest they never wrote.
//!
//! Once seen, a contact is **pinned**: a later lookup that names another
//! Identity Signing Key or another receiving key is refused until the
//! person compares fingerprints again ([`Contact::check_continues`]). The
//! fingerprint is BLAKE3 over the profile, its signing key, the persona
//! and its receiving key, shown as six groups of five digits to read
//! aloud and as a QR payload to scan later ([`Fingerprint`]).

use std::collections::{BTreeMap, BTreeSet};

use ecosystem_identity::Record;
use ecosystem_identity::kem::KemPublicKey;
use ecosystem_identity::records::{Device, Persona, PersonaKind, Profile};
use lockdown_crypto::VerifyingKey;
use lockdown_crypto::cbor::to_canonical;
use lockdown_crypto::hash::{derive_key, hash};
use serde::{Deserialize, Serialize};

use crate::identity_set::{IdentityManifest, PROFILE_PATH};
use crate::sharing::Party;
use crate::{Error, Hash32, Id, Result, Timestamp};

/// The contact record's format version.
pub const CONTACT_V: u16 = 1;

/// The derive-key context of a fingerprint.
const FINGERPRINT_CONTEXT: &str = "lockdown 2026-10-18 contact fingerprint";

/// Another person's public identity as the server handed it over: the
/// identity manifest and the records a share needs (profile, personas,
/// devices; never a key envelope).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PublicIdentity {
    /// The server's account id for them.
    pub account_id: Id,
    /// Their login name on that server.
    pub login: String,
    /// The identity manifest, as their ISK signed it.
    pub manifest: IdentityManifest,
    /// Records by path, as their ISK signed them.
    pub records: BTreeMap<String, Vec<u8>>,
}

/// What [`PublicIdentity::verify`] found.
#[derive(Clone, Debug)]
pub struct VerifiedPublic {
    /// The profile.
    pub profile: Profile,
    /// Every persona.
    pub personas: Vec<Persona>,
    /// Every device, revoked ones included.
    pub devices: Vec<Device>,
}

impl PublicIdentity {
    /// Checks the manifest's shape and signature under the profile's
    /// signing key, that every record handed over is listed with the hash
    /// it carries and signed by that key, and that **every** persona and
    /// device the manifest lists was handed over (so a server can't hide a
    /// device, or a revocation). `expected_isk` is the key pinned earlier.
    pub fn verify(&self, expected_isk: Option<&[u8; 32]>) -> Result<VerifiedPublic> {
        let manifest = &self.manifest;
        manifest.check_shape()?;
        let broken = |why: String| Error::ChainBroken {
            seq: manifest.seq,
            why,
        };
        let listed: BTreeMap<&str, Hash32> = manifest
            .entries
            .iter()
            .map(|e| (e.path.as_str(), e.hash))
            .collect();
        for (path, bytes) in &self.records {
            let Some(want) = listed.get(path.as_str()) else {
                return Err(broken(format!("{path} isn't in their identity manifest")));
            };
            if hash(bytes) != *want {
                return Err(broken(format!("{path} doesn't match its hash")));
            }
        }
        for path in listed.keys() {
            let needed = *path == PROFILE_PATH
                || path.starts_with("personas/")
                || path.starts_with("devices/");
            if needed && !self.records.contains_key(*path) {
                return Err(broken(format!(
                    "their identity lists {path} but the server didn't hand it over"
                )));
            }
        }
        let profile = Profile::from_bytes(&self.records[PROFILE_PATH])?;
        if let Some(expected) = expected_isk
            && profile.isk_pub != *expected
        {
            return Err(Error::Transparency(
                "this person's identity signing key changed since you last checked it; compare fingerprints again before sharing".into(),
            ));
        }
        let isk = VerifyingKey::from_bytes(&profile.isk_pub)?;
        let isk_dalek =
            ecosystem_identity::ed25519_dalek::VerifyingKey::from_bytes(&profile.isk_pub)
                .map_err(|_| broken("their profile's signing key is malformed".into()))?;
        profile.verify(&isk_dalek)?;
        manifest.verify_signature(&isk)?;
        crate::pq::verify_manifest(&profile, manifest)?;
        if manifest.profile_id != profile.profile_id {
            return Err(broken(
                "their identity manifest names another profile than their profile record".into(),
            ));
        }
        let mut personas = Vec::new();
        let mut devices = Vec::new();
        for (path, bytes) in &self.records {
            let Some((dir, name)) = path.split_once('/') else {
                continue;
            };
            match dir {
                "personas" => {
                    let r = Persona::from_bytes(bytes)?;
                    r.verify(&isk_dalek)?;
                    if name != format!("{}.cbor", r.persona_id) {
                        return Err(broken(format!("{path} doesn't hold the record it names")));
                    }
                    personas.push(r);
                }
                "devices" => {
                    let r = Device::from_bytes(bytes)?;
                    r.verify(&isk_dalek)?;
                    if name != format!("{}.cbor", r.device_id) {
                        return Err(broken(format!("{path} doesn't hold the record it names")));
                    }
                    devices.push(r);
                }
                _ => {
                    return Err(broken(format!(
                        "{path} isn't a record a share needs; it wasn't asked for"
                    )));
                }
            }
        }
        if personas.is_empty() {
            return Err(broken("their identity has no persona".into()));
        }
        personas.sort_by_key(|p| p.persona_id);
        devices.sort_by_key(|d| d.device_id);
        Ok(VerifiedPublic {
            profile,
            personas,
            devices,
        })
    }
}

/// The persona a share goes to when the caller names none: the only one;
/// else the first `personal` one; else the first by id (the rule D45 uses
/// on one's own device, from the public records alone).
pub fn receiving_persona(personas: &[Persona]) -> Option<&Persona> {
    if personas.len() == 1 {
        return personas.first();
    }
    personas
        .iter()
        .filter(|p| p.kind == PersonaKind::Personal)
        .min_by_key(|p| p.persona_id)
        .or_else(|| personas.iter().min_by_key(|p| p.persona_id))
}

/// One of a contact's devices.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ContactDevice {
    /// Its Ed25519 public key.
    #[serde(with = "serde_bytes")]
    pub device_pub: [u8; 32],
    /// What they call it.
    pub name: String,
    /// Whether their identity revoked it.
    pub revoked: bool,
    /// The post-quantum support level its record marks (`crate::pq`'s
    /// `lockdown.pq`), when it marks one. Absent for a contact pinned
    /// before this field, and for a device from a build before the mark.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pq_level: Option<u64>,
}

/// Another person, pinned on this device.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Contact {
    /// Format version.
    pub v: u16,
    /// The server's account id.
    pub account_id: Id,
    /// Their login name.
    pub login: String,
    /// Their profile.
    pub profile_id: Id,
    /// Their Identity Signing Key, pinned on first sight.
    #[serde(with = "serde_bytes")]
    pub isk_pub: [u8; 32],
    /// The persona shares go to.
    pub persona_id: Id,
    /// What that persona is called.
    pub persona_label: String,
    /// That persona's receiving key, pinned on first sight.
    pub kem_pub: KemPublicKey,
    /// Their devices, by id.
    pub devices: BTreeMap<Id, ContactDevice>,
    /// The identity manifest these came from: `seq` and hash. Never moves back.
    pub identity_seq: u64,
    /// Its hash.
    pub identity_hash: Hash32,
    /// The key-transparency log entry that recorded that manifest.
    pub log_index: u64,
    /// When this device first pinned them.
    pub first_seen: Timestamp,
    /// When the person confirmed the fingerprint matched, read aloud or
    /// scanned; `None` until then.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verified_at: Option<Timestamp>,
    /// Their identity's ML-DSA-65 public key, pinned when first seen
    /// (`crate::pq`; absent for an identity from before it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pq_isk: Option<serde_bytes::ByteBuf>,
    /// The keys this contact moved on from, oldest first: a hand-over the
    /// old keys signed, or the owner of a shared vault letting them back in
    /// after a reset (docs/plan/55-member-key-change.md). Their devices
    /// stay in `devices`, marked revoked, so what they signed before still
    /// verifies; a lookup that shows one of these keys again is refused.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub former: Vec<FormerKeys>,
}

/// How a pin moved on to new keys.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum KeyMove {
    /// A hand-over the old keys signed.
    HandedOver,
    /// The owner of a shared vault let them back in, naming the new keys
    /// in a membership change they signed.
    LetBackIn,
}

/// Keys a contact used before, kept so what they signed still verifies.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct FormerKeys {
    /// The profile.
    pub profile_id: Id,
    /// Its Identity Signing Key.
    #[serde(with = "serde_bytes")]
    pub isk_pub: [u8; 32],
    /// Its post-quantum key, when one was pinned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pq_isk: Option<serde_bytes::ByteBuf>,
    /// The persona shares went to.
    pub persona_id: Id,
    /// Its receiving key.
    pub kem_pub: KemPublicKey,
    /// The devices of that identity.
    pub devices: BTreeSet<Id>,
    /// The identity manifest pinned last: `seq` and hash.
    pub identity_seq: u64,
    /// Its hash.
    pub identity_hash: Hash32,
    /// When this device moved on from them.
    pub until: Timestamp,
    /// How.
    pub how: KeyMove,
}

impl FormerKeys {
    /// Their identity's ML-DSA-65 key, as pinned then.
    pub fn pq_key(&self) -> Result<Option<lockdown_crypto::pq_sign::PqVerifyingKey>> {
        Ok(match &self.pq_isk {
            Some(b) => Some(lockdown_crypto::pq_sign::PqVerifyingKey::from_bytes(b)?),
            None => None,
        })
    }
}

/// What a fresh lookup of a pinned person comes to
/// ([`Contact::take_lookup`]).
#[derive(Clone, Debug)]
pub enum Lookup {
    /// The same keys: the pin goes on (devices and the manifest move forward).
    Continues(Contact),
    /// New keys something this device trusts vouched for: the pin moved.
    Moved(Contact, KeyMove),
    /// New keys nothing vouches for. The pin stays; the fresh lookup is
    /// only ever named, never trusted.
    Changed(Contact),
}

/// A fingerprint, two ways.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Fingerprint {
    /// Six groups of five digits, the same on both people's screens.
    pub digits: String,
    /// What a QR code would carry: `lockdown:contact:v1:<account>:<hex>`.
    pub qr: String,
}

#[derive(Serialize)]
struct FingerprintInput<'a> {
    profile_id: Id,
    #[serde(with = "serde_bytes")]
    isk_pub: &'a [u8; 32],
    persona_id: Id,
    kem_pub: &'a KemPublicKey,
}

/// The 32-byte fingerprint of a profile's signing key and a persona's
/// receiving key.
pub fn fingerprint_bytes(
    profile_id: Id,
    isk_pub: &[u8; 32],
    persona_id: Id,
    kem_pub: &KemPublicKey,
) -> Result<[u8; 32]> {
    let input = to_canonical(&FingerprintInput {
        profile_id,
        isk_pub,
        persona_id,
        kem_pub,
    })?;
    Ok(derive_key(FINGERPRINT_CONTEXT, &input))
}

/// Six groups of five digits from the first thirty bytes: each group is
/// five bytes read as a number, modulo 100 000 (the way Signal's safety
/// numbers are made).
pub fn digits(bytes: &[u8; 32]) -> String {
    bytes[..30]
        .chunks(5)
        .map(|c| {
            let n = c.iter().fold(0u64, |acc, b| (acc << 8) | u64::from(*b));
            format!("{:05}", n % 100_000)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

impl Contact {
    /// Builds a contact from a verified public identity, for the persona
    /// `persona` names (or the receiving one by the default rule).
    pub fn from_public(
        public: &PublicIdentity,
        verified: &VerifiedPublic,
        persona: Option<Id>,
        log_index: u64,
    ) -> Result<Self> {
        let persona = match persona {
            Some(id) => verified
                .personas
                .iter()
                .find(|p| p.persona_id == id)
                .ok_or_else(|| Error::NotFound(format!("their persona {id}")))?,
            None => receiving_persona(&verified.personas)
                .ok_or_else(|| Error::NotFound("a persona to share with".into()))?,
        };
        let revoked: BTreeSet<Id> = public
            .manifest
            .revocations
            .iter()
            .map(|r| r.device_id)
            .collect();
        Ok(Self {
            v: CONTACT_V,
            account_id: public.account_id,
            login: public.login.clone(),
            profile_id: verified.profile.profile_id,
            isk_pub: verified.profile.isk_pub,
            persona_id: persona.persona_id,
            persona_label: persona.label.clone(),
            kem_pub: persona.kem_pub.clone(),
            devices: verified
                .devices
                .iter()
                .map(|d| {
                    (
                        d.device_id,
                        ContactDevice {
                            device_pub: d.device_pub,
                            name: d.name.clone(),
                            revoked: d.revoked_at.is_some() || revoked.contains(&d.device_id),
                            pq_level: crate::pq::level_of(d),
                        },
                    )
                })
                .collect(),
            identity_seq: public.manifest.seq,
            identity_hash: public.manifest.hash()?,
            log_index,
            first_seen: Timestamp::now(),
            verified_at: None,
            pq_isk: crate::pq::published_key(&verified.profile)?
                .map(|k| serde_bytes::ByteBuf::from(k.as_bytes().to_vec())),
            former: Vec::new(),
        })
    }

    /// The keys pinned now.
    pub fn pinned_keys(&self) -> crate::key_change::PinnedKeys {
        crate::key_change::PinnedKeys {
            profile_id: self.profile_id,
            isk_pub: self.isk_pub,
            pq_isk: self.pq_isk.clone(),
            persona_id: self.persona_id,
            kem_pub: self.kem_pub.clone(),
        }
    }

    /// The keys a membership change names for them (`Change::Add.keys`).
    pub fn member_keys(&self) -> crate::sharing::MemberKeys {
        crate::sharing::MemberKeys {
            isk_pub: self.isk_pub,
            pq_isk: self.pq_isk.clone(),
            kem_pub: self.kem_pub.clone(),
        }
    }

    /// Whether `party` and `keys` name exactly these keys, this account.
    pub fn is_named_by(&self, party: &Party, keys: &crate::sharing::MemberKeys) -> bool {
        *party == self.party()
            && keys.isk_pub == self.isk_pub
            && keys.kem_pub == self.kem_pub
            && keys.pq_isk == self.pq_isk
    }

    /// The former identity a device of theirs belonged to, if it is one.
    pub fn former_of(&self, device_id: Id) -> Option<&FormerKeys> {
        self.former
            .iter()
            .rev()
            .find(|f| f.devices.contains(&device_id))
    }

    /// Whether `fresh` shows keys this contact already moved on from.
    fn shows_former(&self, fresh: &Contact) -> bool {
        self.former.iter().any(|f| {
            f.isk_pub == fresh.isk_pub
                || f.kem_pub == fresh.kem_pub
                || (f.profile_id == fresh.profile_id && f.isk_pub == fresh.isk_pub)
        })
    }

    /// Takes a fresh lookup of the same account. The same keys continue the
    /// pin ([`Self::check_continues`]). Other keys move it only when a chain
    /// of hand-overs the fresh identity publishes walks there from the pin
    /// (`key_change::walk`), or when `vouched` says a membership change the
    /// owner of a shared vault signed names exactly them. Anything else is
    /// [`Lookup::Changed`]: the pin stays. Keys it moved on from before, an
    /// older identity of the same profile, another account: refused.
    pub fn take_lookup(
        &self,
        fresh: &Contact,
        handovers: &[crate::key_change::KeyHandover],
        vouched: &dyn Fn(&Contact) -> bool,
    ) -> Result<Lookup> {
        if fresh.account_id != self.account_id {
            return Err(Error::Transparency(format!(
                "the server answered for {} with another account's identity",
                self.login
            )));
        }
        let pinned = self.pinned_keys();
        let shown = fresh.pinned_keys();
        if pinned.same_as(&shown) {
            return Ok(Lookup::Continues(self.check_continues(fresh)?));
        }
        if self.shows_former(fresh) {
            return Err(Error::Transparency(format!(
                "the server showed keys for {} that this device already moved on from; refused",
                self.login
            )));
        }
        if fresh.profile_id == self.profile_id && fresh.identity_seq <= self.identity_seq {
            return Err(Error::Transparency(format!(
                "the server showed an older identity for {} (manifest {}) than this device already saw ({}); refused",
                self.login, fresh.identity_seq, self.identity_seq
            )));
        }
        if crate::key_change::walk(&pinned, handovers, &shown).is_some() {
            return Ok(Lookup::Moved(
                self.moved_to(fresh, KeyMove::HandedOver),
                KeyMove::HandedOver,
            ));
        }
        if vouched(fresh) {
            return Ok(Lookup::Moved(
                self.moved_to(fresh, KeyMove::LetBackIn),
                KeyMove::LetBackIn,
            ));
        }
        Ok(Lookup::Changed(fresh.clone()))
    }

    /// This contact moved on to `fresh`'s keys: the old ones are kept as
    /// former keys, their devices stay (revoked) so what they signed still
    /// verifies, and the fingerprint check carries over only along a
    /// hand-over the compared keys signed.
    pub fn moved_to(&self, fresh: &Contact, how: KeyMove) -> Contact {
        let mut out = fresh.clone();
        out.first_seen = self.first_seen;
        out.verified_at = match how {
            KeyMove::HandedOver => self.verified_at,
            KeyMove::LetBackIn => None,
        };
        out.former = self.former.clone();
        out.former.push(FormerKeys {
            profile_id: self.profile_id,
            isk_pub: self.isk_pub,
            pq_isk: self.pq_isk.clone(),
            persona_id: self.persona_id,
            kem_pub: self.kem_pub.clone(),
            devices: self
                .devices
                .keys()
                .filter(|d| !fresh.devices.contains_key(d))
                .copied()
                .collect(),
            identity_seq: self.identity_seq,
            identity_hash: self.identity_hash,
            until: Timestamp::now(),
            how,
        });
        for (id, d) in &self.devices {
            if fresh.devices.contains_key(id) {
                continue;
            }
            let mut d = d.clone();
            d.revoked = true;
            out.devices.insert(*id, d);
        }
        out
    }

    /// Where shares to this contact go.
    pub fn party(&self) -> Party {
        Party {
            account_id: self.account_id,
            profile_id: self.profile_id,
            persona_id: self.persona_id,
        }
    }

    /// The fingerprint both people compare.
    pub fn fingerprint(&self) -> Result<Fingerprint> {
        let bytes = fingerprint_bytes(
            self.profile_id,
            &self.isk_pub,
            self.persona_id,
            &self.kem_pub,
        )?;
        Ok(Fingerprint {
            digits: digits(&bytes),
            qr: format!(
                "lockdown:contact:v1:{}:{}",
                self.account_id,
                Hash32(bytes).to_hex()
            ),
        })
    }

    /// The public key of a device of theirs that may sign now (revoked
    /// ones are still returned for history; the caller decides).
    pub fn device_key(&self, device_id: Id) -> Option<VerifyingKey> {
        self.devices
            .get(&device_id)
            .and_then(|d| VerifyingKey::from_bytes(&d.device_pub).ok())
    }

    /// Their identity's ML-DSA-65 key, as pinned; `None` while they
    /// published none when this device last looked them up.
    pub fn pq_key(&self) -> Result<Option<lockdown_crypto::pq_sign::PqVerifyingKey>> {
        Ok(match &self.pq_isk {
            Some(b) => Some(lockdown_crypto::pq_sign::PqVerifyingKey::from_bytes(b)?),
            None => None,
        })
    }

    /// Their devices not revoked that don't read post-quantum vault heads
    /// yet (below level 2, or no mark), as this device last saw them, in
    /// id order: who holds their side of a shared vault back.
    pub fn devices_without_head_support(&self) -> Vec<(Id, &ContactDevice)> {
        self.devices
            .iter()
            .filter(|(_, d)| !d.revoked && d.pq_level.is_none_or(|l| l < crate::pq::HEAD_LEVEL))
            .map(|(id, d)| (*id, d))
            .collect()
    }

    /// Whether their side of a shared vault is post-quantum as this device
    /// last saw them: they publish a key and every device not revoked
    /// reads post-quantum heads (doc 28's rollout rule, read from their
    /// public records).
    pub fn signs_heads_post_quantum(&self) -> bool {
        self.pq_isk.is_some() && self.devices_without_head_support().is_empty()
    }

    /// Takes a fresh lookup of the same person: it must keep the pinned
    /// signing key and receiving key, and its identity manifest must not be
    /// older than the one pinned. Devices and the manifest move forward;
    /// the in-person check carries over.
    pub fn check_continues(&self, newer: &Contact) -> Result<Contact> {
        if newer.account_id != self.account_id || newer.profile_id != self.profile_id {
            return Err(Error::Transparency(format!(
                "the server answered for {} with another person's identity",
                self.login
            )));
        }
        if newer.isk_pub != self.isk_pub || newer.kem_pub != self.kem_pub {
            return Err(Error::Transparency(format!(
                "{}'s keys changed since you pinned them; nothing is shared with the new keys until you compare fingerprints again",
                self.login
            )));
        }
        if self.pq_isk.is_some() && newer.pq_isk != self.pq_isk {
            return Err(Error::Transparency(format!(
                "{}'s post-quantum signing key changed or went missing since you pinned it; refused",
                self.login
            )));
        }
        if newer.identity_seq < self.identity_seq {
            return Err(Error::Transparency(format!(
                "the server showed an older identity for {} (manifest {}) than this device already saw ({}); refused",
                self.login, newer.identity_seq, self.identity_seq
            )));
        }
        if newer.identity_seq == self.identity_seq && newer.identity_hash != self.identity_hash {
            return Err(Error::Transparency(format!(
                "the server showed two different identities for {} at manifest {}; refused",
                self.login, self.identity_seq
            )));
        }
        let mut out = newer.clone();
        out.first_seen = self.first_seen;
        out.verified_at = self.verified_at;
        out.former = self.former.clone();
        // A device once revoked stays revoked, whatever a later answer says.
        for (id, d) in &self.devices {
            if d.revoked {
                out.devices
                    .entry(*id)
                    .and_modify(|n| n.revoked = true)
                    .or_insert_with(|| d.clone());
            } else {
                out.devices.entry(*id).or_insert_with(|| d.clone());
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity_set::RecordSet;
    use ecosystem_identity::{FileKeystore, Identity};

    fn public_of(identity: &Identity, keystore: &FileKeystore) -> PublicIdentity {
        let mut set = RecordSet::from_records(
            identity.profile(),
            identity.personas().cloned(),
            identity.devices().cloned(),
            identity.envelopes().cloned(),
            identity.consents().cloned(),
            None,
            Vec::new(),
            identity.device_id(),
        )
        .unwrap();
        let isk = crate::identity::identity_signing_key(identity, keystore).unwrap();
        set.sign(&isk).unwrap();
        let records = set
            .records
            .into_iter()
            .filter(|(p, _)| {
                p == PROFILE_PATH || p.starts_with("personas/") || p.starts_with("devices/")
            })
            .collect();
        PublicIdentity {
            account_id: Id::new(),
            login: "bob@example.com".into(),
            manifest: set.manifest,
            records,
        }
    }

    #[test]
    fn a_public_identity_verifies_pins_and_refuses_a_swap() {
        let dir = tempfile::tempdir().unwrap();
        let keystore = FileKeystore::open_or_create(&dir.path().join("k.cbor")).unwrap();
        let identity =
            Identity::create(&dir.path().join("bob"), "lockdown", "bob", &keystore).unwrap();
        let public = public_of(&identity, &keystore);
        let verified = public.verify(None).unwrap();
        let contact = Contact::from_public(&public, &verified, None, 3).unwrap();
        assert_eq!(contact.devices.len(), 1);
        let fp = contact.fingerprint().unwrap();
        assert_eq!(fp.digits.len(), 6 * 5 + 5);
        assert!(fp.digits.split(' ').all(|g| g.len() == 5));
        assert!(fp.qr.starts_with("lockdown:contact:v1:"));
        // The same lookup again continues the pin.
        let again = Contact::from_public(&public, &verified, None, 3).unwrap();
        contact.check_continues(&again).unwrap();

        // A device hidden by the server: refused.
        let mut hidden = public.clone();
        let device_path = hidden
            .records
            .keys()
            .find(|p| p.starts_with("devices/"))
            .unwrap()
            .clone();
        hidden.records.remove(&device_path);
        assert!(hidden.verify(None).is_err());
        // A record changed after signing: refused.
        let mut bent = public.clone();
        bent.records.get_mut(PROFILE_PATH).unwrap().push(0);
        assert!(bent.verify(None).is_err());
        // An envelope handed over: not asked for.
        let mut extra = public.clone();
        extra.records.insert("envelopes/x".into(), vec![1]);
        assert!(extra.verify(None).is_err());

        // Another person under the same account id: the pin refuses it.
        let keystore2 = FileKeystore::open_or_create(&dir.path().join("k2.cbor")).unwrap();
        let other =
            Identity::create(&dir.path().join("mallory"), "lockdown", "m", &keystore2).unwrap();
        let mut swapped = public_of(&other, &keystore2);
        swapped.account_id = public.account_id;
        assert!(swapped.verify(Some(&contact.isk_pub)).is_err());
        let swapped_v = swapped.verify(None).unwrap();
        let swapped_c = Contact::from_public(&swapped, &swapped_v, None, 9).unwrap();
        assert!(contact.check_continues(&swapped_c).is_err());
        assert_ne!(swapped_c.fingerprint().unwrap(), fp);
    }

    #[test]
    fn a_new_key_moves_the_pin_only_along_a_hand_over_or_a_vouch_and_never_back() {
        use crate::key_change::{HandoverKeys, KeyHandover, publish_handover};
        let dir = tempfile::tempdir().unwrap();
        let open = |name: &str| {
            let keystore =
                FileKeystore::open_or_create(&dir.path().join(format!("{name}.cbor"))).unwrap();
            let mut identity =
                Identity::create(&dir.path().join(name), "lockdown", name, &keystore).unwrap();
            crate::pq::roll_out(&mut identity, &keystore).unwrap();
            (identity, keystore)
        };
        let account = Id::new();
        // One identity's answer from the directory. Its manifest carries the
        // time it was written, so the same identity answered twice is two
        // manifests at seq 1 unless the very same answer is reused.
        let public_for = |identity: &Identity, keystore: &FileKeystore| {
            let mut public = public_of(identity, keystore);
            public.account_id = account;
            let pq = crate::pq::identity_pq_key(identity, keystore).unwrap();
            crate::pq::sign_manifest(&mut public.manifest, &pq).unwrap();
            public
        };
        let contact_from = |public: &PublicIdentity| {
            let verified = public.verify(None).unwrap();
            let c = Contact::from_public(public, &verified, None, 1).unwrap();
            let h = crate::key_change::handovers_of(&verified.profile).unwrap();
            (c, h)
        };
        let contact_of = |identity: &Identity, keystore: &FileKeystore| {
            contact_from(&public_for(identity, keystore))
        };
        let handover = |old: &(Identity, FileKeystore), new: &(Identity, FileKeystore)| {
            let persona = |i: &Identity| i.personas().next().unwrap().persona_id;
            let mut h = KeyHandover::new(
                HandoverKeys::of_identity(&old.0).unwrap(),
                HandoverKeys::of_identity(&new.0).unwrap(),
                BTreeMap::from([(persona(&old.0), persona(&new.0))]),
            );
            for (i, side) in [(old, true), (new, false)] {
                let isk = crate::identity::identity_signing_key(&i.0, &i.1).unwrap();
                let pq = crate::pq::identity_pq_key(&i.0, &i.1).unwrap();
                if side {
                    h.sign_from(&isk, Some(&pq)).unwrap();
                } else {
                    h.sign_to(&isk, Some(&pq)).unwrap();
                }
            }
            h
        };
        let never = |_: &Contact| false;
        let bob = open("bob");
        let bob_public = public_for(&bob.0, &bob.1);
        let (mut pinned, _) = contact_from(&bob_public);
        pinned.verified_at = Some(Timestamp::now());

        // The same keys continue the pin.
        let (same, none) = contact_from(&bob_public);
        assert!(matches!(
            pinned.take_lookup(&same, &none, &never).unwrap(),
            Lookup::Continues(_)
        ));

        // New keys with nothing to vouch for them: the pin stays, the
        // lookup is only named. A plain check refuses it as before.
        let bob2 = open("bob2");
        let (reset, none) = contact_of(&bob2.0, &bob2.1);
        assert!(matches!(
            pinned.take_lookup(&reset, &none, &never).unwrap(),
            Lookup::Changed(_)
        ));
        assert!(pinned.check_continues(&reset).is_err());
        // The owner's vouch names exactly these keys: moved, not verified.
        let named = |c: &Contact| c.is_named_by(&reset.party(), &reset.member_keys());
        let Lookup::Moved(back_in, KeyMove::LetBackIn) =
            pinned.take_lookup(&reset, &none, &named).unwrap()
        else {
            panic!("the vouch didn't move the pin")
        };
        assert!(back_in.verified_at.is_none());
        // A vouch for other keys doesn't.
        let mallory = open("mallory");
        let (fake, none) = contact_of(&mallory.0, &mallory.1);
        assert!(matches!(
            pinned.take_lookup(&fake, &none, &named).unwrap(),
            Lookup::Changed(_)
        ));

        // A hand-over the old keys signed, published on the new profile:
        // moved silently, the compared fingerprint carried along it.
        let mut bob2 = bob2;
        let to_bob2 = handover(&bob, &bob2);
        publish_handover(&mut bob2.0, &bob2.1, &to_bob2).unwrap();
        let (handed, list) = contact_of(&bob2.0, &bob2.1);
        let Lookup::Moved(moved, KeyMove::HandedOver) =
            pinned.take_lookup(&handed, &list, &never).unwrap()
        else {
            panic!("the hand-over didn't move the pin")
        };
        assert_eq!(moved.isk_pub, bob2.0.profile().isk_pub);
        assert!(moved.verified_at.is_some());
        assert_eq!(moved.former.len(), 1);
        assert_eq!(moved.former[0].isk_pub, pinned.isk_pub);
        // The old devices stay, revoked, for what they signed.
        let old_device = *pinned.devices.keys().next().unwrap();
        assert!(moved.devices[&old_device].revoked);
        assert_eq!(
            moved.former_of(old_device).unwrap().profile_id,
            pinned.profile_id
        );
        // Moved once: the same lookup again continues it.
        assert!(matches!(
            moved.take_lookup(&handed, &list, &never).unwrap(),
            Lookup::Continues(_)
        ));
        // Never backwards: the old keys are refused now, vouched or not.
        let everything = |_: &Contact| true;
        assert!(moved.take_lookup(&same, &[], &everything).is_err());

        // A chain: bob2 hands over to bob3, whose profile lists both.
        let mut bob3 = open("bob3");
        publish_handover(&mut bob3.0, &bob3.1, &to_bob2).unwrap_err();
        let mut bob3_list = crate::key_change::handovers_of(bob2.0.profile()).unwrap();
        let to_bob3 = handover(&bob2, &bob3);
        bob3_list.push(to_bob3.clone());
        publish_handover(&mut bob3.0, &bob3.1, &to_bob3).unwrap();
        let (third, _) = contact_of(&bob3.0, &bob3.1);
        assert!(matches!(
            pinned.take_lookup(&third, &bob3_list, &never).unwrap(),
            Lookup::Moved(_, KeyMove::HandedOver)
        ));
        // Another account answering: refused.
        let mut other = third.clone();
        other.account_id = Id::new();
        assert!(pinned.take_lookup(&other, &bob3_list, &everything).is_err());
    }

    #[test]
    fn digits_are_stable() {
        let mut b = [0u8; 32];
        assert_eq!(digits(&b), "00000 00000 00000 00000 00000 00000");
        b[4] = 1;
        assert!(digits(&b).starts_with("00001 "));
    }
}
