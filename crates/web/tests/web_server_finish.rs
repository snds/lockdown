//! What docs/plan/27-web-server-finish.md adds to the web vault's command
//! table, natively against the real server in process: the live channel's
//! target only while open (and the worker-only calls a page never gets),
//! the quiet sync it asks for, a session ended elsewhere forgotten, the
//! session list under the desktop's names, the server's restore drill and
//! the log mirrors.

use std::sync::Arc;

use lockdown_server::{AppState, Config, RunningServer, run_in_background};
use lockdown_web::{Reply, WebPaths, WebVault, call};
use serde_json::{Value, json};

const PASSWORD: &str = "correct horse battery staple";
const LOGIN: &str = "me@example.com";

fn server() -> RunningServer {
    // Each tab signs in several times (register, seal, unlock, a gate), so
    // the per-login brake is opened wide here; its own tests are the server's.
    let config = Config {
        per_login: (1000, 1000.0),
        per_address: (1000, 1000.0),
        ..Config::default()
    };
    let state = Arc::new(AppState::in_memory(config).unwrap());
    run_in_background(state).unwrap()
}

fn tab(dir: &std::path::Path, name: &str) -> WebVault {
    WebVault::new(WebPaths {
        root: dir.join(name).join("identity"),
        data: dir.join(name).join("data"),
    })
}

fn ok(web: &mut WebVault, cmd: &str, args: Value) -> Value {
    match call(web, cmd, args) {
        Ok(Reply::Json(v)) => v,
        Ok(Reply::Bytes(b)) => panic!("{cmd}: bytes ({} of them), not JSON", b.len()),
        Err(e) => panic!("{cmd}: {e}"),
    }
}

fn err(web: &mut WebVault, cmd: &str, args: Value) -> lockdown_app::AppError {
    match call(web, cmd, args) {
        Ok(r) => panic!("{cmd}: expected a refusal, got {r:?}"),
        Err(e) => e,
    }
}

/// Makes the account the way the desktop app does (the web vault signs in
/// to accounts, it never makes one) and returns the Secret Key from the
/// kit page. The app is kept, as the "desktop" device of the account.
fn register(dir: &std::path::Path, url: &str) -> (lockdown_app::App, String) {
    let paths = lockdown_app::Paths::resolve(Some(dir.join("desktop/identity")), None).unwrap();
    let app = lockdown_app::App::start(paths, None, lockdown_app::LockPolicy::default());
    app.open_or_create(true).unwrap();
    let registered = app.account_register(url, LOGIN, PASSWORD, None).unwrap();
    assert!(
        registered.lines.iter().any(|l| l.contains("registered")),
        "{:?}",
        registered.lines
    );
    app.sync().unwrap();
    let page = String::from_utf8(registered.kit.unwrap().expose().to_vec()).unwrap();
    let secret_key = page
        .lines()
        .find_map(|l| l.strip_prefix("Secret Key:"))
        .expect("the kit page names the Secret Key")
        .trim()
        .to_string();
    (app, secret_key)
}

fn titles(web: &mut WebVault) -> Vec<String> {
    let mut t: Vec<String> = ok(web, "list", json!({}))
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["title"].as_str().unwrap().to_string())
        .collect();
    t.sort();
    t
}

fn sign_in(web: &mut WebVault, url: &str, secret_key: &str, carried: Option<Vec<u8>>) -> Value {
    ok(
        web,
        "web_sign_in",
        json!({
            "server": url,
            "login": LOGIN,
            "password": PASSWORD,
            "secretKey": secret_key,
            "carried": carried,
        }),
    )
}

#[test]
fn the_live_channel_target_the_sessions_the_drill_and_the_mirrors_from_a_tab() {
    let running = server();
    let url = running.url();
    let dir = tempfile::tempdir().unwrap();
    let (desktop, secret_key) = register(dir.path(), &url);
    let mut web = tab(dir.path(), "tab");
    sign_in(&mut web, &url, &secret_key, None);

    // Open: the target names this server's channel and this tab's token.
    let target = ok(&mut web, "worker_live_target", json!({}));
    assert_eq!(target["url"], format!("{url}/v1/events"));
    let token = target["token"].as_str().unwrap().to_string();
    assert!(!token.is_empty());

    // The desktop adds a login; the quiet sync the channel asks for pulls it.
    desktop
        .add_login(
            "https://live.example",
            "me",
            b"pw".to_vec(),
            Some("From the desktop"),
        )
        .unwrap();
    desktop.sync().unwrap();
    let synced = ok(&mut web, "worker_live_sync", json!({}));
    assert!(synced["pulled"].as_u64().unwrap() >= 1, "{synced}");
    assert!(titles(&mut web).contains(&"From the desktop".to_string()));

    // Where the account is signed in: this tab and the desktop.
    let sessions = ok(&mut web, "session_list", json!({}));
    let list = sessions.as_array().unwrap();
    assert_eq!(list.len(), 2, "{sessions}");
    assert_eq!(list.iter().filter(|s| s["current"] == true).count(), 1);
    let bad = err(&mut web, "session_end", json!({ "id": "../x" }));
    assert!(bad.message.contains("isn't a session"));

    // The drill, as this server reports it (none scheduled here), and the
    // mirrors: none compared yet; one added, a bad one refused in words.
    let drill = ok(&mut web, "server_restore_drill", json!({}));
    assert_eq!(drill["reported"], true);
    assert!(
        drill["words"]
            .as_str()
            .unwrap()
            .contains("doesn't run restore drills")
    );
    let mirrors = ok(&mut web, "kt_mirrors", json!({}));
    assert_eq!(mirrors, json!({ "configured": [], "checks": [] }));
    let refused = err(
        &mut web,
        "kt_mirrors_set",
        json!({ "urls": ["http://m.example"] }),
    );
    assert!(refused.message.contains("https://"), "{}", refused.message);
    let set = ok(
        &mut web,
        "kt_mirrors_set",
        json!({ "urls": ["https://m.example/"] }),
    );
    assert_eq!(set["configured"], json!(["https://m.example"]));

    // The server ends this tab's session from the desktop; the channel's
    // word makes the tab forget the token, so no target is offered with it.
    ok(&mut web, "worker_live_ended", json!({ "token": token }));
    assert_eq!(ok(&mut web, "worker_live_target", json!({})), Value::Null);

    // Locked: no target at all.
    ok(&mut web, "lock", json!({}));
    assert_eq!(ok(&mut web, "worker_live_target", json!({})), Value::Null);
}
