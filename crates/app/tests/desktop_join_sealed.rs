//! The desktop's sign-in and recovery joins, sealed in one step, through
//! `App` against the real server (docs/plan/45-desktop-join-sealed.md):
//! `account_login_sealed` or `recover_sealed`, then `open_joined` with the
//! same password, as the desktop's join screen calls them.
//!
//! - a wrong password writes nothing;
//! - a wrong Secret Key or recovery code keeps only the sealed key file
//!   (D522), and the retry with the same password is the same device;
//! - the joined computer holds only the sealed key file, opens with the
//!   password (never with the device keystore alone) and pulls the vault;
//! - a recovered computer also has the kit slot;
//! - an install made the old way (plain keys) still opens, and
//!   `set_master_password` seals it.

use std::sync::Arc;

use lockdown_app::local_unlock::{self, KeyFile};
use lockdown_app::{App, ErrorCode, LockPolicy, MasterPasswordInput, Paths};
use lockdown_server::{AppState, Config, RunningServer, run_in_background};

const PASSWORD: &str = "correct horse battery staple";
const LOGIN: &str = "me@example.com";

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

fn data_of(app: &App) -> std::path::PathBuf {
    std::path::PathBuf::from(app.probe().unwrap().data)
}

/// The files in a computer's data folder, sorted.
fn files(app: &App) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(data_of(app))
        .map(|d| {
            d.filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// The laptop that made the account, with one item synced; returns it,
/// the item and the kit's Secret Key and recovery code.
fn laptop(dir: &std::path::Path, url: &str) -> (App, String, String, String) {
    let app = App::start(paths(dir, "laptop"), None, LockPolicy::default());
    app.open_or_create_plain().unwrap();
    let reg = app.account_register(url, LOGIN, PASSWORD, None).unwrap();
    let page = String::from_utf8(reg.kit.unwrap().expose().to_vec()).unwrap();
    let line = |label: &str| {
        page.lines()
            .find_map(|l| l.strip_prefix(label))
            .unwrap()
            .trim()
            .to_string()
    };
    let item = app
        .add_login("https://mail.example.com", "me", b"pw-one".to_vec(), None)
        .unwrap();
    app.sync().unwrap();
    (app, item, line("Secret Key:"), line("Recovery code:"))
}

/// What every sealed join leaves: only the sealed key file, the identity,
/// the login as the lock screen's name, and the password needed again.
fn assert_sealed(app: &App) {
    let data = data_of(app);
    assert_eq!(local_unlock::key_file(&data), KeyFile::Sealed);
    assert!(!data.join(local_unlock::PLAIN_FILE).exists());
    let probe = app.probe().unwrap();
    assert!(probe.master_password && probe.identity_present);
    assert_eq!(probe.name.as_deref(), Some(LOGIN));
    app.lock().unwrap();
    assert!(
        app.unlock().is_err(),
        "the device keystore alone opens nothing"
    );
    assert!(app.unlock_with_password("not the password at all").is_err());
    app.unlock_with_password(PASSWORD).unwrap();
}

#[test]
fn signing_in_on_an_empty_computer_seals_it_in_one_step() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server();
    let url = srv.url();
    let (laptop, item, sk, _) = laptop(dir.path(), &url);
    let phone = App::start(paths(dir.path(), "phone"), None, LockPolicy::default());

    // A wrong password is refused at the sign-in: nothing is written.
    let e = phone
        .account_login_sealed(&url, LOGIN, "not the password at all", Some(&sk))
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::Refused);
    assert!(files(&phone).is_empty(), "{:?}", files(&phone));

    // A wrong Secret Key: no identity, the keys kept sealed (D522).
    let wrong_sk = "A1-00000-00000-00000-00000-000000";
    assert!(
        phone
            .account_login_sealed(&url, LOGIN, PASSWORD, Some(wrong_sk))
            .is_err()
    );
    assert!(!phone.probe().unwrap().identity_present);
    assert_eq!(local_unlock::key_file(&data_of(&phone)), KeyFile::Sealed);
    assert!(!data_of(&phone).join(local_unlock::PLAIN_FILE).exists());
    // Opening is refused until the join finishes; nothing is made.
    let e = phone.open_joined(PASSWORD).unwrap_err();
    assert!(e.message.contains("didn't finish"), "{}", e.message);
    assert!(!phone.probe().unwrap().identity_present);

    // The retry with the same password is the same device, and finishes.
    let lines = phone
        .account_login_sealed(&url, LOGIN, PASSWORD, Some(&sk))
        .unwrap();
    assert!(!lines.is_empty());
    let status = phone.open_joined(PASSWORD).unwrap();
    assert!(status.account.is_some());
    assert!(phone.list().unwrap().iter().any(|i| i.id == item));
    assert_sealed(&phone);
    // Key transparency sees no stranger: the retry kept the device.
    phone.sync().unwrap();
    laptop.sync().unwrap();
    assert_eq!(laptop.device_list().unwrap().len(), 2);
}

#[test]
fn recovering_on_an_empty_computer_seals_it_under_the_password_and_the_kit() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server();
    let url = srv.url();
    let (_laptop, item, sk, rc) = laptop(dir.path(), &url);
    let phone = App::start(paths(dir.path(), "phone"), None, LockPolicy::default());

    let e = phone
        .recover_sealed(&url, LOGIN, "not the password at all", &sk, &rc)
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::Refused);
    assert!(files(&phone).is_empty(), "{:?}", files(&phone));

    phone
        .recover_sealed(&url, LOGIN, PASSWORD, &sk, &rc)
        .unwrap();
    // A wrong password can't open what the join sealed.
    assert!(phone.open_joined("not the password at all").is_err());
    let status = phone.open_joined(PASSWORD).unwrap();
    assert!(status.account.is_some());
    assert!(phone.list().unwrap().iter().any(|i| i.id == item));
    // The kit slot: "Forgot your password?" works here later.
    assert!(
        local_unlock::profile_of(&data_of(&phone))
            .unwrap()
            .is_some()
    );
    assert_sealed(&phone);
}

#[test]
fn an_install_joined_the_old_way_still_opens_and_is_sealed_later() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server();
    let url = srv.url();
    let (_laptop, item, sk, _) = laptop(dir.path(), &url);
    // The unsealed join the desktop made before: a plain key file.
    let old = App::start(paths(dir.path(), "old"), None, LockPolicy::default());
    old.account_login_plain(&url, LOGIN, PASSWORD, Some(&sk))
        .unwrap();
    assert_eq!(local_unlock::key_file(&data_of(&old)), KeyFile::Plain);
    old.open_or_create(true).unwrap();
    assert!(old.list().unwrap().iter().any(|i| i.id == item));
    // `open_joined` is for sealed joins only.
    old.lock().unwrap();
    assert!(old.open_joined(PASSWORD).is_err());
    // Its next start opens with the device keystore, then the migration
    // seals it with the account's password.
    old.unlock().unwrap();
    let mut input = MasterPasswordInput::default();
    input.password = PASSWORD.into();
    old.set_master_password(input).unwrap();
    assert_sealed(&old);
}
