//! Recovery depth through `App` against the real server
//! (docs/plan/22-security-depth.md §5–§8): the first run's kit test and the
//! yearly drill with its date on the profile; the kit's and the account's
//! password stretching rising on a device that can afford more; the
//! password reminder; and crown jewels in emergency access, released only
//! when their owner opted each in.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use lockdown_app::{App, ErrorCode, FieldChange, GateInput, LockPolicy, Paths};
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

fn paths(dir: &std::path::Path, name: &str) -> Paths {
    Paths::resolve(Some(dir.join(name).join("snds/identity")), None).unwrap()
}

/// A person with an account; returns the app and the kit's Secret Key and
/// recovery code, read off the page the registration showed.
fn person(dir: &std::path::Path, name: &str, srv: &Server) -> (App, String, String) {
    let app = App::start(paths(dir, name), None, LockPolicy::default());
    app.open_or_create(true).unwrap();
    let reg = app
        .account_register(
            &srv.running.url(),
            &format!("{name}@example.com"),
            PASSWORD,
            None,
        )
        .unwrap();
    let page = String::from_utf8(reg.kit.unwrap().expose().to_vec()).unwrap();
    let line = |label: &str| {
        page.lines()
            .find_map(|l| l.strip_prefix(label))
            .unwrap()
            .trim()
            .to_string()
    };
    let (sk, code) = (line("Secret Key:"), line("Recovery code:"));
    app.sync().unwrap();
    (app, sk, code)
}

fn pw() -> GateInput {
    GateInput {
        password: Some(PASSWORD.into()),
        confirmed: false,
    }
}

#[test]
fn the_first_run_ends_with_the_kit_test_and_the_drill_dates_the_profile() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server();
    let (app, sk, code) = person(dir.path(), "me", &srv);
    let r = app.reminders().unwrap();
    assert_eq!(r.drill.kit, "created");
    assert!(
        r.drill.kit_test_pending,
        "the test is the first run's last step"
    );
    assert!(r.drill.due);
    // Skipping needs an explicit yes.
    let refused = app.kit_test_skip(false).unwrap_err();
    assert_eq!(refused.detail.as_deref(), Some("confirm_skip"));

    let before = app.status().unwrap();
    let lines = app.kit_verify(&sk, &code, Some(PASSWORD), true).unwrap();
    assert!(
        lines.iter().any(|l| l.contains("drill recorded")),
        "{lines:?}"
    );
    assert!(
        !lines.iter().any(|l| l.contains("stretching rose")),
        "the kit was made under this device's own calibration: {lines:?}"
    );
    let r = app.reminders().unwrap();
    assert_eq!(r.drill.kit, "drilled");
    assert!(!r.drill.due && !r.drill.kit_test_pending);
    let last = r.drill.last_drilled_at.unwrap();
    assert!(r.drill.next_at.unwrap() >= last + 364 * 86_400);
    assert!(app.status().unwrap().identity_seq > before.identity_seq);

    // The kit still opens the identity under the raised stretching: a new
    // device recovers with it.
    let fresh = App::start(paths(dir.path(), "fresh"), None, LockPolicy::default());
    fresh
        .recover(&srv.running.url(), "me@example.com", PASSWORD, &sk, &code)
        .unwrap();
    fresh.open_or_create(true).unwrap();
    let r2 = fresh.reminders().unwrap();
    assert_eq!(
        r2.drill.last_drilled_at,
        Some(last),
        "every device knows the date"
    );

    // The checks look past the kit.
    let checks = app.drill_checks().unwrap();
    assert!(checks.iter().any(|c| c.what == "kit" && c.ok));
    assert!(checks.iter().any(|c| c.what == "secret-key" && c.ok));
    assert!(checks.iter().any(|c| c.what == "contact"));
}

#[test]
fn a_stronger_device_raises_the_accounts_stretching_and_others_follow() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server();
    let (app, sk, _) = person(dir.path(), "me", &srv);
    // A sign-in here raises what the account's password file is made under.
    let lines = app
        .account_login(&srv.running.url(), "me@example.com", PASSWORD, None)
        .unwrap();
    assert!(
        lines.iter().any(|l| l.contains("password stretching rose")),
        "{lines:?}"
    );
    // Nothing more to raise the second time.
    let again = app
        .account_login(&srv.running.url(), "me@example.com", PASSWORD, None)
        .unwrap();
    assert!(!again.iter().any(|l| l.contains("rose")), "{again:?}");
    // The password gate still checks out, and a new device enrols with the
    // password and the Secret Key under the raised set the server announces.
    app.approve(lockdown_app::ApprovalKind::Reveal, "test", pw())
        .unwrap();
    let phone = App::start(paths(dir.path(), "phone"), None, LockPolicy::default());
    phone
        .account_login(&srv.running.url(), "me@example.com", PASSWORD, Some(&sk))
        .unwrap();
    phone.open_or_create(true).unwrap();
    assert!(phone.status().unwrap().account.is_some());
}

#[test]
fn the_password_reminder_counts_a_typed_password() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server();
    let (app, _, _) = person(dir.path(), "me", &srv);
    let r = app.reminders().unwrap();
    assert!(r.password.applies && !r.password.due);
    assert_eq!(r.password.every_days, 14);
    assert!(app.set_password_reminder_days(9).is_err());
    assert_eq!(
        app.set_password_reminder_days(7)
            .unwrap()
            .password
            .every_days,
        7
    );
    assert!(app.password_check("not it").is_err());
    let checked = app.password_check(PASSWORD).unwrap();
    assert!(checked.password.last_at.is_some());
    assert!(
        app.password_snooze().unwrap().password.next_at.unwrap()
            > checked.password.last_at.unwrap()
    );
    // Where every unlock is the password (the web vault), it doesn't apply.
    app.set_unlock_asks_password(true).unwrap();
    assert!(!app.reminders().unwrap().password.applies);
}

#[test]
fn a_crown_jewel_reaches_an_emergency_contact_only_when_opted_in() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server();
    let (alice, _, _) = person(dir.path(), "alice", &srv);
    let (bob, _, _) = person(dir.path(), "bob", &srv);
    let personal = alice.vaults().unwrap()[0].id.clone();
    let everyday = alice
        .add_login(
            "https://shop.example",
            "alice",
            b"shop".to_vec(),
            Some("Shop"),
        )
        .unwrap();
    let wallet = |title: &str| {
        alice
            .add_item(
                "login",
                Some(title),
                true,
                vec![
                    FieldChange::text("user", "alice"),
                    FieldChange::secret("pw", title.as_bytes()),
                ],
            )
            .unwrap()
    };
    let kept_back = wallet("Exchange");
    let released = wallet("Seed account");
    // Opting in lets the secret leave, so it asks for the password.
    assert_eq!(
        alice
            .set_legacy_opt_in(&released, true, GateInput::default())
            .unwrap_err()
            .code,
        ErrorCode::ApprovalNeeded
    );
    alice.set_legacy_opt_in(&released, true, pw()).unwrap();
    assert!(alice.legacy_opt_in(&released).unwrap());
    assert!(!alice.legacy_opt_in(&kept_back).unwrap());
    alice.sync().unwrap();

    let b = alice.lookup_contact("bob@example.com").unwrap();
    let bob_own = bob.sharing_overview().unwrap().fingerprint;
    alice.verify_contact(&b.account_id, &bob_own).unwrap();
    let grant = alice
        .add_emergency_contact("bob@example.com", 1, vec![personal], pw())
        .unwrap()[0]
        .grant_id
        .clone();
    let checks = alice.drill_checks().unwrap();
    assert!(
        checks
            .iter()
            .any(|c| c.what == "crown-jewels" && c.words.contains("Seed account")),
        "{checks:?}"
    );
    bob.emergency_step(&grant, "accept").unwrap();
    bob.emergency_step(&grant, "request").unwrap();
    srv.clock.fetch_add(2 * 86_400, Ordering::SeqCst);
    bob.emergency_open(&grant).unwrap();
    let rows = bob.list().unwrap();
    assert!(rows.iter().any(|r| r.id == everyday));
    assert!(rows.iter().any(|r| r.id == released), "opted in: released");
    assert!(
        !rows.iter().any(|r| r.id == kept_back),
        "not opted in: never leaves alice"
    );
    assert_eq!(
        bob.reveal(&released, "pw", pw()).unwrap().expose(),
        b"Seed account"
    );
    // A later sync keeps the released one and still never the other.
    bob.sync().unwrap();
    let rows = bob.list().unwrap();
    assert!(rows.iter().any(|r| r.id == released));
    assert!(!rows.iter().any(|r| r.id == kept_back));
}
