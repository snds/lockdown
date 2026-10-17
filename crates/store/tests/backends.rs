//! The same vault round trips through every backend. Each test runs once
//! against `MemoryStore` and once against `SqliteStore` on a temp file;
//! nothing here is written twice.

use std::path::PathBuf;

use ecosystem_identity::SymmetricKey;
use ecosystem_identity::keys::{AppDataKey, CrownJewelKey};
use lockdown_crypto::SigningKey;
use lockdown_store::{MemoryStore, SqliteStore, Store, StoreError};
use lockdown_vault::test_support::{sample_context, sample_item, sample_login_fields};
use lockdown_vault::{
    Error, Id, ItemClass, ItemKind, ItemState, PlainField, Unlock, Vault, VaultContext,
};
use tempfile::TempDir;

/// One backend under test. `with` hands out a fresh handle every time, so
/// the SQLite file is closed between steps, as it would be between runs.
enum Fixture {
    Memory(MemoryStore),
    Sqlite { _dir: TempDir, path: PathBuf },
}

/// A copy of the store as a backup would hold it.
enum Snapshot {
    Memory(MemoryStore),
    Sqlite(Vec<u8>),
}

impl Fixture {
    fn all() -> Vec<Fixture> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vault.sqlite");
        vec![
            Fixture::Memory(MemoryStore::new()),
            Fixture::Sqlite { _dir: dir, path },
        ]
    }

    fn name(&self) -> &'static str {
        match self {
            Fixture::Memory(_) => "memory",
            Fixture::Sqlite { .. } => "sqlite",
        }
    }

    fn with<R>(&mut self, f: impl FnOnce(&mut dyn Store) -> R) -> R {
        match self {
            Fixture::Memory(store) => f(store),
            Fixture::Sqlite { path, .. } => {
                let mut store = SqliteStore::open(path).unwrap();
                f(&mut store)
            }
        }
    }

    fn snapshot(&self) -> Snapshot {
        match self {
            Fixture::Memory(store) => Snapshot::Memory(store.clone()),
            Fixture::Sqlite { path, .. } => Snapshot::Sqlite(std::fs::read(path).unwrap()),
        }
    }

    fn restore(&mut self, snapshot: Snapshot) {
        match (self, snapshot) {
            (Fixture::Memory(store), Snapshot::Memory(old)) => *store = old,
            (Fixture::Sqlite { path, .. }, Snapshot::Sqlite(bytes)) => {
                for suffix in ["-wal", "-shm"] {
                    let mut side = path.as_os_str().to_owned();
                    side.push(suffix);
                    let _ = std::fs::remove_file(side);
                }
                std::fs::write(path, bytes).unwrap();
            }
            _ => unreachable!("a snapshot of another backend"),
        }
    }
}

/// What a device holds across runs: the persona's keys and its own signer.
struct Device {
    ctx: VaultContext,
    device_id: Id,
    seed: [u8; 32],
    adk: AppDataKey,
    cjk: CrownJewelKey,
}

impl Device {
    fn new() -> Self {
        Self {
            ctx: sample_context(),
            device_id: Id::new(),
            seed: lockdown_crypto::rand::random_32().unwrap(),
            adk: AppDataKey::generate().unwrap(),
            cjk: CrownJewelKey::generate().unwrap(),
        }
    }

    fn signer(&self) -> Box<SigningKey> {
        Box::new(SigningKey::from_bytes(self.seed))
    }

    fn create(&self) -> Vault {
        Vault::create(
            self.ctx,
            self.device_id,
            self.signer(),
            Some(self.cjk.clone()),
        )
        .unwrap()
    }

    fn load(&self, store: &dyn Store, last_accepted_seq: u64) -> Result<Vault, Error> {
        let nobody = |_: Id| None;
        Vault::load(
            store,
            self.ctx.vault_id,
            Unlock {
                app_data_key: &self.adk,
                crown_jewel_key: Some(self.cjk.clone()),
                device_id: self.device_id,
                signer: self.signer(),
                known_devices: &nobody,
                last_accepted_seq,
            },
        )
    }
}

fn note(body: &str) -> Vec<PlainField> {
    vec![PlainField::text("body", "note", body)]
}

#[test]
fn a_vault_round_trips_through_every_backend() {
    for mut fx in Fixture::all() {
        let device = Device::new();
        let mut vault = device.create();
        let login = vault
            .create_item(ItemKind::Login, ItemClass::Standard, sample_login_fields())
            .unwrap();
        let edited = vault
            .create_item(ItemKind::Note, ItemClass::Standard, note("first"))
            .unwrap();
        vault.update_item(edited, note("second")).unwrap();
        let trashed = vault
            .create_item(ItemKind::Note, ItemClass::Standard, note("bin me"))
            .unwrap();
        vault.trash_item(trashed).unwrap();
        let purged = vault
            .create_item(ItemKind::Note, ItemClass::Standard, note("gone"))
            .unwrap();
        vault.purge_item(purged).unwrap();
        let seed = vec![PlainField::hidden("seed", "seed", "abandon ability able")];
        let jewel = vault
            .create_item(ItemKind::CryptoWallet, ItemClass::CrownJewel, seed.clone())
            .unwrap();
        fx.with(|store| vault.save(store, &device.adk)).unwrap();

        let back = fx.with(|store| device.load(store, vault.seq())).unwrap();
        assert_eq!(back.seq(), vault.seq(), "{}", fx.name());
        assert_eq!(back.heads(), vault.heads());
        assert_eq!(back.item_ids(), vault.item_ids());
        assert_eq!(back.open_item(login).unwrap(), sample_login_fields());
        assert_eq!(back.open_item(edited).unwrap(), note("second"));
        assert_eq!(back.open_version(edited, 1).unwrap(), note("first"));
        assert_eq!(back.item(trashed).unwrap().state, ItemState::Trashed);
        assert!(matches!(
            back.open_item(purged),
            Err(Error::Shredded { .. })
        ));
        assert_eq!(back.open_item(jewel).unwrap(), seed);
        assert_eq!(back.members().unwrap(), vault.members().unwrap());

        // Keep going from the loaded copy: only the new heads are written.
        let mut back = back;
        let later = back
            .create_item(ItemKind::Note, ItemClass::Standard, note("later"))
            .unwrap();
        fx.with(|store| back.save(store, &device.adk)).unwrap();
        let heads = fx.with(|store| store.heads(device.ctx.vault_id)).unwrap();
        assert_eq!(heads.len() as u64, back.seq());
        let again = fx.with(|store| device.load(store, back.seq())).unwrap();
        assert_eq!(again.open_item(later).unwrap(), note("later"));

        // The wrong App Data Key opens nothing, and says why.
        let other = Device {
            ctx: device.ctx,
            device_id: device.device_id,
            seed: device.seed,
            adk: AppDataKey::generate().unwrap(),
            cjk: device.cjk.clone(),
        };
        assert!(matches!(
            fx.with(|store| other.load(store, 0)),
            Err(Error::Crypto(lockdown_crypto::Error::Tampered))
        ));
    }
}

#[test]
fn reopening_verifies_the_chain() {
    for mut fx in Fixture::all() {
        let device = Device::new();
        let mut vault = device.create();
        let id = vault
            .create_item(ItemKind::Note, ItemClass::Standard, note("a"))
            .unwrap();
        vault.update_item(id, note("b")).unwrap();
        fx.with(|store| vault.save(store, &device.adk)).unwrap();
        fx.with(|store| device.load(store, 0)).unwrap();

        // A head edited after signing.
        let mut forged = fx.with(|store| store.heads(device.ctx.vault_id)).unwrap();
        forged[1].entries[0].rev = 99;
        let head = forged[1].clone();
        fx.with(|store| store.put_head(head)).unwrap();
        let err = fx.with(|store| device.load(store, 0)).unwrap_err();
        assert!(
            matches!(err, Error::ChainBroken { seq: 2, .. }),
            "{}: {err}",
            fx.name()
        );
    }
}

#[test]
fn a_tampered_blob_is_refused() {
    for mut fx in Fixture::all() {
        let device = Device::new();
        let mut vault = device.create();
        let id = vault
            .create_item(ItemKind::Login, ItemClass::Standard, sample_login_fields())
            .unwrap();
        fx.with(|store| vault.save(store, &device.adk)).unwrap();
        let clean = fx.snapshot();

        // A byte of a sealed field flipped: the item no longer matches the
        // signed manifest, so it is refused before any field is opened.
        let mut item = fx
            .with(|store| store.get_item(device.ctx.vault_id, id))
            .unwrap();
        item.versions[0].fields[3].sealed.ct[0] ^= 1;
        fx.with(|store| store.put_item(item)).unwrap();
        let err = fx.with(|store| device.load(store, 0)).unwrap_err();
        assert!(
            matches!(err, Error::ManifestMismatch { item_id, .. } if item_id == id),
            "{}: {err}",
            fx.name()
        );

        // Relabelled: the header is under the version signature and hash.
        fx.restore(clean);
        let mut item = fx
            .with(|store| store.get_item(device.ctx.vault_id, id))
            .unwrap();
        item.kind = ItemKind::Note;
        fx.with(|store| store.put_item(item)).unwrap();
        assert!(matches!(
            fx.with(|store| device.load(store, 0)),
            Err(Error::ManifestMismatch { .. })
        ));

        // A byte of the wrapped version key flipped: outside the hash by
        // design (D36), so the load passes and the open fails as tampered.
        let mut item = fx
            .with(|store| store.get_item(device.ctx.vault_id, id))
            .unwrap();
        item.kind = ItemKind::Login;
        item.versions[0].version_key.as_mut().unwrap().sealed.ct[0] ^= 1;
        fx.with(|store| store.put_item(item)).unwrap();
        let back = fx.with(|store| device.load(store, 0)).unwrap();
        assert!(matches!(
            back.open_item(id),
            Err(Error::Crypto(lockdown_crypto::Error::Tampered))
        ));

        // A blob that is not even CBOR any more.
        if let Fixture::Sqlite { path, .. } = &fx {
            let conn = rusqlite::Connection::open(path).unwrap();
            conn.execute(
                "UPDATE items SET body = X'00' WHERE item_id = ?1",
                [id.to_string()],
            )
            .unwrap();
            drop(conn);
            assert!(matches!(
                fx.with(|store| device.load(store, 0)),
                Err(Error::Store(StoreError::Tampered(_)))
            ));
        }
    }
}

#[test]
fn a_rolled_back_store_is_refused() {
    for mut fx in Fixture::all() {
        let device = Device::new();
        let mut vault = device.create();
        vault
            .create_item(ItemKind::Note, ItemClass::Standard, note("one"))
            .unwrap();
        fx.with(|store| vault.save(store, &device.adk)).unwrap();
        let old_copy = fx.snapshot();
        let old_seq = vault.seq();

        vault
            .create_item(ItemKind::Note, ItemClass::Standard, note("two"))
            .unwrap();
        fx.with(|store| vault.save(store, &device.adk)).unwrap();
        let remembered = vault.seq();
        assert_eq!(remembered, old_seq + 1);
        fx.with(|store| device.load(store, remembered)).unwrap();

        // The old copy put back: the chain is valid, but behind what this
        // device remembers.
        fx.restore(old_copy);
        let err = fx.with(|store| device.load(store, remembered)).unwrap_err();
        assert!(
            err.to_string().starts_with("Rollback attempt blocked"),
            "{}: {err}",
            fx.name()
        );
        assert!(matches!(
            err,
            Error::Rollback { found, remembered: r } if found == old_seq && r == remembered
        ));
        // A device that remembers nothing accepts it: the memory is the control.
        fx.with(|store| device.load(store, 0)).unwrap();
    }
}

#[test]
fn stores_list_per_vault_and_round_trip_records() {
    for mut fx in Fixture::all() {
        let a = sample_item();
        let b = sample_item();
        let (va, ia) = (a.vault_id, a.item_id);
        fx.with(|store| {
            store.put_item(a.clone()).unwrap();
            store.put_item(b.clone()).unwrap();
            assert_eq!(store.get_item(va, ia).unwrap(), a);
            assert_eq!(store.list_items(va).unwrap(), vec![ia]);
            assert!(store.list_vaults().unwrap().is_empty());
            store.remove_item(va, ia).unwrap();
            assert!(matches!(
                store.get_item(va, ia),
                Err(StoreError::NotFound(_))
            ));
            assert!(matches!(
                store.remove_item(va, ia),
                Err(StoreError::NotFound(_))
            ));
            assert!(matches!(
                store.latest_head(va),
                Err(StoreError::NotFound(_))
            ));
            assert!(matches!(store.get_vault(va), Err(StoreError::NotFound(_))));
        });
        let device = Device::new();
        let vault = device.create();
        let record = vault.record(&device.adk).unwrap();
        fx.with(|store| store.put_vault(record.clone())).unwrap();
        assert_eq!(
            fx.with(|store| store.get_vault(device.ctx.vault_id))
                .unwrap(),
            record
        );
        assert_eq!(
            fx.with(|store| store.list_vaults()).unwrap(),
            vec![device.ctx.vault_id]
        );
    }
}

#[test]
fn the_sqlite_file_is_private_in_wal_mode_and_refuses_a_newer_schema() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested").join("vault.sqlite");
    let store = SqliteStore::open(&path).unwrap();
    assert_eq!(
        store.schema_version().unwrap(),
        lockdown_store::SCHEMA_VERSION
    );
    drop(store);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let dir_mode = std::fs::metadata(path.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(dir_mode, 0o700);
    }
    let conn = rusqlite::Connection::open(&path).unwrap();
    let mode: String = conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
    conn.execute(
        "INSERT INTO schema_version (version) VALUES (?1)",
        [lockdown_store::SCHEMA_VERSION + 1],
    )
    .unwrap();
    drop(conn);
    assert!(matches!(
        SqliteStore::open(&path),
        Err(StoreError::SchemaTooNew { .. })
    ));
}
