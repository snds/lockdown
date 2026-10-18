//! What the server keeps, behind [`Storage`]: accounts (login name, profile
//! id, OPAQUE password file, the pinned Identity Signing Key), the devices
//! each account logged in with, the account's identity record set (the
//! chain of identity manifests, the records they list, and the revocations
//! the newest one carries), and per vault the record, the item envelopes
//! with their revisions and the signed head chain. All of it is ciphertext
//! or signed public structure (05-self-host §2); the server holds no key
//! and reads no field.
//!
//! [`SqliteStorage`] is the one backend today: a single file, WAL mode,
//! every rule that must hold across rows (the head chain, the item
//! revision) checked inside one transaction.
//!
//! ```text
//! schema_version    (version)
//! accounts          (account_id, login UNIQUE, profile_id, password_file, created_at,
//!                    identity_seq, identity_hash, isk_pub)
//! devices           (account_id, device_id, device_pub, first_seen)
//! identity_heads    (account_id, seq, hash, body)
//! identity_records  (account_id, path, hash, body)
//! revoked_devices   (account_id, device_id, vault_id, boundary_seq)
//! vaults            (vault_id, account_id, record, head_seq, head_hash, item_rev)
//! heads             (vault_id, seq, hash, body)
//! items             (vault_id, item_id, rev, body)
//! blobs             (vault_id, hash, size, body, uploaded_at, unreferenced_at)
//! blob_refs         (vault_id, item_id, hash)
//! kt_log            (idx, account_id, leaf_hash, body)
//! ```
//!
//! Schema 1 kept one last-write-wins identity backup per account
//! (`identity_backups`, D59); schema 2 replaces it with the record set
//! (D71); schema 3 adds attachment blobs (02-model §6): content-addressed
//! ciphertext per vault, and which items name each blob, so a blob nothing
//! names any more can be collected after a grace period, and the
//! key-transparency log (04-security §6): one row per leaf, appended inside
//! the transaction that accepts an identity manifest, a vault head or a
//! new device, never updated or deleted. An old file is upgraded in place:
//! the new tables are added and the old ones left alone.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use lockdown_sync::wire::{ErrorCode, HeadAccepted, HeadRefused};
use lockdown_vault::identity_set::{IdentityManifest, RecordRef};
use lockdown_vault::transparency::{LogEntry, LogLeaf};
use lockdown_vault::{Hash32, Id, Item, ManifestHead};
use rusqlite::{Connection, OptionalExtension, params};

/// The schema this build writes and the highest it reads.
pub const SCHEMA_VERSION: u32 = 3;

/// The marker row in `revoked_devices` that says "revoked at all",
/// beside one row per vault boundary.
const REVOKED_MARKER: &str = "*";

/// What can go wrong in storage.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// Registration: the login name is already an account.
    #[error("that login name is already an account")]
    LoginTaken,
    /// A device id already registered with a different key.
    #[error("device {0} is already registered with another key")]
    DeviceKeyMismatch(Id),
    /// A vault that belongs to another account.
    #[error("vault {0} belongs to another account")]
    Forbidden(Id),
    /// Not there.
    #[error("{0} wasn't found")]
    NotFound(String),
    /// The database said no.
    #[error("the server's database failed: {0}")]
    Backend(String),
    /// The database was written by a newer schema than this build reads.
    #[error(
        "the server's database uses schema {found}, newer than this build understands ({supported})"
    )]
    SchemaTooNew {
        /// What the file says.
        found: u32,
        /// What this build reads.
        supported: u32,
    },
}

/// The storage result type.
pub type Result<T> = std::result::Result<T, StorageError>;

/// One account.
#[derive(Clone, Debug)]
pub struct Account {
    /// The server's id for it.
    pub account_id: Id,
    /// The login name.
    pub login: String,
    /// The profile it was registered for.
    pub profile_id: Id,
    /// The OPAQUE password file.
    pub password_file: Vec<u8>,
}

/// One vault as the server knows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultRow {
    /// The vault.
    pub vault_id: Id,
    /// Who owns it.
    pub account_id: Id,
    /// Its record bytes.
    pub record: Vec<u8>,
    /// The newest accepted head's `seq`; zero before the first.
    pub head_seq: u64,
    /// The newest accepted head's hash.
    pub head_hash: Option<Hash32>,
    /// The newest item revision assigned.
    pub item_rev: u64,
}

/// One head as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeadRow {
    /// Its `seq`.
    pub seq: u64,
    /// Its hash.
    pub hash: Hash32,
    /// The signed bytes.
    pub body: Vec<u8>,
}

/// Where an account's identity chain stands.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct IdentityState {
    /// The newest accepted manifest's `seq`; zero before the first.
    pub seq: u64,
    /// Its hash.
    pub hash: Option<Hash32>,
    /// The Identity Signing Key pinned by the first accepted manifest.
    pub isk_pub: Option<[u8; 32]>,
}

/// One identity record as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordRow {
    /// Its path.
    pub path: String,
    /// BLAKE3 of the bytes.
    pub hash: Hash32,
    /// The signed bytes.
    pub body: Vec<u8>,
}

/// One revocation boundary: the device, the vault, the newest head it may
/// have signed there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RevocationRow {
    /// The device.
    pub device_id: Id,
    /// The vault.
    pub vault_id: Id,
    /// The boundary.
    pub boundary_seq: u64,
}

/// What accepting an identity manifest stores, all at once.
#[derive(Clone, Debug)]
pub struct IdentityAccept<'a> {
    /// The manifest, decoded.
    pub manifest: &'a IdentityManifest,
    /// Its bytes as signed.
    pub body: &'a [u8],
    /// Records that came with it.
    pub records: &'a [RecordRow],
    /// The Identity Signing Key the set verified under.
    pub isk_pub: &'a [u8; 32],
    /// Every revoked device.
    pub revoked: &'a [Id],
    /// Every revocation boundary.
    pub revocations: &'a [RevocationRow],
}

/// One log leaf as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogRow {
    /// Its index.
    pub index: u64,
    /// The leaf's bytes as hashed.
    pub body: Vec<u8>,
}

/// `(identity_seq, identity_hash, isk_pub)` as SQLite returns them.
type IdentityColumns = (i64, Option<Vec<u8>>, Option<Vec<u8>>);

/// One item as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemRow {
    /// The item.
    pub item_id: Id,
    /// Its revision.
    pub rev: u64,
    /// The envelope bytes.
    pub body: Vec<u8>,
}

/// What a push did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PushResult {
    /// Stored at this revision.
    Accepted(u64),
    /// `if_match` was not the current revision; here is the current item.
    Stale(ItemRow),
    /// `if_match` was non-zero but the server holds no such item.
    Unknown,
    /// The item's attachment list names a blob the server doesn't hold.
    UnknownBlob(Hash32),
}

/// The server's store.
pub trait Storage: Send + Sync {
    /// Creates an account. [`StorageError::LoginTaken`] if the login exists.
    fn create_account(&self, login: &str, profile_id: Id, password_file: &[u8]) -> Result<Account>;
    /// The account filed under a login name.
    fn account_by_login(&self, login: &str) -> Result<Option<Account>>;
    /// An account by id.
    fn account(&self, account_id: Id) -> Result<Option<Account>>;
    /// Records a device the account logged in with. The same id with a
    /// different key is [`StorageError::DeviceKeyMismatch`].
    fn register_device(&self, account_id: Id, device_id: Id, device_pub: &[u8; 32]) -> Result<()>;
    /// The public key of a device the account logged in with.
    fn device_pub(&self, account_id: Id, device_id: Id) -> Result<Option<[u8; 32]>>;
    /// Where the account's identity chain stands.
    fn identity_state(&self, account_id: Id) -> Result<IdentityState>;
    /// Every identity manifest with `seq > since`, oldest first.
    fn identity_heads_since(&self, account_id: Id, since: u64) -> Result<Vec<HeadRow>>;
    /// The stored records among `refs`, by path and hash.
    fn identity_records(&self, account_id: Id, refs: &[RecordRef]) -> Result<Vec<RecordRow>>;
    /// Accepts an identity manifest if it extends the chain by exactly one
    /// and names the current manifest as `prev_hash`; stores the records
    /// given alongside, pins the signing key on the account (it must equal
    /// the pinned one when there is one), and replaces the account's
    /// revocation rows (every revoked device, with a marker row and one
    /// row per boundary). The caller has verified everything else. Atomic.
    fn accept_identity(
        &self,
        account_id: Id,
        accept: IdentityAccept<'_>,
    ) -> Result<std::result::Result<HeadAccepted, HeadRefused>>;
    /// For a revoked device, the newest head it may have signed in
    /// `vault_id` (zero when the revocation names no boundary there);
    /// `None` for a device in good standing.
    fn revoked_after(&self, account_id: Id, device_id: Id, vault_id: Id) -> Result<Option<u64>>;
    /// Creates a vault for the account or replaces its record. A vault
    /// another account owns is [`StorageError::Forbidden`].
    fn put_vault(&self, account_id: Id, vault_id: Id, record: &[u8]) -> Result<()>;
    /// A vault.
    fn vault(&self, vault_id: Id) -> Result<Option<VaultRow>>;
    /// Every vault of an account, in id order.
    fn vaults_of(&self, account_id: Id) -> Result<Vec<VaultRow>>;
    /// Every head with `seq > since`, oldest first.
    fn heads_since(&self, vault_id: Id, since: u64) -> Result<Vec<HeadRow>>;
    /// Accepts a head if it extends the chain by exactly one, names the
    /// current head as `prev_hash`, and every entry matches an item the
    /// server holds at that revision. The caller has already checked the
    /// signature and the entries' order. Atomic.
    fn accept_head(
        &self,
        vault_id: Id,
        head: &ManifestHead,
        body: &[u8],
    ) -> Result<std::result::Result<HeadAccepted, HeadRefused>>;
    /// One item.
    fn item(&self, vault_id: Id, item_id: Id) -> Result<Option<ItemRow>>;
    /// Every item with `rev > since`, lowest revision first.
    fn items_since(&self, vault_id: Id, since: u64) -> Result<Vec<ItemRow>>;
    /// Stores an item if `if_match` is its current revision (zero for a
    /// new one) and every blob in `blobs` (the names its attachment list
    /// carries with a key) is held, and assigns the next revision. The
    /// item's blob references are replaced by `blobs`; a blob that lost
    /// its last reference starts its grace period at `now`. Atomic.
    fn push_item(
        &self,
        vault_id: Id,
        item_id: Id,
        body: &[u8],
        if_match: u64,
        blobs: &[Hash32],
        now: i64,
    ) -> Result<PushResult>;
    /// Stores a blob under its name (the caller checked the hash). Returns
    /// whether the server lacked it. A blob nothing names yet is kept for
    /// the grace period.
    fn put_blob(&self, vault_id: Id, hash: Hash32, body: &[u8], now: i64) -> Result<bool>;
    /// A blob's bytes.
    fn blob(&self, vault_id: Id, hash: Hash32) -> Result<Option<Vec<u8>>>;
    /// The account that owns a vault, if the vault is here.
    fn vault_owner(&self, vault_id: Id) -> Result<Option<Id>>;
    /// Bytes of every blob held in the vaults `account_id` owns: what its
    /// attachment quota counts (D234).
    fn account_blob_bytes(&self, account_id: Id) -> Result<u64>;
    /// Whether a blob is held.
    fn has_blob(&self, vault_id: Id, hash: Hash32) -> Result<bool>;
    /// Removes every blob no item names whose grace period (from its upload,
    /// or from when it lost its last reference) is over. Returns how many.
    fn gc_blobs(&self, now: i64, grace_secs: i64) -> Result<usize>;
    /// How many leaves the key-transparency log has.
    fn log_size(&self) -> Result<u64>;
    /// Every leaf hash, oldest first: what the tree is computed from.
    fn log_leaf_hashes(&self) -> Result<Vec<Hash32>>;
    /// An account's leaves at index `from` or later, oldest first.
    fn log_entries(&self, account_id: Id, from: u64) -> Result<Vec<LogRow>>;
    /// Writes a consistent copy of the whole store to `path` (the backup
    /// job's primitive, 05-self-host §4).
    fn snapshot_to(&self, path: &Path) -> Result<()>;
    /// The account alerts (`crate::alerts`), when this store keeps them.
    fn alerts(&self) -> Option<&dyn crate::alerts::AlertStore> {
        None
    }
    /// The session table, when this backend has one.
    fn sessions(&self) -> Option<&dyn crate::sessions::SessionStore> {
        None
    }
    /// The sharing tables (`crate::sharing`), when this store keeps them.
    fn sharing(&self) -> Option<&dyn crate::sharing::SharingStore> {
        None
    }
}

/// The SQLite backend.
pub struct SqliteStorage {
    path: Option<PathBuf>,
    conn: Mutex<Connection>,
    /// With `--blob-dir`: attachment bodies live as files here, one per
    /// blob (`<vault>/<hash>`), and the database keeps only their rows.
    blob_dir: Option<PathBuf>,
}

impl std::fmt::Debug for SqliteStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.path {
            Some(p) => write!(f, "SqliteStorage({})", p.display()),
            None => f.write_str("SqliteStorage(memory)"),
        }
    }
}

fn backend(e: rusqlite::Error) -> StorageError {
    StorageError::Backend(e.to_string())
}

fn parse_id(text: &str) -> Result<Id> {
    Id::parse(text).map_err(|_| StorageError::Backend(format!("'{text}' isn't an id in the store")))
}

fn hash_from(bytes: Option<Vec<u8>>) -> Result<Option<Hash32>> {
    match bytes {
        None => Ok(None),
        Some(b) => {
            let arr: [u8; 32] = b
                .try_into()
                .map_err(|_| StorageError::Backend("a stored hash isn't 32 bytes".into()))?;
            Ok(Some(Hash32(arr)))
        }
    }
}

fn to_i64(n: u64, what: &str) -> Result<i64> {
    i64::try_from(n).map_err(|_| StorageError::Backend(format!("{what} past i64")))
}

fn to_u64(n: i64) -> u64 {
    u64::try_from(n).unwrap_or(0)
}

fn now() -> i64 {
    lockdown_vault::Timestamp::now().0
}

/// `(account_id, record, head_seq, head_hash, item_rev)` as SQLite returns them.
type VaultColumns = (String, Vec<u8>, i64, Option<Vec<u8>>, i64);

impl SqliteStorage {
    /// Opens the store at `path`, creating the file and its schema if they
    /// are not there. The file is made readable only by the user.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent()
            && !dir.as_os_str().is_empty()
        {
            crate::keys::create_private_dir(dir)
                .map_err(|e| StorageError::Backend(e.to_string()))?;
        }
        if !path.exists() {
            crate::keys::write_private(path, &[])
                .map_err(|e| StorageError::Backend(e.to_string()))?;
        }
        let conn = Connection::open(path).map_err(backend)?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(backend)?;
        conn.pragma_update(None, "synchronous", "FULL")
            .map_err(backend)?;
        Self::with_connection(conn, Some(path.to_path_buf()))
    }

    /// A store that lives only in memory, for tests.
    pub fn open_in_memory() -> Result<Self> {
        Self::with_connection(Connection::open_in_memory().map_err(backend)?, None)
    }

    fn with_connection(conn: Connection, path: Option<PathBuf>) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(backend)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS accounts (
                 account_id    TEXT PRIMARY KEY NOT NULL,
                 login         TEXT NOT NULL UNIQUE,
                 profile_id    TEXT NOT NULL,
                 password_file BLOB NOT NULL,
                 created_at    INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS devices (
                 account_id TEXT NOT NULL REFERENCES accounts(account_id),
                 device_id  TEXT NOT NULL,
                 device_pub BLOB NOT NULL,
                 first_seen INTEGER NOT NULL,
                 PRIMARY KEY (account_id, device_id)
             );
             CREATE TABLE IF NOT EXISTS identity_heads (
                 account_id TEXT NOT NULL REFERENCES accounts(account_id),
                 seq        INTEGER NOT NULL,
                 hash       BLOB NOT NULL,
                 body       BLOB NOT NULL,
                 PRIMARY KEY (account_id, seq)
             );
             CREATE TABLE IF NOT EXISTS identity_records (
                 account_id TEXT NOT NULL REFERENCES accounts(account_id),
                 path       TEXT NOT NULL,
                 hash       BLOB NOT NULL,
                 body       BLOB NOT NULL,
                 PRIMARY KEY (account_id, path, hash)
             );
             CREATE TABLE IF NOT EXISTS revoked_devices (
                 account_id   TEXT NOT NULL REFERENCES accounts(account_id),
                 device_id    TEXT NOT NULL,
                 vault_id     TEXT NOT NULL,
                 boundary_seq INTEGER NOT NULL,
                 PRIMARY KEY (account_id, device_id, vault_id)
             );
             CREATE TABLE IF NOT EXISTS vaults (
                 vault_id   TEXT PRIMARY KEY NOT NULL,
                 account_id TEXT NOT NULL REFERENCES accounts(account_id),
                 record     BLOB NOT NULL,
                 head_seq   INTEGER NOT NULL DEFAULT 0,
                 head_hash  BLOB,
                 item_rev   INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE IF NOT EXISTS heads (
                 vault_id TEXT NOT NULL REFERENCES vaults(vault_id),
                 seq      INTEGER NOT NULL,
                 hash     BLOB NOT NULL,
                 body     BLOB NOT NULL,
                 PRIMARY KEY (vault_id, seq)
             );
             CREATE TABLE IF NOT EXISTS items (
                 vault_id TEXT NOT NULL REFERENCES vaults(vault_id),
                 item_id  TEXT NOT NULL,
                 rev      INTEGER NOT NULL,
                 body     BLOB NOT NULL,
                 PRIMARY KEY (vault_id, item_id)
             );
             CREATE INDEX IF NOT EXISTS items_by_rev ON items (vault_id, rev);
             CREATE TABLE IF NOT EXISTS blobs (
                 vault_id        TEXT NOT NULL REFERENCES vaults(vault_id),
                 hash            BLOB NOT NULL,
                 size            INTEGER NOT NULL,
                 body            BLOB NOT NULL,
                 uploaded_at     INTEGER NOT NULL,
                 unreferenced_at INTEGER,
                 PRIMARY KEY (vault_id, hash)
             );
             CREATE TABLE IF NOT EXISTS blob_refs (
                 vault_id TEXT NOT NULL,
                 item_id  TEXT NOT NULL,
                 hash     BLOB NOT NULL,
                 PRIMARY KEY (vault_id, item_id, hash)
             );
             CREATE TABLE IF NOT EXISTS kt_log (
                 idx        INTEGER PRIMARY KEY NOT NULL,
                 account_id TEXT NOT NULL,
                 leaf_hash  BLOB NOT NULL,
                 body       BLOB NOT NULL
             );
             CREATE INDEX IF NOT EXISTS kt_log_by_account ON kt_log (account_id, idx);",
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
                return Err(StorageError::SchemaTooNew {
                    found: v,
                    supported: SCHEMA_VERSION,
                });
            }
            Some(v) if v < SCHEMA_VERSION => {
                conn.execute(
                    "INSERT INTO schema_version (version) VALUES (?1)",
                    params![SCHEMA_VERSION],
                )
                .map_err(backend)?;
            }
            Some(_) => {}
        }
        // Schema 2's columns on `accounts`, added in place to a schema 1 file.
        let mut columns = conn
            .prepare("PRAGMA table_info(accounts)")
            .map_err(backend)?;
        let names: Vec<String> = columns
            .query_map([], |r| r.get::<_, String>(1))
            .map_err(backend)?
            .collect::<std::result::Result<_, _>>()
            .map_err(backend)?;
        drop(columns);
        for (name, decl) in [
            ("identity_seq", "INTEGER NOT NULL DEFAULT 0"),
            ("identity_hash", "BLOB"),
            ("isk_pub", "BLOB"),
        ] {
            if !names.iter().any(|n| n == name) {
                conn.execute(
                    &format!("ALTER TABLE accounts ADD COLUMN {name} {decl}"),
                    [],
                )
                .map_err(backend)?;
            }
        }
        crate::alerts::create_tables(&conn).map_err(backend)?;
        crate::sessions::create_tables(&conn).map_err(backend)?;
        crate::sharing::create_tables(&conn).map_err(backend)?;
        Ok(Self {
            path,
            conn: Mutex::new(conn),
            blob_dir: None,
        })
    }

    /// Keeps new attachment bodies as files under `dir` instead of in the
    /// database file (rows, sizes and references stay in the database, so
    /// every rule about blobs is unchanged). Bodies already in the
    /// database stay there and are still served. The directory is made
    /// readable only by the user.
    pub fn with_blob_dir(mut self, dir: &Path) -> Result<Self> {
        crate::keys::create_private_dir(dir).map_err(|e| StorageError::Backend(e.to_string()))?;
        self.blob_dir = Some(dir.to_path_buf());
        Ok(self)
    }

    /// How many accounts and vaults the store holds, and the log's size
    /// (a restore drill's look inside).
    pub fn counts(&self) -> Result<crate::backup::DbFacts> {
        let conn = self.lock();
        let count = |sql: &str| -> Result<u64> {
            conn.query_row(sql, [], |r| r.get::<_, i64>(0))
                .map(to_u64)
                .map_err(backend)
        };
        Ok(crate::backup::DbFacts {
            accounts: count("SELECT COUNT(*) FROM accounts")?,
            vaults: count("SELECT COUNT(*) FROM vaults")?,
            log_size: count("SELECT COUNT(*) FROM kt_log")?,
        })
    }

    /// Where attachment bodies are kept as files, if they are.
    pub fn blob_dir(&self) -> Option<&Path> {
        self.blob_dir.as_deref()
    }

    fn blob_file(dir: &Path, vault_id: Id, hash: Hash32) -> PathBuf {
        dir.join(vault_id.to_string()).join(hash.to_hex())
    }

    pub(crate) fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        // A poisoned lock means a handler panicked mid-call; the database
        // itself is transactional, so carrying on is safe.
        self.conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn read_account(
        row: &rusqlite::Row<'_>,
    ) -> rusqlite::Result<(String, String, String, Vec<u8>)> {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
    }

    fn account_from(parts: (String, String, String, Vec<u8>)) -> Result<Account> {
        Ok(Account {
            account_id: parse_id(&parts.0)?,
            login: parts.1,
            profile_id: parse_id(&parts.2)?,
            password_file: parts.3,
        })
    }

    fn vault_in(conn: &Connection, vault_id: Id) -> Result<Option<VaultRow>> {
        let row: Option<VaultColumns> = conn
            .query_row(
                "SELECT account_id, record, head_seq, head_hash, item_rev FROM vaults WHERE vault_id = ?1",
                params![vault_id.to_string()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .optional()
            .map_err(backend)?;
        match row {
            None => Ok(None),
            Some((account, record, head_seq, head_hash, item_rev)) => Ok(Some(VaultRow {
                vault_id,
                account_id: parse_id(&account)?,
                record,
                head_seq: to_u64(head_seq),
                head_hash: hash_from(head_hash)?,
                item_rev: to_u64(item_rev),
            })),
        }
    }

    fn identity_state_in(conn: &Connection, account_id: Id) -> Result<IdentityState> {
        let row: Option<IdentityColumns> = conn
            .query_row(
                "SELECT identity_seq, identity_hash, isk_pub FROM accounts WHERE account_id = ?1",
                params![account_id.to_string()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .map_err(backend)?;
        let Some((seq, hash, isk_pub)) = row else {
            return Err(StorageError::NotFound(format!("account {account_id}")));
        };
        let isk_pub = match isk_pub {
            None => None,
            Some(k) => Some(k.try_into().map_err(|_| {
                StorageError::Backend("a stored identity key isn't 32 bytes".into())
            })?),
        };
        Ok(IdentityState {
            seq: to_u64(seq),
            hash: hash_from(hash)?,
            isk_pub,
        })
    }

    /// Appends one leaf to the log, inside the caller's transaction. The
    /// index is the count so far; nothing is ever updated or removed.
    fn log_in(conn: &Connection, account_id: Id, entry: LogEntry) -> Result<u64> {
        let leaf = LogLeaf::new(account_id, entry);
        let body = leaf
            .to_bytes()
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        let hash = lockdown_crypto::merkle::leaf_hash(&body);
        let idx: i64 = conn
            .query_row("SELECT COUNT(*) FROM kt_log", [], |r| r.get(0))
            .map_err(backend)?;
        conn.execute(
            "INSERT INTO kt_log (idx, account_id, leaf_hash, body) VALUES (?1, ?2, ?3, ?4)",
            params![idx, account_id.to_string(), &hash.0[..], body],
        )
        .map_err(backend)?;
        Ok(to_u64(idx))
    }

    fn has_blob_in(conn: &Connection, vault_id: Id, hash: Hash32) -> Result<bool> {
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM blobs WHERE vault_id = ?1 AND hash = ?2",
                params![vault_id.to_string(), &hash.0[..]],
                |r| r.get(0),
            )
            .map_err(backend)?;
        Ok(n > 0)
    }

    fn item_in(conn: &Connection, vault_id: Id, item_id: Id) -> Result<Option<ItemRow>> {
        let row: Option<(i64, Vec<u8>)> = conn
            .query_row(
                "SELECT rev, body FROM items WHERE vault_id = ?1 AND item_id = ?2",
                params![vault_id.to_string(), item_id.to_string()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(backend)?;
        Ok(row.map(|(rev, body)| ItemRow {
            item_id,
            rev: to_u64(rev),
            body,
        }))
    }
}

impl Storage for SqliteStorage {
    fn create_account(&self, login: &str, profile_id: Id, password_file: &[u8]) -> Result<Account> {
        let conn = self.lock();
        let account_id = Id::new();
        let result = conn.execute(
            "INSERT INTO accounts (account_id, login, profile_id, password_file, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                account_id.to_string(),
                login,
                profile_id.to_string(),
                password_file,
                now()
            ],
        );
        match result {
            Ok(_) => Ok(Account {
                account_id,
                login: login.to_string(),
                profile_id,
                password_file: password_file.to_vec(),
            }),
            Err(rusqlite::Error::SqliteFailure(e, _))
                if e.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                Err(StorageError::LoginTaken)
            }
            Err(e) => Err(backend(e)),
        }
    }

    fn account_by_login(&self, login: &str) -> Result<Option<Account>> {
        let conn = self.lock();
        let row = conn
            .query_row(
                "SELECT account_id, login, profile_id, password_file FROM accounts WHERE login = ?1",
                params![login],
                Self::read_account,
            )
            .optional()
            .map_err(backend)?;
        row.map(Self::account_from).transpose()
    }

    fn account(&self, account_id: Id) -> Result<Option<Account>> {
        let conn = self.lock();
        let row = conn
            .query_row(
                "SELECT account_id, login, profile_id, password_file FROM accounts WHERE account_id = ?1",
                params![account_id.to_string()],
                Self::read_account,
            )
            .optional()
            .map_err(backend)?;
        row.map(Self::account_from).transpose()
    }

    fn register_device(&self, account_id: Id, device_id: Id, device_pub: &[u8; 32]) -> Result<()> {
        let mut conn = self.lock();
        let existing: Option<Vec<u8>> = conn
            .query_row(
                "SELECT device_pub FROM devices WHERE account_id = ?1 AND device_id = ?2",
                params![account_id.to_string(), device_id.to_string()],
                |r| r.get(0),
            )
            .optional()
            .map_err(backend)?;
        match existing {
            Some(key) if key == device_pub => Ok(()),
            Some(_) => Err(StorageError::DeviceKeyMismatch(device_id)),
            None => {
                let tx = conn.transaction().map_err(backend)?;
                tx.execute(
                    "INSERT INTO devices (account_id, device_id, device_pub, first_seen) VALUES (?1, ?2, ?3, ?4)",
                    params![
                        account_id.to_string(),
                        device_id.to_string(),
                        &device_pub[..],
                        now()
                    ],
                )
                .map_err(backend)?;
                Self::log_in(
                    &tx,
                    account_id,
                    LogEntry::Device {
                        device_id,
                        device_pub: *device_pub,
                    },
                )?;
                tx.commit().map_err(backend)
            }
        }
    }

    fn device_pub(&self, account_id: Id, device_id: Id) -> Result<Option<[u8; 32]>> {
        let conn = self.lock();
        let key: Option<Vec<u8>> = conn
            .query_row(
                "SELECT device_pub FROM devices WHERE account_id = ?1 AND device_id = ?2",
                params![account_id.to_string(), device_id.to_string()],
                |r| r.get(0),
            )
            .optional()
            .map_err(backend)?;
        match key {
            None => Ok(None),
            Some(k) => Ok(Some(k.try_into().map_err(|_| {
                StorageError::Backend("a stored device key isn't 32 bytes".into())
            })?)),
        }
    }

    fn identity_state(&self, account_id: Id) -> Result<IdentityState> {
        Self::identity_state_in(&self.lock(), account_id)
    }

    fn identity_heads_since(&self, account_id: Id, since: u64) -> Result<Vec<HeadRow>> {
        let conn = self.lock();
        let mut stmt = conn
            .prepare(
                "SELECT seq, hash, body FROM identity_heads WHERE account_id = ?1 AND seq > ?2 ORDER BY seq",
            )
            .map_err(backend)?;
        let rows = stmt
            .query_map(
                params![account_id.to_string(), to_i64(since, "a sequence")?],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, Vec<u8>>(1)?,
                        r.get::<_, Vec<u8>>(2)?,
                    ))
                },
            )
            .map_err(backend)?;
        let mut out = Vec::new();
        for row in rows {
            let (seq, hash, body) = row.map_err(backend)?;
            out.push(HeadRow {
                seq: to_u64(seq),
                hash: hash_from(Some(hash))?.expect("some"),
                body,
            });
        }
        Ok(out)
    }

    fn identity_records(&self, account_id: Id, refs: &[RecordRef]) -> Result<Vec<RecordRow>> {
        let conn = self.lock();
        let mut stmt = conn
            .prepare(
                "SELECT body FROM identity_records WHERE account_id = ?1 AND path = ?2 AND hash = ?3",
            )
            .map_err(backend)?;
        let mut out = Vec::new();
        for r in refs {
            let body: Option<Vec<u8>> = stmt
                .query_row(
                    params![account_id.to_string(), &r.path, &r.hash.0[..]],
                    |row| row.get(0),
                )
                .optional()
                .map_err(backend)?;
            if let Some(body) = body {
                out.push(RecordRow {
                    path: r.path.clone(),
                    hash: r.hash,
                    body,
                });
            }
        }
        Ok(out)
    }

    fn accept_identity(
        &self,
        account_id: Id,
        accept: IdentityAccept<'_>,
    ) -> Result<std::result::Result<HeadAccepted, HeadRefused>> {
        let IdentityAccept {
            manifest,
            body,
            records,
            isk_pub,
            revoked,
            revocations,
        } = accept;
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(backend)?;
        let state = Self::identity_state_in(&tx, account_id)?;
        let refuse = |code: ErrorCode, message: String| HeadRefused {
            code,
            message,
            current_seq: state.seq,
            current_hash: state.hash,
        };
        if let Some(pinned) = state.isk_pub
            && pinned != *isk_pub
        {
            return Ok(Err(refuse(
                ErrorCode::BadSignature,
                "the identity manifest isn't signed by this account's identity".into(),
            )));
        }
        if state.seq == 0 {
            if manifest.seq != 1 {
                return Ok(Err(refuse(
                    ErrorCode::Gap,
                    format!(
                        "the server holds no identity manifest for this account yet; the first must be seq 1, not {}",
                        manifest.seq
                    ),
                )));
            }
            if manifest.prev_hash != Hash32::ZERO {
                return Ok(Err(refuse(
                    ErrorCode::Fork,
                    "the first identity manifest must name no predecessor".into(),
                )));
            }
        } else {
            if manifest.seq <= state.seq {
                return Ok(Err(refuse(
                    ErrorCode::OlderSeq,
                    format!(
                        "the server is already at identity manifest {}; {} is not newer",
                        state.seq, manifest.seq
                    ),
                )));
            }
            if manifest.seq > state.seq + 1 {
                return Ok(Err(refuse(
                    ErrorCode::Gap,
                    format!(
                        "the server is at identity manifest {}; {} would skip {}",
                        state.seq,
                        manifest.seq,
                        manifest.seq - state.seq - 1
                    ),
                )));
            }
            if Some(manifest.prev_hash) != state.hash {
                return Ok(Err(refuse(
                    ErrorCode::Fork,
                    format!(
                        "identity manifest {} names a predecessor that isn't the server's current one ({}); another device moved first",
                        manifest.seq, state.seq
                    ),
                )));
            }
        }
        for r in records {
            tx.execute(
                "INSERT OR IGNORE INTO identity_records (account_id, path, hash, body) VALUES (?1, ?2, ?3, ?4)",
                params![account_id.to_string(), &r.path, &r.hash.0[..], &r.body],
            )
            .map_err(backend)?;
        }
        let hash = manifest
            .hash()
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        let seq = to_i64(manifest.seq, "a sequence")?;
        tx.execute(
            "INSERT INTO identity_heads (account_id, seq, hash, body) VALUES (?1, ?2, ?3, ?4)",
            params![account_id.to_string(), seq, &hash.0[..], body],
        )
        .map_err(backend)?;
        tx.execute(
            "UPDATE accounts SET identity_seq = ?2, identity_hash = ?3, isk_pub = ?4 WHERE account_id = ?1",
            params![account_id.to_string(), seq, &hash.0[..], &isk_pub[..]],
        )
        .map_err(backend)?;
        tx.execute(
            "DELETE FROM revoked_devices WHERE account_id = ?1",
            params![account_id.to_string()],
        )
        .map_err(backend)?;
        for device in revoked {
            tx.execute(
                "INSERT INTO revoked_devices (account_id, device_id, vault_id, boundary_seq) VALUES (?1, ?2, ?3, 0)",
                params![account_id.to_string(), device.to_string(), REVOKED_MARKER],
            )
            .map_err(backend)?;
        }
        for r in revocations {
            tx.execute(
                "INSERT OR REPLACE INTO revoked_devices (account_id, device_id, vault_id, boundary_seq) VALUES (?1, ?2, ?3, ?4)",
                params![
                    account_id.to_string(),
                    r.device_id.to_string(),
                    r.vault_id.to_string(),
                    to_i64(r.boundary_seq, "a boundary")?
                ],
            )
            .map_err(backend)?;
        }
        Self::log_in(
            &tx,
            account_id,
            LogEntry::IdentityHead {
                seq: manifest.seq,
                hash,
            },
        )?;
        tx.commit().map_err(backend)?;
        Ok(Ok(HeadAccepted {
            seq: manifest.seq,
            hash,
        }))
    }

    fn revoked_after(&self, account_id: Id, device_id: Id, vault_id: Id) -> Result<Option<u64>> {
        let conn = self.lock();
        let marker: Option<i64> = conn
            .query_row(
                "SELECT boundary_seq FROM revoked_devices WHERE account_id = ?1 AND device_id = ?2 AND vault_id = ?3",
                params![account_id.to_string(), device_id.to_string(), REVOKED_MARKER],
                |r| r.get(0),
            )
            .optional()
            .map_err(backend)?;
        if marker.is_none() {
            return Ok(None);
        }
        let boundary: Option<i64> = conn
            .query_row(
                "SELECT boundary_seq FROM revoked_devices WHERE account_id = ?1 AND device_id = ?2 AND vault_id = ?3",
                params![
                    account_id.to_string(),
                    device_id.to_string(),
                    vault_id.to_string()
                ],
                |r| r.get(0),
            )
            .optional()
            .map_err(backend)?;
        Ok(Some(boundary.map(to_u64).unwrap_or(0)))
    }

    fn put_vault(&self, account_id: Id, vault_id: Id, record: &[u8]) -> Result<()> {
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(backend)?;
        match Self::vault_in(&tx, vault_id)? {
            Some(row) if row.account_id != account_id => {
                return Err(StorageError::Forbidden(vault_id));
            }
            Some(_) => {
                tx.execute(
                    "UPDATE vaults SET record = ?2 WHERE vault_id = ?1",
                    params![vault_id.to_string(), record],
                )
                .map_err(backend)?;
            }
            None => {
                tx.execute(
                    "INSERT INTO vaults (vault_id, account_id, record) VALUES (?1, ?2, ?3)",
                    params![vault_id.to_string(), account_id.to_string(), record],
                )
                .map_err(backend)?;
            }
        }
        tx.commit().map_err(backend)
    }

    fn vault(&self, vault_id: Id) -> Result<Option<VaultRow>> {
        Self::vault_in(&self.lock(), vault_id)
    }

    fn vaults_of(&self, account_id: Id) -> Result<Vec<VaultRow>> {
        let conn = self.lock();
        let mut stmt = conn
            .prepare(
                "SELECT vault_id, record, head_seq, head_hash, item_rev FROM vaults WHERE account_id = ?1 ORDER BY vault_id",
            )
            .map_err(backend)?;
        let rows = stmt
            .query_map(params![account_id.to_string()], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Vec<u8>>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, Option<Vec<u8>>>(3)?,
                    r.get::<_, i64>(4)?,
                ))
            })
            .map_err(backend)?;
        let mut out = Vec::new();
        for row in rows {
            let (vault_id, record, head_seq, head_hash, item_rev) = row.map_err(backend)?;
            out.push(VaultRow {
                vault_id: parse_id(&vault_id)?,
                account_id,
                record,
                head_seq: to_u64(head_seq),
                head_hash: hash_from(head_hash)?,
                item_rev: to_u64(item_rev),
            });
        }
        Ok(out)
    }

    fn heads_since(&self, vault_id: Id, since: u64) -> Result<Vec<HeadRow>> {
        let conn = self.lock();
        let mut stmt = conn
            .prepare(
                "SELECT seq, hash, body FROM heads WHERE vault_id = ?1 AND seq > ?2 ORDER BY seq",
            )
            .map_err(backend)?;
        let rows = stmt
            .query_map(
                params![vault_id.to_string(), to_i64(since, "a sequence")?],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, Vec<u8>>(1)?,
                        r.get::<_, Vec<u8>>(2)?,
                    ))
                },
            )
            .map_err(backend)?;
        let mut out = Vec::new();
        for row in rows {
            let (seq, hash, body) = row.map_err(backend)?;
            out.push(HeadRow {
                seq: to_u64(seq),
                hash: hash_from(Some(hash))?.expect("some"),
                body,
            });
        }
        Ok(out)
    }

    fn accept_head(
        &self,
        vault_id: Id,
        head: &ManifestHead,
        body: &[u8],
    ) -> Result<std::result::Result<HeadAccepted, HeadRefused>> {
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(backend)?;
        let vault = Self::vault_in(&tx, vault_id)?
            .ok_or_else(|| StorageError::NotFound(format!("vault {vault_id}")))?;
        let refuse = |code: ErrorCode, message: String| HeadRefused {
            code,
            message,
            current_seq: vault.head_seq,
            current_hash: vault.head_hash,
        };
        if vault.head_seq == 0 {
            if head.seq != 1 {
                return Ok(Err(refuse(
                    ErrorCode::Gap,
                    format!(
                        "the server holds no head for this vault yet; the first must be seq 1, not {}",
                        head.seq
                    ),
                )));
            }
            if head.prev_hash != Hash32::ZERO {
                return Ok(Err(refuse(
                    ErrorCode::Fork,
                    "the first head must name no predecessor".into(),
                )));
            }
        } else {
            if head.seq <= vault.head_seq {
                return Ok(Err(refuse(
                    ErrorCode::OlderSeq,
                    format!(
                        "the server is already at seq {}; seq {} is not newer",
                        vault.head_seq, head.seq
                    ),
                )));
            }
            if head.seq > vault.head_seq + 1 {
                return Ok(Err(refuse(
                    ErrorCode::Gap,
                    format!(
                        "the server is at seq {}; seq {} would skip {}",
                        vault.head_seq,
                        head.seq,
                        head.seq - vault.head_seq - 1
                    ),
                )));
            }
            if Some(head.prev_hash) != vault.head_hash {
                return Ok(Err(refuse(
                    ErrorCode::Fork,
                    format!(
                        "seq {} names a predecessor that isn't the server's head {}; another device moved first",
                        head.seq, vault.head_seq
                    ),
                )));
            }
        }
        for entry in &head.entries {
            let Some(row) = Self::item_in(&tx, vault_id, entry.item_id)? else {
                return Ok(Err(refuse(
                    ErrorCode::UnknownItem,
                    format!(
                        "the head lists item {} but the server doesn't hold it; push the item first",
                        entry.item_id
                    ),
                )));
            };
            if row.rev != entry.rev {
                return Ok(Err(refuse(
                    ErrorCode::UnknownItem,
                    format!(
                        "the head lists item {} at revision {} but the server holds revision {}",
                        entry.item_id, entry.rev, row.rev
                    ),
                )));
            }
            let item = Item::from_bytes(&row.body).map_err(|e| {
                StorageError::Backend(format!("stored item {} didn't decode: {e}", entry.item_id))
            })?;
            let hash = item
                .latest_hash()
                .map_err(|e| StorageError::Backend(e.to_string()))?;
            if hash != entry.version_hash
                || item.state != entry.state
                || item.is_shredded() != entry.shredded
            {
                return Ok(Err(refuse(
                    ErrorCode::UnknownItem,
                    format!(
                        "the head describes item {} differently from the envelope the server holds at revision {}",
                        entry.item_id, row.rev
                    ),
                )));
            }
        }
        let hash = head
            .hash()
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        let seq = to_i64(head.seq, "a sequence")?;
        tx.execute(
            "INSERT INTO heads (vault_id, seq, hash, body) VALUES (?1, ?2, ?3, ?4)",
            params![vault_id.to_string(), seq, &hash.0[..], body],
        )
        .map_err(backend)?;
        tx.execute(
            "UPDATE vaults SET head_seq = ?2, head_hash = ?3 WHERE vault_id = ?1",
            params![vault_id.to_string(), seq, &hash.0[..]],
        )
        .map_err(backend)?;
        Self::log_in(
            &tx,
            vault.account_id,
            LogEntry::VaultHead {
                vault_id,
                seq: head.seq,
                hash,
            },
        )?;
        tx.commit().map_err(backend)?;
        Ok(Ok(HeadAccepted {
            seq: head.seq,
            hash,
        }))
    }

    fn item(&self, vault_id: Id, item_id: Id) -> Result<Option<ItemRow>> {
        Self::item_in(&self.lock(), vault_id, item_id)
    }

    fn items_since(&self, vault_id: Id, since: u64) -> Result<Vec<ItemRow>> {
        let conn = self.lock();
        let mut stmt = conn
            .prepare("SELECT item_id, rev, body FROM items WHERE vault_id = ?1 AND rev > ?2 ORDER BY rev")
            .map_err(backend)?;
        let rows = stmt
            .query_map(
                params![vault_id.to_string(), to_i64(since, "a revision")?],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, Vec<u8>>(2)?,
                    ))
                },
            )
            .map_err(backend)?;
        let mut out = Vec::new();
        for row in rows {
            let (item_id, rev, body) = row.map_err(backend)?;
            out.push(ItemRow {
                item_id: parse_id(&item_id)?,
                rev: to_u64(rev),
                body,
            });
        }
        Ok(out)
    }

    fn push_item(
        &self,
        vault_id: Id,
        item_id: Id,
        body: &[u8],
        if_match: u64,
        blobs: &[Hash32],
        now: i64,
    ) -> Result<PushResult> {
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(backend)?;
        let vault = Self::vault_in(&tx, vault_id)?
            .ok_or_else(|| StorageError::NotFound(format!("vault {vault_id}")))?;
        let current = Self::item_in(&tx, vault_id, item_id)?;
        match &current {
            None if if_match != 0 => return Ok(PushResult::Unknown),
            Some(row) if row.rev != if_match => return Ok(PushResult::Stale(row.clone())),
            _ => {}
        }
        for hash in blobs {
            if !Self::has_blob_in(&tx, vault_id, *hash)? {
                return Ok(PushResult::UnknownBlob(*hash));
            }
        }
        // The item's references are replaced; what it no longer names and
        // nothing else names starts its grace period now.
        let before: Vec<Hash32> = {
            let mut stmt = tx
                .prepare("SELECT hash FROM blob_refs WHERE vault_id = ?1 AND item_id = ?2")
                .map_err(backend)?;
            let rows = stmt
                .query_map(params![vault_id.to_string(), item_id.to_string()], |r| {
                    r.get::<_, Vec<u8>>(0)
                })
                .map_err(backend)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(hash_from(Some(row.map_err(backend)?))?.expect("some"));
            }
            out
        };
        tx.execute(
            "DELETE FROM blob_refs WHERE vault_id = ?1 AND item_id = ?2",
            params![vault_id.to_string(), item_id.to_string()],
        )
        .map_err(backend)?;
        for hash in blobs {
            tx.execute(
                "INSERT OR IGNORE INTO blob_refs (vault_id, item_id, hash) VALUES (?1, ?2, ?3)",
                params![vault_id.to_string(), item_id.to_string(), &hash.0[..]],
            )
            .map_err(backend)?;
            tx.execute(
                "UPDATE blobs SET unreferenced_at = NULL WHERE vault_id = ?1 AND hash = ?2",
                params![vault_id.to_string(), &hash.0[..]],
            )
            .map_err(backend)?;
        }
        for hash in before.iter().filter(|h| !blobs.contains(h)) {
            tx.execute(
                "UPDATE blobs SET unreferenced_at = ?3 WHERE vault_id = ?1 AND hash = ?2 AND NOT EXISTS (SELECT 1 FROM blob_refs WHERE blob_refs.vault_id = ?1 AND blob_refs.hash = ?2)",
                params![vault_id.to_string(), &hash.0[..], now],
            )
            .map_err(backend)?;
        }
        let rev = vault.item_rev + 1;
        let rev_i = to_i64(rev, "a revision")?;
        tx.execute(
            "INSERT OR REPLACE INTO items (vault_id, item_id, rev, body) VALUES (?1, ?2, ?3, ?4)",
            params![vault_id.to_string(), item_id.to_string(), rev_i, body],
        )
        .map_err(backend)?;
        tx.execute(
            "UPDATE vaults SET item_rev = ?2 WHERE vault_id = ?1",
            params![vault_id.to_string(), rev_i],
        )
        .map_err(backend)?;
        tx.commit().map_err(backend)?;
        Ok(PushResult::Accepted(rev))
    }

    fn put_blob(&self, vault_id: Id, hash: Hash32, body: &[u8], now: i64) -> Result<bool> {
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(backend)?;
        if Self::vault_in(&tx, vault_id)?.is_none() {
            return Err(StorageError::NotFound(format!("vault {vault_id}")));
        }
        if Self::has_blob_in(&tx, vault_id, hash)? {
            return Ok(false);
        }
        // With a blob directory the body goes to its file first (written
        // whole and renamed into place), then the row; the row's body is
        // empty. A row is only ever there for a file that is.
        let stored: &[u8] = match &self.blob_dir {
            Some(dir) if !body.is_empty() => {
                crate::keys::write_private(&Self::blob_file(dir, vault_id, hash), body).map_err(
                    |e| StorageError::Backend(format!("couldn't write a blob file: {e}")),
                )?;
                &[]
            }
            _ => body,
        };
        tx.execute(
            "INSERT INTO blobs (vault_id, hash, size, body, uploaded_at, unreferenced_at) VALUES (?1, ?2, ?3, ?4, ?5, NULL)",
            params![
                vault_id.to_string(),
                &hash.0[..],
                to_i64(body.len() as u64, "a blob size")?,
                stored,
                now
            ],
        )
        .map_err(backend)?;
        tx.commit().map_err(backend)?;
        Ok(true)
    }

    fn vault_owner(&self, vault_id: Id) -> Result<Option<Id>> {
        let conn = self.lock();
        Ok(Self::vault_in(&conn, vault_id)?.map(|v| v.account_id))
    }

    fn account_blob_bytes(&self, account_id: Id) -> Result<u64> {
        let conn = self.lock();
        let total: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(b.size), 0) FROM blobs b JOIN vaults v ON v.vault_id = b.vault_id WHERE v.account_id = ?1",
                params![account_id.to_string()],
                |r| r.get(0),
            )
            .map_err(backend)?;
        Ok(to_u64(total))
    }

    fn blob(&self, vault_id: Id, hash: Hash32) -> Result<Option<Vec<u8>>> {
        let row: Option<(Vec<u8>, i64)> = self
            .lock()
            .query_row(
                "SELECT body, size FROM blobs WHERE vault_id = ?1 AND hash = ?2",
                params![vault_id.to_string(), &hash.0[..]],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(backend)?;
        match row {
            Some((body, size)) if body.is_empty() && size > 0 => {
                let Some(dir) = &self.blob_dir else {
                    return Err(StorageError::Backend(
                        "a blob lives in the blob directory, but none is set (--blob-dir)".into(),
                    ));
                };
                let bytes = std::fs::read(Self::blob_file(dir, vault_id, hash)).map_err(|e| {
                    StorageError::Backend(format!("couldn't read a blob file: {e}"))
                })?;
                if bytes.len() as u64 != to_u64(size) {
                    return Err(StorageError::Backend(
                        "a blob file isn't the size its row says".into(),
                    ));
                }
                Ok(Some(bytes))
            }
            Some((body, _)) => Ok(Some(body)),
            None => Ok(None),
        }
    }

    fn has_blob(&self, vault_id: Id, hash: Hash32) -> Result<bool> {
        Self::has_blob_in(&self.lock(), vault_id, hash)
    }

    fn gc_blobs(&self, now: i64, grace_secs: i64) -> Result<usize> {
        const DUE: &str = "COALESCE(unreferenced_at, uploaded_at) <= ?1 AND NOT EXISTS (SELECT 1 FROM blob_refs WHERE blob_refs.vault_id = blobs.vault_id AND blob_refs.hash = blobs.hash)";
        let conn = self.lock();
        let cutoff = now.saturating_sub(grace_secs);
        // The files of the rows about to go (bodies in the database go
        // with their rows).
        let mut files = Vec::new();
        if let Some(dir) = &self.blob_dir {
            let mut stmt = conn
                .prepare(&format!(
                    "SELECT vault_id, hash FROM blobs WHERE length(body) = 0 AND size > 0 AND {DUE}"
                ))
                .map_err(backend)?;
            let rows = stmt
                .query_map(params![cutoff], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
                })
                .map_err(backend)?;
            for row in rows {
                let (vault, hash) = row.map_err(backend)?;
                if let (Ok(vault), Some(hash)) = (parse_id(&vault), hash_from(Some(hash))?) {
                    files.push(Self::blob_file(dir, vault, hash));
                }
            }
        }
        let n = conn
            .execute(&format!("DELETE FROM blobs WHERE {DUE}"), params![cutoff])
            .map_err(backend)?;
        drop(conn);
        for file in files {
            let _ = std::fs::remove_file(file);
        }
        Ok(n)
    }

    fn log_size(&self) -> Result<u64> {
        let n: i64 = self
            .lock()
            .query_row("SELECT COUNT(*) FROM kt_log", [], |r| r.get(0))
            .map_err(backend)?;
        Ok(to_u64(n))
    }

    fn log_leaf_hashes(&self) -> Result<Vec<Hash32>> {
        let conn = self.lock();
        let mut stmt = conn
            .prepare("SELECT leaf_hash FROM kt_log ORDER BY idx")
            .map_err(backend)?;
        let rows = stmt
            .query_map([], |r| r.get::<_, Vec<u8>>(0))
            .map_err(backend)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(hash_from(Some(row.map_err(backend)?))?.expect("some"));
        }
        Ok(out)
    }

    fn log_entries(&self, account_id: Id, from: u64) -> Result<Vec<LogRow>> {
        let conn = self.lock();
        let mut stmt = conn
            .prepare(
                "SELECT idx, body FROM kt_log WHERE account_id = ?1 AND idx >= ?2 ORDER BY idx",
            )
            .map_err(backend)?;
        let rows = stmt
            .query_map(
                params![account_id.to_string(), to_i64(from, "a log index")?],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?)),
            )
            .map_err(backend)?;
        let mut out = Vec::new();
        for row in rows {
            let (idx, body) = row.map_err(backend)?;
            out.push(LogRow {
                index: to_u64(idx),
                body,
            });
        }
        Ok(out)
    }

    fn snapshot_to(&self, path: &Path) -> Result<()> {
        let conn = self.lock();
        let target = path
            .to_str()
            .ok_or_else(|| StorageError::Backend("the snapshot path isn't UTF-8".into()))?;
        conn.execute("VACUUM INTO ?1", params![target])
            .map_err(backend)?;
        Ok(())
    }

    fn alerts(&self) -> Option<&dyn crate::alerts::AlertStore> {
        Some(self)
    }

    fn sessions(&self) -> Option<&dyn crate::sessions::SessionStore> {
        Some(self)
    }

    fn sharing(&self) -> Option<&dyn crate::sharing::SharingStore> {
        Some(self)
    }
}
