//! The local store contract (03-architecture §2 `store` row; §11 q.1).
//!
//! The trait lives here, next to [`Vault`](crate::Vault), so
//! [`Vault::load`](crate::Vault::load) and [`Vault::save`](crate::Vault::save)
//! can take any store without the vault crate depending on a backend. The
//! backends (`MemoryStore`, `SqliteStore`) live in `lockdown-store`, which
//! re-exports everything in this module.
//!
//! Everything a store holds is ciphertext or public structure: item
//! envelopes (sealed fields, wrapped keys), manifest heads, and one
//! [`VaultRecord`] per vault naming where the vault hangs in the identity's
//! key tree. A store never sees a key or a plain field, so a plain SQLite
//! file is enough and nothing plaintext touches disk.

use serde::{Deserialize, Serialize};

use crate::item::Item;
use crate::keys::WrappedKey;
use crate::manifest::ManifestHead;
use crate::vault::VaultContext;
use crate::{Hash32, Id, Result};

/// What can go wrong in the store.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// The item, head or vault asked for is not there.
    #[error("{0} wasn't found in the local store")]
    NotFound(String),
    /// The backend failed.
    #[error("the local store failed: {0}")]
    Backend(String),
    /// A stored blob did not decode as the record it should be. Shown as
    /// tampering: a store never writes a blob it cannot read back.
    #[error("{0} in the local store didn't decode; the store may have been tampered with")]
    Tampered(String),
    /// The store was written by a newer schema than this build reads.
    #[error(
        "the local store uses schema {found}, newer than this app understands ({supported}); update the app"
    )]
    SchemaTooNew {
        /// What the file says.
        found: u32,
        /// What this build reads.
        supported: u32,
    },
    /// The feature exists in the plan but not in this build yet.
    #[error("not supported here: {0}")]
    Unsupported(String),
}

/// The store's result type.
pub type StoreResult<T> = std::result::Result<T, StoreError>;

/// Where a blob goes while it is sealed, chunk by chunk (D233): memory for
/// a store that keeps blobs in memory, a file being written for one that
/// keeps them on disk. [`Store::keep_blob`] takes it once its name is
/// known.
#[derive(Debug)]
pub enum BlobSink {
    /// Bytes in memory.
    Memory(Vec<u8>),
    /// A file being written, and where.
    File {
        /// The open file.
        file: std::fs::File,
        /// Its path, renamed into place on keep.
        path: std::path::PathBuf,
    },
}

impl BlobSink {
    /// A sink in memory.
    pub fn memory() -> Self {
        Self::Memory(Vec::new())
    }
}

impl std::io::Write for BlobSink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::Memory(v) => v.write(buf),
            Self::File { file, .. } => file.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Memory(_) => Ok(()),
            Self::File { file, .. } => file.flush(),
        }
    }
}

/// A blob being read, from memory or a file, so it can be hashed and then
/// opened chunk by chunk without being held whole.
#[derive(Debug)]
pub enum BlobSource {
    /// Bytes in memory.
    Memory(std::io::Cursor<Vec<u8>>),
    /// A file on disk.
    File(std::fs::File),
}

impl std::io::Read for BlobSource {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Memory(c) => c.read(buf),
            Self::File(f) => f.read(buf),
        }
    }
}

impl std::io::Seek for BlobSource {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        match self {
            Self::Memory(c) => c.seek(pos),
            Self::File(f) => f.seek(pos),
        }
    }
}

/// One vault as the store knows it: the public ids of its place in the key
/// tree and its Vault Key wrapped by the persona's Lockdown App Data Key.
/// Opening the vault needs that App Data Key from the identity and nothing
/// else (D11: same device, no kit).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct VaultRecord {
    /// Record format version.
    pub v: u16,
    /// Who and where the vault belongs to.
    pub context: VaultContext,
    /// The Vault Key under the App Data Key named in `context`.
    pub vault_key: WrappedKey,
}

/// The vault record format version this crate writes.
pub const VAULT_RECORD_V: u16 = 1;

impl VaultRecord {
    /// As canonical CBOR.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        Ok(lockdown_crypto::cbor::to_canonical(self)?)
    }

    /// From CBOR.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        Ok(lockdown_crypto::cbor::from_slice(bytes)?)
    }
}

/// The local store contract. Everything it holds is ciphertext or public
/// structure (ids, revisions, manifest heads). Each call stands alone; a
/// backend that can group writes does so inside [`Store::save_all`].
pub trait Store {
    /// Writes or replaces a vault record.
    fn put_vault(&mut self, record: VaultRecord) -> StoreResult<()>;
    /// Reads a vault record.
    fn get_vault(&self, vault_id: Id) -> StoreResult<VaultRecord>;
    /// Lists every vault in the store, in id order.
    fn list_vaults(&self) -> StoreResult<Vec<Id>>;
    /// Writes or replaces an item envelope.
    fn put_item(&mut self, item: Item) -> StoreResult<()>;
    /// Reads an item envelope.
    fn get_item(&self, vault_id: Id, item_id: Id) -> StoreResult<Item>;
    /// Lists the item ids in a vault, in id order.
    fn list_items(&self, vault_id: Id) -> StoreResult<Vec<Id>>;
    /// Removes an item's ciphertext after a purge (the keys are already gone).
    fn remove_item(&mut self, vault_id: Id, item_id: Id) -> StoreResult<()>;
    /// Appends a manifest head. Writing the same `seq` again replaces it.
    fn put_head(&mut self, head: ManifestHead) -> StoreResult<()>;
    /// Every manifest head for a vault, oldest first.
    fn heads(&self, vault_id: Id) -> StoreResult<Vec<ManifestHead>>;
    /// Removes every head after `seq`. Sync uses it when the server's chain
    /// replaces heads this device committed offline; nothing else does.
    fn remove_heads_after(&mut self, vault_id: Id, seq: u64) -> StoreResult<()>;
    /// Writes an attachment blob under its name (the BLAKE3 of its bytes;
    /// the caller computed it, the vault checks it on open). Writing a
    /// name again is a no-op. Blobs are ciphertext and keyed by content,
    /// so one table serves every vault.
    fn put_blob(&mut self, hash: Hash32, bytes: &[u8]) -> StoreResult<()>;
    /// Reads a blob.
    fn get_blob(&self, hash: Hash32) -> StoreResult<Vec<u8>>;
    /// Whether a blob is here.
    fn has_blob(&self, hash: Hash32) -> StoreResult<bool>;
    /// Removes a blob nothing readable refers to any more (its Attachment
    /// Key was shredded). Removing a name that isn't here is fine.
    fn remove_blob(&mut self, hash: Hash32) -> StoreResult<()>;
    /// Every blob name held, in hash order.
    fn list_blobs(&self) -> StoreResult<Vec<Hash32>>;
    /// Somewhere to write a blob as it is sealed (D233). The default is
    /// memory; a store on disk gives a file, so a large file's ciphertext
    /// is never whole in memory either.
    fn blob_sink(&mut self) -> StoreResult<BlobSink> {
        Ok(BlobSink::memory())
    }
    /// Keeps a sealed blob under its name, from the sink [`Self::blob_sink`]
    /// gave. Keeping a name already held is a no-op.
    fn keep_blob(&mut self, hash: Hash32, sink: BlobSink) -> StoreResult<()> {
        match sink {
            BlobSink::Memory(bytes) => self.put_blob(hash, &bytes),
            BlobSink::File { path, .. } => {
                let _ = std::fs::remove_file(&path);
                Err(StoreError::Unsupported(
                    "this store keeps blobs in memory, not as files".into(),
                ))
            }
        }
    }
    /// A blob to read in pieces. The default reads it whole.
    fn open_blob(&self, hash: Hash32) -> StoreResult<BlobSource> {
        Ok(BlobSource::Memory(std::io::Cursor::new(
            self.get_blob(hash)?,
        )))
    }
    /// The newest manifest head for a vault.
    fn latest_head(&self, vault_id: Id) -> StoreResult<ManifestHead> {
        self.heads(vault_id)?
            .pop()
            .ok_or_else(|| StoreError::NotFound(format!("a manifest for vault {vault_id}")))
    }
    /// Writes a whole vault in one go: its record, every item and every
    /// head. A backend with transactions makes this all-or-nothing; the
    /// default does the writes one by one.
    fn save_all(
        &mut self,
        record: VaultRecord,
        items: Vec<Item>,
        heads: Vec<ManifestHead>,
    ) -> StoreResult<()> {
        self.put_vault(record)?;
        for item in items {
            self.put_item(item)?;
        }
        for head in heads {
            self.put_head(head)?;
        }
        Ok(())
    }
}
