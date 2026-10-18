//! Bringing items in whole: an import or a restore lands an item with its
//! history, its attachments and its state in one signed head, and the
//! per-item export opt-in (D121) is changed as a new version, so it syncs.

use ecosystem_identity::SymmetricKey;
use zeroize::Zeroizing;

use super::Vault;
use crate::attachment::{AttachmentMeta, seal_attachment};
use crate::field::{FieldKind, FieldValue, PlainField, SCHEMA_V};
use crate::item::{AttachmentRef, ITEM_V, Item, ItemClass, ItemHeader, ItemKind, ItemState};
use crate::keys::{AttachmentKey, ItemKey, wrap_key};
use crate::{Error, Hash32, Id, Result, Timestamp};

/// The field that says a crown jewel may leave in a bulk export or a
/// backup (D121, 02-model §3 "out of bulk export unless the user opts each
/// item in"). A yes-or-no field sealed with the item's other fields, so it
/// is signed, versioned and synced like them. Shells never see it as a
/// field: `show` reports it as a flag, and an edit keeps it without naming
/// it, as it keeps the title.
pub const EXPORT_OPT_IN: &str = "lockdown.export";

/// Whether `fields` carry the export opt-in, set to yes.
pub fn export_opted_in(fields: &[PlainField]) -> bool {
    fields
        .iter()
        .any(|f| f.field_id == EXPORT_OPT_IN && f.value == FieldValue::Boolean(true))
}

/// Whether a field id is one the app keeps for itself (never drawn as a
/// field, never named by an edit).
pub fn is_reserved_field(field_id: &str) -> bool {
    field_id.starts_with("lockdown.")
}

/// A file to attach as an item comes in.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct NewAttachment {
    /// Its name.
    pub name: String,
    /// Its media type.
    pub mime: String,
    /// Its bytes, wiped when dropped.
    pub bytes: Zeroizing<Vec<u8>>,
}

/// An item as an importer or a backup hands it over.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ImportSpec {
    /// The id to keep (a restore keeps ids so a second restore merges);
    /// `None` makes a fresh one.
    pub item_id: Option<Id>,
    /// The type.
    pub kind: ItemKind,
    /// Standard or crown jewel.
    pub class: ItemClass,
    /// Every version's fields, oldest first; the last is the item now.
    pub versions: Vec<Vec<PlainField>>,
    /// Files of the newest version.
    pub attachments: Vec<NewAttachment>,
    /// Live, archived or trashed. A tombstone can't be brought in.
    pub state: ItemState,
}

impl Vault {
    /// Lands one item from an import or a restore, with every version in
    /// `spec.versions` saved in order (each its own Version Key, signed by
    /// this device, chained to the one before) and the attachments sealed
    /// into the newest; then one head is committed. Returns the item's id.
    /// Refused for an id this vault already holds, for no versions, and
    /// for a tombstone.
    pub fn import_item(&mut self, spec: ImportSpec) -> Result<Id> {
        if spec.versions.is_empty() {
            return Err(Error::Invalid("an item needs at least one version".into()));
        }
        if spec.state == ItemState::Tombstone {
            return Err(Error::Invalid("a purged item can't be brought in".into()));
        }
        let item_id = spec.item_id.unwrap_or_default();
        if self.items.contains_key(&item_id) {
            return Err(Error::Invalid(format!(
                "item {item_id} is already in this vault"
            )));
        }
        let item_key = ItemKey::from_bytes(lockdown_crypto::rand::random_32()?);
        let wrapped_item_key = self.wrap_item_key(item_id, spec.class, &item_key)?;
        let header = ItemHeader {
            item_id,
            vault_id: self.ctx.vault_id,
            kind: spec.kind,
            schema_v: SCHEMA_V,
            class: spec.class,
            device_only: false,
        };
        let mut attachments = Vec::with_capacity(spec.attachments.len());
        let mut blobs = Vec::with_capacity(spec.attachments.len());
        for a in &spec.attachments {
            let (r, blob) = self.seal_new_attachment(item_id, &item_key, a)?;
            blobs.push((r.blob, blob));
            attachments.push(r);
        }
        let last = spec.versions.len() - 1;
        let mut versions = Vec::with_capacity(spec.versions.len());
        let mut parent = Hash32::ZERO;
        for (i, fields) in spec.versions.iter().enumerate() {
            let files = if i == last {
                std::mem::take(&mut attachments)
            } else {
                Vec::new()
            };
            let v = self.make_version(&header, &item_key, i as u64 + 1, parent, fields, files)?;
            parent = v.hash(&header)?;
            versions.push(v);
        }
        let item = Item {
            v: ITEM_V,
            item_id,
            vault_id: self.ctx.vault_id,
            rev: 0,
            kind: spec.kind,
            schema_v: SCHEMA_V,
            class: spec.class,
            device_only: false,
            item_key: Some(wrapped_item_key),
            versions,
            trashed_at: (spec.state == ItemState::Trashed).then(Timestamp::now),
            state: spec.state,
        };
        self.items.insert(item_id, item);
        self.pending_blobs.extend(blobs);
        self.commit_head()?;
        Ok(item_id)
    }

    /// Saves a new version of a live or archived item with `fields` and
    /// `extra` files added beside the ones it has (a restore over an item
    /// that changed since the backup). Returns the version number.
    pub fn update_item_with_files(
        &mut self,
        item_id: Id,
        fields: Vec<PlainField>,
        extra: &[NewAttachment],
    ) -> Result<u64> {
        let item = self.item(item_id)?;
        if !matches!(item.state, ItemState::Live | ItemState::Archived) {
            return Err(Error::Invalid(
                "this item is in the trash; restore it before changing it".into(),
            ));
        }
        let item_key = self.item_key(item)?;
        let header = item.header();
        let latest = item.latest()?;
        let n = latest.n + 1;
        let parent_hash = latest.hash(&header)?;
        let mut attachments = latest.attachments.clone();
        let mut blobs = Vec::with_capacity(extra.len());
        for a in extra {
            let (r, blob) = self.seal_new_attachment(item_id, &item_key, a)?;
            blobs.push((r.blob, blob));
            attachments.push(r);
        }
        let version =
            self.make_version(&header, &item_key, n, parent_hash, &fields, attachments)?;
        self.items
            .get_mut(&item_id)
            .ok_or_else(|| Error::NotFound(format!("item {item_id}")))?
            .versions
            .push(version);
        self.pending_blobs.extend(blobs);
        self.commit_head()?;
        Ok(n)
    }

    /// Turns the per-item export opt-in on or off (D121) as a new version
    /// of the item, so it is signed, kept in history and synced. Only a
    /// crown jewel has anything to opt in; asking for a standard item is
    /// refused. Returns the version number, or the current one when the
    /// item already says so.
    pub fn set_export_opt_in(&mut self, item_id: Id, on: bool) -> Result<u64> {
        let item = self.item(item_id)?;
        if item.class != ItemClass::CrownJewel {
            return Err(Error::Invalid(
                "only a crown jewel is kept out of exports, so there is nothing to opt in".into(),
            ));
        }
        if !matches!(item.state, ItemState::Live | ItemState::Archived) {
            return Err(Error::Invalid(
                "this item is in the trash; restore it before changing it".into(),
            ));
        }
        let current = item.latest()?.n;
        let mut fields = self.open_item(item_id)?;
        if export_opted_in(&fields) == on {
            return Ok(current);
        }
        fields.retain(|f| f.field_id != EXPORT_OPT_IN);
        if on {
            fields.push(PlainField::new(
                EXPORT_OPT_IN,
                FieldKind::Boolean,
                "Included in exports and backups",
                FieldValue::Boolean(true),
            ));
        }
        self.update_item(item_id, fields)
    }

    /// Sets an item's state to archived or back to live.
    pub fn set_archived(&mut self, item_id: Id, archived: bool) -> Result<()> {
        let item = self
            .items
            .get_mut(&item_id)
            .ok_or_else(|| Error::NotFound(format!("item {item_id}")))?;
        item.state = match (item.state, archived) {
            (ItemState::Live | ItemState::Archived, true) => ItemState::Archived,
            (ItemState::Live | ItemState::Archived, false) => ItemState::Live,
            _ => {
                return Err(Error::Invalid(
                    "only a live or archived item can be archived or brought back".into(),
                ));
            }
        };
        self.commit_head()
    }

    fn seal_new_attachment(
        &self,
        item_id: Id,
        item_key: &ItemKey,
        a: &NewAttachment,
    ) -> Result<(AttachmentRef, Vec<u8>)> {
        let att_id = Id::new();
        let att_key = AttachmentKey::from_bytes(lockdown_crypto::rand::random_32()?);
        let wrapped = wrap_key(
            &att_key,
            item_key,
            &self.attachment_key_place(item_id, att_id),
        )?;
        let meta = AttachmentMeta {
            name: a.name.clone(),
            mime: a.mime.clone(),
            size: a.bytes.len() as u64,
        };
        let sealed = seal_attachment(
            &att_key,
            &self.attachment_place(item_id, att_id),
            &meta,
            &a.bytes,
        )?;
        Ok((
            AttachmentRef {
                att_id,
                att_key: Some(wrapped),
                chunks: sealed.chunks,
                blob: sealed.blob_hash,
                blob_size: sealed.blob.len() as u64,
                meta: sealed.meta,
            },
            sealed.blob,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::sample_vault;

    fn fields(pw: &str) -> Vec<PlainField> {
        vec![
            PlainField::text("title", "title", "Example"),
            PlainField::hidden("pw", "Password", pw),
        ]
    }

    #[test]
    fn an_item_lands_with_its_history_files_and_state_in_one_head() {
        let mut vault = sample_vault();
        let before = vault.seq();
        let keep = Id::new();
        let id = vault
            .import_item(ImportSpec {
                item_id: Some(keep),
                kind: ItemKind::Login,
                class: ItemClass::Standard,
                versions: vec![fields("one"), fields("two"), fields("three")],
                attachments: vec![NewAttachment {
                    name: "scan.pdf".into(),
                    mime: "application/pdf".into(),
                    bytes: Zeroizing::new(b"%PDF-1.7 hello".to_vec()),
                }],
                state: ItemState::Archived,
            })
            .unwrap();
        assert_eq!(id, keep, "the id is kept");
        assert_eq!(vault.seq(), before + 1, "one head for the whole item");
        assert_eq!(vault.open_version(id, 1).unwrap(), fields("one"));
        assert_eq!(vault.open_item(id).unwrap(), fields("three"));
        let item = vault.item(id).unwrap();
        assert_eq!(item.state, ItemState::Archived);
        assert_eq!(
            item.versions[1].parent_hash,
            item.versions[0].hash(&item.header()).unwrap()
        );
        assert!(item.versions[0].attachments.is_empty());
        let files = vault.attachments(id).unwrap();
        assert_eq!(files[0].meta.as_ref().unwrap().name, "scan.pdf");
        let blob = vault.pending_blob(files[0].blob).unwrap().to_vec();
        let (_, bytes) = vault.open_attachment(id, files[0].att_id, &blob).unwrap();
        assert_eq!(bytes.as_slice(), b"%PDF-1.7 hello");
        vault.verify(vault.seq()).unwrap();

        // The same id again is refused; so are no versions and a tombstone.
        let again = ImportSpec {
            item_id: Some(keep),
            kind: ItemKind::Login,
            class: ItemClass::Standard,
            versions: vec![fields("x")],
            attachments: vec![],
            state: ItemState::Live,
        };
        assert!(vault.import_item(again.clone()).is_err());
        let empty = ImportSpec {
            item_id: None,
            versions: vec![],
            ..again.clone()
        };
        assert!(vault.import_item(empty).is_err());
        let purged = ImportSpec {
            item_id: None,
            state: ItemState::Tombstone,
            ..again
        };
        assert!(vault.import_item(purged).is_err());
    }

    #[test]
    fn the_export_opt_in_is_a_signed_version_on_a_crown_jewel_only() {
        let mut vault = sample_vault();
        let wallet = vault
            .create_item(
                ItemKind::CryptoWallet,
                ItemClass::CrownJewel,
                vec![
                    PlainField::text("title", "title", "Cold"),
                    PlainField::hidden("seed", "Recovery phrase", "abandon ability"),
                ],
            )
            .unwrap();
        assert!(!export_opted_in(&vault.open_item(wallet).unwrap()));
        assert_eq!(vault.set_export_opt_in(wallet, true).unwrap(), 2);
        assert!(export_opted_in(&vault.open_item(wallet).unwrap()));
        assert_eq!(
            vault.set_export_opt_in(wallet, true).unwrap(),
            2,
            "already on: no new version"
        );
        assert_eq!(vault.set_export_opt_in(wallet, false).unwrap(), 3);
        assert!(!export_opted_in(&vault.open_item(wallet).unwrap()));
        assert!(
            export_opted_in(&vault.open_version(wallet, 2).unwrap()),
            "history keeps it"
        );
        let login = vault
            .create_item(ItemKind::Login, ItemClass::Standard, fields("p"))
            .unwrap();
        assert!(vault.set_export_opt_in(login, true).is_err());
        assert!(is_reserved_field(EXPORT_OPT_IN));
        assert!(!is_reserved_field("custom.0"));

        vault.set_archived(login, true).unwrap();
        assert_eq!(vault.item(login).unwrap().state, ItemState::Archived);
        vault.set_archived(login, false).unwrap();
        vault.trash_item(login).unwrap();
        assert!(vault.set_archived(login, true).is_err());
    }
}
