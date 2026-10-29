//! Session renewal end to end with the `lockdown` binary
//! (docs/plan/33-session-renewal.md): with a two-second session lifetime
//! the command line keeps syncing well past it without the password, and
//! when the server's clock jumps a day it renews after the refusal; once
//! the operator ends the account's sessions, the next sync stops with words
//! that say so and how to sign in, and a sign-in with the password brings
//! it back.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use lockdown_server::{AppState, Config, run_in_background};

const LOGIN: &str = "me@example.com";
const PASSWORD: &str = "correct horse battery staple\n";

fn run(root: &Path, args: &[&str], stdin: Option<&str>) -> (bool, String, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_lockdown"))
        .arg("--root")
        .arg(root)
        .args(args)
        .env_remove("LOCKDOWN_PASSWORD")
        .env("HOSTNAME", "laptop")
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the lockdown binary runs");
    if let Some(text) = stdin {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(text.as_bytes())
            .unwrap();
    }
    let out = child.wait_with_output().unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn ok(root: &Path, args: &[&str], stdin: Option<&str>) -> String {
    let (ok, out, err) = run(root, args, stdin);
    assert!(ok, "lockdown {args:?} failed:\n{out}\n{err}");
    out
}

fn add(root: &Path, site: &str) {
    ok(
        root,
        &[
            "add",
            "login",
            "--site",
            site,
            "--username",
            "me",
            "--password-stdin",
        ],
        Some("pw\n"),
    );
}

fn token_in(root: &Path) -> Vec<u8> {
    // The data directory beside the identity root holds `sync.cbor`.
    let data: PathBuf = root.parent().unwrap().join("lockdown").join("sync.cbor");
    std::fs::read(data).unwrap()
}

#[test]
fn the_command_line_syncs_past_a_short_session_and_an_ended_one_asks_for_the_password() {
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(AtomicI64::new(lockdown_vault::Timestamp::now().0));
    let mut state = AppState::in_memory(Config {
        // `--session-ttl 2`.
        session_ttl: Duration::from_secs(2),
        per_login: (100, 10.0),
        ..Config::default()
    })
    .unwrap();
    let c = clock.clone();
    state.clock = Box::new(move || c.load(Ordering::SeqCst));
    let state = Arc::new(state);
    let server = run_in_background(state.clone()).unwrap();
    let url = server.url();
    let root = dir.path().join("laptop").join("snds").join("identity");

    ok(&root, &["init"], None);
    ok(
        &root,
        &[
            "account",
            "register",
            "--server",
            &url,
            "--login",
            LOGIN,
            "--password-stdin",
        ],
        Some(PASSWORD),
    );
    ok(&root, &["sync"], None);

    // Well past the two seconds, three times over: every sync goes through
    // with no password, each on a renewed session.
    for i in 0..3 {
        std::thread::sleep(Duration::from_millis(2_200));
        clock.store(lockdown_vault::Timestamp::now().0, Ordering::SeqCst);
        let before = token_in(&root);
        add(&root, &format!("https://site{i}.example"));
        let out = ok(&root, &["sync"], None);
        assert!(out.contains("pushed"), "{out}");
        assert_ne!(
            token_in(&root),
            before,
            "round {i}: the session was renewed"
        );
    }

    // The server's clock jumps a day: the token this device holds is
    // refused, renewed after the refusal, and the sync goes on.
    clock.fetch_add(86_400, Ordering::SeqCst);
    ok(&root, &["sync"], None);

    // The operator ends the account's sessions (D296): the next sync can't
    // renew, and says why and what to type.
    let now = clock.load(Ordering::SeqCst);
    let account = state
        .storage
        .account_by_login(LOGIN)
        .unwrap()
        .unwrap()
        .account_id;
    let ended = state
        .storage
        .sessions()
        .unwrap()
        .end_other_sessions(account, &[0; 16], now, 30 * 86_400)
        .unwrap();
    assert_eq!(ended.len(), 1);
    let (synced, _, err) = run(&root, &["sync"], None);
    assert!(!synced);
    assert!(err.contains("was ended"), "{err}");
    assert!(err.contains("lockdown account login"), "{err}");
    // Still no session on the next try; nothing knocks for it.
    let (synced, _, err) = run(&root, &["sync"], None);
    assert!(!synced);
    assert!(err.contains("account login"), "{err}");

    // The password brings it back.
    ok(
        &root,
        &[
            "account",
            "login",
            "--server",
            &url,
            "--login",
            LOGIN,
            "--password-stdin",
        ],
        Some(PASSWORD),
    );
    ok(&root, &["sync"], None);
    let listed = ok(&root, &["list"], None);
    assert_eq!(listed.matches("https://site").count(), 3, "{listed}");
}
