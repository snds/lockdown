//! `lockdown-store`: the local store (03-architecture §2 `store` row and
//! §11 open question 1).
//!
//! **Status: skeleton.** The [`Store`] trait is the contract every shell's
//! storage must meet; [`MemoryStore`] is the in-memory implementation that
//! tests and the CLI use. The SQLite-backed store is not chosen yet
//! (SQLCipher-style whole-file encryption versus plain SQLite holding only
//! ciphertext, with the index built in memory; the plan leans to the
//! second). Whatever lands, it stores **ciphertext only**: the store never
//! sees a key or a plain field.

use std::collections::BTreeMap;

use lockdown_vault::{Id, Item, ManifestHead};

/// What can go wrong in the store.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// The item or head asked for is not there.
    #[error("{0} wasn't found in the local store")]
    NotFound(String),
    /// The backend failed.
    #[error("the local store failed: {0}")]
    Backend(String),
    /// The feature exists in the plan but not in this build yet.
    #[error("not supported here: {0}")]
    Unsupported(String),
}

/// The store's result type.
pub type Result<T> = std::result::Result<T, StoreError>;

/// The local store contract. Everything it holds is ciphertext or public
/// structure (ids, revisions, manifest heads).
pub trait Store {
    /// Writes or replaces an item envelope.
    fn put_item(&mut self, item: Item) -> Result<()>;
    /// Reads an item envelope.
    fn get_item(&self, vault_id: Id, item_id: Id) -> Result<Item>;
    /// Lists the item ids in a vault.
    fn list_items(&self, vault_id: Id) -> Result<Vec<Id>>;
    /// Removes an item's ciphertext after a purge (the keys are already gone).
    fn remove_item(&mut self, vault_id: Id, item_id: Id) -> Result<()>;
    /// Appends a manifest head.
    fn put_head(&mut self, head: ManifestHead) -> Result<()>;
    /// The newest manifest head for a vault.
    fn latest_head(&self, vault_id: Id) -> Result<ManifestHead>;
}

/// An in-memory store, for tests, the CLI and the offscreen WASM document.
#[derive(Default, Debug)]
pub struct MemoryStore {
    items: BTreeMap<(Id, Id), Item>,
    heads: BTreeMap<Id, Vec<ManifestHead>>,
}

impl MemoryStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Store for MemoryStore {
    fn put_item(&mut self, item: Item) -> Result<()> {
        self.items.insert((item.vault_id, item.item_id), item);
        Ok(())
    }

    fn get_item(&self, vault_id: Id, item_id: Id) -> Result<Item> {
        self.items
            .get(&(vault_id, item_id))
            .cloned()
            .ok_or_else(|| StoreError::NotFound(format!("item {item_id}")))
    }

    fn list_items(&self, vault_id: Id) -> Result<Vec<Id>> {
        Ok(self
            .items
            .keys()
            .filter(|(v, _)| *v == vault_id)
            .map(|(_, i)| *i)
            .collect())
    }

    fn remove_item(&mut self, vault_id: Id, item_id: Id) -> Result<()> {
        self.items
            .remove(&(vault_id, item_id))
            .map(|_| ())
            .ok_or_else(|| StoreError::NotFound(format!("item {item_id}")))
    }

    fn put_head(&mut self, head: ManifestHead) -> Result<()> {
        self.heads.entry(head.vault_id).or_default().push(head);
        Ok(())
    }

    fn latest_head(&self, vault_id: Id) -> Result<ManifestHead> {
        self.heads
            .get(&vault_id)
            .and_then(|h| h.last())
            .cloned()
            .ok_or_else(|| StoreError::NotFound(format!("a manifest for vault {vault_id}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lockdown_vault::test_support::sample_item;

    #[test]
    fn memory_store_round_trips_and_lists_per_vault() {
        let mut store = MemoryStore::new();
        let a = sample_item();
        let b = sample_item();
        let (va, ia) = (a.vault_id, a.item_id);
        store.put_item(a.clone()).unwrap();
        store.put_item(b.clone()).unwrap();
        assert_eq!(store.get_item(va, ia).unwrap(), a);
        assert_eq!(store.list_items(va).unwrap(), vec![ia]);
        store.remove_item(va, ia).unwrap();
        assert!(matches!(
            store.get_item(va, ia),
            Err(StoreError::NotFound(_))
        ));
        assert!(matches!(
            store.latest_head(va),
            Err(StoreError::NotFound(_))
        ));
    }
}
