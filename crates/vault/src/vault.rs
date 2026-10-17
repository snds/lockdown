//! [`Vault`]: the in-memory model of one vault with its keys, items and
//! manifest chain (02-model §1, §2, §7, §8).
//!
//! Every mutation seals fields under a fresh Version Key, signs the version
//! with the device key, and commits a new manifest head. Purging an item
//! destroys its Item Key and every Version Key (crypto-shredding) and leaves
//! a tombstone so the chain stays valid.
//!
//! A vault round-trips through any [`Store`]: [`Vault::save`] writes the
//! vault record, the items and the heads; [`Vault::load`] reads them back,
//! verifies the whole chain against the devices the caller trusts, refuses
//! a rollback behind the head this device last accepted, unwraps the Vault
//! Key with the persona's App Data Key, and checks every item against the
//! newest signed head before it trusts a byte of it.

use std::collections::BTreeMap;

use ecosystem_identity::SymmetricKey;
use ecosystem_identity::keys::{AppDataKey, CrownJewelKey};
use lockdown_crypto::VerifyingKey;
use lockdown_crypto::aead::{Sealed, open, seal};
use lockdown_crypto::cbor::{from_slice, to_canonical};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::field::{FieldAad, PlainField, SCHEMA_V, SealedField};
use crate::item::{ITEM_V, Item, ItemClass, ItemHeader, ItemKind, ItemState, ItemVersion};
use crate::keys::{
    ItemKey, KeyPlace, KeyPurpose, VaultKey, VersionKey, WRAP_V, WrappedBy, WrappedKey,
    WrapperKind, unwrap_key, wrap_key,
};
use crate::manifest::{MANIFEST_V, ManifestEntry, ManifestHead, Member, verify_chain};
use crate::signer::Signer;
use crate::store::{Store, VAULT_RECORD_V, VaultRecord};
use crate::{Error, Hash32, Id, Result, Timestamp};

/// Who and where a vault belongs to: the ids that go into every piece of
/// associated data.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct VaultContext {
    /// The profile.
    pub profile_id: Id,
    /// The persona that owns the vault (D13).
    pub persona_id: Id,
    /// The vault.
    pub vault_id: Id,
    /// The id of the persona's Lockdown App Data Key, which wraps the Vault Key.
    pub app_data_key_id: Id,
    /// The id of the persona's Crown-Jewel Key, which wraps crown-jewel Item Keys.
    pub crown_jewel_key_id: Id,
}

/// What opens a stored vault on this device: the persona's keys from the
/// identity, this device's signer, and what this device remembers.
pub struct Unlock<'a> {
    /// The persona's Lockdown App Data Key, which wraps the Vault Key.
    pub app_data_key: &'a AppDataKey,
    /// The persona's Crown-Jewel Key, when the persona is unlocked for it.
    pub crown_jewel_key: Option<CrownJewelKey>,
    /// This device's id (the identity's `Device.device_id`).
    pub device_id: Id,
    /// This device's signer: the key behind `Device.device_pub`.
    pub signer: Box<dyn Signer>,
    /// The public key of any other device whose heads and versions may be
    /// trusted, by device id: the identity's `Device` records. This
    /// device's own key is always trusted.
    pub known_devices: &'a dyn Fn(Id) -> Option<VerifyingKey>,
    /// The newest manifest sequence this device accepted for the vault, or
    /// zero when it has none. A store behind it is refused as a rollback.
    pub last_accepted_seq: u64,
}

/// One unlocked vault.
pub struct Vault {
    ctx: VaultContext,
    vault_key: VaultKey,
    vault_key_id: Id,
    crown_jewel_key: Option<CrownJewelKey>,
    device_id: Id,
    signer: Box<dyn Signer>,
    verifying: VerifyingKey,
    items: BTreeMap<Id, Item>,
    heads: Vec<ManifestHead>,
    members: Vec<Member>,
}

impl std::fmt::Debug for Vault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Vault")
            .field("vault_id", &self.ctx.vault_id)
            .field("items", &self.items.len())
            .field("seq", &self.seq())
            .finish_non_exhaustive()
    }
}

const MEMBERS_AAD_LABEL: &[u8] = b"lockdown/vault-members/v1";

impl Vault {
    /// Creates a new vault with a fresh Vault Key and writes its first
    /// manifest head. `crown_jewel_key` is the persona's; without it, no
    /// crown-jewel item can be created or opened here.
    pub fn create(
        ctx: VaultContext,
        device_id: Id,
        signer: Box<dyn Signer>,
        crown_jewel_key: Option<CrownJewelKey>,
    ) -> Result<Self> {
        let verifying = signer.verifying_key()?;
        let mut vault = Self {
            ctx,
            vault_key: VaultKey::from_bytes(lockdown_crypto::rand::random_32()?),
            vault_key_id: Id::new(),
            crown_jewel_key,
            device_id,
            signer,
            verifying,
            items: BTreeMap::new(),
            heads: Vec::new(),
            members: vec![Member {
                persona_id: ctx.persona_id,
                rights: vec!["manage".into()],
                key_env: Id::new(),
            }],
        };
        vault.commit_head()?;
        Ok(vault)
    }

    /// Opens the vault `vault_id` from `store` (02-model §8; 03-architecture
    /// §6 "rollback check"). In order: the chain of heads is verified
    /// against the devices `unlock` trusts and refused if it has fallen
    /// behind `last_accepted_seq` ("Rollback attempt blocked"); the Vault
    /// Key is unwrapped with the App Data Key at the place the record
    /// names; then every item is checked against the newest head (same
    /// set of ids, same latest-version hash, same state) so a blob changed
    /// after the head was signed is refused before any field is opened.
    pub fn load(store: &dyn Store, vault_id: Id, unlock: Unlock<'_>) -> Result<Self> {
        let record = store.get_vault(vault_id)?;
        if record.v > VAULT_RECORD_V {
            return Err(Error::SchemaTooNew {
                found: record.v,
                supported: VAULT_RECORD_V,
            });
        }
        let ctx = record.context;
        if ctx.vault_id != vault_id {
            return Err(Error::ManifestMismatch {
                item_id: vault_id,
                why: format!("the vault record is for vault {}", ctx.vault_id),
            });
        }
        let verifying = unlock.signer.verifying_key()?;
        let own = verifying.clone();
        let device_id = unlock.device_id;
        let known = unlock.known_devices;
        let heads = store.heads(vault_id)?;
        let newest = verify_chain(
            &heads,
            |signer| {
                if signer == device_id {
                    Some(own.clone())
                } else {
                    known(signer)
                }
            },
            unlock.last_accepted_seq,
        )?
        .clone();
        if newest.vault_id != vault_id {
            return Err(Error::ChainBroken {
                seq: newest.seq,
                why: "the heads belong to another vault".into(),
            });
        }

        let vault_key_id = newest.vault_key_ref;
        let place = KeyPlace {
            profile_id: ctx.profile_id,
            persona_id: ctx.persona_id,
            vault_id,
            key_id: vault_key_id,
            purpose: KeyPurpose::Vault,
            wrapped_by: WrappedBy::new(WrapperKind::AppData, ctx.app_data_key_id),
            v: WRAP_V,
        };
        let vault_key: VaultKey = unwrap_key(&record.vault_key, unlock.app_data_key, &place)?;

        let mut items = BTreeMap::new();
        for item_id in store.list_items(vault_id)? {
            let item = store.get_item(vault_id, item_id)?;
            if item.item_id != item_id || item.vault_id != vault_id {
                return Err(Error::ManifestMismatch {
                    item_id,
                    why: "the blob names another item or vault".into(),
                });
            }
            items.insert(item_id, item);
        }
        check_items_against_head(&items, &newest)?;

        let mut vault = Self {
            ctx,
            vault_key,
            vault_key_id,
            crown_jewel_key: unlock.crown_jewel_key,
            device_id,
            signer: unlock.signer,
            verifying,
            items,
            heads,
            members: Vec::new(),
        };
        vault.members = vault.members()?;
        Ok(vault)
    }

    /// Writes the vault to `store`: its record (the Vault Key under
    /// `app_data_key`), every item, and every head the store does not have
    /// yet. Safe to call after every change.
    pub fn save(&self, store: &mut dyn Store, app_data_key: &AppDataKey) -> Result<()> {
        let record = self.record(app_data_key)?;
        let have = match store.latest_head(self.ctx.vault_id) {
            Ok(h) => h.seq,
            Err(crate::store::StoreError::NotFound(_)) => 0,
            Err(e) => return Err(e.into()),
        };
        let new_heads = self
            .heads
            .iter()
            .filter(|h| h.seq > have)
            .cloned()
            .collect();
        store.save_all(record, self.items.values().cloned().collect(), new_heads)?;
        Ok(())
    }

    /// The vault as a store keeps it: the context and the Vault Key under
    /// `app_data_key`.
    pub fn record(&self, app_data_key: &AppDataKey) -> Result<VaultRecord> {
        Ok(VaultRecord {
            v: VAULT_RECORD_V,
            context: self.ctx,
            vault_key: self.wrap_vault_key(app_data_key)?,
        })
    }

    /// The context.
    pub fn context(&self) -> &VaultContext {
        &self.ctx
    }

    /// This device's id.
    pub fn device_id(&self) -> Id {
        self.device_id
    }

    /// The device key's public half, for other devices to verify this
    /// device's heads and versions.
    pub fn verifying_key(&self) -> VerifyingKey {
        self.verifying.clone()
    }

    /// The Vault Key wrapped by the persona's Lockdown App Data Key: what
    /// the identity's key tree hangs the vault from.
    pub fn wrap_vault_key(&self, app_data_key: &AppDataKey) -> Result<WrappedKey> {
        let place = KeyPlace {
            profile_id: self.ctx.profile_id,
            persona_id: self.ctx.persona_id,
            vault_id: self.ctx.vault_id,
            key_id: self.vault_key_id,
            purpose: KeyPurpose::Vault,
            wrapped_by: WrappedBy::new(WrapperKind::AppData, self.ctx.app_data_key_id),
            v: WRAP_V,
        };
        wrap_key(&self.vault_key, app_data_key, &place)
    }

    /// The newest manifest sequence.
    pub fn seq(&self) -> u64 {
        self.heads.last().map(|h| h.seq).unwrap_or(0)
    }

    /// The newest manifest head.
    pub fn head(&self) -> Result<&ManifestHead> {
        self.heads
            .last()
            .ok_or_else(|| Error::NotFound("a manifest head".into()))
    }

    /// Every manifest head, oldest first.
    pub fn heads(&self) -> &[ManifestHead] {
        &self.heads
    }

    /// Every item envelope, in id order.
    pub fn items(&self) -> impl Iterator<Item = &Item> {
        self.items.values()
    }

    /// An item's envelope.
    pub fn item(&self, item_id: Id) -> Result<&Item> {
        self.items
            .get(&item_id)
            .ok_or_else(|| Error::NotFound(format!("item {item_id}")))
    }

    /// Every item id, in id order.
    pub fn item_ids(&self) -> Vec<Id> {
        self.items.keys().copied().collect()
    }

    /// Verifies the whole chain with this device's key and refuses it if
    /// it has fallen below `last_accepted_seq`.
    pub fn verify(&self, last_accepted_seq: u64) -> Result<&ManifestHead> {
        let key = self.verifying_key();
        let device = self.device_id;
        verify_chain(
            &self.heads,
            |signer| {
                if signer == device {
                    Some(key.clone())
                } else {
                    None
                }
            },
            last_accepted_seq,
        )
    }

    /// Creates an item and returns its id.
    pub fn create_item(
        &mut self,
        kind: ItemKind,
        class: ItemClass,
        fields: Vec<PlainField>,
    ) -> Result<Id> {
        let item_id = Id::new();
        let item_key = ItemKey::from_bytes(lockdown_crypto::rand::random_32()?);
        let wrapped_item_key = self.wrap_item_key(item_id, class, &item_key)?;
        let header = ItemHeader {
            item_id,
            vault_id: self.ctx.vault_id,
            kind,
            schema_v: SCHEMA_V,
            class,
            device_only: false,
        };
        let version = self.make_version(&header, &item_key, 1, Hash32::ZERO, &fields)?;
        let item = Item {
            v: ITEM_V,
            item_id,
            vault_id: self.ctx.vault_id,
            rev: 0,
            kind,
            schema_v: SCHEMA_V,
            class,
            device_only: false,
            item_key: Some(wrapped_item_key),
            versions: vec![version],
            state: ItemState::Live,
            trashed_at: None,
        };
        self.items.insert(item_id, item);
        self.commit_head()?;
        Ok(item_id)
    }

    /// Saves a new version of an item and returns its number. History is
    /// never rewritten.
    pub fn update_item(&mut self, item_id: Id, fields: Vec<PlainField>) -> Result<u64> {
        let item = self.item(item_id)?;
        let item_key = self.item_key(item)?;
        let header = item.header();
        let latest = item.latest()?;
        let n = latest.n + 1;
        let parent_hash = latest.hash(&header)?;
        let version = self.make_version(&header, &item_key, n, parent_hash, &fields)?;
        self.items
            .get_mut(&item_id)
            .ok_or_else(|| Error::NotFound(format!("item {item_id}")))?
            .versions
            .push(version);
        self.commit_head()?;
        Ok(n)
    }

    /// Opens the newest version's fields.
    pub fn open_item(&self, item_id: Id) -> Result<Vec<PlainField>> {
        let item = self.item(item_id)?;
        let n = item.latest()?.n;
        self.open_version(item_id, n)
    }

    /// Opens version `n` of an item, verifying its device signature first.
    pub fn open_version(&self, item_id: Id, n: u64) -> Result<Vec<PlainField>> {
        let item = self.item(item_id)?;
        if item.schema_v > SCHEMA_V {
            return Err(Error::SchemaTooNew {
                found: item.schema_v,
                supported: SCHEMA_V,
            });
        }
        let version = item.version(n)?;
        version.verify(&item.header(), &self.verifying_key())?;
        let item_key = self.item_key(item)?;
        let version_key = self.version_key(item, version, &item_key)?;
        version
            .fields
            .iter()
            .map(|f| f.open(&version_key, &self.field_aad(item_id, n, f)))
            .collect()
    }

    /// Soft-deletes an item: it stays readable through the grace period.
    pub fn trash_item(&mut self, item_id: Id) -> Result<()> {
        let item = self
            .items
            .get_mut(&item_id)
            .ok_or_else(|| Error::NotFound(format!("item {item_id}")))?;
        item.state = ItemState::Trashed;
        item.trashed_at = Some(Timestamp::now());
        self.commit_head()
    }

    /// Purges an item: destroys its Item Key, every Version Key and every
    /// Attachment Key, leaves a tombstone with the last version hash, and
    /// commits. The ciphertext may sit in backups forever and stays dead.
    pub fn purge_item(&mut self, item_id: Id) -> Result<()> {
        let item = self
            .items
            .get_mut(&item_id)
            .ok_or_else(|| Error::NotFound(format!("item {item_id}")))?;
        item.item_key = None;
        for v in &mut item.versions {
            v.version_key = None;
            for a in &mut v.attachments {
                a.att_key = None;
            }
        }
        item.state = ItemState::Tombstone;
        self.commit_head()
    }

    /// Destroys one version's key, keeping the rest of the history.
    pub fn shred_version(&mut self, item_id: Id, n: u64) -> Result<()> {
        let item = self
            .items
            .get_mut(&item_id)
            .ok_or_else(|| Error::NotFound(format!("item {item_id}")))?;
        let version = item
            .versions
            .iter_mut()
            .find(|v| v.n == n)
            .ok_or_else(|| Error::NotFound(format!("version {n} of item {item_id}")))?;
        version.version_key = None;
        self.commit_head()
    }

    /// Opens the members list sealed in the newest head.
    pub fn members(&self) -> Result<Vec<Member>> {
        let head = self
            .heads
            .last()
            .ok_or_else(|| Error::NotFound("a manifest head".into()))?;
        let plain = open(&self.vault_key, &self.members_aad(head.seq), &head.members)?;
        Ok(from_slice(&plain)?)
    }

    // --- internals -------------------------------------------------------

    fn members_aad(&self, seq: u64) -> Vec<u8> {
        let mut aad = MEMBERS_AAD_LABEL.to_vec();
        aad.extend_from_slice(self.ctx.vault_id.as_bytes());
        aad.extend_from_slice(&seq.to_be_bytes());
        aad
    }

    fn seal_members(&self, seq: u64) -> Result<Sealed> {
        let plain = Zeroizing::new(to_canonical(&self.members)?);
        Ok(seal(&self.vault_key, &self.members_aad(seq), &plain)?)
    }

    fn field_aad(&self, item_id: Id, version_n: u64, f: &SealedField) -> FieldAad {
        FieldAad {
            profile_id: self.ctx.profile_id,
            persona_id: self.ctx.persona_id,
            vault_id: self.ctx.vault_id,
            item_id,
            version_n,
            field_id: f.field_id.clone(),
            kind: f.kind,
            schema_v: SCHEMA_V,
        }
    }

    fn item_key_place(&self, item_id: Id, class: ItemClass) -> KeyPlace {
        let wrapped_by = match class {
            ItemClass::Standard => WrappedBy::new(WrapperKind::Vault, self.vault_key_id),
            ItemClass::CrownJewel => {
                WrappedBy::new(WrapperKind::CrownJewel, self.ctx.crown_jewel_key_id)
            }
        };
        KeyPlace {
            profile_id: self.ctx.profile_id,
            persona_id: self.ctx.persona_id,
            vault_id: self.ctx.vault_id,
            key_id: item_id,
            purpose: KeyPurpose::Item,
            wrapped_by,
            v: WRAP_V,
        }
    }

    fn version_key_place(&self, item_id: Id, version_key_id: Id) -> KeyPlace {
        KeyPlace {
            profile_id: self.ctx.profile_id,
            persona_id: self.ctx.persona_id,
            vault_id: self.ctx.vault_id,
            key_id: version_key_id,
            purpose: KeyPurpose::Version,
            wrapped_by: WrappedBy::new(WrapperKind::Item, item_id),
            v: WRAP_V,
        }
    }

    fn wrap_item_key(&self, item_id: Id, class: ItemClass, key: &ItemKey) -> Result<WrappedKey> {
        let place = self.item_key_place(item_id, class);
        match class {
            ItemClass::Standard => wrap_key(key, &self.vault_key, &place),
            ItemClass::CrownJewel => {
                let cjk = self
                    .crown_jewel_key
                    .as_ref()
                    .ok_or(Error::NoCrownJewelKey)?;
                wrap_key(key, cjk, &place)
            }
        }
    }

    fn item_key(&self, item: &Item) -> Result<ItemKey> {
        let env = item.item_key.as_ref().ok_or(Error::Shredded {
            item_id: item.item_id,
            version_n: 0,
        })?;
        let place = self.item_key_place(item.item_id, item.class);
        match item.class {
            ItemClass::Standard => unwrap_key(env, &self.vault_key, &place),
            ItemClass::CrownJewel => {
                let cjk = self
                    .crown_jewel_key
                    .as_ref()
                    .ok_or(Error::NoCrownJewelKey)?;
                unwrap_key(env, cjk, &place)
            }
        }
    }

    fn version_key(
        &self,
        item: &Item,
        version: &ItemVersion,
        item_key: &ItemKey,
    ) -> Result<VersionKey> {
        let env = version.version_key.as_ref().ok_or(Error::Shredded {
            item_id: item.item_id,
            version_n: version.n,
        })?;
        unwrap_key(
            env,
            item_key,
            &self.version_key_place(item.item_id, env.key_id),
        )
    }

    fn make_version(
        &self,
        header: &ItemHeader,
        item_key: &ItemKey,
        n: u64,
        parent_hash: Hash32,
        fields: &[PlainField],
    ) -> Result<ItemVersion> {
        let item_id = header.item_id;
        let version_key = VersionKey::from_bytes(lockdown_crypto::rand::random_32()?);
        let version_key_id = Id::new();
        let wrapped = wrap_key(
            &version_key,
            item_key,
            &self.version_key_place(item_id, version_key_id),
        )?;
        let mut sealed = Vec::with_capacity(fields.len());
        for f in fields {
            let aad = FieldAad {
                profile_id: self.ctx.profile_id,
                persona_id: self.ctx.persona_id,
                vault_id: self.ctx.vault_id,
                item_id,
                version_n: n,
                field_id: f.field_id.clone(),
                kind: f.kind,
                schema_v: SCHEMA_V,
            };
            sealed.push(SealedField::seal(&version_key, &aad, f)?);
        }
        let mut version = ItemVersion {
            n,
            version_key: Some(wrapped),
            parent_hash,
            edited_by: self.device_id,
            edited_at: Timestamp::now(),
            fields: sealed,
            attachments: Vec::new(),
            sig: Vec::new(),
        };
        version.sign(header, self.signer.as_ref())?;
        Ok(version)
    }

    fn commit_head(&mut self) -> Result<()> {
        let seq = self.seq() + 1;
        let prev_hash = match self.heads.last() {
            Some(h) => h.hash()?,
            None => Hash32::ZERO,
        };
        let mut entries = Vec::with_capacity(self.items.len());
        for item in self.items.values() {
            entries.push(ManifestEntry {
                item_id: item.item_id,
                rev: item.rev,
                version_hash: item.latest_hash()?,
                state: item.state,
                shredded: item.is_shredded(),
            });
        }
        let mut head = ManifestHead {
            v: MANIFEST_V,
            vault_id: self.ctx.vault_id,
            seq,
            prev_hash,
            vault_key_ref: self.vault_key_id,
            members: self.seal_members(seq)?,
            entries,
            signer: self.device_id,
            signed_at: Timestamp::now(),
            sig: Vec::new(),
            checkpoint: None,
        };
        head.sign(self.signer.as_ref())?;
        self.heads.push(head);
        Ok(())
    }
}

/// The items must be exactly what the newest signed head lists, each with
/// the latest-version hash and state the head recorded.
fn check_items_against_head(items: &BTreeMap<Id, Item>, head: &ManifestHead) -> Result<()> {
    for entry in &head.entries {
        let item = items
            .get(&entry.item_id)
            .ok_or_else(|| Error::ManifestMismatch {
                item_id: entry.item_id,
                why: "the manifest lists it but the store doesn't have it".into(),
            })?;
        let mismatch = |why: &str| Error::ManifestMismatch {
            item_id: entry.item_id,
            why: why.into(),
        };
        if item.v > ITEM_V {
            return Err(Error::SchemaTooNew {
                found: item.v,
                supported: ITEM_V,
            });
        }
        if item.latest_hash()? != entry.version_hash {
            return Err(mismatch(
                "its latest version isn't the one the manifest signed",
            ));
        }
        if item.rev != entry.rev {
            return Err(mismatch("its revision isn't what the manifest says"));
        }
        if item.state != entry.state {
            return Err(mismatch("its state isn't what the manifest says"));
        }
        if item.is_shredded() != entry.shredded {
            return Err(mismatch("its keys aren't in the state the manifest says"));
        }
    }
    if let Some(extra) = items
        .keys()
        .find(|id| !head.entries.iter().any(|e| e.item_id == **id))
    {
        return Err(Error::ManifestMismatch {
            item_id: *extra,
            why: "the store has it but the manifest doesn't list it".into(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{sample_context, sample_vault};

    fn login_fields(pw: &str) -> Vec<PlainField> {
        vec![
            PlainField::text("title", "title", "Example"),
            PlainField::url("url.0", "https://accounts.example.com", "exact_host"),
            PlainField::text("user", "username", "me@example.com"),
            PlainField::hidden("pw", "password", pw),
        ]
    }

    #[test]
    fn seal_open_and_history() {
        let mut vault = sample_vault();
        let id = vault
            .create_item(ItemKind::Login, ItemClass::Standard, login_fields("first"))
            .unwrap();
        assert_eq!(vault.open_item(id).unwrap(), login_fields("first"));
        let n = vault.update_item(id, login_fields("second")).unwrap();
        assert_eq!(n, 2);
        assert_eq!(vault.open_item(id).unwrap(), login_fields("second"));
        assert_eq!(vault.open_version(id, 1).unwrap(), login_fields("first"));
        let item = vault.item(id).unwrap();
        assert_eq!(
            item.versions[1].parent_hash,
            item.versions[0].hash(&item.header()).unwrap()
        );
        // Heads: create (1), item (2), update (3).
        assert_eq!(vault.seq(), 3);
        vault.verify(3).unwrap();
        assert_eq!(
            vault.members().unwrap()[0].persona_id,
            vault.context().persona_id
        );
    }

    #[test]
    fn a_field_moved_between_items_or_versions_fails() {
        let mut vault = sample_vault();
        let a = vault
            .create_item(ItemKind::Login, ItemClass::Standard, login_fields("a"))
            .unwrap();
        let b = vault
            .create_item(ItemKind::Login, ItemClass::Standard, login_fields("b"))
            .unwrap();
        // Move item a's sealed password into item b, same field id and kind,
        // and re-sign b's version with the device key so only the AAD differs.
        let stolen = vault.items[&a].versions[0].fields[3].clone();
        let signer_version = {
            let item_b = vault.items.get_mut(&b).unwrap();
            item_b.versions[0].fields[3] = stolen;
            item_b.versions[0].clone()
        };
        let mut resigned = signer_version;
        let header_b = vault.items[&b].header();
        resigned.sign(&header_b, vault.signer.as_ref()).unwrap();
        vault.items.get_mut(&b).unwrap().versions[0] = resigned;
        assert!(
            matches!(vault.open_item(b), Err(Error::FieldTampered { field_id }) if field_id == "pw")
        );
        // Same item, a sealed field copied from version 1 into version 2.
        vault.update_item(a, login_fields("a2")).unwrap();
        let old = vault.items[&a].versions[0].fields[3].clone();
        let mut v2 = vault.items[&a].versions[1].clone();
        v2.fields[3] = old;
        let header_a = vault.items[&a].header();
        v2.sign(&header_a, vault.signer.as_ref()).unwrap();
        vault.items.get_mut(&a).unwrap().versions[1] = v2;
        assert!(matches!(
            vault.open_version(a, 2),
            Err(Error::FieldTampered { .. })
        ));
        // An unsigned edit is caught before any field is opened.
        vault.items.get_mut(&a).unwrap().versions[0].edited_at = Timestamp(1);
        assert!(matches!(
            vault.open_version(a, 1),
            Err(Error::Crypto(lockdown_crypto::Error::BadSignature(_)))
        ));
        // The header is under the signature too: relabel the item and the
        // untouched version no longer verifies.
        vault.items.get_mut(&b).unwrap().kind = ItemKind::Note;
        assert!(matches!(
            vault.open_item(b),
            Err(Error::Crypto(lockdown_crypto::Error::BadSignature(_)))
        ));
    }

    #[test]
    fn manifest_tamper_and_rollback_are_detected() {
        let mut vault = sample_vault();
        let id = vault
            .create_item(
                ItemKind::Note,
                ItemClass::Standard,
                vec![PlainField::text("body", "note", "hi")],
            )
            .unwrap();
        vault
            .update_item(id, vec![PlainField::text("body", "note", "edited")])
            .unwrap();
        vault.verify(0).unwrap();

        // An entry edited after signing.
        let mut forged = vault.heads.clone();
        forged[2].entries[0].rev = 99;
        let key = vault.verifying_key();
        let err = verify_chain(&forged, |_| Some(key.clone()), 0).unwrap_err();
        assert!(matches!(err, Error::ChainBroken { seq: 3, .. }), "{err}");

        // A head removed from the middle: prev_hash no longer matches.
        let mut spliced = vault.heads.clone();
        spliced.remove(1);
        let err = verify_chain(&spliced, |_| Some(key.clone()), 0).unwrap_err();
        assert!(matches!(err, Error::ChainBroken { .. }), "{err}");

        // The newest head withheld: a rollback, said loudly.
        let truncated = &vault.heads[..2];
        let err = verify_chain(truncated, |_| Some(key.clone()), 3).unwrap_err();
        assert!(
            err.to_string().starts_with("Rollback attempt blocked"),
            "{err}"
        );

        // A head signed by an unknown device.
        let err = verify_chain(&vault.heads, |_| None, 0).unwrap_err();
        assert!(matches!(err, Error::ChainBroken { seq: 1, .. }));
    }

    #[test]
    fn purge_shreds_every_version() {
        let mut vault = sample_vault();
        let id = vault
            .create_item(ItemKind::Login, ItemClass::Standard, login_fields("one"))
            .unwrap();
        vault.update_item(id, login_fields("two")).unwrap();
        // Keep a copy of the ciphertext, as a backup would.
        let backup = vault.item(id).unwrap().clone();
        vault.purge_item(id).unwrap();
        let after = vault.open_version(id, 1);
        assert!(
            matches!(after, Err(Error::Shredded { version_n: 0, .. })),
            "{after:?}"
        );
        assert!(matches!(vault.open_item(id), Err(Error::Shredded { .. })));
        let head = vault.heads.last().unwrap();
        assert_eq!(head.entries[0].state, ItemState::Tombstone);
        assert!(head.entries[0].shredded);
        assert_eq!(head.entries[0].version_hash, backup.latest_hash().unwrap());
        // Putting the backup's ciphertext back does not help: the keys are gone
        // from the item, and the version keys were wrapped by the item key.
        vault.items.insert(
            id,
            Item {
                item_key: None,
                ..backup
            },
        );
        assert!(matches!(
            vault.open_version(id, 1),
            Err(Error::Shredded { .. })
        ));
        // Shredding one version keeps the others.
        let id2 = vault
            .create_item(ItemKind::Login, ItemClass::Standard, login_fields("x"))
            .unwrap();
        vault.update_item(id2, login_fields("y")).unwrap();
        vault.shred_version(id2, 1).unwrap();
        assert!(matches!(
            vault.open_version(id2, 1),
            Err(Error::Shredded { version_n: 1, .. })
        ));
        assert_eq!(vault.open_version(id2, 2).unwrap(), login_fields("y"));
        vault.verify(0).unwrap();
    }

    #[test]
    fn crown_jewel_items_need_the_crown_jewel_key() {
        let ctx = sample_context();
        let device = Id::new();
        let mut with_key = Vault::create(
            ctx,
            device,
            Box::new(lockdown_crypto::SigningKey::generate().unwrap()),
            Some(CrownJewelKey::generate().unwrap()),
        )
        .unwrap();
        let seed = vec![PlainField::hidden(
            "seed",
            "recovery phrase",
            "abandon ability able",
        )];
        let id = with_key
            .create_item(ItemKind::CryptoWallet, ItemClass::CrownJewel, seed.clone())
            .unwrap();
        assert_eq!(with_key.open_item(id).unwrap(), seed);
        assert_eq!(
            with_key
                .item(id)
                .unwrap()
                .item_key
                .as_ref()
                .unwrap()
                .wrapped_by
                .kind,
            WrapperKind::CrownJewel
        );
        // Drop the Crown-Jewel Key (a locked persona): the item is sealed off.
        with_key.crown_jewel_key = None;
        assert!(matches!(
            with_key.open_item(id),
            Err(Error::NoCrownJewelKey)
        ));
        assert!(matches!(
            with_key.create_item(ItemKind::CryptoWallet, ItemClass::CrownJewel, seed),
            Err(Error::NoCrownJewelKey)
        ));
    }

    #[test]
    fn the_vault_key_wraps_under_the_app_data_key() {
        let vault = sample_vault();
        let adk = AppDataKey::generate().unwrap();
        let env = vault.wrap_vault_key(&adk).unwrap();
        assert_eq!(env.purpose, KeyPurpose::Vault);
        assert_eq!(env.wrapped_by.kind, WrapperKind::AppData);
        let place = KeyPlace {
            profile_id: vault.ctx.profile_id,
            persona_id: vault.ctx.persona_id,
            vault_id: vault.ctx.vault_id,
            key_id: vault.vault_key_id,
            purpose: KeyPurpose::Vault,
            wrapped_by: WrappedBy::new(WrapperKind::AppData, vault.ctx.app_data_key_id),
            v: WRAP_V,
        };
        let back: VaultKey = unwrap_key(&env, &adk, &place).unwrap();
        assert_eq!(back.expose(), vault.vault_key.expose());
    }
}
