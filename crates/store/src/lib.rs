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
//! - [`MemoryStore`] — the in-memory store for tests and the offscreen
//!   WASM document
//! - [`HeadMemory`] — what this device remembers about each vault's newest
//!   accepted head, kept in a separate file so a vault file put back from
//!   an old copy is caught ("Rollback attempt blocked")
//! - [`SyncedIdentity`] — the identity's signed record set as last accepted
//!   from the server: every other machine's devices, and where a revoked
//!   one's writes stop, so their heads and versions verify here too

pub mod memory;
pub mod remembered;
pub mod sqlite;
pub mod synced_identity;

pub use lockdown_vault::store::{Store, StoreError, StoreResult, VAULT_RECORD_V, VaultRecord};
pub use memory::MemoryStore;
pub use remembered::{HeadMemory, RememberedHead, write_private};
pub use sqlite::{SCHEMA_VERSION, SqliteStore};
pub use synced_identity::{AcceptError, SyncedIdentity};

/// The store's result type, for callers that used the old name.
pub type Result<T> = StoreResult<T>;
