//! What a vault does when it is shared (`crate::sharing`;
//! `docs/plan/18-sharing.md` §2): hand its Vault Key or one Item Key to
//! another persona, take a newer Vault Key generation, move every item to
//! fresh keys when a member leaves, and hold items another person shared
//! on their own.
//!
//! **Rotation re-keys, then writes one re-key version.** Every signature
//! and hash in an item leaves its key envelopes out (D36), so moving a
//! vault to a new Vault Key gives each item a fresh Item Key and re-wraps
//! its Version and Attachment Keys under it without touching a sealed
//! field or a signed byte. A change that touches no signed byte is one
//! sync can't see, though (`Vault::changed_since` compares the versions
//! the head signed), so the re-keyed item would never reach the server,
//! and everyone handed the fresh key would hold a key for a copy the
//! server doesn't serve (docs/plan/51-rekey-push.md). So each re-keyed
//! item also gets one new version: its newest fields sealed again under a
//! fresh Version Key wrapped by the fresh Item Key, signed by this device,
//! keeping the time the fields were saved. It syncs like any edit. A
//! removed member who kept the old keys can still read the versions they
//! could already read; the re-key version and every version after it are
//! under an Item Key they never held.

use super::*;
use crate::sharing::{Party, ShareEnvelope, SharePlace, SharePurpose};
use ecosystem_identity::kem::KemPublicKey;

/// The record a member's device keeps for a vault shared with it: the
/// owner's public context (whose ids every field is bound to), with the
/// member persona's own App Data Key id, and the Vault Key generation
/// `vault_key_id` wrapped under that key. [`Vault::load`] then opens the
/// shared vault exactly as it opens one of the persona's own.
pub fn member_record(
    ctx: VaultContext,
    vault_key_id: Id,
    vault_key: &VaultKey,
    app_data_key: &AppDataKey,
) -> Result<VaultRecord> {
    let place = KeyPlace {
        profile_id: ctx.profile_id,
        persona_id: ctx.persona_id,
        vault_id: ctx.vault_id,
        key_id: vault_key_id,
        purpose: KeyPurpose::Vault,
        wrapped_by: WrappedBy::new(WrapperKind::AppData, ctx.app_data_key_id),
        v: WRAP_V,
    };
    Ok(VaultRecord {
        v: VAULT_RECORD_V,
        context: ctx,
        vault_key: wrap_key(vault_key, app_data_key, &place)?,
    })
}

impl Vault {
    /// The id of the Vault Key generation in force (every new head's
    /// `vault_key_ref`).
    pub fn vault_key_id(&self) -> Id {
        self.vault_key_id
    }

    /// The Vault Key generation `id`: the current one or a kept earlier one.
    pub(super) fn vault_key_named(&self, id: Id) -> Result<&VaultKey> {
        if id == self.vault_key_id {
            return Ok(&self.vault_key);
        }
        self.old_vault_keys
            .iter()
            .find(|(k, _)| *k == id)
            .map(|(_, key)| key)
            .ok_or_else(|| {
                Error::NotFound(format!(
                    "Vault Key generation {id} of vault {}",
                    self.ctx.vault_id
                ))
            })
    }

    /// Whether any live item is a crown jewel: those hang from the owning
    /// persona's Crown-Jewel Key, which is never shared, so a vault holding
    /// one can't be shared until it moves out.
    pub fn has_crown_jewels(&self) -> bool {
        self.items
            .values()
            .any(|i| i.class == ItemClass::CrownJewel && i.state != ItemState::Tombstone)
    }

    /// Wraps the Vault Key generation in force to another persona and
    /// signs the envelope with this device.
    pub fn seal_vault_key(
        &self,
        to: Party,
        to_kem: &KemPublicKey,
        from_account: Id,
        purpose: SharePurpose,
        grant_id: Option<Id>,
    ) -> Result<ShareEnvelope> {
        if purpose == SharePurpose::Item {
            return Err(Error::Invalid("an item share wraps an Item Key".into()));
        }
        let place = SharePlace {
            purpose,
            vault_id: self.ctx.vault_id,
            item_id: None,
            key_id: self.vault_key_id,
            to,
            from_account,
            grant_id,
        };
        ShareEnvelope::seal(
            &self.vault_key,
            to_kem,
            place,
            self.device_id,
            self.signer.as_ref(),
        )
    }

    /// Wraps one item's Item Key to another persona (sharing that item on
    /// its own), signed by this device. Crown jewels are never shared.
    pub fn seal_item_key(
        &self,
        item_id: Id,
        to: Party,
        to_kem: &KemPublicKey,
        from_account: Id,
        share_id: Id,
    ) -> Result<ShareEnvelope> {
        let item = self.item(item_id)?;
        if item.class == ItemClass::CrownJewel {
            return Err(Error::Invalid(
                "a crown jewel is never shared; it stays under your own key".into(),
            ));
        }
        if !matches!(item.state, ItemState::Live | ItemState::Archived) {
            return Err(Error::Invalid("only a live item can be shared".into()));
        }
        let key = self.item_key(item)?;
        let place = SharePlace {
            purpose: SharePurpose::Item,
            vault_id: self.ctx.vault_id,
            item_id: Some(item_id),
            key_id: item_id,
            to,
            from_account,
            grant_id: Some(share_id),
        };
        ShareEnvelope::seal(&key, to_kem, place, self.device_id, self.signer.as_ref())
    }

    /// Takes a newer Vault Key generation (a member device opening the key
    /// a rotation handed it, or another device of the owner taking the one
    /// the rotating device wrote). The one it replaces is kept for items
    /// not yet moved; nothing is committed.
    pub fn adopt_vault_key(&mut self, key_id: Id, key: VaultKey) {
        if key_id == self.vault_key_id {
            return;
        }
        let old = std::mem::replace(&mut self.vault_key, key);
        let old_id = std::mem::replace(&mut self.vault_key_id, key_id);
        self.old_vault_keys.retain(|(id, _)| *id != key_id);
        self.old_vault_keys.push((old_id, old));
    }

    /// Moves the vault to a fresh Vault Key generation and every item to a
    /// fresh Item Key under it, then commits one head naming the new
    /// generation. Returns the new generation's id. Run when a member
    /// leaves (03-architecture §6: "removal rotates the vault key").
    pub fn rotate_vault_key(&mut self) -> Result<Id> {
        if self.has_crown_jewels() {
            return Err(Error::Invalid(
                "this vault holds a crown jewel, which can't live in a shared vault".into(),
            ));
        }
        let new_id = Id::new();
        let new_key = VaultKey::from_bytes(lockdown_crypto::rand::random_32()?);
        // Every item key is opened under the generation it is wrapped by
        // before the switch, so a half-rotated vault opens either way.
        let ids: Vec<Id> = self.items.keys().copied().collect();
        let mut opened = Vec::with_capacity(ids.len());
        for id in &ids {
            let item = &self.items[id];
            if item.item_key.is_none() || item.class == ItemClass::CrownJewel {
                continue;
            }
            opened.push((*id, self.item_key(item)?));
        }
        self.adopt_vault_key(new_id, new_key);
        for (id, old) in opened {
            self.move_item_to_fresh_key(id, &old)?;
            self.write_rekey_version(id)?;
        }
        self.commit_head()?;
        Ok(new_id)
    }

    /// Gives one item a fresh Item Key (its share to someone was withdrawn,
    /// or it is still under an earlier Vault Key generation), writes its
    /// re-key version and commits a head. Its earlier versions keep every
    /// sealed field and signature.
    pub fn rekey_item(&mut self, item_id: Id) -> Result<()> {
        let item = self.item(item_id)?;
        if item.class == ItemClass::CrownJewel {
            return Err(Error::Invalid("a crown jewel is never shared".into()));
        }
        let old = self.item_key(item)?;
        self.move_item_to_fresh_key(item_id, &old)?;
        self.write_rekey_version(item_id)?;
        self.commit_head()
    }

    /// Writes the re-key version for an item already moved to its Item Key
    /// on this device, and commits a head, when one is due: what repairs an
    /// item whose re-key never reached the server (an earlier build moved
    /// keys without a version; docs/plan/51-rekey-push.md §4). Answers
    /// whether a version was written.
    pub fn reseal_item(&mut self, item_id: Id) -> Result<bool> {
        let wrote = self.write_rekey_version(item_id)?;
        if wrote {
            self.commit_head()?;
        }
        Ok(wrote)
    }

    /// Signs again, as this device, each of `items` (the owner's items
    /// shared on their own) whose newest version none of the owner's own
    /// devices signed (`own` answers which are): a member's edit, or the
    /// re-key versions of a rotation a manager ran. A person holding such
    /// an item trusts only the owner's devices (docs/plan/18-sharing.md), so
    /// this follow version is what lets their copy follow the edit
    /// (docs/plan/54-share-follows-edits.md). It is a re-key version under
    /// the item's current Item Key: the newest fields and files, sealed
    /// again, with the newest version's `edited_at`. Only what this vault
    /// already took is signed, and taking a version means it verified
    /// under a writer's pinned key. An item not here, a crown jewel, or one
    /// with nothing left to read is passed over. Commits one head when
    /// anything was written, and answers the items written, in `items`'
    /// order.
    pub fn follow_edits(&mut self, items: &[Id], own: &dyn Fn(Id) -> bool) -> Result<Vec<Id>> {
        let mut out = Vec::new();
        for id in items {
            let Ok(item) = self.item(*id) else {
                continue;
            };
            if item.class != ItemClass::Standard || out.contains(id) {
                continue;
            }
            let Ok(latest) = item.latest() else {
                continue;
            };
            if own(latest.edited_by) {
                continue;
            }
            if self.write_rekey_version(*id)? {
                out.push(*id);
            }
        }
        if !out.is_empty() {
            self.commit_head()?;
        }
        Ok(out)
    }

    /// Writes the re-key version of every item an earlier build re-keyed on
    /// this device and never pushed (docs/plan/51-rekey-push.md §4): one
    /// the server serves (`served`, each with the revision it is served
    /// at) at the revision and newest version this device holds, under
    /// another Item Key envelope than this device's. Commits one head when
    /// anything was written. Answers the items re-sealed, and whether any
    /// of them had moved to another Vault Key generation.
    pub fn reseal_unpushed(&mut self, served: &[Item]) -> Result<(Vec<Id>, bool)> {
        let mut out = Vec::new();
        let mut rotated = false;
        for theirs in served {
            let Ok(ours) = self.item(theirs.item_id) else {
                continue;
            };
            let (Some(our_key), Some(their_key)) = (&ours.item_key, &theirs.item_key) else {
                continue;
            };
            let same_version = matches!(
                (ours.latest_hash(), theirs.latest_hash()),
                (Ok(a), Ok(b)) if a == b
            );
            if ours.rev != theirs.rev || !same_version || our_key == their_key {
                continue;
            }
            let moved_generation = our_key.wrapped_by.id != their_key.wrapped_by.id;
            if self.write_rekey_version(theirs.item_id)? {
                rotated |= moved_generation;
                out.push(theirs.item_id);
            }
        }
        if !out.is_empty() {
            self.commit_head()?;
        }
        Ok((out, rotated))
    }

    /// The re-key version: the newest version's fields and attachments,
    /// sealed again under a fresh Version Key wrapped by the item's current
    /// Item Key, signed by this device and stamped with the newest
    /// version's own `edited_at` (nothing was edited, so the list's order
    /// and "edited" time stay). Nothing is written for an item with nothing
    /// left to read (purged, its newest version shredded, or one this
    /// device can't open): no one can read it under any key. Not committed.
    fn write_rekey_version(&mut self, item_id: Id) -> Result<bool> {
        let item = self.item(item_id)?;
        if item.item_key.is_none() || item.state == ItemState::Tombstone {
            return Ok(false);
        }
        let latest = item.latest()?;
        if latest.is_shredded() {
            return Ok(false);
        }
        let Ok(fields) = self.open_envelope(item, latest.n) else {
            return Ok(false);
        };
        let item_key = self.item_key(item)?;
        let header = item.header();
        let n = latest.n + 1;
        let parent_hash = latest.hash(&header)?;
        let attachments = latest.attachments.clone();
        let edited_at = latest.edited_at;
        let version = self.make_version_at(
            &header,
            &item_key,
            n,
            parent_hash,
            &fields,
            attachments,
            edited_at,
        )?;
        self.items
            .get_mut(&item_id)
            .ok_or_else(|| Error::NotFound(format!("item {item_id}")))?
            .versions
            .push(version);
        Ok(true)
    }

    /// The items whose Item Key is still wrapped under an earlier Vault
    /// Key generation (one a merge brought back during a rotation); the
    /// caller moves them on with [`Self::rekey_item`].
    pub fn items_under_old_keys(&self) -> Vec<Id> {
        self.items
            .values()
            .filter(|i| i.class == ItemClass::Standard)
            .filter(|i| {
                i.item_key
                    .as_ref()
                    .is_some_and(|k| k.wrapped_by.id != self.vault_key_id)
            })
            .map(|i| i.item_id)
            .collect()
    }

    fn move_item_to_fresh_key(&mut self, item_id: Id, old: &ItemKey) -> Result<()> {
        let fresh = ItemKey::from_bytes(lockdown_crypto::rand::random_32()?);
        let item = self.item(item_id)?.clone();
        let mut versions = item.versions.clone();
        for v in &mut versions {
            if let Some(env) = &v.version_key {
                let place = self.version_key_place(item_id, env.key_id);
                let key: VersionKey = unwrap_key(env, old, &place)?;
                v.version_key = Some(wrap_key(&key, &fresh, &place)?);
            }
            for a in &mut v.attachments {
                if let Some(env) = &a.att_key {
                    let place = self.attachment_key_place(item_id, a.att_id);
                    let key: AttachmentKey = unwrap_key(env, old, &place)?;
                    a.att_key = Some(wrap_key(&key, &fresh, &place)?);
                }
            }
        }
        let wrapped = self.wrap_item_key(item_id, ItemClass::Standard, &fresh)?;
        let slot = self
            .items
            .get_mut(&item_id)
            .ok_or_else(|| Error::NotFound(format!("item {item_id}")))?;
        slot.versions = versions;
        slot.item_key = Some(wrapped);
        Ok(())
    }

    /// Replaces the members list sealed into every head committed from now
    /// on (the share activity a member's head carries, 02-model §8). The
    /// signed membership chain is the authority; this list mirrors it.
    pub fn set_members(&mut self, members: Vec<Member>) {
        self.members = members;
    }

    /// A vault held only on this device for items other people shared on
    /// their own: the owner's context (whose ids the fields are bound to),
    /// each item under the Item Key its share envelope carried, re-wrapped
    /// under a key that never leaves memory. Never saved or synced; every
    /// version still verifies against the owner's devices (`trusted`)
    /// before anything opens. An item whose newest version a trusted
    /// device didn't sign, or that its key doesn't open (a share sealed
    /// with a newer Item Key than the copy the server served, while a
    /// re-key is on its way; [`received_key_opens`]), is left out on its
    /// own: the rest are held, and it comes in once both match.
    pub fn received(
        ctx: VaultContext,
        device_id: Id,
        signer: Box<dyn Signer>,
        trusted: &BTreeMap<Id, VerifyingKey>,
        items: Vec<(Item, ItemKey)>,
    ) -> Result<Self> {
        let mut vault = Self::create(ctx, device_id, signer, None)?;
        for (id, key) in trusted {
            vault.trust_device(*id, key.clone());
        }
        for (mut item, key) in items {
            if item.vault_id != ctx.vault_id {
                return Err(Error::ManifestMismatch {
                    item_id: item.item_id,
                    why: "a shared item names another vault than its share".into(),
                });
            }
            if item.class != ItemClass::Standard {
                return Err(Error::Invalid("a crown jewel is never shared".into()));
            }
            if item.v > ITEM_V {
                return Err(Error::SchemaTooNew {
                    found: item.v,
                    supported: ITEM_V,
                });
            }
            let latest = item.latest()?;
            let Some(signer_key) = vault.key_for(latest.edited_by) else {
                continue;
            };
            if latest.verify(&item.header(), &signer_key).is_err()
                || !received_key_opens(&ctx, &item, &key)
            {
                continue;
            }
            item.item_key = Some(vault.wrap_item_key(item.item_id, ItemClass::Standard, &key)?);
            vault.items.insert(item.item_id, item);
        }
        vault.commit_head()?;
        Ok(vault)
    }
}

/// Whether `key` is the Item Key `item`'s newest version was sealed under,
/// in the vault `ctx` names: its Version Key unwraps under it. An item
/// shared on its own whose share carries a newer Item Key than the copy
/// the server served (or an older one) answers false, and the person's
/// device waits for a copy that matches rather than failing to open it.
/// A purged item or one whose newest version was shredded answers false.
pub fn received_key_opens(ctx: &VaultContext, item: &Item, key: &ItemKey) -> bool {
    let Ok(latest) = item.latest() else {
        return false;
    };
    let Some(env) = latest.version_key.as_ref() else {
        return false;
    };
    let place = KeyPlace {
        profile_id: ctx.profile_id,
        persona_id: ctx.persona_id,
        vault_id: ctx.vault_id,
        key_id: env.key_id,
        purpose: KeyPurpose::Version,
        wrapped_by: WrappedBy::new(WrapperKind::Item, item.item_id),
        v: WRAP_V,
    };
    unwrap_key::<VersionKey, _>(env, key, &place).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::FieldValue;
    use crate::test_support::{sample_context, sample_login_fields, sample_vault};
    use lockdown_crypto::SigningKey;

    fn pw(fields: &[PlainField]) -> String {
        match &fields.iter().find(|f| f.field_id == "pw").unwrap().value {
            FieldValue::Text(t) => t.clone(),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn rotation_moves_every_item_to_fresh_keys_and_keeps_history_readable() {
        let mut vault = sample_vault();
        let a = vault
            .create_item(ItemKind::Login, ItemClass::Standard, sample_login_fields())
            .unwrap();
        let mut second = sample_login_fields();
        second[3] = PlainField::hidden("pw", "password", "second");
        vault.update_item(a, second).unwrap();
        let (_, blob) = vault.attach(a, "a.txt", "text/plain", b"file").unwrap();
        let before_ik = vault.item(a).unwrap().item_key.clone().unwrap();
        let before_hash = vault.item(a).unwrap().latest_hash().unwrap();
        let before_n = vault.item(a).unwrap().latest().unwrap().n;
        let old_id = vault.vault_key_id();
        let seq_before = vault.seq();

        let new_id = vault.rotate_vault_key().unwrap();
        assert_ne!(new_id, old_id);
        assert_eq!(vault.head().unwrap().vault_key_ref, new_id);
        let item = vault.item(a).unwrap();
        assert_eq!(item.item_key.as_ref().unwrap().wrapped_by.id, new_id);
        assert_ne!(item.item_key.as_ref().unwrap(), &before_ik);
        // Earlier versions keep every signed byte; one re-key version on
        // top carries the same fields and files, and the time they were
        // saved, so sync sees the item changed and pushes it.
        let (was_at, was_files) = {
            let v = item.version(before_n).unwrap();
            (v.edited_at, v.attachments.len())
        };
        assert_eq!(
            item.version(before_n)
                .unwrap()
                .hash(&item.header())
                .unwrap(),
            before_hash
        );
        let rekey = item.latest().unwrap();
        assert_eq!(rekey.n, before_n + 1);
        assert_eq!(rekey.parent_hash, before_hash);
        assert_eq!(rekey.edited_at, was_at);
        assert_eq!(rekey.attachments.len(), was_files);
        assert_ne!(item.latest_hash().unwrap(), before_hash);
        assert!(vault.changed_since(seq_before).unwrap().contains(&a));
        assert_eq!(
            pw(&vault.open_version(a, 1).unwrap()),
            "correct horse battery staple"
        );
        assert_eq!(pw(&vault.open_version(a, 2).unwrap()), "second");
        assert_eq!(pw(&vault.open_item(a).unwrap()), "second");
        let blob_bytes = vault.pending_blob(blob).unwrap().to_vec();
        let att = vault.attachments(a).unwrap()[0].att_id;
        assert_eq!(
            &*vault.open_attachment(a, att, &blob_bytes).unwrap().1,
            b"file"
        );
        vault.verify(0).unwrap();
        assert!(vault.items_under_old_keys().is_empty());

        // The old Item Key no longer opens the new version keys.
        let old_vault_key = vault.vault_key_named(old_id).unwrap().clone();
        let mut ctx_place = vault.item_key_place(a, ItemClass::Standard);
        ctx_place.wrapped_by.id = old_id;
        let old_ik: ItemKey = unwrap_key(&before_ik, &old_vault_key, &ctx_place).unwrap();
        let env = vault.item(a).unwrap().versions[0]
            .version_key
            .clone()
            .unwrap();
        assert!(
            unwrap_key::<VersionKey, _>(&env, &old_ik, &vault.version_key_place(a, env.key_id))
                .is_err()
        );
        // A crown jewel stops a rotation.
        vault
            .create_item(ItemKind::CryptoWallet, ItemClass::CrownJewel, Vec::new())
            .unwrap();
        assert!(vault.rotate_vault_key().is_err());
    }

    #[test]
    fn a_member_record_opens_the_shared_vault_and_a_received_item_stands_alone() {
        let owner = sample_vault();
        let mut owner = owner;
        let a = owner
            .create_item(ItemKind::Login, ItemClass::Standard, sample_login_fields())
            .unwrap();
        // A member's record: the owner's context, the member's own key.
        let member_adk = AppDataKey::generate().unwrap();
        let mut ctx = *owner.context();
        ctx.app_data_key_id = Id::new();
        let record =
            member_record(ctx, owner.vault_key_id(), &owner.vault_key, &member_adk).unwrap();
        assert_eq!(record.context.profile_id, owner.context().profile_id);

        // One item received on its own opens with its Item Key alone.
        let ik = owner.item_key(owner.item(a).unwrap()).unwrap();
        let mut trusted = BTreeMap::new();
        trusted.insert(owner.device_id(), owner.verifying_key());
        let me = SigningKey::generate().unwrap();
        let received = Vault::received(
            *owner.context(),
            Id::new(),
            Box::new(me),
            &trusted,
            vec![(owner.item(a).unwrap().clone(), ik)],
        )
        .unwrap();
        assert_eq!(
            pw(&received.open_item(a).unwrap()),
            "correct horse battery staple"
        );
        // From a device nobody trusts: left out, on its own.
        let stranger = Vault::received(
            *owner.context(),
            Id::new(),
            Box::new(SigningKey::generate().unwrap()),
            &BTreeMap::new(),
            vec![(
                owner.item(a).unwrap().clone(),
                owner.item_key(owner.item(a).unwrap()).unwrap(),
            )],
        )
        .unwrap();
        assert!(stranger.item(a).is_err());
        // Another vault's item under this context: refused.
        let mut other = sample_vault();
        let b = other
            .create_item(ItemKind::Login, ItemClass::Standard, sample_login_fields())
            .unwrap();
        let other_ik = other.item_key(other.item(b).unwrap()).unwrap();
        assert!(
            Vault::received(
                sample_context(),
                Id::new(),
                Box::new(SigningKey::generate().unwrap()),
                &trusted,
                vec![(other.item(b).unwrap().clone(), other_ik)],
            )
            .is_err()
        );

        // Re-keying one item leaves the old Item Key useless for new versions.
        let copy_before = owner.item(a).unwrap().clone();
        let seq = owner.seq();
        owner.rekey_item(a).unwrap();
        let fresh = owner.item_key(owner.item(a).unwrap()).unwrap();
        let old = received.item_key(received.item(a).unwrap()).unwrap();
        assert_ne!(fresh.expose(), old.expose());
        assert_eq!(owner.changed_since(seq).unwrap(), vec![a]);
        let copy_after = owner.item(a).unwrap().clone();
        assert!(received_key_opens(owner.context(), &copy_after, &fresh));
        assert!(!received_key_opens(owner.context(), &copy_after, &old));
        assert!(!received_key_opens(owner.context(), &copy_before, &fresh));

        // A share carrying the fresh key beside the copy from before the
        // re-key: that item waits, and the rest of the owner's items are
        // held as ever.
        let b = owner
            .create_item(ItemKind::Login, ItemClass::Standard, sample_login_fields())
            .unwrap();
        let b_key = owner.item_key(owner.item(b).unwrap()).unwrap();
        let held = Vault::received(
            *owner.context(),
            Id::new(),
            Box::new(SigningKey::generate().unwrap()),
            &trusted,
            vec![
                (copy_before.clone(), fresh.clone()),
                (owner.item(b).unwrap().clone(), b_key),
            ],
        )
        .unwrap();
        assert!(held.item(a).is_err());
        assert!(held.open_item(b).is_ok());
        // The re-keyed copy arrives: it opens.
        let held = Vault::received(
            *owner.context(),
            Id::new(),
            Box::new(SigningKey::generate().unwrap()),
            &trusted,
            vec![(copy_after, fresh)],
        )
        .unwrap();
        assert_eq!(
            pw(&held.open_item(a).unwrap()),
            "correct horse battery staple"
        );
    }

    #[test]
    fn an_item_re_keyed_by_an_earlier_build_is_found_and_re_sealed() {
        let mut vault = sample_vault();
        let a = vault
            .create_item(ItemKind::Login, ItemClass::Standard, sample_login_fields())
            .unwrap();
        let b = vault
            .create_item(ItemKind::Login, ItemClass::Standard, sample_login_fields())
            .unwrap();
        let c = vault
            .create_item(ItemKind::Login, ItemClass::Standard, sample_login_fields())
            .unwrap();
        for (id, rev) in [(a, 4), (b, 5), (c, 6)] {
            vault.set_item_rev(id, rev).unwrap();
        }
        let seq = vault.commit().unwrap();
        // What the server serves: every item as pushed.
        let served: Vec<Item> = [a, b, c]
            .iter()
            .map(|id| vault.item(*id).unwrap().clone())
            .collect();
        // An earlier build: a moved key with no version, so nothing to push.
        let old = vault.item_key(vault.item(a).unwrap()).unwrap();
        vault.move_item_to_fresh_key(a, &old).unwrap();
        vault.commit_head().unwrap();
        assert!(vault.changed_since(seq).unwrap().is_empty());
        // An item edited here since (pushed as any edit) is left as it is.
        vault.update_item(c, sample_login_fields()).unwrap();
        let seq = vault.seq();

        let (found, rotated) = vault.reseal_unpushed(&served).unwrap();
        assert_eq!(found, vec![a]);
        assert!(!rotated, "the same Vault Key generation");
        assert_eq!(vault.seq(), seq + 1, "one head");
        assert_eq!(vault.item(a).unwrap().versions.len(), 2);
        assert!(vault.changed_since(seq).unwrap().contains(&a));
        assert_eq!(
            pw(&vault.open_item(a).unwrap()),
            "correct horse battery staple"
        );
        // Asked again with the same copies: the item's version moved on, so
        // it isn't the server's any more; nothing more is written.
        let seq = vault.seq();
        assert_eq!(vault.reseal_unpushed(&served).unwrap(), (Vec::new(), false));
        assert_eq!(vault.seq(), seq);

        // A rotation by an earlier build: another generation, every item.
        let mut vault = sample_vault();
        let d = vault
            .create_item(ItemKind::Login, ItemClass::Standard, sample_login_fields())
            .unwrap();
        vault.set_item_rev(d, 2).unwrap();
        vault.commit().unwrap();
        let served = vec![vault.item(d).unwrap().clone()];
        let old = vault.item_key(vault.item(d).unwrap()).unwrap();
        let new_id = Id::new();
        vault.adopt_vault_key(
            new_id,
            VaultKey::from_bytes(lockdown_crypto::rand::random_32().unwrap()),
        );
        vault.move_item_to_fresh_key(d, &old).unwrap();
        let (found, rotated) = vault.reseal_unpushed(&served).unwrap();
        assert_eq!(found, vec![d]);
        assert!(rotated);
    }

    #[test]
    fn follow_edits_signs_again_only_what_another_device_signed_last() {
        let mut vault = sample_vault();
        let me = vault.device_id();
        let a = vault
            .create_item(ItemKind::Login, ItemClass::Standard, sample_login_fields())
            .unwrap();
        let b = vault
            .create_item(ItemKind::Login, ItemClass::Standard, sample_login_fields())
            .unwrap();
        let mut second = sample_login_fields();
        second[3] = PlainField::hidden("pw", "password", "second");
        vault.update_item(b, second).unwrap();
        let was = vault.item(b).unwrap().latest().unwrap().clone();
        let seq = vault.seq();

        // Signed last by this device, an owner's: nothing to follow.
        assert!(vault.follow_edits(&[a, b], &|d| d == me).unwrap().is_empty());
        assert_eq!(vault.seq(), seq);

        // Signed last by a device that isn't the owner's (here, as if
        // this device weren't one): a follow version on top, the same
        // fields, files and time, signed by this device, in one head.
        let missing = Id::new();
        assert_eq!(
            vault.follow_edits(&[b, missing, b], &|_| false).unwrap(),
            vec![b]
        );
        assert_eq!(vault.seq(), seq + 1);
        let item = vault.item(b).unwrap();
        let follow = item.latest().unwrap();
        assert_eq!(follow.n, was.n + 1);
        assert_eq!(follow.edited_by, me);
        assert_eq!(follow.edited_at, was.edited_at);
        assert_eq!(follow.parent_hash, was.hash(&item.header()).unwrap());
        assert!(vault.changed_since(seq).unwrap().contains(&b));
        assert_eq!(pw(&vault.open_item(b).unwrap()), "second");
        // It opens for a person holding the item under its Item Key.
        let key = vault.item_key(item).unwrap();
        assert!(received_key_opens(vault.context(), item, &key));
        vault.verify(0).unwrap();

        // Once an owner's device signed it, asking again writes nothing.
        let seq = vault.seq();
        assert!(vault.follow_edits(&[b], &|d| d == me).unwrap().is_empty());
        assert_eq!(vault.seq(), seq);
        // Purged, or a crown jewel: passed over.
        vault.purge_item(a).unwrap();
        let jewel = vault
            .create_item(ItemKind::CryptoWallet, ItemClass::CrownJewel, Vec::new())
            .unwrap();
        let seq = vault.seq();
        assert!(vault.follow_edits(&[a, jewel], &|_| false).unwrap().is_empty());
        assert_eq!(vault.seq(), seq);
    }

    #[test]
    fn reseal_writes_a_version_only_when_something_can_still_be_read() {
        let mut vault = sample_vault();
        let a = vault
            .create_item(ItemKind::Login, ItemClass::Standard, sample_login_fields())
            .unwrap();
        let seq = vault.seq();
        assert!(vault.reseal_item(a).unwrap());
        assert_eq!(vault.seq(), seq + 1);
        assert_eq!(vault.item(a).unwrap().versions.len(), 2);
        assert_eq!(
            pw(&vault.open_item(a).unwrap()),
            "correct horse battery staple"
        );
        // Its newest version shredded: nothing to seal again, nothing written.
        vault.shred_version(a, 2).unwrap();
        let seq = vault.seq();
        assert!(!vault.reseal_item(a).unwrap());
        assert_eq!(vault.seq(), seq);
        // Purged: nothing either.
        let b = vault
            .create_item(ItemKind::Login, ItemClass::Standard, sample_login_fields())
            .unwrap();
        vault.purge_item(b).unwrap();
        assert!(!vault.reseal_item(b).unwrap());
    }
}
