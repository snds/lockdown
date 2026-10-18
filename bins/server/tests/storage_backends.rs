//! The second storage pass (docs/plan/25-self-host-2.md §2–§3), against the
//! real outside services when CI gives them:
//!
//! - attachment bodies in an S3-compatible bucket (MinIO in CI:
//!   `LOCKDOWN_TEST_S3_ENDPOINT`, `…_ACCESS_KEY`, `…_SECRET_KEY`): stored,
//!   served, counted against the quota, collected with their rows, carried
//!   by a backup and restored into files;
//! - a Postgres store (`LOCKDOWN_TEST_DATABASE_URL`): backed up into the one
//!   snapshot format and restored into another, empty Postgres database;
//!   one serving server per database.
//!
//! - a Postgres with TLS (`LOCKDOWN_TEST_DATABASE_TLS_URL` and its CA in
//!   `LOCKDOWN_TEST_DATABASE_CA`): the connection is TLS 1.3 and another CA
//!   is refused.
//!
//! Without those variables each test says so and passes; CI sets
//! `LOCKDOWN_TEST_SERVICES=require` so a missing Postgres or bucket fails
//! there.

use std::sync::Arc;

use lockdown_crypto::SigningKey;
use lockdown_crypto::identity::{AppDataKey, SymmetricKey};
use lockdown_server::backup::{self, BackupKey, RestoreTarget};
use lockdown_server::db::PgOptions;
use lockdown_server::s3::{S3Bodies, S3Config};
use lockdown_server::storage::{BlobBodies, SqlStorage, Storage};
use lockdown_vault::test_support::sample_context;
use lockdown_vault::{Id, Vault};

fn service(name: &str) -> Option<String> {
    match std::env::var(name) {
        Ok(v) if !v.trim().is_empty() => Some(v.trim().to_string()),
        _ => {
            assert!(
                std::env::var("LOCKDOWN_TEST_SERVICES").as_deref() != Ok("require"),
                "{name} is required in this run"
            );
            eprintln!("{name} isn't set: this test needs that service and is skipped here");
            None
        }
    }
}

fn bucket() -> Option<S3Bodies> {
    let endpoint = service("LOCKDOWN_TEST_S3_ENDPOINT")?;
    let suffix = &Id::new().to_string()[..8];
    let bodies = S3Bodies::new(S3Config {
        endpoint,
        bucket: format!("lockdown-test-{suffix}"),
        region: "us-east-1".into(),
        access_key: std::env::var("LOCKDOWN_TEST_S3_ACCESS_KEY").unwrap_or_default(),
        secret_key: zeroize::Zeroizing::new(
            std::env::var("LOCKDOWN_TEST_S3_SECRET_KEY").unwrap_or_default(),
        ),
        prefix: "attachments/".into(),
        virtual_hosted: false,
    })
    .unwrap();
    bodies.ensure_bucket().unwrap();
    bodies.check().unwrap();
    Some(bodies)
}

/// A vault owned by a new account in `storage`.
fn vault_in(storage: &SqlStorage, login: &str) -> (Id, Id) {
    let owner = storage.create_account(login, Id::new(), b"file").unwrap();
    let device = Id::new();
    let vault = Vault::create(
        sample_context(),
        device,
        Box::new(SigningKey::from_bytes([5; 32])),
        None,
    )
    .unwrap();
    let vault_id = vault.context().vault_id;
    let record = vault
        .record(&AppDataKey::generate().unwrap())
        .unwrap()
        .to_bytes()
        .unwrap();
    storage
        .put_vault(owner.account_id, vault_id, &record)
        .unwrap();
    (owner.account_id, vault_id)
}

#[test]
fn attachment_bodies_live_in_the_bucket_and_go_with_their_rows() {
    let Some(bodies) = bucket() else { return };
    let bodies = Arc::new(bodies);
    let dir = tempfile::tempdir().unwrap();
    let storage = SqlStorage::open(&dir.path().join("db.sqlite"))
        .unwrap()
        .with_bodies(bodies.clone());
    let (owner, vault_id) = vault_in(&storage, "me@example.com");
    let body = vec![7u8; 300_000];
    let hash = lockdown_crypto::hash::hash(&body);
    assert!(storage.put_blob(vault_id, hash, &body, 1_000).unwrap());
    assert!(!storage.put_blob(vault_id, hash, &body, 1_001).unwrap());
    assert_eq!(bodies.get(vault_id, hash).unwrap().unwrap(), body);
    assert_eq!(storage.blob(vault_id, hash).unwrap().unwrap(), body);
    // The quota counts it from its row, wherever the body is.
    assert_eq!(storage.account_blob_bytes(owner).unwrap(), 300_000);
    // The database holds no copy.
    let db = std::fs::read(dir.path().join("db.sqlite")).unwrap();
    assert!(!db.windows(64).any(|w| w.iter().all(|b| *b == 7)));

    // A backup carries the object; a restore puts it in files.
    let key_file = dir.path().join("server-key.cbor");
    lockdown_server::keys::ServerKeys::load_or_create(&key_file).unwrap();
    let key = BackupKey::generate().unwrap();
    let out = dir.path().join("b.ldbk");
    let summary = backup::create(
        &backup::Sources {
            storage: &storage,
            server_key: &key_file,
        },
        &key,
        &out,
        2_000,
    )
    .unwrap();
    assert_eq!(summary.blobs, 1);
    let restored = dir.path().join("restored");
    backup::restore(
        &out,
        &key,
        &RestoreTarget {
            data: restored.clone(),
            database: None,
            bodies: None,
        },
    )
    .unwrap();
    let back = SqlStorage::open(&restored.join(backup::DB_NAME))
        .unwrap()
        .with_blob_dir(&restored.join("blobs"))
        .unwrap();
    assert_eq!(back.blob(vault_id, hash).unwrap().unwrap(), body);

    // Nothing names it: after the grace period row and object go.
    assert_eq!(storage.gc_blobs(10_000, 3_600).unwrap(), 1);
    assert!(bodies.get(vault_id, hash).unwrap().is_none());
    assert!(storage.blob(vault_id, hash).unwrap().is_none());
}

#[test]
fn a_bucket_with_the_wrong_key_is_refused_at_start() {
    let Some(endpoint) = service("LOCKDOWN_TEST_S3_ENDPOINT") else {
        return;
    };
    let wrong = S3Bodies::new(S3Config {
        endpoint,
        bucket: "lockdown-test-nope".into(),
        region: "us-east-1".into(),
        access_key: "not-a-key".into(),
        secret_key: zeroize::Zeroizing::new("not-a-secret".into()),
        prefix: String::new(),
        virtual_hosted: false,
    })
    .unwrap();
    let e = wrong.check().unwrap_err().to_string();
    assert!(e.contains("refused") || e.contains("no such bucket"), "{e}");
}

fn pg(url: &str, exclusive: bool) -> PgOptions {
    let mut options = PgOptions::new(url);
    options.schema = Some(format!("lockdown_t_{}", &Id::new().to_string()[..8]));
    options.drop_schema_on_close = true;
    options.exclusive = exclusive;
    options
}

#[test]
fn a_postgres_store_backs_up_and_restores_into_another_postgres_database() {
    let Some(url) = service("LOCKDOWN_TEST_DATABASE_URL") else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let from = SqlStorage::open_postgres(pg(&url, true))
        .unwrap()
        .with_blob_dir(&dir.path().join("blobs"))
        .unwrap();
    let (owner, vault_id) = vault_in(&from, "pg@example.com");
    from.register_device(owner, Id::new(), &[9; 32]).unwrap();
    let body = vec![3u8; 2_000];
    let hash = lockdown_crypto::hash::hash(&body);
    from.put_blob(vault_id, hash, &body, 10).unwrap();
    let key_file = dir.path().join("server-key.cbor");
    lockdown_server::keys::ServerKeys::load_or_create(&key_file).unwrap();
    let key = BackupKey::generate().unwrap();
    let out = dir.path().join("pg.ldbk");
    backup::create(
        &backup::Sources {
            storage: &from,
            server_key: &key_file,
        },
        &key,
        &out,
        20,
    )
    .unwrap();
    // The snapshot is the ordinary SQLite one: it verifies as any does.
    let (_, facts) = backup::verify(&out, &key).unwrap();
    assert_eq!(facts.accounts, 1);

    let target = pg(&url, false);
    let schema = target.schema.clone().unwrap();
    let restored = dir.path().join("restored");
    backup::restore(
        &out,
        &key,
        &RestoreTarget {
            data: restored.clone(),
            database: Some(PgOptions {
                drop_schema_on_close: false,
                ..target.clone()
            }),
            bodies: None,
        },
    )
    .unwrap();
    let back = SqlStorage::open_postgres(target)
        .unwrap()
        .with_blob_dir(&restored.join("blobs"))
        .unwrap();
    assert_eq!(back.counts().unwrap(), from.counts().unwrap());
    assert_eq!(
        back.log_leaf_hashes().unwrap(),
        from.log_leaf_hashes().unwrap()
    );
    assert_eq!(back.blob(vault_id, hash).unwrap().unwrap(), body);
    assert!(restored.join(backup::KEY_NAME).exists());
    // A restore over a database that holds accounts is refused.
    let again = backup::restore(
        &out,
        &key,
        &RestoreTarget {
            data: dir.path().join("again"),
            database: Some(PgOptions {
                schema: Some(schema),
                drop_schema_on_close: false,
                ..PgOptions::new(&url)
            }),
            bodies: None,
        },
    );
    assert!(again.unwrap_err().contains("already holds"));
}

#[test]
fn one_serving_server_per_postgres_database() {
    let Some(url) = service("LOCKDOWN_TEST_DATABASE_URL") else {
        return;
    };
    let first = pg(&url, true);
    let mut second = first.clone();
    second.drop_schema_on_close = false;
    let _held = SqlStorage::open_postgres(first).unwrap();
    let e = SqlStorage::open_postgres(second).unwrap_err().to_string();
    assert!(e.contains("another lockdown-server"), "{e}");
}

#[test]
fn a_postgres_store_away_from_plain_text_speaks_tls_and_checks_the_ca() {
    // Not part of `LOCKDOWN_TEST_SERVICES=require`: CI's Postgres service
    // has no certificate; run it against a TLS Postgres by hand.
    let Some(url) = std::env::var("LOCKDOWN_TEST_DATABASE_TLS_URL")
        .ok()
        .filter(|u| !u.trim().is_empty())
    else {
        eprintln!("LOCKDOWN_TEST_DATABASE_TLS_URL isn't set: this test needs a Postgres with TLS");
        return;
    };
    let ca =
        std::env::var("LOCKDOWN_TEST_DATABASE_CA").expect("the database CA beside its TLS URL");
    let mut options = pg(&url, false);
    options.ca = Some(lockdown_tls::read_certs(std::path::Path::new(&ca)).unwrap());
    let store = SqlStorage::open_postgres(options).unwrap();
    let tls = store.database_tls().unwrap();
    assert_eq!(tls.as_deref(), Some("TLSv1.3"));
    // Another CA isn't trusted: the connection is refused, never made plain.
    let mut wrong = pg(&url, false);
    wrong.ca = Some(vec![lockdown_tls::testing::pki(&["localhost"]).ca_der]);
    assert!(SqlStorage::open_postgres(wrong).is_err());
}
