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
    const HELD: &str = "New members wait until the server's log matches its mirrors again.";
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

    // A new device of Alice's writes, and Carol joins and writes. Bob's copy
    // of Family waits at their heads; the rest of his account keeps syncing.
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
    let own = bob
        .add_login("https://bob.example", "bob", b"pw".to_vec(), None)
        .unwrap();
    let outcome = bob.sync().unwrap();
    assert!(
        outcome.lines.iter().any(|l| l.contains(HELD)),
        "{:?}",
        outcome.lines
    );
    let rows = bob.list().unwrap();
    assert!(rows.iter().any(|r| r.id == own));
    assert!(!rows.iter().any(|r| r.id == carols || r.id == alices));
    assert_eq!(
        bob.vault_sharing(&family).unwrap().held.as_deref(),
        Some(HELD)
    );

    // The split clears: Carol is looked up through the log and added,
    // Alice's new device is checked to continue her pin, and Family syncs.
    bob.set_kt_mirrors(vec![honest_running.url()]).unwrap();
    let outcome = bob.sync().unwrap();
    assert!(
        !outcome.lines.iter().any(|l| l.contains(HELD)),
        "{:?}",
        outcome.lines
    );
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
    // Nothing waits any more.
    let again = bob.sync().unwrap();
    assert!(
        !again.lines.iter().any(|l| l.contains(HELD)),
        "{:?}",
        again.lines
    );
}
