//! Organization in the session: the persona's vaults (several, each its
//! own Vault Key under the persona's App Data Key), their names and folder
//! trees, filing and tagging items, favourites and this device's record of
//! uses, the archive, moving an item between vaults or into and out of the
//! crown-jewel class, and the trash's own purge.
//!
//! Every name here (a vault's, a folder's, a tag) is sealed like any other
//! item data (`lockdown_vault::organization`); this module only decides
//! which vault an operation lands in and keeps the device's memory of the
//! vaults' heads in step.

use std::collections::BTreeSet;

use lockdown_store::Store as _;
use lockdown_vault::identity::{
    DeviceTrust, OpenVault, device_keys, keys_for, open_vault_trusting,
};
use lockdown_vault::{
    CrownJewelDefault, FieldChange, FieldInput, Id, ItemClass, ItemKind, ItemState, Organization,
    PlainField, Vault, VaultContext, VaultInfo, template_for,
};

use super::{Session, now_secs, parse_id, text_of};
use crate::types::{
    AttachmentView, ExpiryReminder, FolderView, ItemPlace, OrganizationView, VaultView,
};
use crate::views;

/// What a first vault is called until the person names it.
pub const FIRST_VAULT_NAME: &str = "Personal";
/// What any other vault is called until the person names it.
pub const OTHER_VAULT_NAME: &str = "Vault";
/// The most vaults one persona may hold on this build (02-model §1 sets
/// three on the free tier; plans are not in this build, so this is the
/// ceiling that keeps a sync round bounded).
pub const MAX_VAULTS: usize = 20;

/// The vaults in the order they were made: by the place each one's info
/// names (none for the first), then its first head's time, then its id.
/// Every device orders the same vaults the same way.
pub(crate) fn order_vaults(all: &mut [OpenVault]) {
    all.sort_by_cached_key(|o| {
        (
            o.vault.info().ok().and_then(|i| i.order).unwrap_or(0),
            o.vault.heads().first().map(|h| h.signed_at.0).unwrap_or(0),
            o.vault.context().vault_id,
        )
    });
}

fn parse_vault(id: &str) -> Result<Id, String> {
    Id::parse(id).map_err(|_| format!("'{id}' isn't a vault id"))
}

fn parse_folder(id: &str) -> Result<Id, String> {
    Id::parse(id).map_err(|_| format!("'{id}' isn't a folder id"))
}

/// Whether an item of this kind with these fields is a crown jewel by its
/// template (a wallet, a recovery phrase): it can never leave the class.
pub(crate) fn crown_jewel_fixed(kind: ItemKind, fields: &[PlainField]) -> bool {
    let template = template_for(kind);
    template.crown_jewel_default() == CrownJewelDefault::Default
        || fields
            .iter()
            .any(|f| template.field(&f.field_id).is_some_and(|t| t.crown_jewel))
}

impl Session {
    // --- the vaults ---------------------------------------------------------

    fn vault_ids(&self) -> Vec<Id> {
        self.vaults().map(|v| v.context().vault_id).collect()
    }

    pub(crate) fn vault_by_id(&self, vault_id: Id) -> Result<&Vault, String> {
        self.vaults()
            .find(|v| v.context().vault_id == vault_id)
            .ok_or_else(|| format!("vault {vault_id} isn't one of this persona's"))
    }

    pub(crate) fn vault_by_id_mut(&mut self, vault_id: Id) -> Result<&mut Vault, String> {
        if self.opened.vault.context().vault_id == vault_id {
            return Ok(&mut self.opened.vault);
        }
        self.more
            .iter_mut()
            .map(|o| &mut o.vault)
            .find(|v| v.context().vault_id == vault_id)
            .ok_or_else(|| format!("vault {vault_id} isn't one of this persona's"))
    }

    /// Opens every vault of the persona the store holds that isn't open yet
    /// (one a sync just pulled), under the same checks as at unlock.
    pub(crate) fn open_new_vaults(&mut self) -> Result<(), String> {
        let persona_id = self.vault().context().persona_id;
        let have = self.vault_ids();
        let mut keys = self.synced.device_keys();
        for (id, key) in device_keys(&self.identity) {
            keys.entry(id).or_insert(key);
        }
        for vault_id in
            lockdown_vault::identity::persona_vaults(&self.identity, persona_id, &self.store)
                .map_err(|e| e.to_string())?
        {
            if have.contains(&vault_id) {
                continue;
            }
            let trust = DeviceTrust {
                keys: keys.clone(),
                revoked_after: self.synced.revoked_after(vault_id),
            };
            let opened = open_vault_trusting(
                &self.identity,
                vault_id,
                &self.store,
                Box::new(self.keystore.clone()),
                self.memory.last_accepted_seq(vault_id),
                &trust,
            )
            .map_err(|e| e.to_string())?;
            self.more.push(opened);
        }
        order_vaults(&mut self.more);
        self.remember()
    }

    /// Every vault of the persona with its name, folders and counts.
    pub fn vault_views(&self) -> Result<Vec<VaultView>, String> {
        let mut out = Vec::new();
        for (i, vault) in self.vaults().enumerate() {
            let info = vault.info().map_err(|e| e.to_string())?;
            let mut folders: Vec<FolderView> = info
                .folders
                .iter()
                .map(|(id, f)| FolderView {
                    id: id.to_string(),
                    name: f.name.clone(),
                    parent: f
                        .parent
                        .filter(|p| info.folders.contains_key(p))
                        .map(|p| p.to_string()),
                    path: info.path(*id).unwrap_or_else(|| f.name.clone()),
                })
                .collect();
            folders.sort_by_key(|f| f.path.to_lowercase());
            out.push(VaultView {
                id: vault.context().vault_id.to_string(),
                name: info.name.clone().unwrap_or_else(|| {
                    if i == 0 {
                        FIRST_VAULT_NAME.into()
                    } else {
                        OTHER_VAULT_NAME.into()
                    }
                }),
                primary: i == 0,
                items: vault
                    .items()
                    .filter(|it| {
                        !it.kind.hidden()
                            && matches!(it.state, ItemState::Live | ItemState::Archived)
                    })
                    .count(),
                trash_days: info.trash_days(),
                folders,
            });
        }
        Ok(out)
    }

    /// Makes another vault for the persona: a fresh Vault Key wrapped by
    /// the persona's App Data Key, its name sealed in its info item.
    pub fn create_vault(&mut self, name: &str) -> Result<Id, String> {
        let name = lockdown_vault::organization::normalize_name(name, "vault")
            .map_err(|e| e.to_string())?;
        if self.vaults().count() >= MAX_VAULTS {
            return Err(format!("a persona holds at most {MAX_VAULTS} vaults here"));
        }
        let first = self.opened.vault.context();
        let keys = keys_for(
            &self.identity,
            first.persona_id,
            first.app_data_key_id,
            first.crown_jewel_key_id,
        )
        .map_err(|e| e.to_string())?;
        let ctx = VaultContext {
            profile_id: first.profile_id,
            persona_id: first.persona_id,
            vault_id: Id::new(),
            app_data_key_id: first.app_data_key_id,
            crown_jewel_key_id: first.crown_jewel_key_id,
        };
        let mut vault = Vault::create(
            ctx,
            self.identity.device_id(),
            Box::new(self.keystore.clone()),
            Some(keys.crown_jewel_key.clone()),
        )
        .map_err(|e| e.to_string())?;
        for (id, key) in self.opened.vault.trusted_devices() {
            vault.trust_device(*id, key.clone());
        }
        let mut order = 0;
        for v in self.vaults() {
            order = order.max(v.info().map_err(|e| e.to_string())?.order.unwrap_or(0));
        }
        let info = VaultInfo {
            name: Some(name),
            order: Some(order + 1),
            ..VaultInfo::default()
        };
        vault.set_info(&info).map_err(|e| e.to_string())?;
        let vault_id = ctx.vault_id;
        self.more.push(OpenVault {
            vault,
            keys,
            created: true,
        });
        self.save()?;
        Ok(vault_id)
    }

    fn change_info(
        &mut self,
        vault_id: &str,
        change: impl FnOnce(&mut VaultInfo) -> Result<(), String>,
    ) -> Result<(), String> {
        let vault_id = parse_vault(vault_id)?;
        let vault = self.vault_by_id_mut(vault_id)?;
        let mut info = vault.info().map_err(|e| e.to_string())?;
        change(&mut info)?;
        vault.set_info(&info).map_err(|e| e.to_string())?;
        self.save()
    }

    /// Names a vault.
    pub fn rename_vault(&mut self, vault_id: &str, name: &str) -> Result<(), String> {
        let name = lockdown_vault::organization::normalize_name(name, "vault")
            .map_err(|e| e.to_string())?;
        self.change_info(vault_id, |info| {
            info.name = Some(name);
            Ok(())
        })
    }

    /// Sets how many days a trashed item of the vault stays (1–365).
    pub fn set_trash_days(&mut self, vault_id: &str, days: u32) -> Result<(), String> {
        let max = lockdown_vault::organization::MAX_TRASH_DAYS;
        if !(1..=max).contains(&days) {
            return Err(format!("the trash keeps items from 1 to {max} days"));
        }
        self.change_info(vault_id, |info| {
            info.trash_days = Some(days);
            Ok(())
        })
    }

    /// Adds a folder to a vault, at the top or inside `parent`.
    pub fn add_folder(
        &mut self,
        vault_id: &str,
        name: &str,
        parent: Option<&str>,
    ) -> Result<Id, String> {
        let parent = parent.map(parse_folder).transpose()?;
        let mut made = None;
        self.change_info(vault_id, |info| {
            made = Some(info.add_folder(name, parent).map_err(|e| e.to_string())?);
            Ok(())
        })?;
        made.ok_or_else(|| "the folder wasn't made".into())
    }

    /// Renames a folder or moves it inside another of the same vault.
    pub fn change_folder(
        &mut self,
        vault_id: &str,
        folder: &str,
        name: &str,
        parent: Option<&str>,
    ) -> Result<(), String> {
        let folder = parse_folder(folder)?;
        let parent = parent.map(parse_folder).transpose()?;
        self.change_info(vault_id, |info| {
            info.change_folder(folder, name, parent)
                .map_err(|e| e.to_string())
        })
    }

    /// Removes a folder: the folders and items inside it move up to its
    /// parent (or the top). Nothing is deleted. Returns how many items moved.
    pub fn remove_folder(&mut self, vault_id: &str, folder: &str) -> Result<usize, String> {
        let vault_uuid = parse_vault(vault_id)?;
        let folder = parse_folder(folder)?;
        let parent = self
            .vault_by_id(vault_uuid)?
            .info()
            .map_err(|e| e.to_string())?
            .folders
            .get(&folder)
            .ok_or("that folder isn't in this vault")?
            .parent;
        let filed: Vec<Id> = {
            let vault = self.vault_by_id(vault_uuid)?;
            let mut out = Vec::new();
            for item in vault.items() {
                if item.kind.hidden()
                    || !matches!(item.state, ItemState::Live | ItemState::Archived)
                {
                    continue;
                }
                if vault
                    .organization(item.item_id)
                    .map_err(|e| e.to_string())?
                    .folder
                    == Some(folder)
                {
                    out.push(item.item_id);
                }
            }
            out
        };
        let vault = self.vault_by_id_mut(vault_uuid)?;
        for id in &filed {
            let mut org = vault.organization(*id).map_err(|e| e.to_string())?;
            org.folder = parent;
            vault.organize(*id, &org).map_err(|e| e.to_string())?;
        }
        let mut info = vault.info().map_err(|e| e.to_string())?;
        info.remove_folder(folder).map_err(|e| e.to_string())?;
        vault.set_info(&info).map_err(|e| e.to_string())?;
        self.save()?;
        Ok(filed.len())
    }

    // --- items: filing, tags, favourites, the archive ------------------------

    /// Files, tags or favourites one item; what is `None` stays as it was.
    /// `folder: Some(None)` takes it out of any folder. Returns the version.
    pub fn organize(
        &mut self,
        id: &str,
        folder: Option<Option<&str>>,
        tags: Option<Vec<String>>,
        favourite: Option<bool>,
    ) -> Result<u64, String> {
        let item_id = parse_id(id)?;
        let folder = match folder {
            Some(Some(f)) => Some(Some(parse_folder(f)?)),
            Some(None) => Some(None),
            None => None,
        };
        let vault = self.holding_mut(item_id)?;
        let mut org = vault.organization(item_id).map_err(|e| e.to_string())?;
        if let Some(f) = folder {
            org.folder = f;
        }
        if let Some(t) = tags {
            org.tags = t;
        }
        if let Some(f) = favourite {
            org.favourite = f;
        }
        let n = vault.organize(item_id, &org).map_err(|e| e.to_string())?;
        self.save()?;
        Ok(n)
    }

    /// Renames a tag on every item of every vault, the tags nested under it
    /// too (`work` → `job` turns `work/clients` into `job/clients`); with
    /// `to: None`, takes it off instead. Returns how many items changed.
    pub fn rename_tag(&mut self, from: &str, to: Option<&str>) -> Result<usize, String> {
        let from = lockdown_vault::organization::normalize_tag(from).map_err(|e| e.to_string())?;
        let to = to
            .map(lockdown_vault::organization::normalize_tag)
            .transpose()
            .map_err(|e| e.to_string())?;
        let prefix = format!("{from}/");
        let mut changed = 0;
        for idx in 0..=self.more.len() {
            let vault = if idx == 0 {
                &mut self.opened.vault
            } else {
                &mut self.more[idx - 1].vault
            };
            let ids: Vec<Id> = vault
                .items()
                .filter(|i| {
                    !i.kind.hidden() && matches!(i.state, ItemState::Live | ItemState::Archived)
                })
                .map(|i| i.item_id)
                .collect();
            for id in ids {
                let mut org = vault.organization(id).map_err(|e| e.to_string())?;
                let mut hit = false;
                let mut tags = Vec::with_capacity(org.tags.len());
                for t in &org.tags {
                    let rest = if t.eq_ignore_ascii_case(&from) {
                        Some("")
                    } else if t.len() > prefix.len()
                        && t[..prefix.len()].eq_ignore_ascii_case(&prefix)
                    {
                        Some(&t[from.len()..])
                    } else {
                        None
                    };
                    match (rest, &to) {
                        (Some(rest), Some(to)) => {
                            hit = true;
                            tags.push(format!("{to}{rest}"));
                        }
                        (Some(_), None) => hit = true,
                        (None, _) => tags.push(t.clone()),
                    }
                }
                if hit {
                    org.tags = tags;
                    vault.organize(id, &org).map_err(|e| e.to_string())?;
                    changed += 1;
                }
            }
        }
        if changed > 0 {
            self.save()?;
        }
        Ok(changed)
    }

    /// Archives an item: out of the main list, search, the palette and the
    /// codes view; kept, synced and exported.
    pub fn archive(&mut self, id: &str) -> Result<(), String> {
        let item_id = parse_id(id)?;
        self.holding_mut(item_id)?
            .archive_item(item_id)
            .map_err(|e| e.to_string())?;
        self.save()
    }

    /// Brings an archived item back to the main list.
    pub fn unarchive(&mut self, id: &str) -> Result<(), String> {
        let item_id = parse_id(id)?;
        self.holding_mut(item_id)?
            .unarchive_item(item_id)
            .map_err(|e| e.to_string())?;
        self.save()
    }

    /// Notes a deliberate use of an item on this device (a copy, a reveal,
    /// its site opened), for the ranking. Never synced.
    pub fn note_use(&mut self, item_id: Id) -> Result<(), String> {
        self.holding(item_id)?;
        self.recency
            .note(item_id, now_secs(), &self.opened.keys.app_data_key)
    }

    /// Checks that every folder a change files the item in is one of
    /// `vault`'s folders (the core checks the value is an id).
    pub(crate) fn check_folder(&self, vault: Id, changes: &[FieldChange]) -> Result<(), String> {
        for c in changes {
            if c.field_id.as_deref() != Some(lockdown_vault::organization::ORG_FOLDER) {
                continue;
            }
            let lockdown_vault::Change::Set(FieldInput::Text(t)) = &c.change else {
                continue;
            };
            if t.trim().is_empty() {
                continue;
            }
            let folder = parse_folder(t.trim())?;
            if !self
                .vault_by_id(vault)?
                .info()
                .map_err(|e| e.to_string())?
                .folders
                .contains_key(&folder)
            {
                return Err("that folder isn't in this item's vault".into());
            }
        }
        Ok(())
    }

    // --- moves and the crown-jewel class -----------------------------------

    /// Seals an item again in `target` (another vault, or the same one) in
    /// `class`, then purges the old one. Its history, files and filing come
    /// along; a move to another vault drops the folder (folders are per
    /// vault). Returns the new id; this device's record of uses follows it.
    fn rehome(&mut self, item_id: Id, target: Id, class: ItemClass) -> Result<Id, String> {
        let source = self.holding(item_id)?.context().vault_id;
        let carried = {
            let store = &self.store;
            self.holding(item_id)?
                .carry_out(item_id, &mut |hash| {
                    store
                        .get_blob(hash)
                        .map_err(|e| lockdown_vault::Error::Invalid(e.to_string()))
                })
                .map_err(|e| e.to_string())?
        };
        let new_id = self
            .vault_by_id_mut(target)?
            .carry_in(&carried, class)
            .map_err(|e| e.to_string())?;
        drop(carried);
        self.holding_mut(item_id)?
            .purge_item(item_id)
            .map_err(|e| e.to_string())?;
        if source != target {
            let vault = self.vault_by_id_mut(target)?;
            let mut org = vault.organization(new_id).map_err(|e| e.to_string())?;
            if org.folder.is_some() {
                org.folder = None;
                vault.organize(new_id, &org).map_err(|e| e.to_string())?;
            }
        }
        self.recency
            .rename(item_id, new_id, &self.opened.keys.app_data_key)?;
        self.save()?;
        Ok(new_id)
    }

    /// Moves an item to another vault of the persona. Returns its new id.
    pub fn move_item(&mut self, id: &str, vault_id: &str) -> Result<Id, String> {
        let item_id = parse_id(id)?;
        let target = parse_vault(vault_id)?;
        self.vault_by_id(target)?;
        let vault = self.holding(item_id)?;
        if vault.context().vault_id == target {
            return Err("the item is already in that vault".into());
        }
        let class = vault.item(item_id).map_err(|e| e.to_string())?.class;
        self.rehome(item_id, target, class)
    }

    /// Puts an item into the crown-jewel class, or takes it out. An item
    /// that is a crown jewel by its kind or a field (a wallet, a recovery
    /// phrase) can't leave (02-model §3). Returns its new id.
    pub fn set_crown_jewel(&mut self, id: &str, on: bool) -> Result<Id, String> {
        let item_id = parse_id(id)?;
        let vault = self.holding(item_id)?;
        let item = vault.item(item_id).map_err(|e| e.to_string())?;
        let want = if on {
            ItemClass::CrownJewel
        } else {
            ItemClass::Standard
        };
        if item.class == want {
            return Err(if on {
                "it is already a crown jewel".into()
            } else {
                "it isn't a crown jewel".into()
            });
        }
        if !on {
            let fields = vault.open_item(item_id).map_err(|e| e.to_string())?;
            if crown_jewel_fixed(item.kind, &fields) {
                return Err(
                    "this kind of item is always a crown jewel, so it can't leave the class".into(),
                );
            }
        }
        let target = vault.context().vault_id;
        self.rehome(item_id, target, want)
    }

    // --- the trash ---------------------------------------------------------

    /// Purges a trashed item now ("Delete now" in Deleted): its keys are
    /// destroyed on this device and, at the next sync, everywhere.
    pub fn purge(&mut self, id: &str) -> Result<(), String> {
        let item_id = parse_id(id)?;
        let vault = self.holding_mut(item_id)?;
        if vault.item(item_id).map_err(|e| e.to_string())?.state != ItemState::Trashed {
            return Err("only an item in the trash can be deleted for good".into());
        }
        vault.purge_item(item_id).map_err(|e| e.to_string())?;
        self.recency
            .forget(&[item_id], &self.opened.keys.app_data_key)?;
        self.save()
    }

    /// The trashed items an open conflict holds (D58): a trashed item while
    /// a live conflict copy of it is waiting for the person to choose.
    fn held(vault: &Vault) -> Result<BTreeSet<Id>, String> {
        let mut titles = Vec::new();
        for item in vault.items() {
            if item.kind.hidden() || !matches!(item.state, ItemState::Live | ItemState::Trashed) {
                continue;
            }
            let title = text_of(
                &vault.open_item(item.item_id).map_err(|e| e.to_string())?,
                "title",
            )
            .unwrap_or_default();
            titles.push((item.item_id, item.kind, item.state, title));
        }
        let mut held = BTreeSet::new();
        for (id, kind, state, title) in &titles {
            if *state != ItemState::Trashed {
                continue;
            }
            let open_copy = titles.iter().any(|(other, k, s, t)| {
                other != id
                    && k == kind
                    && *s == ItemState::Live
                    && views::is_conflict_copy(t)
                    && views::base_title(t) == views::base_title(title)
            });
            if open_copy {
                held.insert(*id);
            }
        }
        Ok(held)
    }

    /// Purges, in every vault, the trashed items whose grace period (the
    /// vault's setting, thirty days unless changed) has run out, except
    /// those an open conflict holds. Run at unlock and after every sync, so
    /// the purge happens on whichever device gets there first and reaches
    /// the others as a tombstone. Returns how many were purged.
    pub fn sweep_trash(&mut self) -> Result<usize, String> {
        let now = now_secs();
        let mut purged = Vec::new();
        for idx in 0..=self.more.len() {
            let vault = if idx == 0 {
                &mut self.opened.vault
            } else {
                &mut self.more[idx - 1].vault
            };
            if !vault.items().any(|i| i.state == ItemState::Trashed) {
                continue;
            }
            let grace = i64::from(vault.info().map_err(|e| e.to_string())?.trash_days()) * 86_400;
            let hold = Self::held(vault)?;
            purged.extend(
                vault
                    .purge_due(now, grace, &hold)
                    .map_err(|e| e.to_string())?,
            );
        }
        if !purged.is_empty() {
            self.recency
                .forget(&purged, &self.opened.keys.app_data_key)?;
            self.save()?;
        }
        Ok(purged.len())
    }

    /// Settles a conflict (D58) by keeping `keep` and trashing `other`
    /// (readable in Deleted through its grace period). When the copy kept
    /// is the conflict copy, it takes the item's own title back, so it is
    /// no longer marked and the trashed original can purge in time.
    pub fn resolve_conflict(&mut self, keep: &str, other: &str) -> Result<(), String> {
        let keep_id = parse_id(keep)?;
        let other_id = parse_id(other)?;
        if keep_id == other_id {
            return Err("keep one copy and trash the other".into());
        }
        let title = text_of(
            &self
                .holding(keep_id)?
                .open_item(keep_id)
                .map_err(|e| e.to_string())?,
            "title",
        )
        .unwrap_or_default();
        self.holding_mut(other_id)?
            .trash_item(other_id)
            .map_err(|e| e.to_string())?;
        if views::is_conflict_copy(&title) {
            let base = views::base_title(&title).to_string();
            let vault = self.holding_mut(keep_id)?;
            let mut fields = vault.open_item(keep_id).map_err(|e| e.to_string())?;
            if let Some(t) = fields.iter_mut().find(|f| f.field_id == "title") {
                t.value = lockdown_vault::FieldValue::Text(base);
            }
            vault
                .update_item(keep_id, fields)
                .map_err(|e| e.to_string())?;
        }
        self.save()
    }

    // --- what show and the reminders read ----------------------------------

    /// Where an item is filed and what is attached to it.
    pub(crate) fn place_of(
        &self,
        vault: &Vault,
        item_id: Id,
        fields: &[PlainField],
    ) -> Result<ItemPlace, String> {
        let org = Organization::of(fields);
        let item = vault.item(item_id).map_err(|e| e.to_string())?;
        let attachments = if item.is_shredded() {
            Vec::new()
        } else {
            vault
                .attachments(item_id)
                .map_err(|e| e.to_string())?
                .into_iter()
                .filter_map(|a| {
                    a.meta.map(|m| AttachmentView {
                        id: a.att_id.to_string(),
                        name: m.name.clone(),
                        mime: m.mime.clone(),
                        size: m.size,
                    })
                })
                .collect()
        };
        Ok(ItemPlace {
            vault_id: vault.context().vault_id.to_string(),
            organization: OrganizationView {
                folder: org.folder.map(|f| f.to_string()),
                tags: org.tags,
                favourite: org.favourite,
            },
            attachments,
            crown_jewel_fixed: item.class == ItemClass::CrownJewel
                && crown_jewel_fixed(item.kind, fields),
        })
    }

    /// The items due to lapse within their kind's reminder lead (or
    /// already lapsed), soonest first: what a shell reminds about.
    pub fn expiry_reminders(&self) -> Result<Vec<ExpiryReminder>, String> {
        let mut out: Vec<ExpiryReminder> = self
            .list()?
            .into_iter()
            .filter(|r| r.views.iter().any(|v| v == "expiring"))
            .filter_map(|r| {
                Some(ExpiryReminder {
                    days: r.expires_in_days?,
                    date: r.expires_at.clone()?,
                    id: r.id,
                    title: r.title,
                    kind: r.kind,
                })
            })
            .collect();
        out.sort_by(|a, b| a.days.cmp(&b.days).then(a.title.cmp(&b.title)));
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lockdown_vault::test_support::sample_vault;

    fn titled(vault: &mut Vault, title: &str) -> Id {
        vault
            .create_item(
                ItemKind::Login,
                ItemClass::Standard,
                vec![PlainField::text("title", "title", title)],
            )
            .unwrap()
    }

    #[test]
    fn an_open_conflict_holds_the_trashed_half_from_the_purge() {
        let mut vault = sample_vault();
        let original = titled(&mut vault, "Bank");
        let copy = titled(&mut vault, "Bank (conflict copy from laptop, 2026-10-17)");
        let other = titled(&mut vault, "Shop");
        vault.trash_item(original).unwrap();
        vault.trash_item(other).unwrap();
        let held = Session::held(&vault).unwrap();
        assert_eq!(held, BTreeSet::from([original]));
        // Once the copy is settled (trashed too, or renamed), nothing holds.
        vault.trash_item(copy).unwrap();
        assert!(Session::held(&vault).unwrap().is_empty());
    }
}
