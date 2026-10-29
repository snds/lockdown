//! Session renewal at the server (docs/plan/33-session-renewal.md), driven
//! in process: a device renews with its key past the token's lifetime and
//! the old session ends; a stolen token without the key, another device's
//! key, a signature under another context, a used challenge and a challenge
//! given to another session renew nothing; an ended session, a revoked
//! device, a key the identity doesn't list, an idle session and a server
//! with renewal off each refuse with their own code.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
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
use lockdown_server::{AppState, Config, app};
use lockdown_sync::wire::{
    self, CONTENT_TYPE, DeviceProofWire, ErrorBody, ErrorCode, IdentityPush, LoginFinish,
    LoginFinishReply, LoginStart, LoginStartReply, RENEW_V, RecordWire, RegisterFinish,
    RegisterStart, RegisterStartReply, RenewChallenge, RenewFinish, SessionList,
};
use lockdown_vault::Id;
use lockdown_vault::identity::identity_signing_key;
use lockdown_vault::identity_set::{RecordSet, Revocation};
use tower::ServiceExt;

const LOGIN: &str = "me@example.com";
const PASSWORD: &str = "correct horse battery staple";
const START: i64 = 1_700_000_000;
const TTL: i64 = 60;

struct Harness {
    router: Router,
    clock: Arc<AtomicI64>,
    state: Arc<AppState>,
}

fn harness(config: Config) -> Harness {
    let clock = Arc::new(AtomicI64::new(START));
    let mut state = AppState::in_memory(Config {
        session_ttl: Duration::from_secs(TTL as u64),
        per_login: (1000, 1000.0),
        per_address: (1000, 1000.0),
        ..config
    })
    .unwrap();
    let c = clock.clone();
    state.clock = Box::new(move || c.load(Ordering::SeqCst));
    let state = Arc::new(state);
    Harness {
        router: app(state.clone()),
        clock,
        state,
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

impl Harness {
    fn at(&self, t: i64) {
        self.clock.store(t, Ordering::SeqCst);
    }

    async fn call(
        &self,
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
        let res = self
            .router
            .clone()
            .oneshot(req.body(Body::from(body)).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, bytes)
    }

    async fn cbor<T: serde::Serialize, R: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        path: &str,
        token: Option<&str>,
        body: Option<&T>,
    ) -> Result<R, (StatusCode, ErrorCode)> {
        let body = body.map(|b| wire::encode(b).unwrap()).unwrap_or_default();
        let (status, bytes) = self.call(method, path, token, body).await;
        if status.is_success() {
            Ok(wire::decode(&bytes).unwrap_or_else(|e| panic!("decode {path}: {e}")))
        } else {
            let body: ErrorBody = wire::decode(&bytes).expect("an error body");
            Err((status, body.code))
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

    async fn login(&self, device: &Device) -> LoginFinishReply {
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
        .unwrap()
    }

    async fn challenge(&self, token: &str) -> Result<RenewChallenge, (StatusCode, ErrorCode)> {
        self.cbor::<(), _>("POST", "/v1/account/session/challenge", Some(token), None)
            .await
    }

    async fn finish(
        &self,
        token: &str,
        challenge_id: Id,
        signature: Vec<u8>,
    ) -> Result<LoginFinishReply, (StatusCode, ErrorCode)> {
        self.cbor(
            "POST",
            "/v1/account/session/renew",
            Some(token),
            Some(&RenewFinish {
                v: RENEW_V,
                challenge_id,
                signature,
            }),
        )
        .await
    }

    /// The whole renewal, signed by `key`.
    async fn renew(
        &self,
        token: &str,
        key: &SigningKey,
    ) -> Result<LoginFinishReply, (StatusCode, ErrorCode)> {
        let c = self.challenge(token).await?;
        let sig = key.sign(&session_renew_message(&c.challenge_id, &c.challenge, token));
        self.finish(token, c.challenge_id, sig.to_vec()).await
    }

    async fn vaults(&self, token: &str) -> (StatusCode, ErrorCode) {
        let (status, bytes) = self
            .call("GET", "/v1/vaults", Some(token), Vec::new())
            .await;
        if status.is_success() {
            (status, ErrorCode::Internal)
        } else {
            (status, wire::decode::<ErrorBody>(&bytes).unwrap().code)
        }
    }

    async fn sessions(&self, token: &str) -> SessionList {
        self.cbor::<(), _>("GET", "/v1/account/sessions", Some(token), None)
            .await
            .unwrap()
    }
}

#[tokio::test]
async fn a_device_renews_with_its_key_past_the_tokens_life_and_the_old_session_ends() {
    let h = harness(Config::default());
    let profile = Id::new();
    h.register(profile).await;
    let device = Device::new();
    let first = h.login(&device).await;
    let listed = h.sessions(&first.token).await;
    let signed_in_at = listed.sessions[0].created_at;

    // The token runs out: calls are refused as expired, renewal is not.
    h.at(START + TTL + 600);
    assert_eq!(
        h.vaults(&first.token).await,
        (StatusCode::UNAUTHORIZED, ErrorCode::TokenExpired)
    );
    let renewed = h.renew(&first.token, &device.key()).await.unwrap();
    assert_ne!(renewed.token, first.token);
    assert_eq!(renewed.profile_id, profile);
    assert_eq!(renewed.expires_at, START + TTL + 600 + TTL);
    assert!(renewed.second_factor.is_none());
    assert_eq!(h.vaults(&renewed.token).await.0, StatusCode::OK);

    // One session, a new id, still "signed in" when the password was typed.
    let listed = h.sessions(&renewed.token).await;
    assert_eq!(listed.sessions.len(), 1);
    assert!(listed.sessions[0].current);
    assert_eq!(listed.sessions[0].created_at, signed_in_at);
    assert_eq!(listed.sessions[0].last_seen, START + TTL + 600);

    // Renewing before the token runs out works too, and the old token
    // stops at once with words that say it was renewed, not ended.
    h.at(START + TTL + 610);
    let again = h.renew(&renewed.token, &device.key()).await.unwrap();
    assert_eq!(
        h.vaults(&renewed.token).await,
        (StatusCode::UNAUTHORIZED, ErrorCode::Unauthenticated)
    );
    assert_eq!(h.vaults(&again.token).await.0, StatusCode::OK);
    // The first token, renewed long ago (past the retry window), is done.
    h.at(START + TTL + 600 + 121);
    assert_eq!(
        h.challenge(&first.token).await.unwrap_err(),
        (StatusCode::UNAUTHORIZED, ErrorCode::SessionEnded)
    );
}

#[tokio::test]
async fn a_lost_answer_may_be_fetched_again_briefly_and_only_once_more() {
    let h = harness(Config::default());
    h.register(Id::new()).await;
    let device = Device::new();
    let first = h.login(&device).await;
    h.at(START + TTL + 1);
    let lost = h.renew(&first.token, &device.key()).await.unwrap();
    // The answer never arrived: the device renews the old token again.
    let second = h.renew(&first.token, &device.key()).await.unwrap();
    assert_eq!(
        h.vaults(&lost.token).await,
        (StatusCode::UNAUTHORIZED, ErrorCode::Unauthenticated),
        "the unused successor went"
    );
    assert_eq!(h.vaults(&second.token).await.0, StatusCode::OK);
    assert_eq!(h.sessions(&second.token).await.sessions.len(), 1);
    // After the window, the old token renews nothing.
    h.at(START + TTL + 1 + 121);
    assert_eq!(
        h.renew(&first.token, &device.key()).await.unwrap_err(),
        (StatusCode::UNAUTHORIZED, ErrorCode::SessionEnded)
    );
}

#[tokio::test]
async fn a_token_without_its_device_key_renews_nothing() {
    let h = harness(Config::default());
    h.register(Id::new()).await;
    let (device, thief) = (Device::new(), Device::new());
    let session = h.login(&device).await;
    let other = h.login(&thief).await;
    h.at(START + TTL + 5);

    // Another device's key (even one on the account), or no key at all.
    assert_eq!(
        h.renew(&session.token, &thief.key()).await.unwrap_err(),
        (StatusCode::UNAUTHORIZED, ErrorCode::BadSignature)
    );
    let c = h.challenge(&session.token).await.unwrap();
    assert_eq!(
        h.finish(&session.token, c.challenge_id, vec![0; 64])
            .await
            .unwrap_err(),
        (StatusCode::UNAUTHORIZED, ErrorCode::BadSignature)
    );
    // That challenge was used up by the failure: the right signature now
    // comes too late.
    let sig = device.key().sign(&session_renew_message(
        &c.challenge_id,
        &c.challenge,
        &session.token,
    ));
    assert_eq!(
        h.finish(&session.token, c.challenge_id, sig.to_vec())
            .await
            .unwrap_err(),
        (StatusCode::BAD_REQUEST, ErrorCode::BadRequest)
    );

    // The right key under another context (a login proof's bytes).
    let c = h.challenge(&session.token).await.unwrap();
    let wrong = device
        .key()
        .sign(&device_pop_message(LOGIN, &c.challenge_id));
    assert_eq!(
        h.finish(&session.token, c.challenge_id, wrong.to_vec())
            .await
            .unwrap_err(),
        (StatusCode::UNAUTHORIZED, ErrorCode::BadSignature)
    );

    // A challenge given to another session can't renew this one, even
    // signed over this token.
    let theirs = h.challenge(&other.token).await.unwrap();
    let sig = device.key().sign(&session_renew_message(
        &theirs.challenge_id,
        &theirs.challenge,
        &session.token,
    ));
    assert_eq!(
        h.finish(&session.token, theirs.challenge_id, sig.to_vec())
            .await
            .unwrap_err(),
        (StatusCode::BAD_REQUEST, ErrorCode::BadRequest)
    );

    // A challenge that ran out.
    let c = h.challenge(&session.token).await.unwrap();
    h.at(START + TTL + 5 + 121);
    let sig = device.key().sign(&session_renew_message(
        &c.challenge_id,
        &c.challenge,
        &session.token,
    ));
    assert_eq!(
        h.finish(&session.token, c.challenge_id, sig.to_vec())
            .await
            .unwrap_err(),
        (StatusCode::BAD_REQUEST, ErrorCode::BadRequest)
    );

    // A forged or foreign token gets no challenge at all.
    assert_eq!(
        h.challenge("lk1.not.real").await.unwrap_err(),
        (StatusCode::UNAUTHORIZED, ErrorCode::Unauthenticated)
    );
    let elsewhere = harness(Config::default());
    assert_eq!(
        elsewhere.challenge(&session.token).await.unwrap_err(),
        (StatusCode::UNAUTHORIZED, ErrorCode::Unauthenticated)
    );

    // A used challenge is never answered twice.
    let c = h.challenge(&session.token).await.unwrap();
    let sig = device.key().sign(&session_renew_message(
        &c.challenge_id,
        &c.challenge,
        &session.token,
    ));
    let renewed = h
        .finish(&session.token, c.challenge_id, sig.to_vec())
        .await
        .unwrap();
    assert_eq!(
        h.finish(&session.token, c.challenge_id, sig.to_vec())
            .await
            .unwrap_err(),
        (StatusCode::BAD_REQUEST, ErrorCode::BadRequest)
    );
    assert_eq!(h.vaults(&renewed.token).await.0, StatusCode::OK);
    assert_eq!(h.state.renewals.waiting(), 0);
}

#[tokio::test]
async fn an_ended_session_an_idle_one_and_renewal_off_refuse_with_their_own_codes() {
    let h = harness(Config::default());
    h.register(Id::new()).await;
    let (laptop, phone) = (Device::new(), Device::new());
    let mine = h.login(&laptop).await;
    let other = h.login(&phone).await;

    // The phone's session is ended from the laptop (D221).
    let theirs = h
        .sessions(&mine.token)
        .await
        .sessions
        .into_iter()
        .find(|s| !s.current)
        .unwrap();
    let (status, _) = h
        .call(
            "POST",
            &format!("/v1/account/sessions/{}/end", theirs.id),
            Some(&mine.token),
            Vec::new(),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(
        h.renew(&other.token, &phone.key()).await.unwrap_err(),
        (StatusCode::UNAUTHORIZED, ErrorCode::SessionEnded)
    );

    // The laptop's session idles past the limit (30 days by default),
    // counted from its last use.
    h.at(START + 30 * 86_400 + 1);
    assert_eq!(
        h.renew(&mine.token, &laptop.key()).await.unwrap_err(),
        (StatusCode::UNAUTHORIZED, ErrorCode::SessionIdle)
    );
    // Just inside the limit it still renews.
    let fresh = h.login(&laptop).await;
    h.at(START + 30 * 86_400 + 2 + 30 * 86_400 - 10);
    assert!(h.renew(&fresh.token, &laptop.key()).await.is_ok());

    // A server with renewal off says so.
    let off = harness(Config {
        session_idle: Duration::ZERO,
        ..Config::default()
    });
    off.register(Id::new()).await;
    let s = off.login(&laptop).await;
    off.at(START + TTL + 1);
    assert_eq!(
        off.challenge(&s.token).await.unwrap_err(),
        (StatusCode::FORBIDDEN, ErrorCode::RenewalOff)
    );
    // A shorter limit, from the flag.
    let short = harness(Config {
        session_idle: Duration::from_secs(3600),
        ..Config::default()
    });
    short.register(Id::new()).await;
    let s = short.login(&laptop).await;
    short.at(START + 3601);
    assert_eq!(
        short.challenge(&s.token).await.unwrap_err(),
        (StatusCode::UNAUTHORIZED, ErrorCode::SessionIdle)
    );
}

/// `PUT /v1/identity`, with every record of the set.
async fn put_identity(h: &Harness, token: &str, set: &RecordSet) {
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
    let (status, bytes) = h
        .call(
            "PUT",
            "/v1/identity",
            Some(token),
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

#[tokio::test]
async fn a_revoked_device_and_a_key_the_identity_doesnt_list_cant_renew() {
    let dir = tempfile::tempdir().unwrap();
    let keystore = FileKeystore::open_or_create(&dir.path().join("k.cbor")).unwrap();
    let identity =
        Identity::create(&dir.path().join("root"), "lockdown", "laptop", &keystore).unwrap();
    let isk = identity_signing_key(&identity, &keystore).unwrap();
    let h = harness(Config::default());
    h.register(identity.profile().profile_id).await;
    let (a, b, c) = (Device::new(), Device::new(), Device::new());
    let token_a = h.login(&a).await.token;
    let token_b = h.login(&b).await.token;
    let token_c = h.login(&c).await.token;

    let record = |d: &Device, name: &str, key: [u8; 32]| {
        let mut r = identity.devices().next().unwrap().clone();
        r.device_id = d.id;
        r.device_pub = key;
        r.name = name.into();
        r.revoked_at = None;
        r.sign(isk.inner()).unwrap();
        r
    };
    let rec_a = record(&a, "laptop", a.key().verifying_key().to_bytes());
    let rec_b = record(&b, "phone", b.key().verifying_key().to_bytes());
    // The identity lists C under a key C never signed in with.
    let stranger = SigningKey::generate().unwrap();
    let rec_c = record(&c, "tablet", stranger.verifying_key().to_bytes());
    let set = |devices: Vec<_>, prev: Option<&RecordSet>, revocations: Vec<Revocation>| {
        let mut s = RecordSet::from_records(
            identity.profile(),
            identity.personas().cloned(),
            identity.devices().cloned().chain(devices),
            identity.envelopes().cloned(),
            identity.consents().cloned(),
            prev.map(|p| &p.manifest),
            revocations,
            identity.device_id(),
        )
        .unwrap();
        s.sign(&isk).unwrap();
        s
    };
    let first = set(
        vec![rec_a.clone(), rec_b.clone(), rec_c.clone()],
        None,
        Vec::new(),
    );
    put_identity(&h, &token_a, &first).await;

    // Listed with its own key, B renews; C, listed under another key, can't.
    h.at(START + TTL + 1);
    let token_b = h.renew(&token_b, &b.key()).await.unwrap().token;
    assert_eq!(
        h.renew(&token_c, &c.key()).await.unwrap_err(),
        (StatusCode::FORBIDDEN, ErrorCode::UnknownDevice)
    );

    // A revokes B (the account has no vault yet, so no boundaries).
    let token_a = h.login(&a).await.token;
    let mut revoked_b = rec_b.clone();
    revoked_b.revoked_at = Some(lockdown_vault::Timestamp(START));
    revoked_b.sign(isk.inner()).unwrap();
    let second = set(
        vec![rec_a, revoked_b, rec_c],
        Some(&first),
        vec![Revocation {
            device_id: b.id,
            vaults: Vec::new(),
        }],
    );
    put_identity(&h, &token_a, &second).await;
    h.at(START + 2 * TTL + 2);
    assert_eq!(
        h.renew(&token_b, &b.key()).await.unwrap_err(),
        (StatusCode::FORBIDDEN, ErrorCode::RevokedDevice)
    );
    // A, in good standing, renews as ever.
    assert!(h.renew(&token_a, &a.key()).await.is_ok());
}

#[tokio::test]
async fn a_revocation_ends_every_session_of_that_device_at_once() {
    // D405's follow-up: a revoked device's token stops reading the moment
    // the identity that revokes it is accepted, not when it runs out.
    let dir = tempfile::tempdir().unwrap();
    let keystore = FileKeystore::open_or_create(&dir.path().join("k.cbor")).unwrap();
    let identity =
        Identity::create(&dir.path().join("root"), "lockdown", "laptop", &keystore).unwrap();
    let isk = identity_signing_key(&identity, &keystore).unwrap();
    let h = harness(Config::default());
    h.register(identity.profile().profile_id).await;
    let (a, b, c) = (Device::new(), Device::new(), Device::new());
    let token_a = h.login(&a).await.token;
    // B holds two sessions (a second sign-in); C one.
    let token_b = h.login(&b).await.token;
    let token_b2 = h.login(&b).await.token;
    let token_c = h.login(&c).await.token;

    let record = |d: &Device, name: &str| {
        let mut r = identity.devices().next().unwrap().clone();
        r.device_id = d.id;
        r.device_pub = d.key().verifying_key().to_bytes();
        r.name = name.into();
        r.revoked_at = None;
        r.sign(isk.inner()).unwrap();
        r
    };
    let (rec_a, rec_b, rec_c) = (
        record(&a, "laptop"),
        record(&b, "phone"),
        record(&c, "tablet"),
    );
    let set = |devices: Vec<_>, prev: Option<&RecordSet>, revocations: Vec<Revocation>| {
        let mut s = RecordSet::from_records(
            identity.profile(),
            identity.personas().cloned(),
            identity.devices().cloned().chain(devices),
            identity.envelopes().cloned(),
            identity.consents().cloned(),
            prev.map(|p| &p.manifest),
            revocations,
            identity.device_id(),
        )
        .unwrap();
        s.sign(&isk).unwrap();
        s
    };
    let first = set(
        vec![rec_a.clone(), rec_b.clone(), rec_c.clone()],
        None,
        Vec::new(),
    );
    put_identity(&h, &token_a, &first).await;
    // Listing devices ends nobody's session.
    assert_eq!(h.sessions(&token_b).await.sessions.len(), 4);

    let mut revoked_b = rec_b.clone();
    revoked_b.revoked_at = Some(lockdown_vault::Timestamp(START));
    revoked_b.sign(isk.inner()).unwrap();
    let second = set(
        vec![rec_a, revoked_b, rec_c],
        Some(&first),
        vec![Revocation {
            device_id: b.id,
            vaults: Vec::new(),
        }],
    );
    put_identity(&h, &token_a, &second).await;

    // The clock hasn't moved: both of B's tokens still verify by signature,
    // and neither reads anything.
    for t in [&token_b, &token_b2] {
        let (status, bytes) = h
            .call("GET", "/v1/account/sessions", Some(t), Vec::new())
            .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        let body: ErrorBody = wire::decode(&bytes).unwrap();
        assert_eq!(body.code, ErrorCode::Unauthenticated);
        assert!(body.message.contains("was revoked"), "{}", body.message);
    }
    // Only the signed identity record set still answers it: that is how
    // the device learns, from records it can verify, that it is out.
    let (status, _) = h
        .call("GET", "/v1/identity?since=0", Some(&token_b), Vec::new())
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = h
        .call("GET", "/v1/vaults", Some(&token_b), Vec::new())
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    // Renewing one says why, in the code the client turns into words.
    assert_eq!(
        h.renew(&token_b2, &b.key()).await.unwrap_err(),
        (StatusCode::FORBIDDEN, ErrorCode::RevokedDevice)
    );
    // A's list shows A and C only; C reads as ever.
    let left = h.sessions(&token_a).await.sessions;
    assert_eq!(left.len(), 2, "{left:?}");
    assert!(left.iter().all(|s| s.device_id != b.id));
    assert_eq!(h.sessions(&token_c).await.sessions.len(), 2);

    // A later identity push (nothing newly revoked) ends no one else's.
    let mut rec_a2 = record(&a, "laptop (renamed)");
    rec_a2.sign(isk.inner()).unwrap();
    let mut revoked_b = record(&b, "phone");
    revoked_b.revoked_at = Some(lockdown_vault::Timestamp(START));
    revoked_b.sign(isk.inner()).unwrap();
    let third = set(
        vec![rec_a2, revoked_b, record(&c, "tablet")],
        Some(&second),
        vec![Revocation {
            device_id: b.id,
            vaults: Vec::new(),
        }],
    );
    put_identity(&h, &token_a, &third).await;
    assert_eq!(h.sessions(&token_c).await.sessions.len(), 2);
}
