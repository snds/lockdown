//! One page table for Lockdown's secrets and the identity crate's keys
//! (docs/plan/52-identity-adopt.md, D314, the identity crate's decision
//! 0003).
//!
//! The kernel keeps one lock bit per page. With two tables, a page holding
//! a Lockdown key and a crate key was counted once in each, and whichever
//! dropped first unlocked the page under the other. Now both lock through
//! `ecosystem_identity::memlock::PageLock`, so a shared page stays locked
//! until both are gone.
//!
//! One test function on purpose: this binary runs nothing else, so the
//! process-wide counts it compares are not moved by another test thread.

use ecosystem_identity::memlock;
use ecosystem_identity::{AppDataKey, SymmetricKey};
use lockdown_crypto::Key32;
use lockdown_crypto::locked::{self, PageLock};
use lockdown_crypto::pq_sign::{self, PqSigningKey};

/// Whether the kernel holds the page at `addr` locked: the `lo` flag on the
/// mapping that holds it in `/proc/self/smaps` (a lock splits the mapping,
/// so the flag is exact per page).
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

/// Lockdown's report and the crate's are the same table.
fn same_counts() {
    let ours = locked::report();
    let theirs = memlock::report();
    assert_eq!(ours.supported, theirs.supported);
    assert_eq!(ours.page_size, theirs.page_size);
    assert_eq!(ours.pages_locked, theirs.pages_locked);
    assert_eq!(ours.pages_locked_ever, theirs.pages_locked_ever);
    assert_eq!(ours.refused, theirs.refused);
}

fn page_of(p: &[u8]) -> usize {
    p.as_ptr() as usize / locked::page_size()
}

#[test]
fn lockdown_and_the_identity_crate_share_one_page_table() {
    let size = locked::page_size();
    assert!(locked::supported());
    same_counts();
    let start = locked::report();

    // 1. The ADR's case, placed exactly: a Lockdown secret and a crate key
    // on one page that nothing else shares (it lies wholly inside the
    // buffer). Lockdown locks through `locked::PageLock`, the crate through
    // `memlock::PageLock`, as its key types do.
    let buf = vec![5u8; size * 3];
    let at = (size - (buf.as_ptr() as usize % size)) % size;
    let page = buf.as_ptr() as usize + at;
    let ours = PageLock::of(&buf[at..at + 32]);
    let theirs = memlock::PageLock::of(&buf[at + 64..at + 96]);
    same_counts();
    match (ours, theirs) {
        (Some(ours), Some(theirs)) => {
            assert_eq!(
                locked::report().pages_locked_ever,
                start.pages_locked_ever + 1,
                "one page, locked once for both"
            );
            drop(theirs);
            #[cfg(target_os = "linux")]
            assert!(
                kernel_locked(page),
                "the Lockdown secret still holds the page"
            );
            assert_eq!(locked::report().pages_locked, start.pages_locked + 1);
            drop(ours);
            #[cfg(target_os = "linux")]
            assert!(!kernel_locked(page), "released with the last secret");
            assert_eq!(locked::report().pages_locked, start.pages_locked);
        }
        (ours, theirs) => {
            // Refused (a tight limit): counted, never fatal, nothing held.
            assert!(locked::report().refused > start.refused);
            drop((ours, theirs));
            assert_eq!(locked::report().pages_locked, start.pages_locked);
        }
    }
    let _ = page;
    same_counts();

    // 2. The same with the real types, where the allocator puts them:
    // `Key32` and the crate's `AppDataKey` are both a boxed `[u8; 32]`, so
    // made in turn they land side by side, often on one page. Dropping
    // every crate key leaves every page under a live `Key32` locked.
    let before = locked::report();
    let mut ours = Vec::new();
    let mut theirs = Vec::new();
    for i in 0..64u8 {
        ours.push(Key32::from_bytes([i; 32]));
        theirs.push(AppDataKey::from_bytes([i ^ 0xff; 32]));
    }
    let shared = ours
        .iter()
        .filter(|k| {
            theirs
                .iter()
                .any(|t| page_of(t.expose()) == page_of(k.expose()))
        })
        .count();
    let all_locked = theirs.iter().all(AppDataKey::is_locked);
    same_counts();
    drop(theirs);
    if all_locked && locked::report().refused == before.refused {
        #[cfg(target_os = "linux")]
        {
            assert!(shared > 0, "side-by-side keys shared no page");
            for k in &ours {
                assert!(
                    kernel_locked(k.expose().as_ptr() as usize),
                    "a crate key's drop unlocked a Lockdown key's page"
                );
            }
        }
    }
    let _ = shared;
    for (i, k) in ours.iter().enumerate() {
        assert_eq!(k.expose(), &[i as u8; 32]);
    }
    drop(ours);
    assert_eq!(locked::report().pages_locked, before.pages_locked);
    same_counts();

    // 3. The ML-DSA expanded key: about 64 KiB, so 17 or 18 pages of 4 KiB
    // or 5 or 6 of 16 KiB (Apple silicon), all
    // locked while it lives, and counted in the report Settings reads.
    let before = locked::report();
    let pq = PqSigningKey::from_seed([9; 32]);
    let sig = pq.sign(b"head", b"lockdown/test/v1").unwrap();
    pq.verifying_key()
        .verify(b"head", b"lockdown/test/v1", &sig, "head")
        .unwrap();
    if pq.is_locked() {
        let least = pq_sign::EXPANDED_KEY_LEN.div_ceil(size);
        assert!((least..=least + 1).contains(&pq.locked_pages()));
        assert!(locked::report().pages_locked >= before.pages_locked + least as u64);
    } else {
        assert!(locked::report().refused > before.refused);
    }
    same_counts();
    drop(pq);
    assert_eq!(locked::report().pages_locked, before.pages_locked);

    // 4. Refusals stay non-fatal. Lower this process's lock limit to zero
    // (an unprivileged process is then refused every new page; one with
    // CAP_IPC_LOCK is not), and every secret still works and is counted
    // one way or the other.
    #[cfg(unix)]
    {
        let mut old = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // SAFETY: getrlimit and setrlimit read and write the struct passed.
        unsafe {
            assert_eq!(libc::getrlimit(libc::RLIMIT_MEMLOCK, &mut old), 0);
            let zero = libc::rlimit {
                rlim_cur: 0,
                rlim_max: old.rlim_max,
            };
            assert_eq!(libc::setrlimit(libc::RLIMIT_MEMLOCK, &zero), 0);
        }
        let before = locked::report();
        let key = Key32::from_bytes([1; 32]);
        let bytes = lockdown_crypto::SecretBytes::new(b"typed on its way in".to_vec());
        let crate_key = AppDataKey::from_bytes([2; 32]);
        let pq = PqSigningKey::from_seed([3; 32]);
        let r = locked::report();
        if !pq.is_locked() {
            assert!(r.refused > before.refused, "a refusal is counted");
        }
        if !crate_key.is_locked() {
            assert!(r.refused > before.refused);
        }
        assert_eq!(key.expose(), &[1; 32]);
        assert_eq!(bytes.expose(), b"typed on its way in");
        assert_eq!(crate_key.expose(), &[2; 32]);
        let sig = pq.sign(b"m", b"lockdown/test/v1").unwrap();
        pq.verifying_key()
            .verify(b"m", b"lockdown/test/v1", &sig, "m")
            .unwrap();
        same_counts();
        drop((key, bytes, crate_key, pq));
        // SAFETY: as above; the soft limit goes back to what it was.
        unsafe {
            assert_eq!(libc::setrlimit(libc::RLIMIT_MEMLOCK, &old), 0);
        }
    }
    assert_eq!(locked::report().pages_locked, start.pages_locked);
    same_counts();
}
