//! Keeping secret pages out of swap (03-architecture §8 "stay out of swap";
//! 04-security §14; docs/plan/26-desktop-finish.md §7).
//!
//! Every [`crate::Key32`] (vault, item, version and attachment keys, and
//! every key Lockdown makes) and every [`crate::SecretBytes`] (a decrypted
//! field, a typed password on its way in) asks for the memory pages it sits
//! on to be locked into RAM while it lives: `mlock` on Linux and macOS,
//! `VirtualLock` on Windows. Pages are shared with whatever else the
//! allocator put beside the secret, so a page is locked once and unlocked
//! only when the last secret on it is dropped (a count per page; `munlock`
//! is not counted by the kernel). A lock the system refuses (the
//! `RLIMIT_MEMLOCK` limit, typically 8 MiB on Linux and plenty for keys;
//! Windows' working-set minimum) is counted and the secret works as before:
//! it is still wiped on drop, it may just reach swap.
//!
//! [`harden_process`] turns core dumps off (`RLIMIT_CORE` = 0 on Linux and
//! macOS), so a crash never writes the heap to disk. It deliberately does
//! **not** use `PR_SET_DUMPABLE`: that would hide `/proc/<pid>/exe` from
//! the browser's native host, which checks the app by it (D155).
//!
//! What this does not reach: the identity crate's own key types (the root
//! key, persona keys, the App Data Key) live in that crate (consumed, never
//! forked, D12), and copies the allocator made before a value became a
//! secret type. Both are recorded in doc 26 §7. In a build with no such
//! calls (WASI, the web vault) every lock is a no-op and says so.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Pages locked now, with how many secrets sit on each.
static PAGES: Mutex<BTreeMap<usize, u32>> = Mutex::new(BTreeMap::new());
static REFUSED: AtomicU64 = AtomicU64::new(0);
static LOCKED_EVER: AtomicU64 = AtomicU64::new(0);
static DUMPS_OFF: AtomicBool = AtomicBool::new(false);

/// What the process kept out of swap, for Settings and the doc's measure.
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

/// The pages under one secret, locked while this lives.
#[derive(Debug)]
pub struct PageLock {
    first: usize,
    count: usize,
}

impl PageLock {
    /// Locks the pages under `bytes`; `None` when empty, unsupported, or
    /// refused (counted in [`report`]).
    pub fn of(bytes: &[u8]) -> Option<Self> {
        if bytes.is_empty() || !supported() {
            return None;
        }
        let size = page_size();
        let start = bytes.as_ptr() as usize;
        let first = start / size;
        let last = (start + bytes.len() - 1) / size;
        let mut pages = PAGES.lock().ok()?;
        let mut taken = Vec::new();
        for page in first..=last {
            let n = pages.entry(page).or_insert(0);
            if *n == 0 {
                if !os::lock(page * size, size) {
                    pages.remove(&page);
                    // Undo what this call took, so no page is counted twice.
                    for p in taken {
                        release(&mut pages, p, size);
                    }
                    REFUSED.fetch_add(1, Ordering::Relaxed);
                    return None;
                }
                LOCKED_EVER.fetch_add(1, Ordering::Relaxed);
            }
            *pages.entry(page).or_insert(0) += 1;
            taken.push(page);
        }
        Some(Self {
            first,
            count: last - first + 1,
        })
    }
}

fn release(pages: &mut BTreeMap<usize, u32>, page: usize, size: usize) {
    if let Some(n) = pages.get_mut(&page) {
        *n -= 1;
        if *n == 0 {
            pages.remove(&page);
            os::unlock(page * size, size);
        }
    }
}

impl Drop for PageLock {
    fn drop(&mut self) {
        let size = page_size();
        if let Ok(mut pages) = PAGES.lock() {
            for page in self.first..self.first + self.count {
                release(&mut pages, page, size);
            }
        }
    }
}

/// Whether this build can lock pages.
pub fn supported() -> bool {
    cfg!(any(unix, windows))
}

/// The system's page size.
pub fn page_size() -> usize {
    static SIZE: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *SIZE.get_or_init(|| os::page_size().max(1))
}

/// What is locked now and what was refused.
pub fn report() -> MemoryReport {
    MemoryReport {
        supported: supported(),
        pages_locked: PAGES.lock().map(|p| p.len() as u64).unwrap_or(0),
        page_size: page_size() as u64,
        pages_locked_ever: LOCKED_EVER.load(Ordering::Relaxed),
        refused: REFUSED.load(Ordering::Relaxed),
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
    pub fn page_size() -> usize {
        // SAFETY: sysconf reads a constant of the system; no memory is touched.
        let n = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        if n > 0 { n as usize } else { 4096 }
    }

    pub fn lock(addr: usize, len: usize) -> bool {
        // SAFETY: mlock only changes how the kernel pages this range; the
        // range is a whole page the caller's live allocation sits on.
        unsafe { libc::mlock(addr as *const libc::c_void, len) == 0 }
    }

    pub fn unlock(addr: usize, len: usize) {
        // SAFETY: as `lock`; unlocking a page that is no longer mapped
        // fails harmlessly with ENOMEM.
        unsafe {
            libc::munlock(addr as *const libc::c_void, len);
        }
    }

    pub fn no_core_dumps() -> bool {
        let limit = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // SAFETY: setrlimit reads the struct passed by reference.
        unsafe { libc::setrlimit(libc::RLIMIT_CORE, &limit) == 0 }
    }
}

#[cfg(windows)]
mod os {
    use windows_sys::Win32::System::Memory::{VirtualLock, VirtualUnlock};
    use windows_sys::Win32::System::SystemInformation::{GetSystemInfo, SYSTEM_INFO};

    pub fn page_size() -> usize {
        // SAFETY: GetSystemInfo fills the struct passed to it.
        unsafe {
            let mut info: SYSTEM_INFO = std::mem::zeroed();
            GetSystemInfo(&mut info);
            info.dwPageSize as usize
        }
    }

    pub fn lock(addr: usize, len: usize) -> bool {
        // SAFETY: VirtualLock only pins the committed page the caller's
        // live allocation sits on into the working set.
        unsafe { VirtualLock(addr as *const core::ffi::c_void, len) != 0 }
    }

    pub fn unlock(addr: usize, len: usize) {
        // SAFETY: as `lock`.
        unsafe {
            VirtualUnlock(addr as *const core::ffi::c_void, len);
        }
    }

    /// Windows writes no core file; crash dumps go through Windows Error
    /// Reporting, which an installer opts out of (doc 26 §7).
    pub fn no_core_dumps() -> bool {
        false
    }
}

#[cfg(not(any(unix, windows)))]
mod os {
    pub fn page_size() -> usize {
        65_536
    }
    pub fn lock(_addr: usize, _len: usize) -> bool {
        false
    }
    pub fn unlock(_addr: usize, _len: usize) {}
    pub fn no_core_dumps() -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// This process's locked memory as the kernel counts it.
    #[cfg(target_os = "linux")]
    fn vm_locked_kb() -> u64 {
        std::fs::read_to_string("/proc/self/status")
            .unwrap()
            .lines()
            .find_map(|l| l.strip_prefix("VmLck:"))
            .and_then(|v| v.trim().trim_end_matches("kB").trim().parse().ok())
            .unwrap()
    }

    #[test]
    fn pages_are_counted_once_and_released_with_the_last_secret() {
        let size = page_size();
        // Two secrets on one page, a third across two pages.
        let page = vec![7u8; size * 3];
        let at = (size - (page.as_ptr() as usize % size)) % size;
        let a = PageLock::of(&page[at..at + 16]);
        let b = PageLock::of(&page[at + 32..at + 48]);
        let c = PageLock::of(&page[at + size - 8..at + size + 8]);
        if a.is_none() {
            // Refused by the system here: counted, and nothing held.
            assert!(report().refused > 0);
            return;
        }
        let held = PAGES.lock().unwrap().clone();
        let first = (page.as_ptr() as usize + at) / size;
        assert_eq!(held.get(&first), Some(&3));
        assert_eq!(held.get(&(first + 1)), Some(&1));
        #[cfg(target_os = "linux")]
        assert!(vm_locked_kb() >= (2 * size / 1024) as u64);
        drop((a, b));
        assert_eq!(PAGES.lock().unwrap().get(&first), Some(&1));
        drop(c);
        let held = PAGES.lock().unwrap().clone();
        assert!(!held.contains_key(&first) && !held.contains_key(&(first + 1)));
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
