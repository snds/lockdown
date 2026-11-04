//! The web vault keeps opening what stays shared with it after a member is
//! removed and after another person's share is withdrawn
//! (docs/plan/51-rekey-push.md), natively against the real server in
//! process. Alice shares "Trip" with Bob and Carol, and one login of it,
//! and one of her own, with Dave on their own; Dave is a tab. She removes
//! Carol, and withdraws Carol's share of the other login: Dave's tab still
//! lists and opens both, and Bob still opens the vault.

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

#[test]
fn a_tab_keeps_opening_its_shares_after_a_removal_and_a_withdrawal() {
    let dir = tempfile::tempdir().unwrap();
    let running = server(dir.path());
    let url = running.url();
    let (alice, _) = desktop(dir.path(), &url, "alice");
    let (bob, _) = desktop(dir.path(), &url, "bob");
    let (carol, _) = desktop(dir.path(), &url, "carol");
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
    let mail = alice
        .add_login(
            "https://mail.example.com",
            "alice",
            b"pw-mail".to_vec(),
            Some("Mail"),
        )
        .unwrap();
    alice.sync().unwrap();
    for who in ["bob", "carol", "dave"] {
        alice.lookup_contact(&format!("{who}@example.com")).unwrap();
    }
    let carol_account = alice
        .lookup_contact("carol@example.com")
        .unwrap()
        .account_id;
    alice
        .share_vault(&trip, "bob@example.com", "view", pw())
        .unwrap();
    alice
        .share_vault(&trip, "carol@example.com", "view", pw())
        .unwrap();
    alice
        .share_item(&rail, "dave@example.com", "view", pw())
        .unwrap();
    alice
        .share_item(&mail, "dave@example.com", "view", pw())
        .unwrap();
    let to_carol = alice
        .share_item(&mail, "carol@example.com", "view", pw())
        .unwrap()
        .share_id;
    bob.sync().unwrap();
    carol.sync().unwrap();

    let mut web = WebVault::new(WebPaths {
        root: dir.path().join("tab").join("identity"),
        data: dir.path().join("tab").join("data"),
    });
    ok(
        &mut web,
        "web_sign_in",
        json!({
            "server": url,
            "login": "dave@example.com",
            "password": PASSWORD,
            "secretKey": dave_secret,
            "carried": null,
        }),
    );
    ok(&mut web, "sync", json!({}));
    let reveal =
        |web: &mut WebVault, id: &str| bytes(web, "reveal", json!({ "id": id, "fieldId": "pw" }));
    assert_eq!(reveal(&mut web, &rail), b"pw-rail");
    assert_eq!(reveal(&mut web, &mail), b"pw-mail");

    // Carol leaves the vault and loses her share of Mail.
    alice.remove_member(&trip, &carol_account).unwrap();
    alice.unshare_item(&to_carol).unwrap();

    ok(&mut web, "sync", json!({}));
    assert_eq!(rows_of(&mut web, &rail), 1);
    assert_eq!(rows_of(&mut web, &mail), 1);
    assert_eq!(reveal(&mut web, &rail), b"pw-rail");
    assert_eq!(reveal(&mut web, &mail), b"pw-mail");
    bob.sync().unwrap();
    assert_eq!(
        bob.reveal(&rail, "pw", GateInput::default())
            .unwrap()
            .expose(),
        b"pw-rail"
    );
    carol.sync().unwrap();
    assert!(
        carol
            .list()
            .unwrap()
            .iter()
            .all(|r| r.id != rail && r.id != mail)
    );

    // A later sync keeps it so.
    ok(&mut web, "sync", json!({}));
    assert_eq!(reveal(&mut web, &rail), b"pw-rail");
}
