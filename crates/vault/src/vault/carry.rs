//! Moving an item to another vault, or into or out of the crown-jewel
//! class: the item is opened in full here and sealed again there, under
//! new keys, and the old one is purged by the caller.
//!
//! Every field's associated data names its vault (02-model §2), and every
//! version's signature covers the item's class, so neither a move nor a
//! change of class can be a re-wrap of one key: the fields have to be
//! sealed again in their new place. [`Vault::carry_out`] opens every
//! version that is still readable (each signature checked first) and every
//! attachment once; [`Vault::carry_in`] makes a new item from them with a
//! fresh Item Key under the target's branch (its Vault Key, or the
//! persona's Crown-Jewel Key), a fresh Version Key per version, a fresh
//! Attachment Key per file, signed by this device. Each version keeps its
//! number and the time it was first saved, so the history reads the same;
//! a version already shredded stays behind, dead, with the old item.

use std::collections::BTreeMap;

use ecosystem_identity::SymmetricKey;
use zeroize::Zeroizing;

use super::Vault;
use crate::attachment::{AttachmentMeta, open_attachment, seal_attachment};
use crate::field::PlainField;
use crate::item::{AttachmentRef, ITEM_V, Item, ItemClass, ItemHeader, ItemKind, ItemState};
use crate::keys::{AttachmentKey, ItemKey, unwrap_key, wrap_key};
use crate::{Error, Hash32, Id, Result, SCHEMA_V, Timestamp};

/// One file of a carried item, opened.
pub struct CarriedAttachment {
    /// Its name, type and size.
    pub meta: AttachmentMeta,
    /// Its bytes; wiped when dropped.
    pub bytes: Zeroizing<Vec<u8>>,
}

/// One readable version of a carried item, opened.
pub struct CarriedVersion {
    /// Its number in the old item; kept in the new one.
    pub n: u64,
    /// When it was first saved; kept.
    pub edited_at: Timestamp,
    /// Its fields.
    pub fields: Vec<PlainField>,
    /// The files it names, by their id in the old item.
    pub attachments: Vec<Id>,
}

/// Everything still readable of one item, opened, on its way to a new
/// place. Holds secrets: every field and file is wiped when it drops.
pub struct CarriedItem {
    /// The item it came from.
    pub from: Id,
    /// The kind.
    pub kind: ItemKind,
    /// The class it had.
    pub class: ItemClass,
    /// Whether it never syncs.
    pub device_only: bool,
    /// Live or archived.
    pub state: ItemState,
    /// Every readable version, oldest first.
    pub versions: Vec<CarriedVersion>,
    /// Every readable file, by its id in the old item.
    pub attachments: BTreeMap<Id, CarriedAttachment>,
}

impl std::fmt::Debug for CarriedItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CarriedItem")
            .field("from", &self.from)
            .field("kind", &self.kind)
            .field("versions", &self.versions.len())
            .field("attachments", &self.attachments.len())
            .finish_non_exhaustive()
    }
}

impl Vault {
    /// Opens everything still readable of a live or archived item, to be
    /// sealed again by [`Self::carry_in`] in this vault or another. `blob`
    /// fetches an attachment's ciphertext from the store (a blob sealed
    /// here and not yet saved is found without it). Nothing changes here;
    /// the caller purges the item once the new one is saved.
    pub fn carry_out(
        &self,
        item_id: Id,
        blob: &mut dyn FnMut(Hash32) -> Result<Vec<u8>>,
    ) -> Result<CarriedItem> {
        let item = self.item(item_id)?;
        if item.kind.hidden() {
            return Err(Error::Invalid("that item can't be moved".into()));
        }
        if !matches!(item.state, ItemState::Live | ItemState::Archived) {
            return Err(Error::Invalid(
                "this item is in the trash; restore it before moving it".into(),
            ));
        }
        let item_key = self.item_key(item)?;
        let mut versions = Vec::new();
        let mut attachments = BTreeMap::new();
        for v in item.versions.iter().filter(|v| !v.is_shredded()) {
            let fields = self.open_envelope(item, v.n)?;
            let mut names = Vec::new();
            for r in &v.attachments {
                let Some(env) = &r.att_key else { continue };
                names.push(r.att_id);
                if attachments.contains_key(&r.att_id) {
                    continue;
                }
                let key: AttachmentKey = unwrap_key(
                    env,
                    &item_key,
                    &self.attachment_key_place(item_id, r.att_id),
                )?;
                let bytes = match self.pending_blob(r.blob) {
                    Some(b) => b.to_vec(),
                    None => blob(r.blob)?,
                };
                let (meta, plain) = open_attachment(
                    &key,
                    &self.attachment_place(item_id, r.att_id),
                    r.chunks,
                    r.blob,
                    &r.meta,
                    &bytes,
                )?;
                attachments.insert(r.att_id, CarriedAttachment { meta, bytes: plain });
            }
            versions.push(CarriedVersion {
                n: v.n,
                edited_at: v.edited_at,
                fields,
                attachments: names,
            });
        }
        if versions.is_empty() {
            return Err(Error::Shredded {
                item_id,
                version_n: 0,
            });
        }
        Ok(CarriedItem {
            from: item_id,
            kind: item.kind,
            class: item.class,
            device_only: item.device_only,
            state: item.state,
            versions,
            attachments,
        })
    }

    /// Makes a new item in this vault from a carried one, in `class`: new
    /// ids, new keys, every version sealed and signed here, each keeping
    /// its number and time. Returns the new item's id.
    pub fn carry_in(&mut self, carried: &CarriedItem, class: ItemClass) -> Result<Id> {
        let item_id = Id::new();
        let item_key = ItemKey::from_bytes(lockdown_crypto::rand::random_32()?);
        let wrapped_item_key = self.wrap_item_key(item_id, class, &item_key)?;
        let header = ItemHeader {
            item_id,
            vault_id: self.ctx.vault_id,
            kind: carried.kind,
            schema_v: SCHEMA_V,
            class,
            device_only: carried.device_only,
        };
        let mut refs: BTreeMap<Id, AttachmentRef> = BTreeMap::new();
        let mut blobs = Vec::new();
        for (old, file) in &carried.attachments {
            let att_id = Id::new();
            let att_key = AttachmentKey::from_bytes(lockdown_crypto::rand::random_32()?);
            let wrapped = wrap_key(
                &att_key,
                &item_key,
                &self.attachment_key_place(item_id, att_id),
            )?;
            let sealed = seal_attachment(
                &att_key,
                &self.attachment_place(item_id, att_id),
                &file.meta,
                &file.bytes,
            )?;
            refs.insert(
                *old,
                AttachmentRef {
                    att_id,
                    att_key: Some(wrapped),
                    chunks: sealed.chunks,
                    blob: sealed.blob_hash,
                    blob_size: sealed.blob.len() as u64,
                    meta: sealed.meta,
                },
            );
            blobs.push((sealed.blob_hash, sealed.blob));
        }
        let mut versions = Vec::with_capacity(carried.versions.len());
        let mut parent = Hash32::ZERO;
        let mut last_n = 0;
        for v in &carried.versions {
            if v.n <= last_n {
                return Err(Error::Invalid(
                    "a carried item's versions must rise in order".into(),
                ));
            }
            last_n = v.n;
            let attachments = v
                .attachments
                .iter()
                .map(|old| {
                    refs.get(old).cloned().ok_or_else(|| {
                        Error::NotFound(format!("attachment {old} of the carried item"))
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            let version = self.make_version_at(
                &header,
                &item_key,
                v.n,
                parent,
                &v.fields,
                attachments,
                v.edited_at,
            )?;
            parent = version.hash(&header)?;
            versions.push(version);
        }
        let state = match carried.state {
            ItemState::Archived => ItemState::Archived,
            _ => ItemState::Live,
        };
        self.items.insert(
            item_id,
            Item {
                v: ITEM_V,
                item_id,
                vault_id: self.ctx.vault_id,
                rev: 0,
                kind: carried.kind,
                schema_v: SCHEMA_V,
                class,
                device_only: carried.device_only,
                item_key: Some(wrapped_item_key),
                versions,
                state,
                trashed_at: None,
            },
        );
        self.pending_blobs.extend(blobs);
        self.commit_head()?;
        Ok(item_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::FieldValue;
    use crate::test_support::{sample_login_fields, sample_vault};

    fn no_store(_: Hash32) -> Result<Vec<u8>> {
        Err(Error::NotFound("a store".into()))
    }

    #[test]
    fn an_item_moves_to_another_vault_with_its_history_and_files() {
        let mut from = sample_vault();
        let mut to = sample_vault();
        let id = from
            .create_item(ItemKind::Login, ItemClass::Standard, sample_login_fields())
            .unwrap();
        let mut second = sample_login_fields();
        second[3] = PlainField::hidden("pw", "password", "a newer password");
        from.update_item(id, second).unwrap();
        from.attach(id, "scan.pdf", "application/pdf", b"%PDF-1.7 scan")
            .unwrap();
        from.archive_item(id).unwrap();
        let carried = from.carry_out(id, &mut no_store).unwrap();
        assert_eq!(carried.versions.len(), 3);
        let new_id = to.carry_in(&carried, ItemClass::Standard).unwrap();
        drop(carried);
        from.purge_item(id).unwrap();

        let moved = to.item(new_id).unwrap();
        assert_eq!(moved.state, ItemState::Archived);
        assert_eq!(moved.vault_id, to.context().vault_id);
        assert_eq!(
            moved.versions.iter().map(|v| v.n).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        let v1 = to.open_version(new_id, 1).unwrap();
        assert_eq!(v1, sample_login_fields(), "the history moved too");
        let latest = to.open_item(new_id).unwrap();
        assert!(
            latest
                .iter()
                .any(|f| f.value == FieldValue::Text("a newer password".into()))
        );
        let files = to.attachments(new_id).unwrap();
        assert_eq!(files.len(), 1);
        let (meta, bytes) = to
            .open_attachment(
                new_id,
                files[0].att_id,
                to.pending_blob(files[0].blob).unwrap(),
            )
            .unwrap();
        assert_eq!(meta.name, "scan.pdf");
        assert_eq!(bytes.as_slice(), b"%PDF-1.7 scan");
        assert!(from.item(id).unwrap().is_shredded());
        // The new item verifies under the target's chain.
        to.verify(0).unwrap();
    }

    #[test]
    fn an_item_changes_class_by_being_sealed_again() {
        let mut vault = sample_vault();
        let id = vault
            .create_item(ItemKind::Login, ItemClass::Standard, sample_login_fields())
            .unwrap();
        let carried = vault.carry_out(id, &mut no_store).unwrap();
        let jewel = vault.carry_in(&carried, ItemClass::CrownJewel).unwrap();
        drop(carried);
        vault.purge_item(id).unwrap();
        let item = vault.item(jewel).unwrap();
        assert_eq!(item.class, ItemClass::CrownJewel);
        assert_eq!(
            item.item_key.as_ref().unwrap().wrapped_by.kind,
            crate::keys::WrapperKind::CrownJewel
        );
        assert_eq!(vault.open_item(jewel).unwrap(), sample_login_fields());

        // A trashed item is restored before it moves; a shredded one can't.
        vault.trash_item(jewel).unwrap();
        assert!(vault.carry_out(jewel, &mut no_store).is_err());
        assert!(vault.carry_out(id, &mut no_store).is_err());
    }
}
