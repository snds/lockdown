//! The in-memory store: tests, the CLI's dry runs, the offscreen WASM
//! document. Same contract as the file, nothing on disk.

use std::collections::BTreeMap;

use lockdown_vault::{Hash32, Id, Item, ManifestHead};

use crate::{Store, StoreError, StoreResult, VaultRecord};

/// An in-memory store. `Clone` is there so a test can keep an old copy and
/// put it back, the way a restored backup would.
#[derive(Default, Debug, Clone)]
pub struct MemoryStore {
    vaults: BTreeMap<Id, VaultRecord>,
    items: BTreeMap<(Id, Id), Item>,
    heads: BTreeMap<(Id, u64), ManifestHead>,
    blobs: BTreeMap<Hash32, Vec<u8>>,
}

impl MemoryStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Store for MemoryStore {
    fn put_vault(&mut self, record: VaultRecord) -> StoreResult<()> {
        self.vaults.insert(record.context.vault_id, record);
        Ok(())
    }

    fn get_vault(&self, vault_id: Id) -> StoreResult<VaultRecord> {
        self.vaults
            .get(&vault_id)
            .cloned()
            .ok_or_else(|| StoreError::NotFound(format!("vault {vault_id}")))
    }

    fn list_vaults(&self) -> StoreResult<Vec<Id>> {
        Ok(self.vaults.keys().copied().collect())
    }

    fn put_item(&mut self, item: Item) -> StoreResult<()> {
        self.items.insert((item.vault_id, item.item_id), item);
        Ok(())
    }

    fn get_item(&self, vault_id: Id, item_id: Id) -> StoreResult<Item> {
        self.items
            .get(&(vault_id, item_id))
            .cloned()
            .ok_or_else(|| StoreError::NotFound(format!("item {item_id}")))
    }

    fn list_items(&self, vault_id: Id) -> StoreResult<Vec<Id>> {
        Ok(self
            .items
            .keys()
            .filter(|(v, _)| *v == vault_id)
            .map(|(_, i)| *i)
            .collect())
    }

    fn remove_item(&mut self, vault_id: Id, item_id: Id) -> StoreResult<()> {
        self.items
            .remove(&(vault_id, item_id))
            .map(|_| ())
            .ok_or_else(|| StoreError::NotFound(format!("item {item_id}")))
    }

    fn put_head(&mut self, head: ManifestHead) -> StoreResult<()> {
        self.heads.insert((head.vault_id, head.seq), head);
        Ok(())
    }

    fn heads(&self, vault_id: Id) -> StoreResult<Vec<ManifestHead>> {
        Ok(self
            .heads
            .range((vault_id, 0)..=(vault_id, u64::MAX))
            .map(|(_, h)| h.clone())
            .collect())
    }

    fn remove_heads_after(&mut self, vault_id: Id, seq: u64) -> StoreResult<()> {
        self.heads.retain(|(v, s), _| *v != vault_id || *s <= seq);
        Ok(())
    }

    fn put_blob(&mut self, hash: Hash32, bytes: &[u8]) -> StoreResult<()> {
        self.blobs.entry(hash).or_insert_with(|| bytes.to_vec());
        Ok(())
    }

    fn get_blob(&self, hash: Hash32) -> StoreResult<Vec<u8>> {
        self.blobs
            .get(&hash)
            .cloned()
            .ok_or_else(|| StoreError::NotFound(format!("blob {hash}")))
    }

    fn has_blob(&self, hash: Hash32) -> StoreResult<bool> {
        Ok(self.blobs.contains_key(&hash))
    }

    fn remove_blob(&mut self, hash: Hash32) -> StoreResult<()> {
        self.blobs.remove(&hash);
        Ok(())
    }

    fn list_blobs(&self) -> StoreResult<Vec<Hash32>> {
        Ok(self.blobs.keys().copied().collect())
    }
}
