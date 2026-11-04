//! An item shared on its own and the vault it came from, through `App`
//! against the real server (docs/plan/50-received-vs-open.md). The item is
//! held in memory under its vault's id; the vault opening for real (an
//! emergency release, a membership joined at sync, a shared vault opened at
//! unlock) takes its place, so it is listed once and the vault's own copy
//! answers. When the vault goes again, the item comes back as received.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use lockdown_app::{App, FieldChange, GateInput, LockPolicy, Paths};
use lockdown_server::{AppState, Config, RunningServer, run_in_background};

const PASSWORD: &str = "correct horse battery staple";

struct Server {
    running: RunningServer,
    clock: Arc<AtomicI64>,
}

fn server() -> Server {
    let mut state = AppState::in_memory(Config {
        session_ttl: std::time::Duration::from_secs(60 * 86_400),
        per_address: (500, 100.0),
        per_login: (500, 100.0),
        ..Config::default()
    })
    .unwrap();
    let clock = Arc::new(AtomicI64::new(lockdown_vault::Timestamp::now().0));
    let c = clock.clone();
    state.clock = Box::new(move || c.load(Ordering::SeqCst));
    Server {
        running: run_in_background(Arc::new(state)).unwrap(),
        clock,
    }
}

fn person(dir: &std::path::Path, name: &str, srv: &Server) -> App {
    let paths = Paths::resolve(Some(dir.join(name).join("snds/identity")), None).unwrap();
    let app = App::start(paths, None, LockPolicy::default());
    app.open_or_create_plain().unwrap();
    app.account_register(
        &srv.running.url(),
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

/// How many rows `id` has in the list, and the access the first one says.
fn rows_of(app: &App, id: &str) -> (usize, Option<String>) {
    let rows: Vec<_> = app
        .list()
        .unwrap()
        .into_iter()
        .filter(|r| r.id == id)
        .collect();
    (rows.len(), rows.first().and_then(|r| r.access.clone()))
}

/// How many sidebar entries `vault` has, and the kind its badge says.
fn vault_entries(app: &App, vault: &str) -> (usize, Option<String>) {
    let views: Vec<_> = app
        .vaults()
        .unwrap()
        .into_iter()
        .filter(|v| v.id == vault)
        .collect();
    (
        views.len(),
        views
            .first()
            .and_then(|v| v.sharing.as_ref())
            .map(|b| b.kind.clone()),
    )
}

#[test]
fn an_emergency_release_opens_a_vault_an_item_came_from() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server();
    let alice = person(dir.path(), "alice", &srv);
    let bob = person(dir.path(), "bob", &srv);
    let personal = alice.vaults().unwrap()[0].id.clone();
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
    let seed = alice
        .add_item(
            "login",
            Some("Seed account"),
            true,
            vec![
                FieldChange::text("user", "alice"),
                FieldChange::secret("pw", b"seed words"),
            ],
        )
        .unwrap();
    alice.set_legacy_opt_in(&seed, true, pw()).unwrap();
    alice.sync().unwrap();

    // One item of the vault, shared with Bob on its own.
    let b = alice.lookup_contact("bob@example.com").unwrap();
    alice
        .share_item(&mail, "bob@example.com", "view", pw())
        .unwrap();
    bob.sync().unwrap();
    assert_eq!(rows_of(&bob, &mail), (1, Some("view".into())));
    assert_eq!(rows_of(&bob, &shop).0, 0);
    assert_eq!(vault_entries(&bob, &personal), (1, Some("received".into())));

    // Then Bob is named her emergency contact for that vault.
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

    // The wait ran out: the release opens the whole vault, with the crown
    // jewel she opted in, and the shared item is listed once.
    let opened = bob.emergency_open(&grant).unwrap();
    assert_eq!(opened[0].state, "released");
    let check = |bob: &App| {
        assert_eq!(rows_of(bob, &mail), (1, Some("view".into())));
        assert_eq!(rows_of(bob, &shop), (1, Some("view".into())));
        assert_eq!(rows_of(bob, &seed).0, 1, "the opted-in crown jewel");
        assert_eq!(vault_entries(bob, &personal), (1, Some("emergency".into())));
        assert_eq!(
            bob.reveal(&shop, "pw", GateInput::default())
                .unwrap()
                .expose(),
            b"pw-shop"
        );
        assert_eq!(
            bob.reveal(&seed, "pw", pw()).unwrap().expose(),
            b"seed words"
        );
        assert!(bob.trash(&mail).is_err(), "released to read only");
    };
    check(&bob);
    bob.sync().unwrap();
    check(&bob);
    // An unlock opens the released vault again, once, with the item in it;
    // a released crown jewel's key is never saved, so it is back at the
    // next sync, which takes the release again.
    bob.lock().unwrap();
    bob.unlock().unwrap();
    assert_eq!(rows_of(&bob, &mail), (1, Some("view".into())));
    assert_eq!(rows_of(&bob, &shop), (1, Some("view".into())));
    assert_eq!(
        vault_entries(&bob, &personal),
        (1, Some("emergency".into()))
    );
    bob.sync().unwrap();
    check(&bob);

    // Revoked: the vault closes, and the item still shared on its own is
    // back as received, alone.
    alice.emergency_step(&grant, "revoke").unwrap();
    bob.sync().unwrap();
    assert_eq!(rows_of(&bob, &mail), (1, Some("view".into())));
    assert_eq!(rows_of(&bob, &shop).0, 0);
    assert_eq!(rows_of(&bob, &seed).0, 0);
    assert_eq!(vault_entries(&bob, &personal), (1, Some("received".into())));
    assert_eq!(
        bob.reveal(&mail, "pw", GateInput::default())
            .unwrap()
            .expose(),
        b"pw-mail"
    );
}

#[test]
fn a_vault_an_item_came_from_joins_at_sync_and_opens_at_unlock() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server();
    let alice = person(dir.path(), "alice", &srv);
    let bob = person(dir.path(), "bob", &srv);
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
            "note",
            Some("Hotel"),
            false,
            vec![FieldChange::text("body", "Room 12")],
        )
        .unwrap();
    alice.sync().unwrap();

    // One item of the vault, shared with Bob on its own to fill only.
    alice.lookup_contact("bob@example.com").unwrap();
    alice
        .share_item(&rail, "bob@example.com", "fill-only", pw())
        .unwrap();
    bob.sync().unwrap();
    assert_eq!(rows_of(&bob, &rail), (1, Some("fill-only".into())));
    assert_eq!(rows_of(&bob, &hotel).0, 0);
    assert!(bob.reveal(&rail, "pw", GateInput::default()).is_err());

    // Then Bob is added to the vault: his next sync joins it, and the
    // vault's own copy (and right) answers for the item.
    alice
        .share_vault(&trip, "bob@example.com", "view", pw())
        .unwrap();
    bob.sync().unwrap();
    let check = |bob: &App| {
        assert_eq!(rows_of(bob, &rail), (1, Some("view".into())));
        assert_eq!(rows_of(bob, &hotel), (1, Some("view".into())));
        assert_eq!(vault_entries(bob, &trip), (1, Some("shared".into())));
        assert_eq!(
            bob.reveal(&rail, "pw", GateInput::default())
                .unwrap()
                .expose(),
            b"pw-rail"
        );
    };
    check(&bob);

    // A later sync and an unlock open it the same: nothing fails, and the
    // item is still listed once.
    bob.sync().unwrap();
    check(&bob);
    bob.lock().unwrap();
    bob.unlock().unwrap();
    check(&bob);
}
