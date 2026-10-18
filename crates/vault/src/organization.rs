//! Organization: folders, tags and favourites on items, and the vault's
//! own facts (its name, its folder tree, its trash setting) in a hidden
//! item of kind [`ItemKind::VaultInfo`] (02-model §1, §4, §7, §10).
//!
//! Nothing here adds plain-text metadata or a key layer. An item's folder,
//! tags and favourite are three sealed fields on the item ([`ORG_FOLDER`],
//! [`ORG_TAGS`], [`ORG_FAVOURITE`]), written on every item this build makes
//! or changes, empty when unset, so their ids (which are in the clear, as
//! every field id is) say only that a recent build saved the item. The
//! vault's name and folder names live in fields of its info item, sealed
//! under that item's key: a server sees one more item and its size, never a
//! name. Both ride the ordinary sync path, so two devices that organize at
//! once merge field by field (D58) like any other edit.
//!
//! An info item is made only when something is set (a name, a folder, the
//! trash setting), so a vault from an older build has none and reads as
//! [`VaultInfo::default`]. Two devices that each make one before they sync
//! end up with two; [`Vault::info`] reads them as one (the oldest wins a
//! name, folders are the union) and the next [`Vault::set_info`] folds the
//! extra ones into the oldest and purges them.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::field::{FieldKind, FieldValue, PlainField};
use crate::item::{ItemClass, ItemKind, ItemState};
use crate::vault::Vault;
use crate::{Error, Id, Result};

/// The field holding an item's folder: the folder's id, or empty.
pub const ORG_FOLDER: &str = "org.folder";
/// The field holding an item's tags: one per line, sorted, or empty.
pub const ORG_TAGS: &str = "org.tags";
/// The field saying whether the item is a favourite.
pub const ORG_FAVOURITE: &str = "org.favourite";

/// Whether a field id is one of the three organization fields.
pub fn is_org_field(field_id: &str) -> bool {
    matches!(field_id, ORG_FOLDER | ORG_TAGS | ORG_FAVOURITE)
}

/// The longest tag, in characters.
pub const MAX_TAG_CHARS: usize = 64;
/// The most tags one item may carry.
pub const MAX_TAGS: usize = 32;
/// The longest vault or folder name, in characters.
pub const MAX_NAME_CHARS: usize = 64;
/// The most folders one vault may hold.
pub const MAX_FOLDERS: usize = 500;
/// The trash's grace period when the vault sets none (02-model §7).
pub const DEFAULT_TRASH_DAYS: u32 = 30;
/// The longest grace period a vault may set.
pub const MAX_TRASH_DAYS: u32 = 365;

fn invalid(why: impl Into<String>) -> Error {
    Error::Invalid(why.into())
}

/// One tag as stored: trimmed, inner spaces kept, each `/`-separated part
/// trimmed and non-empty (`work / clients` → `work/clients`).
pub fn normalize_tag(tag: &str) -> Result<String> {
    let parts: Vec<&str> = tag.split('/').map(str::trim).collect();
    if parts.iter().any(|p| p.is_empty()) {
        return Err(invalid(format!(
            "'{}' isn't a tag: each part between slashes needs a word",
            tag.trim()
        )));
    }
    let out = parts.join("/");
    if out.chars().count() > MAX_TAG_CHARS {
        return Err(invalid(format!(
            "a tag is at most {MAX_TAG_CHARS} characters"
        )));
    }
    if out.chars().any(char::is_control) {
        return Err(invalid("a tag is one line of text"));
    }
    Ok(out)
}

/// Tags as stored: each normalized, duplicates (ignoring case) dropped,
/// sorted case-insensitively.
pub fn normalize_tags<S: AsRef<str>>(tags: &[S]) -> Result<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for t in tags {
        if t.as_ref().trim().is_empty() {
            continue;
        }
        let t = normalize_tag(t.as_ref())?;
        if !out.iter().any(|o| o.eq_ignore_ascii_case(&t)) {
            out.push(t);
        }
    }
    if out.len() > MAX_TAGS {
        return Err(invalid(format!("an item takes at most {MAX_TAGS} tags")));
    }
    out.sort_by_key(|t| t.to_lowercase());
    Ok(out)
}

/// A name for a vault or a folder: one trimmed line, not empty.
pub fn normalize_name(name: &str, what: &str) -> Result<String> {
    let n = name.trim();
    if n.is_empty() {
        return Err(invalid(format!("give the {what} a name")));
    }
    if n.contains('\n') || n.chars().any(char::is_control) {
        return Err(invalid(format!("a {what}'s name is one line")));
    }
    if n.chars().count() > MAX_NAME_CHARS {
        return Err(invalid(format!(
            "a {what}'s name is at most {MAX_NAME_CHARS} characters"
        )));
    }
    Ok(n.to_string())
}

/// An item's folder, tags and favourite.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct Organization {
    /// The folder it is filed in, in its own vault.
    pub folder: Option<Id>,
    /// Its tags, normalized ([`normalize_tags`]).
    pub tags: Vec<String>,
    /// Shown first and ranked first.
    pub favourite: bool,
}

impl Organization {
    /// Read from an item's fields; missing fields (an older item) read as
    /// unset, and a folder id that doesn't parse reads as no folder.
    pub fn of(fields: &[PlainField]) -> Self {
        let text = |id: &str| {
            fields
                .iter()
                .find(|f| f.field_id == id)
                .and_then(|f| match &f.value {
                    FieldValue::Text(t) => Some(t.clone()),
                    _ => None,
                })
        };
        let folder = text(ORG_FOLDER)
            .filter(|t| !t.is_empty())
            .and_then(|t| Id::parse(&t).ok());
        let tags = text(ORG_TAGS)
            .map(|t| {
                t.lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let favourite = fields
            .iter()
            .find(|f| f.field_id == ORG_FAVOURITE)
            .is_some_and(|f| matches!(f.value, FieldValue::Boolean(true)));
        Self {
            folder,
            tags,
            favourite,
        }
    }

    /// The three fields, in order, as stored.
    pub fn fields(&self) -> Vec<PlainField> {
        let mut folder = PlainField::text(
            ORG_FOLDER,
            "Folder",
            &self.folder.map(|f| f.to_string()).unwrap_or_default(),
        );
        folder.section = Some("Organization".into());
        let mut tags = PlainField::text(ORG_TAGS, "Tags", &self.tags.join("\n"));
        tags.section = Some("Organization".into());
        let mut favourite = PlainField::new(
            ORG_FAVOURITE,
            FieldKind::Boolean,
            "Favourite",
            FieldValue::Boolean(self.favourite),
        );
        favourite.section = Some("Organization".into());
        vec![folder, tags, favourite]
    }

    /// Replaces (or adds, at the end) the three fields in `fields`.
    pub fn apply(&self, fields: &mut Vec<PlainField>) {
        fields.retain(|f| !is_org_field(&f.field_id));
        fields.extend(self.fields());
    }
}

/// Adds the organization fields an item lacks, empty, so every item this
/// build saves carries all three.
pub fn ensure_org_fields(fields: &mut Vec<PlainField>) {
    for f in Organization::default().fields() {
        if !fields.iter().any(|p| p.field_id == f.field_id) {
            fields.push(f);
        }
    }
}

/// One folder of a vault.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Folder {
    /// Its name.
    pub name: String,
    /// The folder it sits in, or `None` at the top.
    pub parent: Option<Id>,
}

/// How a folder is sealed in the info item's field `folder.<id>`.
#[derive(Serialize, Deserialize)]
struct FolderRecord {
    v: u16,
    name: String,
    parent: Option<Id>,
}

const FOLDER_RECORD_V: u16 = 1;
const INFO_NAME: &str = "vault.name";
const INFO_TRASH_DAYS: &str = "vault.trash_days";
const INFO_ORDER: &str = "vault.order";
const FOLDER_PREFIX: &str = "folder.";

/// A vault's own facts.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct VaultInfo {
    /// The name the person gave it; `None` until one is given.
    pub name: Option<String>,
    /// The trash's grace period in days; `None` is the default
    /// ([`DEFAULT_TRASH_DAYS`]).
    pub trash_days: Option<u32>,
    /// Every folder, by id.
    pub folders: BTreeMap<Id, Folder>,
    /// Where the vault sits among the persona's vaults: zero (or none) for
    /// the first, then one more for each made after it, so every device
    /// lists them in the order they were made.
    pub order: Option<u32>,
}

impl VaultInfo {
    /// The grace period in force.
    pub fn trash_days(&self) -> u32 {
        self.trash_days
            .unwrap_or(DEFAULT_TRASH_DAYS)
            .clamp(1, MAX_TRASH_DAYS)
    }

    /// A folder's path from the top, names joined by ` / ` (a folder whose
    /// parent is gone sits at the top).
    pub fn path(&self, folder: Id) -> Option<String> {
        let mut names = Vec::new();
        let mut at = Some(folder);
        let mut seen = BTreeSet::new();
        while let Some(id) = at {
            if !seen.insert(id) {
                break;
            }
            let f = self.folders.get(&id)?;
            names.push(f.name.clone());
            at = f.parent.filter(|p| self.folders.contains_key(p));
        }
        names.reverse();
        Some(names.join(" / "))
    }

    /// Whether `folder` is `ancestor` or sits somewhere under it.
    pub fn is_within(&self, folder: Id, ancestor: Id) -> bool {
        let mut at = Some(folder);
        let mut seen = BTreeSet::new();
        while let Some(id) = at {
            if id == ancestor {
                return true;
            }
            if !seen.insert(id) {
                return false;
            }
            at = self.folders.get(&id).and_then(|f| f.parent);
        }
        false
    }

    /// Adds a folder under `parent` and returns its id.
    pub fn add_folder(&mut self, name: &str, parent: Option<Id>) -> Result<Id> {
        let name = normalize_name(name, "folder")?;
        if let Some(p) = parent
            && !self.folders.contains_key(&p)
        {
            return Err(invalid("that parent folder isn't in this vault"));
        }
        if self.folders.len() >= MAX_FOLDERS {
            return Err(invalid(format!(
                "a vault holds at most {MAX_FOLDERS} folders"
            )));
        }
        if self.sibling_named(&name, parent, None) {
            return Err(invalid(format!("there is already a folder '{name}' here")));
        }
        let id = Id::new();
        self.folders.insert(id, Folder { name, parent });
        Ok(id)
    }

    /// Renames a folder and moves it under `parent` (never under itself).
    pub fn change_folder(&mut self, folder: Id, name: &str, parent: Option<Id>) -> Result<()> {
        let name = normalize_name(name, "folder")?;
        if !self.folders.contains_key(&folder) {
            return Err(invalid("that folder isn't in this vault"));
        }
        if let Some(p) = parent {
            if !self.folders.contains_key(&p) {
                return Err(invalid("that parent folder isn't in this vault"));
            }
            if self.is_within(p, folder) {
                return Err(invalid("a folder can't go inside itself"));
            }
        }
        if self.sibling_named(&name, parent, Some(folder)) {
            return Err(invalid(format!("there is already a folder '{name}' here")));
        }
        self.folders.insert(folder, Folder { name, parent });
        Ok(())
    }

    /// Removes a folder; the folders inside it move up to its parent.
    /// Items filed in it read as filed nowhere (the caller re-files them).
    pub fn remove_folder(&mut self, folder: Id) -> Result<()> {
        let gone = self
            .folders
            .remove(&folder)
            .ok_or_else(|| invalid("that folder isn't in this vault"))?;
        for f in self.folders.values_mut() {
            if f.parent == Some(folder) {
                f.parent = gone.parent;
            }
        }
        Ok(())
    }

    fn sibling_named(&self, name: &str, parent: Option<Id>, except: Option<Id>) -> bool {
        self.folders.iter().any(|(id, f)| {
            Some(*id) != except && f.parent == parent && f.name.eq_ignore_ascii_case(name)
        })
    }

    fn from_fields(fields: &[PlainField]) -> Self {
        let mut info = Self::default();
        for f in fields {
            match (f.field_id.as_str(), &f.value) {
                (INFO_NAME, FieldValue::Text(t)) if !t.is_empty() => info.name = Some(t.clone()),
                (INFO_TRASH_DAYS, FieldValue::Text(t)) => info.trash_days = t.parse().ok(),
                (INFO_ORDER, FieldValue::Text(t)) => info.order = t.parse().ok(),
                (id, FieldValue::Bytes(b)) if id.starts_with(FOLDER_PREFIX) => {
                    let Ok(folder_id) = Id::parse(&id[FOLDER_PREFIX.len()..]) else {
                        continue;
                    };
                    let Ok(rec) = lockdown_crypto::cbor::from_slice::<FolderRecord>(b) else {
                        continue;
                    };
                    if rec.v > FOLDER_RECORD_V {
                        continue;
                    }
                    info.folders.insert(
                        folder_id,
                        Folder {
                            name: rec.name,
                            parent: rec.parent,
                        },
                    );
                }
                _ => {}
            }
        }
        info
    }

    fn to_fields(&self) -> Result<Vec<PlainField>> {
        let mut out = vec![PlainField::text("title", "title", "Vault details")];
        if let Some(n) = &self.name {
            out.push(PlainField::text(INFO_NAME, "Name", n));
        }
        if let Some(d) = self.trash_days {
            out.push(PlainField::text(
                INFO_TRASH_DAYS,
                "Trash days",
                &d.clamp(1, MAX_TRASH_DAYS).to_string(),
            ));
        }
        if let Some(o) = self.order {
            out.push(PlainField::text(INFO_ORDER, "Order", &o.to_string()));
        }
        for (id, f) in &self.folders {
            let rec = FolderRecord {
                v: FOLDER_RECORD_V,
                name: f.name.clone(),
                parent: f.parent,
            };
            out.push(PlainField::new(
                format!("{FOLDER_PREFIX}{id}"),
                FieldKind::Text,
                "Folder",
                FieldValue::Bytes(lockdown_crypto::cbor::to_canonical(&rec)?),
            ));
        }
        Ok(out)
    }
}

impl Vault {
    /// The live info items, oldest first (by their first version's time,
    /// then id).
    fn info_items(&self) -> Result<Vec<Id>> {
        let mut found = Vec::new();
        for item in self.items() {
            if item.kind == ItemKind::VaultInfo && item.state == ItemState::Live {
                let first = item.versions.first().map(|v| v.edited_at.0).unwrap_or(0);
                found.push((first, item.item_id));
            }
        }
        found.sort();
        Ok(found.into_iter().map(|(_, id)| id).collect())
    }

    /// The vault's own facts, read from its info items (none: the default).
    pub fn info(&self) -> Result<VaultInfo> {
        let mut out = VaultInfo::default();
        for (i, id) in self.info_items()?.into_iter().enumerate() {
            let info = VaultInfo::from_fields(&self.open_item(id)?);
            if i == 0 {
                out = info;
                continue;
            }
            out.name = out.name.or(info.name);
            out.trash_days = out.trash_days.or(info.trash_days);
            out.order = out.order.or(info.order);
            for (fid, f) in info.folders {
                out.folders.entry(fid).or_insert(f);
            }
        }
        Ok(out)
    }

    /// Writes the vault's facts: into the oldest info item (made when there
    /// is none), purging any other one after its folders were folded in by
    /// the caller's [`Self::info`]. Nothing is written when nothing changed.
    pub fn set_info(&mut self, info: &VaultInfo) -> Result<()> {
        let ids = self.info_items()?;
        let fields = info.to_fields()?;
        match ids.first() {
            None => {
                self.create_item(ItemKind::VaultInfo, ItemClass::Standard, fields)?;
            }
            Some(first) => {
                if VaultInfo::from_fields(&self.open_item(*first)?) != *info {
                    self.update_item(*first, fields)?;
                }
            }
        }
        for extra in ids.iter().skip(1) {
            self.purge_item(*extra)?;
        }
        Ok(())
    }

    /// An item's folder, tags and favourite.
    pub fn organization(&self, item_id: Id) -> Result<Organization> {
        Ok(Organization::of(&self.open_item(item_id)?))
    }

    /// Files, tags or favourites an item: a new version with the three
    /// organization fields replaced and every other field kept. The folder
    /// must be one of this vault's; the tags are normalized. Answers the new
    /// version's number, or the current one when nothing changed.
    pub fn organize(&mut self, item_id: Id, org: &Organization) -> Result<u64> {
        let item = self.item(item_id)?;
        if item.kind.hidden() {
            return Err(invalid("that item can't be organized"));
        }
        if !matches!(item.state, ItemState::Live | ItemState::Archived) {
            return Err(invalid(
                "this item is in the trash; restore it before filing it",
            ));
        }
        let current_n = item.latest()?.n;
        if let Some(folder) = org.folder
            && !self.info()?.folders.contains_key(&folder)
        {
            return Err(invalid("that folder isn't in this item's vault"));
        }
        let org = Organization {
            folder: org.folder,
            tags: normalize_tags(&org.tags)?,
            favourite: org.favourite,
        };
        let mut fields = self.open_item(item_id)?;
        let before = Organization::of(&fields);
        let complete = fields.iter().filter(|f| is_org_field(&f.field_id)).count() == 3;
        if before == org && complete {
            return Ok(current_n);
        }
        org.apply(&mut fields);
        self.update_item(item_id, fields)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::sample_vault;

    #[test]
    fn tags_normalize_nest_and_refuse_junk() {
        assert_eq!(normalize_tag(" work / clients ").unwrap(), "work/clients");
        assert!(normalize_tag("work//x").is_err());
        assert!(normalize_tag("/x").is_err());
        assert!(normalize_tag(&"x".repeat(MAX_TAG_CHARS + 1)).is_err());
        assert_eq!(
            normalize_tags(&["b", "A", "a", "", " b "]).unwrap(),
            vec!["A".to_string(), "b".to_string()]
        );
        let many: Vec<String> = (0..=MAX_TAGS).map(|i| format!("t{i}")).collect();
        assert!(normalize_tags(&many).is_err());
    }

    #[test]
    fn organization_round_trips_through_fields_and_old_items_read_as_unset() {
        assert_eq!(Organization::of(&[]), Organization::default());
        let org = Organization {
            folder: Some(Id::new()),
            tags: vec!["family".into(), "work/clients".into()],
            favourite: true,
        };
        let mut fields = vec![PlainField::text("title", "title", "Bank")];
        org.apply(&mut fields);
        assert_eq!(fields.len(), 4);
        assert_eq!(Organization::of(&fields), org);
        // Applying again replaces, never doubles.
        Organization::default().apply(&mut fields);
        assert_eq!(fields.len(), 4);
        assert_eq!(Organization::of(&fields), Organization::default());
        let mut bare = vec![PlainField::text("title", "title", "Bank")];
        ensure_org_fields(&mut bare);
        ensure_org_fields(&mut bare);
        assert_eq!(bare.len(), 4);
    }

    #[test]
    fn folders_nest_and_never_loop() {
        let mut info = VaultInfo::default();
        let work = info.add_folder("Work", None).unwrap();
        let clients = info.add_folder("Clients", Some(work)).unwrap();
        assert!(
            info.add_folder("work", None).is_err(),
            "same name, same place"
        );
        assert!(
            info.add_folder("Clients", None).is_ok(),
            "same name, elsewhere"
        );
        assert_eq!(info.path(clients).unwrap(), "Work / Clients");
        assert!(info.is_within(clients, work));
        assert!(info.change_folder(work, "Work", Some(clients)).is_err());
        info.remove_folder(work).unwrap();
        assert_eq!(info.folders[&clients].parent, None, "moved up");
        assert_eq!(info.trash_days(), DEFAULT_TRASH_DAYS);
        info.trash_days = Some(10_000);
        assert_eq!(info.trash_days(), MAX_TRASH_DAYS);
    }

    #[test]
    fn the_vault_keeps_its_name_and_folders_sealed_in_one_hidden_item() {
        let mut vault = sample_vault();
        assert_eq!(vault.info().unwrap(), VaultInfo::default());
        let mut info = vault.info().unwrap();
        info.name = Some("Family".into());
        let f = info.add_folder("Streaming", None).unwrap();
        vault.set_info(&info).unwrap();
        let seq = vault.seq();
        vault.set_info(&info).unwrap();
        assert_eq!(vault.seq(), seq, "nothing changed, nothing written");
        assert_eq!(vault.info().unwrap(), info);
        let hidden: Vec<_> = vault
            .items()
            .filter(|i| i.kind == ItemKind::VaultInfo)
            .collect();
        assert_eq!(hidden.len(), 1);
        // The folder's name is in no field id and no clear text of the envelope.
        let bytes = hidden[0].to_bytes().unwrap();
        assert!(!bytes.windows(9).any(|w| w == b"Streaming"));

        // A second info item (another device, before a sync) reads as one,
        // and the next write folds it in.
        let mut other = VaultInfo {
            name: Some("Other".into()),
            ..VaultInfo::default()
        };
        other.folders.insert(
            Id::new(),
            Folder {
                name: "Games".into(),
                parent: None,
            },
        );
        vault
            .create_item(
                ItemKind::VaultInfo,
                ItemClass::Standard,
                other.to_fields().unwrap(),
            )
            .unwrap();
        let merged = vault.info().unwrap();
        // The oldest by first save, then by id, wins the name: the same
        // pick on every device (both were made within one second here).
        assert!(matches!(merged.name.as_deref(), Some("Family" | "Other")));
        assert_eq!(merged.folders.len(), 2);
        assert!(merged.folders.contains_key(&f));
        vault.set_info(&merged).unwrap();
        let live = vault
            .items()
            .filter(|i| i.kind == ItemKind::VaultInfo && i.state == ItemState::Live)
            .count();
        assert_eq!(live, 1);
        assert_eq!(vault.info().unwrap(), merged);
    }

    #[test]
    fn organize_files_tags_and_favourites_in_a_new_version() {
        let mut vault = sample_vault();
        let id = vault
            .create_item(
                ItemKind::Note,
                ItemClass::Standard,
                vec![PlainField::text("title", "title", "Plans")],
            )
            .unwrap();
        let mut info = vault.info().unwrap();
        let folder = info.add_folder("Home", None).unwrap();
        vault.set_info(&info).unwrap();
        let n = vault
            .organize(
                id,
                &Organization {
                    folder: Some(folder),
                    tags: vec!["b".into(), "a".into()],
                    favourite: true,
                },
            )
            .unwrap();
        assert_eq!(n, 2);
        let org = vault.organization(id).unwrap();
        assert_eq!(org.folder, Some(folder));
        assert_eq!(org.tags, vec!["a".to_string(), "b".to_string()]);
        assert!(org.favourite);
        // The same again writes nothing.
        assert_eq!(vault.organize(id, &org).unwrap(), 2);
        // A folder of another vault is refused.
        let bad = Organization {
            folder: Some(Id::new()),
            ..org.clone()
        };
        assert!(vault.organize(id, &bad).is_err());
        // The other fields are untouched.
        let fields = vault.open_item(id).unwrap();
        assert_eq!(fields[0].field_id, "title");
    }
}
