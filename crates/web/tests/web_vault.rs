//! The web vault's session host and command table, natively, against the
//! real server in process: what the worker runs in the browser, on the
//! desktop's backends (SQLite, the built-in HTTPS client) instead of the
//! tab's memory and the browser's fetch. The browser half is the
//! Playwright suite in apps/web/e2e.

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

/// A tab served by `url`, which links must name.
fn page(dir: &std::path::Path, name: &str, url: &str) -> WebVault {
    tab(dir, name).serving(url)
}

fn ok(web: &mut WebVault, cmd: &str, args: Value) -> Value {
    match call(web, cmd, args) {
        Ok(Reply::Json(v)) => v,
        Ok(Reply::Bytes(b)) => panic!("{cmd}: bytes ({} of them), not JSON", b.len()),
        Err(e) => panic!("{cmd}: {e}"),
    }
}

fn bytes(web: &mut WebVault, cmd: &str, args: Value) -> Vec<u8> {
    match call(web, cmd, args) {
        Ok(Reply::Bytes(b)) => b.to_vec(),
        Ok(Reply::Json(v)) => panic!("{cmd}: JSON {v}, not bytes"),
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
    assert!(registered.lines.iter().any(|l| l.contains("registered")), "{:?}", registered.lines);
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

fn add_login(web: &mut WebVault, title: &str, site: &str, password: &str) -> String {
    ok(
        web,
        "add_item",
        json!({
            "kind": "login",
            "title": title,
            "fields": [
                { "field_id": "user", "action": "set", "text": "me" },
                { "field_id": "pw", "action": "set", "secret": password.as_bytes() },
                { "field_id": "url.0", "action": "set", "text": site },
            ],
        }),
    )
    .as_str()
    .unwrap()
    .to_string()
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
fn two_tabs_sign_in_with_the_secret_key_and_share_one_vault_with_the_desktop() {
    let running = server();
    let url = running.url();
    let dir = tempfile::tempdir().unwrap();

    let (desktop, secret_key) = register(dir.path(), &url);
    assert!(secret_key.starts_with("A1-"), "{secret_key}");
    let mut first = tab(dir.path(), "first");
    sign_in(&mut first, &url, &secret_key, None);
    let state = ok(&mut first, "web_state", json!({}));
    assert_eq!(state["signed_in"], true);
    assert_eq!(state["locked"], false);
    let netflix = add_login(&mut first, "Netflix", "https://netflix.com", "hunter2hunter2");
    ok(&mut first, "sync", json!({}));

    // A second tab is a new device: the password alone is not enough.
    let mut second = tab(dir.path(), "second");
    let refused = err(
        &mut second,
        "web_sign_in",
        json!({ "server": url, "login": LOGIN, "password": PASSWORD }),
    );
    assert!(refused.message.contains("Secret Key"), "{}", refused.message);
    assert_eq!(ok(&mut second, "web_state", json!({}))["signed_in"], false);
    let wrong = err(
        &mut second,
        "web_sign_in",
        json!({ "server": url, "login": LOGIN, "password": "not the password", "secretKey": secret_key }),
    );
    assert!(wrong.message.contains("didn't match"), "{}", wrong.message);
    let malformed = err(
        &mut second,
        "web_sign_in",
        json!({ "server": url, "login": LOGIN, "password": PASSWORD, "secretKey": "A1-NOT-THE-KEY" }),
    );
    assert_eq!(malformed.code, lockdown_app::ErrorCode::Refused);
    // The worker ticks every second between attempts; that keeps the
    // pending key too.
    assert_eq!(ok(&mut second, "tick", json!({})), json!(true));

    // The attempts that reached the server were this tab's one pending
    // device, which the next sign-in enrols: no stranger in the log.
    sign_in(&mut second, &url, &secret_key, None);
    assert_eq!(titles(&mut second), vec!["Netflix"]);
    let pw = bytes(&mut second, "reveal", json!({ "id": netflix, "fieldId": "pw" }));
    assert_eq!(pw, b"hunter2hunter2");

    // An edit in the second tab reaches the first through the server.
    let version = ok(
        &mut second,
        "edit_item",
        json!({
            "id": netflix,
            "title": "Netflix (family)",
            "changes": [
                { "field_id": "user", "action": "keep" },
                { "field_id": "pw", "action": "set", "secret": b"a new one entirely".as_slice() },
                { "field_id": "url.0", "action": "keep" },
            ],
        }),
    );
    assert_eq!(version, 2);
    ok(&mut second, "sync", json!({}));
    ok(&mut first, "sync", json!({}));
    assert_eq!(titles(&mut first), vec!["Netflix (family)"]);
    assert_eq!(
        bytes(&mut first, "reveal_to_use", json!({ "id": netflix, "fieldId": "pw" })),
        b"a new one entirely"
    );

    // The desktop sees the tabs' work too, and both tabs are devices of
    // the one identity beside it.
    desktop.sync().unwrap();
    let titles_there: Vec<String> = desktop.list().unwrap().into_iter().map(|r| r.title).collect();
    assert_eq!(titles_there, vec!["Netflix (family)"]);
    let devices = ok(&mut first, "device_list", json!({}));
    assert_eq!(devices.as_array().unwrap().len(), 3, "{devices}");
}

#[test]
fn lock_wipes_the_device_key_and_unlock_is_a_fresh_sign_in() {
    let running = server();
    let url = running.url();
    let dir = tempfile::tempdir().unwrap();
    let (_desktop, secret_key) = register(dir.path(), &url);
    let mut web = tab(dir.path(), "tab");
    sign_in(&mut web, &url, &secret_key, None);
    add_login(&mut web, "Bank", "https://bank.example", "s3cret-s3cret");
    let key_file = web.paths().data.join("device-key.cbor");
    assert!(key_file.exists());

    ok(&mut web, "lock", json!({}));
    assert!(!key_file.exists(), "the key file is gone while locked");
    let state = ok(&mut web, "web_state", json!({}));
    assert_eq!((state["signed_in"].clone(), state["locked"].clone()), (json!(true), json!(true)));
    let locked = err(&mut web, "list", json!({}));
    assert_eq!(locked.code, lockdown_app::ErrorCode::Locked);

    let wrong = err(&mut web, "web_unlock", json!({ "password": "not it at all" }));
    assert!(wrong.message.contains("didn't match"), "{}", wrong.message);
    assert!(!key_file.exists(), "a failed unlock leaves no key behind");

    let status = ok(&mut web, "web_unlock", json!({ "password": PASSWORD }));
    assert!(status.is_object(), "{status}");
    assert!(key_file.exists());
    assert_eq!(titles(&mut web), vec!["Bank"]);
    // The fresh session works for a sync.
    ok(&mut web, "sync", json!({}));

    // A timer that locks (idle set to one second here) wipes the key too.
    ok(
        &mut web,
        "set_policy",
        json!({ "policy": { "idle_secs": 1, "hidden_secs": 300, "sleep_gap_secs": 30 } }),
    );
    std::thread::sleep(std::time::Duration::from_millis(2500));
    assert_eq!(ok(&mut web, "tick", json!({})), json!(true));
    assert!(!key_file.exists());

    ok(&mut web, "web_sign_out", json!({}));
    assert_eq!(ok(&mut web, "web_state", json!({}))["signed_in"], false);
    assert!(!web.paths().root.exists() && !web.paths().data.exists());
    let after = err(&mut web, "web_unlock", json!({ "password": PASSWORD }));
    assert!(after.message.contains("sign in first"), "{}", after.message);
}

#[test]
fn a_crown_jewel_is_gated_by_the_password_on_the_web_too() {
    let running = server();
    let url = running.url();
    let dir = tempfile::tempdir().unwrap();
    let (_desktop, secret_key) = register(dir.path(), &url);
    let mut web = tab(dir.path(), "tab");
    sign_in(&mut web, &url, &secret_key, None);
    let id = ok(
        &mut web,
        "add_item",
        json!({
            "kind": "crypto-wallet",
            "title": "Cold wallet",
            "crownJewel": true,
            "fields": [
                { "field_id": "seed", "action": "set", "secret": b"abandon ability able".as_slice() },
            ],
        }),
    );
    let id = id.as_str().unwrap().to_string();
    let field = "seed";

    let route = ok(&mut web, "approval_route", json!({ "kind": "reveal" }));
    assert_eq!(route["route"], "password");
    let needed = err(&mut web, "reveal", json!({ "id": id, "fieldId": field }));
    assert_eq!(needed.code, lockdown_app::ErrorCode::ApprovalNeeded);
    assert_eq!(needed.detail.as_deref(), Some("password"));
    let bad = err(
        &mut web,
        "reveal",
        json!({ "id": id, "fieldId": field, "gate": { "password": "nope nope nope" } }),
    );
    assert_eq!(bad.code, lockdown_app::ErrorCode::Refused);
    let seed = bytes(
        &mut web,
        "reveal",
        json!({ "id": id, "fieldId": field, "gate": { "password": PASSWORD } }),
    );
    assert_eq!(seed, b"abandon ability able");
    // Search shows a crown jewel by title and kind only.
    let rows = ok(&mut web, "list", json!({}));
    let row = rows
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == id.as_str())
        .unwrap();
    assert!(!row.to_string().contains("abandon"), "{row}");
}

#[test]
fn what_one_visit_carries_holds_the_next_to_the_same_log_and_heads() {
    let running = server();
    let url = running.url();
    let dir = tempfile::tempdir().unwrap();
    let (_desktop, secret_key) = register(dir.path(), &url);
    let mut first = tab(dir.path(), "first");
    sign_in(&mut first, &url, &secret_key, None);
    add_login(&mut first, "Mail", "https://mail.example", "mail-password-1");
    ok(&mut first, "sync", json!({}));

    let mut visit = tab(dir.path(), "visit");
    sign_in(&mut visit, &url, &secret_key, None);
    let carried = bytes(&mut visit, "web_carried", json!({}));
    let text = String::from_utf8_lossy(&carried);
    assert!(!text.contains("Mail") && !text.contains("mail-password"), "{text}");
    ok(&mut visit, "web_sign_out", json!({}));

    // The next visit in the same browser starts from what was carried and
    // still opens (the log and the chain only moved forward).
    sign_in(&mut visit, &url, &secret_key, Some(carried.clone()));
    assert_eq!(titles(&mut visit), vec!["Mail"]);
    ok(&mut visit, "web_sign_out", json!({}));

    // A carried head the server's chain never reached (as if the server
    // now served an older copy of the vault than this browser saw) stops
    // the sign-in, and nothing of the vault opens.
    let mut value: ciborium::Value = ciborium::from_reader(carried.as_slice()).unwrap();
    let vaults = value
        .as_map_mut()
        .unwrap()
        .iter_mut()
        .find(|(k, _)| k.as_text() == Some("vaults"))
        .map(|(_, v)| v)
        .unwrap();
    let (_, head) = vaults.as_map_mut().unwrap().first_mut().unwrap();
    for (k, v) in head.as_map_mut().unwrap().iter_mut() {
        if k.as_text() == Some("seq") {
            let seq: u64 = v.as_integer().unwrap().try_into().unwrap();
            *v = ciborium::Value::Integer((seq + 5).into());
        }
    }
    let mut forged = Vec::new();
    ciborium::into_writer(&value, &mut forged).unwrap();
    let refused = err(
        &mut visit,
        "web_sign_in",
        json!({
            "server": url,
            "login": LOGIN,
            "password": PASSWORD,
            "secretKey": secret_key,
            "carried": forged,
        }),
    );
    assert!(
        refused.message.to_lowercase().contains("rollback"),
        "{}",
        refused.message
    );
    assert_eq!(ok(&mut visit, "web_state", json!({}))["signed_in"], false);
}

#[test]
fn an_unknown_command_and_a_missing_argument_are_refused_in_words() {
    let dir = tempfile::tempdir().unwrap();
    let mut web = tab(dir.path(), "tab");
    let e = err(&mut web, "export_save", json!({}));
    assert!(e.message.contains("no command 'export_save'"), "{}", e.message);
    let e = err(&mut web, "web_sign_in", json!({ "server": "http://127.0.0.1:9" }));
    assert!(e.message.contains("'login' is missing"), "{}", e.message);
}

#[test]
fn a_share_link_opens_and_a_request_is_answered_with_no_account() {
    let running = server();
    let url = running.url();
    let dir = tempfile::tempdir().unwrap();
    let (desktop, secret_key) = register(dir.path(), &url);
    desktop.sync().unwrap();

    // A text link with a password, made on the desktop.
    let link = desktop
        .send_text(
            "Wi-Fi at the cabin",
            b"pine-needles-42".to_vec(),
            Some(b"a link password".to_vec()),
            lockdown_app::sharing::SendOptions { days: 3, max_views: Some(2) },
        )
        .unwrap();
    let link = String::from_utf8(link.expose().to_vec()).unwrap();
    assert!(link.starts_with(&format!("{url}/s/")) && link.contains('#'), "{link}");

    // Someone with no account opens it in the web vault's page.
    let mut stranger = page(dir.path(), "stranger", &url);
    let meta = ok(&mut stranger, "link_meta", json!({ "link": link }));
    assert_eq!(meta["password"], true);
    assert_eq!(meta["views_left"], 2);
    let refused = err(&mut stranger, "link_open", json!({ "link": link }));
    assert!(refused.message.contains("password"), "{}", refused.message);
    let view = ok(
        &mut stranger,
        "link_open",
        json!({ "link": link, "password": b"a link password".as_slice() }),
    );
    assert_eq!(view["kind"], "text");
    assert_eq!(view["title"], "Wi-Fi at the cabin");
    assert!(!view.to_string().contains("pine-needles"), "{view}");
    assert_eq!(bytes(&mut stranger, "link_value", json!({ "index": 0 })), b"pine-needles-42");
    ok(&mut stranger, "link_close", json!({}));
    assert!(err(&mut stranger, "link_value", json!({ "index": 0 })).message.contains("no link"));
    // The opens are counted: one left after this one.
    assert_eq!(ok(&mut stranger, "link_meta", json!({ "link": link }))["views_left"], 1);

    // A link for another server is refused before anything is fetched.
    let elsewhere = link.replace(&url, "https://elsewhere.example");
    let e = err(&mut stranger, "link_open", json!({ "link": elsewhere }));
    assert!(e.message.contains("belongs to https://elsewhere.example"), "{}", e.message);

    // A request link a signed-in tab made, answered by the stranger and
    // read back in the tab.
    let mut asker = page(dir.path(), "asker", &url);
    sign_in(&mut asker, &url, &secret_key, None);
    let request = bytes(&mut asker, "request_create", json!({ "label": "The door code", "days": 7 }));
    let request = String::from_utf8(request).unwrap();
    assert_eq!(ok(&mut stranger, "request_label", json!({ "link": request })), json!("The door code"));
    ok(
        &mut stranger,
        "request_answer",
        json!({ "link": request, "title": "Front door", "secret": b"2468".as_slice() }),
    );
    let again = err(
        &mut stranger,
        "request_answer",
        json!({ "link": request, "title": "Front door", "secret": b"1357".as_slice() }),
    );
    assert!(again.message.contains("already answered"), "{}", again.message);
    let asked = ok(&mut asker, "requests", json!({}));
    let request_id = asked[0]["request_id"].as_str().unwrap().to_string();
    assert!(asked[0]["answered_at"].is_i64(), "{asked}");
    let answered = ok(&mut asker, "request_open", json!({ "requestId": request_id }));
    assert_eq!(answered["title"], "Front door");
    assert_eq!(bytes(&mut asker, "opened_value", json!({ "index": 0 })), b"2468");

    // A text link made in the tab opens for the stranger too.
    let made = bytes(&mut asker, "send_text", json!({ "title": "Gate", "text": b"0042".as_slice(), "days": 1, "maxViews": 1 }));
    let made = String::from_utf8(made).unwrap();
    assert_eq!(ok(&mut asker, "sends", json!({})).as_array().unwrap().len(), 2);
    ok(&mut stranger, "link_open", json!({ "link": made }));
    assert_eq!(bytes(&mut stranger, "link_value", json!({ "index": 0 })), b"0042");
    let gone = err(&mut stranger, "link_open", json!({ "link": made }));
    assert_eq!(gone.code, lockdown_app::ErrorCode::Refused);
    drop(desktop);
}
