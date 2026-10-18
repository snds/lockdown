//! Checking a contact's keys with a phrase said aloud, through `App` and
//! the real server's check relay (docs/plan/22-security-depth.md §9): the
//! right phrase verifies both people; a wrong one verifies neither; and an
//! answer can't be retried within the same check.

use std::sync::Arc;

use lockdown_app::{App, LockPolicy, Paths};
use lockdown_server::{AppState, Config, RunningServer, run_in_background};

const PASSWORD: &str = "correct horse battery staple";

fn server() -> RunningServer {
    let state = AppState::in_memory(Config {
        per_address: (500, 100.0),
        per_login: (500, 100.0),
        ..Config::default()
    })
    .unwrap();
    run_in_background(Arc::new(state)).unwrap()
}

fn person(dir: &std::path::Path, name: &str, srv: &RunningServer) -> App {
    let paths = Paths::resolve(Some(dir.join(name).join("snds/identity")), None).unwrap();
    let app = App::start(paths, None, LockPolicy::default());
    app.open_or_create(true).unwrap();
    app.account_register(&srv.url(), &format!("{name}@example.com"), PASSWORD, None)
        .unwrap();
    app.sync().unwrap();
    app
}

/// Both apps poll, as their windows do every second, until neither waits.
fn settle(a: &App, b: &App, id: &str) -> (String, String, Option<String>) {
    for _ in 0..6 {
        let x = a.phrase_poll(id).unwrap();
        let y = b.phrase_poll(id).unwrap();
        if x.state != "waiting" && y.state != "waiting" {
            return (x.state, y.state, x.phrase);
        }
    }
    panic!("the check never settled");
}

fn verified(app: &App, login: &str) -> bool {
    app.sharing_overview()
        .unwrap()
        .contacts
        .iter()
        .any(|c| c.login == login && c.verified)
}

#[test]
fn the_right_phrase_verifies_both_people_and_a_wrong_one_neither() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server();
    let alice = person(dir.path(), "alice", &srv);
    let bob = person(dir.path(), "bob", &srv);

    // A wrong phrase first.
    let check = alice.phrase_invite("bob@example.com").unwrap();
    let said = check.phrase.clone().unwrap();
    assert_eq!(check.state, "waiting");
    let incoming = bob.phrase_incoming().unwrap();
    assert_eq!(incoming.len(), 1);
    assert_eq!(incoming[0].login, "alice@example.com");
    let wrong = if said.starts_with('0') {
        "111 111 111"
    } else {
        "000 000 000"
    };
    bob.phrase_answer(&check.verify_id, wrong).unwrap();
    let (a, b, shown) = settle(&alice, &bob, &check.verify_id);
    assert_eq!((a.as_str(), b.as_str()), ("wrong-phrase", "wrong-phrase"));
    assert!(shown.is_none(), "the phrase is gone once the check ends");
    assert!(
        bob.phrase_answer(&check.verify_id, &said).is_err(),
        "one try per check"
    );
    assert!(!verified(&alice, "bob@example.com") && !verified(&bob, "alice@example.com"));

    // Then the right one.
    let check = alice.phrase_invite("bob@example.com").unwrap();
    let said = check.phrase.clone().unwrap();
    let theirs = bob
        .phrase_incoming()
        .unwrap()
        .into_iter()
        .find(|c| c.verify_id == check.verify_id)
        .unwrap();
    bob.phrase_answer(&theirs.verify_id, &said.replace(' ', "-"))
        .unwrap();
    let (a, b, _) = settle(&alice, &bob, &check.verify_id);
    assert_eq!((a.as_str(), b.as_str()), ("matched", "matched"));
    assert!(verified(&alice, "bob@example.com"));
    assert!(verified(&bob, "alice@example.com"));
    assert_eq!(alice.phrase_checks().unwrap().len(), 2);
}

#[test]
fn a_contact_can_also_be_verified_by_scanning_their_code() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server();
    let alice = person(dir.path(), "alice", &srv);
    let bob = person(dir.path(), "bob", &srv);
    let b = alice.lookup_contact("bob@example.com").unwrap();
    assert!(
        b.post_quantum,
        "bob's identity signs with ML-DSA-65 too, pinned here"
    );
    let scanned = bob.sharing_overview().unwrap().fingerprint_qr;
    assert!(scanned.starts_with("lockdown:contact:v1:"));
    assert!(
        alice
            .verify_contact(&b.account_id, &scanned)
            .unwrap()
            .verified
    );
}
