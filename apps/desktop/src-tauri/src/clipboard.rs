//! The copy rule (03-architecture §8, design brief §4): a copied secret is
//! cleared from the clipboard after a timer, and only if the clipboard
//! still holds our value, so a later copy of the person's own text is
//! never destroyed. Nothing here keeps the secret: what is remembered is a
//! hash of what was written and a generation number, so the timer of an
//! older copy cannot clear a newer one.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use sha2::{Digest, Sha256};

/// The default time a copied secret stays on the clipboard.
pub const DEFAULT_CLEAR_AFTER: Duration = Duration::from_secs(30);
/// The shortest and longest a person may set (design brief §4).
pub const MIN_CLEAR_AFTER: Duration = Duration::from_secs(10);
pub const MAX_CLEAR_AFTER: Duration = Duration::from_secs(90);

/// What the shell remembers about the last copy: two hashes of the text
/// (BLAKE3 for the shell's own timer, SHA-256 so unicron's SecretField,
/// which names a copy by the hex SHA-256 of the bytes it handed over, can
/// ask about the right one) and a generation number. Never the text.
#[derive(Default)]
pub struct ClipboardGuard {
    generation: AtomicU64,
    written: Mutex<Option<Written>>,
}

#[derive(Clone, Copy)]
struct Written {
    blake3: [u8; 32],
    sha256: [u8; 32],
}

fn sha256_hex_of(digest: &[u8; 32]) -> String {
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// The hex SHA-256 of `text`, as unicron's SecretField computes it.
pub fn sha256_hex(text: &[u8]) -> String {
    sha256_hex_of(&Sha256::digest(text).into())
}

impl ClipboardGuard {
    /// Records that `text` was just written. Returns the copy's generation,
    /// which the clear timer hands back to [`ClipboardGuard::should_clear`].
    pub fn note_written(&self, text: &[u8]) -> u64 {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        if let Ok(mut w) = self.written.lock() {
            *w = Some(Written {
                blake3: *blake3::hash(text).as_bytes(),
                sha256: Sha256::digest(text).into(),
            });
        }
        generation
    }

    /// Whether the clipboard that now holds `current` still holds what the
    /// shell last wrote (`clipboard_holds`, for the component's own timer).
    /// With `hash` (a hex SHA-256) the question is about that copy in
    /// particular: a field asking about an older copy after a newer one
    /// hears no. The hash is only ever compared with the shell's own
    /// record, never used to test what the clipboard holds, so the
    /// webview cannot use this to recognise text it did not copy.
    pub fn holds(&self, current: Option<&[u8]>, hash: Option<&str>) -> bool {
        let Some(current) = current else {
            return false;
        };
        let Ok(w) = self.written.lock() else {
            return false;
        };
        let Some(written) = w.as_ref() else {
            return false;
        };
        if let Some(hash) = hash
            && !hash.eq_ignore_ascii_case(&sha256_hex_of(&written.sha256))
        {
            return false;
        }
        written.blake3 == *blake3::hash(current).as_bytes()
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
        guard.note_written(b"hunter2");
        let ours = sha256_hex(b"hunter2");
        assert!(guard.holds(Some(b"hunter2"), None));
        assert!(guard.holds(Some(b"hunter2"), Some(&ours)));
        assert!(guard.holds(Some(b"hunter2"), Some(&ours.to_uppercase())));
        // The person copied their own text since.
        assert!(!guard.holds(Some(b"a shopping list"), Some(&ours)));
        assert!(!guard.holds(None, Some(&ours)));
        // A field asking about an older copy after a newer one hears no,
        // even while the newer one is still there.
        guard.note_written(b"correct horse");
        assert!(!guard.holds(Some(b"correct horse"), Some(&ours)));
        assert!(guard.holds(Some(b"correct horse"), Some(&sha256_hex(b"correct horse"))));
        // A hash of text the shell never wrote is never confirmed, whatever
        // the clipboard holds: no oracle.
        assert!(!guard.holds(
            Some(b"a shopping list"),
            Some(&sha256_hex(b"a shopping list"))
        ));
        guard.forget();
        assert!(!guard.holds(Some(b"correct horse"), None));
    }

    #[test]
    fn the_clear_time_stays_in_range() {
        assert_eq!(clear_after(None), DEFAULT_CLEAR_AFTER);
        assert_eq!(clear_after(Some(1)), MIN_CLEAR_AFTER);
        assert_eq!(clear_after(Some(45)), Duration::from_secs(45));
        assert_eq!(clear_after(Some(600)), MAX_CLEAR_AFTER);
    }
}
