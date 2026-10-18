//! What this device remembers, so a rollback is caught (02-model §8:
//! "Every device remembers the newest sequence it has seen"; 04-security
//! §7).
//!
//! The memory lives in its own small file, **not** in the vault file. If it
//! sat inside the SQLite file, putting back an old copy of that file would
//! put back the memory too and the rollback would pass. Here it holds, per
//! vault, the newest manifest `seq` and head hash this device accepted,
//! and the identity store's own `Head` (the identity crate leaves that
//! memory to the app, D26). Written through a temporary file and a rename,
//! readable only by the user, so a crash leaves the old memory or the new
//! one, never half of one. It is per device and never syncs.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ecosystem_identity::Head;
use lockdown_vault::{Hash32, Id};
use serde::{Deserialize, Serialize};

use crate::{StoreError, StoreResult};

const MEMORY_V: u16 = 1;

/// The newest head of one vault this device accepted.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct RememberedHead {
    /// The manifest `seq`.
    pub seq: u64,
    /// The head's hash.
    pub hash: Hash32,
}

#[derive(Default, Serialize, Deserialize)]
struct MemoryFile {
    v: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    identity: Option<Head>,
    #[serde(default)]
    vaults: BTreeMap<Id, RememberedHead>,
    /// Per vault, the `seq` of the first head with a post-quantum
    /// signature the server confirmed to this device: from there on every
    /// head by the identity's devices must carry one
    /// (docs/plan/28-pq-signatures.md). Additive; absent before.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pq_since: BTreeMap<Id, u64>,
}

/// The device's memory of accepted heads, backed by one file.
#[derive(Debug)]
pub struct HeadMemory {
    path: PathBuf,
    file: MemoryFile,
}

impl std::fmt::Debug for MemoryFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemoryFile")
            .field("identity", &self.identity.map(|h| h.seq))
            .field("vaults", &self.vaults.len())
            .finish()
    }
}

impl HeadMemory {
    /// Opens the memory at `path`, or starts an empty one if there is none.
    pub fn open(path: &Path) -> StoreResult<Self> {
        let file = match std::fs::read(path) {
            Ok(bytes) => {
                let file: MemoryFile = lockdown_crypto::cbor::from_slice(&bytes)
                    .map_err(|_| StoreError::Tampered("the head memory".into()))?;
                if file.v > MEMORY_V {
                    return Err(StoreError::SchemaTooNew {
                        found: u32::from(file.v),
                        supported: u32::from(MEMORY_V),
                    });
                }
                file
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => MemoryFile {
                v: MEMORY_V,
                ..Default::default()
            },
            Err(e) => return Err(StoreError::Backend(e.to_string())),
        };
        Ok(Self {
            path: path.to_path_buf(),
            file,
        })
    }

    /// Where the file lives.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The newest head this device accepted for a vault, if any.
    pub fn vault(&self, vault_id: Id) -> Option<RememberedHead> {
        self.file.vaults.get(&vault_id).copied()
    }

    /// The newest manifest sequence accepted for a vault, or zero.
    pub fn last_accepted_seq(&self, vault_id: Id) -> u64 {
        self.vault(vault_id).map(|h| h.seq).unwrap_or(0)
    }

    /// Records a vault head this device accepted and writes the file. A
    /// head behind the one remembered is refused: memory only moves forward.
    pub fn remember_vault(&mut self, vault_id: Id, head: RememberedHead) -> StoreResult<()> {
        if let Some(seen) = self.vault(vault_id)
            && (head.seq < seen.seq || (head.seq == seen.seq && head.hash != seen.hash))
        {
            return Err(StoreError::Backend(format!(
                "refusing to remember manifest {} for vault {vault_id}: {} is already remembered",
                head.seq, seen.seq
            )));
        }
        self.file.vaults.insert(vault_id, head);
        self.write()
    }

    /// Replaces what is remembered for a vault, forward or back. Only for
    /// sync, after it has verified the server's chain from the last seq
    /// the server confirmed and replaced this device's never-pushed heads
    /// with it: the new head can then carry a lower `seq` than a local
    /// provisional one did. Every other caller uses [`Self::remember_vault`].
    pub fn replace_vault(&mut self, vault_id: Id, head: RememberedHead) -> StoreResult<()> {
        self.file.vaults.insert(vault_id, head);
        self.write()
    }

    /// The `seq` from which this vault's heads must carry a post-quantum
    /// signature, if this device saw one confirmed.
    pub fn pq_since(&self, vault_id: Id) -> Option<u64> {
        self.file.pq_since.get(&vault_id).copied()
    }

    /// Records the first confirmed post-quantum head of a vault and writes
    /// the file. Only ever moves down (an earlier first head), never up or
    /// away: the requirement once seen stays.
    pub fn remember_pq_since(&mut self, vault_id: Id, seq: u64) -> StoreResult<()> {
        if self.pq_since(vault_id).is_some_and(|seen| seen <= seq) {
            return Ok(());
        }
        self.file.pq_since.insert(vault_id, seq);
        self.write()
    }

    /// The identity store head this device last accepted, for
    /// `Identity::attach`.
    pub fn identity(&self) -> Option<Head> {
        self.file.identity
    }

    /// Records the identity store head and writes the file.
    pub fn remember_identity(&mut self, head: Head) -> StoreResult<()> {
        if let Some(seen) = self.file.identity
            && seen.is_behind(&head)
        {
            return Err(StoreError::Backend(format!(
                "refusing to remember identity manifest {}: {} is already remembered",
                head.seq, seen.seq
            )));
        }
        self.file.identity = Some(head);
        self.write()
    }

    fn write(&self) -> StoreResult<()> {
        let bytes = lockdown_crypto::cbor::to_canonical(&self.file)
            .map_err(|e| StoreError::Backend(e.to_string()))?;
        write_private(&self.path, &bytes).map_err(|e| StoreError::Backend(e.to_string()))
    }
}

/// Writes a file readable only by the user, through a temporary file and a
/// rename. For Lockdown's own small files (memory, known devices, the
/// sync state); the vault file is SQLite's.
pub fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        create_private_dir(dir)?;
    }
    let tmp = path.with_extension("tmp");
    {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&tmp)?;
        std::io::Write::write_all(&mut file, bytes)?;
        file.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}

/// Creates a directory only the user can enter.
pub(crate) fn create_private_dir(path: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_only_moves_forward_and_survives_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("remembered.cbor");
        let vault = Id::new();
        let mut memory = HeadMemory::open(&path).unwrap();
        assert_eq!(memory.last_accepted_seq(vault), 0);
        let h2 = RememberedHead {
            seq: 2,
            hash: Hash32([2; 32]),
        };
        memory.remember_vault(vault, h2).unwrap();
        memory.remember_identity(Head::new(3, [3; 32])).unwrap();
        drop(memory);

        let mut memory = HeadMemory::open(&path).unwrap();
        assert_eq!(memory.vault(vault), Some(h2));
        assert_eq!(memory.identity().unwrap().seq, 3);
        // Behind: refused. Same seq, other hash: refused. Forward: fine.
        assert!(
            memory
                .remember_vault(
                    vault,
                    RememberedHead {
                        seq: 1,
                        hash: Hash32([1; 32])
                    }
                )
                .is_err()
        );
        assert!(
            memory
                .remember_vault(
                    vault,
                    RememberedHead {
                        seq: 2,
                        hash: Hash32([9; 32])
                    }
                )
                .is_err()
        );
        memory.remember_vault(vault, h2).unwrap();
        memory
            .remember_vault(
                vault,
                RememberedHead {
                    seq: 3,
                    hash: Hash32([3; 32]),
                },
            )
            .unwrap();
        assert!(memory.remember_identity(Head::new(2, [2; 32])).is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    #[test]
    fn the_first_post_quantum_head_is_remembered_and_never_forgotten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("remembered.cbor");
        let (vault, other) = (Id::new(), Id::new());
        let mut memory = HeadMemory::open(&path).unwrap();
        assert_eq!(memory.pq_since(vault), None);
        memory.remember_pq_since(vault, 7).unwrap();
        memory.remember_pq_since(vault, 9).unwrap();
        assert_eq!(memory.pq_since(vault), Some(7), "never moves up");
        memory.remember_pq_since(vault, 4).unwrap();
        drop(memory);
        let memory = HeadMemory::open(&path).unwrap();
        assert_eq!(memory.pq_since(vault), Some(4));
        assert_eq!(memory.pq_since(other), None);
    }
}
