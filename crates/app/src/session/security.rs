//! What the security center reads from the open vault, and the one thing
//! it writes there (docs/plan/13-security-center.md).
//!
//! Reading: [`Session::security_scan`] walks the live items once and hands
//! each stored password to a callback **on this thread**, as a borrowed
//! slice, so the caller can score it and hash it without the bytes ever
//! leaving the core; what comes back is facts (title, kind, websites,
//! whether a code or passkey is held, the item's own security settings),
//! never a value.
//!
//! Writing: a finding dismissed or snoozed is a setting **on the item**,
//! the reserved field `lockdown.security` ([`lockdown_vault::is_reserved`]),
//! so it syncs to every device and travels with the item's history and
//! export like any field, while `show` and the editors never see it.

use std::collections::BTreeMap;

use lockdown_vault::{
    FieldKind, FieldValue, ItemClass, ItemKind, ItemState, PlainField, template_for,
};
use serde::{Deserialize, Serialize};

use super::{Session, parse_id, text_of};

/// The item field that holds the security center's settings for the item.
pub(crate) const SECURITY_FIELD: &str = "lockdown.security";

/// Its label, for an export reader.
const SECURITY_LABEL: &str = "Security review";

/// A finding the person dismissed (`until: None`) or snoozed until a time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dismissal {
    /// When (seconds since the Unix epoch).
    pub at: i64,
    /// Snoozed until then; `None` is dismissed for good.
    pub until: Option<i64>,
}

/// What the reserved field holds: dismissals by finding key (`weak:pw`,
/// `missing_2fa`, …). Unknown fields are kept out; a newer build's extra
/// fields are dropped on rewrite, which loses at most a dismissal.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ItemSecurity {
    #[serde(default)]
    pub dismissed: BTreeMap<String, Dismissal>,
}

impl ItemSecurity {
    fn read(fields: &[PlainField]) -> Self {
        text_of(fields, SECURITY_FIELD)
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }
}

/// One item as the report sees it: facts, no values.
#[derive(Clone, Debug)]
pub(crate) struct ItemFacts {
    pub item_id: String,
    pub title: String,
    pub kind: ItemKind,
    pub crown_jewel: bool,
    pub version: u64,
    /// Every web address saved on it, as typed.
    pub urls: Vec<String>,
    pub username: String,
    /// Whether it holds a one-time code.
    pub has_code: bool,
    /// Whether it holds a passkey.
    pub has_passkey: bool,
    pub security: ItemSecurity,
}

/// Whether a stored field is a password the report looks at: a one-line
/// secret the template offers the generator for (D112, D116's same set).
fn is_password(kind: ItemKind, f: &PlainField) -> bool {
    f.kind == FieldKind::Hidden
        && template_for(kind)
            .field(&f.field_id)
            .is_some_and(|t| t.generator.is_some())
}

impl Session {
    /// Every live item's facts; each password goes to `each_password`
    /// (`facts`, field id, field label, bytes) here, borrowed, and nowhere
    /// else. Trashed, archived and hidden items are left out.
    pub(crate) fn security_scan(
        &self,
        mut each_password: impl FnMut(&ItemFacts, &str, &str, &[u8]),
    ) -> Result<Vec<ItemFacts>, String> {
        let mut out = Vec::new();
        for (vault, item) in self.vaults().flat_map(|v| v.items().map(move |i| (v, i))) {
            if item.kind.hidden() || item.state != ItemState::Live {
                continue;
            }
            let fields = vault.open_item(item.item_id).map_err(|e| e.to_string())?;
            let latest = item.latest().map_err(|e| e.to_string())?;
            let facts = ItemFacts {
                item_id: item.item_id.to_string(),
                title: text_of(&fields, "title").unwrap_or_default(),
                kind: item.kind,
                crown_jewel: item.class == ItemClass::CrownJewel,
                version: latest.n,
                urls: fields
                    .iter()
                    .filter_map(|f| match &f.value {
                        FieldValue::Url { value, .. } if f.kind == FieldKind::Url => {
                            Some(value.clone())
                        }
                        _ => None,
                    })
                    .collect(),
                username: text_of(&fields, "user").unwrap_or_default(),
                has_code: fields.iter().any(|f| f.kind == FieldKind::Totp),
                has_passkey: fields.iter().any(|f| f.kind == FieldKind::Passkey),
                security: ItemSecurity::read(&fields),
            };
            for f in fields.iter().filter(|f| is_password(item.kind, f)) {
                match &f.value {
                    FieldValue::Text(t) if !t.is_empty() => {
                        each_password(&facts, &f.field_id, &f.label, t.as_bytes())
                    }
                    FieldValue::Bytes(b) if !b.is_empty() => {
                        each_password(&facts, &f.field_id, &f.label, b)
                    }
                    _ => {}
                }
            }
            out.push(facts);
        }
        Ok(out)
    }

    /// Sets (`Some`) or clears (`None`) the dismissal of finding `key` on
    /// item `id`, as a new version of the item, and saves. Returns the new
    /// version. A trashed item is refused.
    pub(crate) fn security_set(
        &mut self,
        id: &str,
        key: &str,
        dismissal: Option<Dismissal>,
    ) -> Result<u64, String> {
        let item_id = parse_id(id)?;
        let item = self
            .holding(item_id)?
            .item(item_id)
            .map_err(|e| e.to_string())?;
        if !matches!(item.state, ItemState::Live | ItemState::Archived) {
            return Err("this item is in the trash; restore it first".into());
        }
        if key.is_empty() || key.len() > 64 || !key.bytes().all(|b| b.is_ascii_graphic()) {
            return Err(format!("'{key}' isn't a finding"));
        }
        let mut fields = self
            .holding(item_id)?
            .open_item(item_id)
            .map_err(|e| e.to_string())?;
        let mut security = ItemSecurity::read(&fields);
        let changed = match dismissal {
            Some(d) => security.dismissed.insert(key.to_string(), d) != Some(d),
            None => security.dismissed.remove(key).is_some(),
        };
        if !changed {
            return Ok(item.latest().map_err(|e| e.to_string())?.n);
        }
        fields.retain(|f| f.field_id != SECURITY_FIELD);
        if !security.dismissed.is_empty() {
            let text = serde_json::to_string(&security).map_err(|e| e.to_string())?;
            fields.push(PlainField::text(SECURITY_FIELD, SECURITY_LABEL, &text));
        }
        let n = self
            .holding_mut(item_id)?
            .update_item(item_id, fields)
            .map_err(|e| e.to_string())?;
        self.save()?;
        Ok(n)
    }

    /// The first web address saved on a live item, for "Change password".
    pub(crate) fn security_site(&self, id: &str) -> Result<Option<String>, String> {
        let item_id = parse_id(id)?;
        let fields = self
            .holding(item_id)?
            .open_item(item_id)
            .map_err(|e| e.to_string())?;
        Ok(fields.iter().find_map(|f| match &f.value {
            FieldValue::Url { value, .. } if f.kind == FieldKind::Url => Some(value.clone()),
            _ => None,
        }))
    }

    /// The data directory, for the security center's cache file.
    pub(crate) fn data_dir(&self) -> std::path::PathBuf {
        self.paths.data.clone()
    }

    /// The server and a live session token, when there is an account.
    pub(crate) fn account_session(&mut self) -> Option<Result<(String, String), String>> {
        let server = self.sync.as_ref()?.server.clone();
        // The local account's server is the app itself: it has no live
        // channel, no mail and no clock to compare (docs/plan/39-first-run.md §1.1).
        if lockdown_sync::is_local(&server) {
            return None;
        }
        Some(self.token().map(|t| (server, t)))
    }
}
