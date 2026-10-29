//! Fills an open vault from a pack through the core's real paths
//! (docs/plan/40-devtools.md §3.4):
//!
//! 1. The CXF document is read by the import's own reader
//!    (`lockdown_cxf::convert::to_items`), as an import of a CXF file is.
//! 2. Each item gets its extras (class, state, history, inline files,
//!    fresh keys) and its filing (folders from the CXF collections, tags,
//!    favourite), and the lot is written as a Lockdown backup document,
//!    one per vault, which the app imports (`import_preview`, then
//!    `import_commit`): the restore path, which keeps history, class,
//!    state, folders and files.
//! 3. Then the app's own calls: placeholder files (`attach`), Watchtower
//!    dismissals (`security_dismiss`), templates of the person's own and
//!    items made from them (`save_user_template`, `add_item`).

use std::collections::BTreeMap;

use lockdown_app::{App, FieldChange, GateInput, UserTemplateInput};
use lockdown_cxf::ImportedItem;
use lockdown_cxf::archive::{
    DocAttachment, DocField, DocFolder, DocItem, DocVault, DocVersion, Document,
};
use lockdown_vault::organization::{Organization, normalize_tag};
use lockdown_vault::{Id, PlainField};

use super::fresh;
use super::pack::{Extras, Fresh, Pack};

/// What a load made.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct Loaded {
    /// The pack's name.
    pub pack: String,
    /// Items made.
    pub items: usize,
    /// Of those, by kind.
    pub by_kind: BTreeMap<String, usize>,
    /// What the import said about the pack, per item.
    pub notes: Vec<String>,
}

const DAY: i64 = 86_400;

fn yes() -> GateInput {
    GateInput {
        password: None,
        confirmed: true,
    }
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Placeholder bytes for a file of `size`: a readable line, repeated.
fn placeholder(name: &str, size: u64) -> Vec<u8> {
    let line = format!("Lockdown demo placeholder for {name}. Nothing here is real.\n");
    line.bytes().cycle().take(size as usize).collect()
}

/// The folder tree a vault's items name, each path given an id.
#[derive(Default)]
struct Folders {
    ids: BTreeMap<String, Id>,
    list: Vec<DocFolder>,
}

impl Folders {
    fn id(&mut self, path: &str) -> Option<Id> {
        let mut parent: Option<Id> = None;
        let mut so_far = String::new();
        for part in path.split('/').map(str::trim).filter(|p| !p.is_empty()) {
            if !so_far.is_empty() {
                so_far.push('/');
            }
            so_far.push_str(part);
            let id = match self.ids.get(&so_far) {
                Some(id) => *id,
                None => {
                    let id = Id::new();
                    self.ids.insert(so_far.clone(), id);
                    self.list.push(DocFolder {
                        id: id.to_string(),
                        name: part.to_string(),
                        parent: parent.map(|p| p.to_string()),
                    });
                    id
                }
            };
            parent = Some(id);
        }
        parent
    }
}

/// One item, ready for the backup document, and what happens after.
struct Prepared {
    item_id: Id,
    doc: DocItem,
    later_files: Vec<(String, String, u64)>,
    extras: Extras,
}

fn prepare(
    mut item: ImportedItem,
    extras: Extras,
    folders: &mut Folders,
) -> Result<Prepared, String> {
    let mut fields = std::mem::take(&mut item.fields);
    if !fields.iter().any(|f| f.field_id == "title") {
        fields.insert(0, PlainField::text("title", "title", &item.title));
    }
    for fresh_kind in &extras.fresh {
        match fresh_kind {
            Fresh::Passkey => {
                fresh::passkeys(&mut fields)?;
            }
            Fresh::SshKey => fresh::ssh_fields(&mut fields, &item.title)?,
        }
    }
    let tags: Vec<String> = item
        .tags
        .iter()
        .filter_map(|t| normalize_tag(t).ok())
        .collect();
    let folder = item.folder.as_deref().and_then(|p| folders.id(p));
    Organization {
        folder,
        tags,
        favourite: item.favourite,
    }
    .apply(&mut fields);

    let title_field = fields.iter().find(|f| f.field_id == "title").cloned();
    let stamp = now();
    let versions = extras.history.len() as i64;
    let mut history = Vec::with_capacity(extras.history.len());
    for (i, version) in extras.history.iter().enumerate() {
        let mut plain: Vec<PlainField> = version
            .iter()
            .map(DocField::to_plain)
            .collect::<Result<_, _>>()?;
        if !plain.iter().any(|f| f.field_id == "title")
            && let Some(t) = &title_field
        {
            plain.insert(0, t.clone());
        }
        // Earlier versions a month apart, the oldest first.
        history.push(DocVersion {
            n: i as u64 + 1,
            edited_at: stamp - (versions - i as i64) * 30 * DAY,
            fields: plain.iter().map(DocField::from_plain).collect(),
        });
    }

    let mut attachments = Vec::new();
    let mut later_files = Vec::new();
    for f in &extras.files {
        match (&f.text, f.size) {
            (Some(text), _) => {
                attachments.push(DocAttachment::new(&f.name, &f.mime, text.as_bytes()))
            }
            (None, Some(size)) => later_files.push((f.name.clone(), f.mime.clone(), size)),
            (None, None) => {}
        }
    }

    let item_id = Id::new();
    let doc = DocItem {
        id: item_id.to_string(),
        kind: item.kind.as_str().into(),
        class: if extras.crown_jewel {
            "crown_jewel"
        } else {
            "standard"
        }
        .into(),
        state: extras
            .state
            .clone()
            .unwrap_or_else(|| if item.archived { "archived" } else { "live" }.into()),
        created_at: item.created_at.map(|t| t as i64),
        modified_at: item.modified_at.map(|t| t as i64),
        fields: fields.iter().map(DocField::from_plain).collect(),
        history,
        attachments,
        vault: None,
    };
    Ok(Prepared {
        item_id,
        doc,
        later_files,
        extras,
    })
}

/// Fills the open vault from `pack`.
pub fn load(app: &App, pack: &Pack) -> Result<Loaded, String> {
    pack.check()?;
    let header = pack.header()?;
    let items = lockdown_cxf::to_items(&header).map_err(|e| e.to_string())?;
    let err = |e: lockdown_app::AppError| e.message;

    // The vaults: the first one as it is, the pack's others made by name.
    let first = app
        .vaults()
        .map_err(err)?
        .into_iter()
        .find(|v| v.primary)
        .ok_or("the demo vault didn't open")?;
    let mut vault_ids: BTreeMap<Option<String>, String> = BTreeMap::new();
    vault_ids.insert(None, first.id.clone());
    for name in &pack.vaults {
        let id = app.create_vault(name).map_err(err)?;
        vault_ids.insert(Some(name.clone()), id);
    }

    // Each vault's items, in the pack's order.
    let mut per_vault: BTreeMap<Option<String>, (Folders, Vec<Prepared>)> = BTreeMap::new();
    let mut loaded = Loaded {
        pack: pack.name.clone(),
        ..Loaded::default()
    };
    for item in items {
        let extras = pack.items.get(&item.source_id).cloned().unwrap_or_default();
        *loaded
            .by_kind
            .entry(item.kind.as_str().to_string())
            .or_default() += 1;
        let (folders, list) = per_vault.entry(extras.vault.clone()).or_default();
        list.push(prepare(item, extras, folders)?);
    }

    let mut after = Vec::new();
    for (vault, (folders, list)) in per_vault {
        let vault_id = vault_ids
            .get(&vault)
            .cloned()
            .ok_or("a pack item names a vault the pack doesn't list")?;
        let mut doc = Document::new(now().max(0) as u64, "Lockdown demo pack");
        doc.vaults.push(DocVault {
            id: vault_id.clone(),
            name: None,
            folders: folders.list,
        });
        for p in list {
            let mut doc_item = p.doc;
            doc_item.vault = Some(vault_id.clone());
            doc.items.push(doc_item);
            after.push((p.item_id, p.later_files, p.extras));
        }
        let bytes = doc.to_bytes().map_err(|e| e.to_string())?;
        let preview = app
            .import_preview(
                bytes.to_vec(),
                "lockdown",
                "demo-pack.json",
                None,
                Some(vault_id),
            )
            .map_err(err)?;
        let report = app.import_commit(preview.token).map_err(err)?;
        loaded.items += report.imported;
        loaded.notes.extend(report.notes);
    }

    // What the import can't do: placeholder files, Watchtower states.
    let stamp = now();
    for (item_id, later_files, extras) in after {
        let id = item_id.to_string();
        for (name, mime, size) in later_files {
            app.attach(&id, &name, &mime, placeholder(&name, size), yes())
                .map_err(err)?;
        }
        for d in &extras.dismiss {
            let until = d.snooze_days.map(|days| stamp + i64::from(days) * DAY);
            app.security_dismiss(&id, &d.finding, until).map_err(err)?;
        }
    }

    // Templates of the person's own, and items made from them.
    for t in &pack.templates {
        let input: UserTemplateInput =
            serde_json::from_value(t.template.clone()).map_err(|e| e.to_string())?;
        // A secret field's value goes as bytes, as the editor sends it.
        let keys: BTreeMap<String, (String, bool)> = input
            .fields
            .iter()
            .map(|f| {
                (
                    f.key.clone(),
                    (format!("tpl.{}", f.key), f.kind == "hidden"),
                )
            })
            .collect();
        let template_id = app.save_user_template(None, None, input).map_err(err)?;
        *loaded.by_kind.entry("template".into()).or_default() += 1;
        for item in &t.items {
            let changes: Vec<FieldChange> = item
                .values
                .iter()
                .filter_map(|(k, v)| {
                    keys.get(k).map(|(id, secret)| {
                        if *secret {
                            FieldChange::secret(id, v.as_bytes())
                        } else {
                            FieldChange::text(id, v)
                        }
                    })
                })
                .collect();
            app.add_item(
                &format!("user:{template_id}"),
                Some(&item.title),
                false,
                changes,
            )
            .map_err(err)?;
            loaded.items += 1;
            *loaded.by_kind.entry("custom".into()).or_default() += 1;
        }
    }
    Ok(loaded)
}
