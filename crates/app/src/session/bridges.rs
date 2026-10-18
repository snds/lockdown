//! What the bridges slice reads from and writes to the open vault
//! (docs/plan/23-bridges.md): the facts of every live login (websites,
//! user name, how it is signed in to, its fill fix), the alias services'
//! settings and keys, and one writer for the slice's reserved fields.
//!
//! The reserved fields are Lockdown's own (`lockdown.` prefix,
//! [`lockdown_vault::is_reserved`]): synced, in the item's history and in
//! a Lockdown export like any field, kept by every edit without being
//! named, never shown by `show` and refused when a shell names one, as
//! the security center's `lockdown.security` is (D132).

use lockdown_vault::sharing::Right;
use lockdown_vault::{FieldKind, FieldValue, Id, ItemClass, ItemKind, ItemState, PlainField};
use zeroize::Zeroizing;

use super::{Session, parse_id, text_of};

/// How a login is signed in to through another account (the federated
/// sign-in map): `{provider, via, at}`.
pub(crate) const SIGNIN_FIELD: &str = "lockdown.signin";
/// On a provider's own login: which provider it is the account for.
pub(crate) const PROVIDER_FIELD: &str = "lockdown.provider";
/// A login's fill fix: `{never, fields: [{match, role}]}`.
pub(crate) const FILL_FIELD: &str = "lockdown.fill";
/// An alias service's settings, on its API key's item.
pub(crate) const ALIAS_FIELD: &str = "lockdown.alias";

/// The labels an export reader sees for them.
fn label_for(field_id: &str) -> &'static str {
    match field_id {
        SIGNIN_FIELD => "Signed in through",
        PROVIDER_FIELD => "Sign-in provider account",
        FILL_FIELD => "Fill fix",
        ALIAS_FIELD => "Alias service",
        _ => "Lockdown setting",
    }
}

/// One live login, as the slice reads it: facts, never a value.
#[derive(Clone, Debug)]
pub(crate) struct LoginFacts {
    pub item_id: Id,
    pub title: String,
    pub crown_jewel: bool,
    /// Every website saved on it, as typed.
    pub urls: Vec<String>,
    pub username: String,
    /// It holds a password of its own.
    pub has_password: bool,
    /// It holds a one-time code or a passkey.
    pub has_second_step: bool,
    /// `sign_in_method` and `sign_in_provider` as the editor shows them.
    pub method: String,
    pub provider_text: String,
    /// The reserved fields, as their JSON text.
    pub signin: Option<String>,
    pub provider_of: Option<String>,
    pub fill: Option<String>,
    /// The person may change it (their own vault, or edit rights).
    pub editable: bool,
}

/// An alias service's API key item.
#[derive(Clone, Debug)]
pub(crate) struct AliasItem {
    pub item_id: Id,
    pub title: String,
    pub config: String,
}

impl Session {
    fn may_edit(&self, item_id: Id) -> bool {
        !matches!(
            self.item_right(item_id),
            Some(Right::View | Right::FillOnly)
        )
    }

    /// Every live (not trashed, not archived) login's facts.
    pub(crate) fn bridge_logins(&self) -> Result<Vec<LoginFacts>, String> {
        let mut out = Vec::new();
        for vault in self.vaults() {
            for item in vault.items() {
                if item.kind != ItemKind::Login || item.state != ItemState::Live {
                    continue;
                }
                let fields = vault.open_item(item.item_id).map_err(|e| e.to_string())?;
                let filled = |id: &str| {
                    fields.iter().any(|f| {
                        f.field_id == id
                            && match &f.value {
                                FieldValue::Text(t) => !t.is_empty(),
                                FieldValue::Bytes(b) => !b.is_empty(),
                                _ => false,
                            }
                    })
                };
                out.push(LoginFacts {
                    item_id: item.item_id,
                    title: text_of(&fields, "title").unwrap_or_default(),
                    crown_jewel: item.class == ItemClass::CrownJewel,
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
                    has_password: filled("pw"),
                    has_second_step: fields
                        .iter()
                        .any(|f| matches!(f.kind, FieldKind::Totp | FieldKind::Passkey)),
                    method: text_of(&fields, "sign_in_method").unwrap_or_default(),
                    provider_text: text_of(&fields, "sign_in_provider").unwrap_or_default(),
                    signin: text_of(&fields, SIGNIN_FIELD),
                    provider_of: text_of(&fields, PROVIDER_FIELD),
                    fill: text_of(&fields, FILL_FIELD),
                    editable: false,
                });
            }
        }
        for f in &mut out {
            f.editable = self.may_edit(f.item_id);
        }
        Ok(out)
    }

    /// Every live item holding a one-time code: id and title.
    pub(crate) fn bridge_code_items(&self) -> Result<Vec<(Id, String, bool)>, String> {
        let mut out = Vec::new();
        for vault in self.vaults() {
            for item in vault.items() {
                if item.kind.hidden() || item.state != ItemState::Live {
                    continue;
                }
                let fields = vault.open_item(item.item_id).map_err(|e| e.to_string())?;
                if fields.iter().any(|f| f.kind == FieldKind::Totp) {
                    out.push((
                        item.item_id,
                        text_of(&fields, "title").unwrap_or_default(),
                        item.class == ItemClass::CrownJewel,
                    ));
                }
            }
        }
        Ok(out)
    }

    /// Every live crown jewel: id and title.
    pub(crate) fn bridge_crown_jewels(&self) -> Vec<(Id, String)> {
        let mut out = Vec::new();
        for vault in self.vaults() {
            for item in vault.items() {
                if item.class == ItemClass::CrownJewel
                    && item.state == ItemState::Live
                    && !item.kind.hidden()
                    && let Ok(fields) = vault.open_item(item.item_id)
                {
                    out.push((item.item_id, text_of(&fields, "title").unwrap_or_default()));
                }
            }
        }
        out
    }

    /// Sets (`Some`) or removes (`None`) one of the slice's reserved fields
    /// on item `id`, and with it any template text fields named in `also`
    /// (the federated map sets `sign_in_method` and `sign_in_provider`).
    /// Nothing changed is no new version. Returns the item's version.
    pub(crate) fn bridge_set(
        &mut self,
        id: Id,
        field_id: &'static str,
        value: Option<String>,
        also: &[(&str, &str, &str)],
    ) -> Result<u64, String> {
        if !lockdown_vault::is_reserved(field_id) {
            return Err(format!("{field_id} isn't one of Lockdown's own fields"));
        }
        if !self.may_edit(id) {
            return Err("this item is shared with you without the right to change it".into());
        }
        let item = self.holding(id)?.item(id).map_err(|e| e.to_string())?;
        if !matches!(item.state, ItemState::Live | ItemState::Archived) {
            return Err("this item is in the trash; restore it first".into());
        }
        let current = item.latest().map_err(|e| e.to_string())?.n;
        let mut fields = self.holding(id)?.open_item(id).map_err(|e| e.to_string())?;
        let before = fields.clone();
        fields.retain(|f| f.field_id != field_id);
        if let Some(v) = value {
            fields.push(PlainField::text(field_id, label_for(field_id), &v));
        }
        for (fid, label, text) in also {
            match fields.iter_mut().find(|f| f.field_id == *fid) {
                Some(f) => f.value = FieldValue::Text((*text).to_string()),
                None => fields.push(PlainField::text(fid, label, text)),
            }
        }
        if fields == before {
            return Ok(current);
        }
        let n = self
            .holding_mut(id)?
            .update_item(id, fields)
            .map_err(|e| e.to_string())?;
        self.save()?;
        Ok(n)
    }

    /// Every live alias-service key item.
    pub(crate) fn bridge_alias_items(&self) -> Result<Vec<AliasItem>, String> {
        let mut out = Vec::new();
        for vault in self.vaults() {
            for item in vault.items() {
                if item.kind != ItemKind::ApiCredential || item.state != ItemState::Live {
                    continue;
                }
                let fields = vault.open_item(item.item_id).map_err(|e| e.to_string())?;
                if let Some(config) = text_of(&fields, ALIAS_FIELD) {
                    out.push(AliasItem {
                        item_id: item.item_id,
                        title: text_of(&fields, "title").unwrap_or_default(),
                        config,
                    });
                }
            }
        }
        Ok(out)
    }

    /// The API key of an alias-service item, as bytes the caller wipes.
    /// The caller has run the `Use` gate (the item is a crown jewel).
    pub(crate) fn bridge_alias_key(
        &self,
        id: &str,
    ) -> Result<(Zeroizing<Vec<u8>>, String), String> {
        let item_id = parse_id(id)?;
        let fields = self
            .holding(item_id)?
            .open_item(item_id)
            .map_err(|e| e.to_string())?;
        let config = text_of(&fields, ALIAS_FIELD)
            .ok_or_else(|| "this item isn't an alias service's key".to_string())?;
        let key = fields
            .iter()
            .find(|f| f.field_id == "api.key")
            .and_then(|f| match &f.value {
                FieldValue::Bytes(b) => Some(Zeroizing::new(b.to_vec())),
                FieldValue::Text(t) => Some(Zeroizing::new(t.as_bytes().to_vec())),
                _ => None,
            })
            .filter(|k| !k.is_empty())
            .ok_or_else(|| "this alias service has no API key saved".to_string())?;
        Ok((key, config))
    }

    /// The persona's id, for the device's sealed files.
    pub(crate) fn bridge_persona(&self) -> Id {
        self.vault().context().persona_id
    }

    /// Seals `plain` under the persona's App Data Key with `aad`.
    pub(crate) fn bridge_seal(&self, aad: &[u8], plain: &[u8]) -> Result<Vec<u8>, String> {
        let sealed = lockdown_crypto::aead::seal(&self.opened.keys.app_data_key, aad, plain)
            .map_err(|e| e.to_string())?;
        lockdown_crypto::cbor::to_canonical(&sealed).map_err(|e| e.to_string())
    }

    /// Opens what [`Self::bridge_seal`] sealed; `None` when it doesn't open.
    pub(crate) fn bridge_open(&self, aad: &[u8], bytes: &[u8]) -> Option<Zeroizing<Vec<u8>>> {
        let sealed: lockdown_crypto::aead::Sealed =
            lockdown_crypto::cbor::from_slice(bytes).ok()?;
        lockdown_crypto::aead::open(&self.opened.keys.app_data_key, aad, &sealed)
            .ok()
            .map(|p| Zeroizing::new(p.to_vec()))
    }
}

impl Session {
    /// Every vault (or the one `vault` names) as a Lockdown document,
    /// sealed with age to `recipients` (docs/plan/23-bridges.md §11). The
    /// same document a Lockdown export holds, so the same crown jewels are
    /// left out (D121) and listed.
    pub(crate) fn export_to_recipients(
        &self,
        recipients: &[String],
        vault: Option<&str>,
    ) -> Result<(Vec<u8>, crate::ExportPreview), String> {
        let only = self.vault_choice(vault)?;
        let (doc, mut preview) = self.backup_document(only)?;
        let plain = doc.to_bytes().map_err(|e| e.to_string())?;
        let sealed = lockdown_cxf::age_file::seal_to(&plain, recipients)?;
        preview.encrypted = true;
        Ok((sealed, preview))
    }
}
