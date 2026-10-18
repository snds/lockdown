//! Post-quantum signatures end to end, against the real server
//! (docs/plan/28-pq-signatures.md): once both devices of an account read
//! them, every vault head carries ML-DSA-65 beside Ed25519 and the log's
//! tree heads do too; a server that bends either signature on a head, or
//! strips the post-quantum one, is refused by the device that pulls it, and
//! the device remembers where the post-quantum heads began.

use std::sync::Arc;

use lockdown_app::{App, ErrorCode, LockPolicy, Paths};
use lockdown_server::{AppState, Config, RunningServer, run_in_background};
use lockdown_store::{HeadMemory, TransparencyMemory};
use lockdown_vault::{Id, ManifestHead};

const LOGIN: &str = "me@example.com";
const PASSWORD: &str = "correct horse battery staple";

fn server_on_disk(dir: &std::path::Path) -> (Arc<AppState>, RunningServer) {
    let storage =
        lockdown_server::storage::SqliteStorage::open(&dir.join("server.sqlite")).unwrap();
    let keys =
        lockdown_server::keys::ServerKeys::load_or_create(&dir.join("server-key.cbor")).unwrap();
    let state = Arc::new(AppState::new(Box::new(storage), keys, Config::default()));
    let running = run_in_background(state.clone()).unwrap();
    (state, running)
}

fn device(dir: &std::path::Path, name: &str) -> App {
    let paths = Paths::resolve(Some(dir.join(name).join("snds/identity")), None).unwrap();
    App::start(paths, None, LockPolicy::default())
}

fn secret_key_of(page: &[u8]) -> String {
    let text = std::str::from_utf8(page).unwrap();
    text.lines()
        .find_map(|l| l.strip_prefix("Secret Key:"))
        .map(|v| v.trim().to_string())
        .expect("the kit page names the Secret Key")
}

/// The server's newest head of the account's vault: its id, `seq` and head.
fn newest_head(db: &std::path::Path) -> (String, i64, ManifestHead) {
    let conn = rusqlite::Connection::open(db).unwrap();
    conn.query_row(
        "SELECT vault_id, seq, body FROM heads ORDER BY seq DESC LIMIT 1",
        [],
        |r| {
            let body: Vec<u8> = r.get(2)?;
            Ok((
                r.get(0)?,
                r.get(1)?,
                ManifestHead::from_bytes(&body).unwrap(),
            ))
        },
    )
    .unwrap()
}

/// Rewrites the body the server holds for one head, as a hostile server would.
fn rewrite_head(db: &std::path::Path, vault_id: &str, seq: i64, head: &ManifestHead) {
    let conn = rusqlite::Connection::open(db).unwrap();
    let n = conn
        .execute(
            "UPDATE heads SET body = ?1 WHERE vault_id = ?2 AND seq = ?3",
            rusqlite::params![head.to_bytes().unwrap(), vault_id, seq],
        )
        .unwrap();
    assert_eq!(n, 1);
}

#[test]
fn both_devices_sign_heads_post_quantum_and_a_server_that_bends_or_strips_one_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let (_state, running) = server_on_disk(dir.path());
    let url = running.url();
    let db = dir.path().join("server.sqlite");

    // Device A alone: it marks itself, publishes the key, and its first
    // synced head carries both signatures.
    let a = device(dir.path(), "a");
    a.open_or_create(true).unwrap();
    let registered = a.account_register(&url, LOGIN, PASSWORD, None).unwrap();
    let secret_key = secret_key_of(registered.kit.as_ref().unwrap().expose());
    a.add_login("https://example.com", "me", b"pw".to_vec(), None)
        .unwrap();
    a.sync().unwrap();
    let (vault, first_seq, head) = newest_head(&db);
    assert_eq!(head.pq_sig.as_ref().map(|s| s.len()), Some(3309));

    // A pinned the server's post-quantum log key with its tree head.
    let a_data = std::path::PathBuf::from(a.probe().unwrap().data);
    let view = TransparencyMemory::open(&a_data.join("transparency.cbor"))
        .unwrap()
        .view(&url)
        .cloned()
        .unwrap();
    assert_eq!(view.pq_log_key.as_ref().map(|k| k.len()), Some(1952));
    assert!(view.sth.pq_sig.is_some());
    // And remembers where the post-quantum heads began.
    let vault_id = Id::parse(&vault).unwrap();
    let since = HeadMemory::open(&a_data.join("remembered.cbor"))
        .unwrap()
        .pq_since(vault_id)
        .expect("remembered");
    assert!(since <= first_seq as u64);

    // Device B joins, pulls the vault under both signatures, and signs its
    // own heads with both: the chain already carries them.
    let b = device(dir.path(), "b");
    b.account_login(&url, LOGIN, PASSWORD, Some(&secret_key))
        .unwrap();
    assert_eq!(b.open_or_create(true).unwrap().live_items, 1);
    b.add_login("https://example.org", "me", b"pw2".to_vec(), None)
        .unwrap();
    b.sync().unwrap();
    let (_, b_seq, b_head) = newest_head(&db);
    assert!(b_seq > first_seq);
    assert!(b_head.pq_sig.is_some(), "B's head is post-quantum too");
    a.sync().unwrap();

    // B saves once more; the server bends that head's ML-DSA signature
    // (its Ed25519 signature and hash untouched). A refuses it.
    b.add_login("https://example.net", "me", b"pw3".to_vec(), None)
        .unwrap();
    b.sync().unwrap();
    let (_, seq, good) = newest_head(&db);
    let mut bent = good.clone();
    bent.pq_sig.as_mut().unwrap()[11] ^= 1;
    rewrite_head(&db, &vault, seq, &bent);
    let err = a.sync().unwrap_err();
    assert_eq!(err.code, ErrorCode::Refused, "{}", err.message);
    assert!(
        err.message
            .contains("post-quantum signature didn't check out"),
        "{}",
        err.message
    );

    // The server strips it instead: refused, since the heads before carry one.
    let mut stripped = good.clone();
    stripped.pq_sig = None;
    rewrite_head(&db, &vault, seq, &stripped);
    let err = a.sync().unwrap_err();
    assert!(
        err.message.contains("lost its post-quantum signature"),
        "{}",
        err.message
    );

    // The server bends the Ed25519 signature: refused as before.
    let mut bent = good.clone();
    bent.sig[3] ^= 1;
    rewrite_head(&db, &vault, seq, &bent);
    let err = a.sync().unwrap_err();
    assert!(err.message.contains("device signature"), "{}", err.message);

    // Put back as B signed it: A takes it and sees all three logins.
    rewrite_head(&db, &vault, seq, &good);
    a.sync().unwrap();
    assert_eq!(a.status().unwrap().live_items, 3);
}
