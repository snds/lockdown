//! The file-backed store: one plain SQLite file holding ciphertext only.
//!
//! ```text
//! schema_version (version)                     one row; readers refuse a newer one
//! vaults         (vault_id, body)              VaultRecord as canonical CBOR
//! items          (vault_id, item_id, body)     Item envelope as canonical CBOR
//! heads          (vault_id, seq, body)         ManifestHead as canonical CBOR
//! ```
//!
//! What the file reveals to someone who copies it: vault, item and device
//! ids, manifest sequence numbers and timestamps, ciphertext sizes. What
//! it never holds: a key, a title, a URL, a username, a field value. The
//! Vault Key in `vaults` is wrapped under the persona's App Data Key,
//! which lives in the identity store under the device keystore, so the
//! file alone opens nothing.
//!
//! WAL mode, so a reader never blocks a writer and a crash mid-write
//! leaves the last committed state. `synchronous=FULL` because a password
//! manager would rather be slow than lose a save. `secure_delete` so a
//! removed blob is overwritten, on top of the crypto-shredding that makes
//! it unreadable anyway. The file and its directory are created readable
//! only by the user; SQLite gives the `-wal` and `-shm` files the same mode.

use std::path::{Path, PathBuf};

use lockdown_vault::{Id, Item, ManifestHead};
use rusqlite::{Connection, OptionalExtension, params};

use crate::remembered::{create_private_dir, write_private};
use crate::{Store, StoreError, StoreResult, VaultRecord};

/// The schema this build writes and the highest it reads.
pub const SCHEMA_VERSION: u32 = 1;

/// A store in one SQLite file.
pub struct SqliteStore {
    path: PathBuf,
    conn: Connection,
}

impl std::fmt::Debug for SqliteStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SqliteStore({})", self.path.display())
    }
}

fn backend(e: rusqlite::Error) -> StoreError {
    StoreError::Backend(e.to_string())
}

impl SqliteStore {
    /// Opens the store at `path`, creating the file and its schema if they
    /// are not there. Refuses a file written by a newer schema.
    pub fn open(path: &Path) -> StoreResult<Self> {
        if !path.exists() {
            // An empty file with the right mode; SQLite fills it in.
            write_private(path, &[]).map_err(|e| StoreError::Backend(e.to_string()))?;
        } else if let Some(dir) = path.parent() {
            create_private_dir(dir).map_err(|e| StoreError::Backend(e.to_string()))?;
        }
        let conn = Connection::open(path).map_err(backend)?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(backend)?;
        conn.pragma_update(None, "synchronous", "FULL")
            .map_err(backend)?;
        conn.pragma_update(None, "secure_delete", "ON")
            .map_err(backend)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS vaults (
                 vault_id TEXT PRIMARY KEY NOT NULL,
                 body     BLOB NOT NULL
             );
             CREATE TABLE IF NOT EXISTS items (
                 vault_id TEXT NOT NULL,
                 item_id  TEXT NOT NULL,
                 body     BLOB NOT NULL,
                 PRIMARY KEY (vault_id, item_id)
             );
             CREATE TABLE IF NOT EXISTS heads (
                 vault_id TEXT NOT NULL,
                 seq      INTEGER NOT NULL,
                 body     BLOB NOT NULL,
                 PRIMARY KEY (vault_id, seq)
             );",
        )
        .map_err(backend)?;
        let found: Option<u32> = conn
            .query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))
            .map_err(backend)?;
        match found {
            None => {
                conn.execute(
                    "INSERT INTO schema_version (version) VALUES (?1)",
                    params![SCHEMA_VERSION],
                )
                .map_err(backend)?;
            }
            Some(v) if v > SCHEMA_VERSION => {
                return Err(StoreError::SchemaTooNew {
                    found: v,
                    supported: SCHEMA_VERSION,
                });
            }
            Some(_) => {}
        }
        Ok(Self {
            path: path.to_path_buf(),
            conn,
        })
    }

    /// Where the file lives.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The schema version the file carries.
    pub fn schema_version(&self) -> StoreResult<u32> {
        self.conn
            .query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))
            .map_err(backend)
    }

    fn put_vault_in(conn: &Connection, record: &VaultRecord) -> StoreResult<()> {
        let body = record
            .to_bytes()
            .map_err(|e| StoreError::Backend(e.to_string()))?;
        conn.execute(
            "INSERT OR REPLACE INTO vaults (vault_id, body) VALUES (?1, ?2)",
            params![record.context.vault_id.to_string(), body],
        )
        .map_err(backend)?;
        Ok(())
    }

    fn put_item_in(conn: &Connection, item: &Item) -> StoreResult<()> {
        let body = item
            .to_bytes()
            .map_err(|e| StoreError::Backend(e.to_string()))?;
        conn.execute(
            "INSERT OR REPLACE INTO items (vault_id, item_id, body) VALUES (?1, ?2, ?3)",
            params![item.vault_id.to_string(), item.item_id.to_string(), body],
        )
        .map_err(backend)?;
        Ok(())
    }

    fn put_head_in(conn: &Connection, head: &ManifestHead) -> StoreResult<()> {
        let body = head
            .to_bytes()
            .map_err(|e| StoreError::Backend(e.to_string()))?;
        let seq = i64::try_from(head.seq)
            .map_err(|_| StoreError::Backend("a manifest sequence past i64".into()))?;
        conn.execute(
            "INSERT OR REPLACE INTO heads (vault_id, seq, body) VALUES (?1, ?2, ?3)",
            params![head.vault_id.to_string(), seq, body],
        )
        .map_err(backend)?;
        Ok(())
    }
}

fn parse_id(text: String, what: &str) -> StoreResult<Id> {
    Id::parse(&text).map_err(|_| StoreError::Tampered(format!("{what} id '{text}'")))
}

impl Store for SqliteStore {
    fn put_vault(&mut self, record: VaultRecord) -> StoreResult<()> {
        Self::put_vault_in(&self.conn, &record)
    }

    fn get_vault(&self, vault_id: Id) -> StoreResult<VaultRecord> {
        let body: Option<Vec<u8>> = self
            .conn
            .query_row(
                "SELECT body FROM vaults WHERE vault_id = ?1",
                params![vault_id.to_string()],
                |r| r.get(0),
            )
            .optional()
            .map_err(backend)?;
        let body = body.ok_or_else(|| StoreError::NotFound(format!("vault {vault_id}")))?;
        VaultRecord::from_bytes(&body)
            .map_err(|_| StoreError::Tampered(format!("the record of vault {vault_id}")))
    }

    fn list_vaults(&self) -> StoreResult<Vec<Id>> {
        let mut stmt = self
            .conn
            .prepare("SELECT vault_id FROM vaults ORDER BY vault_id")
            .map_err(backend)?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(backend)?;
        rows.map(|r| parse_id(r.map_err(backend)?, "a vault"))
            .collect()
    }

    fn put_item(&mut self, item: Item) -> StoreResult<()> {
        Self::put_item_in(&self.conn, &item)
    }

    fn get_item(&self, vault_id: Id, item_id: Id) -> StoreResult<Item> {
        let body: Option<Vec<u8>> = self
            .conn
            .query_row(
                "SELECT body FROM items WHERE vault_id = ?1 AND item_id = ?2",
                params![vault_id.to_string(), item_id.to_string()],
                |r| r.get(0),
            )
            .optional()
            .map_err(backend)?;
        let body = body.ok_or_else(|| StoreError::NotFound(format!("item {item_id}")))?;
        Item::from_bytes(&body).map_err(|_| StoreError::Tampered(format!("item {item_id}")))
    }

    fn list_items(&self, vault_id: Id) -> StoreResult<Vec<Id>> {
        let mut stmt = self
            .conn
            .prepare("SELECT item_id FROM items WHERE vault_id = ?1 ORDER BY item_id")
            .map_err(backend)?;
        let rows = stmt
            .query_map(params![vault_id.to_string()], |r| r.get::<_, String>(0))
            .map_err(backend)?;
        rows.map(|r| parse_id(r.map_err(backend)?, "an item"))
            .collect()
    }

    fn remove_item(&mut self, vault_id: Id, item_id: Id) -> StoreResult<()> {
        let changed = self
            .conn
            .execute(
                "DELETE FROM items WHERE vault_id = ?1 AND item_id = ?2",
                params![vault_id.to_string(), item_id.to_string()],
            )
            .map_err(backend)?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("item {item_id}")));
        }
        Ok(())
    }

    fn put_head(&mut self, head: ManifestHead) -> StoreResult<()> {
        Self::put_head_in(&self.conn, &head)
    }

    fn heads(&self, vault_id: Id) -> StoreResult<Vec<ManifestHead>> {
        let mut stmt = self
            .conn
            .prepare("SELECT seq, body FROM heads WHERE vault_id = ?1 ORDER BY seq")
            .map_err(backend)?;
        let rows = stmt
            .query_map(params![vault_id.to_string()], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?))
            })
            .map_err(backend)?;
        let mut out = Vec::new();
        for row in rows {
            let (seq, body) = row.map_err(backend)?;
            let head = ManifestHead::from_bytes(&body)
                .map_err(|_| StoreError::Tampered(format!("manifest {seq} of vault {vault_id}")))?;
            if i64::try_from(head.seq) != Ok(seq) || head.vault_id != vault_id {
                return Err(StoreError::Tampered(format!(
                    "manifest {seq} of vault {vault_id}"
                )));
            }
            out.push(head);
        }
        Ok(out)
    }

    fn remove_heads_after(&mut self, vault_id: Id, seq: u64) -> StoreResult<()> {
        let seq = i64::try_from(seq)
            .map_err(|_| StoreError::Backend("a manifest sequence past i64".into()))?;
        self.conn
            .execute(
                "DELETE FROM heads WHERE vault_id = ?1 AND seq > ?2",
                params![vault_id.to_string(), seq],
            )
            .map_err(backend)?;
        Ok(())
    }

    fn save_all(
        &mut self,
        record: VaultRecord,
        items: Vec<Item>,
        heads: Vec<ManifestHead>,
    ) -> StoreResult<()> {
        let tx = self.conn.transaction().map_err(backend)?;
        Self::put_vault_in(&tx, &record)?;
        for item in &items {
            Self::put_item_in(&tx, item)?;
        }
        for head in &heads {
            Self::put_head_in(&tx, head)?;
        }
        tx.commit().map_err(backend)
    }
}
