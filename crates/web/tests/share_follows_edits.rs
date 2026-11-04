//! The web vault follows other people's edits of an item shared on its own
//! (docs/plan/54-share-follows-edits.md), natively against the real server
//! in process. Alice shares "Trip" with Bob to edit, and one login of it
//! with Dave on its own. Alice's device that syncs is a tab, and so is
//! Dave's. Bob changes the login: once Alice's tab syncs, it signs the
//! edit again, and Dave's tab opens the new password at its next sync.

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

fn bytes(web: &mut WebVault, cmd: &str, args: Value) -> Vec<u8> {
    match call(web, cmd, args) {
        Ok(Reply::Bytes(b)) => b.to_vec(),
        Ok(Reply::Json(v)) => panic!("{cmd}: JSON {v}, not bytes"),
        Err(e) => panic!("{cmd}: {e}"),
    }
}

fn rows_of(web: &mut WebVault, id: &str) -> usize {
    let list = ok(web, "list", json!({}));
    list.as_array()
        .unwrap()
        .iter()
        .filter(|r| r["id"] == id)
        .count()
}

fn tab(dir: &std::path::Path, name: &str, url: &str, secret: &str) -> WebVault {
    let mut web = WebVault::new(WebPaths {
        root: dir.join(format!("{name}-tab")).join("identity"),
        data: dir.join(format!("{name}-tab")).join("data"),
    });
    ok(
        &mut web,
        "web_sign_in",
        json!({
            "server": url,
            "login": format!("{name}@example.com"),
            "password": PASSWORD,
            "secretKey": secret,
            "carried": null,
        }),
    );
    ok(&mut web, "sync", json!({}));
    web
}

fn reveal(web: &mut WebVault, id: &str) -> Vec<u8> {
    bytes(web, "reveal", json!({ "id": id, "fieldId": "pw" }))
}

/// The lines a tab's sync said something was held back.
fn quiet_lines(web: &mut WebVault) -> Vec<String> {
    let out = ok(web, "sync", json!({}));
    out["lines"]
        .as_array()
        .map(|l| {
            l.iter()
                .filter_map(|l| l.as_str())
                .filter(|l| l.contains("couldn't be verified"))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn a_tab_follows_a_members_edit_and_a_tab_holding_the_item_sees_it() {
    let dir = tempfile::tempdir().unwrap();
    let running = server(dir.path());
    let url = running.url();
    let (alice, alice_secret) = desktop(dir.path(), &url, "alice");
    let (bob, _) = desktop(dir.path(), &url, "bob");
    let (_dave, dave_secret) = desktop(dir.path(), &url, "dave");
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
    alice.sync().unwrap();
    for who in ["bob", "dave"] {
        alice.lookup_contact(&format!("{who}@example.com")).unwrap();
    }
    alice
        .share_vault(&trip, "bob@example.com", "edit", pw())
        .unwrap();
    alice
        .share_item(&rail, "dave@example.com", "view", pw())
        .unwrap();
    bob.sync().unwrap();
    let mut alice_tab = tab(dir.path(), "alice", &url, &alice_secret);
    let mut dave_tab = tab(dir.path(), "dave", &url, &dave_secret);
    assert_eq!(reveal(&mut alice_tab, &rail), b"pw-rail");
    assert_eq!(reveal(&mut dave_tab, &rail), b"pw-rail");

    let versions = alice.history(&rail).unwrap().len();

    // Bob changes the password. Before Alice's tab has seen it, Dave's tab
    // keeps the copy it has, quietly.
    bob.edit_item(
        &rail,
        None,
        vec![
            FieldChange::keep("user"),
            FieldChange::secret("pw", b"pw-bob"),
        ],
        GateInput::default(),
    )
    .unwrap();
    bob.sync().unwrap();
    assert!(quiet_lines(&mut dave_tab).is_empty());
    assert_eq!(rows_of(&mut dave_tab, &rail), 1);
    assert_eq!(reveal(&mut dave_tab, &rail), b"pw-rail");

    // Alice's tab syncs: it takes Bob's edit and signs it again.
    assert!(quiet_lines(&mut alice_tab).is_empty());
    assert_eq!(reveal(&mut alice_tab, &rail), b"pw-bob");
    // Dave's tab follows, with nothing said.
    assert!(quiet_lines(&mut dave_tab).is_empty());
    assert_eq!(rows_of(&mut dave_tab, &rail), 1);
    assert_eq!(reveal(&mut dave_tab, &rail), b"pw-bob");

    // Alice's desktop takes the tab's version as its own and writes nothing
    // more; a later sync on every side keeps it so.
    alice.sync().unwrap();
    assert_eq!(
        alice.history(&rail).unwrap().len(),
        versions + 2,
        "Bob's edit and the tab's follow"
    );
    alice.sync().unwrap();
    assert_eq!(alice.history(&rail).unwrap().len(), versions + 2);
    ok(&mut alice_tab, "sync", json!({}));
    ok(&mut dave_tab, "sync", json!({}));
    assert_eq!(reveal(&mut dave_tab, &rail), b"pw-bob");
    assert_eq!(
        alice
            .reveal(&rail, "pw", GateInput::default())
            .unwrap()
            .expose(),
        b"pw-bob"
    );
}
