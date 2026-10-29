//! Sharing paused on a split view (D328 as changed by D375;
//! docs/plan/34-small-calls.md §3), end to end through `App` against the
//! real server and two real mirrors in this process: one that copies the
//! server's log honestly, and one whose copy names another log key, so the
//! device sees a split view the way it would in the wild.
//!
//! While the split stands the vault keeps reading and syncing, and every
//! step that trusts the server for someone's keys is refused with a
//! sentence that says what is paused and why: a look-up (and the pin it
//! makes), a phrase check, a vault shared or a household joined, an item
//! shared, share links of an item, of text and of a file, a request link,
//! naming an emergency contact and opening emergency access after its
//! wait. A mirror that can't be reached doesn't lift it. Once the person's
//! mirror agrees again, each of those steps goes through.
//!
//! A shared vault's sync meets nobody new during a split either
//! (docs/plan/35-revoke-split.md §2): a member added meanwhile waits
//! unapplied with a sentence on the vault, items from members already
//! pinned keep arriving, heads signed by the new member or by a known
//! member's new device stop only that vault, and everything that waited
//! applies once the split clears.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use lockdown_app::sharing::SendOptions;
use lockdown_app::{App, AppError, AppResult, GateInput, LockPolicy, Paths};
use lockdown_server::mirror::Mirror;
use lockdown_server::{AppState, Config, RunningServer, run_in_background};

const PASSWORD: &str = "correct horse battery staple";

struct Server {
    running: RunningServer,
    clock: Arc<AtomicI64>,
}

fn server() -> Server {
    let config = Config {
        session_ttl: Duration::from_secs(60 * 86_400),
        // Every gate here checks the password with a sign-in.
        per_login: (1000, 1000.0),
        per_address: (1000, 1000.0),
        ..Config::default()
    };
    let mut state = AppState::in_memory(config).unwrap();
    let clock = Arc::new(AtomicI64::new(lockdown_vault::Timestamp::now().0));
    let c = clock.clone();
    state.clock = Box::new(move || c.load(Ordering::SeqCst));
    Server {
        running: run_in_background(Arc::new(state)).unwrap(),
        clock,
    }
}

fn person(dir: &std::path::Path, name: &str, server: &Server) -> App {
    let paths = Paths::resolve(Some(dir.join(name).join("snds/identity")), None).unwrap();
    let app = App::start(paths, None, LockPolicy::default());
    app.open_or_create(true).unwrap();
    app.account_register(
        &server.running.url(),
        &format!("{name}@example.com"),
        PASSWORD,
        None,
    )
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

fn link() -> SendOptions {
    SendOptions {
        days: 2,
        max_views: Some(2),
    }
}

/// A mirror server copying `upstream`, its state under `dir`.
fn mirror_server(dir: &std::path::Path, upstream: &str) -> (Arc<Mirror>, RunningServer) {
    let state = Arc::new(AppState::in_memory(Config::default()).unwrap());
    let mirror = Arc::new(Mirror::open(upstream, dir, None, Duration::from_secs(60)).unwrap());
    state.mirrors.set_mirror(mirror.clone());
    (mirror, run_in_background(state).unwrap())
}

/// A mirror over a copy of `from`'s state whose pinned log key is another:
/// it vouches for a history this device's server never showed it.
fn split_mirror(from: &std::path::Path, to: &std::path::Path, upstream: &str) -> RunningServer {
    let bytes = std::fs::read(from.join(lockdown_server::mirror::STATE_FILE)).unwrap();
    let mut value: ciborium::Value = ciborium::de::from_reader(&bytes[..]).unwrap();
    let ciborium::Value::Map(map) = &mut value else {
        panic!("the mirror's file is a map")
    };
    let key = ciborium::Value::Text("log_key".into());
    for (k, v) in map.iter_mut() {
        if *k == key
            && let ciborium::Value::Bytes(b) = v
        {
            b[0] ^= 1;
        }
    }
    std::fs::create_dir_all(to).unwrap();
    let mut out = Vec::new();
    ciborium::ser::into_writer(&value, &mut out).unwrap();
    std::fs::write(to.join(lockdown_server::mirror::STATE_FILE), out).unwrap();
    let state = Arc::new(AppState::in_memory(Config::default()).unwrap());
    let mirror = Arc::new(Mirror::open(upstream, to, None, Duration::from_secs(60)).unwrap());
    state.mirrors.set_mirror(mirror);
    run_in_background(state).unwrap()
}

/// `r` was refused as paused, saying which step.
fn paused<T: std::fmt::Debug>(r: AppResult<T>, step: &str) {
    let e: AppError = r.expect_err(step);
    assert!(
        e.message.starts_with(&format!("{step} is paused")),
        "{step}: {}",
        e.message
    );
    assert!(
        e.message.contains("doesn't match a mirror"),
        "{}",
        e.message
    );
    assert!(e.message.contains("still opens and syncs"), "{}", e.message);
}

#[test]
fn a_split_view_pauses_each_new_share_and_the_vault_keeps_syncing() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server();
    let url = srv.running.url();
    let alice = person(dir.path(), "alice", &srv);
    let bob = person(dir.path(), "bob", &srv);
    let personal = alice.vaults().unwrap()[0].id.clone();
    let mail = alice
        .add_login("https://mail.example.com", "alice", b"pw".to_vec(), None)
        .unwrap();
    let work = alice.create_vault("Work").unwrap();
    let papers = alice.create_vault("Papers").unwrap();
    let home = alice.create_household("Home").unwrap();

    // Emergency access made before the split, so its release can be tried
    // during one: Bob agrees, asks, and the wait runs out.
    let b = alice.lookup_contact("bob@example.com").unwrap();
    let bob_own = bob.sharing_overview().unwrap().fingerprint;
    alice.verify_contact(&b.account_id, &bob_own).unwrap();
    let grant = alice
        .add_emergency_contact("bob@example.com", 1, vec![personal.clone()], pw())
        .unwrap()[0]
        .grant_id
        .clone();
    bob.emergency_step(&grant, "accept").unwrap();
    bob.emergency_step(&grant, "request").unwrap();
    srv.clock.fetch_add(2 * 86_400, Ordering::SeqCst);
    alice.sync().unwrap();
    bob.sync().unwrap();

    // Two mirrors of the server's log: one honest, one that saw another
    // history. Both people check against the second.
    let (honest, honest_running) = mirror_server(&dir.path().join("honest"), &url);
    honest
        .poll_once(lockdown_vault::Timestamp::now().0)
        .unwrap();
    let split = split_mirror(&dir.path().join("honest"), &dir.path().join("split"), &url);
    for p in [&alice, &bob] {
        p.set_kt_mirrors(vec![split.url()]).unwrap();
        let outcome = p.sync().unwrap();
        assert!(
            outcome.lines.iter().any(|l| l.contains("Split view")),
            "{:?}",
            outcome.lines
        );
    }

    // The Sharing screen says what is paused and why.
    let banner = alice.sharing_overview().unwrap().paused.expect("paused");
    assert!(banner.starts_with("Sharing is paused"), "{banner}");
    assert!(banner.contains("name an emergency contact"), "{banner}");
    // The vault keeps reading, writing and syncing.
    let note = alice
        .add_login("https://shop.example", "alice", b"pw2".to_vec(), None)
        .unwrap();
    alice.sync().unwrap();
    assert!(alice.list().unwrap().iter().any(|r| r.id == note));

    // Each new share, refused with its own words, before any gate asks.
    let wrong = GateInput {
        password: Some("not the password".into()),
        confirmed: false,
    };
    paused(
        alice.lookup_contact("bob@example.com"),
        "Looking someone up",
    );
    paused(
        alice.phrase_invite("bob@example.com"),
        "Checking someone's keys with a phrase",
    );
    paused(
        alice.share_vault(&work, "bob@example.com", "view", wrong.clone()),
        "Sharing a vault",
    );
    paused(
        alice.add_to_household(&home, "bob@example.com", "edit", wrong.clone()),
        "Sharing a vault",
    );
    paused(
        alice.share_item(&mail, "bob@example.com", "view", wrong.clone()),
        "Sharing an item",
    );
    paused(
        alice.send_item(&mail, None, link(), wrong.clone()),
        "Making a share link",
    );
    paused(
        alice.send_text("Door code", b"4711".to_vec(), None, link()),
        "Making a share link",
    );
    paused(
        alice.send_file("a.txt", "text/plain", b"hello".to_vec(), None, link()),
        "Making a share link",
    );
    paused(
        alice.create_request("The Wi-Fi password", 3),
        "Making a request link",
    );
    paused(
        alice.add_emergency_contact("bob@example.com", 2, vec![papers.clone()], wrong),
        "Naming an emergency contact",
    );
    paused(bob.emergency_open(&grant), "Opening emergency access");

    // A split mirror that can't be reached any more doesn't lift it.
    drop(split);
    alice.sync().unwrap();
    paused(
        alice.create_request("The Wi-Fi password", 3),
        "Making a request link",
    );
    assert!(alice.sharing_overview().unwrap().paused.is_some());

    // The split clears when the person's mirror agrees: each step goes through.
    for p in [&alice, &bob] {
        p.set_kt_mirrors(vec![honest_running.url()]).unwrap();
        p.sync().unwrap();
        assert!(p.sharing_overview().unwrap().paused.is_none());
    }
    let b = alice.lookup_contact("bob@example.com").unwrap();
    assert_eq!(b.login, "bob@example.com");
    alice.phrase_invite("bob@example.com").unwrap();
    alice
        .share_vault(&work, "bob@example.com", "view", pw())
        .unwrap();
    alice
        .add_to_household(&home, "bob@example.com", "edit", pw())
        .unwrap();
    alice
        .share_item(&mail, "bob@example.com", "view", pw())
        .unwrap();
    alice.send_item(&mail, None, link(), pw()).unwrap();
    alice
        .send_text("Door code", b"4711".to_vec(), None, link())
        .unwrap();
    alice
        .send_file("a.txt", "text/plain", b"hello".to_vec(), None, link())
        .unwrap();
    alice.create_request("The Wi-Fi password", 3).unwrap();
    let grants = alice
        .add_emergency_contact("bob@example.com", 2, vec![papers], pw())
        .unwrap();
    assert_eq!(grants.len(), 2);
    let opened = bob.emergency_open(&grant).unwrap();
    assert_eq!(
        opened
            .iter()
            .find(|g| g.grant_id == grant)
            .map(|g| g.state.as_str()),
        Some("released")
    );
    assert!(bob.list().unwrap().iter().any(|r| r.id == mail));
}

/// What a shared vault says while new members wait (D423).
const HELD: &str = "New members wait until the server's log matches its mirrors again.";
/// What it says once a head by someone unmet stopped it.
const HELD_AFTER: &str = "New members wait until the server's log matches its mirrors again. Changes made after a new member's or a new device's wait too, even from members you already have.";
/// The sync's line while a vault is stopped at such a head.
const HELD_AFTER_LINE: &str = "A shared vault has changes from people this device hasn't met yet. They and every change made after them wait until the server's log matches its mirrors again, even from members you already have. Your other vaults keep syncing.";
/// The sync's line while items shared on their own wait.
const HELD_ITEMS_LINE: &str = "Items shared with you by people this device hasn't met yet wait until the server's log matches its mirrors again. Items from people you already have keep syncing.";

/// Whether a sync's lines say anything waits for the split to clear.
fn waiting(lines: &[String]) -> bool {
    lines
        .iter()
        .any(|l| l.contains("wait until the server's log matches its mirrors again"))
}

fn secret_key_of(page: &[u8]) -> String {
    std::str::from_utf8(page)
        .unwrap()
        .lines()
        .find_map(|l| l.strip_prefix("Secret Key:"))
        .map(|v| v.trim().to_string())
        .expect("the kit page names the Secret Key")
}

#[test]
fn during_a_split_new_members_wait_and_known_members_items_still_sync() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server();
    let url = srv.running.url();
    // Alice keeps her kit: a second device of hers joins with it later.
    let alice = App::start(
        Paths::resolve(Some(dir.path().join("alice/snds/identity")), None).unwrap(),
        None,
        LockPolicy::default(),
    );
    alice.open_or_create(true).unwrap();
    let registered = alice
        .account_register(&url, "alice@example.com", PASSWORD, None)
        .unwrap();
    let secret_key = secret_key_of(registered.kit.as_ref().unwrap().expose());
    alice.sync().unwrap();
    let bob = person(dir.path(), "bob", &srv);
    let carol = person(dir.path(), "carol", &srv);

    // Alice shares Family with Bob, who may edit it.
    let family = alice.create_vault("Family").unwrap();
    let first = alice
        .add_item_in(&family, "note", Some("Door code"), false, Vec::new())
        .unwrap();
    alice.lookup_contact("bob@example.com").unwrap();
    alice
        .share_vault(&family, "bob@example.com", "edit", pw())
        .unwrap();
    alice.sync().unwrap();
    bob.sync().unwrap();
    assert!(bob.list().unwrap().iter().any(|r| r.id == first));

    // Bob's view of the server's log splits from a mirror; Alice's doesn't.
    let (honest, honest_running) = mirror_server(&dir.path().join("honest"), &url);
    honest
        .poll_once(lockdown_vault::Timestamp::now().0)
        .unwrap();
    let split = split_mirror(&dir.path().join("honest"), &dir.path().join("split"), &url);
    bob.set_kt_mirrors(vec![split.url()]).unwrap();
    let outcome = bob.sync().unwrap();
    assert!(
        outcome.lines.iter().any(|l| l.contains("Split view")),
        "{:?}",
        outcome.lines
    );

    // Meanwhile Alice adds Carol, and writes another item.
    alice.lookup_contact("carol@example.com").unwrap();
    alice
        .share_vault(&family, "carol@example.com", "edit", pw())
        .unwrap();
    let second = alice
        .add_item_in(&family, "note", Some("Alarm code"), false, Vec::new())
        .unwrap();
    alice.sync().unwrap();

    // Bob's sync holds Carol back, says so on the vault, and still takes
    // Alice's item.
    let outcome = bob.sync().unwrap();
    assert!(
        outcome.lines.iter().any(|l| l.contains(HELD)),
        "{:?}",
        outcome.lines
    );
    assert!(bob.list().unwrap().iter().any(|r| r.id == second));
    let view = bob.vault_sharing(&family).unwrap();
    assert_eq!(view.held.as_deref(), Some(HELD));
    assert!(
        view.members.iter().all(|m| m.login != "carol@example.com"),
        "{:?}",
        view.members
    );
    assert!(
        bob.sharing_overview()
            .unwrap()
            .contacts
            .iter()
            .all(|c| c.login != "carol@example.com"),
        "Carol isn't pinned during the split"
    );
    // Alice, whose view isn't split, sees Carol in it.
    assert!(
        alice
            .vault_sharing(&family)
            .unwrap()
            .members
            .iter()
            .any(|m| m.login == "carol@example.com")
    );
    assert!(alice.vault_sharing(&family).unwrap().held.is_none());

    // A new device of Alice's writes, Carol joins and writes, and then Alice
    // writes once more. Bob's copy of Family waits at the first of their
    // heads, and so does Alice's later one: heads chain hash to hash and
    // hers lists Carol's note (docs/plan/36-shared-sync.md §3). The rest of
    // his account keeps syncing.
    let alice2 = App::start(
        Paths::resolve(Some(dir.path().join("alice2/snds/identity")), None).unwrap(),
        None,
        LockPolicy::default(),
    );
    alice2
        .account_login(&url, "alice@example.com", PASSWORD, Some(&secret_key))
        .unwrap();
    alice2.open_or_create(true).unwrap();
    alice2.sync().unwrap();
    let alices = alice2
        .add_item_in(&family, "note", Some("From the tablet"), false, Vec::new())
        .unwrap();
    alice2.sync().unwrap();
    carol.sync().unwrap();
    let carols = carol
        .add_item_in(&family, "note", Some("Carol's note"), false, Vec::new())
        .unwrap();
    carol.sync().unwrap();
    alice.sync().unwrap();
    let after = alice
        .add_item_in(&family, "note", Some("After Carol"), false, Vec::new())
        .unwrap();
    alice.sync().unwrap();
    let own = bob
        .add_login("https://bob.example", "bob", b"pw".to_vec(), None)
        .unwrap();
    let outcome = bob.sync().unwrap();
    assert!(
        outcome.lines.iter().any(|l| l.contains(HELD_AFTER_LINE)),
        "{:?}",
        outcome.lines
    );
    let rows = bob.list().unwrap();
    assert!(rows.iter().any(|r| r.id == own));
    assert!(
        !rows
            .iter()
            .any(|r| r.id == carols || r.id == alices || r.id == after)
    );
    assert_eq!(
        bob.vault_sharing(&family).unwrap().held.as_deref(),
        Some(HELD_AFTER)
    );

    // The split clears: Carol is looked up through the log and added,
    // Alice's new device is checked to continue her pin, and Family syncs.
    bob.set_kt_mirrors(vec![honest_running.url()]).unwrap();
    let outcome = bob.sync().unwrap();
    assert!(!waiting(&outcome.lines), "{:?}", outcome.lines);
    let view = bob.vault_sharing(&family).unwrap();
    assert!(view.held.is_none());
    assert!(
        view.members.iter().any(|m| m.login == "carol@example.com"),
        "{:?}",
        view.members
    );
    let rows = bob.list().unwrap();
    assert!(rows.iter().any(|r| r.id == carols));
    assert!(rows.iter().any(|r| r.id == alices));
    assert!(rows.iter().any(|r| r.id == after));
    // Nothing waits any more.
    let again = bob.sync().unwrap();
    assert!(
        !again.lines.iter().any(|l| l.contains(HELD)),
        "{:?}",
        again.lines
    );
}

/// Alice with her kit kept, so a second device of hers can join through
/// the account with the Secret Key.
fn alice_with_kit(dir: &std::path::Path, url: &str) -> (App, String) {
    let alice = App::start(
        Paths::resolve(Some(dir.join("alice/snds/identity")), None).unwrap(),
        None,
        LockPolicy::default(),
    );
    alice.open_or_create(true).unwrap();
    let registered = alice
        .account_register(url, "alice@example.com", PASSWORD, None)
        .unwrap();
    let secret_key = secret_key_of(registered.kit.as_ref().unwrap().expose());
    alice.sync().unwrap();
    (alice, secret_key)
}

/// A new device of Alice's, enrolled through the account with the Secret
/// Key, after its first pull.
fn alices_new_device(dir: &std::path::Path, name: &str, url: &str, secret_key: &str) -> App {
    let app = App::start(
        Paths::resolve(Some(dir.join(name).join("snds/identity")), None).unwrap(),
        None,
        LockPolicy::default(),
    );
    app.account_login(url, "alice@example.com", PASSWORD, Some(secret_key))
        .unwrap();
    app.open_or_create(true).unwrap();
    app
}

fn has(app: &App, id: &str) -> bool {
    app.list().unwrap().iter().any(|r| r.id == id)
}

fn pinned(app: &App, login: &str) -> bool {
    app.sharing_overview()
        .unwrap()
        .contacts
        .iter()
        .any(|c| c.login == login)
}

#[test]
fn an_owners_new_device_pulls_a_shared_vault_others_already_wrote_to() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server();
    let url = srv.running.url();
    let (alice, secret_key) = alice_with_kit(dir.path(), &url);
    let bob = person(dir.path(), "bob", &srv);
    let carol = person(dir.path(), "carol", &srv);

    // Family, shared with Bob and Carol, who both write to it.
    let family = alice.create_vault("Family").unwrap();
    let first = alice
        .add_item_in(&family, "note", Some("Door code"), false, Vec::new())
        .unwrap();
    for who in ["bob@example.com", "carol@example.com"] {
        alice.lookup_contact(who).unwrap();
        alice.share_vault(&family, who, "edit", pw()).unwrap();
    }
    alice.sync().unwrap();
    bob.sync().unwrap();
    let bobs = bob
        .add_item_in(&family, "note", Some("Bob's note"), false, Vec::new())
        .unwrap();
    bob.sync().unwrap();
    carol.sync().unwrap();
    let carols = carol
        .add_item_in(&family, "note", Some("Carol's note"), false, Vec::new())
        .unwrap();
    carol.sync().unwrap();
    alice.sync().unwrap();
    assert!(has(&alice, &bobs) && has(&alice, &carols));

    // Alice's tablet joins through the account after both wrote: it looks
    // Family's writers up, checks them against the log, and pulls it all.
    let tablet = alices_new_device(dir.path(), "tablet", &url, &secret_key);
    tablet.sync().unwrap();
    for id in [&first, &bobs, &carols] {
        assert!(has(&tablet, id), "the tablet has {id}");
    }
    assert!(pinned(&tablet, "bob@example.com"));
    assert!(pinned(&tablet, "carol@example.com"));
    let view = tablet.vault_sharing(&family).unwrap();
    assert!(view.held.is_none());
    for who in ["bob@example.com", "carol@example.com"] {
        assert!(view.members.iter().any(|m| m.login == who), "{who}");
    }

    // The tablet writes; Bob takes its head (Alice's new device, checked
    // to continue her pin), and the tablet takes his next one.
    let tablets = tablet
        .add_item_in(&family, "note", Some("From the tablet"), false, Vec::new())
        .unwrap();
    tablet.sync().unwrap();
    bob.sync().unwrap();
    assert!(has(&bob, &tablets));
    let later = bob
        .add_item_in(&family, "note", Some("Bob again"), false, Vec::new())
        .unwrap();
    bob.sync().unwrap();
    tablet.sync().unwrap();
    assert!(has(&tablet, &later));

    // Locked and opened again, both of Alice's devices open Family as
    // before: its other writers are trusted from the pins, not the server.
    for app in [&alice, &tablet] {
        app.sync().unwrap();
        app.lock().unwrap();
        app.unlock().unwrap();
        for id in [&first, &bobs, &carols, &tablets, &later] {
            assert!(has(app, id), "{id} after unlocking");
        }
        app.sync().unwrap();
    }
}

#[test]
fn during_a_split_an_owners_new_device_holds_a_vault_whose_writers_it_hasnt_met() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server();
    let url = srv.running.url();
    let (alice, secret_key) = alice_with_kit(dir.path(), &url);
    let bob = person(dir.path(), "bob", &srv);

    // Alice's tablet joins before anything is shared, and its view of the
    // server's log splits from a mirror.
    let tablet = alices_new_device(dir.path(), "tablet", &url, &secret_key);
    tablet.sync().unwrap();
    let (honest, honest_running) = mirror_server(&dir.path().join("honest"), &url);
    honest
        .poll_once(lockdown_vault::Timestamp::now().0)
        .unwrap();
    let split = split_mirror(&dir.path().join("honest"), &dir.path().join("split"), &url);
    tablet.set_kt_mirrors(vec![split.url()]).unwrap();
    let outcome = tablet.sync().unwrap();
    assert!(
        outcome.lines.iter().any(|l| l.contains("Split view")),
        "{:?}",
        outcome.lines
    );

    // Meanwhile Alice shares Trip with Bob, who writes to it.
    let trip = alice.create_vault("Trip").unwrap();
    let first = alice
        .add_item_in(&trip, "note", Some("Hotel"), false, Vec::new())
        .unwrap();
    alice.lookup_contact("bob@example.com").unwrap();
    alice
        .share_vault(&trip, "bob@example.com", "edit", pw())
        .unwrap();
    alice.sync().unwrap();
    bob.sync().unwrap();
    let bobs = bob
        .add_item_in(&trip, "note", Some("Flights"), false, Vec::new())
        .unwrap();
    bob.sync().unwrap();

    // The tablet pulls Trip but meets nobody during the split: Trip waits
    // unopened and the sync says so; the rest of the account syncs.
    let own = tablet
        .add_login("https://tablet.example", "alice", b"pw".to_vec(), None)
        .unwrap();
    let outcome = tablet.sync().unwrap();
    assert!(
        outcome.lines.iter().any(|l| l.contains(HELD_AFTER_LINE)),
        "{:?}",
        outcome.lines
    );
    assert!(has(&tablet, &own));
    assert!(!has(&tablet, &first) && !has(&tablet, &bobs));
    assert!(!tablet.vaults().unwrap().iter().any(|v| v.id == trip));
    assert!(!pinned(&tablet, "bob@example.com"));
    alice.sync().unwrap();
    assert!(has(&alice, &own), "the tablet's own write reached Alice");

    // Locked and opened during the split, the tablet opens; Trip still waits.
    tablet.lock().unwrap();
    tablet.unlock().unwrap();
    assert!(has(&tablet, &own) && !has(&tablet, &bobs));
    let outcome = tablet.sync().unwrap();
    assert!(waiting(&outcome.lines), "{:?}", outcome.lines);

    // The split clears: Bob is looked up through the log and pinned, and
    // Trip opens with everything in it.
    tablet.set_kt_mirrors(vec![honest_running.url()]).unwrap();
    let outcome = tablet.sync().unwrap();
    assert!(!waiting(&outcome.lines), "{:?}", outcome.lines);
    assert!(has(&tablet, &first) && has(&tablet, &bobs));
    assert!(pinned(&tablet, "bob@example.com"));
    let view = tablet.vault_sharing(&trip).unwrap();
    assert!(view.held.is_none());
    assert!(view.members.iter().any(|m| m.login == "bob@example.com"));
}

#[test]
fn during_a_split_items_shared_by_someone_unmet_wait_and_known_owners_items_sync() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server();
    let url = srv.running.url();
    let (alice, secret_key) = alice_with_kit(dir.path(), &url);
    let bob = person(dir.path(), "bob", &srv);
    let carol = person(dir.path(), "carol", &srv);

    // Alice shares a login with Bob before anything splits: Bob pins her.
    let mail = alice
        .add_login("https://mail.example.com", "alice", b"pw".to_vec(), None)
        .unwrap();
    alice.sync().unwrap();
    alice.lookup_contact("bob@example.com").unwrap();
    alice
        .share_item(&mail, "bob@example.com", "view", pw())
        .unwrap();
    bob.sync().unwrap();
    assert!(has(&bob, &mail));
    assert!(pinned(&bob, "alice@example.com"));
    // Alice's tablet joins after Bob pinned her: he doesn't know it yet.
    let tablet = alices_new_device(dir.path(), "tablet", &url, &secret_key);
    tablet.sync().unwrap();

    // Bob's view of the server's log splits from a mirror.
    let (honest, honest_running) = mirror_server(&dir.path().join("honest"), &url);
    honest
        .poll_once(lockdown_vault::Timestamp::now().0)
        .unwrap();
    let split = split_mirror(&dir.path().join("honest"), &dir.path().join("split"), &url);
    bob.set_kt_mirrors(vec![split.url()]).unwrap();
    let outcome = bob.sync().unwrap();
    assert!(
        outcome.lines.iter().any(|l| l.contains("Split view")),
        "{:?}",
        outcome.lines
    );

    // Alice shares another login; her tablet shares one of its own; Carol,
    // whom Bob never met, shares one.
    let shop = alice
        .add_login("https://shop.example", "alice", b"pw2".to_vec(), None)
        .unwrap();
    alice.sync().unwrap();
    alice
        .share_item(&shop, "bob@example.com", "view", pw())
        .unwrap();
    let bank = tablet
        .add_login("https://bank.example", "alice", b"pw4".to_vec(), None)
        .unwrap();
    tablet.sync().unwrap();
    tablet.lookup_contact("bob@example.com").unwrap();
    tablet
        .share_item(&bank, "bob@example.com", "view", pw())
        .unwrap();
    let wifi = carol
        .add_login("https://router.example", "admin", b"pw3".to_vec(), None)
        .unwrap();
    carol.sync().unwrap();
    carol.lookup_contact("bob@example.com").unwrap();
    carol
        .share_item(&wifi, "bob@example.com", "view", pw())
        .unwrap();

    // Bob takes Alice's from the device he knows; Carol's, and the one
    // signed by Alice's tablet, wait unapplied with the sentence. Carol
    // isn't pinned.
    let outcome = bob.sync().unwrap();
    assert!(
        outcome.lines.iter().any(|l| l == HELD_ITEMS_LINE),
        "{:?}",
        outcome.lines
    );
    assert!(has(&bob, &mail) && has(&bob, &shop));
    assert!(!has(&bob, &wifi) && !has(&bob, &bank));
    assert!(!pinned(&bob, "carol@example.com"));

    // The split clears: Carol is looked up through the log and pinned,
    // Alice is looked up again and continues her pin with the tablet, and
    // both waiting items arrive in the same sync.
    bob.set_kt_mirrors(vec![honest_running.url()]).unwrap();
    let outcome = bob.sync().unwrap();
    assert!(!waiting(&outcome.lines), "{:?}", outcome.lines);
    for id in [&mail, &shop, &bank, &wifi] {
        assert!(has(&bob, id), "{id}");
    }
    assert!(pinned(&bob, "carol@example.com"));
}
