//! Per-item exposure tiers as the shells use them (`exposure-tiers`;
//! docs/plan/22-security-depth.md §1): read an item's tier, choose another,
//! and the gates every path that hands out a secret runs.
//!
//! - **Ask each time** puts the `Use` gate (D101: the platform's check, or
//!   the account password where there is none) in front of every reveal,
//!   copy, code and browser fill of an item that is not a crown jewel (a
//!   crown jewel already has its own, stronger gates, which stay).
//! - **Only in the app** keeps the item away from the browser extension
//!   (`browser`, `passkeys`): never listed, never filled, its passkeys never
//!   offered.
//! - **Weakening** a tier (to one that asks less, or back to a default that
//!   asks less than the choice did) goes through the `Reveal` gate, the
//!   account password where one exists, as taking an item out of the
//!   crown-jewel class does (D148). Strengthening asks nothing.

use std::time::Instant;

use lockdown_vault::exposure::Exposure;

pub use crate::session::exposure::ExposureView;
use crate::{App, AppError, AppResult, ApprovalKind, Core, GateInput};

fn item_id(id: &str) -> AppResult<lockdown_vault::Id> {
    lockdown_vault::Id::parse(id).map_err(|_| AppError::refused(format!("'{id}' isn't an item id")))
}

impl Core {
    /// The `Use` gate for an item at **Ask each time** (crown jewels keep
    /// their own gates and are left to them).
    pub(crate) fn exposure_gate(
        &mut self,
        id: &str,
        reason: &str,
        gate: &GateInput,
    ) -> AppResult<()> {
        let item = item_id(id)?;
        let session = self.session()?;
        if session.is_crown_jewel(id).map_err(AppError::refused)? {
            return Ok(());
        }
        if session.exposure_tier(item).map_err(AppError::refused)? == Exposure::Ask {
            self.approve(ApprovalKind::Use, reason, gate)?;
        }
        Ok(())
    }

    /// Refuses a browser request for an item kept to the app.
    pub(crate) fn refuse_app_only(&mut self, id: &str) -> AppResult<()> {
        let item = item_id(id)?;
        if self
            .session()?
            .kept_to_app(item)
            .map_err(AppError::refused)?
        {
            return Err(AppError::refused(
                "this item is kept to the Lockdown app; open it there to use it",
            ));
        }
        Ok(())
    }
}

impl App {
    /// One item's protection level, where it comes from, and what may be chosen.
    pub fn exposure(&self, id: &str) -> AppResult<ExposureView> {
        let id = id.to_string();
        self.with_session(move |s| s.exposure(&id))
    }

    /// Every item not at the everyday level, for a list's badges.
    pub fn exposures(&self) -> AppResult<Vec<ExposureView>> {
        self.with_session(|s| s.exposures())
    }

    /// Chooses an item's protection level (`app-only`, `ask`, `fill`), or
    /// with `None` goes back to its default. A change that asks less than
    /// before goes through the `Reveal` gate first.
    pub fn set_exposure(
        &self,
        id: &str,
        tier: Option<&str>,
        gate: GateInput,
    ) -> AppResult<ExposureView> {
        let id = id.to_string();
        let choice = match tier {
            Some(t) => Some(Exposure::parse(t).map_err(|e| AppError::refused(e.to_string()))?),
            None => None,
        };
        self.call(move |core| {
            core.clock.touch(Instant::now());
            let now = core.session()?.exposure(&id).map_err(AppError::refused)?;
            let before =
                Exposure::parse(&now.tier).map_err(|e| AppError::refused(e.to_string()))?;
            let after = match choice {
                Some(c) => c,
                None => Exposure::parse(&now.default_tier)
                    .map_err(|e| AppError::refused(e.to_string()))?,
            };
            if after > before {
                core.approve(
                    ApprovalKind::Reveal,
                    "let this item be used with fewer checks",
                    &gate,
                )?;
            }
            core.session()?
                .set_exposure(&id, choice)
                .map_err(AppError::refused)
        })
    }
}
