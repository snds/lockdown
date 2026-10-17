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

/// What the shell remembers about the last copy: a BLAKE3 hash of the
/// text and the copy's generation number. Never the text. The generation
/// is also the name the webview holds for a copy (unicron's SecretField
/// copies by reference and hands it back to `clipboard_holds`), so nothing
/// derived from the secret, not even a digest, crosses to the webview.
#[derive(Default)]
pub struct ClipboardGuard {
    generation: AtomicU64,
    written: Mutex<Option<Written>>,
}

#[derive(Clone, Copy)]
struct Written {
    blake3: [u8; 32],
    generation: u64,
}

impl ClipboardGuard {
    /// Records that `text` was just written. Returns the copy's generation,
    /// which the clear timer hands back to [`ClipboardGuard::should_clear`].
    pub fn note_written(&self, text: &[u8]) -> u64 {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        if let Ok(mut w) = self.written.lock() {
            *w = Some(Written {
                blake3: *blake3::hash(text).as_bytes(),
                generation,
            });
        }
        generation
    }

    /// Whether the clipboard that now holds `current` still holds what the
    /// shell last wrote (`clipboard_holds`, for the component's own timer).
    /// With `copy` (a generation from [`ClipboardGuard::note_written`]) the
    /// question is about that copy in particular: a field asking about an
    /// older copy after a newer one hears no. The number is only ever
    /// compared with the shell's own record, never used to test what the
    /// clipboard holds, so the webview cannot use this to recognise text
    /// it did not copy.
    pub fn holds(&self, current: Option<&[u8]>, copy: Option<u64>) -> bool {
        let Some(current) = current else {
            return false;
        };
        let Ok(w) = self.written.lock() else {
            return false;
        };
        let Some(written) = w.as_ref() else {
            return false;
        };
        if copy.is_some_and(|c| c != written.generation) {
            return false;
        }
        written.blake3 == *blake3::hash(current).as_bytes()
    }

    /// Whether a clear the webview asks for (a component's countdown ran
    /// out: `clipboard_clear`) may run: only for copy `copy`, the number
    /// the shell gave that copy, while it is still the shell's last write
    /// and still on the clipboard. A clear for an older copy, a number the
    /// shell never gave, or a clipboard the person has copied over since
    /// does nothing, so one control's countdown never clears another's
    /// copy or the person's own text (D105, D115).
    pub fn may_clear(&self, copy: u64, current: Option<&[u8]>) -> bool {
        self.holds(current, Some(copy))
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
            Ok(w) => w
                .as_ref()
                .is_some_and(|w| w.blake3 == *blake3::hash(current).as_bytes()),
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
    fn clipboard_holds_answers_for_the_shells_own_copy_only() {
        let guard = ClipboardGuard::default();
        // Nothing written yet: nothing is ours.
        assert!(!guard.holds(Some(b"hunter2"), None));
        assert!(!guard.holds(Some(b"hunter2"), Some(1)));
        let ours = guard.note_written(b"hunter2");
        assert!(guard.holds(Some(b"hunter2"), None));
        assert!(guard.holds(Some(b"hunter2"), Some(ours)));
        // A copy the shell never made is not ours.
        assert!(!guard.holds(Some(b"hunter2"), Some(ours + 1)));
        // The person copied their own text since.
        assert!(!guard.holds(Some(b"a shopping list"), Some(ours)));
        assert!(!guard.holds(None, Some(ours)));
        // A field asking about an older copy after a newer one hears no,
        // even while the newer one is still there.
        let newer = guard.note_written(b"correct horse");
        assert_ne!(ours, newer);
        assert!(!guard.holds(Some(b"correct horse"), Some(ours)));
        assert!(guard.holds(Some(b"correct horse"), Some(newer)));
        // Text the shell never wrote is never confirmed, whatever copy is
        // named: no oracle.
        assert!(!guard.holds(Some(b"a shopping list"), Some(newer)));
        assert!(!guard.holds(Some(b"a shopping list"), None));
        guard.forget();
        assert!(!guard.holds(Some(b"correct horse"), None));
        assert!(!guard.holds(Some(b"correct horse"), Some(newer)));
    }

    #[test]
    fn a_clear_from_the_webview_runs_only_for_the_copy_it_names() {
        let guard = ClipboardGuard::default();
        // Nothing copied yet: no number clears anything.
        assert!(!guard.may_clear(1, Some(b"hunter2")));
        // A code copied, then a password: the code's countdown ends first.
        let code = guard.note_written(b"287082");
        let password = guard.note_written(b"hunter2");
        assert!(
            !guard.may_clear(code, Some(b"hunter2")),
            "the code's clear must not take the password copied after it"
        );
        assert!(guard.may_clear(password, Some(b"hunter2")));
        // A number the shell never gave clears nothing.
        assert!(!guard.may_clear(password + 1, Some(b"hunter2")));
        // The person copied their own text over ours: leave it.
        assert!(!guard.may_clear(password, Some(b"a shopping list")));
        assert!(!guard.may_clear(password, None));
        // After a lock (or a clear) the record is gone.
        guard.forget();
        assert!(!guard.may_clear(password, Some(b"hunter2")));
    }

    #[test]
    fn the_clear_time_stays_in_range() {
        assert_eq!(clear_after(None), DEFAULT_CLEAR_AFTER);
        assert_eq!(clear_after(Some(1)), MIN_CLEAR_AFTER);
        assert_eq!(clear_after(Some(45)), Duration::from_secs(45));
        assert_eq!(clear_after(Some(600)), MAX_CLEAR_AFTER);
    }
}
