//! Ready-made vaults and items for tests in this crate and the crates above
//! it (`store`, `sync`, `cxf`). Not for apps: every key here is random and
//! thrown away.

use ecosystem_identity::SymmetricKey;
use ecosystem_identity::keys::CrownJewelKey;
use lockdown_crypto::SigningKey;

use crate::Id;
use crate::field::PlainField;
use crate::item::{Item, ItemClass, ItemKind};
use crate::vault::{Vault, VaultContext};

/// A context with fresh random ids.
pub fn sample_context() -> VaultContext {
    VaultContext {
        profile_id: Id::new(),
        persona_id: Id::new(),
        vault_id: Id::new(),
        app_data_key_id: Id::new(),
        crown_jewel_key_id: Id::new(),
    }
}

/// An empty vault with a Crown-Jewel Key and a fresh device key.
pub fn sample_vault() -> Vault {
    let cjk = CrownJewelKey::from_bytes(lockdown_crypto::rand::random_32().expect("random"));
    Vault::create(
        sample_context(),
        Id::new(),
        Box::new(SigningKey::generate().expect("random")),
        Some(cjk),
    )
    .expect("a fresh vault")
}

/// The fields of a plain login.
pub fn sample_login_fields() -> Vec<PlainField> {
    vec![
        PlainField::text("title", "title", "Example"),
        PlainField::url("url.0", "https://accounts.example.com", "exact_host"),
        PlainField::text("user", "username", "me@example.com"),
        PlainField::hidden("pw", "password", "correct horse battery staple"),
    ]
}

/// A sealed login item from a throw-away vault.
pub fn sample_item() -> Item {
    let mut vault = sample_vault();
    let id = vault
        .create_item(ItemKind::Login, ItemClass::Standard, sample_login_fields())
        .expect("an item");
    vault.item(id).expect("the item").clone()
}

// --- devices from older builds (docs/plan/30-pq-status-line.md) ----------------
//
// The post-quantum rollout waits for every device's mark (`crate::pq`). A
// test that needs a device from an older build adds its record here: a
// fresh device key, a name, and the mark at the level that build would
// write. The record is signed by the identity's key like any enrolment, so
// only a holder of the root key can make one; it is the record an approver
// writes for a new device, with the mark that device would add itself.

/// Adds a device record named `name` that marks post-quantum support at
/// `level` (`None`: a build from before the mark). Answers its id.
pub fn add_device_at_level(
    identity: &mut ecosystem_identity::Identity,
    keystore: &dyn ecosystem_identity::keystore::DeviceKeystore,
    name: &str,
    level: Option<u64>,
) -> crate::Result<Id> {
    use ecosystem_identity::Record as _;
    let id = Id::new();
    let mine = identity.device_id();
    let key = SigningKey::generate()?.verifying_key().to_bytes();
    crate::identity::write_identity_store(identity, keystore, |contents, isk| {
        let mut d = contents
            .devices
            .get(&mine)
            .cloned()
            .ok_or_else(|| crate::Error::NotFound(format!("device {mine} in this identity")))?;
        d.device_id = id;
        d.name = name.to_string();
        d.device_pub = key;
        d.attestation = None;
        d.revoked_at = None;
        d.enrolled_at = ecosystem_identity::records::Timestamp::now();
        set_mark(&mut d, level);
        d.sign(isk.inner())?;
        contents.devices.insert(id, d);
        Ok(true)
    })?;
    Ok(id)
}

/// Changes the post-quantum mark on another device's record to `level`,
/// as that device would once updated.
pub fn set_device_level(
    identity: &mut ecosystem_identity::Identity,
    keystore: &dyn ecosystem_identity::keystore::DeviceKeystore,
    device: Id,
    level: Option<u64>,
) -> crate::Result<()> {
    use ecosystem_identity::Record as _;
    crate::identity::write_identity_store(identity, keystore, |contents, isk| {
        let d = contents
            .devices
            .get_mut(&device)
            .ok_or_else(|| crate::Error::NotFound(format!("device {device} in this identity")))?;
        set_mark(d, level);
        d.sign(isk.inner())?;
        Ok(true)
    })?;
    Ok(())
}

fn set_mark(device: &mut ecosystem_identity::records::Device, level: Option<u64>) {
    match level {
        Some(n) => {
            device.extra.insert(
                crate::pq::SUPPORT_FIELD.to_string(),
                ciborium::Value::Integer(n.into()),
            );
        }
        None => {
            device.extra.remove(crate::pq::SUPPORT_FIELD);
        }
    }
}
