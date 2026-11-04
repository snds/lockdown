//! An item shared on its own follows other people's edits
//! (docs/plan/54-share-follows-edits.md), through `App` against the real
//! server. Alice shares "Trip" with members and one login of it with Dave
//! on its own. Dave trusts only Alice's devices for it. When a member edits
//! the login, or a manager re-keys the vault, Alice's device signs the
//! newest version again at its next sync, and Dave's copy follows at his.
//! A forged edit by someone who may not write is held back at Alice's and
//! never reaches Dave. Before this, Dave kept the last copy that opened.

use std::path::Path;
use std::sync::Arc;

use ecosystem_identity::FileKeystore;
use lockdown_app::{App, FieldChange, GateInput, LockPolicy, Paths};
use lockdown_server::{AppState, Config, RunningServer, run_in_background};
use lockdown_vault::{Item, ManifestHead};

const PASSWORD: &str = "correct horse battery staple";

fn server(dir: &Path) -> RunningServer {
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
    run_in_background(Arc::new(AppState::new(Box::new(storage), keys, config))).unwrap()
}

fn person(dir: &Path, name: &str, srv: &RunningServer) -> App {
    let paths = Paths::resolve(Some(dir.join(name).join("snds/identity")), None).unwrap();
    let app = App::start(paths, None, LockPolicy::default());
    app.open_or_create_plain().unwrap();
    app.account_register(&srv.url(), &login(name), PASSWORD, None)
        .unwrap();
    app.sync().unwrap();
    app
}

fn pw() -> GateInput {
    GateInput {
        password: Some(PASSWORD.into()),
        confirmed: false,
    }
}

fn login(name: &str) -> String {
    format!("{name}@example.com")
}

fn rows_of(app: &App, id: &str) -> usize {
    app.list().unwrap().iter().filter(|r| r.id == id).count()
}

fn secret(app: &App, id: &str) -> Vec<u8> {
    app.reveal(id, "pw", GateInput::default())
        .unwrap()
        .expose()
        .to_vec()
}

/// Syncs and answers the lines that said something was held back or waits.
fn quiet_lines(app: &App) -> Vec<String> {
    app.sync()
        .unwrap()
        .lines
        .into_iter()
        .filter(|l| l.contains("couldn't be verified") || l.contains("wait until"))
        .collect()
}

/// A new password for `id`, as an edit.
fn set_pw(app: &App, id: &str, value: &[u8]) {
    app.edit_item(
        id,
        None,
        vec![FieldChange::keep("user"), FieldChange::secret("pw", value)],
        GateInput::default(),
    )
    .unwrap();
}

/// Alice's vault "Trip" with a login and a note in it, synced; the login,
/// Rail, goes to Dave on its own.
fn trip(alice: &App) -> (String, String, String) {
    let trip = alice.create_vault("Trip").unwrap();
    let mut ids = Vec::new();
    for (title, value) in [("Rail", &b"pw-rail"[..]), ("Hotel", &b"pw-hotel"[..])] {
        ids.push(
            alice
                .add_item_in(
                    &trip,
                    "login",
                    Some(title),
                    false,
                    vec![
                        FieldChange::text("user", "alice"),
                        FieldChange::secret("pw", value),
                    ],
                )
                .unwrap(),
        );
    }
    alice.sync().unwrap();
    let hotel = ids.pop().unwrap();
    let rail = ids.pop().unwrap();
    (trip, rail, hotel)
}

#[test]
fn a_members_edit_reaches_the_person_holding_the_item_once_the_owners_device_syncs() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server(dir.path());
    let alice = person(dir.path(), "alice", &srv);
    let bob = person(dir.path(), "bob", &srv);
    let dave = person(dir.path(), "dave", &srv);
    let (trip, rail, hotel) = trip(&alice);
    for who in ["bob", "dave"] {
        alice.lookup_contact(&login(who)).unwrap();
    }
    alice
        .share_vault(&trip, &login("bob"), "edit", pw())
        .unwrap();
    alice
        .share_item(&rail, &login("dave"), "view", pw())
        .unwrap();
    bob.sync().unwrap();
    dave.sync().unwrap();
    assert_eq!(secret(&dave, &rail), b"pw-rail");
    let versions = alice.history(&rail).unwrap().len();

    // Bob changes the password. Until Alice's device has seen it, Dave
    // keeps the copy he has, quietly.
    set_pw(&bob, &rail, b"pw-bob");
    bob.sync().unwrap();
    assert!(quiet_lines(&dave).is_empty());
    assert_eq!(rows_of(&dave, &rail), 1);
    assert_eq!(secret(&dave, &rail), b"pw-rail");

    // Alice's next sync takes Bob's edit and signs it again, in that sync.
    alice.sync().unwrap();
    assert_eq!(secret(&alice, &rail), b"pw-bob");
    let history = alice.history(&rail).unwrap();
    assert_eq!(history.len(), versions + 2, "Bob's edit and the follow");
    assert!(history[0].changed.is_empty(), "nothing changed");
    assert!(history[0].current && !history[1].changed.is_empty());
    // Dave's next sync brings it, with nothing said.
    assert!(quiet_lines(&dave).is_empty());
    assert_eq!(rows_of(&dave, &rail), 1);
    assert_eq!(rows_of(&dave, &hotel), 0);
    assert_eq!(secret(&dave, &rail), b"pw-bob");

    // Later syncs on every side change nothing: Alice's own version needs
    // no follow, and Bob takes it with no conflict copy.
    for app in [&alice, &bob, &dave, &alice] {
        app.sync().unwrap();
    }
    assert_eq!(alice.history(&rail).unwrap().len(), versions + 2);
    assert_eq!(rows_of(&bob, &rail), 1);
    assert_eq!(bob.list().unwrap().len(), alice.list().unwrap().len());
    assert_eq!(secret(&bob, &rail), b"pw-bob");
    // After Dave's lock and unlock too.
    dave.lock().unwrap();
    dave.unlock().unwrap();
    assert_eq!(secret(&dave, &rail), b"pw-bob");

    // An edit of Alice's own needs no follow.
    set_pw(&alice, &rail, b"pw-alice");
    alice.sync().unwrap();
    assert_eq!(alice.history(&rail).unwrap().len(), versions + 3);
    dave.sync().unwrap();
    assert_eq!(secret(&dave, &rail), b"pw-alice");
}

#[test]
fn a_managers_re_key_still_opens_for_the_person_holding_the_item() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server(dir.path());
    let alice = person(dir.path(), "alice", &srv);
    let bob = person(dir.path(), "bob", &srv);
    let carol = person(dir.path(), "carol", &srv);
    let dave = person(dir.path(), "dave", &srv);
    let (trip, rail, _) = trip(&alice);
    for who in ["bob", "carol", "dave"] {
        alice.lookup_contact(&login(who)).unwrap();
    }
    alice
        .share_vault(&trip, &login("bob"), "manage", pw())
        .unwrap();
    alice
        .share_vault(&trip, &login("carol"), "view", pw())
        .unwrap();
    alice
        .share_item(&rail, &login("dave"), "view", pw())
        .unwrap();
    for app in [&bob, &carol, &dave] {
        app.sync().unwrap();
    }
    assert_eq!(secret(&dave, &rail), b"pw-rail");

    // Bob, a manager, removes Carol: a new Vault Key, fresh Item Keys and
    // a re-key version of Bob's on every item.
    let carol_account = bob.lookup_contact(&login("carol")).unwrap().account_id;
    bob.remove_member(&trip, &carol_account).unwrap();
    // Before Alice's device has seen it, Dave keeps his copy.
    assert!(quiet_lines(&dave).is_empty());
    assert_eq!(secret(&dave, &rail), b"pw-rail");

    // Alice's sync takes the new key and Bob's re-key versions, signs Rail
    // again and hands Dave the new key.
    alice.sync().unwrap();
    assert!(quiet_lines(&dave).is_empty());
    assert_eq!(rows_of(&dave, &rail), 1);
    assert_eq!(secret(&dave, &rail), b"pw-rail");
    // Dave holds the newest copy, not the one from before the re-key.
    assert_eq!(
        dave.history(&rail).unwrap().len(),
        alice.history(&rail).unwrap().len()
    );
    carol.sync().unwrap();
    assert_eq!(rows_of(&carol, &rail), 0);

    // An edit Bob makes under the new keys follows too.
    set_pw(&bob, &rail, b"pw-new");
    bob.sync().unwrap();
    alice.sync().unwrap();
    dave.sync().unwrap();
    assert_eq!(secret(&dave, &rail), b"pw-new");
    dave.lock().unwrap();
    dave.unlock().unwrap();
    assert_eq!(secret(&dave, &rail), b"pw-new");
}

/// A hostile server's hand on its own database.
struct Db(std::path::PathBuf);

impl Db {
    fn conn(&self) -> rusqlite::Connection {
        rusqlite::Connection::open(&self.0).unwrap()
    }

    fn newest_head(&self, vault: &str) -> ManifestHead {
        let body: Vec<u8> = self
            .conn()
            .query_row(
                "SELECT body FROM heads WHERE vault_id = ?1 ORDER BY seq DESC LIMIT 1",
                [vault],
                |r| r.get(0),
            )
            .unwrap();
        ManifestHead::from_bytes(&body).unwrap()
    }

    fn count_heads(&self, vault: &str) -> i64 {
        self.conn()
            .query_row(
                "SELECT COUNT(*) FROM heads WHERE vault_id = ?1",
                [vault],
                |r| r.get(0),
            )
            .unwrap()
    }

    /// Puts `head` in place of the head at its seq, as the vault's newest.
    fn put_head(&self, vault: &str, head: &ManifestHead) {
        let hash = head.hash().unwrap().as_bytes().to_vec();
        let conn = self.conn();
        let n = conn
            .execute(
                "UPDATE heads SET body = ?1, hash = ?2 WHERE vault_id = ?3 AND seq = ?4",
                rusqlite::params![head.to_bytes().unwrap(), hash, vault, head.seq as i64],
            )
            .unwrap();
        assert_eq!(n, 1);
        conn.execute(
            "UPDATE vaults SET head_hash = ?1 WHERE vault_id = ?2",
            rusqlite::params![hash, vault],
        )
        .unwrap();
    }

    fn item(&self, item: &str) -> Item {
        let body: Vec<u8> = self
            .conn()
            .query_row("SELECT body FROM items WHERE item_id = ?1", [item], |r| {
                r.get(0)
            })
            .unwrap();
        Item::from_bytes(&body).unwrap()
    }

    fn put_item(&self, item: &Item) {
        let n = self
            .conn()
            .execute(
                "UPDATE items SET body = ?1 WHERE item_id = ?2",
                rusqlite::params![item.to_bytes().unwrap(), item.item_id.to_string()],
            )
            .unwrap();
        assert_eq!(n, 1);
    }
}

/// The device id and keystore of a locked app's device, then unlocked again.
fn device_of(app: &App) -> (lockdown_vault::Id, Arc<FileKeystore>) {
    app.lock().unwrap();
    let probe = app.probe().unwrap();
    let keystore = Arc::new(
        FileKeystore::open_or_create(&Path::new(&probe.data).join("device-key.cbor")).unwrap(),
    );
    let identity =
        lockdown_vault::identity::open_identity(Path::new(&probe.root), keystore.as_ref(), None)
            .unwrap()
            .unwrap();
    let device = identity.device_id();
    app.unlock().unwrap();
    (device, keystore)
}

#[test]
fn a_forged_edit_by_someone_who_may_not_write_is_held_back_and_never_followed() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server(dir.path());
    let db = Db(dir.path().join("server.sqlite"));
    let alice = person(dir.path(), "alice", &srv);
    let bob = person(dir.path(), "bob", &srv);
    let carol = person(dir.path(), "carol", &srv);
    let dave = person(dir.path(), "dave", &srv);
    let (trip, rail, _) = trip(&alice);
    for who in ["bob", "carol", "dave"] {
        alice.lookup_contact(&login(who)).unwrap();
    }
    alice
        .share_vault(&trip, &login("bob"), "edit", pw())
        .unwrap();
    alice
        .share_vault(&trip, &login("carol"), "view", pw())
        .unwrap();
    alice
        .share_item(&rail, &login("dave"), "view", pw())
        .unwrap();
    for app in [&bob, &carol, &dave] {
        app.sync().unwrap();
    }
    let versions = alice.history(&rail).unwrap().len();

    // Bob's edit goes up. The server turns it into Carol's: her own device
    // key signs the newest version and the head, though she may only view.
    set_pw(&bob, &rail, b"pw-bob");
    bob.sync().unwrap();
    let good_head = db.newest_head(&trip);
    let good_item = db.item(&rail);
    let (carol_device, carol_key) = device_of(&carol);
    let mut forged = good_item.clone();
    let header = forged.header();
    let latest = forged.versions.last_mut().unwrap();
    latest.edited_by = carol_device;
    latest.sign(&header, &carol_key).unwrap();
    let forged_hash = forged.latest_hash().unwrap();
    let mut head = good_head.clone();
    for e in head
        .entries
        .iter_mut()
        .filter(|e| e.item_id == forged.item_id)
    {
        e.version_hash = forged_hash;
    }
    head.signer = carol_device;
    head.pq_sig = None;
    head.sign(&carol_key).unwrap();
    db.put_item(&forged);
    db.put_head(&trip, &head);
    let on_server = db.count_heads(&trip);

    // Alice holds it back: Rail stays as it was, nothing is signed again,
    // and nothing goes out on top of it.
    let lines = quiet_lines(&alice);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].contains("couldn't be verified"), "{lines:?}");
    assert_eq!(secret(&alice, &rail), b"pw-rail");
    assert_eq!(alice.history(&rail).unwrap().len(), versions);
    assert_eq!(db.count_heads(&trip), on_server);
    // Dave, served the forged copy, keeps his own, quietly.
    assert!(quiet_lines(&dave).is_empty());
    assert_eq!(rows_of(&dave, &rail), 1);
    assert_eq!(secret(&dave, &rail), b"pw-rail");

    // The server serves Bob's real edit again: Alice takes it and follows
    // it, and Dave's copy follows.
    db.put_item(&good_item);
    db.put_head(&trip, &good_head);
    assert!(quiet_lines(&alice).is_empty());
    assert_eq!(secret(&alice, &rail), b"pw-bob");
    assert_eq!(alice.history(&rail).unwrap().len(), versions + 2);
    assert!(quiet_lines(&dave).is_empty());
    assert_eq!(secret(&dave, &rail), b"pw-bob");
}
