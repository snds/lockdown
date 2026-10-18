//! Checking a contact's keys with a phrase said aloud
//! (`key-transparency`; docs/plan/22-security-depth.md §9). The exchange
//! is `lockdown_vault::phrase_check`; the session keeps each check open
//! until it ends or the app locks. A match marks the contact verified, the
//! same mark comparing the thirty digits (or scanning their QR code into
//! [`App::verify_contact`]) leaves.

pub use crate::session::sharing::phrase::{PhraseCheckView, PhraseIncoming};
use crate::{App, AppResult};

impl App {
    /// Opens a check with `login` and returns the phrase to tell them.
    pub fn phrase_invite(&self, login: &str) -> AppResult<PhraseCheckView> {
        let login = login.to_string();
        self.with_session(move |s| s.phrase_invite(&login))
    }

    /// Checks others opened with this person.
    pub fn phrase_incoming(&self) -> AppResult<Vec<PhraseIncoming>> {
        self.with_session(|s| s.phrase_incoming())
    }

    /// Answers a check with the phrase the other person said.
    pub fn phrase_answer(&self, verify_id: &str, phrase: &str) -> AppResult<PhraseCheckView> {
        let (verify_id, phrase) = (
            verify_id.to_string(),
            zeroize::Zeroizing::new(phrase.to_string()),
        );
        self.with_session(move |s| s.phrase_answer(&verify_id, &phrase))
    }

    /// Moves a check on; a shell calls it every second or two while open.
    pub fn phrase_poll(&self, verify_id: &str) -> AppResult<PhraseCheckView> {
        let verify_id = verify_id.to_string();
        self.with_session(move |s| s.phrase_poll(&verify_id))
    }

    /// Every check under way here.
    pub fn phrase_checks(&self) -> AppResult<Vec<PhraseCheckView>> {
        self.with_session(|s| Ok(s.phrase_checks()))
    }

    /// Drops a check.
    pub fn phrase_cancel(&self, verify_id: &str) -> AppResult<()> {
        let verify_id = verify_id.to_string();
        self.with_session(move |s| s.phrase_cancel(&verify_id))
    }
}
