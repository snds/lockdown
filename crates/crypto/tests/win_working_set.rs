//! Lockdown raises its own working-set limits on Windows, so its page locks
//! succeed (docs/plan/59-win-working-set.md; D664's open question).
//!
//! `VirtualLock` lets a process lock about as many pages as its minimum
//! working set holds: about 42 by default, far fewer than the deep secret
//! stack's 256. Before each lock `locked::PageLock` makes room for it by
//! raising the minimum, within `locked::WORKING_SET_BOUND`, and gives the
//! room back as locks are released.
//!
//! One test function on purpose: this binary runs nothing else, so the
//! process-wide counts and limits it reads are not moved by another test
//! thread. On Linux and macOS the same steps run with the system's own
//! limit, where a refusal is counted as before.

use std::sync::{Arc, Barrier};

use ecosystem_identity::SymmetricKey;
use lockdown_crypto::locked::{self, PageLock};
use lockdown_crypto::secret_stack::{self, Kind, Tier};

/// The process's minimum and maximum working set, and its flags, read
/// straight from the system.
#[cfg(windows)]
fn limits() -> (usize, usize, u32) {
    use windows_sys::Win32::System::Memory::GetProcessWorkingSetSizeEx;
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    let (mut min, mut max, mut flags) = (0usize, 0usize, 0u32);
    // SAFETY: fills the three values for this process.
    let ok =
        unsafe { GetProcessWorkingSetSizeEx(GetCurrentProcess(), &mut min, &mut max, &mut flags) };
    assert_ne!(ok, 0, "GetProcessWorkingSetSizeEx");
    (min, max, flags)
}

/// `QUOTA_LIMITS_HARDWS_MIN_ENABLE`: never set by Lockdown.
#[cfg(windows)]
const HARD_MIN: u32 = 1;

/// Runs four deep calls and four light ones at once, each on its own
/// block, so both pools end up holding four.
fn four_of_each_at_once() {
    let barrier = Arc::new(Barrier::new(8));
    let threads: Vec<_> = (0..8)
        .map(|i| {
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                if i < 4 {
                    secret_stack::run_deep(|| {
                        barrier.wait();
                    });
                } else {
                    secret_stack::run(|| {
                        barrier.wait();
                    });
                }
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }
}

#[test]
fn lockdown_makes_room_in_its_working_set_so_its_locks_succeed() {
    let page = locked::page_size();
    let pages = |bytes: usize| (bytes / page) as u64;
    assert!(locked::supported());
    let start = locked::report();
    #[cfg(windows)]
    let (min0, max0, flags0) = limits();
    #[cfg(windows)]
    assert_eq!(
        start.refused, 0,
        "nothing was refused before the first lock"
    );

    // 1. A deep secret-stack call: its 1 MiB block locks, and Settings'
    // refused count stays where it was (0 on Windows).
    secret_stack::run_deep(|| ());
    let r = locked::report();
    #[cfg(windows)]
    {
        assert_eq!(secret_stack::kind(Tier::Deep), Kind::Locked);
        assert_eq!(r.refused, 0, "Settings shows no refusal");
        let (min, max, flags) = limits();
        assert!(
            min >= min0 + secret_stack::SIZE,
            "the minimum was raised: {min0} -> {min}"
        );
        assert!(max >= min && max >= max0, "the maximum keeps up: {max}");
        assert_eq!(flags & HARD_MIN, 0, "the minimum stays soft");
        assert_eq!(flags & HARD_MIN, flags0 & HARD_MIN);
        assert_eq!(min, min0 + locked::working_set_room(), "read back");
        println!(
            "deep call: locked, refused 0, minimum {} -> {} KiB, maximum {} -> {} KiB",
            min0 / 1024,
            min / 1024,
            max0 / 1024,
            max / 1024
        );
    }
    #[cfg(not(windows))]
    match secret_stack::kind(Tier::Deep) {
        Kind::Locked => assert!(r.pages_locked >= pages(secret_stack::SIZE)),
        Kind::Unlocked => assert!(r.refused > start.refused),
        Kind::Scrubbed => panic!("this build has a secret stack"),
    }

    // 2. Four deep blocks and four light ones live at once: all lock.
    let before = locked::report();
    four_of_each_at_once();
    let r = locked::report();
    let blocks = 4 * pages(secret_stack::SIZE) + 4 * pages(secret_stack::LIGHT);
    #[cfg(windows)]
    {
        assert_eq!(r.refused, 0, "every block locked");
        assert!(r.pages_locked >= blocks, "{} < {blocks}", r.pages_locked);
        let (min, _, flags) = limits();
        assert!(min >= min0 + 4 * secret_stack::SIZE + 4 * secret_stack::LIGHT);
        assert_eq!(flags & HARD_MIN, 0);
        println!(
            "4 deep + 4 light blocks: {} pages locked, refused 0, minimum {} KiB",
            r.pages_locked,
            min / 1024
        );
    }
    #[cfg(not(windows))]
    assert!(r.pages_locked >= blocks || r.refused > before.refused);
    let _ = before;

    // 3. Keys lock beside them, and Settings still shows no refusal.
    let key = lockdown_crypto::Key32::from_bytes([3; 32]);
    let pq = lockdown_crypto::pq_sign::PqSigningKey::from_seed([4; 32]);
    let ed = lockdown_crypto::SigningKey::from_bytes([5; 32]);
    #[cfg(windows)]
    {
        assert!(pq.is_locked() && ed.is_locked());
        assert_eq!(locked::report().refused, 0);
    }
    assert_eq!(key.expose(), &[3; 32]);
    drop((key, pq, ed));

    // 4. A large lock raises the limits, and its release gives the room
    // back. (Linux and macOS: the system's limit decides, as before.)
    #[cfg(windows)]
    {
        let big = vec![1u8; 8 * 1024 * 1024];
        let lock = PageLock::of(&big).expect("8 MiB fits under the bound");
        assert!(lock.pages() >= big.len() / page);
        let (raised, _, _) = limits();
        assert!(raised >= min0 + 4 * secret_stack::SIZE + big.len());
        assert_eq!(locked::report().refused, 0);
        drop(lock);
        let (lowered, _, _) = limits();
        let held = locked::report().pages_locked as usize * page;
        assert!(
            lowered + big.len() / 2 < raised,
            "the release gave the room back: {raised} -> {lowered}"
        );
        assert!(lowered >= min0 + held, "still room for what stays locked");
        assert_eq!(lowered, min0 + locked::working_set_room());
        println!(
            "8 MiB lock: minimum {} KiB, after release {} KiB",
            raised / 1024,
            lowered / 1024
        );
    }

    // 5. Past the bound the system refuses: counted, never fatal, nothing
    // held, and the limits are not raised for it.
    #[cfg(windows)]
    {
        let huge = vec![2u8; locked::WORKING_SET_BOUND + 8 * 1024 * 1024];
        let before = locked::report();
        let (min_before, _, _) = limits();
        let lock = PageLock::of(&huge);
        assert!(lock.is_none(), "a lock past the bound is refused");
        let after = locked::report();
        assert_eq!(after.refused, before.refused + 1, "the refusal is counted");
        assert_eq!(after.pages_locked, before.pages_locked, "nothing held");
        let (min_after, _, _) = limits();
        assert!(min_after <= min0 + locked::WORKING_SET_BOUND);
        assert!(min_after <= min_before + 4 * 1024 * 1024);
        assert_eq!(huge[huge.len() - 1], 2, "the memory still works");
        println!(
            "lock past the {} MiB bound: refused and counted ({}), not fatal",
            locked::WORKING_SET_BOUND / (1024 * 1024),
            after.refused
        );
    }
    let _ = (start, PageLock::of(&[]).is_none());

    // Calls still run on their blocks afterwards.
    assert!(secret_stack::run_deep(|| 7) == 7 && secret_stack::run(|| 8) == 8);
}
