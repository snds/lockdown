//! What the session knows about the post-quantum rollout
//! (`crate::pq_status`): the identity's records, and whether the account's
//! server confirmed them since unlock.

use lockdown_vault::pq_status::{Reported, Rollout, rollout};

use super::Session;

impl Session {
    /// Where the rollout stands, from the identity's records as the server
    /// last confirmed them in this session.
    pub(crate) fn pq_rollout(&self) -> Result<Rollout, String> {
        let reported = if self.sync.is_none() {
            Reported::NoAccount
        } else if !self.identity_confirmed {
            Reported::Unreached
        } else if self.synced.verified().is_none() {
            Reported::Silent
        } else {
            Reported::Confirmed
        };
        rollout(&self.identity, reported).map_err(|e| e.to_string())
    }
}
