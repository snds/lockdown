//! The server's half of docs/plan/27-web-server-finish.md:
//!
//! - a restore drill that also starts a real client session against the
//!   scratch copy and reads an item back, with the server's own
//!   restore-drill account, made on the live server through closed sign-ups;
//!   the clients then read the last successful restore from the server;
//! - passkeys on the admin page: added while signed in with the operator
//!   key, then a sign-in with the passkey alone; user verification, the
//!   challenge and the page's token are all required;
//! - the mirror endpoints: nothing served before a copy, the names a server
//!   gives its clients, and the web vault's policy widened to those
//!   mirrors' origins only.

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::http::{HeaderMap, Request, StatusCode, header};
use http_body_util::BodyExt;
use lockdown_passkey::encode::{b64url, from_b64url};
use lockdown_passkey::{
    Ceremony, ClientData, Flags, MakeCredential, get_assertion, make_credential,
};
use lockdown_server::backup::BackupKey;
use lockdown_server::drill_account::{LOGIN_PREFIX, READ_BACK};
use lockdown_server::drills::{Schedule, backup_now, drill_now};
use lockdown_server::keys::ServerKeys;
use lockdown_server::storage::SqlStorage;
use lockdown_server::{AppState, Config, app, run_in_background};
use tower::ServiceExt;

const HOST: &str = "localhost:8787";
const ORIGIN: &str = "https://localhost:8787";

#[test]
fn a_drill_signs_a_client_in_to_the_restored_copy_and_the_clients_read_the_result() {
    let dir = tempfile::tempdir().unwrap();
    let key_file = dir.path().join("server-key.cbor");
    let keys = ServerKeys::load_or_create(&key_file).unwrap();
    let storage = SqlStorage::open_scratch()
        .unwrap()
        .with_blob_dir(&dir.path().join("blobs"))
        .unwrap();
    let config = Config {
        signups_open: false,
        ..Config::default()
    };
    let state = Arc::new(AppState::new(Box::new(storage), keys, config));
    let key_text = BackupKey::generate().unwrap().display();
    let schedule = Arc::new(
        Schedule::new(
            &dir.path().join("backups"),
            BackupKey::parse(&key_text).unwrap(),
            &key_file,
            Duration::from_secs(86_400),
            3,
            Duration::from_secs(7 * 86_400),
        )
        .unwrap()
        .with_client_check(&dir.path().join("restore-drill")),
    );
    state.admin.set_schedule(schedule.clone());

    // Before any drill, a person's app says so in words, read from the server.
    let running = run_in_background(state.clone()).unwrap();
    let url = running.url();
    let sql = state.storage.sql().unwrap();
    sql.put_invite("sean@example.com", 0, i64::MAX).unwrap();
    let person = lockdown_app::App::start(
        lockdown_app::Paths::resolve(Some(dir.path().join("person/identity")), None).unwrap(),
        None,
        lockdown_app::LockPolicy::default(),
    );
    person.open_or_create(true).unwrap();
    person
        .account_register(&url, "sean@example.com", "a long account password", None)
        .unwrap();
    let before = person.restore_drill().unwrap();
    assert!(before.reported && before.scheduled);
    assert_eq!(before.last_success_at, None);
    assert!(
        before.words.contains("none has run yet"),
        "{}",
        before.words
    );

    // A drill on a backup older than the drill account checks the tables
    // and says the client check waits for the next backup.
    backup_now(&state, &schedule).unwrap();
    std::thread::sleep(Duration::from_millis(1100));
    schedule.ensure_client_check(&state);
    assert!(
        sql.account_overview(state.now())
            .unwrap()
            .iter()
            .any(|a| a.login.starts_with(LOGIN_PREFIX)),
        "the server made its restore-drill account through closed sign-ups"
    );
    let early = drill_now(&state, &schedule);
    assert!(early.passed, "{}", early.detail);
    assert!(
        early.detail.contains("newer than this backup"),
        "{}",
        early.detail
    );

    // The next backup holds it: the drill restores into scratch, starts a
    // server over the copy, signs the drill account's client in, syncs,
    // and reads its login back.
    std::thread::sleep(Duration::from_millis(1100));
    backup_now(&state, &schedule).unwrap();
    let report = drill_now(&state, &schedule);
    assert!(report.passed, "{}", report.detail);
    assert!(report.detail.contains(READ_BACK), "{}", report.detail);
    // Making the account again is a no-op.
    schedule.ensure_client_check(&state);
    assert_eq!(
        sql.account_overview(state.now())
            .unwrap()
            .iter()
            .filter(|a| a.login.starts_with(LOGIN_PREFIX))
            .count(),
        1
    );
    let after = person.restore_drill().unwrap();
    assert!(after.client_read_back);
    assert_eq!(after.last_passed, Some(true));
    assert!(after.last_success_at.is_some());
    assert!(after.words.contains("read an item back"), "{}", after.words);

    // A failing drill shows in the clients' words.
    std::fs::write(
        dir.path()
            .join("backups")
            .join("lockdown-29991231-235959.ldbk"),
        b"not a backup",
    )
    .unwrap();
    let failed = drill_now(&state, &schedule);
    assert!(!failed.passed);
    let words = person.restore_drill().unwrap();
    assert_eq!(words.last_passed, Some(false));
    assert!(words.words.contains("failed"), "{}", words.words);
    assert!(
        words.last_success_at.is_some(),
        "the last success is still the last success"
    );
}

// --- the admin page's passkeys -------------------------------------------------

async fn send(router: &Router, req: Request<Body>) -> (StatusCode, HeaderMap, Bytes) {
    let res = router.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    (
        status,
        headers,
        res.into_body().collect().await.unwrap().to_bytes(),
    )
}

async fn post(
    router: &Router,
    path: &str,
    cookie: Option<&str>,
    content_type: &str,
    body: String,
) -> (StatusCode, HeaderMap, Bytes) {
    let mut req = Request::builder()
        .method("POST")
        .uri(path)
        .header(header::HOST, HOST)
        .header(header::CONTENT_TYPE, content_type);
    if let Some(c) = cookie {
        req = req.header(header::COOKIE, c);
    }
    send(router, req.body(Body::from(body)).unwrap()).await
}

async fn get(router: &Router, path: &str, cookie: Option<&str>) -> (StatusCode, HeaderMap, String) {
    let mut req = Request::builder().uri(path).header(header::HOST, HOST);
    if let Some(c) = cookie {
        req = req.header(header::COOKIE, c);
    }
    let (s, h, b) = send(router, req.body(Body::empty()).unwrap()).await;
    (s, h, String::from_utf8(b.to_vec()).unwrap())
}

fn json(b: &Bytes) -> serde_json::Value {
    serde_json::from_slice(b).unwrap()
}

fn cookie_of(h: &HeaderMap) -> String {
    let set = h[header::SET_COOKIE].to_str().unwrap();
    assert!(set.contains("HttpOnly") && set.contains("Secure") && set.contains("SameSite=Strict"));
    set.split(';').next().unwrap().to_string()
}

fn csrf_of(html: &str) -> String {
    let at = html.find("data-csrf=\"").expect("the add button's token") + 11;
    html[at..at + 64].to_string()
}

fn flags(uv: bool) -> Flags {
    Flags {
        up: true,
        uv,
        be: false,
        bs: false,
    }
}

/// A software passkey answering `options` (the add ceremony).
fn create(options: &serde_json::Value, uv: bool) -> lockdown_passkey::Made {
    make_credential(&MakeCredential {
        rp_id: options["rp_id"].as_str().unwrap(),
        client_data: &ClientData {
            ceremony: Ceremony::Create,
            challenge: from_b64url(options["challenge"].as_str().unwrap()).unwrap(),
            origin: ORIGIN.into(),
            top_origin: None,
        },
        flags: flags(uv),
        algs: &[lockdown_passkey::COSE_ES256],
    })
    .unwrap()
}

#[tokio::test]
async fn the_operator_adds_a_passkey_with_the_key_and_then_signs_in_with_the_passkey_alone() {
    let state = Arc::new(AppState::in_memory(Config::default()).unwrap());
    let key = lockdown_server::admin::generate_key().unwrap();
    state.storage.sql().unwrap().set_operator_key(&key).unwrap();
    let router = app(state.clone());

    // The sign-in page carries the one script, allowed by its hash alone.
    let (s, h, html) = get(&router, "/admin", None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let csp = h[header::CONTENT_SECURITY_POLICY].to_str().unwrap();
    assert!(
        csp.contains("script-src 'sha256-") && csp.contains("connect-src 'self'"),
        "{csp}"
    );
    assert!(!csp.contains("unsafe-inline"));
    assert!(html.contains("Sign in with a passkey"));
    // No passkey yet: the passkey sign-in says how to get one.
    let (s, _, b) = post(
        &router,
        "/admin/passkey/options",
        None,
        "application/json",
        "{}".into(),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    assert!(json(&b)["error"].as_str().unwrap().contains("Passkeys"));

    // In with the operator key.
    let (s, h, _) = post(
        &router,
        "/admin/sign-in",
        None,
        "application/x-www-form-urlencoded",
        format!("key={}", key.as_str()),
    )
    .await;
    assert_eq!(s, StatusCode::SEE_OTHER);
    let cookie = cookie_of(&h);
    let (s, _, html) = get(&router, "/admin/passkeys", Some(&cookie)).await;
    assert_eq!(s, StatusCode::OK);
    assert!(html.contains("No passkeys yet"));
    let csrf = csrf_of(&html);

    // Without the page's token: refused.
    let (s, _, _) = post(
        &router,
        "/admin/passkeys/options",
        Some(&cookie),
        "application/json",
        r#"{"csrf":"00"}"#.into(),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    // A passkey that doesn't verify the person: refused.
    let (_, _, b) = post(
        &router,
        "/admin/passkeys/options",
        Some(&cookie),
        "application/json",
        format!(r#"{{"csrf":"{csrf}"}}"#),
    )
    .await;
    let options = json(&b);
    assert_eq!(options["rp_id"], "localhost");
    let weak = create(&options, false);
    let (s, _, b) = post(
        &router,
        "/admin/passkeys/add",
        Some(&cookie),
        "application/json",
        serde_json::json!({
            "csrf": csrf,
            "pending": options["pending"],
            "name": "Weak",
            "client_data_json": b64url(weak.client_data_json.as_bytes()),
            "attestation_object": b64url(&weak.attestation_object),
        })
        .to_string(),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(
        json(&b)["error"]
            .as_str()
            .unwrap()
            .contains("PIN or biometric")
    );

    // A good one is added and listed.
    let (_, _, b) = post(
        &router,
        "/admin/passkeys/options",
        Some(&cookie),
        "application/json",
        format!(r#"{{"csrf":"{csrf}"}}"#),
    )
    .await;
    let options = json(&b);
    let made = create(&options, true);
    let add = serde_json::json!({
        "csrf": csrf,
        "pending": options["pending"],
        "name": "Laptop <b>key</b>",
        "client_data_json": b64url(made.client_data_json.as_bytes()),
        "attestation_object": b64url(&made.attestation_object),
    })
    .to_string();
    let (s, _, b) = post(
        &router,
        "/admin/passkeys/add",
        Some(&cookie),
        "application/json",
        add.clone(),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{:?}", json(&b));
    // The same ceremony can't be answered twice.
    let (s, _, _) = post(
        &router,
        "/admin/passkeys/add",
        Some(&cookie),
        "application/json",
        add,
    )
    .await;
    assert_eq!(s, StatusCode::GONE);
    let (_, _, html) = get(&router, "/admin/passkeys", Some(&cookie)).await;
    assert!(
        html.contains("Laptop &lt;b&gt;key&lt;/b&gt;"),
        "names are escaped"
    );

    // Signed out, a passkey alone signs in.
    let (_, _, b) = post(
        &router,
        "/admin/passkey/options",
        None,
        "application/json",
        "{}".into(),
    )
    .await;
    let options = json(&b);
    assert_eq!(options["allow"][0], b64url(&made.credential_id));
    let answer = |uv: bool, challenge: &str, pending: &serde_json::Value| {
        let a = get_assertion(
            &made.pkcs8,
            "localhost",
            &ClientData {
                ceremony: Ceremony::Get,
                challenge: from_b64url(challenge).unwrap(),
                origin: ORIGIN.into(),
                top_origin: None,
            },
            flags(uv),
        )
        .unwrap();
        serde_json::json!({
            "pending": pending,
            "credential_id": b64url(&made.credential_id),
            "client_data_json": b64url(a.client_data_json.as_bytes()),
            "authenticator_data": b64url(&a.authenticator_data),
            "signature": b64url(&a.signature),
        })
        .to_string()
    };
    // Another challenge: refused.
    let (s, _, _) = post(
        &router,
        "/admin/passkey/sign-in",
        None,
        "application/json",
        answer(true, &b64url(b"another challenge"), &options["pending"]),
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    // Without user verification: refused.
    let (_, _, b) = post(
        &router,
        "/admin/passkey/options",
        None,
        "application/json",
        "{}".into(),
    )
    .await;
    let options = json(&b);
    let (s, _, _) = post(
        &router,
        "/admin/passkey/sign-in",
        None,
        "application/json",
        answer(
            false,
            options["challenge"].as_str().unwrap(),
            &options["pending"],
        ),
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    // The real answer: a cookie that opens the page.
    let (_, _, b) = post(
        &router,
        "/admin/passkey/options",
        None,
        "application/json",
        "{}".into(),
    )
    .await;
    let options = json(&b);
    let (s, h, _) = post(
        &router,
        "/admin/passkey/sign-in",
        None,
        "application/json",
        answer(
            true,
            options["challenge"].as_str().unwrap(),
            &options["pending"],
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let by_passkey = cookie_of(&h);
    let (s, _, html) = get(&router, "/admin", Some(&by_passkey)).await;
    assert_eq!(s, StatusCode::OK);
    assert!(html.contains("Overview"));
    let events = state
        .storage
        .sql()
        .unwrap()
        .events(Some("sign_in"), 10)
        .unwrap();
    assert!(
        events[0].ok && events[0].detail.contains("passkey"),
        "{:?}",
        events[0]
    );
    assert!(events.iter().any(|e| !e.ok && e.detail.contains("passkey")));

    // Clearing the operator key closes the page to passkeys too.
    state.storage.sql().unwrap().clear_operator_key().unwrap();
    let (s, _, _) = post(
        &router,
        "/admin/passkey/options",
        None,
        "application/json",
        "{}".into(),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}

// --- mirrors ------------------------------------------------------------------

#[tokio::test]
async fn the_mirror_endpoints_and_the_web_vaults_policy() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("index.html"), "<!doctype html>").unwrap();
    let config = Config {
        web_vault: Some(dir.path().to_path_buf()),
        ..Config::default()
    };
    let state = Arc::new(AppState::in_memory(config).unwrap());
    let router = app(state.clone());
    // Not a mirror: nothing served.
    let (s, _, _) = get(&router, "/v1/kt/mirror", None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    // No mirrors named: an empty list, and the vault's policy unchanged.
    let req = Request::builder()
        .uri("/v1/kt/mirrors")
        .header(header::HOST, HOST)
        .body(Body::empty())
        .unwrap();
    let (s, _, b) = send(&router, req).await;
    assert_eq!(s, StatusCode::OK);
    let none: lockdown_sync::mirror::KtMirrors = lockdown_sync::wire::decode(&b).unwrap();
    assert!(none.mirrors.is_empty());
    let (_, h, _) = get(&router, "/vault/", None).await;
    assert!(
        h[header::CONTENT_SECURITY_POLICY]
            .to_str()
            .unwrap()
            .contains("connect-src 'self';")
    );
    state
        .mirrors
        .set_named(&[
            "https://mirror.example/".into(),
            "https://m2.example:8443".into(),
        ])
        .unwrap();
    let req = Request::builder()
        .uri("/v1/kt/mirrors")
        .header(header::HOST, HOST)
        .body(Body::empty())
        .unwrap();
    let (_, _, b) = send(&router, req).await;
    let named: lockdown_sync::mirror::KtMirrors = lockdown_sync::wire::decode(&b).unwrap();
    assert_eq!(
        named.mirrors,
        vec!["https://mirror.example", "https://m2.example:8443"]
    );
    let (_, h, _) = get(&router, "/vault/", None).await;
    let csp = h[header::CONTENT_SECURITY_POLICY].to_str().unwrap();
    assert!(
        csp.contains("connect-src 'self' https://mirror.example https://m2.example:8443;"),
        "{csp}"
    );
    assert!(csp.contains("default-src 'none'") && !csp.contains('*'));
}
