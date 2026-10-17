//! Two local stores (A and B) against one in-process server, through the
//! sync engine and the HTTP transport: A pushes; B pulls; both edit the
//! same item; B's push is stale; the three-way merge resolves different
//! fields and makes a conflict copy for the same one; manifest heads
//! advance on both; a head a revoked device signed after its revocation,
//! let through by a server that should have refused it, is refused by
//! the other device on pull; a server put back from an older copy is
//! refused as a rollback.

use std::collections::BTreeMap;
use std::sync::Arc;

use lockdown_crypto::auth::{DeviceBinding, OpaqueAccountAuth};
use lockdown_crypto::identity::{AppDataKey, SymmetricKey};
use lockdown_crypto::{SigningKey, VerifyingKey};
use lockdown_server::keys::ServerKeys;
use lockdown_server::storage::SqliteStorage;
use lockdown_server::{AppState, Config, RunningServer, run_in_background};
use lockdown_store::MemoryStore;
use lockdown_sync::wire::{ItemPush, PushReply};
use lockdown_sync::{
    HttpTransport, SyncError, Syncer, Transport, VaultSyncState, bootstrap_vault, sync_vault,
};
use lockdown_vault::test_support::{sample_context, sample_login_fields};
use lockdown_vault::{
    FieldValue, Id, ItemClass, ItemKind, PlainField, Unlock, Vault, VaultContext,
};

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

struct Client {
    transport: HttpTransport,
    token: String,
    label: &'static str,
}

impl Client {
    fn syncer(&self) -> Syncer<'_> {
        Syncer {
            transport: &self.transport,
            token: &self.token,
            device_label: self.label,
        }
    }
}

fn server(dir: &std::path::Path, name: &str) -> (Arc<AppState>, RunningServer) {
    let storage = SqliteStorage::open(&dir.join(name)).unwrap();
    let keys = ServerKeys::load_or_create(&dir.join("server-key.cbor")).unwrap();
    let state = Arc::new(AppState::new(Box::new(storage), keys, Config::default()));
    let running = run_in_background(state.clone()).unwrap();
    (state, running)
}

fn login(
    url: &str,
    login_name: &str,
    password: &str,
    device: &Device,
    label: &'static str,
) -> Client {
    let transport = HttpTransport::new(url).unwrap();
    let mut auth = OpaqueAccountAuth::new(transport, login_name, device.binding());
    let (_export, session) = auth.sign_in(password).unwrap();
    Client {
        transport: auth.into_rounds(),
        token: session.token,
        label,
    }
}

fn field<'a>(fields: &'a [PlainField], id: &str) -> &'a str {
    match &fields.iter().find(|f| f.field_id == id).unwrap().value {
        FieldValue::Text(t) => t,
        other => panic!("{other:?}"),
    }
}

fn with(fields: &[PlainField], id: &str, value: &str) -> Vec<PlainField> {
    fields
        .iter()
        .cloned()
        .map(|mut f| {
            if f.field_id == id {
                f.value = FieldValue::Text(value.into());
            }
            f
        })
        .collect()
}

fn load_b(
    store: &MemoryStore,
    ctx: &VaultContext,
    adk: &AppDataKey,
    device_b: &Device,
    device_a: &Device,
    last_accepted: u64,
) -> Vault {
    let known = BTreeMap::from([(device_a.id, device_a.public())]);
    let known_fn = |id: Id| known.get(&id).cloned();
    let mut vault = Vault::load(
        store,
        ctx.vault_id,
        Unlock {
            app_data_key: adk,
            crown_jewel_key: None,
            device_id: device_b.id,
            signer: Box::new(device_b.key()),
            known_devices: &known_fn,
            revoked_after: &|_| None,
            last_accepted_seq: last_accepted,
        },
    )
    .unwrap();
    vault.trust_device(device_a.id, device_a.public());
    vault
}

#[test]
fn two_stores_one_server_merge_conflict_and_rollback() {
    let dir = tempfile::tempdir().unwrap();
    let (state, running) = server(dir.path(), "one.sqlite");
    let url = running.url();
    let (device_a, device_b) = (Device::new(), Device::new());
    let profile = Id::new();

    // A registers the account; B logs in to it with its own device key.
    let mut auth_a = OpaqueAccountAuth::new(
        HttpTransport::new(&url).unwrap(),
        "me@example.com",
        device_a.binding(),
    );
    auth_a
        .register_account(&profile, "correct horse battery staple")
        .unwrap();
    let a = login(
        &url,
        "me@example.com",
        "correct horse battery staple",
        &device_a,
        "laptop",
    );
    let b = login(
        &url,
        "me@example.com",
        "correct horse battery staple",
        &device_b,
        "phone",
    );

    // A's vault, with one login, pushed.
    let adk = AppDataKey::generate().unwrap();
    let ctx = sample_context();
    let mut vault_a = Vault::create(ctx, device_a.id, Box::new(device_a.key()), None).unwrap();
    vault_a.trust_device(device_b.id, device_b.public());
    let record = vault_a.record(&adk).unwrap().to_bytes().unwrap();
    let item = vault_a
        .create_item(ItemKind::Login, ItemClass::Standard, sample_login_fields())
        .unwrap();
    assert_eq!(vault_a.seq(), 2, "create + item, both local");
    let mut state_a = VaultSyncState::default();
    let report = sync_vault(&a.syncer(), &mut vault_a, &record, &mut state_a).unwrap();
    assert_eq!((report.pushed, report.pulled, report.merged), (1, 0, 0));
    assert_eq!(report.head_seq, 1, "the local heads were squashed into one");
    assert!(report.chain_rewritten);
    assert_eq!(state_a.synced_seq, 1);
    assert_eq!(vault_a.item(item).unwrap().rev, 1);
    let mut store_a = MemoryStore::new();
    vault_a.save(&mut store_a, &adk).unwrap();

    // B pulls from nothing: the record, the chain, the items.
    let mut store_b = MemoryStore::new();
    let list = b.transport.list_vaults(&b.token).unwrap();
    assert_eq!(list.vaults.len(), 1);
    let mut state_b = bootstrap_vault(&b.syncer(), &mut store_b, &list.vaults[0])
        .unwrap()
        .expect("the server holds a head");
    assert_eq!(state_b.synced_seq, 1);
    let mut vault_b = load_b(&store_b, &ctx, &adk, &device_b, &device_a, 0);
    assert_eq!(vault_b.seq(), 1);
    assert_eq!(
        field(&vault_b.open_item(item).unwrap(), "pw"),
        "correct horse battery staple"
    );

    // Both edit the same field. A syncs first; B's state is stale.
    let base = vault_a.open_item(item).unwrap();
    vault_a
        .update_item(item, with(&base, "pw", "a-one"))
        .unwrap();
    let report = sync_vault(&a.syncer(), &mut vault_a, &record, &mut state_a).unwrap();
    assert_eq!(report.pushed, 1);
    assert_eq!(report.head_seq, 2);
    vault_b
        .update_item(item, with(&base, "pw", "b-one"))
        .unwrap();
    let report = sync_vault(&b.syncer(), &mut vault_b, &record, &mut state_b).unwrap();
    assert_eq!(report.merged, 1, "{report:?}");
    assert_eq!(report.conflict_copies.len(), 1, "{report:?}");
    assert_eq!(report.pushed, 1, "the conflict copy went up");
    assert_eq!(report.head_seq, 3);
    assert!(report.chain_rewritten);
    // The server's value stays in the item; B's whole version is the copy.
    assert_eq!(field(&vault_b.open_item(item).unwrap(), "pw"), "a-one");
    let copy = report.conflict_copies[0];
    let copy_fields = vault_b.open_item(copy).unwrap();
    assert_eq!(field(&copy_fields, "pw"), "b-one");
    assert!(field(&copy_fields, "title").contains("(conflict copy from phone, 20"));
    vault_b.save(&mut store_b, &adk).unwrap();
    // B's store reopens and verifies with A's and its own signatures.
    let vault_b2 = load_b(&store_b, &ctx, &adk, &device_b, &device_a, 3);
    assert_eq!(vault_b2.seq(), 3);
    assert_eq!(vault_b2.item_ids().len(), 2);

    // A pulls the copy; nothing of A's is dirty, so no new head.
    let report = sync_vault(&a.syncer(), &mut vault_a, &record, &mut state_a).unwrap();
    assert_eq!((report.pushed, report.pulled, report.merged), (0, 1, 0));
    assert_eq!(report.head_seq, 3);
    assert!(field(&vault_a.open_item(copy).unwrap(), "title").contains("conflict copy"));
    vault_a.save(&mut store_a, &adk).unwrap();

    // Different fields merge cleanly, with no copy.
    let base = vault_a.open_item(item).unwrap();
    vault_a
        .update_item(item, with(&base, "user", "me@laptop"))
        .unwrap();
    sync_vault(&a.syncer(), &mut vault_a, &record, &mut state_a).unwrap();
    vault_b
        .update_item(item, with(&base, "title", "Example, renamed on the phone"))
        .unwrap();
    let report = sync_vault(&b.syncer(), &mut vault_b, &record, &mut state_b).unwrap();
    assert_eq!(report.merged, 1);
    assert!(report.conflict_copies.is_empty());
    let merged = vault_b.open_item(item).unwrap();
    assert_eq!(field(&merged, "user"), "me@laptop");
    assert_eq!(field(&merged, "title"), "Example, renamed on the phone");
    assert_eq!(report.head_seq, 5);
    let report = sync_vault(&a.syncer(), &mut vault_a, &record, &mut state_a).unwrap();
    assert_eq!(report.pulled, 1);
    assert_eq!(
        field(&vault_a.open_item(item).unwrap(), "title"),
        "Example, renamed on the phone"
    );
    assert_eq!(vault_a.seq(), 5);
    assert_eq!(vault_b.seq(), 5);

    // The server's own stale answer: a push behind the current revision
    // comes back as the merge instruction with the server's copy.
    let current = vault_a.item(item).unwrap().clone();
    match a
        .transport
        .push_item(
            &a.token,
            ctx.vault_id,
            item,
            &ItemPush {
                if_match: 1,
                item: current.to_bytes().unwrap(),
            },
        )
        .unwrap()
    {
        PushReply::Stale { current: theirs } => assert_eq!(theirs.rev, current.rev),
        other => panic!("expected stale, got {other:?}"),
    }
    // And through the engine: A pushes an item without a head (a device
    // that stopped half-way); B's push of the same item is stale, B merges
    // what the server returned and finishes the round.
    let base = vault_a.open_item(item).unwrap();
    vault_a
        .update_item(item, with(&base, "user", "me@laptop-again"))
        .unwrap();
    let half = vault_a.item(item).unwrap().clone();
    match a
        .transport
        .push_item(
            &a.token,
            ctx.vault_id,
            item,
            &ItemPush {
                if_match: half.rev,
                item: half.to_bytes().unwrap(),
            },
        )
        .unwrap()
    {
        PushReply::Accepted { rev } => vault_a.set_item_rev(item, rev).unwrap(),
        other => panic!("{other:?}"),
    }
    vault_b
        .update_item(item, with(&base, "title", "Example, once more"))
        .unwrap();
    let report = sync_vault(&b.syncer(), &mut vault_b, &record, &mut state_b).unwrap();
    assert_eq!(report.merged, 1, "{report:?}");
    assert!(report.rounds >= 2, "{report:?}");
    assert!(report.conflict_copies.is_empty());
    let merged = vault_b.open_item(item).unwrap();
    assert_eq!(field(&merged, "user"), "me@laptop-again");
    assert_eq!(field(&merged, "title"), "Example, once more");
    let report = sync_vault(&a.syncer(), &mut vault_a, &record, &mut state_a).unwrap();
    assert_eq!(
        report.merged, 1,
        "A's half-pushed change meets B's merge: {report:?}"
    );
    assert!(report.conflict_copies.is_empty());
    assert_eq!(
        field(&vault_a.open_item(item).unwrap(), "title"),
        "Example, once more"
    );
    assert_eq!(state_a.synced_seq, state_b.synced_seq);

    // Revocation on the client side (D73): A's synced identity says B is
    // revoked after the server's current head. A server that fails to
    // refuse B's next head (here: one that was never told) serves it, and
    // A refuses it on pull without changing anything.
    let boundary = state_a.synced_seq;
    vault_a.revoke_device(device_b.id, boundary);
    let base = vault_b.open_item(item).unwrap();
    vault_b
        .update_item(item, with(&base, "pw", "after revocation"))
        .unwrap();
    let report = sync_vault(&b.syncer(), &mut vault_b, &record, &mut state_b).unwrap();
    assert_eq!(report.head_seq, boundary + 1, "the server took it");
    let before = vault_a.seq();
    let err = sync_vault(&a.syncer(), &mut vault_a, &record, &mut state_a).unwrap_err();
    match err {
        SyncError::Vault(lockdown_vault::Error::RevokedDevice { device, seq, after }) => {
            assert_eq!(device, device_b.id);
            assert_eq!(seq, boundary + 1);
            assert_eq!(after, boundary);
        }
        other => panic!("expected a revoked-device refusal, got {other}"),
    }
    assert_eq!(vault_a.seq(), before, "nothing changed locally");
    assert_eq!(
        field(&vault_a.open_item(item).unwrap(), "pw"),
        "a-one",
        "B's late value never arrived"
    );
    // What B signed before the revocation still verifies on A's side.
    vault_a.save(&mut store_a, &adk).unwrap();
    let known = BTreeMap::from([(device_b.id, device_b.public())]);
    let known_fn = |id: Id| known.get(&id).cloned();
    let revoked = |id: Id| (id == device_b.id).then_some(boundary);
    let reopened = Vault::load(
        &store_a,
        ctx.vault_id,
        Unlock {
            app_data_key: &adk,
            crown_jewel_key: None,
            device_id: device_a.id,
            signer: Box::new(device_a.key()),
            known_devices: &known_fn,
            revoked_after: &revoked,
            last_accepted_seq: boundary,
        },
    )
    .unwrap();
    assert_eq!(reopened.seq(), boundary);
    // For the rest of the test, let A forget the revocation and catch up
    // (a test convenience: the real A would stop here).
    let mut vault_a = load_b(&store_a, &ctx, &adk, &device_a, &device_b, boundary);
    vault_a.trust_device(device_b.id, device_b.public());
    let report = sync_vault(&a.syncer(), &mut vault_a, &record, &mut state_a).unwrap();
    assert_eq!(report.head_seq, boundary + 1);
    assert_eq!(
        field(&vault_a.open_item(item).unwrap(), "pw"),
        "after revocation"
    );

    // Rollback: a copy of the server from before the last heads, served
    // again. A device that has seen further refuses it and writes nothing.
    let synced = state_a.synced_seq;
    let snapshot = dir.path().join("older.sqlite");
    state.storage.snapshot_to(&snapshot).unwrap();
    let base = vault_a.open_item(item).unwrap();
    vault_a
        .update_item(item, with(&base, "pw", "after the snapshot"))
        .unwrap();
    let report = sync_vault(&a.syncer(), &mut vault_a, &record, &mut state_a).unwrap();
    assert_eq!(report.head_seq, synced + 1);
    drop(running);
    let (_older_state, older) = server(dir.path(), "older.sqlite");
    let a_old = Client {
        transport: HttpTransport::new(&older.url()).unwrap(),
        token: a.token.clone(),
        label: "laptop",
    };
    let before = vault_a.seq();
    let err = sync_vault(&a_old.syncer(), &mut vault_a, &record, &mut state_a).unwrap_err();
    match err {
        SyncError::Rollback {
            offered,
            remembered,
        } => {
            assert_eq!(offered, synced);
            assert_eq!(remembered, synced + 1);
        }
        other => panic!("expected a rollback refusal, got {other}"),
    }
    assert!(err.to_string().starts_with("Rollback attempt blocked"));
    assert_eq!(vault_a.seq(), before, "nothing changed locally");
    assert_eq!(state_a.synced_seq, synced + 1);
}
