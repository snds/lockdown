//! The security-depth endpoints over a real socket with Lockdown's own
//! client (docs/plan/22-security-depth.md):
//!
//! - a device link relays frames between an enrolled device and one new
//!   device, takes one join, refuses the wrong side and a closed link, and
//!   binds the device the person approved, so its first sign-in is known;
//! - security keys gate a device the account has never seen: the sign-in
//!   waits, a key answers in a browser, and only the poll key collects the
//!   session; a known device and an account without keys are not asked;
//! - the password file is made again under stronger stretching, announced
//!   at every login start, and never lowered;
//! - a phrase check is a relay between exactly two accounts.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use lockdown_crypto::Argon2Params;
use lockdown_crypto::SigningKey;
use lockdown_crypto::auth::{
    ACCOUNT_KSF, AccountKsf, DeviceBinding, OpaqueAccountAuth, RegistrationClient, Session,
};
use lockdown_passkey::{
    Ceremony, ClientData, Flags, MakeCredential, get_assertion, make_credential,
};
use lockdown_server::{AppState, Config, run_in_background};
use lockdown_sync::account_security::{
    KeyRegisterFinish, LinkDevice, LinkOpen, LinkSide, PasswordFinish, SecondFactorAnswer,
    request_of,
};
use lockdown_sync::wire::ErrorCode;
use lockdown_sync::{HttpTransport, TransportError};
use lockdown_vault::Id;
use lockdown_vault::device_link::LinkCode;
use serde_bytes::ByteBuf;

struct Device {
    id: Id,
    key: [u8; 32],
}

impl Device {
    fn new() -> Self {
        Self {
            id: Id::new(),
            key: SigningKey::generate().unwrap().inner().to_bytes(),
        }
    }

    fn public(&self) -> [u8; 32] {
        SigningKey::from_bytes(self.key).verifying_key().to_bytes()
    }

    fn binding(&self) -> DeviceBinding {
        let seed = self.key;
        DeviceBinding {
            device_id: self.id,
            device_pub: self.public(),
            sign: Box::new(move |m| Ok(SigningKey::from_bytes(seed).sign(m))),
        }
    }
}

const PASSWORD: &str = "correct horse battery staple";

struct Server {
    _running: lockdown_server::RunningServer,
    url: String,
    clock: Arc<AtomicI64>,
}

fn server() -> Server {
    let mut state = AppState::in_memory(Config {
        per_login: (100, 10.0),
        per_address: (200, 50.0),
        ..Config::default()
    })
    .unwrap();
    let clock = Arc::new(AtomicI64::new(1_800_000_000));
    let c = clock.clone();
    state.clock = Box::new(move || c.load(Ordering::SeqCst));
    let running = run_in_background(Arc::new(state)).unwrap();
    Server {
        url: running.url(),
        _running: running,
        clock,
    }
}

fn sign_in(url: &str, login: &str, device: &Device, register: bool) -> Session {
    let mut auth =
        OpaqueAccountAuth::new(HttpTransport::new(url).unwrap(), login, device.binding());
    if register {
        auth.register_account(&Id::new(), PASSWORD).unwrap();
    }
    auth.sign_in(PASSWORD).unwrap().1
}

fn refused(e: TransportError) -> (u16, ErrorCode) {
    match e {
        TransportError::Refused { status, code, .. } => (status, code),
        other => panic!("expected a refusal, got {other}"),
    }
}

#[test]
fn a_device_link_relays_one_join_and_binds_the_approved_device() {
    let s = server();
    let t = HttpTransport::new(&s.url).unwrap();
    let laptop = Device::new();
    let session = sign_in(&s.url, "me@example.com", &laptop, true);
    let code = LinkCode::generate().unwrap();
    let opened = t
        .link_open(
            &session.token,
            &LinkOpen {
                token_hash: code.token_hash(),
                hello: ByteBuf::from(b"hello".to_vec()),
            },
        )
        .unwrap();
    // A wrong code finds nothing; the right one joins once.
    let wrong = LinkCode::generate().unwrap();
    assert_eq!(refused(t.link_join(wrong.as_str()).unwrap_err()).0, 404);
    let joined = t.link_join(&code.display().to_lowercase()).unwrap();
    assert_eq!(joined.link_id, opened.link_id);
    assert_eq!(refused(t.link_join(code.as_str()).unwrap_err()).0, 409);

    let link = opened.link_id;
    let frames = t.link_poll(link, LinkSide::Joiner(&joined.key), 0).unwrap();
    assert_eq!(frames.frames, vec![ByteBuf::from(b"hello".to_vec())]);
    t.link_push(link, LinkSide::Joiner(&joined.key), b"request")
        .unwrap();
    // Another key, or another account's session, gets nothing.
    assert_eq!(
        refused(
            t.link_poll(link, LinkSide::Joiner(&[7; 32]), 0)
                .unwrap_err()
        )
        .0,
        403
    );
    let stranger = sign_in(&s.url, "other@example.com", &Device::new(), true);
    assert_eq!(
        refused(
            t.link_poll(link, LinkSide::Opener(&stranger.token), 0)
                .unwrap_err()
        )
        .0,
        403
    );
    let mine = t
        .link_poll(link, LinkSide::Opener(&session.token), 0)
        .unwrap();
    assert_eq!(mine.frames, vec![ByteBuf::from(b"request".to_vec())]);

    // The existing device binds the device it approved: its first sign-in
    // is then a device the account knows (no new-device alert).
    let phone = Device::new();
    t.link_device(
        &session.token,
        link,
        &LinkDevice {
            device_id: phone.id,
            device_pub: phone.public(),
        },
    )
    .unwrap();
    assert!(
        t.link_device(
            &session.token,
            link,
            &LinkDevice {
                device_id: Id::new(),
                device_pub: phone.public(),
            },
        )
        .is_err(),
        "once per link"
    );
    let phone_session = sign_in(&s.url, "me@example.com", &phone, false);
    let alerts = t.account_alerts(&session.token, 0).unwrap();
    assert!(
        alerts.alerts.is_empty(),
        "an approved device is no stranger: {alerts:?}"
    );
    assert!(!phone_session.token.is_empty());

    t.link_close(&session.token, link).unwrap();
    assert!(
        t.link_push(link, LinkSide::Joiner(&joined.key), b"late")
            .is_err()
    );
    assert!(
        t.link_poll(link, LinkSide::Joiner(&joined.key), 0)
            .unwrap()
            .closed
    );

    // A link runs out after ten minutes.
    let code2 = LinkCode::generate().unwrap();
    let second = t
        .link_open(
            &session.token,
            &LinkOpen {
                token_hash: code2.token_hash(),
                hello: ByteBuf::from(b"hello".to_vec()),
            },
        )
        .unwrap();
    s.clock.fetch_add(11 * 60, Ordering::SeqCst);
    assert_eq!(refused(t.link_join(code2.as_str()).unwrap_err()).0, 404);
    let _ = second;
}

fn register_key(t: &HttpTransport, token: &str, url: &str) -> lockdown_passkey::Made {
    let request = t.security_key_start(token).unwrap();
    let origin = url.trim_end_matches('/').to_string();
    let made = make_credential(&MakeCredential {
        rp_id: &request.rp_id,
        client_data: &ClientData {
            ceremony: Ceremony::Create,
            challenge: request.challenge.to_vec(),
            origin,
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
    .unwrap();
    let info = t
        .security_key_finish(&KeyRegisterFinish {
            pending_id: request.pending_id,
            name: "Blue key".into(),
            client_data_json: ByteBuf::from(made.client_data_json.as_bytes().to_vec()),
            attestation_object: ByteBuf::from(made.attestation_object.clone()),
        })
        .unwrap();
    assert_eq!(info.name, "Blue key");
    made
}

#[test]
fn a_security_key_gates_a_new_device_and_only_the_poll_key_collects_the_session() {
    let s = server();
    let t = HttpTransport::new(&s.url).unwrap();
    let laptop = Device::new();
    let session = sign_in(&s.url, "me@example.com", &laptop, true);
    assert!(session.second_factor.is_none(), "no keys yet");
    let made = register_key(&t, &session.token, &s.url);
    let list = t.security_keys(&session.token).unwrap();
    assert_eq!(list.keys.len(), 1);
    assert_eq!(list.rp_id, "127.0.0.1");

    // The laptop is known: signing in again asks for no key.
    let again = sign_in(&s.url, "me@example.com", &laptop, false);
    assert!(again.second_factor.is_none() && !again.token.is_empty());

    // A new device passes the password and waits.
    let phone = Device::new();
    let waiting = sign_in(&s.url, "me@example.com", &phone, false);
    assert!(waiting.token.is_empty());
    let needed = waiting.second_factor.expect("a key is needed");
    let request = request_of(&needed).unwrap();
    assert_eq!(request.allow.len(), 1);
    assert!(
        t.second_factor_poll(needed.pending_id, &needed.poll_key)
            .unwrap()
            .session
            .is_none(),
        "not yet"
    );
    // The page in a browser looks the request up by its id alone.
    let looked = t.second_factor_request(needed.pending_id).unwrap();
    assert_eq!(looked.challenge, request.challenge);

    let answer = |challenge: &[u8]| {
        let a = get_assertion(
            &made.pkcs8,
            &request.rp_id,
            &ClientData {
                ceremony: Ceremony::Get,
                challenge: challenge.to_vec(),
                origin: s.url.trim_end_matches('/').to_string(),
                top_origin: None,
            },
            Flags {
                up: true,
                ..Flags::default()
            },
        )
        .unwrap();
        SecondFactorAnswer {
            pending_id: needed.pending_id,
            credential_id: ByteBuf::from(made.credential_id.clone()),
            client_data_json: ByteBuf::from(a.client_data_json.into_bytes()),
            authenticator_data: ByteBuf::from(a.authenticator_data),
            signature: ByteBuf::from(a.signature),
        }
    };
    // An answer to another challenge is refused and the wait goes on.
    assert_eq!(
        refused(
            t.second_factor_answer(&answer(b"another challenge"))
                .unwrap_err()
        )
        .0,
        401
    );
    t.second_factor_answer(&answer(&request.challenge)).unwrap();
    // The wrong poll key gets nothing; the right one the session, once.
    assert!(t.second_factor_poll(needed.pending_id, &[0; 32]).is_err());
    let got = t
        .second_factor_poll(needed.pending_id, &needed.poll_key)
        .unwrap()
        .session
        .expect("the session");
    assert!(!got.token.is_empty());
    assert!(t.list_vaults_for_test(&got.token));
    assert!(
        t.second_factor_poll(needed.pending_id, &needed.poll_key)
            .is_err()
    );

    // The phone is known now; with the key removed nobody is asked.
    let list = t.security_keys(&session.token).unwrap();
    t.security_key_remove(&session.token, &list.keys[0].id)
        .unwrap();
    let tablet = sign_in(&s.url, "me@example.com", &Device::new(), false);
    assert!(tablet.second_factor.is_none());
}

trait ListForTest {
    fn list_vaults_for_test(&self, token: &str) -> bool;
}

impl ListForTest for HttpTransport {
    fn list_vaults_for_test(&self, token: &str) -> bool {
        lockdown_sync::Transport::list_vaults(self, token).is_ok()
    }
}

#[test]
fn the_password_file_is_made_again_under_stronger_stretching_and_never_lowered() {
    let s = server();
    let t = HttpTransport::new(&s.url).unwrap();
    let laptop = Device::new();
    let session = sign_in(&s.url, "me@example.com", &laptop, true);
    let raised = Argon2Params {
        t_cost: ACCOUNT_KSF.t_cost + 1,
        ..ACCOUNT_KSF
    };
    let restretch = |params: Argon2Params| {
        let (client, request) = RegistrationClient::start(PASSWORD).unwrap();
        let reply = t.password_start(&session.token, &request).unwrap();
        let (upload, _) = client
            .finish_stretched(PASSWORD, &reply.response, &AccountKsf::at(params).unwrap())
            .unwrap();
        t.password_finish(
            &session.token,
            &PasswordFinish {
                upload: ByteBuf::from(upload),
                ksf: params,
            },
        )
    };
    restretch(raised).unwrap();
    // Every sign-in now stretches under the raised set, announced by the server.
    let mut auth = OpaqueAccountAuth::new(
        HttpTransport::new(&s.url).unwrap(),
        "me@example.com",
        laptop.binding(),
    );
    auth.sign_in(PASSWORD).unwrap();
    assert_eq!(auth.last_ksf(), Some(raised));
    // A device that used the raised set refuses a server offering less.
    let mut cautious = OpaqueAccountAuth::new(
        HttpTransport::new(&s.url).unwrap(),
        "me@example.com",
        laptop.binding(),
    )
    .at_least(Some(Argon2Params {
        t_cost: raised.t_cost + 1,
        ..raised
    }));
    assert!(cautious.sign_in(PASSWORD).is_err());
    // Lowering is refused.
    assert_eq!(refused(restretch(ACCOUNT_KSF).unwrap_err()).0, 400);
}

#[test]
fn a_phrase_check_is_a_relay_between_exactly_two_accounts() {
    let s = server();
    let t = HttpTransport::new(&s.url).unwrap();
    let alice = sign_in(&s.url, "alice@example.com", &Device::new(), true);
    let bob = sign_in(&s.url, "bob@example.com", &Device::new(), true);
    let carol = sign_in(&s.url, "carol@example.com", &Device::new(), true);
    let bob_account = account_of(&t, &bob.token);
    let opened = t.verify_open(&alice.token, bob_account).unwrap();
    let listed = t.verify_list(&bob.token).unwrap();
    assert_eq!(listed.open.len(), 1);
    assert_eq!(listed.open[0].verify_id, opened.verify_id);
    t.verify_push(&bob.token, opened.verify_id, b"ke1").unwrap();
    assert_eq!(
        t.verify_poll(&alice.token, opened.verify_id, 0)
            .unwrap()
            .frames,
        vec![ByteBuf::from(b"ke1".to_vec())]
    );
    assert!(
        t.verify_poll(&bob.token, opened.verify_id, 0)
            .unwrap()
            .frames
            .is_empty()
    );
    assert_eq!(
        refused(
            t.verify_poll(&carol.token, opened.verify_id, 0)
                .unwrap_err()
        )
        .0,
        403
    );
    assert!(
        t.verify_open(&alice.token, Id::new()).is_err(),
        "no such account"
    );
}

fn account_of(t: &HttpTransport, token: &str) -> Id {
    lockdown_sync::Transport::kt_entries(t, token, 0)
        .unwrap()
        .account_id
}
