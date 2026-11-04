//! The web vault holds back what doesn't verify, quietly, and opens a
//! person's only vault when it is shared (docs/plan/49-shared-vault-quiet.md),
//! natively against the real server in process. Alice's only vault is
//! shared with Bob, who wrote to it last. Her tab, a device that has met
//! nobody yet, signs in all the same and shows the vault with its line;
//! the sync meets Bob and it opens. A server that bends Bob's next head:
//! the tab leaves it out, says so in one line, and takes it once the server
//! serves it as signed.

use std::sync::Arc;

use lockdown_app::{App, GateInput, LockPolicy, Paths};
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

fn line_on(web: &mut WebVault, vault: &str) -> Option<String> {
    ok(web, "vaults", json!({}))
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == vault)
        .and_then(|v| v["unverified"].as_str().map(str::to_string))
}

#[test]
fn a_tab_opens_an_only_vault_that_is_shared_and_holds_back_a_bent_head_quietly() {
    let dir = tempfile::tempdir().unwrap();
    let running = server(dir.path());
    let url = running.url();
    let db = dir.path().join("server.sqlite");
    let (alice, alice_secret) = desktop(dir.path(), &url, "alice");
    let (bob, _) = desktop(dir.path(), &url, "bob");
    let vault = alice.vaults().unwrap()[0].id.clone();
    alice
        .add_login(
            "https://mail.example.com",
            "alice",
            b"pw".to_vec(),
            Some("Mail"),
        )
        .unwrap();
    alice.lookup_contact("bob@example.com").unwrap();
    alice
        .share_vault(
            &vault,
            "bob@example.com",
            "edit",
            GateInput {
                password: Some(PASSWORD.into()),
                confirmed: false,
            },
        )
        .unwrap();
    bob.sync().unwrap();
    bob.add_item_in(&vault, "note", Some("Bob one"), false, Vec::new())
        .unwrap();
    bob.sync().unwrap();

    // Alice's tab has met nobody: it signs in, shows the vault waiting with
    // its line, and opens it after the sync.
    let mut web = WebVault::new(WebPaths {
        root: dir.path().join("tab").join("identity"),
        data: dir.path().join("tab").join("data"),
    });
    ok(
        &mut web,
        "web_sign_in",
        json!({
            "server": url,
            "login": "alice@example.com",
            "password": PASSWORD,
            "secretKey": alice_secret,
            "carried": null,
        }),
    );
    let waiting = "This shared vault opens by itself once Lockdown has checked who changed it, at the next sync. You don't need to do anything.";
    assert_eq!(line_on(&mut web, &vault).as_deref(), Some(waiting));
    assert!(titles(&mut web).is_empty());
    ok(&mut web, "sync", json!({}));
    assert!(line_on(&mut web, &vault).is_none());
    let t = titles(&mut web);
    assert!(
        t.contains(&"Bob one".to_string()) && t.contains(&"Mail".to_string()),
        "{t:?}"
    );

    // Bob writes again and the server bends his head's device signature.
    bob.add_item_in(&vault, "note", Some("Bob two"), false, Vec::new())
        .unwrap();
    bob.sync().unwrap();
    let conn = rusqlite::Connection::open(&db).unwrap();
    let (seq, body): (i64, Vec<u8>) = conn
        .query_row(
            "SELECT seq, body FROM heads WHERE vault_id = ?1 ORDER BY seq DESC LIMIT 1",
            [&vault],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    let good = lockdown_vault::ManifestHead::from_bytes(&body).unwrap();
    let mut bent = good.clone();
    bent.sig[5] ^= 1;
    let put = |h: &lockdown_vault::ManifestHead| {
        conn.execute(
            "UPDATE heads SET body = ?1 WHERE vault_id = ?2 AND seq = ?3",
            rusqlite::params![h.to_bytes().unwrap(), &vault, seq],
        )
        .unwrap();
    };
    put(&bent);
    let line = "Some recent changes from bob@example.com couldn't be verified, so Lockdown left them out. You don't need to do anything.";
    let out = ok(&mut web, "sync", json!({}));
    assert!(
        out["lines"]
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l.as_str().unwrap().ends_with(line)),
        "{out}"
    );
    assert_eq!(line_on(&mut web, &vault).as_deref(), Some(line));
    assert!(!titles(&mut web).contains(&"Bob two".to_string()));
    let status = ok(&mut web, "status", json!({}));
    assert!(
        status["held_back"][0]
            .as_str()
            .unwrap()
            .contains("device signature didn't check out"),
        "{status}"
    );
    put(&good);
    ok(&mut web, "sync", json!({}));
    assert!(line_on(&mut web, &vault).is_none());
    assert!(titles(&mut web).contains(&"Bob two".to_string()));
    assert!(ok(&mut web, "status", json!({})).get("held_back").is_none());
}
