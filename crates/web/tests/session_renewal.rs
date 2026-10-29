//! Session renewal in the web vault, natively against the real server in
//! process (docs/plan/33-session-renewal.md): an open tab renews its
//! session with the device key it holds (D173) when the server refuses the
//! token, so it keeps syncing and the live channel gets the new token; a
//! locked tab still needs the password to open (D174); a session ended
//! from another device stops with a sign-in reason, not a plain refusal.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use lockdown_server::{AppState, Config, RunningServer, run_in_background};
use lockdown_web::{Reply, WebPaths, WebVault, call};
use serde_json::{Value, json};

const PASSWORD: &str = "correct horse battery staple";
const LOGIN: &str = "me@example.com";

fn server() -> (RunningServer, Arc<AtomicI64>) {
    let clock = Arc::new(AtomicI64::new(lockdown_vault::Timestamp::now().0));
    let mut state = AppState::in_memory(Config {
        per_login: (1000, 1000.0),
        per_address: (1000, 1000.0),
        ..Config::default()
    })
    .unwrap();
    let c = clock.clone();
    state.clock = Box::new(move || c.load(Ordering::SeqCst));
    (run_in_background(Arc::new(state)).unwrap(), clock)
}

fn ok(web: &mut WebVault, cmd: &str, args: Value) -> Value {
    match call(web, cmd, args) {
        Ok(Reply::Json(v)) => v,
        Ok(Reply::Bytes(b)) => panic!("{cmd}: bytes ({} of them), not JSON", b.len()),
        Err(e) => panic!("{cmd}: {e}"),
    }
}

/// The account, made the way the desktop app does; its Secret Key.
fn register(dir: &std::path::Path, url: &str) -> (lockdown_app::App, String) {
    let paths = lockdown_app::Paths::resolve(Some(dir.join("desktop/identity")), None).unwrap();
    let app = lockdown_app::App::start(paths, None, lockdown_app::LockPolicy::default());
    app.open_or_create(true).unwrap();
    let registered = app.account_register(url, LOGIN, PASSWORD, None).unwrap();
    app.sync().unwrap();
    let page = String::from_utf8(registered.kit.unwrap().expose().to_vec()).unwrap();
    let secret_key = page
        .lines()
        .find_map(|l| l.strip_prefix("Secret Key:"))
        .unwrap()
        .trim()
        .to_string();
    (app, secret_key)
}

#[test]
fn an_open_tab_renews_with_its_own_key_and_a_locked_one_still_needs_the_password() {
    let (running, clock) = server();
    let url = running.url();
    let dir = tempfile::tempdir().unwrap();
    let (desktop, secret_key) = register(dir.path(), &url);
    let mut web = WebVault::new(WebPaths {
        root: dir.path().join("tab/identity"),
        data: dir.path().join("tab/data"),
    });
    ok(
        &mut web,
        "web_sign_in",
        json!({ "server": url, "login": LOGIN, "password": PASSWORD, "secretKey": secret_key }),
    );
    let first = ok(&mut web, "worker_live_target", json!({}))["token"]
        .as_str()
        .unwrap()
        .to_string();

    // A day later by the server's clock: the token is refused, renewed
    // with the tab's device key, and the sync goes through.
    clock.fetch_add(86_400, Ordering::SeqCst);
    desktop
        .add_login("https://later.example", "me", b"pw".to_vec(), Some("Later"))
        .unwrap();
    desktop.sync().unwrap();
    let synced = ok(&mut web, "sync", json!({}));
    assert!(synced["pulled"].as_u64().unwrap() >= 1, "{synced}");
    let second = ok(&mut web, "worker_live_target", json!({}))["token"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(first, second, "the live channel opens with the new token");
    let status = ok(&mut web, "status", json!({}));
    assert!(status["account"]["sign_in"].is_null(), "{status}");

    // Locked, the tab holds no key and no token: only the password opens it.
    ok(&mut web, "lock", json!({}));
    assert_eq!(ok(&mut web, "worker_live_target", json!({})), Value::Null);
    assert!(call(&mut web, "sync", json!({})).is_err());
    ok(&mut web, "web_unlock", json!({ "password": PASSWORD }));
    ok(&mut web, "sync", json!({}));

    // Nine hours on, both renew again; the session the tab left behind
    // when it locked ends with the tab's renewal, so ending every other
    // session from the desktop ends exactly the tab's one.
    clock.fetch_add(9 * 3600, Ordering::SeqCst);
    ok(&mut web, "sync", json!({}));
    assert_eq!(desktop.sessions().unwrap().len(), 2);
    assert_eq!(desktop.end_other_sessions().unwrap(), 1);

    // Ended from the desktop: the tab's next sync can't renew and says so.
    let e = match call(&mut web, "sync", json!({})) {
        Err(e) => e,
        Ok(r) => panic!("expected a refusal, got {r:?}"),
    };
    assert_eq!(e.code, lockdown_app::ErrorCode::SignInNeeded, "{e:?}");
    assert_eq!(e.detail.as_deref(), Some("ended"));
}
