//! The web vault opens a post-quantum shared vault (docs/plan/47-shared-vault-pq.md),
//! natively against the real server in process: Alice's desktop shares a
//! vault with Bob; Bob's tab signs in, takes it with Alice's heads checked
//! under her pinned post-quantum key, writes to it with both of his own
//! signatures, and carries where Alice's post-quantum heads began to the
//! next visit, which opens the vault again under the same rule.

use std::sync::Arc;

use lockdown_app::{App, GateInput, LockPolicy, Paths};
use lockdown_server::{AppState, Config, RunningServer, run_in_background};
use lockdown_store::HeadMemory;
use lockdown_vault::Id;
use lockdown_web::{Reply, WebPaths, WebVault, call};
use serde_json::{Value, json};

const PASSWORD: &str = "correct horse battery staple";

fn server() -> (Arc<AppState>, RunningServer) {
    let config = Config {
        per_login: (1000, 1000.0),
        per_address: (1000, 1000.0),
        ..Config::default()
    };
    let state = Arc::new(AppState::in_memory(config).unwrap());
    (state.clone(), run_in_background(state).unwrap())
}

fn ok(web: &mut WebVault, cmd: &str, args: Value) -> Value {
    match call(web, cmd, args) {
        Ok(Reply::Json(v)) => v,
        Ok(Reply::Bytes(b)) => panic!("{cmd}: bytes ({} of them), not JSON", b.len()),
        Err(e) => panic!("{cmd}: {e}"),
    }
}

/// A desktop device registered as `name`, with the Secret Key from its kit.
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
        .expect("the kit page names the Secret Key")
        .trim()
        .to_string();
    (app, secret_key)
}

fn tab(dir: &std::path::Path, name: &str) -> WebVault {
    WebVault::new(WebPaths {
        root: dir.join(name).join("identity"),
        data: dir.join(name).join("data"),
    })
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

#[test]
fn a_tab_opens_a_post_quantum_shared_vault_and_carries_its_members_rule() {
    let (state, running) = server();
    let url = running.url();
    let dir = tempfile::tempdir().unwrap();
    let (alice, _) = desktop(dir.path(), &url, "alice");
    let (bob, bob_secret) = desktop(dir.path(), &url, "bob");
    alice
        .add_login(
            "https://mail.example.com",
            "alice",
            b"pw".to_vec(),
            Some("Mail"),
        )
        .unwrap();
    alice.sync().unwrap();
    let vault = alice.vaults().unwrap()[0].id.clone();
    let alice_account = bob.lookup_contact("alice@example.com").unwrap().account_id;
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
    alice
        .add_item_in(&vault, "note", Some("Alice's note"), false, Vec::new())
        .unwrap();
    alice.sync().unwrap();
    bob.sync().unwrap();

    // Bob's tab: signs in, syncs, and the shared vault opens with Alice's
    // heads checked under the post-quantum key it pinned for her.
    let mut web = tab(dir.path(), "tab");
    let sign_in = |web: &mut WebVault, carried: Option<Vec<u8>>| {
        ok(
            web,
            "web_sign_in",
            json!({
                "server": url,
                "login": "bob@example.com",
                "password": PASSWORD,
                "secretKey": bob_secret,
                "carried": carried,
            }),
        )
    };
    sign_in(&mut web, None);
    ok(&mut web, "sync", json!({}));
    assert!(titles(&mut web).contains(&"Alice's note".to_string()));

    // The tab writes into it: both signatures, from Bob's identity.
    ok(
        &mut web,
        "add_item",
        json!({ "kind": "note", "title": "From Bob's tab", "vault": vault }),
    );
    ok(&mut web, "sync", json!({}));
    let heads = state
        .storage
        .heads_since(Id::parse(&vault).unwrap(), 0)
        .unwrap();
    let newest = lockdown_vault::ManifestHead::from_bytes(&heads.last().unwrap().body).unwrap();
    assert!(newest.pq_sig.is_some(), "the tab signs post-quantum too");
    alice.sync().unwrap();
    assert!(
        alice
            .list()
            .unwrap()
            .iter()
            .any(|r| r.title == "From Bob's tab")
    );

    // Alice writes again; the tab remembers where her post-quantum heads
    // began and carries it, with everything else, to the next visit.
    alice
        .add_item_in(&vault, "note", Some("Alice again"), false, Vec::new())
        .unwrap();
    alice.sync().unwrap();
    ok(&mut web, "sync", json!({}));
    let remembered = HeadMemory::open(&dir.path().join("tab/data/remembered.cbor"))
        .unwrap()
        .member_pq_since(Id::parse(&vault).unwrap());
    assert!(
        remembered.contains_key(&Id::parse(&alice_account).unwrap()),
        "{remembered:?}"
    );
    let carried = web.carried().unwrap().expect("signed in");
    ok(&mut web, "web_sign_out", json!({}));

    let mut again = tab(dir.path(), "tab2");
    sign_in(&mut again, Some(carried));
    ok(&mut again, "sync", json!({}));
    assert!(titles(&mut again).contains(&"Alice again".to_string()));
    let remembered = HeadMemory::open(&dir.path().join("tab2/data/remembered.cbor"))
        .unwrap()
        .member_pq_since(Id::parse(&vault).unwrap());
    assert!(remembered.contains_key(&Id::parse(&alice_account).unwrap()));
}
