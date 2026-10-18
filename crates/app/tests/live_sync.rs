//! Live sync between two apps through the real server
//! (docs/plan/20-self-host-server.md §5–§6): with the live channel open, a
//! change on one device appears on the other without anyone pressing
//! Sync; the session list names both devices; ending the other device's
//! session reaches it on the channel, which forgets the dead token so its
//! next sync asks for a login.

use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use lockdown_app::live::{LiveNotice, LiveSink};
use lockdown_app::{App, LockPolicy, Paths};
use lockdown_server::{AppState, Config, run_in_background};

const LOGIN: &str = "me@example.com";
const PASSWORD: &str = "correct horse battery staple";

fn device(dir: &std::path::Path, name: &str) -> App {
    let paths = Paths::resolve(Some(dir.join(name).join("snds/identity")), None).unwrap();
    App::start(paths, None, LockPolicy::default())
}

fn secret_key_of(page: &[u8]) -> String {
    std::str::from_utf8(page)
        .unwrap()
        .lines()
        .find_map(|l| l.strip_prefix("Secret Key:"))
        .map(|v| v.trim().to_string())
        .expect("the kit page names the Secret Key")
}

fn next(rx: &mpsc::Receiver<LiveNotice>, want: impl Fn(&LiveNotice) -> bool) -> LiveNotice {
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        let n = rx.recv_timeout(left).expect("the notice arrives");
        if want(&n) {
            return n;
        }
    }
}

#[test]
fn a_change_on_one_device_shows_on_the_other_without_a_sync_and_sessions_end() {
    let dir = tempfile::tempdir().unwrap();
    let config = Config {
        per_login: (100, 10.0),
        ..Config::default()
    };
    let running = run_in_background(Arc::new(AppState::in_memory(config).unwrap())).unwrap();
    let url = running.url();

    let a = device(dir.path(), "a");
    a.open_or_create(true).unwrap();
    let registered = a.account_register(&url, LOGIN, PASSWORD, None).unwrap();
    let secret_key = secret_key_of(registered.kit.as_ref().unwrap().expose());
    a.add_login("https://one.example", "me", b"pw".to_vec(), None)
        .unwrap();
    a.sync().unwrap();

    let b = device(dir.path(), "b");
    b.account_login(&url, LOGIN, PASSWORD, Some(&secret_key))
        .unwrap();
    b.open_or_create(true).unwrap();
    assert_eq!(b.list().unwrap().len(), 1);

    let (tx, rx) = mpsc::channel();
    let tx = std::sync::Mutex::new(tx);
    let sink: LiveSink = Arc::new(move |n| {
        let _ = tx.lock().unwrap().send(n);
    });
    let _live = b.start_live(sink);
    next(&rx, |n| *n == LiveNotice::Connected);

    // A adds two items and syncs: B pulls them by itself, in one sync.
    a.add_login("https://two.example", "me", b"pw2".to_vec(), None)
        .unwrap();
    a.add_login("https://three.example", "me", b"pw3".to_vec(), None)
        .unwrap();
    a.sync().unwrap();
    let synced = next(&rx, |n| matches!(n, LiveNotice::Synced { .. }));
    let LiveNotice::Synced { pulled, .. } = synced else {
        unreachable!()
    };
    assert!(pulled >= 2, "{pulled}");
    assert_eq!(b.list().unwrap().len(), 3);

    // Both devices' sessions are listed, names and all.
    let sessions = a.sessions().unwrap();
    assert_eq!(sessions.len(), 2, "{sessions:?}");
    assert_eq!(sessions.iter().filter(|s| s.current).count(), 1);
    assert!(
        sessions.iter().all(|s| s.device_name.is_some()),
        "{sessions:?}"
    );
    assert!(
        sessions
            .iter()
            .all(|s| s.client.as_deref().unwrap_or("").starts_with("lockdown/"))
    );

    // A ends B's session: B hears it and forgets the token.
    assert_eq!(a.end_other_sessions().unwrap(), 1);
    next(&rx, |n| *n == LiveNotice::SessionEnded);
    let err = b.sync().unwrap_err();
    assert!(err.message.contains("log in again"), "{}", err.message);
    assert_eq!(a.sessions().unwrap().len(), 1);
    // A's own session still works.
    a.sync().unwrap();
}
