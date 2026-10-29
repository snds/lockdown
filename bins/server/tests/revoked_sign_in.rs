//! A revoked device can't sign back in (docs/plan/35-revoke-split.md §1),
//! driven in process against the router:
//!
//! - after the identity revokes it, the device's password sign-in is
//!   refused at the login's finish with `revoked_sign_in` and the plain
//!   words, and so is its session renewal; its old token still reads the
//!   signed identity record set and nothing else; the account's other
//!   devices sign in and renew as ever, and the same machine comes back as
//!   a new device id;
//! - a device revoked while it waits for a security key is refused when it
//!   collects its session, and once revoked it isn't asked for a key again;
//! - a device revoked while it waits for the account's email approval is
//!   refused when it collects its session, and no approval email goes out
//!   for its next try.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use lockdown_crypto::SigningKey;
use lockdown_crypto::auth::{
    LoginClient, RegistrationClient, device_pop_message, session_renew_message,
};
use lockdown_crypto::identity::{FileKeystore, Identity, Record};
use lockdown_passkey::{
    Ceremony, ClientData, Flags, MakeCredential, get_assertion, make_credential,
};
use lockdown_server::mail::{Mail, MailSettings, Mailer, Outbox};
use lockdown_server::{AppState, Config, app};
use lockdown_sync::account_security::{
    CreationRequest, KeyRegisterFinish, SecondFactorAnswer, SecondFactorPoll, SecondFactorStatus,
};
use lockdown_sync::wire::{
    self, CONTENT_TYPE, DeviceProofWire, ErrorBody, ErrorCode, IdentityPush, LoginFinish,
    LoginFinishReply, LoginStart, LoginStartReply, RENEW_V, RecordWire, RegisterFinish,
    RegisterStart, RegisterStartReply, RenewChallenge, RenewFinish,
};
use lockdown_vault::Id;
use lockdown_vault::identity::identity_signing_key;
use lockdown_vault::identity_set::{RecordSet, Revocation};
use serde_bytes::ByteBuf;
use tower::ServiceExt;

const LOGIN: &str = "me@example.com";
const PASSWORD: &str = "correct horse battery staple";
const WORDS: &str =
    "This device was revoked from your account. Join it again from a device you still use.";
const HOST: &str = "127.0.0.1";

type Refused = (StatusCode, ErrorBody);

#[derive(Clone, Default)]
struct Inbox(Arc<Mutex<Vec<Mail>>>);

impl Mailer for Inbox {
    fn send(&self, mail: &Mail) -> Result<(), String> {
        self.0.lock().unwrap().push(mail.clone());
        Ok(())
    }
}

impl Inbox {
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

/// A device: an id and a signing key seed.
struct Device {
    id: Id,
    seed: [u8; 32],
}

impl Device {
    fn new() -> Self {
        Self {
            id: Id::new(),
            seed: SigningKey::generate().unwrap().inner().to_bytes(),
        }
    }

    fn key(&self) -> SigningKey {
        SigningKey::from_bytes(self.seed)
    }
}

/// The account's identity, which signs the device records and revocations.
struct Ident {
    _dir: tempfile::TempDir,
    identity: Identity,
    isk: SigningKey,
    last: Option<RecordSet>,
}

impl Ident {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let keystore = FileKeystore::open_or_create(&dir.path().join("k.cbor")).unwrap();
        let identity =
            Identity::create(&dir.path().join("root"), "lockdown", "laptop", &keystore).unwrap();
        let isk = identity_signing_key(&identity, &keystore).unwrap();
        Self {
            _dir: dir,
            identity,
            isk,
            last: None,
        }
    }

    /// The next record set: these devices listed, the `revoked` ones
    /// marked and revoked in the manifest.
    fn next(&mut self, devices: &[&Device], revoked: &[&Device]) -> RecordSet {
        let records = devices.iter().enumerate().map(|(i, d)| {
            let mut r = self.identity.devices().next().unwrap().clone();
            r.device_id = d.id;
            r.device_pub = d.key().verifying_key().to_bytes();
            r.name = format!("device {i}");
            r.revoked_at = revoked
                .iter()
                .any(|x| x.id == d.id)
                .then_some(lockdown_vault::Timestamp(1_700_000_000));
            r.sign(self.isk.inner()).unwrap();
            r
        });
        let mut set = RecordSet::from_records(
            self.identity.profile(),
            self.identity.personas().cloned(),
            self.identity.devices().cloned().chain(records),
            self.identity.envelopes().cloned(),
            self.identity.consents().cloned(),
            self.last.as_ref().map(|p| &p.manifest),
            revoked
                .iter()
                .map(|d| Revocation {
                    device_id: d.id,
                    vaults: Vec::new(),
                })
                .collect(),
            self.identity.device_id(),
        )
        .unwrap();
        set.sign(&self.isk).unwrap();
        self.last = Some(set.clone());
        set
    }
}

struct Harness {
    router: Router,
}

fn harness(inbox: Option<&Inbox>) -> Harness {
    let mut state = AppState::in_memory(Config {
        per_login: (1000, 1000.0),
        per_address: (1000, 1000.0),
        ..Config::default()
    })
    .unwrap();
    if let Some(inbox) = inbox {
        state.mail = Some(
            MailSettings::new(
                Outbox::start(Box::new(inbox.clone())).unwrap(),
                "https://vault.example",
                true,
                "a test inbox".into(),
            )
            .unwrap(),
        );
    }
    Harness {
        router: app(Arc::new(state)),
    }
}

impl Harness {
    async fn call(
        &self,
        method: &str,
        path: &str,
        token: Option<&str>,
        content_type: &str,
        body: Vec<u8>,
    ) -> (StatusCode, Bytes) {
        let mut req = Request::builder()
            .method(method)
            .uri(path)
            .header(header::HOST, HOST)
            .header(header::CONTENT_TYPE, content_type)
            .header(header::USER_AGENT, "lockdown/0.1.0 (linux)");
        if let Some(t) = token {
            req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
        }
        let res = self
            .router
            .clone()
            .oneshot(req.body(Body::from(body)).unwrap())
            .await
            .unwrap();
        let status = res.status();
        (status, res.into_body().collect().await.unwrap().to_bytes())
    }

    async fn cbor<T: serde::Serialize, R: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        path: &str,
        token: Option<&str>,
        body: Option<&T>,
    ) -> Result<R, Refused> {
        let body = body.map(|b| wire::encode(b).unwrap()).unwrap_or_default();
        let (status, bytes) = self.call(method, path, token, CONTENT_TYPE, body).await;
        if status.is_success() {
            Ok(wire::decode(&bytes).unwrap_or_else(|e| panic!("decode {path}: {e}")))
        } else {
            Err((status, wire::decode(&bytes).expect("an error body")))
        }
    }

    async fn register(&self, profile: Id) {
        let (client, request) = RegistrationClient::start(PASSWORD).unwrap();
        let reply: RegisterStartReply = self
            .cbor(
                "POST",
                "/v1/account/register/start",
                None,
                Some(&RegisterStart {
                    login: LOGIN.into(),
                    request,
                }),
            )
            .await
            .unwrap();
        let (upload, _export) = client.finish(PASSWORD, &reply.response).unwrap();
        let (status, _) = self
            .call(
                "POST",
                "/v1/account/register/finish",
                None,
                CONTENT_TYPE,
                wire::encode(&RegisterFinish {
                    login: LOGIN.into(),
                    profile_id: profile,
                    upload,
                })
                .unwrap(),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED);
    }

    /// The password sign-in, to the login's finish.
    async fn login(&self, device: &Device) -> Result<LoginFinishReply, Refused> {
        let (client, request) = LoginClient::start(PASSWORD).unwrap();
        let started: LoginStartReply = self
            .cbor(
                "POST",
                "/v1/account/login/start",
                None,
                Some(&LoginStart {
                    login: LOGIN.into(),
                    request,
                }),
            )
            .await
            .unwrap();
        let finished = client.finish(PASSWORD, &started.response).unwrap();
        let pop = device
            .key()
            .sign(&device_pop_message(LOGIN, &started.login_id));
        self.cbor(
            "POST",
            "/v1/account/login/finish",
            None,
            Some(&LoginFinish {
                login_id: started.login_id,
                finalization: finished.finalization,
                device: DeviceProofWire {
                    device_id: device.id,
                    device_pub: device.key().verifying_key().to_bytes(),
                    pop: pop.to_vec(),
                },
            }),
        )
        .await
    }

    async fn renew(&self, token: &str, key: &SigningKey) -> Result<LoginFinishReply, Refused> {
        let c: RenewChallenge = self
            .cbor::<(), _>("POST", "/v1/account/session/challenge", Some(token), None)
            .await?;
        let sig = key.sign(&session_renew_message(&c.challenge_id, &c.challenge, token));
        self.cbor(
            "POST",
            "/v1/account/session/renew",
            Some(token),
            Some(&RenewFinish {
                v: RENEW_V,
                challenge_id: c.challenge_id,
                signature: sig.to_vec(),
            }),
        )
        .await
    }

    async fn put_identity(&self, token: &str, set: &RecordSet) {
        let push = IdentityPush {
            manifest: set.manifest.to_bytes().unwrap(),
            records: set
                .records
                .iter()
                .map(|(path, body)| RecordWire {
                    path: path.clone(),
                    body: body.clone(),
                })
                .collect(),
        };
        let (status, bytes) = self
            .call(
                "PUT",
                "/v1/identity",
                Some(token),
                CONTENT_TYPE,
                wire::encode(&push).unwrap(),
            )
            .await;
        assert_eq!(
            status,
            StatusCode::OK,
            "{:?}",
            wire::decode::<ErrorBody>(&bytes)
        );
    }

    async fn poll(&self, pending: Id, poll_key: &ByteBuf) -> Result<SecondFactorStatus, Refused> {
        self.cbor(
            "POST",
            "/v1/account/login/second-factor/poll",
            None,
            Some(&SecondFactorPoll {
                pending_id: pending,
                poll_key: poll_key.clone(),
            }),
        )
        .await
    }

    async fn status_of(&self, method: &str, path: &str, token: &str) -> StatusCode {
        self.call(method, path, Some(token), CONTENT_TYPE, Vec::new())
            .await
            .0
    }
}

/// The refusal a revoked device's sign-in gets.
fn revoked(r: Result<impl std::fmt::Debug, Refused>) {
    let (status, body) = r.expect_err("a revoked device is refused");
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body.code, ErrorCode::RevokedSignIn);
    assert_eq!(body.message, WORDS);
}

#[tokio::test]
async fn a_revoked_device_cant_sign_in_or_renew_and_the_others_go_on() {
    let mut ident = Ident::new();
    let h = harness(None);
    h.register(ident.identity.profile().profile_id).await;
    let (a, b, c) = (Device::new(), Device::new(), Device::new());
    let token_a = h.login(&a).await.unwrap().token;
    let token_b = h.login(&b).await.unwrap().token;
    let token_c = h.login(&c).await.unwrap().token;
    let listed = ident.next(&[&a, &b, &c], &[]);
    h.put_identity(&token_a, &listed).await;
    // Before the revocation B signs in like anyone.
    assert!(h.login(&b).await.is_ok());

    let revoking = ident.next(&[&a, &b, &c], &[&b]);
    h.put_identity(&token_a, &revoking).await;

    // The password no longer brings B back, nor does its device key.
    revoked(h.login(&b).await);
    revoked(h.renew(&token_b, &b.key()).await);
    // Its old token reads the signed identity record set, and nothing else:
    // that is how B verifies it is out and shuts its own copy (D73).
    assert_eq!(
        h.status_of("GET", "/v1/identity?since=0", &token_b).await,
        StatusCode::OK
    );
    assert_eq!(
        h.status_of("GET", "/v1/vaults", &token_b).await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        h.status_of("GET", "/v1/account/sessions", &token_b).await,
        StatusCode::UNAUTHORIZED
    );

    // A and C are untouched: they sign in and renew as ever.
    assert!(!h.login(&a).await.unwrap().token.is_empty());
    assert!(h.renew(&token_c, &c.key()).await.is_ok());
    assert_eq!(
        h.status_of("GET", "/v1/vaults", &token_a).await,
        StatusCode::OK
    );

    // The same machine comes back by joining as a new device: a new id
    // isn't revoked (in the apps the join is approved from A, D246).
    let again = Device::new();
    assert!(!h.login(&again).await.unwrap().token.is_empty());
}

fn register_key(request: &CreationRequest) -> lockdown_passkey::Made {
    make_credential(&MakeCredential {
        rp_id: &request.rp_id,
        client_data: &ClientData {
            ceremony: Ceremony::Create,
            challenge: request.challenge.to_vec(),
            origin: format!("http://{HOST}"),
            top_origin: None,
        },
        flags: Flags {
            up: true,
            uv: true,
            be: false,
            bs: false,
        },
        algs: &[lockdown_passkey::COSE_ES256],
    })
    .unwrap()
}

#[tokio::test]
async fn a_device_revoked_while_it_waits_for_a_security_key_is_refused_at_the_end() {
    let mut ident = Ident::new();
    let h = harness(None);
    h.register(ident.identity.profile().profile_id).await;
    let a = Device::new();
    let token_a = h.login(&a).await.unwrap().token;
    let request: CreationRequest = h
        .cbor::<(), _>(
            "POST",
            "/v1/account/security-keys/start",
            Some(&token_a),
            None,
        )
        .await
        .unwrap();
    let made = register_key(&request);
    let _: lockdown_sync::account_security::SecurityKeyInfo = h
        .cbor(
            "POST",
            "/v1/account/security-keys/finish",
            None,
            Some(&KeyRegisterFinish {
                pending_id: request.pending_id,
                name: "Blue key".into(),
                client_data_json: ByteBuf::from(made.client_data_json.as_bytes().to_vec()),
                attestation_object: ByteBuf::from(made.attestation_object.clone()),
            }),
        )
        .await
        .unwrap();

    // A device the account has never seen passes the password and waits.
    let e = Device::new();
    let waiting = h.login(&e).await.unwrap();
    assert!(waiting.token.is_empty());
    let pending = waiting.second_factor.expect("a key is asked for");
    // While it waits, A revokes it.
    let listed = ident.next(&[&a, &e], &[&e]);
    h.put_identity(&token_a, &listed).await;
    // The key answers; collecting the session is refused.
    let asserted = get_assertion(
        &made.pkcs8,
        &pending.request.rp_id,
        &ClientData {
            ceremony: Ceremony::Get,
            challenge: pending.request.challenge.to_vec(),
            origin: format!("http://{HOST}"),
            top_origin: None,
        },
        Flags {
            up: true,
            ..Flags::default()
        },
    )
    .unwrap();
    let (status, _) = h
        .call(
            "POST",
            "/v1/account/login/second-factor",
            None,
            CONTENT_TYPE,
            wire::encode(&SecondFactorAnswer {
                pending_id: pending.pending_id,
                credential_id: ByteBuf::from(made.credential_id.clone()),
                client_data_json: ByteBuf::from(asserted.client_data_json.into_bytes()),
                authenticator_data: ByteBuf::from(asserted.authenticator_data),
                signature: ByteBuf::from(asserted.signature),
            })
            .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    revoked(h.poll(pending.pending_id, &pending.poll_key).await);
    // Its next try is refused at the password, with no key asked for.
    revoked(h.login(&e).await);
    // A new device still waits for the key as ever.
    let waiting = h.login(&Device::new()).await.unwrap();
    assert!(waiting.second_factor.is_some());
}

#[tokio::test]
async fn a_device_revoked_while_it_waits_for_email_approval_is_refused_at_the_end() {
    let mut ident = Ident::new();
    let inbox = Inbox::default();
    let h = harness(Some(&inbox));
    h.register(ident.identity.profile().profile_id).await;
    let a = Device::new();
    let token_a = h.login(&a).await.unwrap().token;

    // A second device waits for the account's email.
    let e = Device::new();
    let waiting = h.login(&e).await.unwrap();
    let pending = waiting.second_factor.expect("the email is asked for");
    assert!(pending.request.email_to.is_some());
    let mail = inbox.wait_for(1)[0].clone();
    let line = mail
        .body
        .lines()
        .find(|l| l.starts_with("https://vault.example/approve/"))
        .expect("the link");
    let code = line.split_once("?code=").unwrap().1.to_string();

    // A revokes it before anyone approves; then the link is approved.
    let listed = ident.next(&[&a, &e], &[&e]);
    h.put_identity(&token_a, &listed).await;
    let (status, _) = h
        .call(
            "POST",
            &format!("/approve/{}", pending.pending_id),
            None,
            "application/x-www-form-urlencoded",
            format!("code={code}&answer=approve").into_bytes(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    revoked(h.poll(pending.pending_id, &pending.poll_key).await);

    // Its next try is refused at the password; no approval email goes out.
    revoked(h.login(&e).await);
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(inbox.0.lock().unwrap().len(), 1);
    // A signs in as ever.
    assert!(!h.login(&a).await.unwrap().token.is_empty());
}
