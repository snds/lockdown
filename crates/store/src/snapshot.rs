//! The file store for a build with no SQLite (the web vault): the same
//! contract and the same bytes as [`SqliteStore`](crate::SqliteStore), held
//! in memory and written whole to one file after every change.
//!
//! The file holds what the SQLite file holds and nothing more: each vault
//! record, item envelope and manifest head as the canonical CBOR the
//! device signed or sealed, and attachment blobs by hash. No key, no
//! plain field. In the web vault the file lives in the worker's in-memory
//! file system, so it outlives a lock (an unlock reopens the vault from it
//! without the network, as the desktop does from its SQLite file) and dies
//! with the tab. Reads decode every record and refuse the file as
//! tampered when one doesn't, as the SQLite store does per row.

use std::path::{Path, PathBuf};

use lockdown_vault::{Hash32, Id, Item, ManifestHead};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

use crate::memory::MemoryStore;
use crate::remembered::write_private;
use crate::{Store, StoreError, StoreResult, VaultRecord};

/// The snapshot format this build writes.
const SNAPSHOT_V: u16 = 1;

#[derive(Default, Serialize, Deserialize)]
struct Snapshot {
    v: u16,
    vaults: Vec<ByteBuf>,
    items: Vec<ByteBuf>,
    heads: Vec<ByteBuf>,
    blobs: Vec<(Hash32, ByteBuf)>,
}

/// A store kept as one file of sealed records.
#[derive(Debug)]
pub struct SnapshotStore {
    path: PathBuf,
    inner: MemoryStore,
}

fn backend(e: impl std::fmt::Display) -> StoreError {
    StoreError::Backend(e.to_string())
}

impl SnapshotStore {
    /// Opens the store at `path`, or starts an empty one if there is none.
    pub fn open(path: &Path) -> StoreResult<Self> {
        let mut inner = MemoryStore::new();
        match std::fs::read(path) {
            Ok(bytes) => {
                let snap: Snapshot = lockdown_crypto::cbor::from_slice(&bytes)
                    .map_err(|_| StoreError::Tampered("the vault file".into()))?;
                if snap.v > SNAPSHOT_V {
                    return Err(StoreError::SchemaTooNew {
                        found: u32::from(snap.v),
                        supported: u32::from(SNAPSHOT_V),
                    });
                }
                for b in &snap.vaults {
                    let record = VaultRecord::from_bytes(b)
                        .map_err(|_| StoreError::Tampered("a vault record".into()))?;
                    inner.put_vault(record)?;
                }
                for b in &snap.items {
                    let item =
                        Item::from_bytes(b).map_err(|_| StoreError::Tampered("an item".into()))?;
                    inner.put_item(item)?;
                }
                for b in &snap.heads {
                    let head = ManifestHead::from_bytes(b)
                        .map_err(|_| StoreError::Tampered("a manifest head".into()))?;
                    inner.put_head(head)?;
                }
                for (hash, b) in &snap.blobs {
                    inner.put_blob(*hash, b)?;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(backend(e)),
        }
        Ok(Self {
            path: path.to_path_buf(),
            inner,
        })
    }

    /// Where the file lives.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Writes the whole store to its file, in id order.
    fn flush(&self) -> StoreResult<()> {
        let mut snap = Snapshot {
            v: SNAPSHOT_V,
            ..Default::default()
        };
        for vault_id in self.inner.list_vaults()? {
            let record = self.inner.get_vault(vault_id)?;
            snap.vaults
                .push(ByteBuf::from(record.to_bytes().map_err(backend)?));
            for item_id in self.inner.list_items(vault_id)? {
                let item = self.inner.get_item(vault_id, item_id)?;
                snap.items
                    .push(ByteBuf::from(item.to_bytes().map_err(backend)?));
            }
            for head in self.inner.heads(vault_id)? {
                snap.heads
                    .push(ByteBuf::from(head.to_bytes().map_err(backend)?));
            }
        }
        for hash in self.inner.list_blobs()? {
            snap.blobs
                .push((hash, ByteBuf::from(self.inner.get_blob(hash)?)));
        }
        let bytes = lockdown_crypto::cbor::to_canonical(&snap).map_err(backend)?;
        write_private(&self.path, &bytes).map_err(backend)
    }
}

impl Store for SnapshotStore {
    fn put_vault(&mut self, record: VaultRecord) -> StoreResult<()> {
        self.inner.put_vault(record)?;
        self.flush()
    }

    fn get_vault(&self, vault_id: Id) -> StoreResult<VaultRecord> {
        self.inner.get_vault(vault_id)
    }

    fn list_vaults(&self) -> StoreResult<Vec<Id>> {
        self.inner.list_vaults()
    }

    fn put_item(&mut self, item: Item) -> StoreResult<()> {
        self.inner.put_item(item)?;
        self.flush()
    }

    fn get_item(&self, vault_id: Id, item_id: Id) -> StoreResult<Item> {
        self.inner.get_item(vault_id, item_id)
    }

    fn list_items(&self, vault_id: Id) -> StoreResult<Vec<Id>> {
        self.inner.list_items(vault_id)
    }

    fn remove_item(&mut self, vault_id: Id, item_id: Id) -> StoreResult<()> {
        self.inner.remove_item(vault_id, item_id)?;
        self.flush()
    }

    fn put_head(&mut self, head: ManifestHead) -> StoreResult<()> {
        self.inner.put_head(head)?;
        self.flush()
    }

    fn heads(&self, vault_id: Id) -> StoreResult<Vec<ManifestHead>> {
        self.inner.heads(vault_id)
    }

    fn remove_heads_after(&mut self, vault_id: Id, seq: u64) -> StoreResult<()> {
        self.inner.remove_heads_after(vault_id, seq)?;
        self.flush()
    }

    fn put_blob(&mut self, hash: Hash32, bytes: &[u8]) -> StoreResult<()> {
        self.inner.put_blob(hash, bytes)?;
        self.flush()
    }

    fn get_blob(&self, hash: Hash32) -> StoreResult<Vec<u8>> {
        self.inner.get_blob(hash)
    }

    fn has_blob(&self, hash: Hash32) -> StoreResult<bool> {
        self.inner.has_blob(hash)
    }

    fn remove_blob(&mut self, hash: Hash32) -> StoreResult<()> {
        self.inner.remove_blob(hash)?;
        self.flush()
    }

    fn list_blobs(&self) -> StoreResult<Vec<Hash32>> {
        self.inner.list_blobs()
    }

    /// One write of the file for the whole vault.
    fn save_all(
        &mut self,
        record: VaultRecord,
        items: Vec<Item>,
        heads: Vec<ManifestHead>,
    ) -> StoreResult<()> {
        self.inner.save_all(record, items, heads)?;
        self.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ecosystem_identity::SymmetricKey;
    use ecosystem_identity::keys::AppDataKey;
    use lockdown_vault::test_support::{sample_login_fields, sample_vault};
    use lockdown_vault::{ItemClass, ItemKind};

    #[test]
    fn a_vault_saved_reopens_from_the_file_and_holds_no_plain_field() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vault.store");
        let adk = AppDataKey::from_bytes([7; 32]);
        let mut vault = sample_vault();
        let id = vault
            .create_item(ItemKind::Login, ItemClass::Standard, sample_login_fields())
            .unwrap();
        let vault_id = vault.context().vault_id;
        let mut store = SnapshotStore::open(&path).unwrap();
        vault.save(&mut store, &adk).unwrap();

        let bytes = std::fs::read(&path).unwrap();
        for plain in [
            b"correct horse battery staple".as_slice(),
            b"accounts.example.com",
            b"me@example.com",
        ] {
            assert!(
                !bytes.windows(plain.len()).any(|w| w == plain),
                "the file holds no plain field"
            );
        }

        let again = SnapshotStore::open(&path).unwrap();
        assert_eq!(again.list_vaults().unwrap(), vec![vault_id]);
        assert_eq!(
            again.get_item(vault_id, id).unwrap(),
            store.get_item(vault_id, id).unwrap()
        );
        assert_eq!(
            again.heads(vault_id).unwrap(),
            store.heads(vault_id).unwrap()
        );
        assert_eq!(
            again.get_vault(vault_id).unwrap(),
            store.get_vault(vault_id).unwrap()
        );
    }

    #[test]
    fn a_damaged_file_is_refused_as_tampered() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vault.store");
        std::fs::write(&path, b"not cbor at all").unwrap();
        assert!(matches!(
            SnapshotStore::open(&path),
            Err(StoreError::Tampered(_))
        ));
    }
}
