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

use std::collections::{BTreeMap, BTreeSet};

use ecosystem_identity::SymmetricKey;
use ecosystem_identity::keys::{AppDataKey, CrownJewelKey};
use lockdown_crypto::VerifyingKey;
use lockdown_crypto::aead::{Sealed, open, seal};
use lockdown_crypto::cbor::{from_slice, to_canonical};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::attachment::{
    AttachmentMeta, AttachmentPlace, open_attachment, open_meta, seal_attachment,
};
use crate::compose::{FieldChange, compose_edit, compose_new};
use crate::field::{FieldAad, PlainField, SCHEMA_V, SealedField};
use crate::item::{
    AttachmentRef, ITEM_V, Item, ItemClass, ItemHeader, ItemKind, ItemState, ItemVersion,
};
use crate::keys::{
    AttachmentKey, ItemKey, KeyPlace, KeyPurpose, VaultKey, VersionKey, WRAP_V, WrappedBy,
    WrappedKey, WrapperKind, unwrap_key, wrap_key,
};
use crate::manifest::{
    MANIFEST_V, ManifestEntry, ManifestHead, Member, check_revocations, verify_chain,
};
use crate::signer::Signer;
use crate::store::{Store, VAULT_RECORD_V, VaultRecord};
use crate::transparency::SthRef;
use crate::{Error, Hash32, Id, Result, Timestamp};

mod restore;
pub use restore::{EXPORT_OPT_IN, ImportSpec, NewAttachment, export_opted_in, is_reserved_field};
mod carry;
pub use carry::{CarriedAttachment, CarriedItem, CarriedVersion};
mod share;
pub use share::member_record;

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
    /// For a revoked device, the newest head `seq` it may have signed in
    /// this vault (from the synced identity manifest, D73); `None` for a
    /// device in good standing. A head by a revoked device after that
    /// point is refused; what it signed before still verifies.
    pub revoked_after: &'a dyn Fn(Id) -> Option<u64>,
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
    /// The public key of every device whose heads and versions this vault
    /// accepts, by device id; this device's own key is always in it.
    trusted: BTreeMap<Id, VerifyingKey>,
    /// Revoked devices and the newest head each may have signed here.
    revoked: BTreeMap<Id, u64>,
    items: BTreeMap<Id, Item>,
    heads: Vec<ManifestHead>,
    members: Vec<Member>,
    /// Attachment blobs sealed here and not yet written to a store.
    pending_blobs: Vec<(Hash32, Vec<u8>)>,
    /// The key-transparency tree head cited in every head committed from
    /// now on; set by sync once it has verified one.
    cited_sth: Option<SthRef>,
    /// Earlier Vault Key generations of a shared vault, kept while an item
    /// key may still be wrapped under one (`share`).
    old_vault_keys: Vec<(Id, VaultKey)>,
}

/// A file sealed by [`Vault::seal_stream`], not yet on its item.
#[derive(Clone, Debug)]
pub struct PreparedAttachment {
    item_id: Id,
    reference: AttachmentRef,
}

impl PreparedAttachment {
    /// The blob's name, to keep it under.
    pub fn blob(&self) -> Hash32 {
        self.reference.blob
    }

    /// The blob's length.
    pub fn blob_size(&self) -> u64 {
        self.reference.blob_size
    }
}

/// One attachment of an item, as listed: its sealed facts opened for the
/// caller, and where its blob is.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AttachmentInfo {
    /// The attachment.
    pub att_id: Id,
    /// Its name, type and size; `None` once its key was shredded.
    pub meta: Option<AttachmentMeta>,
    /// How many chunks.
    pub chunks: u32,
    /// The blob's name.
    pub blob: Hash32,
    /// The blob's length.
    pub blob_size: u64,
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
        let trusted = BTreeMap::from([(device_id, verifying.clone())]);
        let mut vault = Self {
            ctx,
            vault_key: VaultKey::from_bytes(lockdown_crypto::rand::random_32()?),
            vault_key_id: Id::new(),
            crown_jewel_key,
            device_id,
            signer,
            verifying,
            trusted,
            revoked: BTreeMap::new(),
            items: BTreeMap::new(),
            heads: Vec::new(),
            members: vec![Member {
                persona_id: ctx.persona_id,
                rights: vec!["manage".into()],
                key_env: Id::new(),
            }],
            pending_blobs: Vec::new(),
            cited_sth: None,
            old_vault_keys: Vec::new(),
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
        let revoked_after = unlock.revoked_after;
        check_revocations(&heads, revoked_after)?;
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

        // Every device that signed a head or a version must be one the
        // caller trusts; their keys are kept so versions open later.
        let mut trusted = BTreeMap::from([(device_id, own.clone())]);
        let mut revoked = BTreeMap::new();
        let signers = heads.iter().map(|h| h.signer).chain(
            items
                .values()
                .flat_map(|i| i.versions.iter().map(|v| v.edited_by)),
        );
        for signer in signers {
            if let Some(after) = revoked_after(signer) {
                revoked.insert(signer, after);
            }
            if trusted.contains_key(&signer) {
                continue;
            }
            let key = known(signer).ok_or(Error::UnknownDevice(signer))?;
            trusted.insert(signer, key);
        }

        let mut vault = Self {
            ctx,
            vault_key,
            vault_key_id,
            crown_jewel_key: unlock.crown_jewel_key,
            device_id,
            signer: unlock.signer,
            verifying,
            trusted,
            revoked,
            items,
            heads,
            members: Vec::new(),
            pending_blobs: Vec::new(),
            cited_sth: None,
            old_vault_keys: Vec::new(),
        };
        vault.members = vault.members()?;
        Ok(vault)
    }

    /// Writes the vault to `store`: its record (the Vault Key under
    /// `app_data_key`), every item, every head the store does not have
    /// yet, and every attachment blob sealed since the last save. When the
    /// store's chain parts from this vault's (sync replaced heads this
    /// device had committed offline), the store's heads after the common
    /// prefix are removed first. Blobs whose Attachment Key is gone from
    /// every version (a purge) are removed from the store: their
    /// ciphertext is dead anyway. Safe to call after every change.
    pub fn save(&mut self, store: &mut dyn Store, app_data_key: &AppDataKey) -> Result<()> {
        let record = self.record(app_data_key)?;
        // Blobs first, so a crash between the two leaves an unreferenced
        // blob (collected on the next save), never a listed blob that is
        // missing.
        for (hash, bytes) in self.pending_blobs.drain(..) {
            store.put_blob(hash, &bytes)?;
        }
        // The blob table serves every vault in the store. Alone in it, this
        // vault removes every blob it holds no key for (a purge's, or one a
        // crash left unreferenced); beside other vaults it removes only the
        // blobs its own shredded attachments name, never another vault's.
        let live = self.live_blobs();
        let alone = store
            .list_vaults()?
            .iter()
            .all(|id| *id == self.ctx.vault_id);
        let dead: BTreeSet<Hash32> = self
            .items
            .values()
            .flat_map(|i| i.versions.iter())
            .flat_map(|v| v.attachments.iter())
            .filter(|a| a.is_shredded())
            .map(|a| a.blob)
            .collect();
        for hash in store.list_blobs()? {
            if !live.contains(&hash) && (alone || dead.contains(&hash)) {
                store.remove_blob(hash)?;
            }
        }
        let stored = store.heads(self.ctx.vault_id)?;
        let mut common = 0;
        for (theirs, ours) in stored.iter().zip(self.heads.iter()) {
            if theirs.seq == ours.seq && theirs.hash()? == ours.hash()? {
                common = ours.seq;
            } else {
                break;
            }
        }
        if stored.last().is_some_and(|h| h.seq > common) {
            store.remove_heads_after(self.ctx.vault_id, common)?;
        }
        let new_heads = self
            .heads
            .iter()
            .filter(|h| h.seq > common)
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
        check_revocations(&self.heads, |id| self.revoked_after(id))?;
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
        let version =
            self.make_version(&header, &item_key, 1, Hash32::ZERO, &fields, Vec::new())?;
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
        let attachments = latest.attachments.clone();
        let version =
            self.make_version(&header, &item_key, n, parent_hash, &fields, attachments)?;
        self.items
            .get_mut(&item_id)
            .ok_or_else(|| Error::NotFound(format!("item {item_id}")))?
            .versions
            .push(version);
        self.commit_head()?;
        Ok(n)
    }

    /// Makes an item of `kind` from what a shell sent, checked against the
    /// kind's template ([`crate::compose::compose_new`]): the class comes
    /// from the template's crown-jewel default or the person's opt-in.
    pub fn add_item(
        &mut self,
        kind: ItemKind,
        title: &str,
        crown_jewel: bool,
        changes: Vec<FieldChange>,
    ) -> Result<Id> {
        let (class, fields) = compose_new(kind, title, crown_jewel, changes)?;
        self.create_item(kind, class, fields)
    }

    /// [`Self::add_item`] from one template, a kind's own or a variant (D231).
    pub fn add_item_from(
        &mut self,
        template: &crate::Template,
        title: &str,
        crown_jewel: bool,
        changes: Vec<FieldChange>,
    ) -> Result<Id> {
        let (class, fields) =
            crate::compose::compose_new_from(template, title, crown_jewel, changes)?;
        self.create_item(template.kind, class, fields)
    }

    /// Makes a Custom item from the person's template held in this vault
    /// as the item `template_id` (D232).
    pub fn add_item_from_user(
        &mut self,
        template_id: Id,
        title: &str,
        crown_jewel: bool,
        changes: Vec<FieldChange>,
    ) -> Result<Id> {
        let def = self.user_template(template_id)?;
        let (class, fields) =
            crate::compose::compose_new_user(&def, template_id, title, crown_jewel, changes)?;
        self.create_item(ItemKind::Custom, class, fields)
    }

    /// The person's template held as the live item `template_id`.
    pub fn user_template(&self, template_id: Id) -> Result<crate::UserTemplate> {
        let item = self.item(template_id)?;
        if item.kind != ItemKind::Template || item.state != ItemState::Live {
            return Err(Error::NotFound(format!("template {template_id}")));
        }
        crate::UserTemplate::from_fields(&self.open_item(template_id)?)
    }

    /// Every live template the person made in this vault, by name.
    pub fn user_templates(&self) -> Result<Vec<(Id, crate::UserTemplate)>> {
        let mut out = Vec::new();
        for item in self
            .items()
            .filter(|i| i.kind == ItemKind::Template && i.state == ItemState::Live)
        {
            // One that doesn't read (a newer build's) is left out, not fatal.
            if let Ok(def) = crate::UserTemplate::from_fields(&self.open_item(item.item_id)?) {
                out.push((item.item_id, def));
            }
        }
        out.sort_by(|a, b| {
            a.1.name
                .to_lowercase()
                .cmp(&b.1.name.to_lowercase())
                .then(a.0.cmp(&b.0))
        });
        Ok(out)
    }

    /// Saves a person's template: a new template item, or with `id` a new
    /// version of that one. Items already made from it keep their fields.
    pub fn save_user_template(&mut self, id: Option<Id>, def: &crate::UserTemplate) -> Result<Id> {
        let fields = def.to_fields()?;
        match id {
            None => self.create_item(ItemKind::Template, ItemClass::Standard, fields),
            Some(id) => {
                self.user_template(id)?;
                self.update_item(id, fields)?;
                Ok(id)
            }
        }
    }

    /// Saves a new version of a live item from an edit that names every
    /// field ([`crate::compose::compose_edit`]); `title: None` keeps it.
    /// Returns the version number.
    pub fn edit_item(
        &mut self,
        item_id: Id,
        title: Option<&str>,
        changes: Vec<FieldChange>,
    ) -> Result<u64> {
        let item = self.item(item_id)?;
        if !matches!(item.state, ItemState::Live | ItemState::Archived) {
            return Err(Error::Invalid(
                "this item is in the trash; restore it before changing it".into(),
            ));
        }
        let kind = item.kind;
        let existing = self.open_item(item_id)?;
        let fields = compose_edit(kind, &existing, title, changes)?;
        self.update_item(item_id, fields)
    }

    // --- attachments (02-model §6) ------------------------------------------

    /// Attaches a file to an item: seals it in chunks under a fresh
    /// Attachment Key wrapped by the Item Key, and saves a new version of
    /// the item whose signed attachment list names the blob. The blob is
    /// written to the store at the next [`Self::save`]. Returns the
    /// attachment's id and the blob's name.
    pub fn attach(
        &mut self,
        item_id: Id,
        name: &str,
        mime: &str,
        bytes: &[u8],
    ) -> Result<(Id, Hash32)> {
        let item = self.item(item_id)?;
        let item_key = self.item_key(item)?;
        let header = item.header();
        let latest = item.latest()?;
        let att_id = Id::new();
        let att_key = AttachmentKey::from_bytes(lockdown_crypto::rand::random_32()?);
        let wrapped = wrap_key(
            &att_key,
            &item_key,
            &self.attachment_key_place(item_id, att_id),
        )?;
        let meta = AttachmentMeta {
            name: name.to_string(),
            mime: mime.to_string(),
            size: bytes.len() as u64,
        };
        let sealed = seal_attachment(
            &att_key,
            &self.attachment_place(item_id, att_id),
            &meta,
            bytes,
        )?;
        let mut attachments = latest.attachments.clone();
        attachments.push(AttachmentRef {
            att_id,
            att_key: Some(wrapped),
            chunks: sealed.chunks,
            blob: sealed.blob_hash,
            blob_size: sealed.blob.len() as u64,
            meta: sealed.meta,
        });
        let fields = self.open_envelope(item, latest.n)?;
        let n = latest.n + 1;
        let parent_hash = latest.hash(&header)?;
        let version =
            self.make_version(&header, &item_key, n, parent_hash, &fields, attachments)?;
        self.items
            .get_mut(&item_id)
            .ok_or_else(|| Error::NotFound(format!("item {item_id}")))?
            .versions
            .push(version);
        self.pending_blobs.push((sealed.blob_hash, sealed.blob));
        self.commit_head()?;
        Ok((att_id, sealed.blob_hash))
    }

    /// [`Self::attach`] for a file read from `reader` (`size` bytes),
    /// sealed chunk by chunk into `out` (a store's [`crate::BlobSink`]),
    /// so the file is never whole in memory (D233). The item is not
    /// changed yet: the caller keeps the blob in its store under the name
    /// this returns, then hands the result to [`Self::add_prepared`], so a
    /// version never names a blob the store failed to keep.
    pub fn seal_stream(
        &self,
        item_id: Id,
        name: &str,
        mime: &str,
        size: u64,
        reader: &mut dyn std::io::Read,
        out: &mut dyn std::io::Write,
    ) -> Result<PreparedAttachment> {
        let item = self.item(item_id)?;
        let item_key = self.item_key(item)?;
        let att_id = Id::new();
        let att_key = AttachmentKey::from_bytes(lockdown_crypto::rand::random_32()?);
        let wrapped = wrap_key(
            &att_key,
            &item_key,
            &self.attachment_key_place(item_id, att_id),
        )?;
        let meta = AttachmentMeta {
            name: name.to_string(),
            mime: mime.to_string(),
            size,
        };
        let sealed = crate::attachment::seal_attachment_stream(
            &att_key,
            &self.attachment_place(item_id, att_id),
            &meta,
            reader,
            out,
        )?;
        Ok(PreparedAttachment {
            item_id,
            reference: AttachmentRef {
                att_id,
                att_key: Some(wrapped),
                chunks: sealed.chunks,
                blob: sealed.blob_hash,
                blob_size: sealed.blob_size,
                meta: sealed.meta,
            },
        })
    }

    /// Adds a file [`Self::seal_stream`] sealed: a new version of the item
    /// whose signed attachment list names its blob. Returns the
    /// attachment's id.
    pub fn add_prepared(&mut self, prepared: PreparedAttachment) -> Result<Id> {
        let item_id = prepared.item_id;
        let item = self.item(item_id)?;
        let item_key = self.item_key(item)?;
        let header = item.header();
        let latest = item.latest()?;
        let att_id = prepared.reference.att_id;
        let mut attachments = latest.attachments.clone();
        attachments.push(prepared.reference);
        let fields = self.open_envelope(item, latest.n)?;
        let n = latest.n + 1;
        let parent_hash = latest.hash(&header)?;
        let version =
            self.make_version(&header, &item_key, n, parent_hash, &fields, attachments)?;
        self.items
            .get_mut(&item_id)
            .ok_or_else(|| Error::NotFound(format!("item {item_id}")))?
            .versions
            .push(version);
        self.commit_head()?;
        Ok(att_id)
    }

    /// [`Self::open_attachment`] from a blob read in pieces, written to
    /// `out` chunk by chunk (D233).
    pub fn open_attachment_to<R: std::io::Read + std::io::Seek>(
        &self,
        item_id: Id,
        att_id: Id,
        src: &mut R,
        out: &mut dyn std::io::Write,
    ) -> Result<AttachmentMeta> {
        let item = self.item(item_id)?;
        let latest = item.latest()?;
        let r = latest
            .attachments
            .iter()
            .find(|a| a.att_id == att_id)
            .ok_or_else(|| Error::NotFound(format!("attachment {att_id} of item {item_id}")))?;
        let env = r.att_key.as_ref().ok_or(Error::Shredded {
            item_id,
            version_n: latest.n,
        })?;
        let item_key = self.item_key(item)?;
        let key: AttachmentKey =
            unwrap_key(env, &item_key, &self.attachment_key_place(item_id, att_id))?;
        crate::attachment::open_attachment_stream(
            &key,
            &self.attachment_place(item_id, att_id),
            r.chunks,
            r.blob,
            &r.meta,
            src,
            out,
        )
    }

    /// The attachments of an item's newest version, with their sealed
    /// facts opened where the key still exists.
    pub fn attachments(&self, item_id: Id) -> Result<Vec<AttachmentInfo>> {
        let item = self.item(item_id)?;
        let latest = item.latest()?;
        let item_key = item
            .item_key
            .as_ref()
            .map(|_| self.item_key(item))
            .transpose()?;
        let mut out = Vec::with_capacity(latest.attachments.len());
        for r in &latest.attachments {
            let meta = match (&r.att_key, &item_key) {
                (Some(env), Some(item_key)) => {
                    let key: AttachmentKey =
                        unwrap_key(env, item_key, &self.attachment_key_place(item_id, r.att_id))?;
                    Some(open_meta(
                        &key,
                        &self.attachment_place(item_id, r.att_id),
                        r.chunks,
                        &r.meta,
                    )?)
                }
                _ => None,
            };
            out.push(AttachmentInfo {
                att_id: r.att_id,
                meta,
                chunks: r.chunks,
                blob: r.blob,
                blob_size: r.blob_size,
            });
        }
        Ok(out)
    }

    /// Opens an attachment of an item's newest version from its blob: the
    /// blob must hash to the name the signed list carries, every chunk
    /// must open in its place, and the padding comes off.
    pub fn open_attachment(
        &self,
        item_id: Id,
        att_id: Id,
        blob: &[u8],
    ) -> Result<(AttachmentMeta, Zeroizing<Vec<u8>>)> {
        let item = self.item(item_id)?;
        let latest = item.latest()?;
        let r = latest
            .attachments
            .iter()
            .find(|a| a.att_id == att_id)
            .ok_or_else(|| Error::NotFound(format!("attachment {att_id} of item {item_id}")))?;
        let env = r.att_key.as_ref().ok_or(Error::Shredded {
            item_id,
            version_n: latest.n,
        })?;
        let item_key = self.item_key(item)?;
        let key: AttachmentKey =
            unwrap_key(env, &item_key, &self.attachment_key_place(item_id, att_id))?;
        open_attachment(
            &key,
            &self.attachment_place(item_id, att_id),
            r.chunks,
            r.blob,
            &r.meta,
            blob,
        )
    }

    /// The name of every blob some version of some item still holds a key
    /// for: what a store and the server must keep.
    pub fn live_blobs(&self) -> BTreeSet<Hash32> {
        self.items
            .values()
            .flat_map(|i| i.versions.iter())
            .flat_map(|v| v.attachments.iter())
            .filter(|a| !a.is_shredded())
            .map(|a| a.blob)
            .collect()
    }

    /// The name of every blob one item's versions hold a key for: what
    /// travels with the item on push and pull.
    pub fn item_blobs(&self, item_id: Id) -> Result<Vec<(Hash32, u64)>> {
        let item = self.item(item_id)?;
        let mut out: Vec<(Hash32, u64)> = Vec::new();
        for a in item
            .versions
            .iter()
            .flat_map(|v| v.attachments.iter())
            .filter(|a| !a.is_shredded())
        {
            if !out.iter().any(|(h, _)| *h == a.blob) {
                out.push((a.blob, a.blob_size));
            }
        }
        Ok(out)
    }

    /// Blobs sealed here and not yet saved, by name.
    pub fn pending_blobs(&self) -> &[(Hash32, Vec<u8>)] {
        &self.pending_blobs
    }

    /// Hands over a pending blob's bytes (sync pushes them before the
    /// item; the store still gets them at the next save).
    pub fn pending_blob(&self, hash: Hash32) -> Option<&[u8]> {
        self.pending_blobs
            .iter()
            .find(|(h, _)| *h == hash)
            .map(|(_, b)| b.as_slice())
    }

    /// Opens the newest version's fields.
    pub fn open_item(&self, item_id: Id) -> Result<Vec<PlainField>> {
        let item = self.item(item_id)?;
        let n = item.latest()?.n;
        self.open_version(item_id, n)
    }

    /// Opens version `n` of an item, verifying its device signature first.
    pub fn open_version(&self, item_id: Id, n: u64) -> Result<Vec<PlainField>> {
        self.open_envelope(self.item(item_id)?, n)
    }

    /// Opens version `n` of any envelope of this vault, inserted or not
    /// (sync opens the server's copy of an item before deciding what to
    /// keep). The version's device signature is checked against the
    /// trusted devices first.
    pub fn open_envelope(&self, item: &Item, n: u64) -> Result<Vec<PlainField>> {
        if item.vault_id != self.ctx.vault_id {
            return Err(Error::ManifestMismatch {
                item_id: item.item_id,
                why: "the envelope belongs to another vault".into(),
            });
        }
        if item.schema_v > SCHEMA_V {
            return Err(Error::SchemaTooNew {
                found: item.schema_v,
                supported: SCHEMA_V,
            });
        }
        let version = item.version(n)?;
        let key = self
            .key_for(version.edited_by)
            .ok_or(Error::UnknownDevice(version.edited_by))?;
        version.verify(&item.header(), &key)?;
        let item_key = self.item_key(item)?;
        let version_key = self.version_key(item, version, &item_key)?;
        version
            .fields
            .iter()
            .map(|f| f.open(&version_key, &self.field_aad(item.item_id, n, f)))
            .collect()
    }

    // --- what sync needs -------------------------------------------------

    /// The public key of a trusted device, this one included.
    pub fn key_for(&self, device_id: Id) -> Option<VerifyingKey> {
        self.trusted.get(&device_id).cloned()
    }

    /// Every trusted device's key, by id.
    pub fn trusted_devices(&self) -> &BTreeMap<Id, VerifyingKey> {
        &self.trusted
    }

    /// Trusts another device's key, so its heads and versions verify. The
    /// caller got the key from a signed identity record, never from the
    /// server's word.
    pub fn trust_device(&mut self, device_id: Id, key: VerifyingKey) {
        self.trusted.insert(device_id, key);
    }

    /// Marks a device revoked: heads it signs after `after` are refused
    /// from now on (in [`Self::rebase`] and [`Self::verify`]); what it
    /// signed up to `after` still counts. The caller read `after` from the
    /// synced identity manifest (D73). A boundary only ever moves down to
    /// the first one given for this device.
    pub fn revoke_device(&mut self, device_id: Id, after: u64) {
        self.revoked
            .entry(device_id)
            .and_modify(|a| *a = (*a).min(after))
            .or_insert(after);
    }

    /// The newest head a revoked device may have signed here, if it is
    /// revoked.
    pub fn revoked_after(&self, device_id: Id) -> Option<u64> {
        self.revoked.get(&device_id).copied()
    }

    /// The head at `seq`, if this vault holds it.
    pub fn head_at(&self, seq: u64) -> Option<&ManifestHead> {
        self.heads.iter().find(|h| h.seq == seq)
    }

    /// Records the revision the server assigned to an item. The revision
    /// sits outside every signature; only the manifest entries name it.
    pub fn set_item_rev(&mut self, item_id: Id, rev: u64) -> Result<()> {
        self.items
            .get_mut(&item_id)
            .ok_or_else(|| Error::NotFound(format!("item {item_id}")))?
            .rev = rev;
        Ok(())
    }

    /// Takes in an envelope from another device (the server's copy),
    /// replacing any local one: it must be this vault's, readable by this
    /// build, and its latest version signed by a trusted device.
    pub fn adopt_item(&mut self, item: Item) -> Result<()> {
        if item.vault_id != self.ctx.vault_id {
            return Err(Error::ManifestMismatch {
                item_id: item.item_id,
                why: "the envelope belongs to another vault".into(),
            });
        }
        if item.v > ITEM_V {
            return Err(Error::SchemaTooNew {
                found: item.v,
                supported: ITEM_V,
            });
        }
        let latest = item.latest()?;
        let key = self
            .key_for(latest.edited_by)
            .ok_or(Error::UnknownDevice(latest.edited_by))?;
        latest.verify(&item.header(), &key)?;
        self.items.insert(item.item_id, item);
        Ok(())
    }

    /// Replaces every head after `keep_through` with `remote` (the server's
    /// chain, pulled since that seq), after verifying that `remote`
    /// continues this vault's head at `keep_through` and is signed by
    /// trusted devices. Local heads after `keep_through` were never on the
    /// server; their changes live on in the items and go into the next
    /// head this device commits. With `keep_through` zero, `remote` is the
    /// whole chain.
    pub fn rebase(&mut self, keep_through: u64, remote: Vec<ManifestHead>) -> Result<()> {
        if remote.is_empty() {
            return Ok(());
        }
        let mut chain: Vec<ManifestHead> = Vec::with_capacity(remote.len() + 1);
        if keep_through > 0 {
            let anchor = self
                .head_at(keep_through)
                .ok_or_else(|| Error::NotFound(format!("this vault's manifest {keep_through}")))?;
            chain.push(anchor.clone());
        } else if remote[0].seq != 1 {
            return Err(Error::ChainBroken {
                seq: remote[0].seq,
                why: "the server's chain doesn't start at 1".into(),
            });
        }
        chain.extend(remote.iter().cloned());
        verify_chain(&chain, |id| self.key_for(id), 0)?;
        check_revocations(&remote, |id| self.revoked_after(id))?;
        self.heads.retain(|h| h.seq <= keep_through);
        self.heads.extend(remote);
        self.members = self.members()?;
        Ok(())
    }

    /// Sets the key-transparency tree head every head committed from now
    /// on cites (04-security §6 gossip). Sync sets it after verifying the
    /// server's current tree head; `None` cites nothing.
    pub fn cite_sth(&mut self, sth: Option<SthRef>) {
        self.cited_sth = sth;
    }

    /// The citation heads carry now.
    pub fn cited_sth(&self) -> Option<SthRef> {
        self.cited_sth
    }

    /// Drops every head after `seq`: the provisional heads this device
    /// committed offline, just before sync commits one head in their
    /// place over the same items. Nothing about the items changes.
    pub fn truncate_heads_after(&mut self, seq: u64) {
        self.heads.retain(|h| h.seq <= seq);
    }

    /// Commits a new head over the items as they are now and returns its
    /// `seq`. Every mutation commits on its own; sync commits once more
    /// after it has set the server's revisions.
    pub fn commit(&mut self) -> Result<u64> {
        self.commit_head()?;
        Ok(self.seq())
    }

    /// The items that differ from what the head at `seq` recorded (latest
    /// version, state, keys), plus any the head does not list: what has
    /// changed on this device since that head. With `seq` zero, every item.
    pub fn changed_since(&self, seq: u64) -> Result<Vec<Id>> {
        let base = if seq == 0 { None } else { self.head_at(seq) };
        let mut out = Vec::new();
        for item in self.items.values() {
            let same = match base.and_then(|h| h.entries.iter().find(|e| e.item_id == item.item_id))
            {
                Some(entry) => {
                    entry.version_hash == item.latest_hash()?
                        && entry.state == item.state
                        && entry.shredded == item.is_shredded()
                }
                None => false,
            };
            if !same {
                out.push(item.item_id);
            }
        }
        Ok(out)
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

    /// Brings a trashed item back as live, before its grace period ends.
    /// Only the trash comes back: an archived item is not in the trash,
    /// and a purged one has no keys to come back with.
    pub fn restore_item(&mut self, item_id: Id) -> Result<()> {
        let item = self
            .items
            .get_mut(&item_id)
            .ok_or_else(|| Error::NotFound(format!("item {item_id}")))?;
        if item.state != ItemState::Trashed {
            return Err(Error::Invalid(format!("item {item_id} isn't in the trash")));
        }
        item.state = ItemState::Live;
        item.trashed_at = None;
        self.commit_head()
    }

    /// Archives a live item (02-model §7): out of autofill, agents, default
    /// search and the main list, kept in exports, nothing destroyed.
    pub fn archive_item(&mut self, item_id: Id) -> Result<()> {
        let item = self
            .items
            .get_mut(&item_id)
            .ok_or_else(|| Error::NotFound(format!("item {item_id}")))?;
        match item.state {
            ItemState::Archived => return Ok(()),
            ItemState::Live => {}
            _ => {
                return Err(Error::Invalid(format!(
                    "item {item_id} isn't live, so it can't be archived"
                )));
            }
        }
        item.state = ItemState::Archived;
        self.commit_head()
    }

    /// Brings an archived item back to the main list.
    pub fn unarchive_item(&mut self, item_id: Id) -> Result<()> {
        let item = self
            .items
            .get_mut(&item_id)
            .ok_or_else(|| Error::NotFound(format!("item {item_id}")))?;
        match item.state {
            ItemState::Live => return Ok(()),
            ItemState::Archived => {}
            _ => {
                return Err(Error::Invalid(format!("item {item_id} isn't archived")));
            }
        }
        item.state = ItemState::Live;
        self.commit_head()
    }

    /// Purges an item: destroys its Item Key, every Version Key and every
    /// Attachment Key, leaves a tombstone with the last version hash, and
    /// commits. The ciphertext may sit in backups forever and stays dead.
    pub fn purge_item(&mut self, item_id: Id) -> Result<()> {
        self.shred_item(item_id)?;
        self.commit_head()
    }

    /// The trashed items whose grace period of `grace_secs` has run out at
    /// `now` (seconds since the epoch), leaving out any in `hold` (an open
    /// conflict, D58): what [`Self::purge_due`] would purge.
    pub fn due_for_purge(&self, now: i64, grace_secs: i64, hold: &BTreeSet<Id>) -> Vec<Id> {
        self.items
            .values()
            .filter(|i| i.state == ItemState::Trashed && !hold.contains(&i.item_id))
            .filter(|i| {
                i.trashed_at
                    .is_some_and(|t| t.0.saturating_add(grace_secs) <= now)
            })
            .map(|i| i.item_id)
            .collect()
    }

    /// Purges every trashed item whose grace period has run out (02-model
    /// §7), except those in `hold`, under one new head. Every device runs
    /// this on its own clock, so the purge happens wherever the vault is
    /// open; a tombstone from whichever device was first syncs to the rest.
    /// Returns what was purged.
    pub fn purge_due(&mut self, now: i64, grace_secs: i64, hold: &BTreeSet<Id>) -> Result<Vec<Id>> {
        let due = self.due_for_purge(now, grace_secs, hold);
        for id in &due {
            self.shred_item(*id)?;
        }
        if !due.is_empty() {
            self.commit_head()?;
        }
        Ok(due)
    }

    fn shred_item(&mut self, item_id: Id) -> Result<()> {
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
        Ok(())
    }

    /// Takes a file off an item: a new version whose attachment list no
    /// longer names it. Earlier versions still do, so the history keeps it
    /// (and its blob stays) until those versions are shredded or the item
    /// is purged. Returns the new version's number.
    pub fn remove_attachment(&mut self, item_id: Id, att_id: Id) -> Result<u64> {
        let item = self.item(item_id)?;
        if !matches!(item.state, ItemState::Live | ItemState::Archived) {
            return Err(Error::Invalid(
                "this item is in the trash; restore it before changing it".into(),
            ));
        }
        let item_key = self.item_key(item)?;
        let header = item.header();
        let latest = item.latest()?;
        if !latest.attachments.iter().any(|a| a.att_id == att_id) {
            return Err(Error::NotFound(format!(
                "attachment {att_id} of item {item_id}"
            )));
        }
        let attachments: Vec<AttachmentRef> = latest
            .attachments
            .iter()
            .filter(|a| a.att_id != att_id)
            .cloned()
            .collect();
        let fields = self.open_envelope(item, latest.n)?;
        let n = latest.n + 1;
        let parent_hash = latest.hash(&header)?;
        let version =
            self.make_version(&header, &item_key, n, parent_hash, &fields, attachments)?;
        self.items
            .get_mut(&item_id)
            .ok_or_else(|| Error::NotFound(format!("item {item_id}")))?
            .versions
            .push(version);
        self.commit_head()?;
        Ok(n)
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
        let plain = open(
            self.vault_key_named(head.vault_key_ref)?,
            &self.members_aad(head.seq),
            &head.members,
        )?;
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

    fn attachment_key_place(&self, item_id: Id, att_id: Id) -> KeyPlace {
        KeyPlace {
            profile_id: self.ctx.profile_id,
            persona_id: self.ctx.persona_id,
            vault_id: self.ctx.vault_id,
            key_id: att_id,
            purpose: KeyPurpose::Attachment,
            wrapped_by: WrappedBy::new(WrapperKind::Item, item_id),
            v: WRAP_V,
        }
    }

    fn attachment_place(&self, item_id: Id, att_id: Id) -> AttachmentPlace {
        AttachmentPlace {
            profile_id: self.ctx.profile_id,
            persona_id: self.ctx.persona_id,
            vault_id: self.ctx.vault_id,
            item_id,
            att_id,
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
        let mut place = self.item_key_place(item.item_id, item.class);
        match item.class {
            ItemClass::Standard => {
                // A shared vault may still hold an item under an earlier
                // Vault Key generation until its rotation reaches it.
                place.wrapped_by.id = env.wrapped_by.id;
                unwrap_key(env, self.vault_key_named(env.wrapped_by.id)?, &place)
            }
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
        attachments: Vec<AttachmentRef>,
    ) -> Result<ItemVersion> {
        self.make_version_at(
            header,
            item_key,
            n,
            parent_hash,
            fields,
            attachments,
            Timestamp::now(),
        )
    }

    /// [`Self::make_version`] stamped `edited_at` (a carried version keeps
    /// the time it was first saved).
    #[allow(clippy::too_many_arguments)]
    fn make_version_at(
        &self,
        header: &ItemHeader,
        item_key: &ItemKey,
        n: u64,
        parent_hash: Hash32,
        fields: &[PlainField],
        attachments: Vec<AttachmentRef>,
        edited_at: Timestamp,
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
            edited_at,
            fields: sealed,
            attachments,
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
            sth: self.cited_sth,
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
    use crate::field::FieldValue;
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
    fn a_revoked_device_keeps_its_history_and_loses_the_future() {
        // One device wrote heads 1–3. Revoked after head 2: head 3 is
        // refused, heads 1–2 still count, and a later vault keeps the rule.
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
        let device = vault.device_id();
        let heads = vault.heads.clone();
        check_revocations(&heads, |_| None).unwrap();
        check_revocations(&heads[..2], |d| (d == device).then_some(2)).unwrap();
        let err = check_revocations(&heads, |d| (d == device).then_some(2)).unwrap_err();
        assert!(
            matches!(
                err,
                Error::RevokedDevice {
                    device: d,
                    seq: 3,
                    after: 2
                } if d == device
            ),
            "{err}"
        );
        assert!(err.to_string().contains("after that device was revoked"));
        // Through the vault: `verify` consults what it was told.
        vault.verify(0).unwrap();
        vault.revoke_device(device, 2);
        assert!(matches!(
            vault.verify(0),
            Err(Error::RevokedDevice { seq: 3, .. })
        ));
        // A boundary only moves down, never up.
        vault.revoke_device(device, 5);
        assert_eq!(vault.revoked_after(device), Some(2));
        vault.revoke_device(device, 1);
        assert_eq!(vault.revoked_after(device), Some(1));
    }

    #[test]
    fn add_and_edit_go_through_the_template_and_seal_like_any_version() {
        use crate::compose::{Change, FieldChange};
        let mut vault = sample_vault();
        let wallet = vault
            .add_item(
                ItemKind::CryptoWallet,
                "Cold wallet",
                false,
                vec![
                    FieldChange::secret("seed", b"abandon ability able about above absent absorb abstract absurd abuse access accident"),
                    FieldChange::text("wallet.network", "Bitcoin"),
                ],
            )
            .unwrap();
        let item = vault.item(wallet).unwrap();
        assert_eq!(
            item.class,
            ItemClass::CrownJewel,
            "the seed's default (02-model §3)"
        );
        assert_eq!(
            item.item_key.as_ref().unwrap().wrapped_by.kind,
            WrapperKind::CrownJewel
        );
        let n = vault
            .edit_item(
                wallet,
                Some("Cold wallet (2026)"),
                vec![
                    FieldChange::keep("seed"),
                    FieldChange::new("wallet.network", Change::Remove),
                    FieldChange::new("wallet.password", Change::Generate),
                ],
            )
            .unwrap();
        assert_eq!(n, 2);
        let fields = vault.open_item(wallet).unwrap();
        let ids: Vec<&str> = fields.iter().map(|f| f.field_id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "title",
                "seed",
                "wallet.password",
                "org.folder",
                "org.tags",
                "org.favourite"
            ],
            "the organization fields ride on every composed item, kept by an edit that leaves them out"
        );
        assert_eq!(
            fields[1].value,
            FieldValue::Text("abandon ability able about above absent absorb abstract absurd abuse access accident".into()),
            "kept, never resent"
        );
        // The first version is still there, unchanged.
        assert_eq!(vault.open_version(wallet, 1).unwrap().len(), 6);
        vault.verify(0).unwrap();

        // A refused edit writes nothing.
        let heads = vault.heads.len();
        assert!(vault.edit_item(wallet, None, vec![]).is_err());
        assert_eq!(vault.heads.len(), heads);
        vault.trash_item(wallet).unwrap();
        let err = vault
            .edit_item(
                wallet,
                None,
                vec![
                    FieldChange::keep("seed"),
                    FieldChange::keep("wallet.password"),
                ],
            )
            .unwrap_err();
        assert!(err.to_string().contains("restore it"), "{err}");
    }

    #[test]
    fn trash_then_restore_brings_the_item_back_live_with_its_history() {
        let mut vault = sample_vault();
        let id = vault
            .create_item(ItemKind::Login, ItemClass::Standard, login_fields("one"))
            .unwrap();
        vault.update_item(id, login_fields("two")).unwrap();
        let heads_before = vault.heads.len();
        vault.trash_item(id).unwrap();
        assert_eq!(vault.item(id).unwrap().state, ItemState::Trashed);
        assert!(vault.item(id).unwrap().trashed_at.is_some());
        vault.restore_item(id).unwrap();
        let item = vault.item(id).unwrap();
        assert_eq!(item.state, ItemState::Live);
        assert!(item.trashed_at.is_none());
        assert_eq!(item.versions.len(), 2, "nothing was rewritten");
        assert_eq!(vault.open_item(id).unwrap(), login_fields("two"));
        // Each move is a committed head, so the other devices learn it.
        assert_eq!(vault.heads.len(), heads_before + 2);
        assert_eq!(
            vault.heads.last().unwrap().entries[0].state,
            ItemState::Live
        );
        // A live item is not in the trash; a purged one cannot come back.
        assert!(matches!(vault.restore_item(id), Err(Error::Invalid(_))));
        vault.purge_item(id).unwrap();
        assert!(matches!(vault.restore_item(id), Err(Error::Invalid(_))));
    }

    #[test]
    fn archive_and_unarchive_move_only_between_live_and_archived() {
        let mut vault = sample_vault();
        let id = vault
            .create_item(ItemKind::Login, ItemClass::Standard, login_fields("pw"))
            .unwrap();
        vault.archive_item(id).unwrap();
        assert_eq!(vault.item(id).unwrap().state, ItemState::Archived);
        let seq = vault.seq();
        vault.archive_item(id).unwrap();
        assert_eq!(vault.seq(), seq, "archiving twice writes nothing");
        assert_eq!(vault.head().unwrap().entries[0].state, ItemState::Archived);
        vault.unarchive_item(id).unwrap();
        assert_eq!(vault.item(id).unwrap().state, ItemState::Live);
        vault.trash_item(id).unwrap();
        assert!(vault.archive_item(id).is_err());
        assert!(vault.unarchive_item(id).is_err());
        vault.verify(0).unwrap();
    }

    #[test]
    fn the_trash_purges_itself_after_its_grace_period_unless_held() {
        let mut vault = sample_vault();
        let a = vault
            .create_item(ItemKind::Login, ItemClass::Standard, login_fields("a"))
            .unwrap();
        let b = vault
            .create_item(ItemKind::Login, ItemClass::Standard, login_fields("b"))
            .unwrap();
        let c = vault
            .create_item(ItemKind::Login, ItemClass::Standard, login_fields("c"))
            .unwrap();
        vault.trash_item(a).unwrap();
        vault.trash_item(b).unwrap();
        let now = Timestamp::now().0;
        let day = 86_400;
        let none = BTreeSet::new();
        assert!(vault.purge_due(now, 30 * day, &none).unwrap().is_empty());
        let later = now + 31 * day;
        let hold = BTreeSet::from([b]);
        let seq = vault.seq();
        assert_eq!(vault.purge_due(later, 30 * day, &hold).unwrap(), [a]);
        assert_eq!(vault.seq(), seq + 1, "one head for the sweep");
        assert!(vault.item(a).unwrap().is_shredded());
        assert_eq!(vault.item(b).unwrap().state, ItemState::Trashed, "held");
        assert_eq!(vault.item(c).unwrap().state, ItemState::Live);
        assert_eq!(vault.purge_due(later, 30 * day, &none).unwrap(), [b]);
        let seq = vault.seq();
        assert!(vault.purge_due(later, 30 * day, &none).unwrap().is_empty());
        assert_eq!(vault.seq(), seq, "nothing due, nothing written");
        vault.verify(0).unwrap();
    }

    #[test]
    fn removing_an_attachment_keeps_it_in_the_history() {
        let mut vault = sample_vault();
        let id = vault
            .create_item(ItemKind::Login, ItemClass::Standard, login_fields("pw"))
            .unwrap();
        let (att, blob) = vault.attach(id, "a.txt", "text/plain", b"hello").unwrap();
        let n = vault.remove_attachment(id, att).unwrap();
        assert_eq!(n, 3);
        assert!(vault.attachments(id).unwrap().is_empty());
        assert!(
            vault.live_blobs().contains(&blob),
            "version 2 still names it"
        );
        assert!(vault.remove_attachment(id, att).is_err());
        vault.purge_item(id).unwrap();
        assert!(!vault.live_blobs().contains(&blob));
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
    fn attachments_ride_with_the_item_and_die_with_its_keys() {
        let mut vault = sample_vault();
        let id = vault
            .create_item(ItemKind::Document, ItemClass::Standard, login_fields("x"))
            .unwrap();
        let data: Vec<u8> = (0..(5 << 20) + 17).map(|i| (i % 13) as u8).collect();
        let (att, blob_hash) = vault
            .attach(id, "scan.pdf", "application/pdf", &data)
            .unwrap();
        let blob = vault.pending_blob(blob_hash).unwrap().to_vec();
        assert_eq!(lockdown_crypto::hash::hash(&blob), blob_hash);
        // The signed list names it; the version carries it; the fields held.
        let listed = vault.attachments(id).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].att_id, att);
        assert_eq!(listed[0].chunks, 6);
        assert_eq!(listed[0].meta.as_ref().unwrap().name, "scan.pdf");
        assert_eq!(listed[0].meta.as_ref().unwrap().size, data.len() as u64);
        assert_eq!(vault.open_item(id).unwrap(), login_fields("x"));
        let (meta, plain) = vault.open_attachment(id, att, &blob).unwrap();
        assert_eq!(meta.mime, "application/pdf");
        assert_eq!(plain.as_slice(), data.as_slice());
        // An edit keeps the attachment; the version signature covers the list.
        vault.update_item(id, login_fields("y")).unwrap();
        assert_eq!(vault.attachments(id).unwrap().len(), 1);
        assert_eq!(
            vault.open_attachment(id, att, &blob).unwrap().1.as_slice(),
            data.as_slice()
        );
        let mut forged = vault.items[&id].clone();
        forged.versions.last_mut().unwrap().attachments[0].chunks = 7;
        assert!(matches!(
            vault.open_envelope(&forged, forged.latest().unwrap().n),
            Err(Error::Crypto(lockdown_crypto::Error::BadSignature(_)))
        ));
        // A blob that isn't the one the list names is refused by name.
        let mut other = blob.clone();
        other[100] ^= 1;
        assert!(matches!(
            vault.open_attachment(id, att, &other),
            Err(Error::BlobMismatch { .. })
        ));
        assert_eq!(vault.live_blobs().len(), 1);
        assert_eq!(vault.item_blobs(id).unwrap()[0].0, blob_hash);
        // Purge: the Attachment Key is gone, the blob is dead, nothing lists it.
        vault.purge_item(id).unwrap();
        assert!(matches!(
            vault.open_attachment(id, att, &blob),
            Err(Error::Shredded { .. })
        ));
        assert!(vault.attachments(id).unwrap()[0].meta.is_none());
        assert!(vault.live_blobs().is_empty());
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
