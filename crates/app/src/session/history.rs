//! An item's history (02-model §7): every saved version with who saved it
//! and which fields it changed, an earlier version shown masked, and
//! "Restore", which copies an earlier version's fields into a new version
//! (history is never rewritten). Attached files: add, list, open, remove.

use lockdown_vault::{Id, ItemState, PlainField, is_org_field};

use super::{Session, parse_id, shown_fields, state_name};
use crate::Revealed;
use crate::types::{ItemDetail, VersionView};

/// The largest file the app attaches in one go. The vault format takes up
/// to 1 GiB per file; the whole file is sealed in memory today, so the app
/// stops well short of that until attachments stream.
pub const MAX_ATTACH_BYTES: usize = 100 << 20;

/// The labels of the fields that differ between two versions (changed,
/// added or removed), title first, never a value.
fn changed(before: &[PlainField], after: &[PlainField]) -> Vec<String> {
    let mut out = Vec::new();
    let label = |f: &PlainField| {
        if f.field_id == "title" {
            "Title".to_string()
        } else {
            f.label.clone()
        }
    };
    for f in after
        .iter()
        .filter(|f| !is_org_field(&f.field_id) && !lockdown_vault::is_reserved(&f.field_id))
    {
        match before.iter().find(|b| b.field_id == f.field_id) {
            Some(b) if b == f => {}
            _ => out.push(label(f)),
        }
    }
    for b in before
        .iter()
        .filter(|b| !is_org_field(&b.field_id) && !lockdown_vault::is_reserved(&b.field_id))
    {
        if !after.iter().any(|f| f.field_id == b.field_id) {
            out.push(format!("{} (removed)", label(b)));
        }
    }
    let org = |fields: &[PlainField]| lockdown_vault::Organization::of(fields);
    if org(before) != org(after) {
        out.push("Folder, tags or favourite".into());
    }
    out
}

impl Session {
    /// Every version of an item, newest first.
    pub fn history(&self, id: &str) -> Result<Vec<VersionView>, String> {
        let item_id = parse_id(id)?;
        let vault = self.holding(item_id)?;
        let item = vault.item(item_id).map_err(|e| e.to_string())?;
        let latest = item.latest().map_err(|e| e.to_string())?.n;
        let name_of = |device: Id| {
            self.identity
                .devices()
                .find(|d| d.device_id == device)
                .map(|d| d.name.clone())
                .unwrap_or_else(|| format!("device {}", &device.to_string()[..8]))
        };
        let mut out = Vec::new();
        let mut previous: Option<Vec<PlainField>> = None;
        for v in &item.versions {
            let fields = if v.is_shredded() || item.item_key.is_none() {
                None
            } else {
                vault.open_version(item_id, v.n).ok()
            };
            let changes = match (&previous, &fields) {
                (Some(p), Some(f)) => changed(p, f),
                (None, Some(_)) if v.n == item.versions[0].n => vec!["Created".into()],
                _ => Vec::new(),
            };
            out.push(VersionView {
                n: v.n,
                edited_at: v.edited_at.0,
                edited_by: name_of(v.edited_by),
                changed: changes,
                readable: fields.is_some(),
                current: v.n == latest,
            });
            if fields.is_some() {
                previous = fields;
            }
        }
        out.reverse();
        Ok(out)
    }

    /// Version `n` of an item, masked as `show` masks the newest.
    pub fn show_version(&self, id: &str, n: u64) -> Result<ItemDetail, String> {
        let item_id = parse_id(id)?;
        let vault = self.holding(item_id)?;
        let item = vault.item(item_id).map_err(|e| e.to_string())?;
        let version = item.version(n).map_err(|e| e.to_string())?;
        let fields = vault.open_version(item_id, n).map_err(|e| e.to_string())?;
        Ok(ItemDetail {
            id: item_id.to_string(),
            kind: item.kind.as_str().to_string(),
            state: state_name(item.state).to_string(),
            version: n,
            edited_at: version.edited_at.0,
            crown_jewel: item.class == lockdown_vault::ItemClass::CrownJewel,
            export_opt_in: lockdown_vault::export_opted_in(&fields),
            fields: shown_fields(&fields, false),
            place: self.place_of(vault, item_id, &fields)?,
        })
    }

    /// Copies version `n`'s fields into a new version of a live or archived
    /// item; where it is filed now stays as it is. Returns the new number.
    pub fn restore_version(&mut self, id: &str, n: u64) -> Result<u64, String> {
        let item_id = parse_id(id)?;
        let vault = self.holding_mut(item_id)?;
        let item = vault.item(item_id).map_err(|e| e.to_string())?;
        if !matches!(item.state, ItemState::Live | ItemState::Archived) {
            return Err("this item is in the trash; restore it before going back a version".into());
        }
        if item.latest().map_err(|e| e.to_string())?.n == n {
            return Err("that is already the current version".into());
        }
        let mut old = vault.open_version(item_id, n).map_err(|e| e.to_string())?;
        let current = vault.open_item(item_id).map_err(|e| e.to_string())?;
        // Where it is filed, and Lockdown's own fields (the security
        // center's settings), stay as they are now.
        lockdown_vault::Organization::of(&current).apply(&mut old);
        old.retain(|f| !lockdown_vault::is_reserved(&f.field_id));
        old.extend(
            current
                .into_iter()
                .filter(|f| lockdown_vault::is_reserved(&f.field_id)),
        );
        let new = vault.update_item(item_id, old).map_err(|e| e.to_string())?;
        self.save()?;
        Ok(new)
    }

    // --- attachments ---------------------------------------------------------

    /// Attaches a file to a live or archived item; its name, type and size
    /// are sealed with it. Returns the attachment's id.
    pub fn attach(&mut self, id: &str, name: &str, mime: &str, bytes: &[u8]) -> Result<Id, String> {
        let item_id = parse_id(id)?;
        let name = name.trim();
        if name.is_empty() || name.contains(['/', '\\', '\0']) {
            return Err("the file needs a plain name".into());
        }
        if bytes.len() > MAX_ATTACH_BYTES {
            return Err(format!(
                "files up to {} MB attach here; this one is larger",
                MAX_ATTACH_BYTES >> 20
            ));
        }
        let mime = if mime.trim().is_empty() {
            "application/octet-stream"
        } else {
            mime.trim()
        };
        let vault = self.holding_mut(item_id)?;
        let item = vault.item(item_id).map_err(|e| e.to_string())?;
        if !matches!(item.state, ItemState::Live | ItemState::Archived) || item.kind.hidden() {
            return Err("files attach to a live item; restore it first".into());
        }
        let (att_id, _) = vault
            .attach(item_id, name, mime, bytes)
            .map_err(|e| e.to_string())?;
        self.save()?;
        Ok(att_id)
    }

    /// An attached file's bytes, for the person to save: the blob must
    /// hash to the name the signed version carries and every chunk must
    /// open in its place first.
    pub fn attachment_bytes(&self, id: &str, att_id: &str) -> Result<Revealed, String> {
        let item_id = parse_id(id)?;
        let att_id = Id::parse(att_id).map_err(|_| format!("'{att_id}' isn't an attachment id"))?;
        let vault = self.holding(item_id)?;
        let info = vault
            .attachments(item_id)
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|a| a.att_id == att_id)
            .ok_or("that file isn't attached to this item")?;
        let blob = match vault.pending_blob(info.blob) {
            Some(b) => b.to_vec(),
            None => lockdown_store::Store::get_blob(&self.store, info.blob)
                .map_err(|e| e.to_string())?,
        };
        let (_, bytes) = vault
            .open_attachment(item_id, att_id, &blob)
            .map_err(|e| e.to_string())?;
        Ok(Revealed::new(bytes.to_vec()))
    }

    /// Takes a file off an item (a new version; the history keeps it).
    pub fn detach(&mut self, id: &str, att_id: &str) -> Result<u64, String> {
        let item_id = parse_id(id)?;
        let att_id = Id::parse(att_id).map_err(|_| format!("'{att_id}' isn't an attachment id"))?;
        let n = self
            .holding_mut(item_id)?
            .remove_attachment(item_id, att_id)
            .map_err(|e| e.to_string())?;
        self.save()?;
        Ok(n)
    }
}
