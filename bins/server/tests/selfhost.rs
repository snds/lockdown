//! The self-host server's own TLS, sessions and live channel, driven by
//! Lockdown's real client (`lockdown_sync::HttpTransport` with the OPAQUE
//! login) over a real socket:
//!
//! - hybrid X25519MLKEM768 TLS between Lockdown's client and server, HSTS
//!   on every answer, and no way in for plain HTTP or an untrusted CA;
//! - a device sees every session of the account, ends another one, and the
//!   ended token stops working at once;
//! - a write on one device reaches the other's live channel as a
//!   content-free event, not the writer's own; an ended session hears so
//!   and its stream closes;
//! - attachment bodies kept as files (`--blob-dir`) round-trip and are
//!   collected with their rows.

use std::sync::Arc;
use std::time::Duration;

use lockdown_crypto::SigningKey;
use lockdown_crypto::auth::{DeviceBinding, OpaqueAccountAuth, Session};
use lockdown_crypto::identity::{AppDataKey, SymmetricKey};
use lockdown_server::storage::{SqliteStorage, Storage};
use lockdown_server::{AppState, Config, run_tls_in_background};
use lockdown_sync::wire::{ErrorCode, LiveKind};
use lockdown_sync::{HttpTransport, Transport, TransportError};
use lockdown_tls::testing::pki;
use lockdown_tls::{Groups, ReloadingCert, Trust};
use lockdown_vault::test_support::sample_context;
use lockdown_vault::{Id, Vault};

struct Tls {
    _dir: tempfile::TempDir,
    trust: Trust,
    config: Arc<rustls::ServerConfig>,
}

fn tls() -> Tls {
    let dir = tempfile::tempdir().unwrap();
    let files = pki(&["localhost", "127.0.0.1"]).write(dir.path());
    let cert = ReloadingCert::load(
        &files.cert,
        &files.key,
        Arc::new(lockdown_tls::provider(Groups::PreferHybrid)),
    )
    .unwrap();
    Tls {
        trust: Trust::Only(lockdown_tls::read_certs(&files.ca).unwrap()),
        config: Arc::new(lockdown_tls::server_config(cert, Groups::PreferHybrid).unwrap()),
        _dir: dir,
    }
}

fn config() -> Config {
    Config {
        per_login: (100, 10.0),
        ..Config::default()
    }
}

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

    fn binding(&self) -> DeviceBinding {
        let seed = self.key;
        DeviceBinding {
            device_id: self.id,
            device_pub: SigningKey::from_bytes(seed).verifying_key().to_bytes(),
            sign: Box::new(move |m| Ok(SigningKey::from_bytes(seed).sign(m))),
        }
    }
}

const PASSWORD: &str = "correct horse battery staple";

/// Registers (once) and signs in `device` with Lockdown's own client.
fn sign_in(url: &str, trust: &Trust, login: &str, device: &Device, register: bool) -> Session {
    let transport = HttpTransport::with_trust(url, trust).unwrap();
    let mut auth = OpaqueAccountAuth::new(transport, login, device.binding());
    if register {
        auth.register_account(&Id::new(), PASSWORD).unwrap();
    }
    auth.sign_in(PASSWORD).unwrap().1
}

#[test]
fn lockdowns_client_and_server_speak_hybrid_tls_end_to_end() {
    let t = tls();
    let state = Arc::new(AppState::in_memory(config()).unwrap());
    let server = run_tls_in_background(state, t.config.clone()).unwrap();
    let url = server.url();
    assert!(url.starts_with("https://127.0.0.1:"));

    let laptop = Device::new();
    let session = sign_in(&url, &t.trust, "me@example.com", &laptop, true);
    let transport = HttpTransport::with_trust(&url, &t.trust).unwrap();
    assert!(transport.list_vaults(&session.token).unwrap().vaults.is_empty());
    let sth = transport.kt_sth().unwrap();
    assert!(sth.sth.size >= 1, "the device's login is in the log");

    // Every handshake so far was the hybrid one; nothing fell back.
    let counts = server.tls_stats.as_ref().unwrap().counts();
    assert!(counts.hybrid >= 2, "{counts:?}");
    assert_eq!(counts.classical, 0, "{counts:?}");

    // HSTS on the answers.
    let raw = reqwest::blocking::Client::builder()
        .tls_backend_preconfigured(lockdown_tls::client_config(&t.trust).unwrap())
        .build()
        .unwrap();
    let res = raw.get(format!("{url}/healthz")).send().unwrap();
    assert_eq!(
        res.headers()
            .get("strict-transport-security")
            .and_then(|v| v.to_str().ok()),
        Some("max-age=31536000")
    );

    // Plain HTTP to the TLS port gets nothing; a client that doesn't trust
    // this CA is refused before any request.
    let plain = format!("http://{}", server.addr);
    assert!(matches!(
        HttpTransport::with_trust(&plain, &t.trust).unwrap().kt_sth(),
        Err(TransportError::Network(_)) | Err(TransportError::Decode(_)) | Err(TransportError::Refused { .. })
    ));
    let other = pki(&["localhost", "127.0.0.1"]);
    let stranger = HttpTransport::with_trust(&url, &Trust::Only(vec![other.ca_der])).unwrap();
    assert!(matches!(stranger.kt_sth(), Err(TransportError::Network(_))));
}

#[test]
fn a_device_lists_the_sessions_and_ends_another_whose_token_then_stops() {
    let t = tls();
    let state = Arc::new(AppState::in_memory(config()).unwrap());
    let server = run_tls_in_background(state, t.config.clone()).unwrap();
    let url = server.url();
    let (laptop, phone) = (Device::new(), Device::new());
    let a = sign_in(&url, &t.trust, "me@example.com", &laptop, true);
    let b = sign_in(&url, &t.trust, "me@example.com", &phone, false);
    let b2 = sign_in(&url, &t.trust, "me@example.com", &phone, false);
    let ta = HttpTransport::with_trust(&url, &t.trust).unwrap();

    let list = ta.sessions(&a.token).unwrap().sessions;
    assert_eq!(list.len(), 3);
    let mine: Vec<_> = list.iter().filter(|s| s.current).collect();
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0].device_id, laptop.id);
    for s in &list {
        assert_eq!(s.address.as_deref(), Some("127.0.0.1"));
        assert!(
            s.client.as_deref().unwrap_or("").starts_with("lockdown/"),
            "{:?}",
            s.client
        );
        assert!(s.expires_at > s.created_at);
    }
    let phones: Vec<_> = list.iter().filter(|s| s.device_id == phone.id).collect();
    assert_eq!(phones.len(), 2);

    // End one of the phone's sessions: that token stops, the other works.
    ta.end_session(&a.token, &phones[0].id).unwrap();
    let tb = HttpTransport::with_trust(&url, &t.trust).unwrap();
    let ended = |token: &str| match tb.list_vaults(token) {
        Err(TransportError::Refused { status, code, message, .. }) => {
            assert_eq!((status, code), (401, ErrorCode::Unauthenticated));
            assert!(message.contains("ended"), "{message}");
            true
        }
        Ok(_) => false,
        Err(e) => panic!("{e}"),
    };
    assert_eq!(
        [ended(&b.token), ended(&b2.token)].iter().filter(|e| **e).count(),
        1,
        "exactly one of the phone's sessions ended"
    );
    // Ending it again, or another account's session, or junk: refused.
    assert!(ta.end_session(&a.token, &phones[0].id).is_err());
    assert!(ta.end_session(&a.token, "not-an-id").is_err());

    // End every other session: the phone is out, the laptop stays in.
    let n = ta.end_other_sessions(&a.token).unwrap().ended;
    assert_eq!(n, 1);
    assert!(ended(&b.token) && ended(&b2.token));
    assert_eq!(ta.sessions(&a.token).unwrap().sessions.len(), 1);

    // Another account can't see or end these.
    let stranger = sign_in(&url, &t.trust, "you@example.com", &Device::new(), true);
    let theirs = ta.sessions(&stranger.token).unwrap().sessions;
    assert_eq!(theirs.len(), 1);
    let laptop_session = ta.sessions(&a.token).unwrap().sessions[0].id.clone();
    assert!(ta.end_session(&stranger.token, &laptop_session).is_err());
    assert!(ta.list_vaults(&a.token).is_ok());
}

#[test]
fn a_write_on_one_device_reaches_the_others_live_channel_and_not_its_own() {
    let t = tls();
    let state = Arc::new(AppState::in_memory(config()).unwrap());
    let server = run_tls_in_background(state.clone(), t.config.clone()).unwrap();
    let url = server.url();
    let (laptop, phone) = (Device::new(), Device::new());
    let a = sign_in(&url, &t.trust, "me@example.com", &laptop, true);
    let b = sign_in(&url, &t.trust, "me@example.com", &phone, false);

    let (tx, rx) = std::sync::mpsc::channel();
    let listen = |token: String, tx: std::sync::mpsc::Sender<(&'static str, LiveKind)>, who| {
        let (url, trust) = (url.clone(), t.trust.clone());
        std::thread::spawn(move || {
            let mut events = HttpTransport::with_trust(&url, &trust)
                .unwrap()
                .events(&token)
                .unwrap();
            while let Ok(Some(e)) = events.next_event() {
                assert_eq!(e.v, 1);
                if tx.send((who, e.kind)).is_err() {
                    break;
                }
            }
            let _ = tx.send((who, LiveKind::Other));
        })
    };
    let _phone_thread = listen(b.token.clone(), tx.clone(), "phone");
    let _laptop_thread = listen(a.token.clone(), tx.clone(), "laptop");
    // Both streams are open before the write.
    for _ in 0..100 {
        if state.live.open_streams() == 2 {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(state.live.open_streams(), 2);

    // The laptop claims a vault: the phone hears "sync", the laptop nothing.
    let device_key = SigningKey::from_bytes(laptop.key);
    let vault = Vault::create(sample_context(), laptop.id, Box::new(device_key), None).unwrap();
    let record = vault
        .record(&AppDataKey::generate().unwrap())
        .unwrap()
        .to_bytes()
        .unwrap();
    let ta = HttpTransport::with_trust(&url, &t.trust).unwrap();
    ta.put_vault(&a.token, vault.context().vault_id, &record)
        .unwrap();
    let got = rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(got, ("phone", LiveKind::Sync));
    assert!(
        rx.recv_timeout(Duration::from_millis(300)).is_err(),
        "the writer doesn't hear its own write"
    );

    // The laptop ends the phone's session: the phone hears it and its
    // stream closes; the laptop hears the session list changed.
    let phone_session = ta
        .sessions(&a.token)
        .unwrap()
        .sessions
        .into_iter()
        .find(|s| !s.current)
        .unwrap();
    ta.end_session(&a.token, &phone_session.id).unwrap();
    let mut seen = Vec::new();
    while seen.len() < 3 {
        seen.push(rx.recv_timeout(Duration::from_secs(10)).unwrap());
    }
    seen.sort_by_key(|(who, kind)| (*who, format!("{kind:?}")));
    assert_eq!(
        seen,
        vec![
            ("laptop", LiveKind::Sessions),
            ("phone", LiveKind::Other),
            ("phone", LiveKind::SessionEnded),
        ]
    );
    // The ended token can't open the channel again.
    assert!(
        HttpTransport::with_trust(&url, &t.trust)
            .unwrap()
            .events(&b.token)
            .is_err()
    );
}

#[test]
fn blob_bodies_kept_as_files_round_trip_and_go_with_their_rows() {
    let dir = tempfile::tempdir().unwrap();
    let blobs = dir.path().join("blobs");
    let storage = SqliteStorage::open(&dir.path().join("db.sqlite"))
        .unwrap()
        .with_blob_dir(&blobs)
        .unwrap();
    let owner = storage
        .create_account("me@example.com", Id::new(), b"file")
        .unwrap();
    let device = Device::new();
    let vault = Vault::create(
        sample_context(),
        device.id,
        Box::new(SigningKey::from_bytes(device.key)),
        None,
    )
    .unwrap();
    let vault_id = vault.context().vault_id;
    let record = vault
        .record(&AppDataKey::generate().unwrap())
        .unwrap()
        .to_bytes()
        .unwrap();
    storage
        .put_vault(owner.account_id, vault_id, &record)
        .unwrap();
    let body = vec![7u8; 100_000];
    let hash = lockdown_crypto::hash::hash(&body);
    assert!(storage.put_blob(vault_id, hash, &body, 1_000).unwrap());
    assert!(!storage.put_blob(vault_id, hash, &body, 1_001).unwrap());
    let file = blobs.join(vault_id.to_string()).join(hash.to_hex());
    assert_eq!(std::fs::read(&file).unwrap(), body);
    assert_eq!(storage.blob(vault_id, hash).unwrap().unwrap(), body);
    assert!(storage.has_blob(vault_id, hash).unwrap());
    // The database itself holds no copy of the body.
    let db = std::fs::read(dir.path().join("db.sqlite")).unwrap();
    assert!(!db.windows(64).any(|w| w.iter().all(|b| *b == 7)));
    // Nothing names it: after the grace period both row and file go.
    assert_eq!(storage.gc_blobs(1_500, 3_600).unwrap(), 0);
    assert_eq!(storage.gc_blobs(10_000, 3_600).unwrap(), 1);
    assert!(!file.exists());
    assert!(storage.blob(vault_id, hash).unwrap().is_none());
}
