//! Memory hygiene as a shell sees it (docs/plan/26-desktop-finish.md §7):
//! what the core keeps out of swap (`lockdown_crypto::locked`), for the
//! Settings screen and the doc's measurement. Nothing here holds a secret.

pub use lockdown_crypto::locked::MemoryReport;

use crate::App;

impl App {
    /// Pages locked now, refused locks, and whether core dumps are off.
    pub fn memory_report(&self) -> MemoryReport {
        lockdown_crypto::locked::report()
    }

    /// Turns core dumps off for this process; a shell calls it once at
    /// start. Answers whether they are off.
    pub fn harden_process() -> bool {
        lockdown_crypto::locked::harden_process()
    }
}
