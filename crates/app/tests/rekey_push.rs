//! Re-keyed items reach the server, and the people they stay shared with
//! keep opening them (docs/plan/51-rekey-push.md), through `App` against
//! the real server. Removing a member moves a vault to a new Vault Key and
//! every item to a fresh Item Key; withdrawing one share of an item moves
//! that item. Each re-keyed item gets a re-key version that syncs like an
//! edit, and the shares that stay get the new key only once it is on the
//! server. Before this, the re-keyed items never left the device that
//! re-keyed them, and every other person's copy stopped opening.

use std::sync::Arc;

use lockdown_app::{App, FieldChange, GateInput, LockPolicy, Paths};
use lockdown_server::{AppState, Config, RunningServer, run_in_background};

const PASSWORD: &str = "correct horse battery staple";

fn server() -> RunningServer {
    let state = AppState::in_memory(Config {
        session_ttl: std::time::Duration::from_secs(60 * 86_400),
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
    app.open_or_create_plain().unwrap();
    app.account_register(&srv.url(), &format!("{name}@example.com"), PASSWORD, None)
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

/// How many rows `id` has in the list, and the state the first one says.
fn rows_of(app: &App, id: &str) -> (usize, Option<String>) {
    let rows: Vec<_> = app
        .list()
        .unwrap()
        .into_iter()
        .filter(|r| r.id == id)
        .collect();
    (rows.len(), rows.first().map(|r| r.state.clone()))
}

fn secret(app: &App, id: &str) -> Vec<u8> {
    app.reveal(id, "pw", GateInput::default())
        .unwrap()
        .expose()
        .to_vec()
}

/// Alice's vault "Trip" with a login and a note in it, synced.
fn trip(alice: &App) -> (String, String, String) {
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
    let hotel = alice
        .add_item_in(
            &trip,
            "login",
            Some("Hotel"),
            false,
            vec![
                FieldChange::text("user", "alice"),
                FieldChange::secret("pw", b"pw-hotel"),
            ],
        )
        .unwrap();
    alice.sync().unwrap();
    (trip, rail, hotel)
}

#[test]
fn removing_a_member_keeps_everyone_else_opening_the_vault_and_its_shares() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server();
    let alice = person(dir.path(), "alice", &srv);
    let bob = person(dir.path(), "bob", &srv);
    let carol = person(dir.path(), "carol", &srv);
    let dave = person(dir.path(), "dave", &srv);
    let (trip, rail, hotel) = trip(&alice);
    for who in ["bob", "carol", "dave"] {
        alice.lookup_contact(&login(who)).unwrap();
    }
    let carol_account = alice.lookup_contact(&login("carol")).unwrap().account_id;
    alice
        .share_vault(&trip, &login("bob"), "view", pw())
        .unwrap();
    alice
        .share_vault(&trip, &login("carol"), "view", pw())
        .unwrap();
    // Rail also goes to Dave on its own.
    alice
        .share_item(&rail, &login("dave"), "view", pw())
        .unwrap();
    for app in [&bob, &carol, &dave] {
        app.sync().unwrap();
    }
    assert_eq!(secret(&bob, &hotel), b"pw-hotel");
    assert_eq!(secret(&dave, &rail), b"pw-rail");
    let versions = alice.history(&rail).unwrap().len();
    let edited = alice
        .list()
        .unwrap()
        .into_iter()
        .find(|r| r.id == rail)
        .unwrap()
        .edited_at;

    alice.remove_member(&trip, &carol_account).unwrap();

    // One re-key version per item, keeping the time it was last edited.
    assert_eq!(alice.history(&rail).unwrap().len(), versions + 1);
    let row = alice
        .list()
        .unwrap()
        .into_iter()
        .find(|r| r.id == rail)
        .unwrap();
    assert_eq!(row.edited_at, edited);
    // The member who stays opens every item; so does the person the login
    // was shared with on its own. Carol's copy is gone.
    bob.sync().unwrap();
    assert_eq!(rows_of(&bob, &rail).0, 1);
    assert_eq!(secret(&bob, &rail), b"pw-rail");
    assert_eq!(secret(&bob, &hotel), b"pw-hotel");
    dave.sync().unwrap();
    assert_eq!(rows_of(&dave, &rail).0, 1);
    assert_eq!(rows_of(&dave, &hotel).0, 0);
    assert_eq!(secret(&dave, &rail), b"pw-rail");
    carol.sync().unwrap();
    assert_eq!(rows_of(&carol, &rail).0, 0);
    assert_eq!(rows_of(&carol, &hotel).0, 0);

    // Someone added afterwards holds only the new Vault Key: every item on
    // the server is under it.
    let erin = person(dir.path(), "erin", &srv);
    alice.lookup_contact(&login("erin")).unwrap();
    alice
        .share_vault(&trip, &login("erin"), "view", pw())
        .unwrap();
    erin.sync().unwrap();
    assert_eq!(secret(&erin, &rail), b"pw-rail");
    assert_eq!(secret(&erin, &hotel), b"pw-hotel");

    // Later syncs on every side change nothing and fail nowhere.
    for app in [&alice, &bob, &dave, &erin] {
        app.sync().unwrap();
    }
    assert_eq!(alice.history(&rail).unwrap().len(), versions + 1);
    assert_eq!(secret(&dave, &rail), b"pw-rail");
    // After an unlock too.
    dave.lock().unwrap();
    dave.unlock().unwrap();
    assert_eq!(secret(&dave, &rail), b"pw-rail");
}

#[test]
fn withdrawing_one_share_keeps_the_others_opening() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server();
    let alice = person(dir.path(), "alice", &srv);
    let bob = person(dir.path(), "bob", &srv);
    let carol = person(dir.path(), "carol", &srv);
    let mail = alice
        .add_login(
            "https://mail.example.com",
            "alice",
            b"pw-mail".to_vec(),
            Some("Mail"),
        )
        .unwrap();
    let shop = alice
        .add_login(
            "https://shop.example.com",
            "alice",
            b"pw-shop".to_vec(),
            Some("Shop"),
        )
        .unwrap();
    alice.sync().unwrap();
    alice.lookup_contact(&login("bob")).unwrap();
    alice.lookup_contact(&login("carol")).unwrap();
    alice
        .share_item(&mail, &login("bob"), "view", pw())
        .unwrap();
    alice
        .share_item(&shop, &login("bob"), "view", pw())
        .unwrap();
    let to_carol = alice
        .share_item(&mail, &login("carol"), "view", pw())
        .unwrap()
        .share_id;
    bob.sync().unwrap();
    carol.sync().unwrap();
    assert_eq!(secret(&carol, &mail), b"pw-mail");
    let versions = alice.history(&mail).unwrap().len();

    alice.unshare_item(&to_carol).unwrap();

    assert_eq!(alice.history(&mail).unwrap().len(), versions + 1);
    assert_eq!(alice.history(&shop).unwrap().len(), 1, "only Mail moved");
    bob.sync().unwrap();
    assert_eq!(rows_of(&bob, &mail).0, 1);
    assert_eq!(secret(&bob, &mail), b"pw-mail");
    assert_eq!(secret(&bob, &shop), b"pw-shop");
    carol.sync().unwrap();
    assert_eq!(rows_of(&carol, &mail).0, 0);
    // And again, after more syncs on both sides.
    alice.sync().unwrap();
    bob.sync().unwrap();
    assert_eq!(secret(&bob, &mail), b"pw-mail");
}

#[test]
fn a_recipient_who_syncs_while_a_new_key_waits_still_opens_and_the_rest_still_lists() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server();
    let alice = person(dir.path(), "alice", &srv);
    let bob = person(dir.path(), "bob", &srv);
    let dave = person(dir.path(), "dave", &srv);
    let erin = person(dir.path(), "erin", &srv);
    let (trip, rail, hotel) = trip(&alice);
    for who in ["bob", "dave", "erin"] {
        alice.lookup_contact(&login(who)).unwrap();
    }
    let bob_account = alice.lookup_contact(&login("bob")).unwrap().account_id;
    alice
        .share_vault(&trip, &login("bob"), "view", pw())
        .unwrap();
    // Both items go to Dave and Erin on their own; Dave syncs now, Erin
    // not until after the removal.
    for who in ["dave", "erin"] {
        alice.share_item(&rail, &login(who), "view", pw()).unwrap();
        alice.share_item(&hotel, &login(who), "view", pw()).unwrap();
    }
    bob.sync().unwrap();
    dave.sync().unwrap();
    assert_eq!(secret(&dave, &rail), b"pw-rail");

    // Rail is in the trash when Bob is removed: it moves to the new keys
    // and is pushed, but its shares can't be sealed again until it is
    // restored. Hotel's are, at once.
    alice.trash(&rail).unwrap();
    alice.sync().unwrap();
    alice.remove_member(&trip, &bob_account).unwrap();

    // Dave synced before: the new copy of Rail doesn't open with the key
    // his share still carries, so he keeps the copy he had, and it still
    // opens. Nothing fails.
    dave.sync().unwrap();
    assert_eq!(rows_of(&dave, &rail).0, 1);
    assert_eq!(secret(&dave, &rail), b"pw-rail");
    assert_eq!(secret(&dave, &hotel), b"pw-hotel");
    // Erin's first sync: her share of Rail doesn't open the copy the server
    // serves, and she had nothing of it. It is left out on its own; Hotel
    // lists and opens, and the sync says nothing about it.
    let outcome = erin.sync().unwrap();
    assert!(
        outcome.lines.iter().all(|l| !l.contains("Rail")),
        "{:?}",
        outcome.lines
    );
    assert_eq!(rows_of(&erin, &rail).0, 0);
    assert_eq!(rows_of(&erin, &hotel).0, 1);
    assert_eq!(secret(&erin, &hotel), b"pw-hotel");

    // Restored: the restore is pushed, then Rail's shares get its new key.
    alice.restore(&rail).unwrap();
    alice.sync().unwrap();
    dave.sync().unwrap();
    erin.sync().unwrap();
    for app in [&dave, &erin] {
        assert_eq!(rows_of(app, &rail), (1, Some("live".into())));
        assert_eq!(secret(app, &rail), b"pw-rail");
        assert_eq!(secret(app, &hotel), b"pw-hotel");
    }
}

#[test]
fn a_member_removed_from_a_vault_keeps_an_item_still_shared_on_its_own() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server();
    let alice = person(dir.path(), "alice", &srv);
    let bob = person(dir.path(), "bob", &srv);
    let (trip, rail, hotel) = trip(&alice);
    let bob_account = alice.lookup_contact(&login("bob")).unwrap().account_id;
    // Rail goes to Bob on its own; then the whole vault does.
    alice
        .share_item(&rail, &login("bob"), "view", pw())
        .unwrap();
    alice
        .share_vault(&trip, &login("bob"), "view", pw())
        .unwrap();
    bob.sync().unwrap();
    assert_eq!(rows_of(&bob, &rail).0, 1);
    assert_eq!(rows_of(&bob, &hotel).0, 1);

    // Removed from the vault: it closes on his next sync, and Rail, still
    // shared with him on its own, is back as received under its new key.
    alice.remove_member(&trip, &bob_account).unwrap();
    bob.sync().unwrap();
    assert_eq!(rows_of(&bob, &rail).0, 1);
    assert_eq!(rows_of(&bob, &hotel).0, 0);
    assert_eq!(secret(&bob, &rail), b"pw-rail");
    let entries: Vec<_> = bob
        .vaults()
        .unwrap()
        .into_iter()
        .filter(|v| v.id == trip)
        .collect();
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].sharing.as_ref().map(|b| b.kind.as_str()),
        Some("received")
    );
    bob.sync().unwrap();
    bob.lock().unwrap();
    bob.unlock().unwrap();
    assert_eq!(secret(&bob, &rail), b"pw-rail");
}
