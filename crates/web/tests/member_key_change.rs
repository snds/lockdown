//! The web vault asks its person, as an owner, about someone who reset
//! their account, and lets them back in (docs/plan/55-member-key-change.md),
//! natively against the real server in process. The server publishes new
//! keys for Bob under his account, as an operator's reset would; Alice's tab
//! syncs, finds the question in the overview, answers yes through the gate,
//! and the vault key is wrapped to Bob's new keys and nobody else's.

use std::sync::Arc;

use ecosystem_identity::{FileKeystore, Identity};
use lockdown_app::{App, GateInput, LockPolicy, Paths};
use lockdown_server::storage::{IdentityAccept, RecordRow, SqliteStorage, Storage};
use lockdown_server::{AppState, Config, RunningServer, run_in_background};
use lockdown_vault::Id;
use lockdown_vault::identity_set::{IdentityManifest, RecordSet};
use lockdown_vault::keys::VaultKey;
use lockdown_vault::sharing::ShareEnvelope;
use lockdown_web::{Reply, WebPaths, WebVault, call};
use serde_json::{Value, json};

const PASSWORD: &str = "correct horse battery staple";

fn server(dir: &std::path::Path) -> RunningServer {
    let storage = SqliteStorage::open(&dir.join("server.sqlite")).unwrap();
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

/// Publishes a fresh identity as Bob's, continuing his chain and logged
/// like any manifest (the server's own hand). Answers it and its keystore.
fn reset_bob(dir: &std::path::Path, db: &std::path::Path) -> (Identity, FileKeystore, Id) {
    let keystore = FileKeystore::open_or_create(&dir.join("bob2-key.cbor")).unwrap();
    let mut id = Identity::create(&dir.join("bob2"), "lockdown", "bob2", &keystore).unwrap();
    lockdown_vault::pq::roll_out(&mut id, &keystore).unwrap();
    let conn = rusqlite::Connection::open(db).unwrap();
    let account: String = conn
        .query_row(
            "SELECT account_id FROM accounts WHERE login = 'bob@example.com'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let prev: Vec<u8> = conn
        .query_row(
            "SELECT body FROM identity_heads WHERE account_id = ?1 ORDER BY seq DESC LIMIT 1",
            [&account],
            |r| r.get(0),
        )
        .unwrap();
    let prev = IdentityManifest::from_bytes(&prev).unwrap();
    let mut set = RecordSet::from_records(
        id.profile(),
        id.personas().cloned(),
        id.devices().cloned(),
        id.envelopes().cloned(),
        id.consents().cloned(),
        Some(&prev),
        Vec::new(),
        id.device_id(),
    )
    .unwrap();
    set.sign(&lockdown_vault::identity::identity_signing_key(&id, &keystore).unwrap())
        .unwrap();
    let pq = lockdown_vault::pq::identity_pq_key(&id, &keystore).unwrap();
    lockdown_vault::pq::sign_manifest(&mut set.manifest, &pq).unwrap();
    let rows: Vec<RecordRow> = set
        .records
        .iter()
        .map(|(path, body)| RecordRow {
            path: path.clone(),
            hash: lockdown_crypto::hash::hash(body),
            body: body.clone(),
        })
        .collect();
    let body = set.manifest.to_bytes().unwrap();
    let account = Id::parse(&account).unwrap();
    SqliteStorage::open(db)
        .unwrap()
        .accept_identity(
            account,
            IdentityAccept {
                manifest: &set.manifest,
                body: &body,
                records: &rows,
                isk_pub: &id.profile().isk_pub,
                revoked: &[],
                revocations: &[],
                handed_over_to: Some(id.profile().profile_id),
            },
        )
        .unwrap()
        .unwrap();
    (id, keystore, account)
}

/// Whether Bob's vault key envelope on the server opens with `identity`'s
/// receiving key.
fn opens_for(db: &std::path::Path, vault: &str, account: Id, identity: &Identity) -> bool {
    let body: Vec<u8> = rusqlite::Connection::open(db)
        .unwrap()
        .query_row(
            "SELECT e.body FROM share_envelopes e JOIN shared_vaults s ON s.vault_id = e.vault_id AND s.key_ref = e.key_ref WHERE e.vault_id = ?1 AND e.account_id = ?2",
            [vault, &account.to_string()],
            |r| r.get(0),
        )
        .unwrap();
    let env = ShareEnvelope::from_bytes(&body).unwrap();
    let persona = identity.personas().next().unwrap().persona_id;
    let kem = identity.persona_kem_secret(persona).unwrap();
    // The signature is the server's to check on the way in; here only
    // whose key opens it matters, so the envelope's own sender key is
    // looked up from the device that signed it.
    let sender: Vec<u8> = rusqlite::Connection::open(db)
        .unwrap()
        .query_row(
            "SELECT device_pub FROM devices WHERE device_id = ?1",
            [env.by_device.to_string()],
            |r| r.get(0),
        )
        .unwrap();
    let sender = lockdown_crypto::VerifyingKey::from_bytes(&sender.try_into().unwrap()).unwrap();
    env.open::<VaultKey>(&kem, &env.place, &sender).is_ok()
}

#[test]
fn a_tab_asks_its_owner_about_a_reset_and_lets_them_back_in_through_the_gate() {
    let dir = tempfile::tempdir().unwrap();
    let running = server(dir.path());
    let url = running.url();
    let db = dir.path().join("server.sqlite");
    let (alice, alice_secret) = desktop(dir.path(), &url, "alice");
    let (bob, _) = desktop(dir.path(), &url, "bob");
    let vault = alice.vaults().unwrap()[0].id.clone();
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
    ok(&mut web, "sync", json!({}));
    let overview = ok(&mut web, "sharing_overview", json!({}));
    assert!(
        overview.get("resets").is_none(),
        "absent when none: {overview}"
    );

    // Bob's account is reset. The tab's sync asks, in the core's words.
    let (bob2, _keystore, account) = reset_bob(dir.path(), &db);
    ok(&mut web, "sync", json!({}));
    let overview = ok(&mut web, "sharing_overview", json!({}));
    let resets = overview["resets"].as_array().unwrap();
    assert_eq!(resets.len(), 1);
    assert_eq!(
        resets[0]["words"],
        "bob@example.com reset their account. Let them back in?"
    );
    assert_eq!(resets[0]["account_id"], account.to_string());
    assert!(!opens_for(&db, &vault, account, &bob2));

    // Yes needs the gate: without the password, nothing happens.
    let refused = call(
        &mut web,
        "sharing_let_back_in",
        json!({ "account": account.to_string() }),
    );
    assert!(refused.is_err());
    assert!(!opens_for(&db, &vault, account, &bob2));
    let login = ok(
        &mut web,
        "sharing_let_back_in",
        json!({ "account": account.to_string(), "gate": { "password": PASSWORD } }),
    );
    assert_eq!(login, "bob@example.com");
    assert!(opens_for(&db, &vault, account, &bob2));
    let overview = ok(&mut web, "sharing_overview", json!({}));
    assert!(overview.get("resets").is_none(), "{overview}");
}
