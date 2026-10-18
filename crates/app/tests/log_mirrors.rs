//! Log mirrors (`key-transparency`; docs/plan/27-web-server-finish.md §4),
//! with two servers in this process: the account server, on disk so its
//! log can be rewritten under it, and a second server in mirror mode that
//! copies its signed tree heads.
//!
//! - a device compares the server's log with the mirror the server names
//!   on every sync, and says so when they agree;
//! - the server rewrites its history after the mirror copied it: a new
//!   device, with nothing remembered, can't tell by itself, but the
//!   mirror's head can't be connected to the log it is shown: a split
//!   view, in words, on the sync and in Status; the mirror catches the
//!   rewrite on its own next copy too;
//! - a mirror the server stops naming is still checked (remembered once
//!   seen), and one the person adds is checked beside it;
//! - in static mode the mirror writes the same head to a file a static
//!   host can serve.

use std::sync::Arc;
use std::time::Duration;

use lockdown_app::{App, LockPolicy, Paths};
use lockdown_server::mirror::Mirror;
use lockdown_server::{AppState, Config, RunningServer, run_in_background};
use lockdown_sync::mirror::{MIRROR_PATH, MirrorHead, MirrorVerdict};

const PASSWORD: &str = "correct horse battery staple";

/// The account server, on disk.
fn account_server(dir: &std::path::Path) -> (Arc<AppState>, RunningServer) {
    let storage =
        lockdown_server::storage::SqliteStorage::open(&dir.join("server.sqlite")).unwrap();
    let keys =
        lockdown_server::keys::ServerKeys::load_or_create(&dir.join("server-key.cbor")).unwrap();
    let state = Arc::new(AppState::new(Box::new(storage), keys, Config::default()));
    let running = run_in_background(state.clone()).unwrap();
    (state, running)
}

/// The second server, mirroring `upstream`, with its head also written
/// under `out` for a static host.
fn mirror_server(
    dir: &std::path::Path,
    upstream: &str,
    out: &std::path::Path,
) -> (Arc<Mirror>, RunningServer) {
    let state = Arc::new(AppState::in_memory(Config::default()).unwrap());
    let mirror = Arc::new(Mirror::open(upstream, dir, Some(out), Duration::from_secs(60)).unwrap());
    state.mirrors.set_mirror(mirror.clone());
    let running = run_in_background(state).unwrap();
    (mirror, running)
}

fn device(dir: &std::path::Path, name: &str) -> App {
    let paths = Paths::resolve(Some(dir.join(name).join("snds/identity")), None).unwrap();
    App::start(paths, None, LockPolicy::default())
}

fn now() -> i64 {
    lockdown_vault::Timestamp::now().0
}

/// Rewrites the first leaf of the account server's log: the history
/// before the mirror's copy is no longer the history the server shows.
fn rewrite_first_leaf(dir: &std::path::Path) {
    let conn = rusqlite::Connection::open(dir.join("server.sqlite")).unwrap();
    let body: Vec<u8> = conn
        .query_row("SELECT body FROM kt_log ORDER BY idx LIMIT 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    let mut forged = body.clone();
    let last = forged.len() - 1;
    forged[last] ^= 1;
    let hash = lockdown_crypto::merkle::leaf_hash(&forged);
    conn.execute(
        "UPDATE kt_log SET leaf_hash = ?1, body = ?2 WHERE idx = (SELECT MIN(idx) FROM kt_log)",
        rusqlite::params![&hash.0[..], forged],
    )
    .unwrap();
}

#[test]
fn a_mirror_agrees_then_catches_a_server_that_rewrote_its_history() {
    let dir = tempfile::tempdir().unwrap();
    let (account, running) = account_server(&dir.path().join("account"));
    let url = running.url();
    let out = dir.path().join("static");
    let (mirror, mirror_running) = mirror_server(&dir.path().join("mirror"), &url, &out);
    let mirror_url = mirror_running.url();
    account
        .mirrors
        .set_named(std::slice::from_ref(&mirror_url))
        .unwrap();

    // Device A: an account, a login, a sync. The mirror hasn't copied
    // anything yet, so it can't vouch: said, not counted as agreement.
    let a = device(dir.path(), "a");
    a.open_or_create(true).unwrap();
    a.account_register(&url, "a@example.com", PASSWORD, None)
        .unwrap();
    a.add_login("https://example.com", "me", b"pw".to_vec(), None)
        .unwrap();
    let first = a.sync().unwrap();
    assert_eq!(first.transparency.mirrors.len(), 1);
    assert!(
        matches!(
            first.transparency.mirrors[0].verdict,
            MirrorVerdict::Unreachable { .. }
        ),
        "{:?}",
        first.transparency.mirrors
    );
    assert_eq!(first.transparency.mirrors[0].named_by, "server");

    // The mirror copies; the next sync agrees with it.
    let said = mirror.poll_once(now()).unwrap();
    assert!(said.starts_with("first head copied"), "{said}");
    let copied = mirror.head().unwrap();
    // The static file holds the same head.
    let file = std::fs::read(out.join(MIRROR_PATH.trim_start_matches('/'))).unwrap();
    assert_eq!(
        lockdown_sync::wire::decode::<MirrorHead>(&file).unwrap(),
        copied
    );
    let agreed = a.sync().unwrap();
    assert!(
        matches!(
            agreed.transparency.mirrors[0].verdict,
            MirrorVerdict::Agrees { size, .. } if size == copied.sth.size
        ),
        "{:?}",
        agreed.transparency.mirrors
    );
    assert!(
        agreed.lines.iter().any(|l| l.contains("agrees")),
        "{:?}",
        agreed.lines
    );
    // The mirror follows a log that only grows.
    a.add_login("https://two.example", "me", b"pw2".to_vec(), None)
        .unwrap();
    a.sync().unwrap();
    let said = mirror.poll_once(now()).unwrap();
    assert!(said.starts_with("copied"), "{said}");
    assert!(!a.sync().unwrap().transparency.mirrors[0].verdict.is_split());
    let copied = mirror.head().unwrap();

    // The server rewrites its history under the copy and stops naming the
    // mirror. Device B is new: it has no memory of this log to hold the
    // server's word against, so on its own it would accept the rewritten
    // log. The server never named the mirror to B, so the person adds it
    // in Settings, as someone who runs their own mirror does.
    rewrite_first_leaf(&dir.path().join("account"));
    account.mirrors.set_named(&[]).unwrap();
    let b = device(dir.path(), "b");
    b.open_or_create(true).unwrap();
    b.set_kt_mirrors(vec![format!("{mirror_url}/")]).unwrap();
    assert_eq!(b.kt_mirrors().unwrap().configured, vec![mirror_url.clone()]);
    b.account_register(&url, "b@example.com", PASSWORD, None)
        .unwrap();
    let split = b.sync().unwrap();
    let check = &split.transparency.mirrors[0];
    assert_eq!(check.named_by, "settings");
    let MirrorVerdict::Split { message } = &check.verdict else {
        panic!("expected a split view, got {:?}", check.verdict);
    };
    assert!(message.starts_with("Split view"), "{message}");
    assert!(
        split
            .lines
            .iter()
            .any(|l| l.contains(&mirror_url) && l.contains("Split view")),
        "{:?}",
        split.lines
    );
    // Status keeps saying it, after the sync and after a lock.
    b.lock().unwrap();
    let status = b.open_or_create(false).unwrap();
    let shown = status.account.unwrap().transparency.unwrap();
    assert!(shown.mirrors[0].verdict.is_split());
    assert!(b.kt_mirrors().unwrap().checks[0].verdict.is_split());

    // Device A remembered the server's old log itself: its own check
    // refuses the rewrite before any mirror is asked.
    let err = a.sync().unwrap_err();
    assert!(
        err.message.contains("Split view") || err.message.contains("rewritten"),
        "{}",
        err.message
    );
    // A's last word from the mirror the server no longer names stays.
    assert_eq!(
        a.kt_mirrors().unwrap().checks[0].named_by,
        "server",
        "the last word stays"
    );

    // The mirror's own next copy catches the rewrite, keeps its last good
    // head, and serves the problem with it.
    let said = mirror.poll_once(now()).unwrap();
    assert!(said.starts_with("PROBLEM"), "{said}");
    let after = mirror.head().unwrap();
    assert_eq!(after.sth, copied.sth, "the last good head stays");
    assert!(after.problem.as_deref().unwrap().contains("rewritten"));
    let again = b.sync().unwrap();
    assert!(again.transparency.mirrors[0].verdict.is_split());
}

#[test]
fn a_mirror_of_another_server_and_a_bad_address_are_said_so() {
    let dir = tempfile::tempdir().unwrap();
    let (_account, running) = account_server(&dir.path().join("account"));
    let url = running.url();
    let (_other, other_running) = account_server(&dir.path().join("other"));
    let (mirror, mirror_running) = mirror_server(
        &dir.path().join("mirror"),
        &other_running.url(),
        &dir.path().join("static"),
    );
    mirror.poll_once(now()).unwrap();
    let a = device(dir.path(), "a");
    a.open_or_create(true).unwrap();
    let refused = a.set_kt_mirrors(vec!["http://mirror.example".into()]);
    assert!(refused.unwrap_err().message.contains("https://"));
    a.set_kt_mirrors(vec![mirror_running.url()]).unwrap();
    a.account_register(&url, "a@example.com", PASSWORD, None)
        .unwrap();
    let outcome = a.sync().unwrap();
    let MirrorVerdict::Unusable { message } = &outcome.transparency.mirrors[0].verdict else {
        panic!("{:?}", outcome.transparency.mirrors);
    };
    assert!(message.contains("copies"), "{message}");
    // Settings' general save never drops the mirrors.
    let mut s = a.settings().unwrap();
    s.kt_mirrors.clear();
    a.set_settings(s).unwrap();
    assert_eq!(
        a.kt_mirrors().unwrap().configured,
        vec![mirror_running.url()]
    );
}

#[test]
fn a_mirror_the_server_stops_naming_is_still_checked() {
    let dir = tempfile::tempdir().unwrap();
    let (account, running) = account_server(&dir.path().join("account"));
    let url = running.url();
    let (mirror, mirror_running) =
        mirror_server(&dir.path().join("mirror"), &url, &dir.path().join("static"));
    account.mirrors.set_named(&[mirror_running.url()]).unwrap();
    let a = device(dir.path(), "a");
    a.open_or_create(true).unwrap();
    a.account_register(&url, "a@example.com", PASSWORD, None)
        .unwrap();
    a.sync().unwrap();
    mirror.poll_once(now()).unwrap();
    // The server no longer names it; the device remembers it was named.
    account.mirrors.set_named(&[]).unwrap();
    let outcome = a.sync().unwrap();
    assert_eq!(outcome.transparency.mirrors.len(), 1);
    assert_eq!(outcome.transparency.mirrors[0].named_by, "server");
    assert!(
        matches!(
            outcome.transparency.mirrors[0].verdict,
            MirrorVerdict::Agrees { .. }
        ),
        "{:?}",
        outcome.transparency.mirrors
    );
}

/// A mirror server over a copy of `from`'s state file with its
/// post-quantum key changed by `bend`.
fn bent_mirror(
    from: &std::path::Path,
    to: &std::path::Path,
    upstream: &str,
    bend: impl Fn(&mut Vec<(ciborium::Value, ciborium::Value)>),
) -> RunningServer {
    let bytes = std::fs::read(from.join(lockdown_server::mirror::STATE_FILE)).unwrap();
    let mut value: ciborium::Value = ciborium::de::from_reader(&bytes[..]).unwrap();
    let ciborium::Value::Map(map) = &mut value else {
        panic!("the mirror's file is a map")
    };
    bend(map);
    std::fs::create_dir_all(to).unwrap();
    let mut out = Vec::new();
    ciborium::ser::into_writer(&value, &mut out).unwrap();
    std::fs::write(to.join(lockdown_server::mirror::STATE_FILE), out).unwrap();
    let state = Arc::new(AppState::in_memory(Config::default()).unwrap());
    let mirror = Arc::new(Mirror::open(upstream, to, None, Duration::from_secs(60)).unwrap());
    state.mirrors.set_mirror(mirror);
    run_in_background(state).unwrap()
}

#[test]
fn a_mirror_that_saw_another_or_no_post_quantum_log_key_is_a_split() {
    let dir = tempfile::tempdir().unwrap();
    let (_account, running) = account_server(&dir.path().join("account"));
    let url = running.url();
    let (mirror, _mirror_running) =
        mirror_server(&dir.path().join("mirror"), &url, &dir.path().join("static"));
    let a = device(dir.path(), "a");
    a.open_or_create(true).unwrap();
    a.account_register(&url, "a@example.com", PASSWORD, None)
        .unwrap();
    a.sync().unwrap();
    mirror.poll_once(now()).unwrap();
    assert!(
        mirror.head().unwrap().pq_log_key.is_some(),
        "the mirror pinned it"
    );
    let key = ciborium::Value::Text("pq_log_key".into());
    let other = bent_mirror(
        &dir.path().join("mirror"),
        &dir.path().join("other"),
        &url,
        |m| {
            for (k, v) in m.iter_mut() {
                if *k == key
                    && let ciborium::Value::Bytes(b) = v
                {
                    b[0] ^= 1;
                }
            }
        },
    );
    let none = bent_mirror(
        &dir.path().join("mirror"),
        &dir.path().join("none"),
        &url,
        |m| m.retain(|(k, _)| *k != key),
    );
    a.set_kt_mirrors(vec![other.url(), none.url()]).unwrap();
    let outcome = a.sync().unwrap();
    let words: Vec<String> = outcome
        .transparency
        .mirrors
        .iter()
        .map(|c| match &c.verdict {
            MirrorVerdict::Split { message } => message.clone(),
            v => panic!("{} should be a split, got {v:?}", c.url),
        })
        .collect();
    assert!(
        words[0].contains("another post-quantum key"),
        "{}",
        words[0]
    );
    assert!(
        words[1].contains("without the post-quantum key"),
        "{}",
        words[1]
    );
}
