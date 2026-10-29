//! The post-quantum status line on the Security screen (D346;
//! docs/plan/30-pq-status-line.md): where the rollout of the stronger
//! signatures stands for this account, in words, from
//! [`lockdown_vault::pq_status::rollout`].
//!
//! The answer rests on the identity's signed device records. It is "on"
//! or "waiting" only once the account's server confirmed those records
//! since this device unlocked (a sync's record pull); before that, with no
//! account, or when the server holds no record set, it is "can't check
//! right now", never a stale "on". Each device is named by its id and the
//! name the Devices screen shows, and nothing else about it leaves here.

use lockdown_vault::pq_status::{Rollout, RolloutDevice, RolloutState, Unknown};
use serde::Serialize;

use crate::{App, AppResult};

/// One device holding the stronger signatures back.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PqDevice {
    /// Its id, as the Devices screen keys it.
    pub id: String,
    /// Its name, as the Devices screen shows it.
    pub name: String,
}

/// The status line, as every shell shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PqStatusView {
    /// `on`, `waiting` or `unknown`.
    pub state: &'static str,
    /// Why it is `unknown`: `no_account`, `unreached`, `silent`,
    /// `not_published` or `managed_elsewhere`; `null` otherwise.
    pub reason: Option<&'static str>,
    /// The line: "Quantum-safe signatures: on", "…: waiting for …",
    /// "…: can't check right now".
    pub headline: String,
    /// One short sentence under it.
    pub explanation: &'static str,
    /// Devices below level 1 (they hold back the account's key too).
    pub without_level_1: Vec<PqDevice>,
    /// Devices at level 1 but below level 2 (they hold back the vault
    /// heads' signature only). Never in `without_level_1` too.
    pub without_level_2: Vec<PqDevice>,
}

impl From<&Rollout> for PqStatusView {
    fn from(r: &Rollout) -> Self {
        let (state, reason) = match r.state {
            RolloutState::On => ("on", None),
            RolloutState::Waiting => ("waiting", None),
            RolloutState::Unknown(why) => (
                "unknown",
                Some(match why {
                    Unknown::NoAccount => "no_account",
                    Unknown::Unreached => "unreached",
                    Unknown::Silent => "silent",
                    Unknown::NotPublished => "not_published",
                    Unknown::ManagedElsewhere => "managed_elsewhere",
                }),
            ),
        };
        let devices = |list: &[RolloutDevice]| {
            list.iter()
                .map(|d| PqDevice {
                    id: d.id.to_string(),
                    name: d.name.clone(),
                })
                .collect()
        };
        Self {
            state,
            reason,
            headline: r.headline(),
            explanation: r.explanation(),
            without_level_1: devices(&r.without_level_1),
            without_level_2: devices(&r.without_level_2),
        }
    }
}

impl App {
    /// Where the post-quantum signature rollout stands for this account.
    /// Reads only what this device holds; the sync that runs on unlock (or
    /// the next one) is what confirms it with the server.
    pub fn pq_status(&self) -> AppResult<PqStatusView> {
        self.with_session(|s| s.pq_rollout().map(|r| PqStatusView::from(&r)))
    }
}
