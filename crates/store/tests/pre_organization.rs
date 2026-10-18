//! A vault file written before folders, tags, favourites, archive, the
//! vault's own name and moves existed still opens, reads and takes every
//! new operation (docs/plan/15-organization.md §2).
//!
//! `fixtures/pre-organization.sqlite` was written by the build before this
//! change (commit 8c0a4f8) with fixed test keys: a Login with two versions
//! and an attachment, a Note, a Crypto wallet (a crown jewel), a Wi-Fi
//! network and a trashed Login. Its items carry no organization fields and
//! the vault has no info item.

use std::collections::BTreeSet;

use ecosystem_identity::SymmetricKey;
use ecosystem_identity::keys::{AppDataKey, CrownJewelKey};
use lockdown_crypto::SigningKey;
use lockdown_store::{SqliteStore, Store};
use lockdown_vault::field::FieldValue;
use lockdown_vault::{Id, ItemClass, ItemKind, ItemState, Organization, Unlock, Vault};

const FIXTURE: &[u8] = include_bytes!("fixtures/pre-organization.sqlite");
const VAULT: &str = "33333333-3333-4333-8333-333333333333";
const DEVICE: &str = "66666666-6666-4666-8666-666666666666";

fn open(store: &SqliteStore, last: u64) -> Vault {
    let adk = AppDataKey::from_bytes([7; 32]);
    let known = |_: Id| None;
    let revoked = |_: Id| None;
    Vault::load(
        store,
        Id::parse(VAULT).unwrap(),
        Unlock {
            app_data_key: &adk,
            crown_jewel_key: Some(CrownJewelKey::from_bytes([9; 32])),
            device_id: Id::parse(DEVICE).unwrap(),
            signer: Box::new(SigningKey::from_bytes([5; 32])),
            known_devices: &known,
            revoked_after: &revoked,
            last_accepted_seq: last,
        },
    )
    .expect("the old vault opens")
}

fn title(vault: &Vault, id: Id) -> String {
    vault
        .open_item(id)
        .unwrap()
        .into_iter()
        .find(|f| f.field_id == "title")
        .and_then(|f| match &f.value {
            FieldValue::Text(t) => Some(t.clone()),
            _ => None,
        })
        .unwrap()
}

#[test]
fn a_vault_from_before_organization_opens_and_takes_every_new_operation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vault.sqlite");
    std::fs::write(&path, FIXTURE).unwrap();
    let mut store = SqliteStore::open(&path).unwrap();
    let mut vault = open(&store, 0);
    let seq = vault.seq();

    // Everything reads as it did, with nothing organized and no name.
    let by_title = |vault: &Vault, t: &str| {
        vault
            .items()
            .find(|i| i.state != ItemState::Tombstone && title(vault, i.item_id) == t)
            .map(|i| i.item_id)
            .unwrap()
    };
    assert_eq!(vault.items().count(), 5);
    let mail = by_title(&vault, "Mail");
    assert_eq!(vault.item(mail).unwrap().versions.len(), 3);
    assert_eq!(vault.organization(mail).unwrap(), Organization::default());
    assert!(
        !vault
            .open_item(mail)
            .unwrap()
            .iter()
            .any(|f| f.field_id.starts_with("org."))
    );
    assert_eq!(vault.info().unwrap(), Default::default());
    let wallet = by_title(&vault, "Cold wallet");
    assert_eq!(vault.item(wallet).unwrap().class, ItemClass::CrownJewel);

    // The new operations work on the old items.
    let mut info = vault.info().unwrap();
    info.name = Some("Personal".into());
    let folder = info.add_folder("Email", None).unwrap();
    vault.set_info(&info).unwrap();
    vault
        .organize(
            mail,
            &Organization {
                folder: Some(folder),
                tags: vec!["work".into()],
                favourite: true,
            },
        )
        .unwrap();
    let plans = by_title(&vault, "Plans");
    vault.archive_item(plans).unwrap();
    // A move re-seals the old attachment; its blob comes from the old file.
    let carried = vault
        .carry_out(mail, &mut |hash| {
            store
                .get_blob(hash)
                .map_err(|e| lockdown_vault::Error::Invalid(e.to_string()))
        })
        .unwrap();
    assert_eq!(carried.attachments.len(), 1);
    let moved = vault.carry_in(&carried, ItemClass::Standard).unwrap();
    drop(carried);
    vault.purge_item(mail).unwrap();
    let old = vault
        .items()
        .find(|i| i.state == ItemState::Trashed)
        .unwrap()
        .item_id;
    let purged = vault.purge_due(i64::MAX, 0, &BTreeSet::new()).unwrap();
    assert_eq!(purged, [old]);
    let adk = AppDataKey::from_bytes([7; 32]);
    vault.save(&mut store, &adk).unwrap();
    let after = vault.seq();
    assert!(after > seq);
    drop(vault);

    // And all of it reads back from the file.
    let store = SqliteStore::open(&path).unwrap();
    let vault = open(&store, after);
    assert_eq!(vault.info().unwrap().name.as_deref(), Some("Personal"));
    assert_eq!(vault.organization(moved).unwrap().folder, Some(folder));
    assert_eq!(vault.item(plans).unwrap().state, ItemState::Archived);
    let files = vault.attachments(moved).unwrap();
    let blob = store.get_blob(files[0].blob).unwrap();
    let (meta, bytes) = vault
        .open_attachment(moved, files[0].att_id, &blob)
        .unwrap();
    assert_eq!(meta.name, "recovery.txt");
    assert_eq!(bytes.as_slice(), b"old recovery note");
    assert_eq!(vault.item(moved).unwrap().kind, ItemKind::Login);
}
