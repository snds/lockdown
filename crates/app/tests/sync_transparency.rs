//! `App::sync` against the real server, audited like `lockdown sync`
//! (04-security §6; D78–D80): a clean sync verifies the log before and
//! after and remembers the view in `transparency.cbor`, where the Status
//! screen and the CLI both read it; a device that joins pulls its first
//! vault under the same check; a server that binds this device to another
//! key is refused by name before the vault round; a tree head smaller
//! than the one remembered is a rollback.

use std::sync::Arc;

use lockdown_app::{App, ErrorCode, LockPolicy, Paths};
use lockdown_server::{AppState, Config, RunningServer, run_in_background};
use lockdown_store::TransparencyMemory;
use lockdown_vault::transparency::{LogEntry, LogLeaf};

const LOGIN: &str = "me@example.com";
const PASSWORD: &str = "correct horse battery staple";

fn server() -> (Arc<AppState>, RunningServer) {
    let state = Arc::new(AppState::in_memory(Config::default()).unwrap());
    let running = run_in_background(state.clone()).unwrap();
    (state, running)
}

/// A server on disk, so its log can be rewritten under it.
fn server_on_disk(dir: &std::path::Path) -> (Arc<AppState>, RunningServer) {
    let storage =
        lockdown_server::storage::SqliteStorage::open(&dir.join("server.sqlite")).unwrap();
    let keys =
        lockdown_server::keys::ServerKeys::load_or_create(&dir.join("server-key.cbor")).unwrap();
    let state = Arc::new(AppState::new(Box::new(storage), keys, Config::default()));
    let running = run_in_background(state.clone()).unwrap();
    (state, running)
}

/// An app on its own identity root and data directory.
fn device(dir: &std::path::Path, name: &str) -> App {
    let paths = Paths::resolve(Some(dir.join(name).join("snds/identity")), None).unwrap();
    App::start(paths, None, LockPolicy::default())
}

/// The Secret Key from the kit page `account_register` hands back.
fn secret_key_of(page: &[u8]) -> String {
    let text = std::str::from_utf8(page).unwrap();
    text.lines()
        .find_map(|l| l.strip_prefix("Secret Key:"))
        .map(|v| v.trim().to_string())
        .expect("the kit page names the Secret Key")
}

#[test]
fn a_sync_audits_the_log_before_and_after_and_remembers_the_view() {
    let dir = tempfile::tempdir().unwrap();
    let (state, running) = server();
    let url = running.url();

    // Device A: identity, account, one login, sync.
    let a = device(dir.path(), "a");
    a.open_or_create(true).unwrap();
    let registered = a.account_register(&url, LOGIN, PASSWORD, None).unwrap();
    let secret_key = secret_key_of(registered.kit.as_ref().unwrap().expose());
    assert_eq!(
        state.storage.log_size().unwrap(),
        2,
        "A's device key and the identity manifest"
    );
    a.add_login("https://example.com", "me", b"pw".to_vec(), None)
        .unwrap();
    let outcome = a.sync().unwrap();
    assert_eq!(outcome.pushed, 1);
    // The second audit saw the head this sync added.
    assert_eq!(outcome.transparency.log_size, 3);
    assert_eq!(outcome.transparency.audited, 3);
    assert_eq!(outcome.transparency.root_prefix.len(), 16);
    assert!(outcome.transparency.checked_at.is_some());
    assert!(
        outcome
            .lines
            .iter()
            .any(|l| l.starts_with("transparency: log of 3 entries verified")),
        "{:?}",
        outcome.lines
    );

    // The view is remembered where the CLI reads it, and the status shows it.
    let data = std::path::PathBuf::from(a.probe().unwrap().data);
    let remembered = TransparencyMemory::open(&data.join("transparency.cbor")).unwrap();
    let view = remembered.view(&url).expect("remembered for this server");
    assert_eq!(view.sth.size, 3);
    assert_eq!(view.audited, 3);
    assert!(view.own_device_logged);
    let status = a.status().unwrap();
    let shown = status.account.unwrap().transparency.unwrap();
    assert_eq!(shown, outcome.transparency);
    // Still there after a lock: it is read from the file, not kept in memory.
    a.lock().unwrap();
    a.unlock().unwrap();
    let status = a.status().unwrap();
    assert_eq!(status.account.unwrap().transparency.unwrap().log_size, 3);

    // Device B joins from the account: its first pull goes through the
    // same check and leaves a view behind before anything else happens.
    let b = device(dir.path(), "b");
    b.account_login(&url, LOGIN, PASSWORD, Some(&secret_key))
        .unwrap();
    let status = b.open_or_create(true).unwrap();
    assert_eq!(status.live_items, 1, "A's vault was pulled");
    let shown = status.account.unwrap().transparency.unwrap();
    // A's device, the manifest, A's head, B's device, the manifest that lists B.
    assert_eq!(shown.log_size, 5);
    assert_eq!(shown.audited, 5);

    // A's next sync learns B, proves the log grew from what it saw, and
    // moves its view forward.
    let outcome = a.sync().unwrap();
    assert!(outcome.transparency.log_size >= 5);
    assert_eq!(outcome.transparency.audited, outcome.transparency.log_size);
}

#[test]
fn a_server_that_swaps_this_devices_key_is_refused_by_the_audit() {
    let dir = tempfile::tempdir().unwrap();
    let (state, running) = server_on_disk(dir.path());
    let url = running.url();
    let a = device(dir.path(), "a");
    a.open_or_create(true).unwrap();
    a.account_register(&url, LOGIN, PASSWORD, None).unwrap();
    assert_eq!(state.storage.log_size().unwrap(), 2);

    // The server rewrites the leaf that recorded this device's key: the
    // same device, another key. The tree is honest about the rewrite (the
    // inclusion proof holds), so only the audit against the identity's
    // own record of the key can catch it.
    {
        let conn = rusqlite::Connection::open(dir.path().join("server.sqlite")).unwrap();
        let mut stmt = conn
            .prepare("SELECT idx, body FROM kt_log ORDER BY idx")
            .unwrap();
        let rows: Vec<(i64, Vec<u8>)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        let (idx, body) = rows
            .iter()
            .find(|(_, body)| {
                matches!(
                    LogLeaf::from_bytes(body).unwrap().entry,
                    LogEntry::Device { .. }
                )
            })
            .expect("the device leaf");
        let mut leaf = LogLeaf::from_bytes(body).unwrap();
        if let LogEntry::Device { device_pub, .. } = &mut leaf.entry {
            *device_pub = [42; 32];
        }
        let body = leaf.to_bytes().unwrap();
        let hash = lockdown_crypto::merkle::leaf_hash(&body);
        conn.execute(
            "UPDATE kt_log SET leaf_hash = ?1, body = ?2 WHERE idx = ?3",
            rusqlite::params![&hash.0[..], body, idx],
        )
        .unwrap();
    }

    a.add_login("https://example.com", "me", b"pw".to_vec(), None)
        .unwrap();
    let err = a.sync().unwrap_err();
    assert_eq!(err.code, ErrorCode::Refused);
    assert!(
        err.message.contains("swapped a device key"),
        "{}",
        err.message
    );
    // Nothing went to the server and nothing was remembered.
    assert_eq!(state.storage.log_size().unwrap(), 2, "no head leaf");
    let account = a.status().unwrap().account.unwrap();
    assert_eq!(account.synced_seq, 0);
    assert!(account.transparency.is_none());
}

#[test]
fn a_tree_head_smaller_than_the_remembered_one_is_a_rollback() {
    let dir = tempfile::tempdir().unwrap();
    let (_state, running) = server();
    let url = running.url();
    let a = device(dir.path(), "a");
    a.open_or_create(true).unwrap();
    a.account_register(&url, LOGIN, PASSWORD, None).unwrap();
    let outcome = a.sync().unwrap();
    assert_eq!(outcome.transparency.log_size, 3);

    // This device remembers a bigger tree than the server now shows (as
    // if the server had been restored from an older backup). The memory
    // is read at open, so the session is reopened.
    a.lock().unwrap();
    let data = std::path::PathBuf::from(a.probe().unwrap().data);
    {
        let mut memory = TransparencyMemory::open(&data.join("transparency.cbor")).unwrap();
        let mut ahead = memory.view(&url).unwrap().clone();
        ahead.sth.size = 10;
        memory.remember(&url, ahead).unwrap();
    }
    a.unlock().unwrap();
    let err = a.sync().unwrap_err();
    assert_eq!(err.code, ErrorCode::Refused);
    assert!(
        err.message.contains("Rollback attempt blocked"),
        "{}",
        err.message
    );
    // The memory stands: the bigger tree is still what this device holds.
    let memory = TransparencyMemory::open(&data.join("transparency.cbor")).unwrap();
    assert_eq!(memory.view(&url).unwrap().sth.size, 10);
}
