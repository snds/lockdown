//! Import and export (D103, D121) through `lockdown-cxf`.
//!
//! **Import** is two steps so the person sees what a file holds before
//! anything is kept: [`Session::import_preview`] parses the file (hostile
//! until the crate's capped readers say otherwise) and holds the result
//! under a token; [`Session::import_commit`] lands it. A Lockdown backup
//! keeps each item's id, class and state, so restoring the same backup
//! twice merges instead of doubling; every other source lands as new
//! items, a crown jewel by the template's rules (02-model §3).
//!
//! **Export** writes one file of the vault in the format asked for: the
//! Lockdown format (plain, or sealed under a password), CXF, CSV (logins)
//! or KDBX (sealed under a password). Crown jewels leave only when the
//! item was opted in (D121); the export says how many and which stayed,
//! and what the format itself couldn't carry. The gate in front of it
//! (the `Reveal` approval, D101) is the app's, not this module's.
//!
//! **Vaults and folders.** An import lands in the vault the person picked
//! (the first one by default); a source's folder path becomes that vault's
//! folders (made where missing), its tags and favourite the item's own
//! (15-organization). An export takes every vault or one; the Lockdown
//! format carries each vault's folders so a restore files items back where
//! they were, and the other formats carry the folder as the format can
//! (a CXF collection, a KeePass group).

use std::collections::BTreeMap;
use std::path::Path;

use lockdown_cxf::archive::{
    self, DocAttachment, DocField, DocFolder, DocItem, DocVault, DocVersion, Document, LeftOut,
    class_name,
};
use lockdown_cxf::convert::{ImportedItem, from_items};
use lockdown_cxf::import::{self, Format, Parsed};
use lockdown_cxf::model::{self, b64url};
use lockdown_cxf::{ImportedAttachment, Limits, csv, kdbx_out};
use lockdown_vault::{
    CrownJewelDefault, FieldKind, FieldValue, Id, ImportSpec, ItemClass, ItemKind, ItemState,
    NewAttachment, Organization, PlainField, Store, Vault, VaultInfo, export_opted_in,
    is_org_field, is_reserved_field, template_for,
};
use zeroize::Zeroizing;

use super::{Session, now_secs, text_of};
use crate::types::{
    ExportPreview, ImportFormatView, ImportPreview, ImportReport, KindCount, LeftOutView,
};

/// Who the CXF file says exported it.
const EXPORTER_RP_ID: &str = "lockdown.snds.design";
const EXPORTER_NAME: &str = "Lockdown";

/// The formats an export writes.
pub const EXPORT_FORMATS: [&str; 4] = ["lockdown", "cxf", "csv", "kdbx"];

/// A parsed file waiting for the person to say yes.
#[derive(Default)]
pub struct Pending {
    next: u64,
    held: Option<Held>,
}

struct Held {
    token: u64,
    target: Id,
    parsed: Parsed,
}

impl std::fmt::Debug for Pending {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pending")
            .field(
                "held",
                &self.held.as_ref().map(|h| (h.token, h.parsed.items.len())),
            )
            .finish()
    }
}

fn unknown_export(format: &str) -> String {
    format!("'{format}' isn't a format this build writes; use lockdown, cxf, csv or kdbx")
}

/// The import formats, for the wizard's list.
pub fn import_formats() -> Vec<ImportFormatView> {
    Format::ALL
        .into_iter()
        .map(|f| ImportFormatView {
            id: f.id().into(),
            name: f.name().into(),
            how: f.how().into(),
            may_need_password: f.may_need_password(),
        })
        .collect()
}

/// The format a file looks like, from its bytes and name.
pub fn detect_format(bytes: &[u8], name: &str) -> Option<String> {
    import::detect(bytes, name).map(|f| f.id().to_string())
}

/// What happens to one item when the file lands.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Landing {
    New,
    Update(Id),
    Unchanged,
    InTrashHere,
}

fn crown_by_template(kind: ItemKind, fields: &[PlainField]) -> bool {
    let template = template_for(kind);
    template.crown_jewel_default() == CrownJewelDefault::Default
        || fields
            .iter()
            .any(|f| template.field(&f.field_id).is_some_and(|t| t.crown_jewel))
}

/// The same item but for where it is filed (a restore over a copy filed
/// since): left as it is here.
fn same_but_filing(a: &[PlainField], b: &[PlainField]) -> bool {
    let strip = |x: &[PlainField]| -> Vec<PlainField> {
        x.iter()
            .filter(|f| !is_org_field(&f.field_id))
            .cloned()
            .collect()
    };
    same_fields(&strip(a), &strip(b))
}

fn same_fields(a: &[PlainField], b: &[PlainField]) -> bool {
    let key = |f: &PlainField| (f.field_id.clone(), f.kind, f.label.clone(), f.value.clone());
    let mut x: Vec<_> = a.iter().map(key).collect();
    let mut y: Vec<_> = b.iter().map(key).collect();
    x.sort_by(|p, q| p.0.cmp(&q.0));
    y.sort_by(|p, q| p.0.cmp(&q.0));
    x == y
}

/// Tags a source gave, as Lockdown keeps them: each one normalized, the
/// ones that can't be (too long, empty) named in the item's report.
fn tags_of(item: &mut ImportedItem) -> Vec<String> {
    let mut out = Vec::new();
    for t in std::mem::take(&mut item.tags) {
        match lockdown_vault::organization::normalize_tag(&t) {
            Ok(n) if !out.contains(&n) => out.push(n),
            Ok(_) => {}
            Err(_) => item.report.push(format!("the tag '{t}' couldn't be kept")),
        }
    }
    let max = lockdown_vault::organization::MAX_TAGS;
    if out.len() > max {
        item.report.push(format!(
            "only the first {max} of its {} tags were kept",
            out.len()
        ));
        out.truncate(max);
    }
    out
}

/// The folder a path (`Work/Servers`) names in `info`, made where missing.
/// `None` (and a report) when a name can't be a folder's.
fn folder_for(info: &mut VaultInfo, path: &str, report: &mut Vec<String>) -> Option<Id> {
    let mut parent: Option<Id> = None;
    for part in path.split('/').map(str::trim).filter(|p| !p.is_empty()) {
        let found = info
            .folders
            .iter()
            .find(|(_, f)| f.parent == parent && f.name.eq_ignore_ascii_case(part))
            .map(|(id, _)| *id);
        parent = Some(match found {
            Some(id) => id,
            None => match info.add_folder(part, parent) {
                Ok(id) => id,
                Err(e) => {
                    report.push(format!("the folder '{path}' couldn't be made ({e})"));
                    return None;
                }
            },
        });
    }
    parent
}

fn to_new(a: &ImportedAttachment) -> NewAttachment {
    NewAttachment {
        name: a.name.clone(),
        mime: a.mime.clone(),
        bytes: a.bytes.clone(),
    }
}

impl Session {
    /// Parses `bytes` as `format` (or the format the bytes look like when
    /// `format` is `auto`) and holds the result for [`Self::import_commit`].
    /// Nothing is written. A file with a password of its own answers
    /// "password needed" until `password` is given.
    pub fn import_preview(
        &mut self,
        bytes: &[u8],
        format: &str,
        file_name: &str,
        password: Option<&[u8]>,
        vault: Option<&str>,
    ) -> Result<ImportPreview, String> {
        let target = match vault {
            Some(v) => {
                let id = Id::parse(v).map_err(|_| format!("'{v}' isn't a vault id"))?;
                self.vault_by_id(id)?.context().vault_id
            }
            None => self.vault().context().vault_id,
        };
        let format = if format == "auto" {
            import::detect(bytes, file_name).ok_or_else(|| {
                "Lockdown couldn't tell which app made this file; pick the app from the list"
                    .to_string()
            })?
        } else {
            Format::from_id(format)
                .ok_or_else(|| format!("'{format}' isn't a format this build reads"))?
        };
        let limits = if format == Format::Lockdown {
            Limits::backup()
        } else {
            Limits::default()
        };
        let mut parsed =
            import::read(bytes, format, password, &limits).map_err(|e| e.to_string())?;
        for item in &mut parsed.items {
            if item.kept.is_none() {
                // A file from elsewhere doesn't get to set the app's own
                // fields (the export opt-in of D121) or name folders of a
                // vault it never saw; its folder and tags are applied at
                // commit, in the vault picked.
                let own = |id: &str| is_reserved_field(id) || is_org_field(id);
                item.fields.retain(|f| !own(&f.field_id));
                for h in &mut item.history {
                    h.retain(|f| !own(&f.field_id));
                }
            }
            if !item.fields.iter().any(|f| f.field_id == "title") {
                item.fields
                    .insert(0, PlainField::text("title", "title", &item.title));
            }
        }
        let preview = self.preview_of(format, &parsed, target)?;
        self.transfer.next += 1;
        let token = self.transfer.next;
        self.transfer.held = Some(Held {
            token,
            target,
            parsed,
        });
        Ok(ImportPreview { token, ..preview })
    }

    fn landing(vault: &Vault, item: &ImportedItem) -> Landing {
        let Some(kept) = item.kept else {
            return Landing::New;
        };
        match vault.item(kept.item_id) {
            Err(_) => Landing::New,
            Ok(existing) => match existing.state {
                ItemState::Tombstone => Landing::New,
                ItemState::Trashed => Landing::InTrashHere,
                _ => match vault.open_item(kept.item_id) {
                    Ok(fields)
                        if same_fields(&fields, &item.fields)
                            || same_but_filing(&fields, &item.fields) =>
                    {
                        let have = vault
                            .attachments(kept.item_id)
                            .map(|a| a.len())
                            .unwrap_or(0);
                        if have >= item.attachments.len() {
                            Landing::Unchanged
                        } else {
                            Landing::Update(kept.item_id)
                        }
                    }
                    _ => Landing::Update(kept.item_id),
                },
            },
        }
    }

    fn preview_of(
        &self,
        format: Format,
        parsed: &Parsed,
        target: Id,
    ) -> Result<ImportPreview, String> {
        let vault = self.vault_by_id(target)?;
        let mut kinds: BTreeMap<&'static str, (String, usize)> = BTreeMap::new();
        let mut preview = ImportPreview {
            token: 0,
            format: format.id().into(),
            format_name: format.name().into(),
            total: parsed.items.len(),
            by_kind: Vec::new(),
            attachments: 0,
            crown_jewels: 0,
            new: 0,
            updated: 0,
            unchanged: 0,
            problems: parsed.problems.clone(),
            item_notes: Vec::new(),
        };
        for item in &parsed.items {
            let t = template_for(item.kind);
            kinds
                .entry(item.kind.as_str())
                .or_insert((t.name.into(), 0))
                .1 += 1;
            preview.attachments += item.attachments.len();
            let crown = item.kept.is_some_and(|k| k.class == ItemClass::CrownJewel)
                || crown_by_template(item.kind, &item.fields);
            if crown {
                preview.crown_jewels += 1;
            }
            match Self::landing(vault, item) {
                Landing::New => preview.new += 1,
                Landing::Update(_) => preview.updated += 1,
                Landing::Unchanged => preview.unchanged += 1,
                Landing::InTrashHere => {
                    preview.unchanged += 1;
                    preview.item_notes.push(format!(
                        "{}: it's in Deleted here; restore it from there first to bring the backup's copy in",
                        item.title
                    ));
                }
            }
            for note in &item.report {
                preview.item_notes.push(format!("{}: {note}", item.title));
            }
        }
        preview.by_kind = kinds
            .into_iter()
            .map(|(kind, (name, count))| KindCount {
                kind: kind.into(),
                name,
                count,
            })
            .collect();
        Ok(preview)
    }

    /// Lands the file held under `token`: new items with their history,
    /// files and state; a backup's item already here gets the backup's
    /// fields as a new version (history kept); an unchanged one is left.
    pub fn import_commit(&mut self, token: u64) -> Result<ImportReport, String> {
        let Held { target, parsed, .. } = match self.transfer.held.take() {
            Some(h) if h.token == token => h,
            Some(h) => {
                self.transfer.held = Some(h);
                return Err("that import is no longer waiting; choose the file again".into());
            }
            None => return Err("there is no import waiting; choose the file again".into()),
        };
        let mut report = ImportReport {
            imported: 0,
            updated: 0,
            unchanged: 0,
            notes: parsed.problems.clone(),
        };
        let vault = self.vault_by_id_mut(target)?;
        let mut info = vault.info().map_err(|e| e.to_string())?;
        let info_before = info.clone();
        // A backup's folders come back with their ids, so its items' filing holds.
        for f in &parsed.folders {
            info.folders
                .entry(f.id)
                .or_insert_with(|| lockdown_vault::Folder {
                    name: f.name.clone(),
                    parent: f.parent,
                });
        }
        for mut item in parsed.items {
            let landing = Self::landing(vault, &item);
            let mut fields = std::mem::take(&mut item.fields);
            if item.kept.is_some() {
                let mut org = Organization::of(&fields);
                if org.folder.is_some_and(|f| !info.folders.contains_key(&f)) {
                    org.folder = None;
                    item.report
                        .push("its folder isn't in this backup, so it came back unfiled".into());
                    org.apply(&mut fields);
                }
            } else {
                let folder = match item.folder.clone() {
                    Some(path) => folder_for(&mut info, &path, &mut item.report),
                    None => None,
                };
                Organization {
                    folder,
                    tags: tags_of(&mut item),
                    favourite: item.favourite,
                }
                .apply(&mut fields);
            }
            lockdown_vault::organization::ensure_org_fields(&mut fields);
            for note in &item.report {
                report.notes.push(format!("{}: {note}", item.title));
            }
            match landing {
                Landing::Unchanged | Landing::InTrashHere => {
                    report.unchanged += 1;
                    continue;
                }
                Landing::Update(id) => {
                    let have: Vec<(String, u64)> = vault
                        .attachments(id)
                        .map_err(|e| e.to_string())?
                        .into_iter()
                        .filter_map(|a| a.meta.as_ref().map(|m| (m.name.clone(), m.size)))
                        .collect();
                    let extra: Vec<NewAttachment> = item
                        .attachments
                        .iter()
                        .filter(|a| !have.contains(&(a.name.clone(), a.bytes.len() as u64)))
                        .map(to_new)
                        .collect();
                    vault
                        .update_item_with_files(id, fields, &extra)
                        .map_err(|e| e.to_string())?;
                    report.updated += 1;
                }
                Landing::New => {
                    let crown = crown_by_template(item.kind, &fields);
                    let (item_id, class, state) = match item.kept {
                        Some(k) if vault.item(k.item_id).is_err() => {
                            (Some(k.item_id), k.class, k.state)
                        }
                        Some(k) => (None, k.class, k.state),
                        None => (
                            None,
                            ItemClass::Standard,
                            if item.archived {
                                ItemState::Archived
                            } else {
                                ItemState::Live
                            },
                        ),
                    };
                    let class = if crown { ItemClass::CrownJewel } else { class };
                    let mut versions = item.history;
                    versions.push(fields);
                    let spec = ImportSpec {
                        item_id,
                        kind: item.kind,
                        class,
                        versions,
                        attachments: item.attachments.iter().map(to_new).collect(),
                        state,
                    };
                    vault.import_item(spec).map_err(|e| e.to_string())?;
                    report.imported += 1;
                }
            }
        }
        if info != info_before {
            vault.set_info(&info).map_err(|e| e.to_string())?;
        }
        if report.imported + report.updated > 0 || info != info_before {
            self.save()?;
        }
        Ok(report)
    }

    /// Drops a held import.
    pub fn import_cancel(&mut self) {
        self.transfer.held = None;
    }

    /// One step, for callers that don't preview (the CLI shape): parse and
    /// land at once.
    pub fn import(&mut self, bytes: &[u8], format: &str) -> Result<ImportReport, String> {
        let preview = self.import_preview(bytes, format, "", None, None)?;
        self.import_commit(preview.token)
    }

    /// Turns a crown jewel's export opt-in on or off (D121), as a new
    /// version so it syncs. The gate in front of turning it on is the app's.
    pub fn set_export_opt_in(&mut self, id: &str, on: bool) -> Result<u64, String> {
        let item_id = Id::parse(id).map_err(|_| format!("'{id}' isn't an item id"))?;
        let n = self
            .holding_mut(item_id)?
            .set_export_opt_in(item_id, on)
            .map_err(|e| e.to_string())?;
        self.save()?;
        Ok(n)
    }

    fn blob(&self, vault: &Vault, hash: lockdown_vault::Hash32) -> Option<Vec<u8>> {
        vault
            .pending_blob(hash)
            .map(<[u8]>::to_vec)
            .or_else(|| self.store.get_blob(hash).ok())
    }

    /// Every visible item as an export takes it: what goes, and what stays
    /// (crown jewels not opted in, D121). `trashed` takes the Deleted view
    /// too (the Lockdown format keeps it; the others don't).
    fn gather(&self, trashed: bool, only: Option<Id>) -> Result<Gathered, String> {
        let mut out = Gathered::default();
        for vault in self
            .vaults()
            .filter(|v| only.is_none_or(|o| v.context().vault_id == o))
            .filter(|v| self.exportable(v.context().vault_id))
        {
            let info = vault.info().map_err(|e| e.to_string())?;
            out.vaults.push(DocVault {
                id: vault.context().vault_id.to_string(),
                name: info.name.clone(),
                folders: info
                    .folders
                    .iter()
                    .map(|(id, f)| DocFolder {
                        id: id.to_string(),
                        name: f.name.clone(),
                        parent: f.parent.map(|p| p.to_string()),
                    })
                    .collect(),
            });
            self.gather_vault(vault, &info, trashed, &mut out)?;
        }
        Ok(out)
    }

    fn gather_vault(
        &self,
        vault: &Vault,
        info: &VaultInfo,
        trashed: bool,
        out: &mut Gathered,
    ) -> Result<(), String> {
        for item in vault.items() {
            if item.kind.hidden() {
                continue;
            }
            let in_scope = match item.state {
                ItemState::Live | ItemState::Archived => true,
                ItemState::Trashed => trashed,
                ItemState::Tombstone => false,
            };
            if !in_scope {
                if item.state == ItemState::Trashed {
                    out.trashed_left += 1;
                }
                continue;
            }
            let fields = vault.open_item(item.item_id).map_err(|e| e.to_string())?;
            let title = text_of(&fields, "title").unwrap_or_default();
            if item.class == ItemClass::CrownJewel && !export_opted_in(&fields) {
                out.left_out.push(LeftOut {
                    id: item.item_id.to_string(),
                    title,
                    kind: item.kind.as_str().into(),
                    reason: "crown-jewel".into(),
                });
                continue;
            }
            let latest = item.latest().map_err(|e| e.to_string())?;
            let mut history = Vec::new();
            let mut shredded = 0;
            for v in &item.versions[..item.versions.len() - 1] {
                match vault.open_version(item.item_id, v.n) {
                    Ok(f) => history.push((v.n, v.edited_at.0, f)),
                    Err(_) => shredded += 1,
                }
            }
            let mut files = Vec::new();
            let mut missing = Vec::new();
            for a in vault.attachments(item.item_id).map_err(|e| e.to_string())? {
                let Some(meta) = a.meta.clone() else { continue };
                match self
                    .blob(vault, a.blob)
                    .and_then(|b| vault.open_attachment(item.item_id, a.att_id, &b).ok())
                {
                    Some((_, bytes)) => files.push(ImportedAttachment {
                        name: meta.name.clone(),
                        mime: meta.mime.clone(),
                        bytes,
                    }),
                    None => missing.push(meta.name.clone()),
                }
            }
            let org = Organization::of(&fields);
            out.items.push(Gathered1 {
                vault_id: vault.context().vault_id,
                folder: org.folder.and_then(|f| info.path(f)),
                tags: org.tags,
                favourite: org.favourite,
                item_id: item.item_id,
                kind: item.kind,
                class: item.class,
                state: item.state,
                created_at: item.versions.first().map(|v| v.edited_at.0),
                modified_at: latest.edited_at.0,
                title,
                fields,
                history,
                files,
                missing,
                shredded,
            });
        }
        Ok(())
    }

    fn vault_choice(&self, vault: Option<&str>) -> Result<Option<Id>, String> {
        match vault {
            None => Ok(None),
            Some(v) => {
                let id = Id::parse(v).map_err(|_| format!("'{v}' isn't a vault id"))?;
                Ok(Some(self.vault_by_id(id)?.context().vault_id))
            }
        }
    }

    /// What an export in `format` would carry and leave, before anything
    /// is written.
    pub fn export_preview(
        &self,
        format: &str,
        vault: Option<&str>,
    ) -> Result<ExportPreview, String> {
        if !EXPORT_FORMATS.contains(&format) {
            return Err(unknown_export(format));
        }
        let g = self.gather(format == "lockdown", self.vault_choice(vault)?)?;
        Ok(g.preview(format))
    }

    /// The Lockdown document of everything an export or backup may carry.
    pub(crate) fn backup_document(
        &self,
        only: Option<Id>,
    ) -> Result<(Document, ExportPreview), String> {
        let g = self.gather(true, only)?;
        let preview = g.preview("lockdown");
        let mut doc = Document::new(
            now_secs().max(0) as u64,
            &format!("Lockdown {}", env!("CARGO_PKG_VERSION")),
        );
        for it in &g.items {
            doc.items.push(DocItem {
                id: it.item_id.to_string(),
                kind: it.kind.as_str().into(),
                class: class_name(it.class).into(),
                state: archive::state_name(it.state).into(),
                created_at: it.created_at,
                modified_at: Some(it.modified_at),
                fields: it.fields.iter().map(DocField::from_plain).collect(),
                history: it
                    .history
                    .iter()
                    .map(|(n, at, f)| DocVersion {
                        n: *n,
                        edited_at: *at,
                        fields: f.iter().map(DocField::from_plain).collect(),
                    })
                    .collect(),
                attachments: it
                    .files
                    .iter()
                    .map(|a| DocAttachment::new(&a.name, &a.mime, &a.bytes))
                    .collect(),
                vault: Some(it.vault_id.to_string()),
            });
        }
        doc.left_out = g.left_out.clone();
        doc.vaults = g.vaults.clone();
        Ok((doc, preview))
    }

    /// The export file's bytes: `lockdown` plain (no password) or sealed
    /// (a password), `cxf` and `csv` plain, `kdbx` sealed (a password is
    /// needed). The bytes carry secrets; the caller writes them once and
    /// drops them.
    pub fn export_file(
        &self,
        format: &str,
        password: Option<&[u8]>,
        vault: Option<&str>,
    ) -> Result<(Zeroizing<Vec<u8>>, ExportPreview), String> {
        if !EXPORT_FORMATS.contains(&format) {
            return Err(unknown_export(format));
        }
        let only = self.vault_choice(vault)?;
        let password = password.filter(|p| !p.is_empty());
        if format == "lockdown" {
            let (doc, mut preview) = self.backup_document(only)?;
            let plain = doc.to_bytes().map_err(|e| e.to_string())?;
            let Some(pw) = password else {
                return Ok((plain, preview));
            };
            let kdf = archive::Kdf::fresh().map_err(|e| e.to_string())?;
            let key = kdf.derive(pw).map_err(|e| e.to_string())?;
            let sealed = archive::seal(&plain, &key, &kdf, now_secs().max(0) as u64)
                .map_err(|e| e.to_string())?;
            preview.encrypted = true;
            return Ok((Zeroizing::new(sealed), preview));
        }
        let g = self.gather(false, only)?;
        let mut preview = g.preview(format);
        let items = g.imported_items();
        let bytes = match format {
            "cxf" => {
                let header = from_items(
                    &items,
                    EXPORTER_RP_ID,
                    EXPORTER_NAME,
                    now_secs().max(0) as u64,
                )
                .map_err(|e| e.to_string())?;
                Zeroizing::new(model::write(&header).map_err(|e| e.to_string())?)
            }
            "csv" => Zeroizing::new(csv_of(&items).into_bytes()),
            _ => {
                let pw = password.ok_or("a KeePass file needs a password of its own")?;
                let pw = std::str::from_utf8(pw).map_err(|_| "the password isn't valid text")?;
                preview.encrypted = true;
                Zeroizing::new(kdbx_out::write(&items, pw, "Lockdown").map_err(|e| e.to_string())?)
            }
        };
        Ok((bytes, preview))
    }

    /// The live vault as one plain file, CXF or CSV, crown jewels left out
    /// unless opted in (the one-call shape the first desktop slice used).
    pub fn export(&self, format: &str, _now: u64) -> Result<Vec<u8>, String> {
        if !["cxf", "csv"].contains(&format) {
            return Err(unknown_export(format));
        }
        Ok(self.export_file(format, None, None)?.0.to_vec())
    }
}

/// Writes `bytes` to `path` the person chose: readable by them only,
/// through a partial file and a rename, so a failed write leaves nothing
/// half-made under the name they picked.
pub fn write_user_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let name = path
        .file_name()
        .ok_or_else(|| format!("{} isn't a file name", path.display()))?;
    let mut partial = name.to_os_string();
    partial.push(".partial");
    let tmp = path.with_file_name(partial);
    let write = || -> std::io::Result<()> {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&tmp)?;
        std::io::Write::write_all(&mut file, bytes)?;
        file.sync_all()?;
        std::fs::rename(&tmp, path)
    };
    write().map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("couldn't write {}: {e}", path.display())
    })
}

#[derive(Default)]
struct Gathered {
    items: Vec<Gathered1>,
    vaults: Vec<DocVault>,
    left_out: Vec<LeftOut>,
    trashed_left: usize,
}

struct Gathered1 {
    vault_id: Id,
    folder: Option<String>,
    tags: Vec<String>,
    favourite: bool,
    item_id: Id,
    kind: ItemKind,
    class: ItemClass,
    state: ItemState,
    created_at: Option<i64>,
    modified_at: i64,
    title: String,
    fields: Vec<PlainField>,
    history: Vec<(u64, i64, Vec<PlainField>)>,
    files: Vec<ImportedAttachment>,
    missing: Vec<String>,
    shredded: usize,
}

impl Gathered {
    fn preview(&self, format: &str) -> ExportPreview {
        let crown_in = self
            .items
            .iter()
            .filter(|i| i.class == ItemClass::CrownJewel)
            .count();
        let mut dropped = Vec::new();
        let files: usize = self.items.iter().map(|i| i.files.len()).sum();
        let with_history = self.items.iter().filter(|i| !i.history.is_empty()).count();
        for i in &self.items {
            for m in &i.missing {
                dropped.push(format!(
                    "{}: the file '{m}' isn't on this device yet (sync to fetch it)",
                    i.title
                ));
            }
            if i.shredded > 0 {
                dropped.push(format!(
                    "{}: {} earlier versions were deleted for good and can't be exported",
                    i.title, i.shredded
                ));
            }
        }
        let mut carried = self.items.len();
        match format {
            "cxf" => {
                if files > 0 {
                    dropped.push(format!(
                        "{files} attached files: a CXF file has no place for them; a Lockdown backup keeps them"
                    ));
                }
                if with_history > 0 {
                    dropped.push(format!(
                        "earlier versions of {with_history} items: CXF keeps only the item as it is now"
                    ));
                }
            }
            "csv" => {
                let logins = self
                    .items
                    .iter()
                    .filter(|i| i.kind == ItemKind::Login)
                    .count();
                if logins < self.items.len() {
                    dropped.push(format!(
                        "{} items that aren't logins: a CSV holds logins only",
                        self.items.len() - logins
                    ));
                }
                let passkeys = self
                    .items
                    .iter()
                    .filter(|i| i.fields.iter().any(|f| f.kind == FieldKind::Passkey))
                    .count();
                if passkeys > 0 {
                    dropped.push(format!(
                        "passkeys on {passkeys} items: a CSV can't hold them"
                    ));
                }
                if files > 0 {
                    dropped.push(format!("{files} attached files"));
                }
                if with_history > 0 {
                    dropped.push(format!("earlier versions of {with_history} items"));
                }
                carried = logins;
            }
            "kdbx" => {
                let mut rules = 0;
                for i in &self.items {
                    let as_item = i.imported();
                    rules += kdbx_out::dropped(&as_item)
                        .iter()
                        .filter(|d| d.contains("match rules"))
                        .count();
                }
                if with_history > 0 {
                    dropped.push(format!(
                        "earlier versions of {with_history} items: KeePass keeps its own history from now on"
                    ));
                }
                if rules > 0 {
                    dropped.push(format!(
                        "website match rules on {rules} items: KeePass matches by address only"
                    ));
                }
                dropped.push(
                    "item kinds: each kind becomes a KeePass group, every item a KeePass entry"
                        .into(),
                );
            }
            _ => {}
        }
        if format != "lockdown" && self.trashed_left > 0 {
            dropped.push(format!(
                "{} items in Deleted: only a Lockdown backup keeps those",
                self.trashed_left
            ));
        }
        ExportPreview {
            format: format.into(),
            items: carried,
            attachments: if matches!(format, "lockdown" | "kdbx") {
                files
            } else {
                0
            },
            crown_jewels_included: crown_in,
            crown_jewels_left_out: self.left_out.len(),
            left_out: self
                .left_out
                .iter()
                .map(|l| LeftOutView {
                    id: l.id.clone(),
                    title: l.title.clone(),
                    kind: l.kind.clone(),
                    reason: "It's a crown jewel and isn't set to be included in exports.".into(),
                })
                .collect(),
            dropped,
            encrypted: false,
        }
    }

    fn imported_items(&self) -> Vec<ImportedItem> {
        self.items.iter().map(Gathered1::imported).collect()
    }
}

impl Gathered1 {
    fn imported(&self) -> ImportedItem {
        let mut item = ImportedItem::new(
            b64url(self.item_id.to_string().as_bytes()),
            self.kind,
            self.title.clone(),
        );
        // Folder, tags and favourite travel in the format's own slots.
        item.fields = self
            .fields
            .iter()
            .filter(|f| !is_org_field(&f.field_id))
            .cloned()
            .collect();
        item.folder = self.folder.clone();
        item.tags = self.tags.clone();
        item.favourite = self.favourite;
        item.created_at = self.created_at.map(|t| t.max(0) as u64);
        item.modified_at = Some(self.modified_at.max(0) as u64);
        item.archived = self.state == ItemState::Archived;
        item.attachments = self.files.clone();
        item.history = self
            .history
            .iter()
            .map(|(_, _, f)| {
                f.iter()
                    .filter(|p| !is_org_field(&p.field_id))
                    .cloned()
                    .collect()
            })
            .collect();
        item
    }
}

fn csv_of(items: &[ImportedItem]) -> String {
    let rows: Vec<Vec<String>> = items
        .iter()
        .filter(|i| i.kind == ItemKind::Login)
        .map(|i| {
            let f = &i.fields;
            let url = f.iter().find_map(|p| match &p.value {
                FieldValue::Url { value, .. } => Some(value.clone()),
                _ => None,
            });
            let totp = f.iter().find_map(|p| match (&p.kind, &p.value) {
                (FieldKind::Totp, FieldValue::Text(t)) => Some(t.clone()),
                _ => None,
            });
            vec![
                i.title.clone(),
                url.unwrap_or_default(),
                text_of(f, "user").unwrap_or_default(),
                text_of(f, "pw").unwrap_or_default(),
                text_of(f, "notes").unwrap_or_default(),
                totp.unwrap_or_default(),
            ]
        })
        .collect();
    csv::write(
        &["name", "url", "username", "password", "notes", "totp"],
        &rows,
    )
}
