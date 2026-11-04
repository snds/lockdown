//! What doesn't verify is held back, quietly, end to end against the real
//! server (docs/plan/49-shared-vault-quiet.md). A hostile server bends,
//! strips or rolls back records other people made: a member's head, a
//! membership change, an item shared on its own, an emergency grant. The
//! vault opens at the newest state that checks out, one calm line names the
//! person, nothing is sent on top of what didn't verify, this device's own
//! changes wait and survive, and once the server serves the good records
//! they are taken and the line goes by itself. The person's own records are
//! still refused, as before.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use ecosystem_identity::FileKeystore;
use lockdown_app::{App, GateInput, LockPolicy, Paths};
use lockdown_server::{AppState, Config, RunningServer, run_in_background};
use lockdown_vault::ManifestHead;
use lockdown_vault::sharing::{EmergencyGrant, MembershipEvent, ShareEnvelope};

const PASSWORD: &str = "correct horse battery staple";
const NOTHING: &str = "You don't need to do anything.";

struct Server {
    running: RunningServer,
    clock: Arc<AtomicI64>,
}

fn server_on_disk(dir: &Path) -> Server {
    let storage =
        lockdown_server::storage::SqliteStorage::open(&dir.join("server.sqlite")).unwrap();
    let keys =
        lockdown_server::keys::ServerKeys::load_or_create(&dir.join("server-key.cbor")).unwrap();
    let config = Config {
        per_login: (1000, 1000.0),
        per_address: (1000, 1000.0),
        session_ttl: std::time::Duration::from_secs(60 * 86_400),
        ..Config::default()
    };
    let mut state = AppState::new(Box::new(storage), keys, config);
    let clock = Arc::new(AtomicI64::new(lockdown_vault::Timestamp::now().0));
    let c = clock.clone();
    state.clock = Box::new(move || c.load(Ordering::SeqCst));
    Server {
        running: run_in_background(Arc::new(state)).unwrap(),
        clock,
    }
}

fn app(dir: &Path, name: &str) -> App {
    let paths = Paths::resolve(Some(dir.join(name).join("snds/identity")), None).unwrap();
    let app = App::start(paths, None, LockPolicy::default());
    app.open_or_create_plain().unwrap();
    app
}

/// Registers `name` and answers the kit's Secret Key.
fn register(app: &App, url: &str, name: &str) -> String {
    let reg = app
        .account_register(url, &format!("{name}@example.com"), PASSWORD, None)
        .unwrap();
    let page = String::from_utf8(reg.kit.unwrap().expose().to_vec()).unwrap();
    let sk = page
        .lines()
        .find_map(|l| l.strip_prefix("Secret Key:"))
        .unwrap()
        .trim()
        .to_string();
    app.sync().unwrap();
    sk
}

fn pw() -> GateInput {
    GateInput {
        password: Some(PASSWORD.into()),
        confirmed: false,
    }
}

/// Changes an identity on disk while its app is locked (an older device's
/// record, as another app holding this device's key would write it).
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

fn titles(app: &App) -> Vec<String> {
    let mut t: Vec<String> = app.list().unwrap().into_iter().map(|r| r.title).collect();
    t.sort();
    t
}

fn has(app: &App, title: &str) -> bool {
    titles(app).iter().any(|t| t == title)
}

/// A hostile server's hand on its own database.
struct Db(std::path::PathBuf);

impl Db {
    fn conn(&self) -> rusqlite::Connection {
        rusqlite::Connection::open(&self.0).unwrap()
    }

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

    /// Drops the newest head, as a server that lost or hid it would.
    fn roll_back(&self, vault: &str) -> (ManifestHead, Vec<u8>) {
        let conn = self.conn();
        let (seq, hash, body): (i64, Vec<u8>, Vec<u8>) = conn
            .query_row(
                "SELECT seq, hash, body FROM heads WHERE vault_id = ?1 ORDER BY seq DESC LIMIT 1",
                [vault],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        let before: Vec<u8> = conn
            .query_row(
                "SELECT hash FROM heads WHERE vault_id = ?1 AND seq = ?2",
                rusqlite::params![vault, seq - 1],
                |r| r.get(0),
            )
            .unwrap();
        conn.execute(
            "DELETE FROM heads WHERE vault_id = ?1 AND seq = ?2",
            rusqlite::params![vault, seq],
        )
        .unwrap();
        conn.execute(
            "UPDATE vaults SET head_seq = ?1, head_hash = ?2 WHERE vault_id = ?3",
            rusqlite::params![seq - 1, before, vault],
        )
        .unwrap();
        (ManifestHead::from_bytes(&body).unwrap(), hash)
    }

    /// Puts back a head [`Self::roll_back`] dropped.
    fn restore(&self, vault: &str, head: &ManifestHead, hash: &[u8]) {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO heads (vault_id, seq, hash, body) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![vault, head.seq as i64, hash, head.to_bytes().unwrap()],
        )
        .unwrap();
        conn.execute(
            "UPDATE vaults SET head_seq = ?1, head_hash = ?2 WHERE vault_id = ?3",
            rusqlite::params![head.seq as i64, hash, vault],
        )
        .unwrap();
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

    /// The share envelope of the item shared on its own with `item`'s id.
    fn item_share(&self, item: &str) -> (String, ShareEnvelope) {
        let (id, body): (String, Vec<u8>) = self
            .conn()
            .query_row(
                "SELECT share_id, envelope FROM item_shares WHERE item_id = ?1",
                [item],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        (id, ShareEnvelope::from_bytes(&body).unwrap())
    }

    fn put_item_share(&self, id: &str, env: &ShareEnvelope) {
        let n = self
            .conn()
            .execute(
                "UPDATE item_shares SET envelope = ?1 WHERE share_id = ?2",
                rusqlite::params![env.to_bytes().unwrap(), id],
            )
            .unwrap();
        assert_eq!(n, 1);
    }

    fn grant(&self, id: &str) -> EmergencyGrant {
        let body: Vec<u8> = self
            .conn()
            .query_row(
                "SELECT body FROM emergency_grants WHERE grant_id = ?1",
                [id],
                |r| r.get(0),
            )
            .unwrap();
        EmergencyGrant::from_bytes(&body).unwrap()
    }

    fn put_grant(&self, grant: &EmergencyGrant) {
        let n = self
            .conn()
            .execute(
                "UPDATE emergency_grants SET body = ?1 WHERE grant_id = ?2",
                rusqlite::params![grant.to_bytes().unwrap(), grant.grant_id.to_string()],
            )
            .unwrap();
        assert_eq!(n, 1);
    }
}

/// The lines a sync said that something was held back.
fn quiet_lines(app: &App) -> Vec<String> {
    app.sync()
        .unwrap()
        .lines
        .into_iter()
        .filter(|l| l.contains("couldn't be verified"))
        .collect()
}

fn vault_line(app: &App, vault: &str) -> Option<String> {
    app.vaults()
        .unwrap()
        .into_iter()
        .find(|v| v.id == vault)
        .and_then(|v| v.unverified)
}

#[test]
fn a_shared_vault_holds_back_what_doesnt_verify_keeps_its_own_changes_and_clears_by_itself() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server_on_disk(dir.path());
    let url = srv.running.url();
    let db = Db(dir.path().join("server.sqlite"));

    let alice = app(dir.path(), "alice");
    let alice_sk = register(&alice, &url, "alice");
    let bob = app(dir.path(), "bob");
    register(&bob, &url, "bob");
    // A household beside Alice's own first vault, shared with Bob to edit.
    let vault = alice.create_household("Family").unwrap();
    alice
        .add_item_in(&vault, "note", Some("Mail"), false, Vec::new())
        .unwrap();
    alice.sync().unwrap();
    alice.lookup_contact("bob@example.com").unwrap();
    alice
        .share_vault(&vault, "bob@example.com", "edit", pw())
        .unwrap();
    bob.sync().unwrap();
    alice.sync().unwrap();

    // Bob writes twice; the server bends the device signature on his newer
    // head. Alice has a change of her own she hasn't synced yet.
    for title in ["Bob one", "Bob two"] {
        bob.add_item_in(&vault, "note", Some(title), false, Vec::new())
            .unwrap();
        bob.sync().unwrap();
    }
    let good = db.newest_head(&vault);
    let mut forged = good.clone();
    forged.sig[5] ^= 1;
    db.put_head(&vault, &forged);
    alice
        .add_item_in(&vault, "note", Some("Alice local"), false, Vec::new())
        .unwrap();
    let on_server = db.heads(&vault).len();

    // The vault opens at the newest state that checks out: Bob's first
    // head, not the forged one, and Alice's own change is still there.
    let lines = quiet_lines(&alice);
    let line = "Some recent changes from bob@example.com couldn't be verified, so Lockdown left them out. You don't need to do anything.";
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(lines[0], format!("In Family: {line}"));
    assert!(has(&alice, "Bob one"));
    assert!(!has(&alice, "Bob two"));
    assert!(has(&alice, "Alice local"));
    assert_eq!(vault_line(&alice, &vault).as_deref(), Some(line));
    // Nothing went out on top of the head that didn't verify.
    assert_eq!(db.heads(&vault).len(), on_server);
    // The detail is for support, on Status.
    let detail = alice.status().unwrap().held_back.join("\n");
    assert!(
        detail.contains("device signature didn't check out"),
        "{detail}"
    );
    assert!(
        detail.contains(&format!("manifest {}", good.seq)),
        "{detail}"
    );
    assert!(detail.contains("bob@example.com"), "{detail}");
    // Who's in it can't change from here meanwhile: the read-only words.
    let carol = app(dir.path(), "carol");
    register(&carol, &url, "carol");
    alice.lookup_contact("carol@example.com").unwrap();
    let refused = alice
        .share_vault(&vault, "carol@example.com", "view", pw())
        .unwrap_err();
    assert_eq!(
        refused.message,
        "Some recent changes from bob@example.com couldn't be verified, so Lockdown left them out. Until they can be, people can't be added or removed here. You don't need to do anything."
    );
    let panel = alice.vault_sharing(&vault).unwrap();
    assert!(!panel.can_manage);
    assert_eq!(panel.held.as_deref(), Some(refused.message.as_str()));
    // Alice's change survives a lock, and the next sync tries again.
    alice.lock().unwrap();
    alice.unlock().unwrap();
    assert!(has(&alice, "Alice local") && has(&alice, "Bob one"));
    assert_eq!(quiet_lines(&alice).len(), 1);
    assert_eq!(db.heads(&vault).len(), on_server);

    // The server serves the good head: it is taken, the line goes, and
    // Alice's change goes out on top of it.
    db.put_head(&vault, &good);
    assert!(quiet_lines(&alice).is_empty());
    assert!(has(&alice, "Bob two") && has(&alice, "Alice local"));
    assert!(vault_line(&alice, &vault).is_none());
    assert!(alice.status().unwrap().held_back.is_empty());
    assert!(alice.vault_sharing(&vault).unwrap().can_manage);
    let heads = db.heads(&vault);
    assert_eq!(heads.len(), on_server + 1);
    let mine = heads.last().unwrap();
    assert_ne!(mine.signer, good.signer);
    assert_eq!(mine.prev_hash, good.hash().unwrap());
    bob.sync().unwrap();
    assert!(has(&bob, "Alice local"));

    // A rolled-back head of Bob's: the server drops the head Alice already
    // took. Held back, the vault stays as it was, and it clears once the
    // server shows it again.
    bob.add_item_in(&vault, "note", Some("Bob three"), false, Vec::new())
        .unwrap();
    bob.sync().unwrap();
    alice.sync().unwrap();
    assert!(has(&alice, "Bob three"));
    let (dropped, hash) = db.roll_back(&vault);
    let lines = quiet_lines(&alice);
    assert!(lines[0].ends_with(line), "{lines:?}");
    assert!(has(&alice, "Bob three"));
    let detail = alice.status().unwrap().held_back.join("\n");
    assert!(detail.contains("ollback"), "{detail}");
    db.restore(&vault, &dropped, &hash);
    assert!(quiet_lines(&alice).is_empty());

    // A membership change of Bob's own making is held back at Alice's. Bob
    // gets manage and adds Carol; the server bends the change's device
    // signature before Alice pulls it.
    let b = alice.lookup_contact("bob@example.com").unwrap();
    alice
        .set_member_right(&vault, &b.account_id, "manage")
        .unwrap();
    bob.sync().unwrap();
    bob.lookup_contact("carol@example.com").unwrap();
    bob.share_vault(&vault, "carol@example.com", "view", pw())
        .unwrap();
    let event = db.newest_event(&vault);
    let mut bad = event.clone();
    bad.sig[7] ^= 1;
    db.put_event(&bad);
    let carol_in = |app: &App| {
        app.vault_sharing(&vault)
            .unwrap()
            .members
            .iter()
            .any(|m| m.login == "carol@example.com")
    };
    let lines = quiet_lines(&alice);
    assert!(lines[0].ends_with(line), "{lines:?}");
    assert!(!carol_in(&alice));
    // While it is held back, Alice's changes wait here and nothing is sent.
    alice
        .add_item_in(&vault, "note", Some("Alice waits"), false, Vec::new())
        .unwrap();
    let before = db.heads(&vault).len();
    assert_eq!(quiet_lines(&alice).len(), 1);
    assert_eq!(db.heads(&vault).len(), before);
    assert!(has(&alice, "Alice waits"));
    db.put_event(&event);
    assert!(quiet_lines(&alice).is_empty());
    assert!(carol_in(&alice));
    assert_eq!(
        db.heads(&vault).len(),
        before + 1,
        "it went out once all verified"
    );

    // The person's own records are still refused: Alice's tablet takes a
    // head her laptop wrote, bent by the server, as a refusal, as before.
    let tablet = App::start(
        Paths::resolve(Some(dir.path().join("tablet").join("snds/identity")), None).unwrap(),
        None,
        LockPolicy::default(),
    );
    tablet
        .account_login(&url, "alice@example.com", PASSWORD, Some(&alice_sk))
        .unwrap();
    tablet.open_or_create(true).unwrap();
    tablet.sync().unwrap();
    alice
        .add_item_in(&vault, "note", Some("Alice laptop"), false, Vec::new())
        .unwrap();
    alice.sync().unwrap();
    let own = db.newest_head(&vault);
    let mut bent = own.clone();
    bent.sig[5] ^= 1;
    db.put_head(&vault, &bent);
    let err = tablet.sync().unwrap_err();
    assert!(err.message.contains("didn't check out"), "{}", err.message);
    db.put_head(&vault, &own);
    tablet.sync().unwrap();
    assert!(has(&tablet, "Alice laptop"));
}

#[test]
fn a_share_or_grant_from_someone_who_signed_before_must_carry_it_and_older_ones_still_arrive() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server_on_disk(dir.path());
    let url = srv.running.url();
    let db = Db(dir.path().join("server.sqlite"));

    let dan = app(dir.path(), "dan");
    register(&dan, &url, "dan");
    // Eve has a device from an older build: she publishes no post-quantum
    // key, so her shares carry Ed25519 only.
    let eve = app(dir.path(), "eve");
    let old = on_disk(&eve, |identity, keystore| {
        lockdown_vault::test_support::add_device_at_level(
            identity,
            keystore,
            "Eve's old phone",
            None,
        )
        .unwrap()
    });
    register(&eve, &url, "eve");
    let alice = app(dir.path(), "alice");
    register(&alice, &url, "alice");

    // Eve, who never signed post-quantum, shares an item: it arrives.
    let eves_old = eve
        .add_login(
            "https://old.example.com",
            "eve",
            b"pw".to_vec(),
            Some("Eve old"),
        )
        .unwrap();
    eve.sync().unwrap();
    eve.lookup_contact("dan@example.com").unwrap();
    eve.share_item(&eves_old, "dan@example.com", "view", pw())
        .unwrap();
    assert!(db.item_share(&eves_old).1.pq_sig.is_none());
    dan.sync().unwrap();
    assert!(has(&dan, "Eve old"));

    // Eve's phone updates: her key goes out and her new shares carry it.
    on_disk(&eve, |identity, keystore| {
        lockdown_vault::test_support::set_device_level(identity, keystore, old, Some(2)).unwrap()
    });
    eve.sync().unwrap();
    let eves_new = eve
        .add_login(
            "https://new.example.com",
            "eve",
            b"pw".to_vec(),
            Some("Eve new"),
        )
        .unwrap();
    eve.sync().unwrap();
    eve.share_item(&eves_new, "dan@example.com", "view", pw())
        .unwrap();
    let (new_share, signed) = db.item_share(&eves_new);
    assert!(signed.pq_sig.is_some());
    // Dan looks Eve up again and pins her key, then sees her signed share:
    // from his next sync on, hers must carry one. Her older share, which he
    // took before, still opens.
    dan.lookup_contact("eve@example.com").unwrap();
    dan.sync().unwrap();
    assert!(has(&dan, "Eve new"));
    dan.sync().unwrap();
    assert!(has(&dan, "Eve old") && has(&dan, "Eve new"));

    // The server strips the signature off her newer share: held back with
    // the quiet line, and the copy Dan had stays.
    let mut stripped = signed.clone();
    stripped.pq_sig = None;
    db.put_item_share(&new_share, &stripped);
    let lines = quiet_lines(&dan);
    assert_eq!(
        lines,
        vec![format!(
            "A shared item from eve@example.com couldn't be verified, so Lockdown left it out. {NOTHING}"
        )]
    );
    assert!(has(&dan, "Eve new") && has(&dan, "Eve old"));
    let detail = dan.status().unwrap().held_back.join("\n");
    assert!(detail.contains("no post-quantum signature"), "{detail}");
    db.put_item_share(&new_share, &signed);
    assert!(quiet_lines(&dan).is_empty());

    // An emergency grant from Alice, whose shares Dan saw signed: stripped,
    // it releases nothing and says so; put back, it releases.
    // Her item shared on its own comes from another vault than the one
    // the grant covers.
    let home = alice.create_household("Home").unwrap();
    let alices = alice
        .add_item_in(&home, "note", Some("Bank"), false, Vec::new())
        .unwrap();
    alice
        .add_login(
            "https://diary.example.com",
            "alice",
            b"pw".to_vec(),
            Some("Diary"),
        )
        .unwrap();
    alice.sync().unwrap();
    alice.lookup_contact("dan@example.com").unwrap();
    alice
        .share_item(&alices, "dan@example.com", "view", pw())
        .unwrap();
    dan.lookup_contact("alice@example.com").unwrap();
    dan.sync().unwrap();
    assert!(has(&dan, "Bank"));
    let d = alice.lookup_contact("dan@example.com").unwrap();
    let dans_own = dan.sharing_overview().unwrap().fingerprint;
    alice.verify_contact(&d.account_id, &dans_own).unwrap();
    let personal = alice.vaults().unwrap()[0].id.clone();
    let grant_id = alice
        .add_emergency_contact("dan@example.com", 1, vec![personal], pw())
        .unwrap()[0]
        .grant_id
        .clone();
    let grant = db.grant(&grant_id);
    assert!(grant.pq_sig.is_some());
    dan.sync().unwrap();
    let mut stripped = grant.clone();
    stripped.pq_sig = None;
    db.put_grant(&stripped);
    dan.emergency_step(&grant_id, "accept").unwrap();
    dan.emergency_step(&grant_id, "request").unwrap();
    srv.clock.fetch_add(2 * 86_400, Ordering::SeqCst);
    let refused = dan.emergency_open(&grant_id).unwrap_err();
    assert_eq!(
        refused.message,
        format!(
            "Emergency access from alice@example.com couldn't be verified, so Lockdown left it out for now. {NOTHING}"
        )
    );
    assert!(!has(&dan, "Diary"));
    db.put_grant(&grant);
    dan.emergency_open(&grant_id).unwrap();
    assert!(has(&dan, "Diary"));
    assert!(
        dan.sync()
            .unwrap()
            .lines
            .iter()
            .all(|l| !l.contains("couldn't be verified"))
    );
}
