//! The vault manifest: a signed, hash-chained table of contents per vault
//! (02-model §8, §12; 03-architecture §6). It is the anti-rollback and
//! anti-deletion control.
//!
//! Each head names every live item, its revision and the hash of its latest
//! version, plus tombstones. Each carries the hash of the head before it and
//! a sequence number one higher. A head is signed by the device that made
//! it; the server holds no device keys, so it cannot forge or reorder heads.
//! Every device remembers the newest sequence it has seen and refuses an
//! older one: "Rollback attempt blocked."

use lockdown_crypto::VerifyingKey;
use lockdown_crypto::aead::Sealed;
use lockdown_crypto::cbor::to_canonical;
use lockdown_crypto::hash::hash;
use serde::{Deserialize, Serialize};

use crate::item::ItemState;
use crate::signer::Signer;
use crate::transparency::SthRef;
use crate::{Error, Hash32, Id, Result, Timestamp};

/// The manifest format version.
pub const MANIFEST_V: u16 = 1;

/// One line of the manifest.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ManifestEntry {
    /// The item.
    pub item_id: Id,
    /// Its server revision.
    pub rev: u64,
    /// BLAKE3 of its latest version.
    pub version_hash: Hash32,
    /// Where it is in its life.
    pub state: ItemState,
    /// Whether its keys were destroyed.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub shredded: bool,
}

/// One member of a shared vault, as sealed under the Vault Key.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Member {
    /// The persona.
    pub persona_id: Id,
    /// `view`, `edit`, `fill-only`, `manage`, `re-share`.
    pub rights: Vec<String>,
    /// The id of the Vault Key envelope wrapped to this member.
    pub key_env: Id,
}

/// The server's anchor for a head in its public checkpoint feed. Added by
/// the server, verified by clients; never part of the signature.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Position in the log.
    pub log_index: u64,
    /// The inclusion proof.
    #[serde(with = "serde_bytes")]
    pub inclusion_proof: Vec<u8>,
}

/// One manifest head.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ManifestHead {
    /// Format version.
    pub v: u16,
    /// The vault.
    pub vault_id: Id,
    /// Rises by one on every head; the first is 1.
    pub seq: u64,
    /// Hash of the head before; zero for the first.
    pub prev_hash: Hash32,
    /// Which Vault Key generation this head is under. Only ever moves forward.
    pub vault_key_ref: Id,
    /// The members list, sealed under the Vault Key.
    pub members: Sealed,
    /// Every item, sorted by id.
    pub entries: Vec<ManifestEntry>,
    /// The device that signed it.
    pub signer: Id,
    /// When.
    pub signed_at: Timestamp,
    /// The newest key-transparency tree head the signing device had
    /// verified when it signed (04-security §6): the gossip hook. Another
    /// device that pulls this head checks the citation against its own
    /// view with a consistency proof and refuses a split view. Absent from
    /// a head signed with no server in sight, and from every head before
    /// the log existed. Under the signature.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sth: Option<SthRef>,
    /// Ed25519 by the signing device over the canonical head with `sig`
    /// empty and no checkpoint.
    #[serde(with = "serde_bytes")]
    pub sig: Vec<u8>,
    /// The server's anchor, once it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<Checkpoint>,
}

impl ManifestHead {
    /// The bytes the signature covers.
    pub fn signable(&self) -> Result<Vec<u8>> {
        let mut copy = self.clone();
        copy.sig.clear();
        copy.checkpoint = None;
        Ok(to_canonical(&copy)?)
    }

    /// Signs the head.
    pub fn sign(&mut self, signer: &dyn Signer) -> Result<()> {
        self.sig = signer.sign(&self.signable()?)?.to_vec();
        Ok(())
    }

    /// Checks the signature.
    pub fn verify_signature(&self, key: &VerifyingKey) -> Result<()> {
        key.verify(
            &self.signable()?,
            &self.sig,
            &format!("manifest {}", self.seq),
        )
        .map_err(|_| Error::ChainBroken {
            seq: self.seq,
            why: "the device signature didn't check out".into(),
        })
    }

    /// BLAKE3 of the signed head without the server's checkpoint: what the
    /// next head names as `prev_hash`.
    pub fn hash(&self) -> Result<Hash32> {
        let mut copy = self.clone();
        copy.checkpoint = None;
        Ok(hash(&to_canonical(&copy)?))
    }

    /// As canonical CBOR: what a store keeps.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        Ok(to_canonical(self)?)
    }

    /// From CBOR.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        Ok(lockdown_crypto::cbor::from_slice(bytes)?)
    }
}

/// Refuses any head in `heads` signed by a revoked device after the point
/// its revocation names: `revoked_after(device)` is the newest `seq` that
/// device may have signed, `None` for a device in good standing (D73).
/// History a device signed before revocation still verifies.
pub fn check_revocations(
    heads: &[ManifestHead],
    revoked_after: impl Fn(Id) -> Option<u64>,
) -> Result<()> {
    for head in heads {
        if let Some(after) = revoked_after(head.signer)
            && head.seq > after
        {
            return Err(Error::RevokedDevice {
                device: head.signer,
                seq: head.seq,
                after,
            });
        }
    }
    Ok(())
}

/// Verifies `heads` (oldest first) as a chain: each signed by a key
/// `key_for(signer)` knows, `seq` rising by one, `prev_hash` matching, and
/// `vault_key_ref` never going back to an earlier generation. Then refuses
/// the chain if its newest `seq` is below `last_accepted_seq`.
pub fn verify_chain(
    heads: &[ManifestHead],
    key_for: impl Fn(Id) -> Option<VerifyingKey>,
    last_accepted_seq: u64,
) -> Result<&ManifestHead> {
    let first = heads
        .first()
        .ok_or_else(|| Error::NotFound("any manifest head".into()))?;
    let mut prev: Option<&ManifestHead> = None;
    let mut key_generations: Vec<Id> = Vec::new();
    for head in heads {
        let broken = |why: &str| Error::ChainBroken {
            seq: head.seq,
            why: why.into(),
        };
        if head.v > MANIFEST_V {
            return Err(broken("newer manifest format than this app reads"));
        }
        if head.vault_id != first.vault_id {
            return Err(broken("a head from another vault"));
        }
        let key = key_for(head.signer)
            .ok_or_else(|| broken("signed by a device this vault doesn't know"))?;
        head.verify_signature(&key)?;
        match prev {
            None => {
                if head.prev_hash != Hash32::ZERO && head.seq == 1 {
                    return Err(broken("the first head names a predecessor"));
                }
            }
            Some(p) => {
                if head.seq != p.seq + 1 {
                    return Err(broken("the sequence skipped or repeated"));
                }
                if head.prev_hash != p.hash()? {
                    return Err(broken("prev_hash doesn't match the head before it"));
                }
                if head.vault_key_ref != p.vault_key_ref
                    && key_generations.contains(&head.vault_key_ref)
                {
                    return Err(broken("the vault key went back to an earlier generation"));
                }
            }
        }
        if !key_generations.contains(&head.vault_key_ref) {
            key_generations.push(head.vault_key_ref);
        }
        let mut ids: Vec<Id> = head.entries.iter().map(|e| e.item_id).collect();
        ids.dedup();
        if ids.len() != head.entries.len() || !ids.is_sorted() {
            return Err(broken("entries aren't sorted and unique by item id"));
        }
        prev = Some(head);
    }
    let newest = prev.expect("at least one head");
    if newest.seq < last_accepted_seq {
        return Err(Error::Rollback {
            found: newest.seq,
            remembered: last_accepted_seq,
        });
    }
    Ok(newest)
}
