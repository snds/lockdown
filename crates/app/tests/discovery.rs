//! Discovery against a real server (docs/plan/39-first-run.md §1.7): a
//! typed address and this computer's config are checked through the
//! server's own document, the confirm pins its key, and a sync refuses a
//! server whose key isn't the one confirmed. Nothing is sent to a found
//! server before the person confirms.

use std::sync::Arc;

use lockdown_app::discovery::{self, Found, State};
use lockdown_app::{App, LockPolicy, Paths};
use lockdown_server::{AppState, Config, run_in_background};

#[test]
fn a_running_server_is_found_confirmed_and_held_to_its_key() {
    let dir = tempfile::tempdir().unwrap();
    let state = Arc::new(AppState::in_memory(Config::default()).unwrap());
    let server = run_in_background(state.clone()).unwrap();
    let url = server.url();
    let paths = Paths::resolve(Some(dir.path().join("snds/identity")), None).unwrap();
    let data = paths.data.clone();
    let app = App::start(paths, None, LockPolicy::default());

    // A typed address: ready, with the fingerprint of the server's log key.
    let typed = app.discovery_check(&url).unwrap();
    assert_eq!(typed.state, State::Ready, "{typed:?}");
    assert_eq!(typed.sources, vec![Found::Typed]);
    let key = state.log_key.verifying_key().to_bytes();
    assert_eq!(
        typed.fingerprint.as_deref(),
        Some(discovery::fingerprint(&key).as_str())
    );
    // Checking registered nothing.
    assert!(
        state
            .storage
            .account_by_login("me@example.com")
            .unwrap()
            .is_none()
    );

    // Confirmed: pinned. An account there syncs while the key matches.
    app.discovery_confirm(&url).unwrap();
    app.open_or_create(true).unwrap();
    app.account_register(&url, "me@example.com", "correct horse battery staple", None)
        .unwrap();
    app.sync().unwrap();
    // The account's server is listed as in use, from this computer's config.
    let found = app.discovery_candidates(false).unwrap();
    let ours = found.iter().find(|c| c.address == url).unwrap();
    assert!(ours.in_use && ours.sources.contains(&Found::Settings));
    assert_eq!(found[0].address, url, "the server in use comes first");

    // A different key pinned for it: the sync refuses, in words.
    discovery::forget(&data, &url).unwrap();
    discovery::confirm(&data, &url, [1; 32]).unwrap();
    let err = app.sync().unwrap_err().message;
    assert!(
        err.contains("different key from the one you confirmed"),
        "{err}"
    );
    assert_eq!(app.discovery_check(&url).unwrap().state, State::WrongKey);
}
