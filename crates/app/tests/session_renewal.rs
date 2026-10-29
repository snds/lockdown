//! Session renewal between the app and the real server, in process
//! (docs/plan/33-session-renewal.md): a device keeps syncing past its
//! token's lifetime without the password, whether it renews ahead of time
//! or after the server refused the token; an ended session, a revoked
//! device and a session idle past the limit each stop at a password prompt
//! whose words and machine reason say which; a sign-in clears it. A
//! revoked device can't sign back in with the password either, and the same
//! machine returns as a new device through the join flow
//! (docs/plan/35-revoke-split.md §1).

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use lockdown_app::{App, ErrorCode, GateInput, LockPolicy, Paths};
use lockdown_server::{AppState, Config, RunningServer, run_in_background};

const LOGIN: &str = "me@example.com";
const PASSWORD: &str = "correct horse battery staple";

/// A server whose clock the test moves.
fn server(ttl: Duration) -> (RunningServer, Arc<AtomicI64>) {
    let clock = Arc::new(AtomicI64::new(lockdown_vault::Timestamp::now().0));
    let mut state = AppState::in_memory(Config {
        session_ttl: ttl,
        per_login: (1000, 1000.0),
        per_address: (1000, 1000.0),
        ..Config::default()
    })
    .unwrap();
    let c = clock.clone();
    state.clock = Box::new(move || c.load(Ordering::SeqCst));
    (run_in_background(Arc::new(state)).unwrap(), clock)
}

fn device(dir: &std::path::Path, name: &str) -> App {
    let paths = Paths::resolve(Some(dir.join(name).join("snds/identity")), None).unwrap();
    App::start(paths, None, LockPolicy::default())
}

fn secret_key_of(page: &[u8]) -> String {
    std::str::from_utf8(page)
        .unwrap()
        .lines()
        .find_map(|l| l.strip_prefix("Secret Key:"))
        .map(|v| v.trim().to_string())
        .expect("the kit page names the Secret Key")
}

/// Two devices of one account: A made it, B joined with the kit.
fn two_devices(dir: &std::path::Path, url: &str) -> (App, App) {
    let a = device(dir, "a");
    a.open_or_create(true).unwrap();
    let registered = a.account_register(url, LOGIN, PASSWORD, None).unwrap();
    let secret_key = secret_key_of(registered.kit.as_ref().unwrap().expose());
    a.add_login("https://one.example", "me", b"pw".to_vec(), None)
        .unwrap();
    a.sync().unwrap();
    let b = device(dir, "b");
    b.account_login(url, LOGIN, PASSWORD, Some(&secret_key))
        .unwrap();
    b.open_or_create(true).unwrap();
    b.sync().unwrap();
    (a, b)
}

fn token_until(app: &App) -> i64 {
    app.status().unwrap().account.unwrap().session_expires_at
}

#[test]
fn a_device_keeps_syncing_past_its_tokens_life_without_the_password() {
    let dir = tempfile::tempdir().unwrap();
    // Eight hours, as by default; the server's clock then jumps a day, so
    // the server refuses the token this device still thinks is good.
    let (running, clock) = server(Duration::from_secs(8 * 3600));
    let url = running.url();
    let (a, b) = two_devices(dir.path(), &url);
    let before = token_until(&a);

    clock.fetch_add(24 * 3600, Ordering::SeqCst);
    a.add_login("https://two.example", "me", b"pw2".to_vec(), None)
        .unwrap();
    // Refused as expired, renewed with the device key, tried once more.
    a.sync().unwrap();
    let after = token_until(&a);
    assert!(after > before + 20 * 3600, "{before} → {after}");
    assert!(a.status().unwrap().account.unwrap().sign_in.is_none());
    // The other device does the same and pulls the new item.
    b.sync().unwrap();
    assert_eq!(b.list().unwrap().len(), 2);
    // Its session list shows each device once, both renewed.
    assert_eq!(b.sessions().unwrap().len(), 2);
    // "Check now" renews the same way when its token is refused.
    clock.fetch_add(24 * 3600, Ordering::SeqCst);
    a.audit().unwrap();
    assert!(token_until(&a) > after);
}

#[test]
fn a_short_lived_token_is_renewed_ahead_of_every_expiry() {
    let dir = tempfile::tempdir().unwrap();
    // Two minutes: always inside the renewal margin, so each call renews
    // ahead of time, and the server's clock may run on by hours.
    let (running, clock) = server(Duration::from_secs(120));
    let url = running.url();
    let (a, _b) = two_devices(dir.path(), &url);
    let mut last = token_until(&a);
    for hour in 1..=3 {
        clock.fetch_add(3600, Ordering::SeqCst);
        a.add_login(
            &format!("https://h{hour}.example"),
            "me",
            b"pw".to_vec(),
            None,
        )
        .unwrap();
        a.sync().unwrap();
        let now = token_until(&a);
        assert!(now > last, "hour {hour}: {last} → {now}");
        last = now;
    }
    assert_eq!(a.sessions().unwrap().len(), 2);
}

#[test]
fn an_ended_session_asks_for_the_password_and_says_why() {
    let dir = tempfile::tempdir().unwrap();
    let (running, _clock) = server(Duration::from_secs(8 * 3600));
    let url = running.url();
    let (a, b) = two_devices(dir.path(), &url);

    // A ends every other session: B's next sync can't renew.
    assert_eq!(a.end_other_sessions().unwrap(), 1);
    let e = b.sync().unwrap_err();
    assert_eq!(e.code, ErrorCode::SignInNeeded, "{e:?}");
    assert_eq!(e.detail.as_deref(), Some("ended"));
    assert!(e.message.contains("was ended"), "{}", e.message);
    let status = b.status().unwrap().account.unwrap();
    assert_eq!(status.sign_in.as_ref().unwrap().reason, "ended");
    // The same words on every later call, with no knock on the server.
    assert_eq!(b.sync().unwrap_err().detail.as_deref(), Some("ended"));
    assert_eq!(b.sessions().unwrap_err().code, ErrorCode::SignInNeeded);

    // The password signs it in again, and the prompt is gone.
    b.account_login(&url, LOGIN, PASSWORD, None).unwrap();
    assert!(b.status().unwrap().account.unwrap().sign_in.is_none());
    b.sync().unwrap();
}

#[test]
fn a_revoked_device_and_an_idle_one_ask_for_the_password_and_say_which() {
    let dir = tempfile::tempdir().unwrap();
    let (running, clock) = server(Duration::from_secs(8 * 3600));
    let url = running.url();
    let (a, b) = two_devices(dir.path(), &url);

    // A revokes B; once B's token runs out, it can't renew.
    let b_id = b
        .device_list()
        .unwrap()
        .into_iter()
        .find(|d| d.this_device)
        .unwrap()
        .id;
    a.device_revoke(
        &b_id,
        GateInput {
            password: Some(PASSWORD.into()),
            confirmed: false,
        },
    )
    .unwrap();
    clock.fetch_add(9 * 3600, Ordering::SeqCst);
    let e = b.sync().unwrap_err();
    assert_eq!(e.code, ErrorCode::SignInNeeded, "{e:?}");
    assert_eq!(e.detail.as_deref(), Some("revoked"));
    assert!(e.message.contains("was revoked"), "{}", e.message);
    // A, in good standing, renews and goes on.
    a.sync().unwrap();

    // Thirty-one days with no use: A can't renew either.
    clock.fetch_add(31 * 86_400, Ordering::SeqCst);
    let e = a.sync().unwrap_err();
    assert_eq!(e.detail.as_deref(), Some("idle"), "{e:?}");
    assert!(e.message.contains("went unused"), "{}", e.message);
    assert_eq!(
        a.status().unwrap().account.unwrap().sign_in.unwrap().reason,
        "idle"
    );
}

#[test]
fn revoking_a_device_ends_its_sessions_at_once_so_it_stops_reading_too() {
    // D405's follow-up (docs/plan/34-small-calls.md §4): no waiting for the
    // token to run out.
    let dir = tempfile::tempdir().unwrap();
    let (running, _clock) = server(Duration::from_secs(8 * 3600));
    let url = running.url();
    let (a, b) = two_devices(dir.path(), &url);
    assert_eq!(a.sessions().unwrap().len(), 2);
    let b_id = b
        .device_list()
        .unwrap()
        .into_iter()
        .find(|d| d.this_device)
        .unwrap()
        .id;
    a.device_revoke(
        &b_id,
        GateInput {
            password: Some(PASSWORD.into()),
            confirmed: false,
        },
    )
    .unwrap();
    // A's list holds none of B's sessions any more (the password check
    // A's gate made is a session of A's own, doc 33 §7).
    let left = a.sessions().unwrap();
    assert!(
        !left.is_empty() && left.iter().all(|s| s.device_id != b_id),
        "{left:?}"
    );
    assert!(left.iter().any(|s| s.current));
    // B's token, hours from running out, reads nothing now but the signed
    // identity record set: its sync learns from it that it is out (D73).
    let e = b.sync().unwrap_err();
    assert!(e.message.contains("was revoked"), "{e:?}");
    // Anything else asks for the password and says it was revoked.
    let e = b.sessions().unwrap_err();
    assert_eq!(e.code, ErrorCode::SignInNeeded, "{e:?}");
    assert_eq!(e.detail.as_deref(), Some("revoked"));
    assert!(e.message.contains("was revoked"), "{}", e.message);
    assert_eq!(
        b.status().unwrap().account.unwrap().sign_in.unwrap().reason,
        "revoked"
    );
    // A goes on as ever.
    a.add_login("https://two.example", "me", b"pw2".to_vec(), None)
        .unwrap();
    a.sync().unwrap();
}

#[test]
fn a_revoked_device_cant_sign_back_in_and_joins_again_as_a_new_device() {
    // docs/plan/35-revoke-split.md §1: the password no longer brings a
    // revoked device back; the same machine returns through the join flow.
    let dir = tempfile::tempdir().unwrap();
    let (running, _clock) = server(Duration::from_secs(8 * 3600));
    let url = running.url();
    let (a, b) = two_devices(dir.path(), &url);
    let b_id = b
        .device_list()
        .unwrap()
        .into_iter()
        .find(|d| d.this_device)
        .unwrap()
        .id;
    let gate = || GateInput {
        password: Some(PASSWORD.into()),
        confirmed: false,
    };
    a.device_revoke(&b_id, gate()).unwrap();

    // B types the password: refused, in the revoked words, which say what
    // helps instead.
    let e = b.account_login(&url, LOGIN, PASSWORD, None).unwrap_err();
    assert_eq!(e.code, ErrorCode::SignInNeeded, "{e:?}");
    assert_eq!(e.detail.as_deref(), Some("revoked"));
    assert_eq!(
        e.message,
        "This device was revoked from your account. Join it again from a device you still use."
    );
    // Its old token still reads the signed identity record set, so its
    // sync verifies that it is out (D73), as before.
    let e = b.sync().unwrap_err();
    assert!(e.message.contains("was revoked"), "{e:?}");

    // A is untouched: it signs in with the password and syncs.
    a.account_login(&url, LOGIN, PASSWORD, None).unwrap();
    let two = a
        .add_login("https://two.example", "me", b"pw2".to_vec(), None)
        .unwrap();
    a.sync().unwrap();

    // The same machine, set up afresh, joins from A as a new device (D246).
    let again = device(dir.path(), "b-again");
    let offered = a.link_offer().unwrap();
    again.join_start(&url, &offered.code).unwrap();
    assert_eq!(a.link_poll().unwrap().state, "compare");
    assert_eq!(again.join_poll().unwrap().state, "compare");
    a.link_approve(gate()).unwrap();
    assert_eq!(again.join_poll().unwrap().state, "approved");
    let status = again.join_finish(LOGIN, PASSWORD).unwrap();
    assert!(status.account.is_some());
    assert!(
        again.list().unwrap().iter().any(|r| r.id == two),
        "the vault came over"
    );
    let devices = a.device_list().unwrap();
    assert_eq!(devices.len(), 3, "{devices:?}");
    assert_eq!(
        devices.iter().filter(|d| d.revoked_at.is_some()).count(),
        1,
        "{devices:?}"
    );
    let new_id = again
        .device_list()
        .unwrap()
        .into_iter()
        .find(|d| d.this_device)
        .unwrap()
        .id;
    assert_ne!(new_id, b_id);
}
