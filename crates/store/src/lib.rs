//! `lockdown-store`: the local store (03-architecture §2 `store` row; §11
//! open question 1, now decided).
//!
//! **The decision (q.1):** plain SQLite holding ciphertext only. One file
//! per device holds the item envelopes (sealed fields, wrapped keys), the
//! signed manifest chain and one record per vault, each as canonical CBOR
//! in a `BLOB`. No title, URL, username or key is ever in the file in the
//! clear, so there is nothing for whole-file encryption to add and nothing
//! for a lost laptop's disk image to leak beyond ids, sizes and timings.
//! The searchable index is built in memory after unlock and never written.
//!
//! - [`Store`] — the contract, re-exported from `lockdown-vault` so a
//!   [`Vault`](lockdown_vault::Vault) can load from and save to any backend
//! - [`SqliteStore`] — the file-backed store: WAL mode, a schema version
//!   table, every write to one vault in one transaction
//! - [`MemoryStore`] — the in-memory store for tests, the offscreen
//!   WASM document and the web vault
//! - [`SnapshotStore`] — the same records as the SQLite file, kept in
//!   memory and written whole to one file on every change, for a build
//!   with no SQLite
//! - [`DeviceStore`] — the store a device keeps its vaults in:
//!   [`SqliteStore`] with the default `sqlite` feature, [`SnapshotStore`]
//!   without it (the web vault, docs/plan/17-web-vault.md)
//! - [`HeadMemory`] — what this device remembers about each vault's newest
//!   accepted head, kept in a separate file so a vault file put back from
//!   an old copy is caught ("Rollback attempt blocked")
//! - [`SyncedIdentity`] — the identity's signed record set as last accepted
//!   from the server: every other machine's devices, and where a revoked
//!   one's writes stop, so their heads and versions verify here too
//! - [`TransparencyMemory`] — per server, the pinned log key and the newest
//!   signed tree head this device verified, so a shrunken or forked log is
//!   caught on the next sync

pub mod memory;
pub mod remembered;
pub mod snapshot;
#[cfg(feature = "sqlite")]
pub mod sqlite;
pub mod synced_identity;
pub mod transparency;

pub use lockdown_vault::store::{Store, StoreError, StoreResult, VAULT_RECORD_V, VaultRecord};
pub use memory::MemoryStore;
pub use remembered::{HeadMemory, RememberedHead, write_private};
pub use snapshot::SnapshotStore;
#[cfg(feature = "sqlite")]
pub use sqlite::{SCHEMA_VERSION, SqliteStore};
pub use synced_identity::{AcceptError, SyncedIdentity};
pub use transparency::{ServerView, TransparencyMemory};

/// The store's result type, for callers that used the old name.
pub type Result<T> = StoreResult<T>;

/// The store a device keeps its vaults in: the SQLite file.
#[cfg(feature = "sqlite")]
pub type DeviceStore = SqliteStore;

/// The store a device keeps its vaults in, in a build with no SQLite: the
/// same records in one snapshot file (in the web vault, a file in the
/// worker's memory, which outlives a lock and dies with the tab).
#[cfg(not(feature = "sqlite"))]
pub type DeviceStore = SnapshotStore;

/// Opens the device's store at `path`: the SQLite file, made when missing.
#[cfg(feature = "sqlite")]
pub fn open_device_store(path: &std::path::Path) -> StoreResult<DeviceStore> {
    SqliteStore::open(path)
}

/// Opens the device's store beside `path` (where the SQLite file would
/// be): the snapshot file of the same name ending `.store`, empty when
/// missing.
#[cfg(not(feature = "sqlite"))]
pub fn open_device_store(path: &std::path::Path) -> StoreResult<DeviceStore> {
    SnapshotStore::open(&path.with_extension("store"))
}
