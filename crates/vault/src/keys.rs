//! The vault's keys and the wrap that binds a key to its place (02-model §2).
//!
//! The envelope here mirrors the identity crate's `KeyEnvelope` field for
//! field (version, key id, who wraps it, cipher, the associated data the
//! reader recomputes, nonce, ciphertext, purpose, time), with the same
//! discipline: the associated data is canonical CBOR of the place, the
//! reader rebuilds it from where it found the envelope, and an envelope
//! that names another place is refused by name before any decryption.
//!
//! It is a separate type because the identity crate's `Purpose` and
//! `KeyRefKind` stop at the App Data Key, and 02-model §13 question 2
//! (`purpose: "vault"` on `KeyEnvelope`?) is still open. The proposal to
//! the crate owner is additive: `Purpose::{Vault, Item, Version,
//! Attachment}` and `KeyRefKind::{Vault, Item, CrownJewel}`. When it lands,
//! this file becomes re-exports.

use ecosystem_identity::SymmetricKey;
use lockdown_crypto::aead::{Sealed, open, seal};
use lockdown_crypto::cbor::to_canonical;
use lockdown_crypto::key32;
use serde::{Deserialize, Serialize};

use crate::{Error, Id, Result, Timestamp};

/// The envelope format version this crate writes.
pub const WRAP_V: u16 = 1;

key32!(
    /// The Vault Key: wraps every Item Key in the vault (except crown-jewel
    /// items, which the Crown-Jewel Key wraps).
    VaultKey
);
key32!(
    /// An Item Key: one per item, so sharing or deleting one item never
    /// re-encrypts the vault.
    ItemKey
);
key32!(
    /// A Version Key: one per saved version. Destroying it shreds the version.
    VersionKey
);
key32!(
    /// An Attachment Key: one per file, so a large file is never fully
    /// decrypted in memory.
    AttachmentKey
);

/// What a wrapped key is for. Part of the place, so an envelope cannot be
/// passed off as another purpose.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum KeyPurpose {
    /// A Vault Key.
    Vault,
    /// An Item Key.
    Item,
    /// A Version Key.
    Version,
    /// An Attachment Key.
    Attachment,
}

/// Which kind of key can open an envelope.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WrapperKind {
    /// The persona's Lockdown App Data Key (identity crate). `id` is that key's id.
    AppData,
    /// The persona's Crown-Jewel Key (identity crate). `id` is that key's id.
    CrownJewel,
    /// A Vault Key. `id` is the vault key's id.
    Vault,
    /// An Item Key. `id` is the item.
    Item,
    /// Another persona's receiving key (a share). `id` is that persona.
    Share,
}

/// Who can open an envelope.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub struct WrappedBy {
    /// Which kind of key.
    pub kind: WrapperKind,
    /// Which one.
    pub id: Id,
}

impl WrappedBy {
    /// A reference of this kind to this id.
    pub fn new(kind: WrapperKind, id: Id) -> Self {
        Self { kind, id }
    }
}

/// Where a wrapped key belongs. Canonical CBOR of this is the AEAD's
/// associated data.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct KeyPlace {
    /// The profile.
    pub profile_id: Id,
    /// The persona.
    pub persona_id: Id,
    /// The vault.
    pub vault_id: Id,
    /// The key being wrapped.
    pub key_id: Id,
    /// What it is for.
    pub purpose: KeyPurpose,
    /// Who wraps it.
    pub wrapped_by: WrappedBy,
    /// The envelope format version.
    pub v: u16,
}

impl KeyPlace {
    /// The bytes handed to the AEAD.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        Ok(to_canonical(self)?)
    }
}

impl std::fmt::Display for KeyPlace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:?} key {} of vault {} (persona {}), wrapped by {:?} {}",
            self.purpose,
            self.key_id,
            self.vault_id,
            self.persona_id,
            self.wrapped_by.kind,
            self.wrapped_by.id
        )
    }
}

/// One wrapped key.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct WrappedKey {
    /// Envelope format version.
    pub v: u16,
    /// The key being wrapped.
    pub key_id: Id,
    /// Who can open it.
    pub wrapped_by: WrappedBy,
    /// The cipher; always `xchacha20poly1305` today.
    pub alg: String,
    /// Canonical CBOR of [`KeyPlace`]. Readers recompute it and refuse a
    /// disagreement.
    #[serde(with = "serde_bytes")]
    pub aad: Vec<u8>,
    /// The sealed key.
    pub sealed: Sealed,
    /// What the key is for.
    pub purpose: KeyPurpose,
    /// When it was wrapped.
    pub created_at: Timestamp,
}

/// Wraps `secret` under `with` at `place`.
pub fn wrap_key<K: SymmetricKey, W: SymmetricKey>(
    secret: &K,
    with: &W,
    place: &KeyPlace,
) -> Result<WrappedKey> {
    if place.v != WRAP_V {
        return Err(Error::Invalid(
            "the place must carry the current envelope version".into(),
        ));
    }
    let aad = place.to_bytes()?;
    let sealed = seal(with, &aad, secret.expose())?;
    Ok(WrappedKey {
        v: WRAP_V,
        key_id: place.key_id,
        wrapped_by: place.wrapped_by,
        alg: "xchacha20poly1305".into(),
        aad,
        sealed,
        purpose: place.purpose,
        created_at: Timestamp::now(),
    })
}

/// Opens an envelope that was made for `expected`. One whose place differs
/// is refused by name, before any decryption.
pub fn unwrap_key<K: SymmetricKey, W: SymmetricKey>(
    envelope: &WrappedKey,
    with: &W,
    expected: &KeyPlace,
) -> Result<K> {
    let expected_bytes = expected.to_bytes()?;
    if envelope.aad != expected_bytes
        || envelope.key_id != expected.key_id
        || envelope.purpose != expected.purpose
        || envelope.wrapped_by != expected.wrapped_by
    {
        let found: Option<KeyPlace> = lockdown_crypto::cbor::from_slice(&envelope.aad).ok();
        return Err(Error::KeyMoved(
            found
                .map(|p| p.to_string())
                .unwrap_or_else(|| "an unreadable place".into()),
        ));
    }
    let plain = open(with, &expected_bytes, &envelope.sealed)?;
    let bytes: [u8; 32] = plain
        .as_slice()
        .try_into()
        .map_err(|_| Error::Crypto(lockdown_crypto::Error::Tampered))?;
    Ok(K::from_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(vault_id: Id, key_id: Id, wrapped_by: WrappedBy) -> KeyPlace {
        KeyPlace {
            profile_id: Id::new(),
            persona_id: Id::new(),
            vault_id,
            key_id,
            purpose: KeyPurpose::Item,
            wrapped_by,
            v: WRAP_V,
        }
    }

    #[test]
    fn a_wrap_opens_only_at_its_place() {
        let vault_key = VaultKey::from_bytes(lockdown_crypto::rand::random_32().unwrap());
        let vault_key_id = Id::new();
        let item_key = ItemKey::from_bytes(lockdown_crypto::rand::random_32().unwrap());
        let p = place(
            Id::new(),
            Id::new(),
            WrappedBy::new(WrapperKind::Vault, vault_key_id),
        );
        let env = wrap_key(&item_key, &vault_key, &p).unwrap();
        let back: ItemKey = unwrap_key(&env, &vault_key, &p).unwrap();
        assert_eq!(back.expose(), item_key.expose());

        // Another vault: refused by name.
        let mut moved = p.clone();
        moved.vault_id = Id::new();
        assert!(matches!(
            unwrap_key::<ItemKey, _>(&env, &vault_key, &moved),
            Err(Error::KeyMoved(_))
        ));
        // Another purpose.
        let mut repurposed = p.clone();
        repurposed.purpose = KeyPurpose::Version;
        assert!(matches!(
            unwrap_key::<ItemKey, _>(&env, &vault_key, &repurposed),
            Err(Error::KeyMoved(_))
        ));
        // The aad rewritten to match the move: the tag fails.
        let mut forged = env.clone();
        forged.aad = moved.to_bytes().unwrap();
        assert!(matches!(
            unwrap_key::<ItemKey, _>(&forged, &vault_key, &moved),
            Err(Error::Crypto(lockdown_crypto::Error::Tampered))
        ));
        // Another key.
        let other = VaultKey::from_bytes(lockdown_crypto::rand::random_32().unwrap());
        assert!(matches!(
            unwrap_key::<ItemKey, _>(&env, &other, &p),
            Err(Error::Crypto(lockdown_crypto::Error::Tampered))
        ));
    }
}
