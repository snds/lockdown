//! The demo's data pack, version 1 (docs/plan/40-devtools.md §3.3): what
//! the demo vault is filled from. One JSON file:
//!
//! ```text
//! {
//!   "format": "lockdown-demo-pack",
//!   "v": 1,
//!   "name": "…", "about": "…",
//!   "cxf": { a CXF v1.0 document: items, credentials, collections (folders), tags, favourites },
//!   "vaults": ["Work"],                          the vaults made beside the first one
//!   "items": { "<CXF item id>": extras },         what CXF can't say, per item
//!   "templates": [ { "template": {…}, "items": [ { "title": "…", "values": { "<key>": "…" } } ] } ]
//! }
//! ```
//!
//! Extras: `vault` (a name from `vaults`), `crown_jewel`, `state` (`live`,
//! `archived`, `trashed`), `history` (earlier versions, oldest first, each
//! a list of fields in the backup format's shape, `docs/format/backup-v1.md`
//! §2.2), `files` (each `{name, mime, text}` inline, or `{name, mime,
//! size}` made of placeholder bytes when the demo opens), `fresh`
//! (`passkey`, `ssh-key`: keys made when the demo opens) and `dismiss`
//! (`{finding, snooze_days}`: a Watchtower finding dismissed, or snoozed).
//!
//! A bare CXF document is a pack too, with no extras: another manager's
//! CXF export can be tried as it is. Everything is checked before the demo
//! opens; a pack that doesn't read is refused with the reader's sentence.

use std::collections::{BTreeMap, BTreeSet};

use lockdown_cxf::archive::DocField;
use lockdown_cxf::{Header, Limits};
use serde::{Deserialize, Serialize};

/// The `format` member.
pub const FORMAT: &str = "lockdown-demo-pack";
/// The version this build reads and writes.
pub const PACK_V: u16 = 1;
/// The largest placeholder file a pack may ask for (the attachment limit
/// of an import, so the same file could be restored from a backup).
pub const MAX_FILE: u64 = 32 * 1024 * 1024;
/// The largest pack file read.
pub const MAX_PACK: usize = 16 * 1024 * 1024;

/// The default pack, built in.
pub const DEFAULT: &[u8] = include_bytes!("../../demo/default-pack.json");

/// A pack.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pack {
    /// Always [`FORMAT`].
    pub format: String,
    /// [`PACK_V`].
    pub v: u16,
    /// Its name, shown in the demo's bar and the Developer menu's notices.
    pub name: String,
    /// A sentence on what it holds.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub about: String,
    /// The CXF document.
    pub cxf: serde_json::Value,
    /// Vaults made beside the first, by name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vaults: Vec<String>,
    /// What CXF can't say, by the CXF item's id.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub items: BTreeMap<String, Extras>,
    /// Templates of the person's own, and items made from them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub templates: Vec<PackTemplate>,
}

/// One item's extras.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Extras {
    /// The vault it goes in, by name; the first vault when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault: Option<String>,
    /// In the crown-jewel class.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub crown_jewel: bool,
    /// `live` (the default), `archived` or `trashed`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    /// Earlier versions, oldest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<Vec<DocField>>,
    /// Attached files.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<PackFile>,
    /// Keys made when the demo opens.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fresh: Vec<Fresh>,
    /// Watchtower findings dismissed or snoozed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dismiss: Vec<PackDismissal>,
}

/// A file on an item.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackFile {
    /// Its name.
    pub name: String,
    /// Its media type.
    pub mime: String,
    /// Its contents, as text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Or its size: placeholder bytes made when the demo opens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

/// A key made when the demo opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Fresh {
    /// Every `passkey.<n>` the item names gets a new P-256 key, credential
    /// id and user handle.
    Passkey,
    /// A new Ed25519 key in OpenSSH form, its public key and fingerprint.
    SshKey,
}

/// A Watchtower finding put away.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackDismissal {
    /// The finding's key (`weak:pw`, `reused:pw`, `missing_2fa`).
    pub finding: String,
    /// Snoozed this many days; dismissed for good when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snooze_days: Option<u32>,
}

/// A template of the person's own, with items made from it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackTemplate {
    /// The template, as the app's `save_user_template` takes it.
    pub template: serde_json::Value,
    /// Items made from it.
    #[serde(default)]
    pub items: Vec<TemplateItem>,
}

/// An item made from a pack's template.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateItem {
    /// Its title.
    pub title: String,
    /// Values by the template's field key.
    #[serde(default)]
    pub values: BTreeMap<String, String>,
}

impl Pack {
    /// Reads and checks a pack: one in this format, or a bare CXF document.
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_PACK {
            return Err(format!(
                "The pack is {} bytes; the limit is {MAX_PACK}.",
                bytes.len()
            ));
        }
        let value: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|e| format!("The pack isn't JSON: {e}."))?;
        let pack = if value.get("format").is_some() && value.get("accounts").is_none() {
            serde_json::from_value::<Pack>(value)
                .map_err(|e| format!("The pack didn't read: {e}."))?
        } else {
            Pack {
                format: FORMAT.into(),
                v: PACK_V,
                name: "A CXF file".into(),
                about: String::new(),
                cxf: value,
                vaults: Vec::new(),
                items: BTreeMap::new(),
                templates: Vec::new(),
            }
        };
        pack.check()?;
        Ok(pack)
    }

    /// The pack as JSON.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = serde_json::to_vec_pretty(self).unwrap_or_default();
        out.push(b'\n');
        out
    }

    /// The CXF document, read by the import's own reader.
    pub fn header(&self) -> Result<Header, String> {
        let bytes = serde_json::to_vec(&self.cxf).map_err(|e| e.to_string())?;
        lockdown_cxf::model::parse(&bytes, &Limits::default())
            .map_err(|e| format!("The pack's CXF document didn't read: {e}."))
    }

    /// Everything a load relies on, each fault in a sentence.
    pub fn check(&self) -> Result<(), String> {
        if self.format != FORMAT {
            return Err(format!(
                "The pack's format is '{}', not '{FORMAT}'.",
                self.format
            ));
        }
        if self.v > PACK_V {
            return Err(format!(
                "The pack is version {}; this build reads up to {PACK_V}.",
                self.v
            ));
        }
        if self.name.trim().is_empty() {
            return Err("The pack has no name.".into());
        }
        let header = self.header()?;
        let ids: BTreeSet<&str> = header
            .accounts
            .iter()
            .flat_map(|a| a.items.iter().map(|i| i.id.as_str()))
            .collect();
        if ids.is_empty() && self.templates.is_empty() {
            return Err("The pack holds no items.".into());
        }
        let vaults: BTreeSet<&str> = self.vaults.iter().map(String::as_str).collect();
        if vaults.len() != self.vaults.len() || vaults.iter().any(|v| v.trim().is_empty()) {
            return Err("The pack's vault names must be different and not empty.".into());
        }
        for (id, extras) in &self.items {
            let what = |why: String| format!("The pack's item '{id}': {why}.");
            if !ids.contains(id.as_str()) {
                return Err(what("there is no CXF item with that id".into()));
            }
            if let Some(v) = &extras.vault
                && !vaults.contains(v.as_str())
            {
                return Err(what(format!("'{v}' isn't one of the pack's vaults")));
            }
            if let Some(s) = &extras.state
                && !matches!(s.as_str(), "live" | "archived" | "trashed")
            {
                return Err(what(format!("'{s}' isn't live, archived or trashed")));
            }
            for version in &extras.history {
                for f in version {
                    f.to_plain()
                        .map_err(|e| what(format!("its history: {e}")))?;
                }
            }
            for f in &extras.files {
                match (&f.text, f.size) {
                    (Some(_), None) => {}
                    (None, Some(size)) if size > 0 && size <= MAX_FILE => {}
                    (None, Some(size)) => {
                        return Err(what(format!(
                            "the file '{}' is {size} bytes; it may be 1 to {MAX_FILE}",
                            f.name
                        )));
                    }
                    _ => {
                        return Err(what(format!(
                            "the file '{}' needs either its text or its size",
                            f.name
                        )));
                    }
                }
                if extras.crown_jewel && f.size.is_some() {
                    return Err(what(format!(
                        "the placeholder file '{}' can't go on a crown jewel (adding it would ask for Touch ID while the demo opens)",
                        f.name
                    )));
                }
            }
            for d in &extras.dismiss {
                if d.finding.trim().is_empty() || d.snooze_days == Some(0) {
                    return Err(what(
                        "a dismissal needs a finding, and a snooze at least a day".into(),
                    ));
                }
            }
        }
        for t in &self.templates {
            let input: lockdown_app::UserTemplateInput = serde_json::from_value(t.template.clone())
                .map_err(|e| format!("A template in the pack didn't read: {e}."))?;
            for item in &t.items {
                if let Some(key) = item
                    .values
                    .keys()
                    .find(|k| !input.fields.iter().any(|f| &f.key == *k))
                {
                    return Err(format!(
                        "The pack's item '{}' made from '{}' names the field '{key}', which the template doesn't have.",
                        item.title, input.name
                    ));
                }
            }
        }
        Ok(())
    }

    /// How many items a load makes.
    pub fn item_count(&self) -> usize {
        let cxf = self
            .header()
            .map(|h| h.accounts.iter().map(|a| a.items.len()).sum())
            .unwrap_or(0);
        cxf + self.templates.iter().map(|t| t.items.len()).sum::<usize>()
    }
}

/// The default pack.
pub fn default_pack() -> Pack {
    Pack::parse(DEFAULT).expect("the built-in demo pack reads")
}
