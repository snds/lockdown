//! The developer reset end to end (docs/plan/40-devtools.md §2): a fully
//! used profile (an account on a real server, items, settings, a kit, a
//! paired browser, the endpoint file, the shell's own folders) in a
//! temporary home and a keystore double; after the reset nothing of
//! Lockdown is left, a sibling's keystore entry is, and a second launch is
//! the first run. An identity another app also uses is kept.

use std::path::Path;
use std::sync::Arc;

use lockdown_app::{App, LockPolicy, Paths};
use lockdown_devtools::reset::{
    self, Keystore, MemoryKeystore, PENDING_FILE, Places, SERVICE, ServiceStore,
};
use lockdown_native_messaging::endpoint::EndpointFile;
use lockdown_server::{AppState, Config, run_in_background};

fn places(home: &Path, paths: &Paths) -> Places {
    Places {
        data: paths.data.clone(),
        identity_root: paths.root.clone(),
        shell_dirs: vec![
            (
                "The app's settings folder".into(),
                home.join(".config/design.snds.lockdown"),
            ),
            (
                "The app's cache".into(),
                home.join(".cache/design.snds.lockdown"),
            ),
            (
                "The app's logs".into(),
                home.join(".local/share/design.snds.lockdown/logs"),
            ),
        ],
        home: Some(home.to_path_buf()),
    }
}

/// Everything under `dir`, as paths relative to it.
fn files(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push(p.strip_prefix(dir).unwrap().display().to_string());
            }
        }
    }
    out.sort();
    out
}

#[test]
fn a_reset_of_a_fully_used_profile_leaves_nothing_and_the_next_launch_is_first_run() {
    let keystore: &'static MemoryKeystore = Box::leak(Box::default());
    lockdown_app::secrets::install(Box::new(ServiceStore(keystore)));
    // A sibling app's entry, which a reset must leave.
    keystore.put("design.snds.mailbird", "session:sam", "theirs");

    let server =
        run_in_background(Arc::new(AppState::in_memory(Config::default()).unwrap())).unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home/sam");
    let paths = Paths::resolve(Some(home.join(".local/share/snds/identity")), None).unwrap();
    let app = App::start(paths.clone(), None, LockPolicy::default());
    app.open_or_create(true).unwrap();
    app.add_login(
        "https://mail.example.com",
        "sam",
        b"a password".to_vec(),
        None,
    )
    .unwrap();
    let mut settings = app.settings().unwrap();
    settings.clipboard_clear_secs = 60;
    app.set_settings(settings).unwrap();
    app.account_register(
        &server.url(),
        "sam@example.com",
        "correct horse battery staple",
        None,
    )
    .unwrap();
    app.sync().unwrap();
    assert!(
        !keystore.list(SERVICE).unwrap().is_empty(),
        "the session token went to the keystore"
    );
    // A paired browser and the endpoint file, as the desktop writes them.
    std::fs::write(paths.data.join("browser-pairings.json"), b"{\"paired\":[]}").unwrap();
    EndpointFile {
        v: 1,
        socket: paths.data.join("browser.sock"),
        app_exe: std::env::current_exe().unwrap(),
        app_sha256: "00".repeat(32),
    }
    .write(&paths.data)
    .unwrap();
    let places = places(&home, &paths);
    for (_, dir) in &places.shell_dirs {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("something"), b"x").unwrap();
    }
    // The person's own files stay.
    std::fs::create_dir_all(home.join("Documents")).unwrap();
    std::fs::write(home.join("Documents/export.json"), b"mine").unwrap();
    app.lock().unwrap();
    drop(app);

    let plan = reset::plan(&places, keystore);
    assert!(plan.identity.erase, "{}", plan.identity.why);
    assert_eq!(plan.identity.apps, vec!["lockdown".to_string()]);
    assert_eq!(plan.erase.len(), 5, "{:?}", plan.erase);
    assert_eq!(
        plan.keystore.entries,
        vec![format!("session:sam@example.com@{}", server.url())]
    );
    assert!(plan.refused.is_empty());
    assert!(plan.keeps.iter().any(|k| k.contains("server")));

    // It waits for the next launch, then runs before anything opens.
    reset::mark_pending(&places.data).unwrap();
    assert!(reset::pending(&places.data));
    let erased = reset::finish_pending(&places, keystore).unwrap();
    assert!(erased.failed.is_empty(), "{:?}", erased.failed);
    assert_eq!(erased.removed.len(), 5);
    assert!(reset::finish_pending(&places, keystore).is_none(), "once");

    assert!(!paths.data.exists());
    assert!(!paths.root.exists());
    for (_, dir) in &places.shell_dirs {
        assert!(!dir.exists(), "{}", dir.display());
    }
    assert!(keystore.list(SERVICE).unwrap().is_empty());
    assert_eq!(
        keystore.all(),
        vec![(
            "design.snds.mailbird".to_string(),
            "session:sam".to_string()
        )]
    );
    assert_eq!(files(&home), vec!["Documents/export.json".to_string()]);

    // The second launch is the first run.
    let again = App::start(paths.clone(), None, LockPolicy::default());
    let probe = again.probe().unwrap();
    assert!(!probe.identity_present && !probe.device_key_present && !probe.account_present);
    assert!(!paths.data.join(PENDING_FILE).exists());
}

#[test]
fn an_identity_another_app_uses_is_kept() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home/alex");
    let paths = Paths::resolve(Some(home.join(".local/share/snds/identity")), None).unwrap();
    let app = App::start(paths.clone(), None, LockPolicy::default());
    app.open_or_create(true).unwrap();
    app.lock().unwrap();
    drop(app);
    assert!(reset::identity_plan(&paths.root).erase);

    // Another app's persona: the store no longer verifies, and its records,
    // read one by one, name that app.
    use ecosystem_identity::records::Persona;
    let dir = paths.root.join("personas");
    let file = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .next()
        .unwrap()
        .path();
    let mut persona: Persona =
        ecosystem_identity::cbor::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    persona.created_by_app = "mailbird".into();
    std::fs::write(
        &file,
        ecosystem_identity::cbor::to_canonical(&persona).unwrap(),
    )
    .unwrap();

    let plan = reset::identity_plan(&paths.root);
    assert!(!plan.erase, "{}", plan.why);
    assert!(
        plan.why.starts_with("It is shared with mailbird"),
        "{}",
        plan.why
    );
    let keystore = MemoryKeystore::default();
    let places = places(&home, &paths);
    let erased = reset::erase(&places, &keystore);
    assert!(erased.failed.is_empty(), "{:?}", erased.failed);
    assert!(!paths.data.exists(), "Lockdown's own data goes");
    assert!(paths.root.exists(), "the shared identity stays");

    // Garbage that names no app at all is left too, with why.
    let junk = tmp.path().join("junk/snds/identity");
    std::fs::create_dir_all(&junk).unwrap();
    std::fs::write(junk.join("FORMAT"), b"ecosystem-identity/1").unwrap();
    std::fs::write(junk.join("profile.cbor"), b"not cbor").unwrap();
    let plan = reset::identity_plan(&junk);
    assert!(!plan.erase);
    assert!(plan.why.starts_with("It couldn't be read"), "{}", plan.why);
}

#[test]
fn the_guard_refuses_a_data_folder_that_holds_home() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home/sam");
    std::fs::create_dir_all(&home).unwrap();
    let places = Places {
        data: tmp.path().join("home"),
        identity_root: tmp.path().join("nothing/snds/identity"),
        shell_dirs: vec![],
        home: Some(home.clone()),
    };
    let plan = reset::plan(&places, &MemoryKeystore::default());
    assert!(plan.erase.is_empty());
    assert_eq!(plan.refused.len(), 1);
    assert!(plan.refused[0].what.contains("it holds your home folder"));
    reset::erase(&places, &MemoryKeystore::default());
    assert!(home.exists());
}
