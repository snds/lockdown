//! Bridges, imports and monitoring (docs/plan/23-bridges.md): what this
//! slice adds to the app's command surface.
//!
//! - [`clock`] — code clock-skew correction against the account's server.
//! - [`federation`] — the federated sign-in map: which logins are entered
//!   through which provider, the blast-radius view and recovery warnings.
//! - [`fixes`] — per-site fill fixes, the person's own (synced on the
//!   login) and the signed community list.
//! - [`aliases`] — email aliases through six services, their API keys
//!   kept as crown jewels, and alias monitoring.
//! - [`incidents`] — the seven guided incident flows, with their affected
//!   items and the person's progress.
//! - [`guides`] — set-up cards for sites that offer two-step sign-in.
//! - [`emergency`] — the emergency sheet and the last-backup line.
//!
//! The rules the rest of the app keeps hold here too: a secret leaves the
//! core only through the existing gates (an alias service's key through
//! `Use`, and its use counts for the ranking, D151); the webview gets
//! words, ids, titles and addresses, never a key; what comes from a page
//! or a network is checked before anything is written.

pub mod age_export;
pub mod aliases;
pub mod clock;
pub mod emergency;
pub mod federation;
pub mod fixes;
pub mod guides;
pub mod incidents;

use lockdown_codes::skew::Skew;

/// What this slice keeps in the core between calls.
#[derive(Debug, Default)]
pub(crate) struct BridgesState {
    /// The last checked clock sample and when it was taken.
    pub clock: Option<(i64, Skew)>,
    /// Sign-ins through a provider the extension saw that need the
    /// person's yes (in memory only; dropped on lock).
    pub pending: Vec<federation::Pending>,
    /// The next pending observation's number.
    pub next_pending: u64,
}

impl BridgesState {
    /// The time codes are worked out from: `now` corrected by the last
    /// checked sample when `correct` (the setting) is on.
    pub fn code_time(&self, now: i64, correct: bool) -> u64 {
        let now = now.max(0) as u64;
        match (&self.clock, correct) {
            (Some((_, skew)), true) => skew.apply(now),
            _ => now,
        }
    }

    /// What a lock forgets: what the extension saw.
    pub fn forget(&mut self) {
        self.pending.clear();
    }
}
