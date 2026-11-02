//! Device approval end to end through `App` against the real server
//! (docs/plan/22-security-depth.md §2–§3): a laptop with the account opens
//! a link; an empty phone joins with the typed code; both show the same six
//! digits; the laptop approves through the `Approve` gate; the phone signs
//! in with the password only (no kit), finds the vault, keeps the Secret
//! Key the laptop handed on after checking it against the account slot,
//! and syncs both ways. A declined link writes nothing, a wrong QR stops
//! the join, and an account made without a Secret Key says so.
//!
//! The phone joins sealed, as the desktop does
//! (docs/plan/45-desktop-join-sealed.md): nothing is written before the
//! password is proved, a wrong one or a cancelled join leaves the phone
//! empty, and the finished phone holds only the sealed key file and opens
//! with the password.

use std::sync::Arc;

use lockdown_app::local_unlock::{self, KeyFile};
use lockdown_app::{App, ErrorCode, GateInput, LockPolicy, Paths};
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

fn paths(dir: &std::path::Path, name: &str) -> Paths {
    Paths::resolve(Some(dir.join(name).join("snds/identity")), None).unwrap()
}

fn laptop(dir: &std::path::Path, server: &RunningServer) -> App {
    let app = App::start(paths(dir, "laptop"), None, LockPolicy::default());
    app.open_or_create_plain().unwrap();
    app.account_register(&server.url(), "me@example.com", PASSWORD, None)
        .unwrap();
    app.sync().unwrap();
    app
}

/// The files in the phone's data folder, sorted.
fn files(app: &App) -> Vec<String> {
    let data = std::path::PathBuf::from(app.probe().unwrap().data);
    let mut names: Vec<String> = std::fs::read_dir(&data)
        .map(|d| {
            d.filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

fn data_of(app: &App) -> std::path::PathBuf {
    std::path::PathBuf::from(app.probe().unwrap().data)
}

fn pw() -> GateInput {
    GateInput {
        password: Some(PASSWORD.into()),
        confirmed: false,
    }
}

#[test]
fn a_new_device_joins_from_an_enrolled_one_without_the_kit() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server();
    let laptop = laptop(dir.path(), &srv);
    let mail = laptop
        .add_login(
            "https://mail.example.com",
            "me",
            b"pw-one".to_vec(),
            Some("Mail"),
        )
        .unwrap();
    laptop.sync().unwrap();
    // The laptop keeps the Secret Key the account was registered with.
    assert_eq!(laptop.secret_key_status().unwrap().state, "kept");

    let offered = laptop.link_offer().unwrap();
    assert_eq!(offered.state, "waiting");
    assert_eq!(offered.code.len(), 11);
    assert!(offered.qr.starts_with("lockdown:link:v1?"));

    let phone = App::start(paths(dir.path(), "phone"), None, LockPolicy::default());
    let joining = phone.join_start_sealed(&srv.url(), &offered.code).unwrap();
    assert_eq!(joining.state, "waiting");
    let asked = laptop.link_poll().unwrap();
    assert_eq!(asked.state, "compare");
    let shown = phone.join_poll().unwrap();
    assert_eq!(shown.state, "compare");
    assert_eq!(shown.sas, asked.sas, "both screens show the same digits");

    // The approval is gated: nothing happens without the password here.
    let gated = laptop.link_approve(GateInput::default()).unwrap_err();
    assert_eq!(gated.code, ErrorCode::ApprovalNeeded);
    let lines = laptop.link_approve(pw()).unwrap();
    assert!(lines[0].contains("is now one of your devices"), "{lines:?}");
    assert!(lines[1].contains("Secret Key went with it"), "{lines:?}");

    let approved = phone.join_poll().unwrap();
    assert_eq!(approved.state, "approved");
    assert_eq!(approved.secret_key.as_deref(), Some("sent"));
    // Approved, and still nothing on the phone: the keys and the identity
    // wait in memory for the password.
    assert!(files(&phone).is_empty(), "{:?}", files(&phone));
    assert!(!phone.probe().unwrap().identity_present);
    // A wrong password writes nothing, and the join stays.
    let wrong = phone
        .join_finish("me@example.com", "not the password at all")
        .unwrap_err();
    assert_eq!(wrong.code, ErrorCode::Refused);
    assert!(files(&phone).is_empty(), "{:?}", files(&phone));
    // The password alone finishes: no Secret Key typed, no recovery code.
    let status = phone.join_finish("me@example.com", PASSWORD).unwrap();
    assert!(status.account.is_some());
    // Only the sealed key file, never the plain one; the lock screen's
    // name is the login.
    assert_eq!(local_unlock::key_file(&data_of(&phone)), KeyFile::Sealed);
    assert!(!data_of(&phone).join(local_unlock::PLAIN_FILE).exists());
    let probe = phone.probe().unwrap();
    assert!(probe.master_password && probe.identity_present);
    assert_eq!(probe.name.as_deref(), Some("me@example.com"));
    let items = phone.list().unwrap();
    assert!(items.iter().any(|i| i.id == mail), "the vault came over");
    assert_eq!(phone.secret_key_status().unwrap().state, "kept");
    let devices = laptop.device_list().unwrap();
    assert_eq!(devices.len(), 2);

    // Both ways: a change on the phone reaches the laptop, audits clean.
    let wifi = phone
        .add_login(
            "https://shop.example",
            "me",
            b"pw-two".to_vec(),
            Some("Shop"),
        )
        .unwrap();
    phone.sync().unwrap();
    laptop.sync().unwrap();
    assert!(laptop.list().unwrap().iter().any(|i| i.id == wifi));
    assert!(!laptop.audit().unwrap().is_empty());
    assert!(!phone.audit().unwrap().is_empty());

    // Locked, the phone needs the password: the device keystore alone
    // and a wrong password are refused.
    phone.lock().unwrap();
    assert!(phone.unlock().is_err());
    assert!(
        phone
            .unlock_with_password("not the password at all")
            .is_err()
    );
    phone.unlock_with_password(PASSWORD).unwrap();
    assert!(phone.list().unwrap().iter().any(|i| i.id == mail));
}

#[test]
fn a_join_cancelled_after_the_approval_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server();
    let laptop = laptop(dir.path(), &srv);
    let offered = laptop.link_offer().unwrap();
    let phone = App::start(paths(dir.path(), "phone"), None, LockPolicy::default());
    phone.join_start_sealed(&srv.url(), &offered.code).unwrap();
    laptop.link_poll().unwrap();
    phone.join_poll().unwrap();
    laptop.link_approve(pw()).unwrap();
    assert_eq!(phone.join_poll().unwrap().state, "approved");
    phone.join_cancel().unwrap();
    assert!(files(&phone).is_empty(), "{:?}", files(&phone));
    assert!(!phone.probe().unwrap().identity_present);
    let err = phone.join_finish("me@example.com", PASSWORD).unwrap_err();
    assert!(
        err.message.contains("no join is under way"),
        "{}",
        err.message
    );
    // Nothing on the phone also means the desktop's open can't make a
    // plain vault from nothing there.
    let refused = phone.open_or_create(true).unwrap_err();
    assert!(
        refused.message.contains("Nothing is set up"),
        "{}",
        refused.message
    );
    assert!(files(&phone).is_empty(), "{:?}", files(&phone));
}

#[test]
fn a_declined_link_writes_nothing_and_a_wrong_qr_stops_the_join() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server();
    let laptop = laptop(dir.path(), &srv);
    let offered = laptop.link_offer().unwrap();
    let phone = App::start(paths(dir.path(), "phone"), None, LockPolicy::default());
    phone.join_start_sealed(&srv.url(), &offered.code).unwrap();
    laptop.link_poll().unwrap();
    phone.join_poll().unwrap();
    laptop.link_decline().unwrap();
    let err = phone.join_poll().unwrap_err();
    assert!(err.message.contains("declined"), "{}", err.message);
    assert_eq!(laptop.device_list().unwrap().len(), 1);
    assert!(!phone.probe().unwrap().identity_present);
    assert!(files(&phone).is_empty(), "{:?}", files(&phone));

    // A QR whose fingerprint names another device: refused before anything is sent.
    let offered = laptop.link_offer().unwrap();
    let forged = offered
        .qr
        .replace(&offered.qr[offered.qr.len() - 32..], &"0".repeat(32));
    let tablet = App::start(paths(dir.path(), "tablet"), None, LockPolicy::default());
    let err = tablet.join_start_sealed("", &forged).unwrap_err();
    assert!(
        err.message.contains("isn't the one whose QR code"),
        "{}",
        err.message
    );
    // The real QR names the server, so the join needs nothing typed.
    let tablet2 = App::start(paths(dir.path(), "tablet2"), None, LockPolicy::default());
    assert!(
        tablet2.join_start_sealed("", &offered.qr).is_err(),
        "that code was used"
    );
}

#[test]
fn the_opt_out_needs_a_yes_and_is_handed_on_as_such() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server();
    let app = App::start(paths(dir.path(), "laptop"), None, LockPolicy::default());
    app.open_or_create_plain().unwrap();
    let refused = app
        .account_register_opted_out(&srv.url(), "me@example.com", PASSWORD, false)
        .unwrap_err();
    assert!(refused.message.contains("Without a Secret Key"));
    assert_eq!(refused.detail.as_deref(), Some("confirm_opt_out"));
    let registered = app
        .account_register_opted_out(&srv.url(), "me@example.com", PASSWORD, true)
        .unwrap();
    assert!(
        registered
            .lines
            .iter()
            .any(|l| l.contains("without a Secret Key"))
    );
    assert_eq!(app.secret_key_status().unwrap().state, "opted-out");
    app.sync().unwrap();

    let offered = app.link_offer().unwrap();
    let phone = App::start(paths(dir.path(), "phone"), None, LockPolicy::default());
    phone.join_start_sealed(&srv.url(), &offered.code).unwrap();
    app.link_poll().unwrap();
    phone.join_poll().unwrap();
    app.link_approve(pw()).unwrap();
    let approved = phone.join_poll().unwrap();
    assert_eq!(approved.secret_key.as_deref(), Some("opted-out"));
    phone.join_finish("me@example.com", PASSWORD).unwrap();
    assert_eq!(phone.secret_key_status().unwrap().state, "opted-out");
}
