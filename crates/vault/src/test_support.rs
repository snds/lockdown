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
