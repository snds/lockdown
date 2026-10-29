//! The local account and the master password, end to end against the
//! in-process server (docs/plan/39-first-run.md §1.1–§1.6, §1.10): a first
//! run with no server to configure, unlock with the password and only the
//! password (also after a restart), the migration of an install made
//! before the master password with nothing in the vault lost, and a
//! forgotten password replaced with the recovery kit.

use std::path::Path;

use lockdown_app::local_unlock::{self, KeyFile};
use lockdown_app::{App, LockPolicy, MasterPasswordInput, Paths};

const NAME: &str = "Sean";
const PASSWORD: &str = "correct horse battery staple";
const NEW_PASSWORD: &str = "orbit lantern pickle meadow";

fn start(dir: &Path) -> App {
    lockdown_server::local::install();
    let paths = Paths::resolve(Some(dir.join("snds/identity")), None).unwrap();
    App::start(paths, None, LockPolicy::default())
}

fn data(app: &App) -> std::path::PathBuf {
    std::path::PathBuf::from(app.probe().unwrap().data)
}

/// The Secret Key and the recovery code from a kit page.
fn kit_lines(page: &[u8]) -> (String, String) {
    let text = std::str::from_utf8(page).unwrap();
    let value = |label: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(label))
            .unwrap()
            .trim()
            .to_string()
    };
    (value("Secret Key:"), value("Recovery code:"))
}

fn titles(app: &App) -> Vec<String> {
    let mut t: Vec<String> = app.list().unwrap().into_iter().map(|i| i.title).collect();
    t.sort();
    t
}

#[test]
fn a_first_run_makes_a_local_account_that_only_the_master_password_opens() {
    let dir = tempfile::tempdir().unwrap();
    let app = start(dir.path());
    let probe = app.probe().unwrap();
    assert!(!probe.identity_present && !probe.master_password && !probe.local_account);

    // The name and the password are checked before anything is made.
    assert!(
        app.first_run_create("  ", PASSWORD)
            .unwrap_err()
            .message
            .starts_with("Type a name")
    );
    assert!(
        app.first_run_create(NAME, "sean1234")
            .unwrap_err()
            .message
            .contains("at least fair")
    );
    assert!(!app.probe().unwrap().identity_present);

    let made = app.first_run_create(NAME, PASSWORD).unwrap();
    let page = made.kit.expect("the kit page, once");
    let text = std::str::from_utf8(page.expose()).unwrap().to_string();
    // A local account's kit names no server and no login.
    assert!(text.contains("Secret Key:") && text.contains("Recovery code:"));
    assert!(!text.contains("Server:") && !text.contains("lockdown-local"));
    assert!(text.contains("Forgot your password? Use your recovery kit"));
    lockdown_app::kit_pdf::check(&text).unwrap();

    let probe = app.probe().unwrap();
    assert!(probe.master_password && probe.local_account);
    assert_eq!(probe.name.as_deref(), Some(NAME));
    assert_eq!(probe.stage.as_deref(), Some("account_made"));
    let data = data(&app);
    assert_eq!(local_unlock::key_file(&data), KeyFile::Sealed);
    assert!(!data.join(local_unlock::PLAIN_FILE).exists());
    assert!(data.join("local-server/server.sqlite").exists());
    assert!(lockdown_app::onboarding::created_identity(&data).is_some());

    let status = app.status().unwrap();
    let account = status.account.expect("the local account");
    assert!(account.local);
    assert_eq!(account.login, NAME);

    app.add_login("example.com", "sam", b"hunter2".to_vec(), Some("Example"))
        .unwrap();
    // The vault syncs to the server inside the app, with the log checked.
    app.sync().unwrap();
    // A second first run is refused.
    assert!(app.first_run_create(NAME, PASSWORD).is_err());

    // Lock: the device key alone no longer opens it, a wrong password
    // doesn't, the right one does.
    app.lock().unwrap();
    assert_eq!(
        app.unlock().unwrap_err().message,
        local_unlock::PASSWORD_NEEDED
    );
    assert_eq!(
        app.unlock_with_password("correct horse battery stapler")
            .unwrap_err()
            .message,
        local_unlock::WRONG_PASSWORD
    );
    app.unlock_with_password(PASSWORD).unwrap();
    assert_eq!(titles(&app), ["Example"]);
    // The gates' password check runs against this computer's slot.
    app.password_check(PASSWORD).unwrap();
    assert!(app.password_check("not it at all, really").is_err());

    // A restart: a new app over the same files needs the password.
    drop(app);
    let app = start(dir.path());
    assert!(app.unlock().is_err());
    assert!(app.status().is_err());
    app.unlock_with_password(PASSWORD).unwrap();
    assert_eq!(titles(&app), ["Example"]);
    app.sync().unwrap();
}

#[test]
fn an_install_without_a_master_password_moves_to_one_and_loses_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let app = start(dir.path());
    // "This device only", the old way: a plain key file and a kit.
    app.open_or_create(true).unwrap();
    let kit = app.kit_create(false, None).unwrap();
    let (secret_key, recovery_code) = kit_lines(kit.kit.expose());
    for (site, title) in [("a.example", "Alpha"), ("b.example", "Beta")] {
        app.add_login(site, "me", b"pw-123".to_vec(), Some(title))
            .unwrap();
    }
    let second = app.create_vault("Home").unwrap();
    let id = app
        .add_login("c.example", "me", b"pw-456".to_vec(), Some("Gamma"))
        .unwrap();
    let id = app.move_item(&id, &second, Default::default()).unwrap();
    // A folder, a tag, an attachment and a crown jewel.
    let alpha = app
        .list()
        .unwrap()
        .into_iter()
        .find(|i| i.title == "Alpha")
        .unwrap()
        .id;
    let first = app.vaults().unwrap()[0].id.clone();
    let folder = app.add_folder(&first, "Work", None).unwrap();
    app.organize(
        &alpha,
        Some(Some(folder)),
        Some(vec!["home".into()]),
        Some(true),
    )
    .unwrap();
    let att = app
        .attach(
            &alpha,
            "note.txt",
            "text/plain",
            b"attached bytes".to_vec(),
            Default::default(),
        )
        .unwrap();
    let confirmed = lockdown_app::GateInput {
        password: None,
        confirmed: true,
    };
    app.set_crown_jewel(&id, true, confirmed.clone()).unwrap();
    let snapshot = |app: &App| {
        let mut items: Vec<(String, String, bool, i64)> = app
            .list()
            .unwrap()
            .into_iter()
            .map(|i| (i.id, i.title, i.crown_jewel, i.edited_at))
            .collect();
        items.sort();
        let versions: Vec<u64> = items
            .iter()
            .map(|(id, ..)| app.show(id, false).unwrap().version)
            .collect();
        (items, versions)
    };
    let items_before = snapshot(&app);
    let place_before = format!("{:?}", app.show(&alpha, false).unwrap().place);
    assert!(place_before.contains("home"), "{place_before}");
    let before = titles(&app);
    let vaults_before: Vec<String> = app.vaults().unwrap().into_iter().map(|v| v.id).collect();
    let data = data(&app);
    assert_eq!(local_unlock::key_file(&data), KeyFile::Plain);
    let probe = app.probe().unwrap();
    assert!(!probe.master_password && !probe.local_account);

    // The kit must match; a weak password is refused; nothing changes.
    let wrong = MasterPasswordInput {
        name: Some(NAME.into()),
        password: PASSWORD.into(),
        secret_key: Some(secret_key.clone()),
        recovery_code: Some("0".repeat(32)),
        new_kit: false,
    };
    assert!(app.set_master_password(wrong).is_err());
    let weak = MasterPasswordInput {
        name: Some(NAME.into()),
        password: "password1".into(),
        secret_key: Some(secret_key.clone()),
        recovery_code: Some(recovery_code.clone()),
        new_kit: false,
    };
    assert!(app.set_master_password(weak).is_err());
    assert_eq!(local_unlock::key_file(&data), KeyFile::Plain);

    let done = app
        .set_master_password(MasterPasswordInput {
            name: Some(NAME.into()),
            password: PASSWORD.into(),
            secret_key: Some(secret_key.clone()),
            recovery_code: Some(recovery_code.clone()),
            new_kit: false,
        })
        .unwrap();
    assert!(done.kit.is_none(), "the old kit still works; no new page");
    assert_eq!(local_unlock::key_file(&data), KeyFile::Sealed);
    assert!(!data.join(local_unlock::PLAIN_FILE).exists());
    assert_eq!(titles(&app), before);

    app.lock().unwrap();
    assert!(app.unlock().is_err());
    app.unlock_with_password(PASSWORD).unwrap();
    assert_eq!(titles(&app), before);
    assert_eq!(snapshot(&app), items_before);
    assert_eq!(
        app.attachment(&alpha, &att, Default::default())
            .unwrap()
            .expose(),
        b"attached bytes"
    );
    let place_after = format!("{:?}", app.show(&alpha, false).unwrap().place);
    assert_eq!(place_after, place_before);
    let vaults_after: Vec<String> = app.vaults().unwrap().into_iter().map(|v| v.id).collect();
    assert_eq!(vaults_before, vaults_after);
    assert!(app.status().unwrap().account.unwrap().local);

    // The old kit opens it after a forgotten password.
    app.lock().unwrap();
    app.password_reset_with_kit(&secret_key, &recovery_code, NEW_PASSWORD)
        .unwrap();
    assert_eq!(titles(&app), before);
    app.lock().unwrap();
    assert_eq!(
        app.unlock_with_password(PASSWORD).unwrap_err().message,
        local_unlock::WRONG_PASSWORD
    );
    app.unlock_with_password(NEW_PASSWORD).unwrap();
    // The account's password changed on the server inside the app too: the
    // gates' check and a sync both work with the new one.
    app.password_check(NEW_PASSWORD).unwrap();
    app.sync().unwrap();
}

#[test]
fn a_migration_can_make_a_new_kit_instead() {
    let dir = tempfile::tempdir().unwrap();
    let app = start(dir.path());
    app.open_or_create(true).unwrap();
    let old = app.kit_create(false, None).unwrap();
    let (old_sk, old_rc) = kit_lines(old.kit.expose());
    app.add_login("a.example", "me", b"pw".to_vec(), Some("Alpha"))
        .unwrap();
    let done = app
        .set_master_password(MasterPasswordInput {
            name: Some(NAME.into()),
            password: PASSWORD.into(),
            secret_key: None,
            recovery_code: None,
            new_kit: true,
        })
        .unwrap();
    let page = done.kit.expect("the new kit's page");
    let (sk, rc) = kit_lines(page.expose());
    assert_ne!(sk, old_sk);
    assert_eq!(app.probe().unwrap().stage.as_deref(), Some("account_made"));
    app.lock().unwrap();
    // The old page no longer opens it; the new one does.
    assert!(
        app.password_reset_with_kit(&old_sk, &old_rc, NEW_PASSWORD)
            .is_err()
    );
    app.password_reset_with_kit(&sk, &rc, NEW_PASSWORD).unwrap();
    assert_eq!(titles(&app), ["Alpha"]);
}

#[test]
fn a_forgotten_password_on_a_first_run_vault_and_a_new_kit_later() {
    let dir = tempfile::tempdir().unwrap();
    let app = start(dir.path());
    let made = app.first_run_create(NAME, PASSWORD).unwrap();
    let (sk, rc) = kit_lines(made.kit.unwrap().expose());
    app.add_login("a.example", "me", b"pw".to_vec(), Some("Alpha"))
        .unwrap();
    app.lock().unwrap();
    // The kit is checked, and the new password must be fair.
    assert!(
        app.password_reset_with_kit(&sk, &"A".repeat(32), NEW_PASSWORD)
            .is_err()
    );
    assert!(app.password_reset_with_kit(&sk, &rc, "abc").is_err());
    app.password_reset_with_kit(&sk, &rc, NEW_PASSWORD).unwrap();
    assert_eq!(titles(&app), ["Alpha"]);
    // A new kit (with the master password) replaces the old one here too.
    let new = app.kit_create(true, Some(NEW_PASSWORD)).unwrap();
    let new_text = std::str::from_utf8(new.kit.expose()).unwrap().to_string();
    assert!(!new_text.contains("lockdown-local"));
    let (sk2, rc2) = kit_lines(new.kit.expose());
    app.lock().unwrap();
    assert!(app.password_reset_with_kit(&sk, &rc, PASSWORD).is_err());
    app.password_reset_with_kit(&sk2, &rc2, PASSWORD).unwrap();
    app.lock().unwrap();
    app.unlock_with_password(PASSWORD).unwrap();
    app.sync().unwrap();
}

#[test]
fn a_local_vault_moves_to_a_real_server_and_another_device_finds_it_there() {
    let dir = tempfile::tempdir().unwrap();
    let app = start(dir.path());
    let made = app.first_run_create(NAME, PASSWORD).unwrap();
    let (sk, rc) = kit_lines(made.kit.unwrap().expose());
    for (site, title) in [("a.example", "Alpha"), ("b.example", "Beta")] {
        app.add_login(site, "me", b"pw".to_vec(), Some(title))
            .unwrap();
    }
    let state = std::sync::Arc::new(
        lockdown_server::AppState::in_memory(lockdown_server::Config::default()).unwrap(),
    );
    let server = lockdown_server::run_in_background(state).unwrap();
    let url = server.url();
    app.discovery_confirm(&url).unwrap();

    // The master password is checked here first; nothing changes on a wrong one.
    assert_eq!(
        app.move_to_server(&url, "me@example.com", "not it at all, really")
            .unwrap_err()
            .message,
        "That isn't your master password."
    );
    assert!(app.status().unwrap().account.unwrap().local);

    app.move_to_server(&url, "me@example.com", PASSWORD)
        .unwrap();
    let account = app.status().unwrap().account.unwrap();
    assert!(!account.local);
    assert_eq!(account.server, url);
    assert_eq!(titles(&app), ["Alpha", "Beta"]);
    assert!(!data(&app).join("local-server").exists());
    // Still opened by the master password here, offline or not.
    app.lock().unwrap();
    app.unlock_with_password(PASSWORD).unwrap();
    app.sync().unwrap();

    // A second computer recovers the same vault from that server with the kit.
    let other = tempfile::tempdir().unwrap();
    let paths = Paths::resolve(Some(other.path().join("snds/identity")), None).unwrap();
    let b = App::start(paths, None, LockPolicy::default());
    b.recover(&url, "me@example.com", PASSWORD, &sk, &rc)
        .unwrap();
    b.open_or_create(true).unwrap();
    assert_eq!(titles(&b), ["Alpha", "Beta"]);

    // A move that fails (the login is taken there) leaves the vault on its
    // own computer, as it was, and its master password still checks.
    let third = tempfile::tempdir().unwrap();
    let c = start(third.path());
    c.first_run_create("Alex", PASSWORD).unwrap();
    c.add_login("c.example", "me", b"pw".to_vec(), Some("Gamma"))
        .unwrap();
    c.discovery_confirm(&url).unwrap();
    let err = c
        .move_to_server(&url, "me@example.com", PASSWORD)
        .unwrap_err()
        .message;
    assert!(err.starts_with("The move stopped:"), "{err}");
    assert!(
        err.ends_with("Your vault is unchanged and still on this computer. You can try again.")
    );
    assert!(c.status().unwrap().account.unwrap().local);
    assert_eq!(titles(&c), ["Gamma"]);
    c.password_check(PASSWORD).unwrap();
    c.sync().unwrap();
    c.kit_create(true, Some(PASSWORD)).unwrap();
}
