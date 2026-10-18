//! Monitored addresses through the endpoints (docs/plan/23-bridges.md §9):
//! an account replaces its list of watched aliases and reads it back,
//! lowercase, sorted and folded; a list that is too long or holds a line
//! that isn't an address is refused whole; another account never sees it;
//! the calls need a session.

use std::sync::Arc;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use lockdown_crypto::SigningKey;
use lockdown_crypto::auth::{LoginClient, RegistrationClient, device_pop_message};
use lockdown_server::{AppState, Config, app};
use lockdown_sync::wire::{
    self, CONTENT_TYPE, DeviceProofWire, LoginFinish, LoginFinishReply, LoginStart,
    LoginStartReply, MonitoredAddresses, RegisterFinish, RegisterStart, RegisterStartReply,
};
use lockdown_vault::Id;
use tower::ServiceExt;

struct Device {
    id: Id,
    key: SigningKey,
}

impl Device {
    fn new() -> Self {
        Self {
            id: Id::new(),
            key: SigningKey::generate().unwrap(),
        }
    }
}

async fn call(
    router: &Router,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Vec<u8>,
) -> (StatusCode, Bytes) {
    let mut req = Request::builder()
        .method(method)
        .uri(path)
        .header(header::CONTENT_TYPE, CONTENT_TYPE);
    if let Some(t) = token {
        req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    let res = router
        .clone()
        .oneshot(req.body(Body::from(body)).unwrap())
        .await
        .unwrap();
    let status = res.status();
    (status, res.into_body().collect().await.unwrap().to_bytes())
}

async fn register(router: &Router, login: &str, password: &str) {
    let (client, request) = RegistrationClient::start(password).unwrap();
    let (_, bytes) = call(
        router,
        "POST",
        "/v1/account/register/start",
        None,
        wire::encode(&RegisterStart {
            login: login.into(),
            request,
        })
        .unwrap(),
    )
    .await;
    let reply: RegisterStartReply = wire::decode(&bytes).unwrap();
    let (upload, _) = client.finish(password, &reply.response).unwrap();
    let (status, _) = call(
        router,
        "POST",
        "/v1/account/register/finish",
        None,
        wire::encode(&RegisterFinish {
            login: login.into(),
            profile_id: Id::new(),
            upload,
        })
        .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
}

async fn login(router: &Router, login: &str, password: &str, device: &Device) -> String {
    let (client, request) = LoginClient::start(password).unwrap();
    let (_, bytes) = call(
        router,
        "POST",
        "/v1/account/login/start",
        None,
        wire::encode(&LoginStart {
            login: login.into(),
            request,
        })
        .unwrap(),
    )
    .await;
    let started: LoginStartReply = wire::decode(&bytes).unwrap();
    let finished = client.finish(password, &started.response).unwrap();
    let pop = device
        .key
        .sign(&device_pop_message(login, &started.login_id));
    let (status, bytes) = call(
        router,
        "POST",
        "/v1/account/login/finish",
        None,
        wire::encode(&LoginFinish {
            login_id: started.login_id,
            finalization: finished.finalization,
            device: DeviceProofWire {
                device_id: device.id,
                device_pub: device.key.verifying_key().to_bytes(),
                pop: pop.to_vec(),
            },
        })
        .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    wire::decode::<LoginFinishReply>(&bytes).unwrap().token
}

#[tokio::test]
async fn an_account_replaces_and_reads_its_watched_addresses() {
    let state = Arc::new(AppState::in_memory(Config::default()).unwrap());
    let router = app(state);
    register(&router, "sean@example.com", "pw-one").await;
    register(&router, "robin@example.com", "pw-two").await;
    let sean = login(&router, "sean@example.com", "pw-one", &Device::new()).await;
    let robin = login(&router, "robin@example.com", "pw-two", &Device::new()).await;
    let put = |list: &[&str]| {
        wire::encode(&MonitoredAddresses {
            addresses: list.iter().map(|s| s.to_string()).collect(),
        })
        .unwrap()
    };
    let (status, bytes) = call(
        &router,
        "PUT",
        "/v1/account/monitored",
        Some(&sean),
        put(&["Shop.X1@SimpleLogin.com", "b@duck.com", "b@duck.com"]),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let back: MonitoredAddresses = wire::decode(&bytes).unwrap();
    assert_eq!(back.addresses, ["b@duck.com", "shop.x1@simplelogin.com"]);
    let (_, bytes) = call(
        &router,
        "GET",
        "/v1/account/monitored",
        Some(&sean),
        Vec::new(),
    )
    .await;
    assert_eq!(wire::decode::<MonitoredAddresses>(&bytes).unwrap(), back);
    // Another account has its own (empty) list.
    let (_, bytes) = call(
        &router,
        "GET",
        "/v1/account/monitored",
        Some(&robin),
        Vec::new(),
    )
    .await;
    assert!(
        wire::decode::<MonitoredAddresses>(&bytes)
            .unwrap()
            .addresses
            .is_empty()
    );
    // Refused whole: a line that isn't an address, or too many.
    let (status, _) = call(
        &router,
        "PUT",
        "/v1/account/monitored",
        Some(&sean),
        put(&["ok@duck.com", "nope"]),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let many: Vec<String> = (0..=wire::MAX_MONITORED)
        .map(|i| format!("a{i}@duck.com"))
        .collect();
    let refs: Vec<&str> = many.iter().map(String::as_str).collect();
    let (status, _) = call(
        &router,
        "PUT",
        "/v1/account/monitored",
        Some(&sean),
        put(&refs),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (_, bytes) = call(
        &router,
        "GET",
        "/v1/account/monitored",
        Some(&sean),
        Vec::new(),
    )
    .await;
    assert_eq!(
        wire::decode::<MonitoredAddresses>(&bytes).unwrap(),
        back,
        "nothing changed"
    );
    // An empty list stops it; no session, no answer.
    let (status, bytes) = call(
        &router,
        "PUT",
        "/v1/account/monitored",
        Some(&sean),
        put(&[]),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        wire::decode::<MonitoredAddresses>(&bytes)
            .unwrap()
            .addresses
            .is_empty()
    );
    let (status, _) = call(&router, "GET", "/v1/account/monitored", None, Vec::new()).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[test]
fn an_operators_range_file_raises_breach_alerts_without_a_paid_key() {
    use lockdown_server::alerts::AlertStore;
    use lockdown_server::breach_monitor::scan_once;
    use lockdown_server::breach_range::{Catalog, RangeBreachFeed};
    use lockdown_server::storage::{SqlStorage, Storage};

    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let feed = RangeBreachFeed::new(
        fixtures.join("breach-range.txt").to_str().unwrap(),
        Catalog::load(&fixtures.join("breaches.json")).unwrap(),
    )
    .unwrap();
    let s = SqlStorage::open_scratch().unwrap();
    let sean = s
        .create_account("sean@example.com", lockdown_vault::Id::new(), b"pf")
        .unwrap();
    let device = lockdown_vault::Id::new();
    s.register_device(sean.account_id, device, &[1; 32])
        .unwrap();
    s.set_monitored(sean.account_id, &["alias.42@example.net".to_string()])
        .unwrap();
    s.create_account("clean@example.com", lockdown_vault::Id::new(), b"pf")
        .unwrap();
    let report = scan_once(&s, &feed, 100, std::time::Duration::ZERO).unwrap();
    assert_eq!((report.checked, report.alerts, report.failed), (3, 3, 0));
    let alerts = s.alerts_for(sean.account_id, device, 0).unwrap();
    let seen: Vec<(String, String, Option<String>)> = alerts
        .iter()
        .map(|a| {
            let b = a.breach.as_ref().unwrap();
            (b.address.clone(), b.name.clone(), b.domain.clone())
        })
        .collect();
    assert_eq!(
        seen,
        [
            (
                "sean@example.com".into(),
                "ExampleShop".into(),
                Some("shop.example".into())
            ),
            (
                "sean@example.com".into(),
                "ForumLeak".into(),
                Some("forum.example".into())
            ),
            (
                "alias.42@example.net".into(),
                "ForumLeak".into(),
                Some("forum.example".into())
            ),
        ]
    );
    assert_eq!(
        scan_once(&s, &feed, 200, std::time::Duration::ZERO)
            .unwrap()
            .alerts,
        0,
        "each breach once per address"
    );
}
