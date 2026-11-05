//! Windows: room in the working set for every lock
//! (docs/plan/59-win-working-set.md; D664's open question).
//!
//! `VirtualLock` lets a process lock about as many pages as its minimum
//! working set holds, less a small overhead. The default minimum is 200
//! KiB, so a process can lock about 42 pages of 4 KiB. The deep secret
//! stack alone is 256 pages (doc 58), and its lock was refused and counted.
//!
//! So Lockdown raises its own limits, on demand. Before each lock made
//! through [`super::PageLock`], [`locking`] works out the room the lock
//! needs: the pages the one table holds now, plus the pages the new lock
//! spans, plus [`SLACK`]. If that is more than the room already made, it
//! raises the minimum working set by that much and [`STEP`] more (and the
//! maximum with it, keeping the gap between them), with
//! `SetProcessWorkingSetSizeEx`, and only then asks the table to lock. Room
//! is counted above the minimum the process started with, so the starting
//! minimum stays for the process's ordinary pages.
//!
//! - **Why before, not after a refusal.** The page table and its refusal
//!   counter are the identity crate's (D601). Its Windows arm counts a
//!   refusal before Lockdown sees one, so a retry after it would still show
//!   on Settings. Making the room first means the lock succeeds the first
//!   time.
//! - **Why on demand, not a fixed size at start.** What a process locks
//!   moves: the secret stack's pools keep up to 4 idle blocks of each size,
//!   made only when calls run at once; the ML-DSA key, the Ed25519 keys,
//!   the OPAQUE setup, every `Key32` and `SecretBytes` lock too, and so do
//!   the identity crate's keys. A number chosen at start would be either
//!   too small for a busy server or a large raise for a desktop that runs
//!   one call at a time.
//! - **The bound.** Room never goes above [`BOUND`] (32 MiB, 8192 pages of
//!   4 KiB). A lock that would need more is not given room and is left to
//!   the system, which refuses it: counted, never fatal, and the secret
//!   still wiped (D607). 32 MiB holds both pools full (4 × 1 MiB and
//!   4 × 48 KiB), every key, and more than twenty deep calls at once.
//! - **Serialized.** One mutex covers reading the room, raising it,
//!   the lock itself and the release, so two threads never race the
//!   limits.
//! - **The flags stay as they are.** Flags are passed as 0, which leaves
//!   the process's enforcement unchanged: soft limits by default. Lockdown
//!   never sets `QUOTA_LIMITS_HARDWS_MIN_ENABLE`, so the system can still
//!   trim the process's ordinary pages under memory pressure.
//! - **No admin rights.** Raising the minimum needs
//!   `SeIncreaseWorkingSetPrivilege`, which Windows gives every user. If a
//!   raise is refused for want of it, Lockdown enables that privilege in its
//!   own token once and tries again. Nothing is asked of the person.
//! - **Unlocks give the room back.** After each release, if the room is
//!   more than two steps above what the table needs (its pages plus
//!   `SLACK` and one `STEP`), the minimum is lowered to that. The gap stops
//!   a key made and dropped in a loop from changing the limits each time.
//! - **When the system says no** (a group policy took the privilege away,
//!   or memory is short), the limits stay where they were and the lock goes
//!   ahead as before: a refusal is counted and never fatal (D607).
//!
//! The identity crate's own keys lock through the same table without
//! coming here. `SLACK` keeps room for them above whatever the table holds
//! after each of Lockdown's locks; they take a page or two each.

use std::sync::{Mutex, PoisonError};

use ecosystem_identity::memlock;
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_PRIVILEGE_NOT_HELD, GetLastError, HANDLE, LUID,
};
use windows_sys::Win32::Security::{
    AdjustTokenPrivileges, LUID_AND_ATTRIBUTES, LookupPrivilegeValueW, SE_INC_WORKING_SET_NAME,
    SE_PRIVILEGE_ENABLED, TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES,
};
use windows_sys::Win32::System::Memory::{GetProcessWorkingSetSizeEx, SetProcessWorkingSetSizeEx};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

/// Room kept free above what the one table holds after each lock, for the
/// identity crate's own keys, which lock through the table directly.
pub const SLACK: usize = 256 * 1024;
/// What each raise adds beyond the room a lock needs, so a run of small
/// locks raises the limits once, not once a page.
pub const STEP: usize = 256 * 1024;
/// The most room Lockdown adds above the minimum working set the process
/// started with.
pub const BOUND: usize = 32 * 1024 * 1024;

/// The limits the process started with, and the room added above them.
struct Limits {
    min: usize,
    max: usize,
    room: usize,
}

enum State {
    /// Not read yet: read on the first lock.
    Unread,
    /// The limits couldn't be read: locks go ahead as before (D607).
    Off,
    Ready(Limits),
}

static STATE: Mutex<State> = Mutex::new(State::Unread);

/// Makes room for a lock spanning `bytes` of whole pages, then runs `lock`
/// (the table's own lock), all under one mutex.
pub fn locking<L>(bytes: usize, lock: impl FnOnce() -> Option<L>) -> Option<L> {
    let mut state = STATE.lock().unwrap_or_else(PoisonError::into_inner);
    if matches!(*state, State::Unread) {
        *state = read().map_or(State::Off, |(min, max)| {
            State::Ready(Limits { min, max, room: 0 })
        });
    }
    let State::Ready(limits) = &mut *state else {
        return lock();
    };
    let need = held().saturating_add(bytes).saturating_add(SLACK);
    if need > limits.room && need <= BOUND {
        set(limits, need.saturating_add(STEP).min(BOUND));
    }
    let out = lock();
    if out.is_none() {
        // Refused anyway (counted by the table): give back what was raised
        // for it.
        settle(limits);
    }
    out
}

/// Runs `unlock` (the table's own release), then gives back room the table
/// no longer needs.
pub fn unlocking(unlock: impl FnOnce()) {
    let mut state = STATE.lock().unwrap_or_else(PoisonError::into_inner);
    unlock();
    if let State::Ready(limits) = &mut *state {
        settle(limits);
    }
}

/// The room Lockdown has added above the starting minimum now, in bytes
/// (0 before the first lock, or when the limits couldn't be read).
pub fn room() -> usize {
    match &*STATE.lock().unwrap_or_else(PoisonError::into_inner) {
        State::Ready(limits) => limits.room,
        State::Unread | State::Off => 0,
    }
}

/// The bytes the one table holds locked now, of either crate.
fn held() -> usize {
    usize::try_from(memlock::report().pages_locked)
        .unwrap_or(usize::MAX)
        .saturating_mul(memlock::page_size())
}

/// Lowers the room to what the table needs, once it is two steps above.
fn settle(limits: &mut Limits) {
    let floor = held().saturating_add(SLACK).saturating_add(STEP);
    if limits.room > floor.saturating_add(2 * STEP) {
        set(limits, floor);
    }
}

/// Sets the minimum to the starting one plus `room`, and the maximum to keep
/// at least the starting gap above it. Answers whether the system took it.
fn set(limits: &mut Limits, room: usize) -> bool {
    let min = limits.min.saturating_add(room);
    let gap = limits.max.saturating_sub(limits.min);
    let max = limits.max.max(min.saturating_add(gap));
    let ok = apply(min, max)
        || (last_error() == ERROR_PRIVILEGE_NOT_HELD && enable_privilege() && apply(min, max));
    if ok {
        limits.room = room;
    }
    ok
}

/// The process's minimum and maximum working set now.
fn read() -> Option<(usize, usize)> {
    let (mut min, mut max, mut flags) = (0usize, 0usize, 0u32);
    // SAFETY: fills the three values for this process (a pseudo-handle
    // with every right).
    let ok =
        unsafe { GetProcessWorkingSetSizeEx(GetCurrentProcess(), &mut min, &mut max, &mut flags) };
    (ok != 0 && min > 0 && max >= min).then_some((min, max))
}

fn apply(min: usize, max: usize) -> bool {
    // SAFETY: sets this process's own limits; flags 0 keeps its enforcement
    // (soft, unless something else made it hard) as it is.
    unsafe { SetProcessWorkingSetSizeEx(GetCurrentProcess(), min, max, 0) != 0 }
}

fn last_error() -> u32 {
    // SAFETY: reads this thread's last error code.
    unsafe { GetLastError() }
}

/// Enables `SeIncreaseWorkingSetPrivilege` in this process's token, which
/// every user holds by default. Answers whether it is enabled now.
fn enable_privilege() -> bool {
    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: opens this process's own token for adjusting privileges.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_ADJUST_PRIVILEGES, &mut token) } == 0 {
        return false;
    }
    let mut luid = LUID {
        LowPart: 0,
        HighPart: 0,
    };
    // SAFETY: looks a privilege's name up on this computer; fills `luid`.
    let found =
        unsafe { LookupPrivilegeValueW(std::ptr::null(), SE_INC_WORKING_SET_NAME, &mut luid) } != 0;
    let enabled = found && {
        let new = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES {
                Luid: luid,
                Attributes: SE_PRIVILEGE_ENABLED,
            }],
        };
        // SAFETY: reads `new`; no previous state asked for. It succeeds
        // with ERROR_NOT_ALL_ASSIGNED when the token lacks the privilege,
        // so the last error is checked too.
        let ok = unsafe {
            AdjustTokenPrivileges(
                token,
                0,
                &new,
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        } != 0;
        ok && last_error() == 0
    };
    // SAFETY: the token this function opened.
    unsafe { CloseHandle(token) };
    enabled
}
