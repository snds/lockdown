//! The key-transparency log against the real server (04-security §6): two
//! devices log in and push a vault; each audits its account's entries with
//! inclusion proofs under the signed tree head; a later sync proves the log
//! grew by a consistency proof; a device that logs in without being in the
//! identity is the loudest refusal; a remembered tree head the server has
//! fallen behind is a rollback.

use std::collections::BTreeMap;
use std::sync::Arc;

use lockdown_crypto::auth::{DeviceBinding, OpaqueAccountAuth};
use lockdown_crypto::identity::SymmetricKey;
use lockdown_crypto::{SigningKey, VerifyingKey};
use lockdown_server::{AppState, Config, RunningServer, run_in_background};
use lockdown_store::{MemoryStore, ServerView};
use lockdown_sync::transport::Transport;
use lockdown_sync::{
    AuditExpectations, HttpTransport, KtSession, Syncer, VaultSyncState, sync_vault,
};
use lockdown_vault::test_support::{sample_context, sample_login_fields};
use lockdown_vault::transparency::LogEntry;
use lockdown_vault::{Hash32, Id, ItemClass, ItemKind, Vault};

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

    fn public(&self) -> VerifyingKey {
        self.key().verifying_key()
    }

    fn binding(&self) -> DeviceBinding {
        let key = self.key();
        DeviceBinding {
            device_id: self.id,
            device_pub: key.verifying_key().to_bytes(),
            sign: Box::new(move |m| Ok(key.sign(m))),
        }
    }
}

fn login(url: &str, name: &str, password: &str, device: &Device) -> (HttpTransport, String) {
    let transport = HttpTransport::new(url).unwrap();
    let mut auth = OpaqueAccountAuth::new(transport, name, device.binding());
    let (_export, session) = auth.sign_in(password).unwrap();
    (auth.into_rounds(), session.token)
}

/// What a device knows: the devices of its identity and the vault it holds.
struct Known<'a> {
    me: Id,
    devices: BTreeMap<Id, [u8; 32]>,
    vault: &'a Vault,
}

impl AuditExpectations for Known<'_> {
    fn device_key(&self, device_id: Id) -> Option<[u8; 32]> {
        self.devices.get(&device_id).copied()
    }
    fn own_device(&self) -> Id {
        self.me
    }
    fn identity_head(&self) -> Option<(u64, Hash32)> {
        None
    }
    fn vault_head(&self, vault_id: Id, seq: u64) -> Option<Hash32> {
        if self.vault.context().vault_id != vault_id {
            return None;
        }
        self.vault.head_at(seq).and_then(|h| h.hash().ok())
    }
}

fn server() -> (Arc<AppState>, RunningServer) {
    let state = Arc::new(AppState::in_memory(Config::default()).unwrap());
    let running = run_in_background(state.clone()).unwrap();
    (state, running)
}

#[test]
fn the_log_records_logins_and_heads_and_the_clients_audit_it() {
    let (state, running) = server();
    let url = running.url();
    let (device_a, device_b) = (Device::new(), Device::new());
    let profile = Id::new();
    let mut auth_a = OpaqueAccountAuth::new(
        HttpTransport::new(&url).unwrap(),
        "me@example.com",
        device_a.binding(),
    );
    auth_a
        .register_account(&profile, "correct horse battery staple")
        .unwrap();
    let (ta, token_a) = login(
        &url,
        "me@example.com",
        "correct horse battery staple",
        &device_a,
    );
    let (tb, token_b) = login(
        &url,
        "me@example.com",
        "correct horse battery staple",
        &device_b,
    );
    assert_eq!(state.storage.log_size().unwrap(), 2, "two device leaves");

    // A pushes a vault: one head leaf per accepted head.
    let adk = lockdown_crypto::identity::AppDataKey::generate().unwrap();
    let ctx = sample_context();
    let mut vault_a = Vault::create(ctx, device_a.id, Box::new(device_a.key()), None).unwrap();
    vault_a.trust_device(device_b.id, device_b.public());
    let record = vault_a.record(&adk).unwrap().to_bytes().unwrap();
    vault_a
        .create_item(ItemKind::Login, ItemClass::Standard, sample_login_fields())
        .unwrap();
    let mut store_a = MemoryStore::new();
    let mut state_a = VaultSyncState::default();
    let syncer_a = Syncer {
        transport: &ta,
        token: &token_a,
        device_label: "laptop",
    };
    sync_vault(&syncer_a, &mut vault_a, &record, &mut state_a, &mut store_a).unwrap();
    assert_eq!(state.storage.log_size().unwrap(), 3);

    // The STH verifies under the key the discovery document names; the
    // entries are the two devices and the head; every proof holds.
    let sth = ta.kt_sth().unwrap();
    assert_eq!(sth.sth.size, 3);
    assert_eq!(sth.log_key, state.log_key.verifying_key().to_bytes());
    let devices = BTreeMap::from([
        (device_a.id, device_a.public().to_bytes()),
        (device_b.id, device_b.public().to_bytes()),
    ]);
    let known_a = Known {
        me: device_a.id,
        devices: devices.clone(),
        vault: &vault_a,
    };
    let mut session = KtSession::begin(&ta, &token_a, None).unwrap();
    let report = session.audit(Id::new(), &known_a);
    assert!(
        report.unwrap_err().to_string().contains("another account"),
        "the account id must be the session's"
    );
    let account = state
        .storage
        .account_by_login("me@example.com")
        .unwrap()
        .unwrap()
        .account_id;
    let mut session = KtSession::begin(&ta, &token_a, None).unwrap();
    let report = session.audit(account, &known_a).unwrap();
    assert_eq!(report.checked.len(), 3);
    assert!(
        matches!(report.checked[0].leaf.entry, LogEntry::Device { device_id, .. } if device_id == device_a.id)
    );
    assert!(matches!(
        report.checked[2].leaf.entry,
        LogEntry::VaultHead { seq: 1, .. }
    ));
    assert!(session.view.own_device_logged);
    let view_a: ServerView = session.view.clone();

    // B's view: the same log, from B's seat.
    let known_b = Known {
        me: device_b.id,
        devices: devices.clone(),
        vault: &vault_a,
    };
    let mut session_b = KtSession::begin(&tb, &token_b, None).unwrap();
    session_b.audit(account, &known_b).unwrap();
    assert_eq!(session_b.view.sth.root, view_a.sth.root);

    // More heads; A's next session proves the log grew from what it saw,
    // and the two devices' citations connect.
    let base = vault_a.open_item(vault_a.item_ids()[0]).unwrap();
    vault_a.update_item(vault_a.item_ids()[0], base).unwrap();
    sync_vault(&syncer_a, &mut vault_a, &record, &mut state_a, &mut store_a).unwrap();
    let mut session = KtSession::begin(&ta, &token_a, Some(&view_a)).unwrap();
    assert_eq!(session.sth().size, 4);
    let known_a = Known {
        me: device_a.id,
        devices: devices.clone(),
        vault: &vault_a,
    };
    let report = session.audit(account, &known_a).unwrap();
    assert_eq!(report.checked.len(), 1);
    session.verify_citation(session_b.citation()).unwrap();
    session_b.verify_citation(session.citation()).unwrap();

    // A remembered tree head the server has fallen behind: rollback, loudly.
    let mut ahead = session.view.clone();
    ahead.sth.size = 10;
    let err = KtSession::begin(&ta, &token_a, Some(&ahead)).unwrap_err();
    assert!(
        err.to_string().contains("Rollback attempt blocked"),
        "{err}"
    );

    // A third device logs in with the account password but was never
    // enrolled in the identity: A's next audit refuses, naming it.
    let device_c = Device::new();
    let _ = login(
        &url,
        "me@example.com",
        "correct horse battery staple",
        &device_c,
    );
    let mut session = KtSession::begin(&ta, &token_a, Some(&session.view)).unwrap();
    let err = session.audit(account, &known_a).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains(&device_c.id.to_string()), "{msg}");
    assert!(msg.contains("doesn't list"), "{msg}");
    // Once the identity lists it (enrolment finished), the audit passes.
    let mut all = devices.clone();
    all.insert(device_c.id, device_c.public().to_bytes());
    let known_all = Known {
        me: device_a.id,
        devices: all,
        vault: &vault_a,
    };
    let mut session = KtSession::begin(&ta, &token_a, Some(&view_a)).unwrap();
    session.audit(account, &known_all).unwrap();
    assert_eq!(session.view.audited, 5);
}
