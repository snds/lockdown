//! Import and export (D103) through `lockdown-cxf`: CXF 1.0 and CSV in,
//! CXF and CSV out. Every file read here is hostile until the crate's
//! size-capped parser says otherwise, and a refusal carries the parser's
//! own reason. Export is one plain file of the whole vault; the gate in
//! front of it (the password, D101) is the app's, not this module's.

use lockdown_cxf::convert::{ImportedItem, from_items, to_items};
use lockdown_cxf::model::{self, b64url};
use lockdown_cxf::{Limits, csv};
use lockdown_vault::{FieldKind, FieldValue, ItemClass, ItemKind, ItemState, PlainField};

use super::{Session, text_of};
use crate::types::ImportReport;

/// Who the CXF file says exported it.
const EXPORTER_RP_ID: &str = "lockdown.snds.design";
const EXPORTER_NAME: &str = "Lockdown";

/// The formats this build moves in and out.
pub const FORMATS: [&str; 2] = ["cxf", "csv"];

fn unknown_format(format: &str) -> String {
    format!("'{format}' isn't a format this build reads; use cxf or csv")
}

impl Session {
    /// Reads `bytes` as `format` and creates one item per entry. Nothing is
    /// written unless the whole file parsed; anything inside an item that
    /// didn't map is reported by title, never dropped silently.
    pub fn import(&mut self, bytes: &[u8], format: &str) -> Result<ImportReport, String> {
        let limits = Limits::default();
        let items = match format {
            "cxf" => {
                let header = model::parse(bytes, &limits).map_err(|e| e.to_string())?;
                to_items(&header).map_err(|e| e.to_string())?
            }
            "csv" => {
                let table = csv::parse(bytes, &limits).map_err(|e| e.to_string())?;
                csv::import_logins(&table)
            }
            other => return Err(unknown_format(other)),
        };
        let mut report = ImportReport {
            imported: 0,
            notes: Vec::new(),
        };
        for item in items {
            let mut fields = item.fields;
            if !fields.iter().any(|f| f.field_id == "title") {
                fields.insert(0, PlainField::text("title", "title", &item.title));
            }
            self.opened
                .vault
                .create_item(item.kind, ItemClass::Standard, fields)
                .map_err(|e| e.to_string())?;
            report.imported += 1;
            for note in item.report {
                report.notes.push(format!("{}: {note}", item.title));
            }
        }
        if report.imported > 0 {
            self.save()?;
        }
        Ok(report)
    }

    /// The live vault as one file: CXF (every kind the crate maps) or CSV
    /// (logins only, the common header names). The bytes carry secrets;
    /// the caller hands them over once and drops them.
    pub fn export(&self, format: &str, now: u64) -> Result<Vec<u8>, String> {
        if !FORMATS.contains(&format) {
            return Err(unknown_format(format));
        }
        let vault = self.vault();
        let mut items = Vec::new();
        for item in vault.items() {
            if item.kind.hidden() || item.state != ItemState::Live {
                continue;
            }
            let fields = vault.open_item(item.item_id).map_err(|e| e.to_string())?;
            let latest = item.latest().map_err(|e| e.to_string())?;
            items.push(ImportedItem {
                source_id: b64url(item.item_id.to_string().as_bytes()),
                kind: item.kind,
                title: text_of(&fields, "title").unwrap_or_default(),
                fields,
                favourite: false,
                tags: Vec::new(),
                created_at: item.versions.first().map(|v| v.edited_at.0.max(0) as u64),
                modified_at: Some(latest.edited_at.0.max(0) as u64),
                report: Vec::new(),
            });
        }
        match format {
            "cxf" => {
                let header = from_items(&items, EXPORTER_RP_ID, EXPORTER_NAME, now)
                    .map_err(|e| e.to_string())?;
                model::write(&header).map_err(|e| e.to_string())
            }
            _ => {
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
                Ok(csv::write(
                    &["name", "url", "username", "password", "notes", "totp"],
                    &rows,
                )
                .into_bytes())
            }
        }
    }
}
