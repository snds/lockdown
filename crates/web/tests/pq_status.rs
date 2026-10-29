//! The web vault's `pq_status` call (docs/plan/30-pq-status-line.md),
//! natively against the real server in process: the desktop's command name
//! and shape, "on" once both devices read post-quantum heads, "waiting for"
//! a device from an older build after the tab's sync learns of it, "on"
//! again once that device reports level 2, and a refusal while locked.

use std::path::Path;
use std::sync::Arc;

use ecosystem_identity::FileKeystore;
use lockdown_server::{AppState, Config, RunningServer, run_in_background};
use lockdown_web::{Reply, WebPaths, WebVault, call};
use serde_json::{Value, json};

const PASSWORD: &str = "correct horse battery staple";
const LOGIN: &str = "me@example.com";

fn server() -> RunningServer {
    let config = Config {
        per_login: (1000, 1000.0),
        per_address: (1000, 1000.0),
        ..Config::default()
    };
    let state = Arc::new(AppState::in_memory(config).unwrap());
    run_in_background(state).unwrap()
}

fn ok(web: &mut WebVault, cmd: &str, args: Value) -> Value {
    match call(web, cmd, args) {
        Ok(Reply::Json(v)) => v,
        Ok(Reply::Bytes(b)) => panic!("{cmd}: bytes ({} of them), not JSON", b.len()),
        Err(e) => panic!("{cmd}: {e}"),
    }
}

/// The desktop device of the account, with the Secret Key from its kit.
fn register(dir: &Path, url: &str) -> (lockdown_app::App, String) {
    let paths = lockdown_app::Paths::resolve(Some(dir.join("desktop/identity")), None).unwrap();
    let app = lockdown_app::App::start(paths, None, lockdown_app::LockPolicy::default());
    app.open_or_create(true).unwrap();
    let registered = app.account_register(url, LOGIN, PASSWORD, None).unwrap();
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

/// Changes the desktop's identity on disk while it is locked, then syncs it.
fn on_desktop<T>(
    desktop: &lockdown_app::App,
    edit: impl FnOnce(&mut ecosystem_identity::Identity, &FileKeystore) -> T,
) -> T {
    desktop.lock().unwrap();
    let probe = desktop.probe().unwrap();
    let keystore =
        FileKeystore::open_or_create(&Path::new(&probe.data).join("device-key.cbor")).unwrap();
    let mut identity =
        lockdown_vault::identity::open_identity(Path::new(&probe.root), &keystore, None)
            .unwrap()
            .unwrap();
    let out = edit(&mut identity, &keystore);
    desktop.unlock().unwrap();
    desktop.sync().unwrap();
    out
}

#[test]
fn a_tab_shows_the_rollout_under_the_desktops_command_name() {
    let running = server();
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
    ok(&mut web, "sync", json!({}));
    desktop.sync().unwrap();
    let s = ok(&mut web, "pq_status", json!({}));
    assert_eq!(s["state"], "on", "{s}");
    assert_eq!(s["reason"], Value::Null);
    assert_eq!(s["headline"], "Quantum-safe signatures: on");
    let mut keys: Vec<&String> = s.as_object().unwrap().keys().collect();
    keys.sort();
    assert_eq!(
        keys,
        [
            "explanation",
            "headline",
            "reason",
            "state",
            "without_level_1",
            "without_level_2"
        ]
    );

    // A device from an older build joins the account's records; the tab
    // sees it at its next sync, by name and id, and nothing else about it.
    let old = on_desktop(&desktop, |identity, keystore| {
        lockdown_vault::test_support::add_device_at_level(
            identity,
            keystore,
            "Lockdown on the old laptop",
            Some(1),
        )
        .unwrap()
    });
    ok(&mut web, "sync", json!({}));
    let s = ok(&mut web, "pq_status", json!({}));
    assert_eq!(s["state"], "waiting", "{s}");
    assert_eq!(
        s["headline"],
        "Quantum-safe signatures: waiting for Lockdown on the old laptop"
    );
    assert_eq!(s["without_level_1"], json!([]));
    assert_eq!(
        s["without_level_2"],
        json!([{ "id": old.to_string(), "name": "Lockdown on the old laptop" }])
    );
    let devices = ok(&mut web, "device_list", json!({}));
    assert!(
        devices
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["id"] == old.to_string() && d["name"] == "Lockdown on the old laptop"),
        "the Devices screen names it the same way: {devices}"
    );

    // It updates: on, once the tab syncs.
    on_desktop(&desktop, |identity, keystore| {
        lockdown_vault::test_support::set_device_level(identity, keystore, old, Some(2)).unwrap()
    });
    ok(&mut web, "sync", json!({}));
    assert_eq!(ok(&mut web, "pq_status", json!({}))["state"], "on");

    // Locked, the call refuses like every vault call.
    ok(&mut web, "lock", json!({}));
    let e = call(&mut web, "pq_status", json!({})).unwrap_err();
    assert_eq!(e.code, lockdown_app::ErrorCode::Locked, "{}", e.message);
}
