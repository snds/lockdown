//! Key transparency (04-security §6; 02-model §8; 03-architecture §5): the
//! shape of the server's public, append-only log and the signed tree heads
//! over it, shared by the server that writes it, the client that checks
//! it, and the store that remembers what was checked.
//!
//! The log is a Merkle tree (`lockdown_crypto::merkle`, RFC 6962 shape over
//! BLAKE3) whose leaves are [`LogLeaf`]s: for every account, every
//! identity manifest head the server accepted, every vault manifest head it
//! accepted, and every device public key it bound at login. The server
//! signs the tree's size and root as a [`SignedTreeHead`] (STH). What the
//! research asked for (brief §4.1, the ETH Zurich attacks) is that the
//! server cannot swap a device's public key, hide a device, or roll back
//! without the clients noticing; so a client:
//!
//! 1. keeps the last STH it saw per server and refuses a new one that is
//!    smaller, or the same size with another root, or larger without a
//!    consistency proof from the old to the new (the log only grows);
//! 2. audits its own account's leaves with an inclusion proof each: every
//!    device key in the log must be one its identity records know, its own
//!    device must be there, its newest identity manifest must be there
//!    with the hash it holds, and no vault head may differ from the one it
//!    holds at that `seq`;
//! 3. cites the STH it verified in every vault head it signs
//!    ([`SthRef`]), so a second device that syncs the same vault checks the
//!    citation against its own view through a consistency proof, and a
//!    server that showed the two devices two different logs is caught
//!    (gossip through the vault head).
//!
//! What this is not yet: VRF-blinded lookups (04-security §6 marks them
//! uncertain; the account's leaves are served to the account's sessions
//! only, proofs are public) and mirrors outside the server
//! (`05-self-host.md` §6); `lockdown checkpoint` exports what a mirror
//! would hold.

use lockdown_crypto::cbor::to_canonical;
use lockdown_crypto::merkle;
use lockdown_crypto::{SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::{Error, Hash32, Id, Result, Timestamp};

/// The leaf format version.
pub const LEAF_V: u16 = 1;

/// The domain-separation prefix under an STH signature.
pub const STH_DOMAIN: &[u8] = b"lockdown-kt-sth-v1\0";

/// The domain-separation prefix under a checkpoint signature (a device's
/// own, over an exported [`Checkpoint`]).
pub const CHECKPOINT_DOMAIN: &[u8] = b"lockdown-kt-checkpoint-v1\0";

/// What one log entry records.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LogEntry {
    /// The server accepted an identity manifest (D71) at this `seq` with
    /// this hash. The manifest lists every device record and key envelope
    /// of the identity, so this one hash pins them all.
    IdentityHead {
        /// The manifest `seq`.
        seq: u64,
        /// Its hash.
        hash: Hash32,
    },
    /// The server accepted a vault manifest head (02-model §8) at this
    /// `seq` with this hash: the anti-rollback checkpoint.
    VaultHead {
        /// The vault.
        vault_id: Id,
        /// The head's `seq`.
        seq: u64,
        /// Its hash.
        hash: Hash32,
    },
    /// A device logged in to the account and bound this public key (D51).
    Device {
        /// The device.
        device_id: Id,
        /// Its Ed25519 public key.
        #[serde(with = "serde_bytes")]
        device_pub: [u8; 32],
    },
}

/// One leaf of the log: an account, a time, and what happened.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct LogLeaf {
    /// Format version.
    pub v: u16,
    /// The account the entry is about.
    pub account_id: Id,
    /// When the server appended it (seconds since the Unix epoch).
    pub time: i64,
    /// What happened.
    pub entry: LogEntry,
}

impl LogLeaf {
    /// A leaf for `entry` now.
    pub fn new(account_id: Id, entry: LogEntry) -> Self {
        Self {
            v: LEAF_V,
            account_id,
            time: Timestamp::now().0,
            entry,
        }
    }

    /// As canonical CBOR: the bytes the log keeps and hashes.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        Ok(to_canonical(self)?)
    }

    /// From CBOR.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let leaf: Self = lockdown_crypto::cbor::from_slice(bytes)?;
        if leaf.v > LEAF_V {
            return Err(Error::SchemaTooNew {
                found: leaf.v,
                supported: LEAF_V,
            });
        }
        Ok(leaf)
    }

    /// The leaf hash of its bytes.
    pub fn leaf_hash(&self) -> Result<Hash32> {
        Ok(merkle::leaf_hash(&self.to_bytes()?))
    }

    /// One line for a person: what the entry says.
    pub fn describe(&self) -> String {
        match &self.entry {
            LogEntry::IdentityHead { seq, hash } => {
                format!("identity manifest {seq} ({})", &hash.to_hex()[..16])
            }
            LogEntry::VaultHead {
                vault_id,
                seq,
                hash,
            } => format!("vault {vault_id} head {seq} ({})", &hash.to_hex()[..16]),
            LogEntry::Device {
                device_id,
                device_pub,
            } => format!(
                "device {device_id} key {}",
                &Hash32(*device_pub).to_hex()[..16]
            ),
        }
    }
}

/// A tree's size and root at a moment: what is signed.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct TreeHead {
    /// How many leaves.
    pub size: u64,
    /// The Merkle root over them.
    pub root: Hash32,
    /// When the server signed it (seconds since the Unix epoch).
    pub time: i64,
}

/// A signed tree head: the server's promise that its log has exactly this
/// many entries with this root at this time.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SignedTreeHead {
    /// How many leaves.
    pub size: u64,
    /// The Merkle root over them.
    pub root: Hash32,
    /// When.
    pub time: i64,
    /// Ed25519 by the server's log key over [`STH_DOMAIN`] and the
    /// canonical [`TreeHead`].
    #[serde(with = "serde_bytes")]
    pub sig: Vec<u8>,
}

impl SignedTreeHead {
    /// The head without its signature.
    pub fn tree_head(&self) -> TreeHead {
        TreeHead {
            size: self.size,
            root: self.root,
            time: self.time,
        }
    }

    /// What the citation in a vault head carries.
    pub fn as_ref(&self) -> SthRef {
        SthRef {
            size: self.size,
            root: self.root,
        }
    }

    fn signable(head: &TreeHead) -> Result<Vec<u8>> {
        let mut bytes = STH_DOMAIN.to_vec();
        bytes.extend_from_slice(&to_canonical(head)?);
        Ok(bytes)
    }

    /// Signs `head` with the server's log key.
    pub fn sign(head: TreeHead, log_key: &SigningKey) -> Result<Self> {
        let sig = log_key.sign(&Self::signable(&head)?).to_vec();
        Ok(Self {
            size: head.size,
            root: head.root,
            time: head.time,
            sig,
        })
    }

    /// Checks the signature under the server's log key.
    pub fn verify(&self, log_key: &VerifyingKey) -> Result<()> {
        log_key
            .verify(
                &Self::signable(&self.tree_head())?,
                &self.sig,
                "signed tree head",
            )
            .map_err(|_| {
                Error::Transparency(
                    "the signed tree head's signature didn't check out under the server's log key"
                        .into(),
                )
            })
    }
}

/// What a vault head cites: the size and root of the newest STH the
/// signing device had verified. Another device checks the citation against
/// its own view with a consistency proof (gossip).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SthRef {
    /// The tree size.
    pub size: u64,
    /// The root.
    pub root: Hash32,
}

/// One account's newest heads as a device holds them, beside the STH that
/// covers them: what `lockdown checkpoint` exports for a person to keep or
/// compare with another device, and what a mirror would hold.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Format version.
    pub v: u16,
    /// The server, as the device addresses it.
    pub server: String,
    /// The server's log key, as pinned by this device.
    #[serde(with = "serde_bytes")]
    pub log_key: [u8; 32],
    /// The newest STH this device verified.
    pub sth: SignedTreeHead,
    /// The identity manifest this device holds: `seq` and hash.
    pub identity_head: Option<(u64, Hash32)>,
    /// Every vault this device holds: id, head `seq`, head hash.
    pub vault_heads: Vec<(Id, u64, Hash32)>,
    /// When it was exported.
    pub exported_at: i64,
    /// The device that exported it.
    pub device_id: Id,
    /// Ed25519 by that device over [`CHECKPOINT_DOMAIN`] and the canonical
    /// checkpoint with `sig` empty.
    #[serde(with = "serde_bytes")]
    pub sig: Vec<u8>,
}

/// The checkpoint format version.
pub const CHECKPOINT_V: u16 = 1;

impl Checkpoint {
    fn signable(&self) -> Result<Vec<u8>> {
        let mut copy = self.clone();
        copy.sig.clear();
        let mut bytes = CHECKPOINT_DOMAIN.to_vec();
        bytes.extend_from_slice(&to_canonical(&copy)?);
        Ok(bytes)
    }

    /// Signs with the exporting device's key.
    pub fn sign(&mut self, signer: &dyn crate::Signer) -> Result<()> {
        self.sig = signer.sign(&self.signable()?)?.to_vec();
        Ok(())
    }

    /// Checks the device signature.
    pub fn verify(&self, device_key: &VerifyingKey) -> Result<()> {
        device_key
            .verify(&self.signable()?, &self.sig, "checkpoint")
            .map_err(|_| {
                Error::Transparency("the checkpoint's device signature didn't check out".into())
            })
    }

    /// BLAKE3 of the signed checkpoint: the one line two people compare.
    pub fn fingerprint(&self) -> Result<Hash32> {
        Ok(lockdown_crypto::hash::hash(&to_canonical(self)?))
    }

    /// As canonical CBOR.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        Ok(to_canonical(self)?)
    }

    /// From CBOR.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let cp: Self = lockdown_crypto::cbor::from_slice(bytes)?;
        if cp.v > CHECKPOINT_V {
            return Err(Error::SchemaTooNew {
                found: cp.v,
                supported: CHECKPOINT_V,
            });
        }
        Ok(cp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_sth_verifies_under_its_key_only_and_a_leaf_round_trips() {
        let key = SigningKey::generate().unwrap();
        let head = TreeHead {
            size: 3,
            root: Hash32([1; 32]),
            time: 1_800_000_000,
        };
        let sth = SignedTreeHead::sign(head, &key).unwrap();
        sth.verify(&key.verifying_key()).unwrap();
        assert!(
            sth.verify(&SigningKey::generate().unwrap().verifying_key())
                .is_err()
        );
        let mut bent = sth.clone();
        bent.size = 4;
        assert!(bent.verify(&key.verifying_key()).is_err());
        assert_eq!(
            sth.as_ref(),
            SthRef {
                size: 3,
                root: Hash32([1; 32])
            }
        );

        let leaf = LogLeaf::new(
            Id::new(),
            LogEntry::Device {
                device_id: Id::new(),
                device_pub: [9; 32],
            },
        );
        let bytes = leaf.to_bytes().unwrap();
        assert_eq!(LogLeaf::from_bytes(&bytes).unwrap(), leaf);
        assert_eq!(leaf.leaf_hash().unwrap(), merkle::leaf_hash(&bytes));
        assert!(leaf.describe().starts_with("device "));
        let mut newer = leaf.clone();
        newer.v = LEAF_V + 1;
        assert!(matches!(
            LogLeaf::from_bytes(&newer.to_bytes().unwrap()),
            Err(Error::SchemaTooNew { .. })
        ));
    }

    #[test]
    fn a_checkpoint_is_signed_by_the_device_and_fingerprinted() {
        let device = SigningKey::generate().unwrap();
        let log = SigningKey::generate().unwrap();
        let sth = SignedTreeHead::sign(
            TreeHead {
                size: 1,
                root: Hash32([2; 32]),
                time: 1,
            },
            &log,
        )
        .unwrap();
        let mut cp = Checkpoint {
            v: CHECKPOINT_V,
            server: "https://example.invalid".into(),
            log_key: log.verifying_key().to_bytes(),
            sth,
            identity_head: Some((2, Hash32([3; 32]))),
            vault_heads: vec![(Id::new(), 5, Hash32([4; 32]))],
            exported_at: 1,
            device_id: Id::new(),
            sig: Vec::new(),
        };
        cp.sign(&device).unwrap();
        cp.verify(&device.verifying_key()).unwrap();
        let back = Checkpoint::from_bytes(&cp.to_bytes().unwrap()).unwrap();
        assert_eq!(back, cp);
        assert_eq!(back.fingerprint().unwrap(), cp.fingerprint().unwrap());
        let mut bent = cp.clone();
        bent.vault_heads[0].1 = 6;
        assert!(bent.verify(&device.verifying_key()).is_err());
        assert_ne!(bent.fingerprint().unwrap(), cp.fingerprint().unwrap());
    }
}
