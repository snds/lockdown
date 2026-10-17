//! The copy rule (03-architecture §8, design brief §4): a copied secret is
//! cleared from the clipboard after a timer, and only if the clipboard
//! still holds our value, so a later copy of the person's own text is
//! never destroyed. Nothing here keeps the secret: what is remembered is a
//! hash of what was written and a generation number, so the timer of an
//! older copy cannot clear a newer one.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// The default time a copied secret stays on the clipboard.
pub const DEFAULT_CLEAR_AFTER: Duration = Duration::from_secs(30);
/// The shortest and longest a person may set (design brief §4).
pub const MIN_CLEAR_AFTER: Duration = Duration::from_secs(10);
pub const MAX_CLEAR_AFTER: Duration = Duration::from_secs(90);

/// What the shell remembers about the last copy.
#[derive(Default)]
pub struct ClipboardGuard {
    generation: AtomicU64,
    written: Mutex<Option<[u8; 32]>>,
}

impl ClipboardGuard {
    /// Records that `text` was just written. Returns the copy's generation,
    /// which the clear timer hands back to [`ClipboardGuard::should_clear`].
    pub fn note_written(&self, text: &[u8]) -> u64 {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        if let Ok(mut w) = self.written.lock() {
            *w = Some(*blake3::hash(text).as_bytes());
        }
        generation
    }

    /// Whether the timer of copy `generation` may clear the clipboard that
    /// now holds `current`: only when no later copy happened and the
    /// clipboard still holds what that copy wrote.
    pub fn should_clear(&self, generation: u64, current: Option<&[u8]>) -> bool {
        if self.generation.load(Ordering::SeqCst) != generation {
            return false;
        }
        let Some(current) = current else {
            return false;
        };
        match self.written.lock() {
            Ok(w) => w.as_ref() == Some(blake3::hash(current).as_bytes()),
            Err(_) => false,
        }
    }

    /// Forgets the last copy, after a clear or a lock.
    pub fn forget(&self) {
        if let Ok(mut w) = self.written.lock() {
            *w = None;
        }
    }
}

/// Clamps a requested clear time into the allowed range; `None` means the
/// default.
pub fn clear_after(requested_secs: Option<u64>) -> Duration {
    match requested_secs {
        None => DEFAULT_CLEAR_AFTER,
        Some(s) => Duration::from_secs(s).clamp(MIN_CLEAR_AFTER, MAX_CLEAR_AFTER),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clears_only_its_own_copy_while_it_is_still_there() {
        let guard = ClipboardGuard::default();
        let first = guard.note_written(b"hunter2");
        assert!(guard.should_clear(first, Some(b"hunter2")));
        // The person copied their own text since: leave it alone.
        assert!(!guard.should_clear(first, Some(b"a shopping list")));
        // The clipboard is empty: nothing to do.
        assert!(!guard.should_clear(first, None));
        // A newer copy happened: the old timer stands down, the new one acts.
        let second = guard.note_written(b"correct horse");
        assert!(!guard.should_clear(first, Some(b"correct horse")));
        assert!(guard.should_clear(second, Some(b"correct horse")));
        guard.forget();
        assert!(!guard.should_clear(second, Some(b"correct horse")));
    }

    #[test]
    fn the_clear_time_stays_in_range() {
        assert_eq!(clear_after(None), DEFAULT_CLEAR_AFTER);
        assert_eq!(clear_after(Some(1)), MIN_CLEAR_AFTER);
        assert_eq!(clear_after(Some(45)), Duration::from_secs(45));
        assert_eq!(clear_after(Some(600)), MAX_CLEAR_AFTER);
    }
}
