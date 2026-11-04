//! Keeping secret pages out of swap (03-architecture §8 "stay out of swap";
//! 04-security §14; docs/plan/26-desktop-finish.md §7,
//! docs/plan/52-identity-adopt.md).
//!
//! Every [`crate::Key32`] (vault, item, version and attachment keys, and
//! every key Lockdown makes), every [`crate::SecretBytes`] (a decrypted
//! field, a typed password on its way in) and every
//! [`crate::pq_sign::PqSigningKey`] asks for the memory pages it sits on to
//! be locked into RAM while it lives: `mlock` on Linux and macOS,
//! `VirtualLock` on Windows. A lock the system refuses (the
//! `RLIMIT_MEMLOCK` limit, typically 8 MiB on Linux and plenty for keys;
//! Windows' working-set minimum) is counted and the secret works as before:
//! it is still wiped on drop, it may just reach swap.
//!
//! **One page table per process.** [`PageLock`] is the identity crate's
//! `ecosystem_identity::memlock::PageLock`, which that crate's own key
//! types (the root key, persona keys, the App Data Key, the KEM seeds, the
//! Identity Signing Key) lock through too. Pages are shared with whatever
//! else the allocator put beside a secret, and the kernel keeps one lock
//! bit per page, so the table counts the secrets on each page, of either
//! crate: a page is locked once and unlocked only when the last secret on
//! it is dropped. Two tables would each unlock a page the other still
//! needed. [`report`] reads that one table, so Settings counts both.
//!
//! [`harden_process`] turns core dumps off (`RLIMIT_CORE` = 0 on Linux and
//! macOS), so a crash never writes the heap to disk. That is process policy
//! and stays here, not in the shared crate. It deliberately does **not**
//! use `PR_SET_DUMPABLE`: that would hide `/proc/<pid>/exe` from the
//! browser's native host, which checks the app by it (D155).
//!
//! What this does not reach: copies the allocator or a move made before a
//! value became a secret type (doc 26 §7, doc 52 §6). In a build with no
//! such calls (WASI, the web vault) every lock is a no-op and says so.

use std::sync::atomic::{AtomicBool, Ordering};

use ecosystem_identity::memlock;

/// The pages under one secret, locked while this lives: the identity
/// crate's, so both crates count on one table. `PageLock::of(bytes)` locks
/// the pages under a slice; `PageLock::of_value(&value)` under a value that
/// does not move (one behind a `Box`). Each answers `None` when empty,
/// unsupported, or refused (counted in [`report`]).
pub use ecosystem_identity::memlock::PageLock;

static DUMPS_OFF: AtomicBool = AtomicBool::new(false);

/// What the process kept out of swap, for Settings and the doc's measure.
/// The page numbers are the identity crate's one table, so they count
/// Lockdown's secrets and that crate's keys together.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct MemoryReport {
    /// Whether this build can lock pages at all.
    pub supported: bool,
    /// Pages locked now.
    pub pages_locked: u64,
    /// The page size, in bytes.
    pub page_size: u64,
    /// Pages ever locked.
    pub pages_locked_ever: u64,
    /// Locks the system refused (each such secret may reach swap).
    pub refused: u64,
    /// Whether core dumps are off for this process.
    pub core_dumps_off: bool,
}

/// Whether this build can lock pages.
pub fn supported() -> bool {
    memlock::supported()
}

/// The system's page size.
pub fn page_size() -> usize {
    memlock::page_size()
}

/// What is locked now and what was refused, across Lockdown's secrets and
/// the identity crate's keys.
pub fn report() -> MemoryReport {
    let shared = memlock::report();
    MemoryReport {
        supported: shared.supported,
        pages_locked: shared.pages_locked,
        page_size: shared.page_size,
        pages_locked_ever: shared.pages_locked_ever,
        refused: shared.refused,
        core_dumps_off: DUMPS_OFF.load(Ordering::Relaxed),
    }
}

/// Turns core dumps off for this process (Linux, macOS); answers whether
/// they are off now. Safe to call more than once.
pub fn harden_process() -> bool {
    let off = os::no_core_dumps();
    if off {
        DUMPS_OFF.store(true, Ordering::Relaxed);
    }
    off
}

#[cfg(unix)]
mod os {
    pub fn no_core_dumps() -> bool {
        let limit = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // SAFETY: setrlimit reads the struct passed by reference.
        unsafe { libc::setrlimit(libc::RLIMIT_CORE, &limit) == 0 }
    }
}

#[cfg(not(unix))]
mod os {
    /// Windows writes no core file; crash dumps go through Windows Error
    /// Reporting, which an installer opts out of (doc 26 §7). WASI has none.
    pub fn no_core_dumps() -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whether the kernel holds the page at `addr` locked: the `lo` flag on
    /// the mapping that holds it in `/proc/self/smaps` (a lock splits the
    /// mapping, so the flag is exact per page).
    #[cfg(target_os = "linux")]
    fn kernel_locked(addr: usize) -> bool {
        let smaps = std::fs::read_to_string("/proc/self/smaps").unwrap();
        let mut inside = false;
        for line in smaps.lines() {
            let first = line.split_whitespace().next().unwrap_or("");
            if let Some((a, b)) = first.split_once('-')
                && let (Ok(a), Ok(b)) = (usize::from_str_radix(a, 16), usize::from_str_radix(b, 16))
            {
                inside = (a..b).contains(&addr);
            } else if inside && let Some(flags) = line.strip_prefix("VmFlags:") {
                return flags.split_whitespace().any(|f| f == "lo");
            }
        }
        panic!("no mapping holds {addr:#x}")
    }

    #[test]
    fn pages_are_counted_once_and_released_with_the_last_secret() {
        let size = page_size();
        // Two secrets on one page, a third across that page and the next.
        // Both pages lie wholly inside the buffer, so nothing else in the
        // process shares them.
        let buf = vec![7u8; size * 3];
        let at = (size - (buf.as_ptr() as usize % size)) % size;
        let start = buf.as_ptr() as usize + at;
        let before = report();
        let Some(a) = PageLock::of(&buf[at..at + 16]) else {
            // Refused by the system here: counted, and nothing held.
            assert!(report().refused > before.refused);
            return;
        };
        let b = PageLock::of(&buf[at + 32..at + 48]).expect("the page is already locked");
        let Some(c) = PageLock::of(&buf[at + size - 8..at + size + 8]) else {
            assert!(report().refused > before.refused);
            return;
        };
        assert_eq!((a.pages(), b.pages(), c.pages()), (1, 1, 2));
        assert!(report().pages_locked_ever >= before.pages_locked_ever + 2);
        #[cfg(target_os = "linux")]
        assert!(kernel_locked(start) && kernel_locked(start + size));
        drop((a, b));
        #[cfg(target_os = "linux")]
        assert!(
            kernel_locked(start),
            "the third secret still holds the page"
        );
        drop(c);
        #[cfg(target_os = "linux")]
        assert!(!kernel_locked(start) && !kernel_locked(start + size));
        let _ = start;
        assert!(PageLock::of(&[]).is_none());
    }

    #[test]
    fn keys_and_secret_bytes_lock_their_pages_and_core_dumps_go_off() {
        use ecosystem_identity::SymmetricKey;
        let before = report().pages_locked_ever;
        let key = crate::Key32::generate().unwrap();
        let bytes = crate::SecretBytes::new(b"a decrypted field".to_vec());
        let r = report();
        assert!(r.supported);
        assert!(r.pages_locked_ever > before || r.refused > 0);
        assert!(r.pages_locked >= 1 || r.refused > 0);
        assert_eq!(bytes.expose(), b"a decrypted field");
        assert_eq!(key.expose().len(), 32);
        #[cfg(unix)]
        {
            assert!(harden_process());
            assert!(report().core_dumps_off);
        }
    }
}
