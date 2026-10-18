//! The legacy-release rule for crown jewels (`high-risk-class`; 02-model §1
//! "Legacy … crown-jewel items stay out unless the owner opts them in", §3
//! "out of bulk export and legacy release unless the user opts each item
//! in"; 04-security §10; docs/plan/22-security-depth.md §8).
//!
//! An emergency grant hands the contact a vault's Vault Key after the
//! wait. A crown jewel's Item Key is under the Crown-Jewel Key, which never
//! leaves the owner, so by default the contact gets every item of the vault
//! but its crown jewels. An owner who wants one released opts it in
//! ([`LEGACY_OPT_IN`], a reserved field like the export opt-in, behind the
//! `Reveal` gate in the app); the owner's device then seals that item's
//! Item Key to the contact ([`Vault::seal_for_emergency`]), the server holds
//! it back with the grant, and the contact's vault takes it at release
//! ([`Vault::adopt_released_item`]) and opens that item, read only, like
//! the rest.
//!
//! Households and shared vaults never hold a crown jewel at all (D189);
//! that rule is untouched.

use ecosystem_identity::SymmetricKey;
use ecosystem_identity::kem::KemPublicKey;

use crate::field::{FieldKind, FieldValue, PlainField};
use crate::item::{ItemClass, ItemState};
use crate::keys::ItemKey;
use crate::sharing::{Party, ShareEnvelope, SharePlace, SharePurpose};
use crate::{Error, Id, Result};

use super::Vault;

/// The reserved field holding a crown jewel's legacy opt-in.
pub const LEGACY_OPT_IN: &str = "lockdown.legacy";

/// Whether `fields` carry the legacy opt-in, set to yes.
pub fn legacy_opted_in(fields: &[PlainField]) -> bool {
    fields
        .iter()
        .any(|f| f.field_id == LEGACY_OPT_IN && f.value == FieldValue::Boolean(true))
}

impl Vault {
    /// Opts a crown jewel in to (or out of) emergency release, as a new
    /// version so it syncs. Only a crown jewel has anything to opt in. The
    /// gate in front of turning it on is the app's.
    pub fn set_legacy_opt_in(&mut self, item_id: Id, on: bool) -> Result<u64> {
        let item = self.item(item_id)?;
        if item.class != ItemClass::CrownJewel {
            return Err(Error::Invalid(
                "only a crown jewel is kept back from emergency access, so there is nothing to opt in".into(),
            ));
        }
        if !matches!(item.state, ItemState::Live | ItemState::Archived) {
            return Err(Error::Invalid(
                "this item is in the trash; restore it before changing it".into(),
            ));
        }
        let current = item.latest()?.n;
        let mut fields = self.open_item(item_id)?;
        if legacy_opted_in(&fields) == on {
            return Ok(current);
        }
        fields.retain(|f| f.field_id != LEGACY_OPT_IN);
        if on {
            fields.push(PlainField::new(
                LEGACY_OPT_IN,
                FieldKind::Boolean,
                "Included in emergency access",
                FieldValue::Boolean(true),
            ));
        }
        self.update_item(item_id, fields)
    }

    /// The live crown jewels opted in to emergency release.
    pub fn legacy_items(&self) -> Vec<Id> {
        self.items
            .values()
            .filter(|i| i.class == ItemClass::CrownJewel && i.state == ItemState::Live)
            .filter(|i| self.open_item(i.item_id).is_ok_and(|f| legacy_opted_in(&f)))
            .map(|i| i.item_id)
            .collect()
    }

    /// Seals an opted-in crown jewel's Item Key to an emergency contact,
    /// for the server to hold back with the grant until its wait runs out.
    pub fn seal_for_emergency(
        &self,
        item_id: Id,
        to: Party,
        to_kem: &KemPublicKey,
        from_account: Id,
        grant_id: Id,
    ) -> Result<ShareEnvelope> {
        let item = self.item(item_id)?;
        if item.class != ItemClass::CrownJewel || !legacy_opted_in(&self.open_item(item_id)?) {
            return Err(Error::Invalid(
                "only a crown jewel its owner opted in is released to an emergency contact".into(),
            ));
        }
        let key = self.item_key(item)?;
        let place = SharePlace {
            purpose: SharePurpose::Emergency,
            vault_id: self.ctx.vault_id,
            item_id: Some(item_id),
            key_id: item_id,
            to,
            from_account,
            grant_id: Some(grant_id),
        };
        ShareEnvelope::seal(&key, to_kem, place, self.device_id, self.signer.as_ref())
    }

    /// Takes a crown jewel's Item Key an emergency release handed over, so
    /// that item opens here (read only; never saved, never synced).
    pub fn adopt_released_item(&mut self, item_id: Id, key: ItemKey) -> Result<()> {
        let item = self.item(item_id)?;
        if item.class != ItemClass::CrownJewel {
            return Err(Error::Invalid(
                "a released key is only ever for a crown jewel".into(),
            ));
        }
        self.released_items
            .insert(item_id, ItemKey::from_bytes(*key.expose()));
        Ok(())
    }

    /// Whether this vault can open `item_id` (a crown jewel it holds no key
    /// for, in a vault an emergency release opened, can't).
    pub fn can_open(&self, item_id: Id) -> bool {
        match self.items.get(&item_id) {
            Some(i) if i.class == ItemClass::CrownJewel => {
                self.crown_jewel_key.is_some() || self.released_items.contains_key(&item_id)
            }
            Some(_) => true,
            None => false,
        }
    }
}
