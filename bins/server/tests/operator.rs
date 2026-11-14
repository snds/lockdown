//! The operator's side of the second self-host pass (docs/plan/25-self-host-2.md):
//!
//! - a device new to an account waits for the account's email: the link's
//!   page changes nothing until Approve is pressed, "This wasn't me" stops
//!   the sign-in, approval mail is capped per hour, and the new device's
//!   sign-in and a security key's removal are mailed and alerted;
//! - the admin page needs the operator key, forms need their session's
//!   token, invites let one login name through closed sign-ups once, and
//!   the operator can end an account's sessions;
//! - scheduled backups and restore drills: a good backup passes, a changed
//!   one and another server's fail;
//! - the SMTP client against a relay in this process, plain on this
//!   machine and with STARTTLS, its password only inside TLS.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::http::{HeaderMap, Request, StatusCode, header};
use http_body_util::BodyExt;
use lockdown_crypto::SigningKey;
use lockdown_crypto::auth::{LoginClient, RegistrationClient, device_pop_message};
use lockdown_server::keys::ServerKeys;
use lockdown_server::mail::{Mail, MailSettings, Mailer, Outbox, SmtpConfig, SmtpMailer};
use lockdown_server::storage::{SqlStorage, Storage};
use lockdown_server::{AppState, Config, app};
use lockdown_sync::account_security::{SecondFactorPoll, SecondFactorStatus};
use lockdown_sync::wire::{
    self, AlertsSince, CONTENT_TYPE, DeviceProofWire, LoginFinish, LoginFinishReply, LoginStart,
    LoginStartReply, RegisterFinish, RegisterStart, RegisterStartReply,
};
use lockdown_vault::Id;
use serde_bytes::ByteBuf;
use tower::ServiceExt;

const PASSWORD: &str = "pw one two three";

#[derive(Clone, Default)]
struct Inbox(Arc<Mutex<Vec<Mail>>>);

impl Mailer for Inbox {
    fn send(&self, mail: &Mail) -> Result<(), String> {
        self.0.lock().unwrap().push(mail.clone());
        Ok(())
    }
}

impl Inbox {
    /// Waits for the outbox's thread to deliver `n` messages in all.
    fn wait_for(&self, n: usize) -> Vec<Mail> {
        for _ in 0..200 {
            let got = self.0.lock().unwrap().clone();
            if got.len() >= n {
                return got;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("expected {n} messages, got {:?}", self.0.lock().unwrap());
    }
}

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
        .header(header::CONTENT_TYPE, CONTENT_TYPE)
        .header(header::USER_AGENT, "lockdown/0.1.0 (linux)");
    if let Some(t) = token {
        req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    let (s, _, b) = send(router, req.body(Body::from(body)).unwrap()).await;
    (s, b)
}

async fn register(router: &Router, login: &str) -> StatusCode {
    let (client, request) = RegistrationClient::start(PASSWORD).unwrap();
    let (status, bytes) = call(
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
    if status != StatusCode::OK {
        return status;
    }
    let reply: RegisterStartReply = wire::decode(&bytes).unwrap();
    let (upload, _) = client.finish(PASSWORD, &reply.response).unwrap();
    call(
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
    .await
    .0
}

async fn login(
    router: &Router,
    login: &str,
    device: &Device,
) -> (StatusCode, Option<LoginFinishReply>) {
    let (client, request) = LoginClient::start(PASSWORD).unwrap();
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
    let finished = client.finish(PASSWORD, &started.response).unwrap();
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
    (
        status,
        (status == StatusCode::OK).then(|| wire::decode(&bytes).unwrap()),
    )
}

async fn poll(
    router: &Router,
    pending: Id,
    key: &ByteBuf,
) -> (StatusCode, Option<SecondFactorStatus>) {
    let (status, bytes) = call(
        router,
        "POST",
        "/v1/account/login/second-factor/poll",
        None,
        wire::encode(&SecondFactorPoll {
            pending_id: pending,
            poll_key: key.clone(),
        })
        .unwrap(),
    )
    .await;
    (
        status,
        (status == StatusCode::OK).then(|| wire::decode(&bytes).unwrap()),
    )
}

async fn page(
    router: &Router,
    method: &str,
    path: &str,
    cookie: Option<&str>,
    form: &str,
) -> (StatusCode, HeaderMap, String) {
    let mut req = Request::builder().method(method).uri(path);
    if method == "POST" {
        req = req.header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
    }
    if let Some(c) = cookie {
        req = req.header(header::COOKIE, c);
    }
    let (s, h, b) = send(router, req.body(Body::from(form.to_string())).unwrap()).await;
    (s, h, String::from_utf8(b.to_vec()).unwrap())
}

fn mailing_state(config: Config, inbox: &Inbox, verify: bool) -> AppState {
    let mut state = AppState::in_memory(config).unwrap();
    state.mail = Some(
        MailSettings::new(
            Outbox::start(Box::new(inbox.clone())).unwrap(),
            "https://vault.example",
            verify,
            "a test inbox".into(),
        )
        .unwrap(),
    );
    state
}

fn link_of(mail: &Mail) -> (Id, String) {
    let line = mail
        .body
        .lines()
        .find(|l| l.starts_with("https://vault.example/approve/"))
        .expect("the link");
    let rest = line.trim_start_matches("https://vault.example/approve/");
    let (id, code) = rest.split_once("?code=").unwrap();
    (Id::parse(id).unwrap(), code.to_string())
}

#[tokio::test]
async fn a_new_device_waits_for_the_accounts_email_and_only_approve_lets_it_in() {
    let inbox = Inbox::default();
    let config = Config {
        per_login: (100, 10.0),
        ..Config::default()
    };
    let router = app(Arc::new(mailing_state(config, &inbox, true)));
    assert_eq!(
        register(&router, "sean@example.com").await,
        StatusCode::CREATED
    );
    // The account's first device has nothing to be approved from.
    let laptop = Device::new();
    let (_, first) = login(&router, "sean@example.com", &laptop).await;
    let first = first.unwrap();
    assert!(first.second_factor.is_none() && !first.token.is_empty());
    assert!(inbox.0.lock().unwrap().is_empty());

    // A second device waits; the link goes to the account's address.
    let phone = Device::new();
    let (_, waiting) = login(&router, "sean@example.com", &phone).await;
    let waiting = waiting.unwrap();
    assert!(waiting.token.is_empty());
    let pending = waiting.second_factor.unwrap();
    assert_eq!(
        pending.request.email_to.as_deref(),
        Some("s•••@example.com")
    );
    assert!(pending.request.allow.is_empty());
    let mail = inbox.wait_for(1)[0].clone();
    assert_eq!(
        (mail.kind, mail.to.as_str()),
        ("verify_device", "sean@example.com")
    );
    assert!(mail.body.contains("lockdown/0.1.0 (linux)"));
    assert!(mail.body.contains("https://vault.example/vault/#/devices"));
    assert!(!mail.body.contains(PASSWORD));
    let (id, code) = link_of(&mail);
    assert_eq!(id, pending.pending_id);
    assert!(
        poll(&router, id, &pending.poll_key)
            .await
            .1
            .unwrap()
            .session
            .is_none()
    );

    // A wrong code shows nothing; the right one shows the device and a
    // choice, and opening it changes nothing.
    let (s, _, _) = page(
        &router,
        "GET",
        &format!("/approve/{id}?code={}", "0".repeat(64)),
        None,
        "",
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, h, html) = page(
        &router,
        "GET",
        &format!("/approve/{id}?code={code}"),
        None,
        "",
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert!(html.contains("sean@example.com") && html.contains(">Approve<"));
    assert!(
        h[header::CONTENT_SECURITY_POLICY]
            .to_str()
            .unwrap()
            .starts_with("default-src 'none'")
    );
    assert!(!html.contains("<script"));
    assert!(
        poll(&router, id, &pending.poll_key)
            .await
            .1
            .unwrap()
            .session
            .is_none()
    );
    // Another device's poll key can't collect it.
    assert_eq!(
        poll(&router, id, &ByteBuf::from(vec![0u8; 32])).await.0,
        StatusCode::FORBIDDEN
    );
    let (s, _, html) = page(
        &router,
        "POST",
        &format!("/approve/{id}"),
        None,
        &format!("code={code}&answer=approve"),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{html}");
    let session = poll(&router, id, &pending.poll_key)
        .await
        .1
        .unwrap()
        .session
        .unwrap();
    assert!(!session.token.is_empty());
    // It is now a known device: no wait the next time.
    let (_, again) = login(&router, "sean@example.com", &phone).await;
    assert!(again.unwrap().second_factor.is_none());
    // Its arrival is mailed and alerted like any new device's.
    let mails = inbox.wait_for(2);
    assert_eq!(mails[1].kind, "new_device");
    let (_, bytes) = call(
        &router,
        "GET",
        "/v1/account/alerts?since=0",
        Some(&first.token),
        Vec::new(),
    )
    .await;
    let alerts: AlertsSince = wire::decode(&bytes).unwrap();
    assert_eq!(alerts.alerts.len(), 1);

    // "This wasn't me" stops a sign-in for good.
    let thief = Device::new();
    let (_, w) = login(&router, "sean@example.com", &thief).await;
    let p = w.unwrap().second_factor.unwrap();
    let (tid, tcode) = link_of(&inbox.wait_for(3)[2]);
    let (_, _, html) = page(
        &router,
        "POST",
        &format!("/approve/{tid}"),
        None,
        &format!("code={tcode}&answer=deny"),
    )
    .await;
    assert!(html.contains("Sign-in stopped"));
    assert_eq!(
        poll(&router, tid, &p.poll_key).await.0,
        StatusCode::NOT_FOUND
    );
    let (s, _, _) = page(
        &router,
        "POST",
        &format!("/approve/{tid}"),
        None,
        &format!("code={tcode}&answer=approve"),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::NOT_FOUND,
        "a stopped sign-in can't be approved after"
    );

    // Approval mail is capped per account per hour.
    for _ in 0..3 {
        let (s, _) = login(&router, "sean@example.com", &Device::new()).await;
        assert_eq!(s, StatusCode::OK);
    }
    let (s, _) = login(&router, "sean@example.com", &Device::new()).await;
    assert_eq!(s, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn with_approval_off_new_devices_sign_in_and_are_only_mailed_about() {
    let inbox = Inbox::default();
    let router = app(Arc::new(mailing_state(Config::default(), &inbox, false)));
    register(&router, "sean@example.com").await;
    login(&router, "sean@example.com", &Device::new()).await;
    let (_, r) = login(&router, "sean@example.com", &Device::new()).await;
    assert!(r.unwrap().second_factor.is_none());
    assert_eq!(inbox.wait_for(1)[0].kind, "new_device");
    // A login name that isn't an address is never mailed.
    register(&router, "just-a-name").await;
    login(&router, "just-a-name", &Device::new()).await;
    let (_, r) = login(&router, "just-a-name", &Device::new()).await;
    assert!(r.unwrap().second_factor.is_none());
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(inbox.0.lock().unwrap().len(), 1);
}

fn cookie_from(h: &HeaderMap) -> String {
    let set = h[header::SET_COOKIE].to_str().unwrap();
    assert!(set.contains("HttpOnly") && set.contains("Secure") && set.contains("SameSite=Strict"));
    set.split(';').next().unwrap().to_string()
}

fn csrf_of(html: &str) -> String {
    let at = html.find("name=\"csrf\" value=\"").expect("a form token") + 19;
    html[at..at + 64].to_string()
}

#[tokio::test]
async fn the_admin_page_needs_the_operator_key_and_invites_open_closed_signups_once() {
    let config = Config {
        signups_open: false,
        ..Config::default()
    };
    let state = AppState::in_memory(config).unwrap();
    let router_closed = app(Arc::new(AppState::in_memory(Config::default()).unwrap()));
    // No key yet: the page is closed and says how to open it.
    let (s, _, html) = page(&router_closed, "GET", "/admin", None, "").await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    assert!(html.contains("admin set-key"));

    let key = lockdown_server::admin::generate_key().unwrap();
    state.storage.sql().unwrap().set_operator_key(&key).unwrap();
    let state = Arc::new(state);
    let router = app(state.clone());
    let (s, _, html) = page(&router, "GET", "/admin", None, "").await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    assert!(html.contains("Operator key"));
    let (s, _, _) = page(
        &router,
        "POST",
        "/admin/sign-in",
        None,
        "key=LDOP-WRONG-WRONG-WRONG-WRONG",
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, h, _) = page(
        &router,
        "POST",
        "/admin/sign-in",
        None,
        &format!("key={}", key.as_str()),
    )
    .await;
    assert_eq!(s, StatusCode::SEE_OTHER);
    let cookie = cookie_from(&h);
    let (s, h, html) = page(&router, "GET", "/admin", Some(&cookie), "").await;
    assert_eq!(s, StatusCode::OK);
    assert!(html.contains("Setup checks") && html.contains("Sign-ups"));
    assert_eq!(h[header::X_FRAME_OPTIONS], "DENY");
    assert_eq!(h[header::CACHE_CONTROL], "no-store");
    // A made-up cookie is nobody.
    let (s, _, _) = page(
        &router,
        "GET",
        "/admin/accounts",
        Some("__Host-lockdown-admin=00"),
        "",
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    // Closed sign-ups refuse a stranger; an invite lets that one name in, once.
    assert_eq!(
        register(&router, "new@example.com").await,
        StatusCode::FORBIDDEN
    );
    let (_, _, html) = page(&router, "GET", "/admin/invites", Some(&cookie), "").await;
    let csrf = csrf_of(&html);
    let (_, _, html) = page(
        &router,
        "POST",
        "/admin/invites",
        Some(&cookie),
        "login=new%40example.com&days=3",
    )
    .await;
    assert!(
        html.contains("old page"),
        "a form without its token changes nothing"
    );
    let (_, _, html) = page(
        &router,
        "POST",
        "/admin/invites",
        Some(&cookie),
        &format!("csrf={csrf}&login=New%40Example.com&days=3"),
    )
    .await;
    assert!(html.contains("may register for 3 day(s)"), "{html}");
    assert_eq!(
        register(&router, "other@example.com").await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        register(&router, "new@example.com").await,
        StatusCode::CREATED
    );
    let (_, _, html) = page(&router, "GET", "/admin/invites", Some(&cookie), "").await;
    assert!(html.contains("used "));

    // The account appears with counts, never secrets.
    let device = Device::new();
    let (_, r) = login(&router, "new@example.com", &device).await;
    let token = r.unwrap().token;
    let (_, _, html) = page(&router, "GET", "/admin/accounts", Some(&cookie), "").await;
    assert!(html.contains("new@example.com"));
    let account = state
        .storage
        .account_by_login("new@example.com")
        .unwrap()
        .unwrap();
    let (_, _, html) = page(
        &router,
        "GET",
        &format!("/admin/accounts/{}", account.account_id),
        Some(&cookie),
        "",
    )
    .await;
    assert!(html.contains(&device.id.to_string()));
    assert!(html.contains("lockdown/0.1.0 (linux)"));
    assert!(!html.contains(&token));
    // The operator ends its sessions: the token stops at once.
    let csrf = csrf_of(&html);
    let (s, _, _) = page(
        &router,
        "POST",
        &format!("/admin/accounts/{}/end-sessions", account.account_id),
        Some(&cookie),
        &format!("csrf={csrf}"),
    )
    .await;
    assert_eq!(s, StatusCode::SEE_OTHER);
    let (s, _) = call(
        &router,
        "GET",
        "/v1/account/alerts?since=0",
        Some(&token),
        Vec::new(),
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    // Signing out ends the cookie.
    let (_, _, html) = page(&router, "GET", "/admin", Some(&cookie), "").await;
    let csrf = csrf_of(&html);
    page(
        &router,
        "POST",
        "/admin/sign-out",
        Some(&cookie),
        &format!("csrf={csrf}"),
    )
    .await;
    let (s, _, _) = page(&router, "GET", "/admin", Some(&cookie), "").await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

/// The manifest a test serves, and every address asked for.
struct Manifest(&'static str, Mutex<Vec<String>>);

impl lockdown_server::updates::ManifestSource for Manifest {
    fn fetch(&self, url: &str) -> Result<Vec<u8>, String> {
        self.1.lock().unwrap().push(url.to_string());
        Ok(self.0.as_bytes().to_vec())
    }
}

async fn signed_in_router(state: AppState) -> (Router, String) {
    let key = lockdown_server::admin::generate_key().unwrap();
    state.storage.sql().unwrap().set_operator_key(&key).unwrap();
    let router = app(Arc::new(state));
    let (_, h, _) = page(
        &router,
        "POST",
        "/admin/sign-in",
        None,
        &format!("key={}", key.as_str()),
    )
    .await;
    (router, cookie_from(&h))
}

#[tokio::test]
async fn the_admin_page_checks_for_updates_and_shows_the_command_for_a_newer_build() {
    use lockdown_server::updates::{FEED, UPDATE_COMMAND, Updates};
    let source = Arc::new(Manifest(
        r#"{"version":"0.1.0-main.20261109120000","platforms":{}}"#,
        Mutex::default(),
    ));
    let mut state = AppState::in_memory(Config::default()).unwrap();
    state.updates = Updates::new(
        semver::Version::parse("0.1.0-main.20261108090000").unwrap(),
        Some(source.clone()),
    );
    let (router, cookie) = signed_in_router(state).await;

    // Before any check: the version, the channel, the switch on.
    let (_, _, html) = page(&router, "GET", "/admin", Some(&cookie), "").await;
    assert!(html.contains("Lockdown server 0.1.0-main.20261108090000</b> · main channel"));
    assert!(html.contains("checks for updates when it starts and every six hours"));
    assert!(html.contains("name=\"auto\" value=\"on\" checked"));
    let csrf = csrf_of(&html);

    // A form from another page checks nothing.
    let (s, h, _) = page(
        &router,
        "POST",
        "/admin/updates/check",
        Some(&cookie),
        "csrf=00",
    )
    .await;
    assert_eq!(s, StatusCode::SEE_OTHER);
    assert_eq!(h[header::LOCATION], "/admin#updates");
    assert!(source.1.lock().unwrap().is_empty());
    // Signed out, nothing either.
    let (s, _, _) = page(
        &router,
        "POST",
        "/admin/updates/check",
        None,
        &format!("csrf={csrf}"),
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    assert!(source.1.lock().unwrap().is_empty());

    // Check for updates: one read of the main channel's manifest.
    page(
        &router,
        "POST",
        "/admin/updates/check",
        Some(&cookie),
        &format!("csrf={csrf}"),
    )
    .await;
    assert_eq!(
        *source.1.lock().unwrap(),
        vec![FEED.main_endpoint.to_string()]
    );
    let (_, _, html) = page(&router, "GET", "/admin", Some(&cookie), "").await;
    assert!(html.contains("Version 0.1.0-main.20261109120000 is out."));
    assert!(html.contains(&UPDATE_COMMAND.replace('&', "&amp;")));
    assert!(html.contains("ghcr.io/snds/lockdown-server:0.1.0-main.20261109120000"));
    // No host helper set up: the command, and where to install the helper.
    assert!(html.contains(lockdown_server::helper::INSTALL_STEPS));
    assert!(!html.contains("/admin/updates/reload"));
    assert!(html.contains("Last checked"));

    // The switch: off, then kept off.
    page(
        &router,
        "POST",
        "/admin/updates/auto",
        Some(&cookie),
        &format!("csrf={csrf}"),
    )
    .await;
    let (_, _, html) = page(&router, "GET", "/admin", Some(&cookie), "").await;
    assert!(html.contains("name=\"auto\" value=\"on\">"));
    page(
        &router,
        "POST",
        "/admin/updates/auto",
        Some(&cookie),
        &format!("csrf={csrf}&auto=on"),
    )
    .await;
    let (_, _, html) = page(&router, "GET", "/admin", Some(&cookie), "").await;
    assert!(html.contains("name=\"auto\" value=\"on\" checked"));
}

/// A host helper that answers as told and remembers each request.
struct Helper {
    refuse: Option<host_helper::ClientError>,
    asked: Mutex<Vec<host_helper::UpdateRequest>>,
    restarts: Mutex<usize>,
}

impl lockdown_server::helper::HostHelper for Helper {
    fn update(
        &self,
        request: &host_helper::UpdateRequest,
    ) -> Result<host_helper::UpdateReply, host_helper::ClientError> {
        self.asked.lock().unwrap().push(request.clone());
        match &self.refuse {
            Some(e) => Err(e.clone()),
            None => Ok(host_helper::UpdateReply {
                stack: "lockdown".into(),
                pulled: format!(
                    "ghcr.io/snds/lockdown-server:{}",
                    request.tag.clone().unwrap_or_default()
                ),
                digest: format!("sha256:{}", "1".repeat(64)),
                services: vec!["lockdown".into()],
                images: Vec::new(),
                before: None,
                after: None,
            }),
        }
    }

    fn restart(&self) -> Result<host_helper::RestartReply, host_helper::ClientError> {
        *self.restarts.lock().unwrap() += 1;
        match &self.refuse {
            Some(e) => Err(e.clone()),
            None => Ok(host_helper::RestartReply {
                stack: "lockdown".into(),
                services: vec!["lockdown".into()],
            }),
        }
    }
}

/// A signed-in router over `state`, keeping the state to look at.
async fn signed_in_keeping(state: AppState) -> (Router, String, Arc<AppState>) {
    let key = lockdown_server::admin::generate_key().unwrap();
    state.storage.sql().unwrap().set_operator_key(&key).unwrap();
    let state = Arc::new(state);
    let router = app(state.clone());
    let (_, h, _) = page(
        &router,
        "POST",
        "/admin/sign-in",
        None,
        &format!("key={}", key.as_str()),
    )
    .await;
    (router, cookie_from(&h), state)
}

/// Waits for the helper's thread to record how the action ended.
fn settled(state: &AppState) -> lockdown_server::helper::Progress {
    for _ in 0..200 {
        if let Some(a) = state.updates.action()
            && a.progress != lockdown_server::helper::Progress::Working
        {
            return a.progress;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the helper's thread never finished");
}

#[tokio::test]
async fn update_and_reload_ask_the_host_helper_with_the_token_and_land_in_the_log() {
    use lockdown_server::helper::{Progress, Setting};
    use lockdown_server::updates::Updates;
    let newer = "0.1.0-main.20261109120000";
    let source = Arc::new(Manifest(
        r#"{"version":"0.1.0-main.20261109120000"}"#,
        Mutex::default(),
    ));
    let helper = Arc::new(Helper {
        refuse: None,
        asked: Mutex::default(),
        restarts: Mutex::default(),
    });
    let mut state = AppState::in_memory(Config::default()).unwrap();
    state.updates = Updates::new(
        semver::Version::parse("0.1.0-main.20261108090000").unwrap(),
        Some(source),
    );
    state.updates.set_helper(Setting::Ready(helper.clone()));
    let (router, cookie, state) = signed_in_keeping(state).await;
    let (_, _, html) = page(&router, "GET", "/admin", Some(&cookie), "").await;
    let csrf = csrf_of(&html);
    // Up to date as far as the page knows: Reload, no Update.
    assert!(html.contains("action=\"/admin/updates/reload\""));
    assert!(!html.contains("action=\"/admin/updates/update\""));
    page(
        &router,
        "POST",
        "/admin/updates/check",
        Some(&cookie),
        &format!("csrf={csrf}"),
    )
    .await;
    let (_, _, html) = page(&router, "GET", "/admin", Some(&cookie), "").await;
    assert!(html.contains(&format!("<button>Update to {newer}</button>")));
    assert!(!html.contains("docker compose pull"));

    // Without the form token, or signed out, nothing is asked.
    let (s, h, _) = page(
        &router,
        "POST",
        "/admin/updates/update",
        Some(&cookie),
        &format!("csrf=00&tag={newer}"),
    )
    .await;
    assert_eq!(s, StatusCode::SEE_OTHER);
    assert_eq!(h[header::LOCATION], "/admin#updates");
    let (s, _, _) = page(
        &router,
        "POST",
        "/admin/updates/reload",
        None,
        &format!("csrf={csrf}"),
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    assert!(helper.asked.lock().unwrap().is_empty());
    assert_eq!(*helper.restarts.lock().unwrap(), 0);

    // A version other than the one on offer is refused before the helper.
    page(
        &router,
        "POST",
        "/admin/updates/update",
        Some(&cookie),
        &format!("csrf={csrf}&tag=0.1.0-main.20200101000000"),
    )
    .await;
    assert!(helper.asked.lock().unwrap().is_empty());
    let (_, _, html) = page(&router, "GET", "/admin", Some(&cookie), "").await;
    assert!(html.contains(lockdown_server::updates::OFFER_CHANGED));

    // Update: the offered tag, no digest (Lockdown's manifest has none yet).
    let (s, h, _) = page(
        &router,
        "POST",
        "/admin/updates/update",
        Some(&cookie),
        &format!("csrf={csrf}&tag={newer}"),
    )
    .await;
    assert_eq!(s, StatusCode::SEE_OTHER);
    assert_eq!(h[header::LOCATION], "/admin/updates/progress");
    assert!(matches!(settled(&state), Progress::Done(_)));
    assert_eq!(
        *helper.asked.lock().unwrap(),
        vec![host_helper::UpdateRequest {
            tag: Some(newer.into()),
            expect_digest: None,
            expect_digests: None,
        }]
    );

    // The waiting page: its one script, allowed by hash, reading the state.
    let (s, h, html) = page(&router, "GET", "/admin/updates/progress", Some(&cookie), "").await;
    assert_eq!(s, StatusCode::OK);
    assert!(html.contains("data-from=\"0.1.0-main.20261108090000\""));
    let csp = h[header::CONTENT_SECURITY_POLICY].to_str().unwrap();
    assert!(csp.contains("script-src 'sha256-") && csp.contains("connect-src 'self'"));
    let (s, _, json) = page(&router, "GET", "/admin/updates/state", Some(&cookie), "").await;
    assert_eq!(s, StatusCode::OK);
    let j: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(j["version"], "0.1.0-main.20261108090000");
    assert_eq!(j["kind"], "update");
    assert_eq!(j["state"], "done");
    let (s, _, _) = page(&router, "GET", "/admin/updates/state", None, "").await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    // Reload.
    page(
        &router,
        "POST",
        "/admin/updates/reload",
        Some(&cookie),
        &format!("csrf={csrf}"),
    )
    .await;
    assert!(matches!(settled(&state), Progress::Done(_)));
    assert_eq!(*helper.restarts.lock().unwrap(), 1);

    // The operator's log: who asked, what, which version, how it went.
    let log: Vec<String> = state
        .storage
        .sql()
        .unwrap()
        .events(Some("updates"), 10)
        .unwrap()
        .into_iter()
        .map(|e| e.detail)
        .collect();
    assert!(log.iter().any(|d| d
        == &format!("the operator, from local, asked the host helper for an update to {newer}")));
    assert!(log.iter().any(|d| d.starts_with(&format!(
        "the host helper finished an update to {newer}: The host helper pulled"
    ))));
    assert!(
        log.iter()
            .any(|d| d == "the operator, from local, asked the host helper for a reload")
    );
    assert!(
        log.iter()
            .any(|d| d.contains("didn't happen") && d.contains("version on offer"))
    );
}

#[tokio::test]
async fn a_refusal_from_the_host_helper_shows_in_plain_words() {
    use lockdown_server::helper::{Progress, Setting};
    let helper = Arc::new(Helper {
        refuse: Some(host_helper::ClientError::Refused {
            status: 401,
            code: "stale".into(),
            message: "The request is too old or from the future.".into(),
        }),
        asked: Mutex::default(),
        restarts: Mutex::default(),
    });
    let state = AppState::in_memory(Config::default()).unwrap();
    state.updates.set_helper(Setting::Ready(helper.clone()));
    let (router, cookie, state) = signed_in_keeping(state).await;
    let (_, _, html) = page(&router, "GET", "/admin", Some(&cookie), "").await;
    let csrf = csrf_of(&html);
    page(
        &router,
        "POST",
        "/admin/updates/reload",
        Some(&cookie),
        &format!("csrf={csrf}"),
    )
    .await;
    assert!(matches!(settled(&state), Progress::Failed(_)));
    let (_, _, html) = page(&router, "GET", "/admin", Some(&cookie), "").await;
    assert!(html.contains("The reload didn't happen. The server&#39;s clock and the host&#39;s differ by more than 90 seconds"));
    let (_, _, json) = page(&router, "GET", "/admin/updates/state", Some(&cookie), "").await;
    let j: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(j["state"], "failed");
    assert!(j["message"].as_str().unwrap().contains("clock"));
}

#[tokio::test]
async fn a_build_from_source_says_it_doesnt_check_and_offers_no_button() {
    let (router, cookie) = signed_in_router(AppState::in_memory(Config::default()).unwrap()).await;
    let (_, _, html) = page(&router, "GET", "/admin", Some(&cookie), "").await;
    // Tests run unstamped (0.1.0): not a main-channel build.
    if lockdown_server::updates::running().pre.is_empty() {
        assert!(html.contains(lockdown_server::updates::OFF_NOT_MAIN));
        assert!(!html.contains("/admin/updates/check"));
    }
}

#[test]
fn scheduled_backups_pass_their_drill_and_a_changed_or_foreign_backup_fails() {
    use lockdown_server::backup::BackupKey;
    use lockdown_server::drills::{Schedule, backup_now, drill_now, list_backups};

    let dir = tempfile::tempdir().unwrap();
    let key_file = dir.path().join("server-key.cbor");
    let keys = ServerKeys::load_or_create(&key_file).unwrap();
    let storage = SqlStorage::open_scratch()
        .unwrap()
        .with_blob_dir(&dir.path().join("blobs"))
        .unwrap();
    let account = storage
        .create_account("sean@example.com", Id::new(), b"pf")
        .unwrap();
    storage
        .register_device(account.account_id, Id::new(), &[3; 32])
        .unwrap();
    let state = Arc::new(AppState::new(Box::new(storage), keys, Config::default()));
    let backups = dir.path().join("backups");
    let key_text = BackupKey::generate().unwrap().display();
    let schedule = Schedule::new(
        &backups,
        BackupKey::parse(&key_text).unwrap(),
        &key_file,
        Duration::from_secs(86_400),
        2,
        Duration::from_secs(7 * 86_400),
    )
    .unwrap();
    let made = backup_now(&state, &schedule).unwrap();
    assert!(made.starts_with("lockdown-"), "{made}");
    let report = drill_now(&state, &schedule);
    assert!(report.passed, "{}", report.detail);
    assert!(report.detail.contains("1 account(s)"), "{}", report.detail);
    let sql = state.storage.sql().unwrap();
    assert!(sql.events(Some("drill"), 1).unwrap()[0].ok);
    // Only the newest two are kept.
    std::thread::sleep(Duration::from_millis(1100));
    backup_now(&state, &schedule).unwrap();
    std::thread::sleep(Duration::from_millis(1100));
    backup_now(&state, &schedule).unwrap();
    assert_eq!(list_backups(&backups).unwrap().len(), 2);

    // A changed byte: the drill fails and says so.
    let newest = list_backups(&backups).unwrap().remove(0);
    let path = backups.join(&newest.name);
    let mut bytes = std::fs::read(&path).unwrap();
    let n = bytes.len();
    bytes[n / 2] ^= 1;
    std::fs::write(&path, &bytes).unwrap();
    let report = drill_now(&state, &schedule);
    assert!(!report.passed);
    assert!(report.detail.contains("doesn't open"), "{}", report.detail);
    assert!(!sql.events(Some("drill"), 1).unwrap()[0].ok);
    std::fs::remove_file(&path).unwrap();

    // Another server's backup (another key file, another log) fails too.
    let other_dir = tempfile::tempdir().unwrap();
    let other_key = other_dir.path().join("server-key.cbor");
    ServerKeys::load_or_create(&other_key).unwrap();
    let other = SqlStorage::open(&other_dir.path().join("db.sqlite")).unwrap();
    other
        .create_account("x@example.com", Id::new(), b"pf")
        .unwrap();
    std::thread::sleep(Duration::from_millis(1100));
    let foreign = backups.join("lockdown-29991231-235959.ldbk");
    lockdown_server::backup::create(
        &lockdown_server::backup::Sources {
            storage: &other,
            server_key: &other_key,
        },
        &BackupKey::parse(&key_text).unwrap(),
        &foreign,
        1,
    )
    .unwrap();
    let report = drill_now(&state, &schedule);
    assert!(!report.passed);
    assert!(report.detail.contains("key file"), "{}", report.detail);
}

// --- SMTP against a relay in this process ---------------------------------------------

struct Relay {
    port: u16,
    seen: Arc<Mutex<Vec<String>>>,
}

fn relay(tls: Option<Arc<rustls::ServerConfig>>) -> Relay {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen: Arc<Mutex<Vec<String>>> = Arc::default();
    let log = seen.clone();
    std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut conn: Box<dyn ReadWrite> = Box::new(stream.try_clone().unwrap());
        let say = |c: &mut Box<dyn ReadWrite>, line: &str| {
            c.write_all(format!("{line}\r\n").as_bytes()).unwrap();
            c.flush().unwrap();
        };
        say(&mut conn, "220 relay ready");
        let mut tls_on = false;
        let mut reader_buf = Vec::new();
        loop {
            let line = read_line(&mut conn, &mut reader_buf);
            let Some(line) = line else { return };
            log.lock()
                .unwrap()
                .push(format!("{}{line}", if tls_on { "[tls] " } else { "" }));
            let upper = line.to_ascii_uppercase();
            if upper.starts_with("EHLO") {
                if tls.is_some() && !tls_on {
                    say(&mut conn, "250-relay");
                    say(&mut conn, "250-STARTTLS");
                    say(&mut conn, "250 AUTH PLAIN");
                } else {
                    say(&mut conn, "250-relay");
                    say(&mut conn, "250 AUTH PLAIN");
                }
            } else if upper == "STARTTLS" {
                say(&mut conn, "220 go ahead");
                let server = rustls::ServerConnection::new(tls.clone().unwrap()).unwrap();
                conn = Box::new(rustls::StreamOwned::new(
                    server,
                    stream.try_clone().unwrap(),
                ));
                tls_on = true;
            } else if upper.starts_with("AUTH PLAIN") {
                say(&mut conn, "235 ok");
            } else if upper.starts_with("MAIL FROM") || upper.starts_with("RCPT TO") {
                say(&mut conn, "250 ok");
            } else if upper == "DATA" {
                say(&mut conn, "354 send it");
                let mut data = String::new();
                loop {
                    let l = read_line(&mut conn, &mut reader_buf).unwrap();
                    if l == "." {
                        break;
                    }
                    data.push_str(&l);
                    data.push('\n');
                }
                log.lock().unwrap().push(format!("DATA:{data}"));
                say(&mut conn, "250 queued");
            } else if upper == "QUIT" {
                say(&mut conn, "221 bye");
                return;
            } else {
                say(&mut conn, "500 what");
            }
        }
    });
    Relay { port, seen }
}

trait ReadWrite: Read + Write + Send {}
impl<T: Read + Write + Send> ReadWrite for T {}

fn read_line(conn: &mut Box<dyn ReadWrite>, buf: &mut Vec<u8>) -> Option<String> {
    loop {
        if let Some(i) = buf.windows(2).position(|w| w == b"\r\n") {
            let line = String::from_utf8(buf[..i].to_vec()).unwrap();
            buf.drain(..i + 2);
            return Some(line);
        }
        let mut chunk = [0u8; 4096];
        let n = conn.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
}

fn sample_mail() -> Mail {
    lockdown_server::mail::new_device(
        "sean@example.com",
        &lockdown_server::mail::SignInFacts {
            device: "lockdown/0.1.0 (linux)".into(),
            address: "192.0.2.7".into(),
            at: 1_792_316_980,
        },
        "https://vault.example",
    )
}

fn body_of(data: &str) -> String {
    let b64: String = data.split("\n\n").nth(1).unwrap().lines().collect();
    String::from_utf8(data_encoding::BASE64.decode(b64.as_bytes()).unwrap()).unwrap()
}

#[test]
fn the_smtp_client_sends_to_a_relay_on_this_machine() {
    let r = relay(None);
    let mailer = SmtpMailer::new(SmtpConfig {
        url: format!("smtp://127.0.0.1:{}", r.port),
        user: None,
        password: None,
        from: "Lockdown <lockdown@example.com>".into(),
        ca: None,
    })
    .unwrap();
    mailer.send(&sample_mail()).unwrap();
    let seen = r.seen.lock().unwrap().clone();
    assert!(seen.contains(&"MAIL FROM:<lockdown@example.com>".to_string()));
    assert!(seen.contains(&"RCPT TO:<sean@example.com>".to_string()));
    let data = seen.iter().find(|l| l.starts_with("DATA:")).unwrap();
    let data = data.trim_start_matches("DATA:").replace("\r\n", "\n");
    assert!(data.contains("Subject: A new device signed in to your Lockdown account"));
    assert!(data.contains("To: <sean@example.com>"));
    assert!(body_of(&data).contains("192.0.2.7"));
}

#[test]
fn the_smtp_client_upgrades_with_starttls_and_signs_in_only_inside_it() {
    let tmp = tempfile::tempdir().unwrap();
    let pki = lockdown_tls::testing::pki(&["127.0.0.1"]);
    let files = pki.write(tmp.path());
    let cert = lockdown_tls::ReloadingCert::load(
        &files.cert,
        &files.key,
        Arc::new(lockdown_tls::provider(lockdown_tls::Groups::PreferHybrid)),
    )
    .unwrap();
    let server =
        Arc::new(lockdown_tls::server_config(cert, lockdown_tls::Groups::PreferHybrid).unwrap());
    let r = relay(Some(server));
    let mailer = SmtpMailer::new(SmtpConfig {
        url: format!("smtp://127.0.0.1:{}", r.port),
        user: Some("relay-user".into()),
        password: Some(zeroize::Zeroizing::new("relay-secret".into())),
        from: "lockdown@example.com".into(),
        ca: Some(vec![pki.ca_der.clone()]),
    })
    .unwrap();
    mailer.send(&sample_mail()).unwrap();
    let seen = r.seen.lock().unwrap().clone();
    let auth = seen.iter().position(|l| l.contains("AUTH PLAIN")).unwrap();
    assert!(
        seen[auth].starts_with("[tls] "),
        "the password went inside TLS: {seen:?}"
    );
    let starttls = seen.iter().position(|l| l == "STARTTLS").unwrap();
    assert!(starttls < auth);
    assert!(
        seen.iter()
            .any(|l| l.starts_with("[tls] RCPT TO:<sean@example.com>"))
    );
}

#[tokio::test]
async fn removing_a_security_key_is_alerted_to_every_device_and_mailed() {
    let inbox = Inbox::default();
    let state = Arc::new(mailing_state(Config::default(), &inbox, true));
    let router = app(state.clone());
    register(&router, "sean@example.com").await;
    let (_, r) = login(&router, "sean@example.com", &Device::new()).await;
    let token = r.unwrap().token;
    let account = state
        .storage
        .account_by_login("sean@example.com")
        .unwrap()
        .unwrap();
    let key = lockdown_server::second_factor::KeyRow {
        credential_id: vec![7; 16],
        name: "Blue key".into(),
        alg: "es256".into(),
        cose_key: vec![1, 2, 3],
        sign_count: 0,
        created_at: 1,
        last_used: None,
    };
    state
        .storage
        .account_security()
        .unwrap()
        .add_key(account.account_id, &key)
        .unwrap();
    let id = data_encoding::BASE64URL_NOPAD.encode(&key.credential_id);
    let (s, _) = call(
        &router,
        "POST",
        &format!("/v1/account/security-keys/{id}/remove"),
        Some(&token),
        Vec::new(),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (_, bytes) = call(
        &router,
        "GET",
        "/v1/account/alerts?since=0",
        Some(&token),
        Vec::new(),
    )
    .await;
    let alerts: AlertsSince = wire::decode(&bytes).unwrap();
    let a = alerts.alerts.last().unwrap();
    assert_eq!(a.kind, wire::AlertKind::SecurityKeyRemoved);
    assert_eq!(a.key_name.as_deref(), Some("Blue key"));
    let mail = inbox.wait_for(1)[0].clone();
    assert_eq!(mail.kind, "security_key");
    assert!(mail.subject.contains("removed") && mail.body.contains("\"Blue key\""));
}
