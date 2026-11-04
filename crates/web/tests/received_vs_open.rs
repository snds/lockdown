//! The web vault keeps an item shared on its own apart from the vault it
//! came from (docs/plan/50-received-vs-open.md), natively against the real
//! server in process. Alice shares one item of a vault with Bob on its own;
//! Bob's tab lists it as received. Then she adds Bob to the vault: the tab's
//! next sync joins it, and the item is listed once, from the vault.

use std::sync::Arc;

use lockdown_app::{App, FieldChange, GateInput, LockPolicy, Paths};
use lockdown_server::{AppState, Config, RunningServer, run_in_background};
use lockdown_web::{Reply, WebPaths, WebVault, call};
use serde_json::{Value, json};

const PASSWORD: &str = "correct horse battery staple";

fn server(dir: &std::path::Path) -> RunningServer {
    let storage =
        lockdown_server::storage::SqliteStorage::open(&dir.join("server.sqlite")).unwrap();
    let keys =
        lockdown_server::keys::ServerKeys::load_or_create(&dir.join("server-key.cbor")).unwrap();
    let config = Config {
        per_login: (1000, 1000.0),
        per_address: (1000, 1000.0),
        ..Config::default()
    };
    run_in_background(Arc::new(AppState::new(Box::new(storage), keys, config))).unwrap()
}

fn ok(web: &mut WebVault, cmd: &str, args: Value) -> Value {
    match call(web, cmd, args) {
        Ok(Reply::Json(v)) => v,
        Ok(Reply::Bytes(b)) => panic!("{cmd}: bytes ({} of them), not JSON", b.len()),
        Err(e) => panic!("{cmd}: {e}"),
    }
}

fn desktop(dir: &std::path::Path, url: &str, name: &str) -> (App, String) {
    let paths = Paths::resolve(Some(dir.join(name).join("identity")), None).unwrap();
    let app = App::start(paths, None, LockPolicy::default());
    app.open_or_create_plain().unwrap();
    let registered = app
        .account_register(url, &format!("{name}@example.com"), PASSWORD, None)
        .unwrap();
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

fn pw() -> GateInput {
    GateInput {
        password: Some(PASSWORD.into()),
        confirmed: false,
    }
}

/// How many rows the tab lists for `id`, and the access the first says.
fn rows_of(web: &mut WebVault, id: &str) -> (usize, Option<String>) {
    let list = ok(web, "list", json!({}));
    let rows: Vec<&Value> = list
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["id"] == id)
        .collect();
    (
        rows.len(),
        rows.first()
            .and_then(|r| r["access"].as_str().map(str::to_string)),
    )
}

/// How many sidebar entries the tab shows for `vault`, and its badge's kind.
fn vault_entries(web: &mut WebVault, vault: &str) -> (usize, Option<String>) {
    let views = ok(web, "vaults", json!({}));
    let entries: Vec<&Value> = views
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["id"] == vault)
        .collect();
    (
        entries.len(),
        entries
            .first()
            .and_then(|v| v["sharing"]["kind"].as_str().map(str::to_string)),
    )
}

#[test]
fn a_tab_joins_a_vault_an_item_was_shared_from_and_lists_the_item_once() {
    let dir = tempfile::tempdir().unwrap();
    let running = server(dir.path());
    let url = running.url();
    let (alice, _) = desktop(dir.path(), &url, "alice");
    let (_bob, bob_secret) = desktop(dir.path(), &url, "bob");
    let trip = alice.create_vault("Trip").unwrap();
    let rail = alice
        .add_item_in(
            &trip,
            "login",
            Some("Rail"),
            false,
            vec![
                FieldChange::text("user", "alice"),
                FieldChange::secret("pw", b"pw-rail"),
            ],
        )
        .unwrap();
    let hotel = alice
        .add_item_in(
            &trip,
            "note",
            Some("Hotel"),
            false,
            vec![FieldChange::text("body", "Room 12")],
        )
        .unwrap();
    alice.sync().unwrap();
    alice.lookup_contact("bob@example.com").unwrap();
    alice
        .share_item(&rail, "bob@example.com", "fill-only", pw())
        .unwrap();

    // Bob's tab: the item alone, as received.
    let mut web = WebVault::new(WebPaths {
        root: dir.path().join("tab").join("identity"),
        data: dir.path().join("tab").join("data"),
    });
    ok(
        &mut web,
        "web_sign_in",
        json!({
            "server": url,
            "login": "bob@example.com",
            "password": PASSWORD,
            "secretKey": bob_secret,
            "carried": null,
        }),
    );
    ok(&mut web, "sync", json!({}));
    assert_eq!(rows_of(&mut web, &rail), (1, Some("fill-only".into())));
    assert_eq!(rows_of(&mut web, &hotel).0, 0);
    assert_eq!(vault_entries(&mut web, &trip), (1, Some("received".into())));

    // Added to the vault: the tab's next sync joins it, and the vault's own
    // copy and right answer for the item.
    alice
        .share_vault(&trip, "bob@example.com", "view", pw())
        .unwrap();
    ok(&mut web, "sync", json!({}));
    assert_eq!(rows_of(&mut web, &rail), (1, Some("view".into())));
    assert_eq!(rows_of(&mut web, &hotel), (1, Some("view".into())));
    assert_eq!(vault_entries(&mut web, &trip), (1, Some("shared".into())));

    // A later sync keeps it that way.
    ok(&mut web, "sync", json!({}));
    assert_eq!(rows_of(&mut web, &rail).0, 1);
    assert_eq!(vault_entries(&mut web, &trip).0, 1);
}
