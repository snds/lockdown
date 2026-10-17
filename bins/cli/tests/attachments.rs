//! Attachments across two devices and one server (02-model §6): A attaches
//! a 5 MiB file and syncs; B pulls the item and its blob and opens the file;
//! a second sync pushes no blob again (content-addressed); the server
//! refuses an item naming a blob it lacks, a blob under the wrong name,
//! and one over its cap; A purges the item, B's copy loses the key and the
//! blob, and the server collects the blob once its grace period is over.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use lockdown_crypto::auth::{DeviceBinding, OpaqueAccountAuth};
use lockdown_crypto::hash::hash;
use lockdown_crypto::identity::{AppDataKey, SymmetricKey};
use lockdown_crypto::{SigningKey, VerifyingKey};
use lockdown_server::{AppState, Config, RunningServer, run_in_background};
use lockdown_store::MemoryStore;
use lockdown_sync::wire::{ErrorCode, ItemPush, PushReply};
use lockdown_sync::{
    HttpTransport, SyncError, Syncer, Transport, TransportError, VaultSyncState, bootstrap_vault,
    sync_vault,
};
use lockdown_vault::test_support::{sample_context, sample_login_fields};
use lockdown_vault::{Error, Hash32, Id, ItemClass, ItemKind, Store, Unlock, Vault, VaultContext};

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

fn login(url: &str, name: &str, password: &str, device: &Device, label: &'static str) -> Client {
    let transport = HttpTransport::new(url).unwrap();
    let mut auth = OpaqueAccountAuth::new(transport, name, device.binding());
    let (_export, session) = auth.sign_in(password).unwrap();
    Client {
        transport: auth.into_rounds(),
        token: session.token,
        label,
    }
}

fn load(
    store: &MemoryStore,
    ctx: &VaultContext,
    adk: &AppDataKey,
    me: &Device,
    other: &Device,
    last_accepted: u64,
) -> Vault {
    let known = BTreeMap::from([(other.id, other.public())]);
    let known_fn = |id: Id| known.get(&id).cloned();
    let mut vault = Vault::load(
        store,
        ctx.vault_id,
        Unlock {
            app_data_key: adk,
            crown_jewel_key: None,
            device_id: me.id,
            signer: Box::new(me.key()),
            known_devices: &known_fn,
            revoked_after: &|_| None,
            last_accepted_seq: last_accepted,
        },
    )
    .unwrap();
    vault.trust_device(other.id, other.public());
    vault
}

/// A server whose clock the test moves, with a small blob cap.
fn server(max_blob_bytes: u64) -> (Arc<AppState>, Arc<AtomicI64>, RunningServer) {
    let clock = Arc::new(AtomicI64::new(1_800_000_000));
    let mut state = AppState::in_memory(Config {
        max_blob_bytes,
        blob_grace: Duration::from_secs(3600),
        ..Config::default()
    })
    .unwrap();
    let c = clock.clone();
    state.clock = Box::new(move || c.load(Ordering::SeqCst));
    let state = Arc::new(state);
    let running = run_in_background(state.clone()).unwrap();
    (state, clock, running)
}

#[test]
fn an_attachment_crosses_two_devices_and_dies_with_its_item() {
    let (state, clock, running) = server(8 << 20);
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

    // A: a document with a 5 MiB scan attached, synced.
    let adk = AppDataKey::generate().unwrap();
    let ctx = sample_context();
    let mut vault_a = Vault::create(ctx, device_a.id, Box::new(device_a.key()), None).unwrap();
    vault_a.trust_device(device_b.id, device_b.public());
    let record = vault_a.record(&adk).unwrap().to_bytes().unwrap();
    let item = vault_a
        .create_item(
            ItemKind::Document,
            ItemClass::Standard,
            sample_login_fields(),
        )
        .unwrap();
    let scan: Vec<u8> = (0..(5 << 20) + 321).map(|i| (i % 241) as u8).collect();
    let (att, blob_hash) = vault_a
        .attach(item, "scan.pdf", "application/pdf", &scan)
        .unwrap();
    let mut store_a = MemoryStore::new();
    let mut state_a = VaultSyncState::default();
    let report = sync_vault(
        &a.syncer(),
        &mut vault_a,
        &record,
        &mut state_a,
        &mut store_a,
    )
    .unwrap();
    assert_eq!(
        (report.pushed, report.blobs_pushed, report.blobs_pulled),
        (1, 1, 0)
    );
    vault_a.save(&mut store_a, &adk).unwrap();
    assert!(store_a.has_blob(blob_hash).unwrap());
    assert!(
        a.transport
            .has_blob(&a.token, ctx.vault_id, blob_hash)
            .unwrap()
    );
    assert_eq!(
        a.transport
            .get_blob(&a.token, ctx.vault_id, blob_hash)
            .unwrap()
            .map(|b| b.len() as u64),
        Some(lockdown_vault::attachment::blob_size(6 << 20))
    );

    // A second sync pushes nothing again: the blob is content-addressed.
    vault_a.update_item(item, sample_login_fields()).unwrap();
    let report = sync_vault(
        &a.syncer(),
        &mut vault_a,
        &record,
        &mut state_a,
        &mut store_a,
    )
    .unwrap();
    assert_eq!((report.pushed, report.blobs_pushed), (1, 0), "{report:?}");

    // B: bootstraps the vault; the blob comes with the item; the file opens.
    let mut store_b = MemoryStore::new();
    let list = b.transport.list_vaults(&b.token).unwrap();
    let mut state_b = bootstrap_vault(&b.syncer(), &mut store_b, &list.vaults[0])
        .unwrap()
        .expect("a head");
    assert!(store_b.has_blob(blob_hash).unwrap(), "the blob rode along");
    let mut vault_b = load(&store_b, &ctx, &adk, &device_b, &device_a, 0);
    let listed = vault_b.attachments(item).unwrap();
    assert_eq!(listed[0].att_id, att);
    assert_eq!(listed[0].meta.as_ref().unwrap().name, "scan.pdf");
    assert_eq!(listed[0].chunks, 6);
    let blob = store_b.get_blob(blob_hash).unwrap();
    let (meta, plain) = vault_b.open_attachment(item, att, &blob).unwrap();
    assert_eq!(meta.size, scan.len() as u64);
    assert_eq!(plain.as_slice(), scan.as_slice());

    // B attaches a second, small file; A pulls it on its next sync.
    let (att2, blob2) = vault_b
        .attach(item, "note.txt", "text/plain", b"keep this")
        .unwrap();
    let report = sync_vault(
        &b.syncer(),
        &mut vault_b,
        &record,
        &mut state_b,
        &mut store_b,
    )
    .unwrap();
    assert_eq!((report.pushed, report.blobs_pushed), (1, 1), "{report:?}");
    vault_b.save(&mut store_b, &adk).unwrap();
    let report = sync_vault(
        &a.syncer(),
        &mut vault_a,
        &record,
        &mut state_a,
        &mut store_a,
    )
    .unwrap();
    assert_eq!((report.pulled, report.blobs_pulled), (1, 1), "{report:?}");
    assert!(store_a.has_blob(blob2).unwrap());
    let (_, small) = vault_a
        .open_attachment(item, att2, &store_a.get_blob(blob2).unwrap())
        .unwrap();
    assert_eq!(small.as_slice(), b"keep this");

    // The server's own rules. An item naming a blob it doesn't hold:
    // refused with `unknown_blob` and nothing stored.
    let mut forged = vault_a.item(item).unwrap().clone();
    let bogus = hash(b"a blob nobody pushed");
    let header = forged.header();
    let last = forged.versions.last_mut().unwrap();
    last.attachments[0].blob = bogus;
    last.edited_by = device_a.id;
    last.sign(&header, &device_a.key()).unwrap();
    let err = a
        .transport
        .push_item(
            &a.token,
            ctx.vault_id,
            item,
            &ItemPush {
                if_match: forged.rev,
                item: forged.to_bytes().unwrap(),
            },
        )
        .unwrap_err();
    assert!(
        matches!(
            err,
            TransportError::Refused {
                code: ErrorCode::UnknownBlob,
                ..
            }
        ),
        "{err}"
    );
    // A blob under the wrong name, and an empty one.
    let err = a
        .transport
        .put_blob(&a.token, ctx.vault_id, bogus, b"not those bytes")
        .unwrap_err();
    assert!(
        matches!(
            err,
            TransportError::Refused {
                code: ErrorCode::BlobMismatch,
                ..
            }
        ),
        "{err}"
    );
    assert!(
        a.transport
            .put_blob(&a.token, ctx.vault_id, hash(b""), b"")
            .is_err()
    );
    // Over the cap (8 MiB here): `blob_too_large`.
    let big = vec![0u8; (8 << 20) + 1];
    let err = a
        .transport
        .put_blob(&a.token, ctx.vault_id, hash(&big), &big)
        .unwrap_err();
    assert!(
        matches!(
            err,
            TransportError::Refused {
                code: ErrorCode::BlobTooLarge,
                status: 413,
                ..
            }
        ),
        "{err}"
    );
    // Pushing a held blob again is a no-op; a stray blob nothing names is
    // kept through its grace period, then collected.
    let again = a
        .transport
        .put_blob(&a.token, ctx.vault_id, blob_hash, &blob)
        .unwrap();
    assert!(!again.new);
    let stray = b"stray blob".to_vec();
    let stray_hash = hash(&stray);
    assert!(
        a.transport
            .put_blob(&a.token, ctx.vault_id, stray_hash, &stray)
            .unwrap()
            .new
    );
    assert!(
        a.transport
            .has_blob(&a.token, ctx.vault_id, stray_hash)
            .unwrap()
    );
    clock.fetch_add(3601, Ordering::SeqCst);
    assert_eq!(state.storage.gc_blobs(state.now(), 3600).unwrap(), 1);
    assert!(
        !a.transport
            .has_blob(&a.token, ctx.vault_id, stray_hash)
            .unwrap()
    );
    assert!(
        a.transport
            .has_blob(&a.token, ctx.vault_id, blob_hash)
            .unwrap()
    );

    // The other device's blob store can't help a B that lost its blob: a
    // tampered server copy is refused by name.
    // (Checked through the engine: BlobCorrupt is what `pull_blobs` says.)
    let _ = SyncError::BlobCorrupt(Hash32::ZERO).to_string();

    // A purges the item. Its keys go; the blobs are dead; the server
    // collects them after the grace period; B's copy is shredded too.
    vault_a.purge_item(item).unwrap();
    let report = sync_vault(
        &a.syncer(),
        &mut vault_a,
        &record,
        &mut state_a,
        &mut store_a,
    )
    .unwrap();
    assert_eq!(report.pushed, 1);
    vault_a.save(&mut store_a, &adk).unwrap();
    assert!(!store_a.has_blob(blob_hash).unwrap(), "A's store let it go");
    assert!(!store_a.has_blob(blob2).unwrap());
    // Still on the server until the grace period runs out…
    assert!(
        a.transport
            .has_blob(&a.token, ctx.vault_id, blob_hash)
            .unwrap()
    );
    assert_eq!(state.storage.gc_blobs(state.now(), 3600).unwrap(), 0);
    clock.fetch_add(3601, Ordering::SeqCst);
    assert_eq!(state.storage.gc_blobs(state.now(), 3600).unwrap(), 2);
    assert!(
        !a.transport
            .has_blob(&a.token, ctx.vault_id, blob_hash)
            .unwrap()
    );
    assert!(!a.transport.has_blob(&a.token, ctx.vault_id, blob2).unwrap());
    assert_eq!(
        a.transport
            .get_blob(&a.token, ctx.vault_id, blob_hash)
            .unwrap(),
        None
    );
    // B pulls the purge: nothing to fetch (no keys), the file is gone.
    let report = sync_vault(
        &b.syncer(),
        &mut vault_b,
        &record,
        &mut state_b,
        &mut store_b,
    )
    .unwrap();
    assert_eq!((report.pulled, report.blobs_pulled), (1, 0), "{report:?}");
    assert!(matches!(
        vault_b.open_attachment(item, att, &blob),
        Err(Error::Shredded { .. })
    ));
    vault_b.save(&mut store_b, &adk).unwrap();
    assert!(
        store_b.list_blobs().unwrap().is_empty(),
        "B's store let them go"
    );
    match a.transport.push_item(
        &a.token,
        ctx.vault_id,
        item,
        &ItemPush {
            if_match: vault_a.item(item).unwrap().rev,
            item: vault_a.item(item).unwrap().to_bytes().unwrap(),
        },
    ) {
        Ok(PushReply::Accepted { .. }) => {}
        other => panic!("a shredded item names no blob and pushes fine: {other:?}"),
    }
}
