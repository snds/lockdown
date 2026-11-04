//! Post-quantum signatures in a shared vault, end to end against the real
//! server (docs/plan/47-shared-vault-pq.md): two people, each with their
//! own rollout. Alice's side signs post-quantum from the start; Bob, who
//! has a device from an older build and so publishes no key yet, writes
//! Ed25519 only and nothing breaks. Once his device updates, his heads
//! carry ML-DSA too, Alice pins his key at her next sync and remembers his
//! first post-quantum head, and from then on what a server bends, forges
//! or strips (a head, a membership change or a vault key grant) is held
//! back with one calm line, and taken once the server serves it as signed
//! (docs/plan/49-shared-vault-quiet.md). An item shared on its own whose
//! signature was bent doesn't arrive.

use std::path::Path;
use std::sync::Arc;

use ecosystem_identity::FileKeystore;
use lockdown_app::{App, GateInput, LockPolicy, Paths};
use lockdown_server::{AppState, Config, RunningServer, run_in_background};
use lockdown_store::HeadMemory;
use lockdown_vault::sharing::{MembershipEvent, ShareEnvelope};
use lockdown_vault::{Id, ManifestHead};

const PASSWORD: &str = "correct horse battery staple";

fn server_on_disk(dir: &Path) -> (Arc<AppState>, RunningServer) {
    let storage =
        lockdown_server::storage::SqliteStorage::open(&dir.join("server.sqlite")).unwrap();
    let keys =
        lockdown_server::keys::ServerKeys::load_or_create(&dir.join("server-key.cbor")).unwrap();
    let config = Config {
        per_login: (1000, 1000.0),
        per_address: (1000, 1000.0),
        ..Config::default()
    };
    let state = Arc::new(AppState::new(Box::new(storage), keys, config));
    let running = run_in_background(state.clone()).unwrap();
    (state, running)
}

fn app(dir: &Path, name: &str) -> App {
    let paths = Paths::resolve(Some(dir.join(name).join("snds/identity")), None).unwrap();
    let app = App::start(paths, None, LockPolicy::default());
    app.open_or_create_plain().unwrap();
    app
}

fn register(app: &App, url: &str, name: &str) {
    app.account_register(url, &format!("{name}@example.com"), PASSWORD, None)
        .unwrap();
    app.sync().unwrap();
}

/// Changes an identity on disk while its app is locked, as another app
/// holding this device's key would (here: an older device's record).
fn on_disk<T>(
    app: &App,
    edit: impl FnOnce(&mut ecosystem_identity::Identity, &FileKeystore) -> T,
) -> T {
    app.lock().unwrap();
    let probe = app.probe().unwrap();
    let keystore =
        FileKeystore::open_or_create(&Path::new(&probe.data).join("device-key.cbor")).unwrap();
    let mut identity =
        lockdown_vault::identity::open_identity(Path::new(&probe.root), &keystore, None)
            .unwrap()
            .unwrap();
    let out = edit(&mut identity, &keystore);
    app.unlock().unwrap();
    out
}

fn pw() -> GateInput {
    GateInput {
        password: Some(PASSWORD.into()),
        confirmed: false,
    }
}

/// A hostile server's hand on its own database.
struct Db(std::path::PathBuf);

impl Db {
    fn conn(&self) -> rusqlite::Connection {
        rusqlite::Connection::open(&self.0).unwrap()
    }

    /// Every head of `vault`, oldest first.
    fn heads(&self, vault: &str) -> Vec<ManifestHead> {
        let conn = self.conn();
        let mut st = conn
            .prepare("SELECT body FROM heads WHERE vault_id = ?1 ORDER BY seq")
            .unwrap();
        st.query_map([vault], |r| r.get::<_, Vec<u8>>(0))
            .unwrap()
            .map(|b| ManifestHead::from_bytes(&b.unwrap()).unwrap())
            .collect()
    }

    fn newest_head(&self, vault: &str) -> ManifestHead {
        self.heads(vault).pop().unwrap()
    }

    fn put_head(&self, vault: &str, head: &ManifestHead) {
        let n = self
            .conn()
            .execute(
                "UPDATE heads SET body = ?1 WHERE vault_id = ?2 AND seq = ?3",
                rusqlite::params![head.to_bytes().unwrap(), vault, head.seq as i64],
            )
            .unwrap();
        assert_eq!(n, 1);
    }

    fn newest_event(&self, vault: &str) -> MembershipEvent {
        let body: Vec<u8> = self
            .conn()
            .query_row(
                "SELECT body FROM membership_events WHERE vault_id = ?1 ORDER BY seq DESC LIMIT 1",
                [vault],
                |r| r.get(0),
            )
            .unwrap();
        MembershipEvent::from_bytes(&body).unwrap()
    }

    fn put_event(&self, event: &MembershipEvent) {
        let n = self
            .conn()
            .execute(
                "UPDATE membership_events SET body = ?1 WHERE vault_id = ?2 AND seq = ?3",
                rusqlite::params![
                    event.to_bytes().unwrap(),
                    event.vault_id.to_string(),
                    event.seq as i64
                ],
            )
            .unwrap();
        assert_eq!(n, 1);
    }

    /// The vault key envelope the server holds for `account`.
    fn envelope(&self, vault: &str, account: &str) -> ShareEnvelope {
        let body: Vec<u8> = self
            .conn()
            .query_row(
                "SELECT body FROM share_envelopes WHERE vault_id = ?1 AND account_id = ?2",
                [vault, account],
                |r| r.get(0),
            )
            .unwrap();
        ShareEnvelope::from_bytes(&body).unwrap()
    }

    fn put_envelope(&self, vault: &str, account: &str, env: &ShareEnvelope) {
        let n = self
            .conn()
            .execute(
                "UPDATE share_envelopes SET body = ?1 WHERE vault_id = ?2 AND account_id = ?3",
                rusqlite::params![env.to_bytes().unwrap(), vault, account],
            )
            .unwrap();
        assert_eq!(n, 1);
    }

    fn item_share(&self) -> (String, ShareEnvelope) {
        let (id, body): (String, Vec<u8>) = self
            .conn()
            .query_row("SELECT share_id, envelope FROM item_shares", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        (id, ShareEnvelope::from_bytes(&body).unwrap())
    }

    fn put_item_share(&self, id: &str, env: &ShareEnvelope) {
        self.conn()
            .execute(
                "UPDATE item_shares SET envelope = ?1 WHERE share_id = ?2",
                rusqlite::params![env.to_bytes().unwrap(), id],
            )
            .unwrap();
    }
}

fn bent(sig: &Option<serde_bytes::ByteBuf>) -> Option<serde_bytes::ByteBuf> {
    let mut sig = sig.clone().expect("signed post-quantum");
    sig[17] ^= 1;
    Some(sig)
}

fn titles(app: &App) -> Vec<String> {
    let mut t: Vec<String> = app.list().unwrap().into_iter().map(|r| r.title).collect();
    t.sort();
    t
}

#[test]
fn a_shared_vault_goes_post_quantum_side_by_side_and_what_a_server_bends_or_strips_is_held_back() {
    let dir = tempfile::tempdir().unwrap();
    let (_state, running) = server_on_disk(dir.path());
    let url = running.url();
    let db = Db(dir.path().join("server.sqlite"));

    // Alice: one up-to-date device, rolled out at her first sync.
    let alice = app(dir.path(), "alice");
    register(&alice, &url, "alice");
    // Bob: also a device from a build before the post-quantum mark, so his
    // profile publishes no post-quantum key yet.
    let bob = app(dir.path(), "bob");
    let old = on_disk(&bob, |identity, keystore| {
        lockdown_vault::test_support::add_device_at_level(
            identity,
            keystore,
            "Bob's old phone",
            None,
        )
        .unwrap()
    });
    register(&bob, &url, "bob");
    assert_eq!(bob.pq_status().unwrap().state, "waiting");

    // Alice shares her vault with Bob, to edit.
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
    let b = alice.lookup_contact("bob@example.com").unwrap();
    alice
        .share_vault(&vault, "bob@example.com", "edit", pw())
        .unwrap();
    // Her membership changes and the vault key grant carry both signatures.
    assert!(db.newest_event(&vault).pq_sig.is_some());
    assert!(db.envelope(&vault, &b.account_id).pq_sig.is_some());

    // Bob takes it and writes: Ed25519 only, and nobody refuses it.
    bob.sync().unwrap();
    bob.add_item_in(&vault, "note", Some("Bob one"), false, Vec::new())
        .unwrap();
    bob.sync().unwrap();
    let bob_head = db.newest_head(&vault);
    assert!(bob_head.pq_sig.is_none(), "Bob can't sign post-quantum yet");
    alice.sync().unwrap();
    assert!(titles(&alice).contains(&"Bob one".to_string()));
    alice
        .add_item_in(&vault, "note", Some("Alice two"), false, Vec::new())
        .unwrap();
    alice.sync().unwrap();
    assert!(
        db.newest_head(&vault).pq_sig.is_some(),
        "Alice's side doesn't wait for Bob's"
    );
    bob.sync().unwrap();
    assert!(titles(&bob).contains(&"Alice two".to_string()));

    // Bob's old phone updates: his key goes out and his heads carry both.
    on_disk(&bob, |identity, keystore| {
        lockdown_vault::test_support::set_device_level(identity, keystore, old, Some(2)).unwrap()
    });
    bob.sync().unwrap();
    assert_eq!(bob.pq_status().unwrap().state, "on");
    bob.add_item_in(&vault, "note", Some("Bob two"), false, Vec::new())
        .unwrap();
    bob.sync().unwrap();
    let first_pq = db.newest_head(&vault);
    assert!(first_pq.pq_sig.is_some(), "Bob's side is post-quantum now");
    assert_eq!(first_pq.signer, bob_head.signer);

    // Alice looks Bob up again at her sync, pins his key, checks his head
    // under it and remembers where his post-quantum heads began.
    alice.sync().unwrap();
    let alice_data = std::path::PathBuf::from(alice.probe().unwrap().data);
    let remembered = HeadMemory::open(&alice_data.join("remembered.cbor"))
        .unwrap()
        .member_pq_since(Id::parse(&vault).unwrap());
    assert_eq!(
        remembered.get(&Id::parse(&b.account_id).unwrap()),
        Some(&first_pq.seq)
    );

    // Bob writes again. The server bends that head's ML-DSA signature,
    // forges it under another key, or strips it: Alice holds it back each
    // time (docs/plan/49-shared-vault-quiet.md), and takes it once the
    // server serves the good one.
    bob.add_item_in(&vault, "note", Some("Bob three"), false, Vec::new())
        .unwrap();
    bob.sync().unwrap();
    let good = db.newest_head(&vault);
    let held = |app: &App, why: &str| {
        let out = app.sync().unwrap();
        assert!(
            out.lines
                .iter()
                .any(|l| l.contains("couldn't be verified") && l.contains("bob@example.com")),
            "{:?}",
            out.lines
        );
        let detail = app.status().unwrap().held_back.join("\n");
        assert!(detail.contains(why), "{detail}");
        assert!(!titles(app).contains(&"Bob three".to_string()));
    };
    let mut bad = good.clone();
    bad.pq_sig = bent(&good.pq_sig);
    db.put_head(&vault, &bad);
    held(&alice, "post-quantum signature didn't check out");
    let mut forged = good.clone();
    forged
        .sign_post_quantum(&lockdown_crypto::pq_sign::PqSigningKey::from_seed([5; 32]))
        .unwrap();
    db.put_head(&vault, &forged);
    held(&alice, "post-quantum signature didn't check out");
    let mut stripped = good.clone();
    stripped.pq_sig = None;
    db.put_head(&vault, &stripped);
    held(&alice, "lost its post-quantum signature");
    db.put_head(&vault, &good);
    let out = alice.sync().unwrap();
    assert!(!out.lines.iter().any(|l| l.contains("couldn't be verified")));
    assert!(titles(&alice).contains(&"Bob three".to_string()));
    assert!(alice.status().unwrap().held_back.is_empty());

    // A membership change: Alice adds Carol. The server bends or strips
    // the event's post-quantum signature before Bob pulls it: held back,
    // and taken once it is put back.
    let carol = app(dir.path(), "carol");
    register(&carol, &url, "carol");
    let c = alice.lookup_contact("carol@example.com").unwrap();
    alice
        .share_vault(&vault, "carol@example.com", "view", pw())
        .unwrap();
    let event = db.newest_event(&vault);
    let carol_in = |app: &App| {
        app.vault_sharing(&vault)
            .unwrap()
            .members
            .iter()
            .any(|m| m.login == "carol@example.com")
    };
    let mut bad = event.clone();
    bad.pq_sig = bent(&event.pq_sig);
    db.put_event(&bad);
    let out = bob.sync().unwrap();
    assert!(
        out.lines
            .iter()
            .any(|l| l.contains("couldn't be verified") && l.contains("alice@example.com")),
        "{:?}",
        out.lines
    );
    assert!(!carol_in(&bob));
    let mut stripped = event.clone();
    stripped.pq_sig = None;
    db.put_event(&stripped);
    bob.sync().unwrap();
    let detail = bob.status().unwrap().held_back.join("\n");
    assert!(
        detail.contains("lost its post-quantum signature"),
        "{detail}"
    );
    assert!(!carol_in(&bob));
    db.put_event(&event);
    bob.sync().unwrap();
    assert!(carol_in(&bob));
    assert!(bob.status().unwrap().held_back.is_empty());

    // The vault key grant to Carol, stripped: the change that handed it
    // over carries a post-quantum signature, so it must too. Held back:
    // she isn't in yet, and joins once it is put back.
    let grant = db.envelope(&vault, &c.account_id);
    let mut stripped = grant.clone();
    stripped.pq_sig = None;
    db.put_envelope(&vault, &c.account_id, &stripped);
    let out = carol.sync().unwrap();
    assert!(
        out.lines
            .iter()
            .any(|l| l.starts_with("A shared vault from alice@example.com")),
        "{:?}",
        out.lines
    );
    assert!(!titles(&carol).contains(&"Bob three".to_string()));
    let mut bad = grant.clone();
    bad.pq_sig = bent(&grant.pq_sig);
    db.put_envelope(&vault, &c.account_id, &bad);
    carol.sync().unwrap();
    assert!(!titles(&carol).contains(&"Bob three".to_string()));
    db.put_envelope(&vault, &c.account_id, &grant);
    carol.sync().unwrap();
    assert!(titles(&carol).contains(&"Bob three".to_string()));

    // An item shared on its own: signed with both keys; bent, it doesn't arrive.
    let mail = alice
        .list()
        .unwrap()
        .into_iter()
        .find(|r| r.title == "Mail")
        .unwrap()
        .id;
    let solo = alice
        .add_login(
            "https://solo.example.com",
            "alice",
            b"pw".to_vec(),
            Some("Solo"),
        )
        .unwrap();
    alice.sync().unwrap();
    assert_ne!(solo, mail);
    let carol_alone = app(dir.path(), "dan");
    register(&carol_alone, &url, "dan");
    alice.lookup_contact("dan@example.com").unwrap();
    alice
        .share_item(&solo, "dan@example.com", "view", pw())
        .unwrap();
    let (share, env) = db.item_share();
    assert!(env.pq_sig.is_some());
    let mut bad = env.clone();
    bad.pq_sig = bent(&env.pq_sig);
    db.put_item_share(&share, &bad);
    let out = carol_alone.sync().unwrap();
    assert!(!titles(&carol_alone).contains(&"Solo".to_string()));
    assert!(
        out.lines
            .iter()
            .any(|l| l.starts_with("A shared item from alice@example.com")),
        "{:?}",
        out.lines
    );
    db.put_item_share(&share, &env);
    carol_alone.sync().unwrap();
    assert!(titles(&carol_alone).contains(&"Solo".to_string()));
}
