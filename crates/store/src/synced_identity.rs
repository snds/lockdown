//! The identity's synced record set, as this device last accepted it from
//! the server, kept in a file of its own (`identity-sync.cbor`).
//!
//! This is the view every other machine's devices are trusted through
//! (07-workspace §4 step 7; D71, D73). The server relays the set; what
//! makes an entry count is the Identity Signing Key's signature on every
//! record and on the manifest that lists them, checked here against the
//! key this machine's own profile carries. The file holds the whole set
//! (manifest and record bytes), so the vault opens offline with every
//! device it needs and the next pull can prove the server continued the
//! chain rather than replaced it: a set behind this one is a rollback, a
//! set that does not chain to it is a fork, and a revocation once in it
//! never comes out.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ecosystem_identity::records::Device;
use lockdown_crypto::VerifyingKey;
use lockdown_vault::identity_set::{IdentityManifest, RecordSet, Verified};
use lockdown_vault::{Hash32, Id};
use serde::{Deserialize, Serialize};

use crate::remembered::write_private;
use crate::{StoreError, StoreResult};

const SYNCED_V: u16 = 1;

#[derive(Default, Serialize, Deserialize)]
struct SyncedFile {
    v: u16,
    /// The newest manifest accepted, as bytes.
    #[serde(default, with = "serde_bytes", skip_serializing_if = "Option::is_none")]
    manifest: Option<Vec<u8>>,
    /// The records it lists, by path.
    #[serde(default)]
    records: BTreeMap<String, serde_bytes::ByteBuf>,
}

/// The synced record set, backed by one file.
pub struct SyncedIdentity {
    path: PathBuf,
    file: SyncedFile,
    /// The set decoded and verified, when the file holds one.
    verified: Option<Verified>,
}

impl std::fmt::Debug for SyncedIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyncedIdentity")
            .field("path", &self.path)
            .field("seq", &self.seq())
            .finish()
    }
}

/// Why a set was not accepted.
#[derive(Debug, thiserror::Error)]
pub enum AcceptError {
    /// The set is behind the one this device accepted.
    #[error(
        "Rollback attempt blocked: the server offered identity manifest {offered} but this device has already accepted {remembered}"
    )]
    Rollback {
        /// What was offered.
        offered: u64,
        /// What this device holds.
        remembered: u64,
    },
    /// The set does not continue the one this device accepted.
    #[error(
        "Fork detected: identity manifest {seq} doesn't continue the one this device accepted; nothing was changed"
    )]
    Fork {
        /// Where.
        seq: u64,
    },
    /// The set failed a check (signature, hash, shape, revocation rule).
    #[error(transparent)]
    Invalid(#[from] lockdown_vault::Error),
    /// The file could not be written or read.
    #[error(transparent)]
    Store(#[from] StoreError),
}

impl SyncedIdentity {
    /// Opens the file at `path`, or starts empty if there is none. A file
    /// that is there must verify under `isk_pub` when one is given (a
    /// device that holds the identity); otherwise under its own profile.
    pub fn open(path: &Path, isk_pub: Option<&[u8; 32]>) -> StoreResult<Self> {
        let file = match std::fs::read(path) {
            Ok(bytes) => {
                let file: SyncedFile = lockdown_crypto::cbor::from_slice(&bytes)
                    .map_err(|_| StoreError::Tampered("the identity-sync file".into()))?;
                if file.v > SYNCED_V {
                    return Err(StoreError::SchemaTooNew {
                        found: u32::from(file.v),
                        supported: u32::from(SYNCED_V),
                    });
                }
                file
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => SyncedFile {
                v: SYNCED_V,
                ..Default::default()
            },
            Err(e) => return Err(StoreError::Backend(e.to_string())),
        };
        let verified =
            match &file.manifest {
                Some(bytes) => {
                    let set = RecordSet {
                        manifest: IdentityManifest::from_bytes(bytes).map_err(|e| {
                            StoreError::Tampered(format!("the identity-sync file: {e}"))
                        })?,
                        records: file
                            .records
                            .iter()
                            .map(|(k, v)| (k.clone(), v.to_vec()))
                            .collect(),
                    };
                    Some(set.verify(isk_pub).map_err(|e| {
                        StoreError::Tampered(format!("the identity-sync file: {e}"))
                    })?)
                }
                None => None,
            };
        Ok(Self {
            path: path.to_path_buf(),
            file,
            verified,
        })
    }

    /// Where the file lives.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The newest manifest accepted, if any.
    pub fn manifest(&self) -> Option<&IdentityManifest> {
        self.verified.as_ref().map(|v| &v.manifest)
    }

    /// The set as decoded and verified, if any.
    pub fn verified(&self) -> Option<&Verified> {
        self.verified.as_ref()
    }

    /// The newest manifest `seq` accepted, or zero.
    pub fn seq(&self) -> u64 {
        self.manifest().map(|m| m.seq).unwrap_or(0)
    }

    /// The newest manifest's hash, if any.
    pub fn hash(&self) -> Option<Hash32> {
        self.manifest().and_then(|m| m.hash().ok())
    }

    /// The set as held, for a push that continues it or a check against
    /// the server.
    pub fn record_set(&self) -> Option<RecordSet> {
        let manifest = self.manifest()?.clone();
        Some(RecordSet {
            manifest,
            records: self
                .file
                .records
                .iter()
                .map(|(k, v)| (k.clone(), v.to_vec()))
                .collect(),
        })
    }

    /// Accepts `set` as the next page after what this device holds (or
    /// the first), after verifying every signature under `isk_pub` (or the
    /// set's own profile key on a device that holds no identity yet), the
    /// chain, and that revocations only grew. `older` is any manifest
    /// between the held one and `set`'s, oldest first, when the server
    /// handed back more than one page; each must chain to the last.
    /// Writes the file. Returns the verified set.
    pub fn accept(
        &mut self,
        older: &[IdentityManifest],
        set: RecordSet,
        isk_pub: Option<&[u8; 32]>,
    ) -> Result<&Verified, AcceptError> {
        let held = match self.manifest() {
            Some(m) => Some((m.seq, m.hash()?)),
            None => None,
        };
        let mut prev = held;
        for m in older {
            m.check_shape()?;
            if let Some((seq, _)) = prev
                && m.seq <= seq
            {
                return Err(AcceptError::Rollback {
                    offered: m.seq,
                    remembered: seq,
                });
            }
            m.continues(prev)
                .map_err(|_| AcceptError::Fork { seq: m.seq })?;
            prev = Some((m.seq, m.hash()?));
        }
        if let Some((seq, hash)) = held
            && set.manifest.seq <= seq
        {
            if set.manifest.seq == seq && set.manifest.hash()? == hash {
                // The same page again: nothing to do.
                return Ok(self.verified.as_ref().expect("held"));
            }
            return Err(AcceptError::Rollback {
                offered: set.manifest.seq,
                remembered: seq,
            });
        }
        set.manifest
            .continues(prev)
            .map_err(|_| AcceptError::Fork {
                seq: set.manifest.seq,
            })?;
        let verified = set.verify(isk_pub)?;
        if let Some(mine) = &self.verified {
            // Signatures passed under `isk_pub`; when the caller gave none,
            // the chain itself ties the new page to the key of the old.
            if verified.profile.isk_pub != mine.profile.isk_pub {
                return Err(AcceptError::Fork {
                    seq: set.manifest.seq,
                });
            }
            let mut chain: Vec<&IdentityManifest> = Vec::with_capacity(older.len() + 2);
            chain.push(&mine.manifest);
            chain.extend(older.iter());
            chain.push(&verified.manifest);
            for pair in chain.windows(2) {
                pair[1].revocations_extend(pair[0])?;
            }
            let key = VerifyingKey::from_bytes(&mine.profile.isk_pub)
                .map_err(lockdown_vault::Error::from)?;
            for m in older {
                m.verify_signature(&key)?;
            }
        }
        self.file.manifest = Some(set.manifest.to_bytes()?);
        self.file.records = set
            .records
            .iter()
            .map(|(k, v)| (k.clone(), serde_bytes::ByteBuf::from(v.clone())))
            .collect();
        self.verified = Some(verified);
        self.write()?;
        Ok(self.verified.as_ref().expect("just set"))
    }

    /// Every device in the set, revoked ones included.
    pub fn devices(&self) -> impl Iterator<Item = &Device> {
        self.verified.iter().flat_map(|v| v.devices.values())
    }

    /// The devices' public keys, by id.
    pub fn device_keys(&self) -> BTreeMap<Id, VerifyingKey> {
        self.verified
            .as_ref()
            .map(|v| v.device_keys())
            .unwrap_or_default()
    }

    /// For each revoked device, where its writes stop in `vault_id`.
    pub fn revoked_after(&self, vault_id: Id) -> BTreeMap<Id, u64> {
        self.verified
            .as_ref()
            .map(|v| v.revoked_after(vault_id))
            .unwrap_or_default()
    }

    /// Whether a device is revoked in the set.
    pub fn is_revoked(&self, device_id: Id) -> bool {
        self.verified
            .as_ref()
            .is_some_and(|v| v.is_revoked(device_id))
    }

    fn write(&self) -> StoreResult<()> {
        let bytes = lockdown_crypto::cbor::to_canonical(&self.file)
            .map_err(|e| StoreError::Backend(e.to_string()))?;
        write_private(&self.path, &bytes).map_err(|e| StoreError::Backend(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ecosystem_identity::keystore::DeviceKeystore;
    use ecosystem_identity::{FileKeystore, Identity, Record};
    use lockdown_vault::identity::{identity_signing_key, record_set, revoke_device};
    use lockdown_vault::identity_set::{Revocation, VaultBoundary};

    #[test]
    fn a_set_is_accepted_once_chained_and_a_rollback_or_fork_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let keystore = FileKeystore::open_or_create(&dir.path().join("k.cbor")).unwrap();
        let mut identity =
            Identity::create(&dir.path().join("root"), "lockdown", "laptop", &keystore).unwrap();
        identity.adopt("lockdown").unwrap();
        let isk_pub = identity.profile().isk_pub;
        let isk = identity_signing_key(&identity, &keystore).unwrap();

        let path = dir.path().join("identity-sync.cbor");
        let mut synced = SyncedIdentity::open(&path, Some(&isk_pub)).unwrap();
        assert_eq!(synced.seq(), 0);
        let mut first = record_set(&identity, None, Vec::new()).unwrap();
        first.sign(&isk).unwrap();
        synced.accept(&[], first.clone(), Some(&isk_pub)).unwrap();
        assert_eq!(synced.seq(), 1);
        assert_eq!(synced.device_keys().len(), 1);
        // The same page again is fine; an older or unchained one is not.
        synced.accept(&[], first.clone(), Some(&isk_pub)).unwrap();
        let mut unchained = record_set(&identity, None, Vec::new()).unwrap();
        unchained.manifest.seq = 2;
        unchained.sign(&isk).unwrap();
        assert!(matches!(
            synced.accept(&[], unchained, Some(&isk_pub)),
            Err(AcceptError::Fork { seq: 2 })
        ));

        // A second device, enrolled elsewhere, revoked here: the set grows
        // and the revocation reads back per vault.
        let other = FileKeystore::open_or_create(&dir.path().join("k2.cbor")).unwrap();
        let backup = identity.export_backup();
        let phone = Identity::recover_with_account(
            &dir.path().join("root2"),
            "lockdown",
            "phone",
            &other,
            ecosystem_identity::AccountCredentials {
                auth: &mut ecosystem_identity::LocalStubAuth::generate().unwrap(),
                password: "correct horse battery",
                secret_key: None,
            },
            &backup,
        );
        // No account slot in this backup: the phone enrols by handing its
        // record over instead (what the kit path does in the CLI).
        assert!(phone.is_err());
        let phone_record = {
            let mut d = identity.devices().next().unwrap().clone();
            d.device_id = other.device_id();
            d.device_pub = other.device_pub();
            d.name = "phone".into();
            d.sign(isk.inner()).unwrap();
            d
        };
        let mut set2 = lockdown_vault::identity_set::RecordSet::from_records(
            identity.profile(),
            identity.personas().cloned(),
            identity.devices().cloned().chain([phone_record.clone()]),
            identity.envelopes().cloned(),
            identity.consents().cloned(),
            Some(&first.manifest),
            Vec::new(),
            identity.device_id(),
        )
        .unwrap();
        set2.sign(&isk).unwrap();
        synced.accept(&[], set2.clone(), Some(&isk_pub)).unwrap();
        assert_eq!(synced.device_keys().len(), 2);
        drop(synced);

        // Reopen: the file holds it; under another key it is refused.
        let synced = SyncedIdentity::open(&path, Some(&isk_pub)).unwrap();
        assert_eq!(synced.seq(), 2);
        assert!(SyncedIdentity::open(&path, Some(&[7; 32])).is_err());
        let mut synced = synced;

        // Merge the phone into the local store, then revoke it.
        let verified = synced.verified().unwrap().clone();
        lockdown_vault::identity::merge_records(
            &mut identity,
            &keystore,
            &verified,
            Some(&first.manifest),
        )
        .unwrap();
        assert_eq!(identity.devices().count(), 2);
        let vault = Id::new();
        revoke_device(&mut identity, &keystore, other.device_id()).unwrap();
        let mut set3 = record_set(
            &identity,
            Some(&set2.manifest),
            vec![Revocation {
                device_id: other.device_id(),
                vaults: vec![VaultBoundary {
                    vault_id: vault,
                    seq: 4,
                }],
            }],
        )
        .unwrap();
        set3.sign(&isk).unwrap();
        synced.accept(&[], set3.clone(), Some(&isk_pub)).unwrap();
        assert!(synced.is_revoked(other.device_id()));
        assert_eq!(synced.revoked_after(vault)[&other.device_id()], 4);
        assert_eq!(synced.revoked_after(Id::new())[&other.device_id()], 0);

        // Undoing it in a later page is refused (D73).
        let mut undo = lockdown_vault::identity_set::RecordSet::from_records(
            identity.profile(),
            identity.personas().cloned(),
            identity
                .devices()
                .filter(|d| d.device_id != other.device_id())
                .cloned()
                .chain([phone_record]),
            identity.envelopes().cloned(),
            identity.consents().cloned(),
            Some(&set3.manifest),
            Vec::new(),
            identity.device_id(),
        )
        .unwrap();
        undo.sign(&isk).unwrap();
        let err = synced.accept(&[], undo, Some(&isk_pub)).unwrap_err();
        assert!(err.to_string().contains("can't be undone"), "{err}");
        assert_eq!(synced.seq(), 3);
    }
}
