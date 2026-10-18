//! Quick access (docs/plan/26-desktop-finish.md §1): the one search every
//! shell asks — the desktop's palette and its quick-access window, the web
//! vault's palette, the browser extension's popup — over the index in
//! `session::quick`, ranked and forgiving of a typo, never over a secret.
//!
//! Its actions are the ones that already exist, so they keep their gates:
//! a copy is `copy_secret` / `copy_code` (`Use` for a crown jewel), a code
//! shown in the palette is [`App::quick_code`] (the code valid now, as the
//! Codes view has it, through the same `Use` gate and counted as a use),
//! and "Fill in the browser" is [`App::quick_fill`]: the core opens nothing
//! itself; it remembers, for two minutes, that the person asked to fill
//! this login on this site, and the browser extension's menu on that site
//! lists it first. The fill itself is the extension's ordinary fill, with
//! its three decisions and its armed click (D158, D159).

use std::time::{Duration, Instant};

pub use crate::session::quick::{MAX_RESULTS, QuickHit};
use crate::{App, AppError, AppResult, GateInput, Revealed};

/// How long a "Fill in the browser" waits for the extension.
pub const FILL_WAIT: Duration = Duration::from_secs(120);

/// A fill the person started from quick access, waiting for the browser.
#[derive(Clone, Debug)]
pub(crate) struct PendingFill {
    pub item_id: String,
    pub host: String,
    pub at: Instant,
}

/// What a "Fill in the browser" answers: the address the shell opens.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct FillStarted {
    /// The login's own website (`http` or `https` only).
    pub url: String,
    /// Its host, for the shell's sentence.
    pub host: String,
    /// Seconds the extension has to pick it up.
    pub waits_secs: u64,
}

impl App {
    /// The live items matching `query`, best first; an empty query lists
    /// them favourites first, then by this device's use, then A–Z.
    pub fn search(&self, query: &str, limit: usize) -> AppResult<Vec<QuickHit>> {
        let query = query.to_string();
        self.with_session(move |s| s.quick_search(&query, limit))
    }

    /// The code valid now for one code field, to show in quick access:
    /// the same value and gate as copying it (`code_now`), counted as a use.
    pub fn quick_code(
        &self,
        item_id: &str,
        field_id: &str,
        gate: GateInput,
    ) -> AppResult<Revealed> {
        self.code_now(item_id, field_id, false, gate)
    }

    /// Starts "Fill in the browser" for a live login: answers its website
    /// for the shell to open and remembers the request for [`FILL_WAIT`],
    /// so the extension's menu on that site offers this login first. A
    /// crown jewel and an item kept to the app are never filled from a
    /// browser (02-model §3; D245), and are refused here in words.
    pub fn quick_fill(&self, item_id: &str) -> AppResult<FillStarted> {
        let rows = self.list()?;
        let row = rows
            .into_iter()
            .find(|r| r.id == item_id && r.state == "live")
            .ok_or_else(|| AppError::refused("that item isn't in the vault"))?;
        if row.kind != "login" {
            return Err(AppError::refused("only a login fills in the browser"));
        }
        if row.crown_jewel {
            return Err(AppError::refused(
                "a protected item never fills in a browser; copy it from Lockdown instead",
            ));
        }
        let exposure = self.exposure(item_id)?;
        if exposure.tier == "app-only" {
            return Err(AppError::refused(
                "this login is kept to the app; Lockdown doesn't fill it in a browser",
            ));
        }
        let origin = lockdown_matcher::Origin::parse(&row.site)
            .map_err(|_| AppError::refused("this login has no website to open"))?;
        if origin.scheme != "https" && origin.scheme != "http" {
            return Err(AppError::refused(
                "this login's website isn't a web address",
            ));
        }
        let host = origin.host.clone();
        let url = row.site.clone();
        let pending = PendingFill {
            item_id: item_id.to_string(),
            host: host.clone(),
            at: Instant::now(),
        };
        self.call(move |core| {
            core.quick_fills
                .retain(|p| p.at.elapsed() < FILL_WAIT && p.item_id != pending.item_id);
            core.quick_fills.push(pending);
            Ok(())
        })?;
        self.note_use(item_id)?;
        Ok(FillStarted {
            url,
            host,
            waits_secs: FILL_WAIT.as_secs(),
        })
    }

    /// The login the person asked quick access to fill on `host` in the
    /// last [`FILL_WAIT`], if any; the browser extension's matches put it
    /// first. Asking does not use it up: the person still picks it in the
    /// menu, and the fill is decided as every fill is.
    pub fn pending_fill(&self, host: &str) -> AppResult<Option<String>> {
        let host = host.to_string();
        self.call(move |core| {
            core.quick_fills.retain(|p| p.at.elapsed() < FILL_WAIT);
            Ok(core
                .quick_fills
                .iter()
                .find(|p| p.host == host)
                .map(|p| p.item_id.clone()))
        })
    }
}
