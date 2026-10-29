//! A confined home (docs/plan/40-devtools.md §3): the app moves to other
//! paths and back without a restart; while confined it never reaches the
//! network and refuses exports and backups; going back finds the first
//! home as it was, locked. In its own test binary, because the network
//! hold is for the whole process.

use std::path::Path;

use lockdown_app::{App, GateInput, LockPolicy, Paths};

const WHY: &str = "the demo never syncs, shares, exports, backs up or goes online";

fn paths(dir: &Path, name: &str) -> Paths {
    Paths::resolve(Some(dir.join(name).join("snds/identity")), None).unwrap()
}

fn yes() -> GateInput {
    GateInput {
        password: None,
        confirmed: true,
    }
}

#[test]
fn a_confined_home_is_separate_offline_and_keeps_nothing_in() {
    let dir = tempfile::tempdir().unwrap();
    let real = paths(dir.path(), "real");
    let app = App::start(real.clone(), None, LockPolicy::default());
    app.open_or_create(true).unwrap();
    app.add_login("https://a.example.com", "sam", b"pw-one".to_vec(), None)
        .unwrap();
    assert_eq!(app.confinement().unwrap(), None);

    let demo = paths(dir.path(), "demo");
    let left = app.rehome(demo.clone(), Some(WHY.into())).unwrap();
    assert_eq!(left.data, real.data);
    assert!(
        app.lock_state().unwrap().locked,
        "the real vault was locked"
    );
    assert_eq!(app.confinement().unwrap().as_deref(), Some(WHY));
    app.open_or_create(true).unwrap();
    assert!(app.list().unwrap().is_empty(), "the demo starts empty");
    app.add_login("https://b.example.org", "demo", b"pw-two".to_vec(), None)
        .unwrap();

    // Exports and backups are refused before any gate, in the home's words.
    let expected = "The demo never syncs, shares, exports, backs up or goes online.";
    let out = dir.path().join("export.json");
    let refused = app
        .export_to(out.clone(), "cxf", None, true, None, yes())
        .unwrap_err();
    assert_eq!(refused.message, expected);
    assert!(!out.exists());
    assert_eq!(
        app.export_items("cxf", yes()).unwrap_err().message,
        expected
    );
    assert_eq!(
        app.export_preview("cxf", None).unwrap_err().message,
        expected
    );
    assert_eq!(app.backup_now().unwrap_err().message, expected);
    let folder = dir.path().join("backups");
    std::fs::create_dir_all(&folder).unwrap();
    let refused = app
        .backup_setup(
            folder.display().to_string(),
            Some(b"backup password".to_vec()),
            24,
            5,
            yes(),
        )
        .unwrap_err();
    assert_eq!(refused.message, expected);
    assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 0);

    // Nothing reaches the network, through either client.
    assert_eq!(lockdown_sync::network_held().as_deref(), Some(WHY));
    let transport = lockdown_sync::HttpTransport::new("https://127.0.0.1:9").unwrap();
    use lockdown_sync::Transport;
    let err = transport.kt_sth().unwrap_err().to_string();
    assert!(err.contains(WHY), "{err}");
    use lockdown_security::Fetch;
    let fetch = lockdown_security::HttpFetch::new(std::time::Duration::from_secs(1)).unwrap();
    let err = fetch
        .get("https://127.0.0.1:9/range/ABCDE", &[])
        .unwrap_err()
        .to_string();
    assert!(err.contains(WHY), "{err}");

    // Back home: locked, the network released, the first vault as it was.
    let left = app.rehome(real.clone(), None).unwrap();
    assert_eq!(left.data, demo.data);
    assert!(app.lock_state().unwrap().locked);
    assert_eq!(app.confinement().unwrap(), None);
    assert_eq!(lockdown_sync::network_held(), None);
    assert_eq!(lockdown_security::http::network_held(), None);
    app.unlock().unwrap();
    let titles: Vec<String> = app.list().unwrap().into_iter().map(|r| r.title).collect();
    assert_eq!(titles, vec!["a.example.com".to_string()]);
    let out = dir.path().join("after.json");
    app.export_to(out.clone(), "cxf", None, true, None, yes())
        .unwrap();
    assert!(out.exists(), "exports work again at home");
}
