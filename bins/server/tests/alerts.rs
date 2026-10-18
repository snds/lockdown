//! Account alerts through the endpoints: a new device's sign-in reaches
//! the account's other devices and not itself, a known device signing in
//! again is not news, a dismissal holds for every device, and the calls
//! need a session.

use std::sync::Arc;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use lockdown_crypto::SigningKey;
use lockdown_crypto::auth::{LoginClient, RegistrationClient, device_pop_message};
use lockdown_server::{AppState, Config, app};
use lockdown_sync::wire::{
    self, AlertKind, AlertsSince, CONTENT_TYPE, DeviceProofWire, LoginFinish, LoginFinishReply,
    LoginStart, LoginStartReply, RegisterFinish, RegisterStart, RegisterStartReply,
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

async fn alerts(router: &Router, token: &str, since: u64) -> AlertsSince {
    let (status, bytes) = call(
        router,
        "GET",
        &format!("/v1/account/alerts?since={since}"),
        Some(token),
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    wire::decode(&bytes).unwrap()
}

#[tokio::test]
async fn a_new_device_is_news_to_the_others_once_and_a_dismissal_holds_for_all() {
    let router = app(Arc::new(AppState::in_memory(Config::default()).unwrap()));
    register(&router, "sean@example.com", "pw one two three").await;
    let laptop = Device::new();
    let first = login(&router, "sean@example.com", "pw one two three", &laptop).await;
    // The first device is nobody's news.
    assert!(alerts(&router, &first, 0).await.alerts.is_empty());

    let phone = Device::new();
    let second = login(&router, "sean@example.com", "pw one two three", &phone).await;
    let seen = alerts(&router, &first, 0).await;
    assert_eq!(seen.alerts.len(), 1);
    assert_eq!(seen.alerts[0].kind, AlertKind::NewDevice);
    assert_eq!(seen.alerts[0].device_id, Some(phone.id));
    assert!(!seen.alerts[0].dismissed);
    assert_eq!(seen.current_seq, 1);
    // The new device doesn't hear about itself.
    assert!(alerts(&router, &second, 0).await.alerts.is_empty());
    // Signing in again on a known device is not news.
    let again = login(&router, "sean@example.com", "pw one two three", &laptop).await;
    assert_eq!(alerts(&router, &again, 0).await.alerts.len(), 1);
    assert!(alerts(&router, &again, 1).await.alerts.is_empty());

    // Dismissed on the laptop is dismissed for the account.
    let (status, _) = call(
        &router,
        "POST",
        "/v1/account/alerts/1/dismiss",
        Some(&first),
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(alerts(&router, &first, 0).await.alerts[0].dismissed);
    let (status, _) = call(
        &router,
        "POST",
        "/v1/account/alerts/9/dismiss",
        Some(&first),
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Another account sees none of it and can't dismiss it.
    register(&router, "other@example.com", "another password here").await;
    let other = login(
        &router,
        "other@example.com",
        "another password here",
        &Device::new(),
    )
    .await;
    assert!(alerts(&router, &other, 0).await.alerts.is_empty());
    let (status, _) = call(
        &router,
        "POST",
        "/v1/account/alerts/1/dismiss",
        Some(&other),
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Both calls need a session.
    let (status, _) = call(&router, "GET", "/v1/account/alerts", None, Vec::new()).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = call(
        &router,
        "POST",
        "/v1/account/alerts/1/dismiss",
        None,
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
