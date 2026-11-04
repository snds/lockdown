//! When another person's keys change (docs/plan/55-member-key-change.md).
//!
//! A contact is pinned the first time Lockdown sees them, and a later
//! lookup that names other keys is refused (`Contact::check_continues`,
//! D248, D551). Two real changes have to get through all the same, and one
//! fake must not:
//!
//! - **A planned change** is vouched for by the keys it replaces: a
//!   [`KeyHandover`] names the old keys and the new ones and is signed by
//!   both identity signing keys, and by both post-quantum keys where they
//!   exist. The new identity publishes its hand-overs on its signed profile
//!   ([`HANDOVER_FIELD`]), oldest first, so a contact pinned several
//!   changes ago walks the whole chain ([`walk`]). No identity can rotate
//!   its keys yet (the identity crate derives its signing key from the root
//!   key with a fixed label, D19); this is the record any rotation uses.
//! - **A reset after a lost recovery kit** has nothing left to sign with.
//!   Only the shared vault's owner can vouch for the new keys, by naming
//!   them in a signed membership change ([`MemberKeys`] on
//!   `Change::Add`, `crate::sharing`); every other member's device follows
//!   that change.
//! - **A key the server or the log alone shows** is never taken: that is
//!   all a hostile server could fake, and taking it would hand it the
//!   vault key.

use std::collections::BTreeMap;

use ciborium::Value;
use ecosystem_identity::kem::KemPublicKey;
use ecosystem_identity::keystore::DeviceKeystore;
use ecosystem_identity::records::Profile;
use ecosystem_identity::{Head, Identity};
use lockdown_crypto::cbor::to_canonical;
use lockdown_crypto::pq_sign::{PqSigningKey, PqVerifyingKey};
use lockdown_crypto::{SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

use crate::{Error, Id, Result, Timestamp};

/// The profile field that lists an identity's hand-overs, oldest first.
pub const HANDOVER_FIELD: &str = "lockdown.handovers";
/// The hand-over's format version.
pub const HANDOVER_V: u16 = 1;
/// Domain separation for the Ed25519 signatures.
const SIG_PREFIX: &[u8] = b"lockdown-key-handover-v1\0";
/// The ML-DSA context of both post-quantum signatures.
pub const HANDOVER_CONTEXT: &[u8] = b"lockdown key handover";
/// The most hand-overs a profile may list (a bound on what a lookup walks).
pub const MAX_HANDOVERS: usize = 64;

/// One identity's public keys, as a hand-over names them.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct HandoverKeys {
    /// The profile.
    pub profile_id: Id,
    /// The Identity Signing Key.
    #[serde(with = "serde_bytes")]
    pub isk_pub: [u8; 32],
    /// The identity's ML-DSA-65 key, when it publishes one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pq_isk: Option<ByteBuf>,
    /// Every persona's receiving key, by persona.
    pub personas: BTreeMap<Id, KemPublicKey>,
}

impl HandoverKeys {
    /// The keys an open identity publishes now.
    pub fn of_identity(identity: &Identity) -> Result<Self> {
        Ok(Self {
            profile_id: identity.profile().profile_id,
            isk_pub: identity.profile().isk_pub,
            pq_isk: crate::pq::published_key(identity.profile())?
                .map(|k| ByteBuf::from(k.as_bytes().to_vec())),
            personas: identity
                .personas()
                .map(|p| (p.persona_id, p.kem_pub.clone()))
                .collect(),
        })
    }

    fn pq_key(&self) -> Result<Option<PqVerifyingKey>> {
        self.pq_isk
            .as_ref()
            .map(|b| PqVerifyingKey::from_bytes(b))
            .transpose()
            .map_err(Error::from)
    }
}

/// A signed hand-over from one set of keys to the next: the old identity
/// says "these are my keys now", and the new one says "these were mine".
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct KeyHandover {
    /// Format version.
    pub v: u16,
    /// The keys handed over from.
    pub from: HandoverKeys,
    /// The keys handed over to.
    pub to: HandoverKeys,
    /// Where each persona of `from` went in `to`, by persona.
    pub personas: BTreeMap<Id, Id>,
    /// When.
    pub at: Timestamp,
    /// Ed25519 by `from.isk_pub` over the prefix and the hand-over with
    /// every signature cleared.
    #[serde(with = "serde_bytes")]
    pub from_sig: Vec<u8>,
    /// Ed25519 by `to.isk_pub` over the same bytes.
    #[serde(with = "serde_bytes")]
    pub to_sig: Vec<u8>,
    /// ML-DSA-65 by `from.pq_isk` over the same bytes; required when
    /// `from` names that key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_pq_sig: Option<ByteBuf>,
    /// ML-DSA-65 by `to.pq_isk`; required when `to` names that key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_pq_sig: Option<ByteBuf>,
}

impl KeyHandover {
    /// An unsigned hand-over from `from` to `to`, each persona of `from`
    /// going to the persona `personas` names.
    pub fn new(from: HandoverKeys, to: HandoverKeys, personas: BTreeMap<Id, Id>) -> Self {
        Self {
            v: HANDOVER_V,
            from,
            to,
            personas,
            at: Timestamp::now(),
            from_sig: Vec::new(),
            to_sig: Vec::new(),
            from_pq_sig: None,
            to_pq_sig: None,
        }
    }

    /// The bytes every signature covers: the hand-over with all four cleared.
    pub fn signable(&self) -> Result<Vec<u8>> {
        let mut copy = self.clone();
        copy.from_sig.clear();
        copy.to_sig.clear();
        copy.from_pq_sig = None;
        copy.to_pq_sig = None;
        let mut out = SIG_PREFIX.to_vec();
        out.extend(to_canonical(&copy)?);
        Ok(out)
    }

    /// Signs as the identity handed over from: its signing key, and its
    /// post-quantum key when `from` names one. Refused for keys that
    /// aren't the ones `from` names.
    pub fn sign_from(&mut self, isk: &SigningKey, pq: Option<&PqSigningKey>) -> Result<()> {
        let (sig, pq_sig) = self.sign_as(&self.from, isk, pq)?;
        self.from_sig = sig;
        self.from_pq_sig = pq_sig;
        Ok(())
    }

    /// Signs as the identity handed over to.
    pub fn sign_to(&mut self, isk: &SigningKey, pq: Option<&PqSigningKey>) -> Result<()> {
        let (sig, pq_sig) = self.sign_as(&self.to, isk, pq)?;
        self.to_sig = sig;
        self.to_pq_sig = pq_sig;
        Ok(())
    }

    fn sign_as(
        &self,
        keys: &HandoverKeys,
        isk: &SigningKey,
        pq: Option<&PqSigningKey>,
    ) -> Result<(Vec<u8>, Option<ByteBuf>)> {
        if isk.verifying_key().to_bytes() != keys.isk_pub {
            return Err(Error::Invalid(
                "that signing key isn't the one the hand-over names".into(),
            ));
        }
        let msg = self.signable()?;
        let pq_sig = match (keys.pq_key()?, pq) {
            (Some(want), Some(pq)) if pq.verifying_key() == want => {
                Some(ByteBuf::from(pq.sign(&msg, HANDOVER_CONTEXT)?))
            }
            (None, None) => None,
            _ => {
                return Err(Error::Invalid(
                    "that post-quantum key isn't the one the hand-over names".into(),
                ));
            }
        };
        Ok((isk.sign(&msg).to_vec(), pq_sig))
    }

    /// Checks the hand-over on its own: its shape, and all four signatures
    /// (each post-quantum one that its side's key calls for, and none
    /// without a key to check it under).
    pub fn verify(&self) -> Result<()> {
        let bad = |why: &str| Error::Transparency(format!("a key hand-over {why}; refused"));
        if self.v > HANDOVER_V {
            return Err(bad("is in a newer format than this app reads"));
        }
        if self.from == self.to {
            return Err(bad("names the same keys on both sides"));
        }
        if self.from.isk_pub == self.to.isk_pub && self.from.profile_id != self.to.profile_id {
            return Err(bad("names one signing key for two profiles"));
        }
        for (old, new) in &self.personas {
            if !self.from.personas.contains_key(old) || !self.to.personas.contains_key(new) {
                return Err(bad("moves a persona it doesn't name"));
            }
        }
        let msg = self.signable()?;
        for (keys, sig, pq_sig, side) in [
            (&self.from, &self.from_sig, &self.from_pq_sig, "old"),
            (&self.to, &self.to_sig, &self.to_pq_sig, "new"),
        ] {
            VerifyingKey::from_bytes(&keys.isk_pub)?
                .verify(&msg, sig, "a key hand-over")
                .map_err(|_| bad(&format!("isn't signed by the {side} signing key")))?;
            match (keys.pq_key()?, pq_sig) {
                (Some(key), Some(sig)) => key
                    .verify(&msg, HANDOVER_CONTEXT, sig, "a key hand-over")
                    .map_err(|_| {
                        bad(&format!(
                            "carries a post-quantum signature the {side} key didn't make"
                        ))
                    })?,
                (Some(_), None) => {
                    return Err(bad(&format!("isn't signed by the {side} post-quantum key")));
                }
                (None, Some(_)) => {
                    return Err(bad(&format!(
                        "carries a post-quantum signature for a {side} key it doesn't name"
                    )));
                }
                (None, None) => {}
            }
        }
        Ok(())
    }

    /// As canonical CBOR.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        Ok(to_canonical(self)?)
    }

    /// From CBOR.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        Ok(lockdown_crypto::cbor::from_slice(bytes)?)
    }
}

/// The hand-overs a profile lists, oldest first, unchecked. None for a
/// profile without the field; a malformed one is an error, never "none".
pub fn handovers_of(profile: &Profile) -> Result<Vec<KeyHandover>> {
    let malformed = || Error::Invalid("the profile's key hand-overs are malformed".into());
    match profile.extra.get(HANDOVER_FIELD) {
        None => Ok(Vec::new()),
        Some(Value::Array(list)) => {
            if list.len() > MAX_HANDOVERS {
                return Err(malformed());
            }
            list.iter()
                .map(|v| match v {
                    Value::Bytes(b) => KeyHandover::from_bytes(b).map_err(|_| malformed()),
                    _ => Err(malformed()),
                })
                .collect()
        }
        Some(_) => Err(malformed()),
    }
}

/// Adds a hand-over to this identity's profile, after the ones it lists.
/// It must check out and hand over to the keys this identity publishes
/// now. Answers the new identity head, or `None` when it was listed.
pub fn publish_handover(
    identity: &mut Identity,
    keystore: &dyn DeviceKeystore,
    handover: &KeyHandover,
) -> Result<Option<Head>> {
    handover.verify()?;
    if handover.to != HandoverKeys::of_identity(identity)? {
        return Err(Error::Invalid(
            "the hand-over doesn't hand over to this identity's keys".into(),
        ));
    }
    let mut list = handovers_of(identity.profile())?;
    if list.contains(handover) {
        return Ok(None);
    }
    if list.len() >= MAX_HANDOVERS {
        return Err(Error::Invalid(format!(
            "a profile lists at most {MAX_HANDOVERS} key hand-overs"
        )));
    }
    list.push(handover.clone());
    let value = list
        .iter()
        .map(|h| h.to_bytes().map(Value::Bytes))
        .collect::<Result<Vec<_>>>()?;
    crate::handover::set_profile_extra(
        identity,
        keystore,
        &[(HANDOVER_FIELD, Some(Value::Array(value)))],
    )
}

/// The keys of one persona as a contact pins them.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PinnedKeys {
    /// The profile.
    pub profile_id: Id,
    /// The Identity Signing Key.
    pub isk_pub: [u8; 32],
    /// The post-quantum key, when one is pinned.
    pub pq_isk: Option<ByteBuf>,
    /// The persona shares go to.
    pub persona_id: Id,
    /// Its receiving key.
    pub kem_pub: KemPublicKey,
}

impl PinnedKeys {
    fn after(&self, h: &KeyHandover) -> Option<Self> {
        let from_kem = h.from.personas.get(&self.persona_id)?;
        let pq_matches = self.pq_isk.is_none() || self.pq_isk == h.from.pq_isk;
        if h.from.profile_id != self.profile_id
            || h.from.isk_pub != self.isk_pub
            || *from_kem != self.kem_pub
            || !pq_matches
        {
            return None;
        }
        let persona_id = *h.personas.get(&self.persona_id)?;
        Some(Self {
            profile_id: h.to.profile_id,
            isk_pub: h.to.isk_pub,
            pq_isk: h.to.pq_isk.clone(),
            persona_id,
            kem_pub: h.to.personas.get(&persona_id)?.clone(),
        })
    }

    /// Whether `other` names the same keys (a post-quantum key `self`
    /// never pinned counts as the same).
    pub fn same_as(&self, other: &Self) -> bool {
        self.profile_id == other.profile_id
            && self.isk_pub == other.isk_pub
            && self.persona_id == other.persona_id
            && self.kem_pub == other.kem_pub
            && (self.pq_isk.is_none() || self.pq_isk == other.pq_isk)
    }
}

/// Walks `handovers` from `pinned` to `target`: each step a hand-over that
/// checks out and starts at the keys reached so far, each used once.
/// Answers how many steps it took, or `None` when no chain reaches it. A
/// hand-over that doesn't check out is passed over, never followed.
pub fn walk(pinned: &PinnedKeys, handovers: &[KeyHandover], target: &PinnedKeys) -> Option<usize> {
    let mut at = pinned.clone();
    let mut used = vec![false; handovers.len()];
    for step in 1..=handovers.len().min(MAX_HANDOVERS) {
        let next = handovers.iter().enumerate().find_map(|(i, h)| {
            if used[i] {
                return None;
            }
            let after = at.after(h)?;
            h.verify().ok()?;
            Some((i, after))
        });
        let (i, after) = next?;
        used[i] = true;
        if after.same_as(target) || target.same_as(&after) {
            return Some(step);
        }
        at = after;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use ecosystem_identity::FileKeystore;

    struct Person {
        _dir: tempfile::TempDir,
        identity: Identity,
        keystore: FileKeystore,
    }

    fn person(name: &str, pq: bool) -> Person {
        let dir = tempfile::tempdir().unwrap();
        let keystore = FileKeystore::open_or_create(&dir.path().join("k.cbor")).unwrap();
        let mut identity =
            Identity::create(&dir.path().join(name), "lockdown", name, &keystore).unwrap();
        if pq {
            crate::pq::roll_out(&mut identity, &keystore).unwrap();
        }
        Person {
            _dir: dir,
            identity,
            keystore,
        }
    }

    impl Person {
        fn keys(&self) -> HandoverKeys {
            HandoverKeys::of_identity(&self.identity).unwrap()
        }
        fn isk(&self) -> SigningKey {
            crate::identity::identity_signing_key(&self.identity, &self.keystore).unwrap()
        }
        fn pq(&self) -> Option<PqSigningKey> {
            crate::pq::published_key(self.identity.profile())
                .unwrap()
                .map(|_| crate::pq::identity_pq_key(&self.identity, &self.keystore).unwrap())
        }
        fn persona(&self) -> Id {
            self.identity.personas().next().unwrap().persona_id
        }
        fn pinned(&self) -> PinnedKeys {
            let p = self.identity.personas().next().unwrap();
            PinnedKeys {
                profile_id: self.identity.profile().profile_id,
                isk_pub: self.identity.profile().isk_pub,
                pq_isk: self.keys().pq_isk,
                persona_id: p.persona_id,
                kem_pub: p.kem_pub.clone(),
            }
        }
    }

    fn handover(old: &Person, new: &Person) -> KeyHandover {
        let mut h = KeyHandover::new(
            old.keys(),
            new.keys(),
            BTreeMap::from([(old.persona(), new.persona())]),
        );
        h.sign_from(&old.isk(), old.pq().as_ref()).unwrap();
        h.sign_to(&new.isk(), new.pq().as_ref()).unwrap();
        h
    }

    #[test]
    fn a_hand_over_is_signed_by_both_sides_and_anything_less_is_refused() {
        let (old, new) = (person("bob", true), person("bob2", true));
        let h = handover(&old, &new);
        h.verify().unwrap();
        assert_eq!(KeyHandover::from_bytes(&h.to_bytes().unwrap()).unwrap(), h);

        // Only the new side signed: a key the old one never vouched for.
        let mallory = person("mallory", true);
        let mut forged = KeyHandover::new(
            old.keys(),
            mallory.keys(),
            BTreeMap::from([(old.persona(), mallory.persona())]),
        );
        assert!(
            forged
                .sign_from(&mallory.isk(), mallory.pq().as_ref())
                .is_err()
        );
        forged
            .sign_to(&mallory.isk(), mallory.pq().as_ref())
            .unwrap();
        forged.from_sig = forged.to_sig.clone();
        assert!(
            forged
                .verify()
                .unwrap_err()
                .to_string()
                .contains("old signing key")
        );

        // Stripped of the old post-quantum signature.
        let mut stripped = h.clone();
        stripped.from_pq_sig = None;
        assert!(
            stripped
                .verify()
                .unwrap_err()
                .to_string()
                .contains("old post-quantum")
        );
        // The new side's post-quantum signature bent.
        let mut bent = h.clone();
        bent.to_pq_sig.as_mut().unwrap()[10] ^= 1;
        assert!(bent.verify().is_err());
        // A field changed after signing.
        let mut moved = h.clone();
        moved.at = Timestamp(moved.at.0 + 1);
        assert!(moved.verify().is_err());
        // A persona moved that it doesn't name.
        let mut stray = h.clone();
        stray.personas.insert(Id::new(), new.persona());
        assert!(stray.verify().is_err());

        // Keys without a post-quantum half: Ed25519 alone, and a stray
        // post-quantum signature is refused.
        let (plain_old, plain_new) = (person("p", false), person("p2", false));
        let mut plain = handover(&plain_old, &plain_new);
        plain.verify().unwrap();
        plain.to_pq_sig = h.to_pq_sig.clone();
        assert!(plain.verify().is_err());
    }

    #[test]
    fn a_chain_of_hand_overs_walks_forward_once_and_never_back() {
        let (a, b, c) = (person("a", true), person("b", true), person("c", true));
        let ab = handover(&a, &b);
        let bc = handover(&b, &c);
        // One step, then two, in any order the profile lists them.
        assert_eq!(
            walk(&a.pinned(), std::slice::from_ref(&ab), &b.pinned()),
            Some(1)
        );
        assert_eq!(
            walk(&a.pinned(), &[bc.clone(), ab.clone()], &c.pinned()),
            Some(2)
        );
        // Never backwards: from c to a, or from b to a.
        assert_eq!(
            walk(&c.pinned(), &[ab.clone(), bc.clone()], &a.pinned()),
            None
        );
        assert_eq!(
            walk(&b.pinned(), &[ab.clone(), bc.clone()], &a.pinned()),
            None
        );
        // A gap in the chain stops it.
        assert_eq!(
            walk(&a.pinned(), std::slice::from_ref(&bc), &c.pinned()),
            None
        );
        // A forged link is passed over.
        let mut bent = bc.clone();
        bent.from_sig[0] ^= 1;
        assert_eq!(walk(&a.pinned(), &[ab.clone(), bent], &c.pinned()), None);
        // A pin from before the post-quantum key still follows its hand-over.
        let mut old_pin = a.pinned();
        old_pin.pq_isk = None;
        assert_eq!(
            walk(&old_pin, std::slice::from_ref(&ab), &b.pinned()),
            Some(1)
        );
        // A pin of another receiving key doesn't.
        let mut other = a.pinned();
        other.kem_pub = c.pinned().kem_pub;
        assert_eq!(walk(&other, std::slice::from_ref(&ab), &b.pinned()), None);
    }

    #[test]
    fn the_new_identity_lists_its_hand_overs_on_its_signed_profile() {
        let old = person("bob", true);
        let mut new = person("bob2", true);
        let h = handover(&old, &new);
        assert!(handovers_of(new.identity.profile()).unwrap().is_empty());
        publish_handover(&mut new.identity, &new.keystore, &h).unwrap();
        assert_eq!(
            handovers_of(new.identity.profile()).unwrap(),
            vec![h.clone()]
        );
        // Once.
        assert!(
            publish_handover(&mut new.identity, &new.keystore, &h)
                .unwrap()
                .is_none()
        );
        // Not on an identity it doesn't hand over to.
        let mut other = person("x", true);
        assert!(publish_handover(&mut other.identity, &other.keystore, &h).is_err());
        // Nor a forged one.
        let mut bent = h.clone();
        bent.to_sig[0] ^= 1;
        assert!(publish_handover(&mut new.identity, &new.keystore, &bent).is_err());
    }
}
