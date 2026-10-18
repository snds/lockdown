//! Per-item exposure tiers in the open session (`exposure-tiers`;
//! `lockdown_vault::exposure` has the rule, docs/plan/22-security-depth.md
//! §1 the flow): which tier an item is at and why, for every item at once
//! (a list's badges) or one (a detail's control), and the choice saved as a
//! new version of the item.

use std::collections::BTreeMap;

use lockdown_vault::exposure::{
    self, Exposure, ExposureContext, ExposureSource, default_for, high_risk,
};
use lockdown_vault::{Id, ItemKind, ItemState};
use serde::Serialize;

use super::{Session, parse_id};

/// One item's tier as a shell shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ExposureView {
    /// The item.
    pub id: String,
    /// `app-only`, `ask` or `fill`.
    pub tier: String,
    /// `chosen`, `crown-jewel`, `high-risk` or `default`.
    pub source: String,
    /// The tier with no choice made, so a shell can offer "back to the default".
    pub default_tier: String,
    /// Why the default asks (a high-risk login), in plain words.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The tiers the person may choose here, strongest first (`fill` is
    /// never offered for a crown jewel; a kind the browser never fills has
    /// none).
    pub choices: Vec<String>,
}

fn source_word(s: ExposureSource) -> &'static str {
    match s {
        ExposureSource::Chosen => "chosen",
        ExposureSource::CrownJewel => "crown-jewel",
        ExposureSource::HighRisk => "high-risk",
        ExposureSource::Default => "default",
    }
}

impl Session {
    /// What the default rule needs: the account's login name and every
    /// "Signed in with" name the persona's live logins give.
    pub(crate) fn exposure_context(&self) -> ExposureContext {
        let mut ctx = ExposureContext {
            account_login: self.sync.as_ref().map(|c| c.login.clone()),
            ..Default::default()
        };
        for vault in self.vaults() {
            for item in vault.items() {
                if item.kind != ItemKind::Login || item.state != ItemState::Live {
                    continue;
                }
                if let Ok(fields) = vault.open_item(item.item_id) {
                    ctx = ctx.with_providers([fields.as_slice()]);
                }
            }
        }
        ctx
    }

    fn view_of(&self, item_id: Id, ctx: &ExposureContext) -> Result<ExposureView, String> {
        let vault = self.holding(item_id)?;
        let item = vault.item(item_id).map_err(|e| e.to_string())?;
        let fields = vault.open_item(item_id).map_err(|e| e.to_string())?;
        let (tier, source) = exposure::effective(item.kind, item.class, &fields, ctx);
        let (default_tier, _) = default_for(item.kind, item.class, &fields, ctx);
        let reason = match source {
            ExposureSource::HighRisk => high_risk(item.kind, &fields, ctx),
            ExposureSource::CrownJewel => Some("a crown jewel always asks first".to_string()),
            _ => None,
        };
        let choices = Exposure::ALL
            .into_iter()
            .filter(|c| exposure::check_choice(item.kind, item.class, *c).is_ok())
            .map(|c| c.as_str().to_string())
            .collect();
        Ok(ExposureView {
            id: item_id.to_string(),
            tier: tier.as_str().into(),
            source: source_word(source).into(),
            default_tier: default_tier.as_str().into(),
            reason,
            choices,
        })
    }

    /// One item's tier.
    pub fn exposure(&self, id: &str) -> Result<ExposureView, String> {
        let ctx = self.exposure_context();
        self.view_of(parse_id(id)?, &ctx)
    }

    /// The tier in force for `item_id`, for the gates.
    pub(crate) fn exposure_tier(&self, item_id: Id) -> Result<Exposure, String> {
        let vault = self.holding(item_id)?;
        let item = vault.item(item_id).map_err(|e| e.to_string())?;
        if !item.kind.autofill_allowed() {
            // A kind the browser never fills keeps its own rules (D-row: the
            // tier only ever adds a check).
            return Ok(Exposure::AppOnly);
        }
        let fields = vault.open_item(item_id).map_err(|e| e.to_string())?;
        if item.class == lockdown_vault::ItemClass::CrownJewel
            || exposure::chosen(&fields).is_some()
        {
            return Ok(exposure::effective(
                item.kind,
                item.class,
                &fields,
                &ExposureContext::default(),
            )
            .0);
        }
        Ok(exposure::effective(item.kind, item.class, &fields, &self.exposure_context()).0)
    }

    /// Whether `item_id` is kept to the app: chosen so, or a kind the
    /// browser never fills. (No default is ever app-only, so this needs no
    /// look at the other logins.)
    pub(crate) fn kept_to_app(&self, item_id: Id) -> Result<bool, String> {
        let vault = self.holding(item_id)?;
        let item = vault.item(item_id).map_err(|e| e.to_string())?;
        if !item.kind.autofill_allowed() {
            return Ok(true);
        }
        let fields = vault.open_item(item_id).map_err(|e| e.to_string())?;
        Ok(exposure::chosen(&fields) == Some(Exposure::AppOnly))
    }

    /// Every visible item's tier, by id, for a list's badges. Items at the
    /// everyday tier by default are left out, so the answer stays small.
    pub fn exposures(&self) -> Result<Vec<ExposureView>, String> {
        let ctx = self.exposure_context();
        let mut out = BTreeMap::new();
        for vault in self.vaults() {
            for item in vault.items() {
                if item.kind.hidden() || item.state == ItemState::Tombstone {
                    continue;
                }
                let view = self.view_of(item.item_id, &ctx)?;
                if view.source == "default" && view.tier == "fill" {
                    continue;
                }
                out.insert(item.item_id, view);
            }
        }
        Ok(out.into_values().collect())
    }

    /// Saves the person's choice (`None` goes back to the default) as a new
    /// version; the gate in front of a weakening is the App's. Returns the
    /// item's view afterwards.
    pub fn set_exposure(
        &mut self,
        id: &str,
        choice: Option<Exposure>,
    ) -> Result<ExposureView, String> {
        let item_id = parse_id(id)?;
        self.holding_mut(item_id)?
            .set_exposure(item_id, choice)
            .map_err(|e| e.to_string())?;
        self.save()?;
        self.exposure(id)
    }
}
