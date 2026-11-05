//! The secret stack: a key's short-lived copies built on locked pages and
//! wiped before the call returns (docs/plan/58-stack-wipe.md; closes D606
//! and D617).
//!
//! `ed25519_dalek::SigningKey::from_bytes`, `ml_dsa::ExpandedSigningKey::
//! from_seed` and `opaque_ke::ServerSetup::{new, deserialize}` return their
//! key by value, and none has an in-place form. So the key is built in a
//! stack frame and moved into its locked box, and the frame's copy stays
//! behind on an ordinary stack page, unlocked and unwiped, until the stack
//! is reused. Signing leaves the same kind of residue: the expanded secret,
//! the nonce, the hash state.
//!
//! [`run`] and [`run_deep`] move that work onto a stack of its own:
//!
//! - **The blocks.** A block is a stack of its own: page-aligned, with a
//!   no-access guard region below it, so an overflow faults instead of
//!   writing into the heap, and its pages locked out of swap through
//!   [`crate::locked::PageLock`], on the process's one page table (a
//!   refused lock is counted there like any other, and the block works
//!   unlocked). [`run`] uses a small block, [`LIGHT`] bytes, enough for an
//!   Ed25519 operation; [`run_deep`] a large one, [`SIZE`] bytes, for
//!   ML-DSA-65 and the OPAQUE server. Two sizes keep a process's locked
//!   memory to a few pages until a deep call needs more. Blocks are made on
//!   first use and kept in a small pool per size, shared by the process's
//!   threads, so a process holds about as many as it runs key operations
//!   at once (usually one), not one per thread.
//! - **The switch.** On Linux, macOS and the other Unixes, `psm::on_stack`
//!   (the stack-switching core of `stacker`, which rustc uses) points the
//!   stack pointer at the block, runs the closure, and points it back. On
//!   Windows, where a stack must be the system's own, a block is a fiber
//!   (`CreateFiberEx`, `SwitchToFiber`), committed in full and locked the
//!   same way. Switching costs no system call.
//! - **The wipe.** A block is all zeros between calls. After each call the
//!   lowest non-zero byte marks how deep the call reached, and the block is
//!   zeroed from there to the top before the call returns (on Windows, by
//!   the fiber itself, below its own frame, before it switches back). The
//!   search compares whole pages against zeros (`memcmp`), so it stays cheap
//!   even in a debug build. The depth feeds [`deepest`], which a test holds
//!   well clear of the guard.
//!
//! Where none of that is available (WASI, the web vault's build: no
//! `mmap`, no memory protection, no locking, and most of the call stack
//! lives in the engine, out of reach), or a block can't be made, the
//! closure runs where it is and is then scrubbed: a never-inlined function
//! zeroes a stack buffer [`SCRUB_LIGHT`] or [`SCRUB_DEEP`] bytes deep below
//! the caller, which covers the frames the closure used. That is best
//! effort: the compiler decides where frames go, and on WASI the engine's
//! own stack is not reachable at all.
//!
//! Calls nest: a call inside a call stays on the same block (a deep call
//! inside a light one moves on to a large block), and the outermost call on
//! each block wipes it. A panic inside is caught on the block, the block is
//! wiped, and the panic carries on from the caller's stack.
//!
//! What a closure returns is moved back to the caller's stack, so return a
//! boxed key or a public value (a signature, a public key), never a bare
//! secret.

use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A large block's usable depth, before rounding up to whole pages, for
/// [`run_deep`]. The deepest call measured is ML-DSA-65's key expansion in
/// a debug build, about 650 KiB (doc 58 §3); this leaves over a third of
/// the block spare.
pub const SIZE: usize = 1024 * 1024;
/// A small block's usable depth, for [`run`]: an Ed25519 operation takes
/// about 9 KiB in a debug build. A multiple of 16 KiB, so no page is partly
/// used on any system Lockdown runs on.
pub const LIGHT: usize = 48 * 1024;
/// How deep the fallback scrub reaches below its caller after [`run`].
pub const SCRUB_LIGHT: usize = 64 * 1024;
/// How deep the fallback scrub reaches below its caller after
/// [`run_deep`].
pub const SCRUB_DEEP: usize = 256 * 1024;
/// The no-access region below a block: larger than any frame a wrapped
/// call makes, so even a frame that skips stack probes lands in it.
#[cfg(any(unix, windows))]
const GUARD: usize = 256 * 1024;
/// Idle blocks each pool keeps; one made beyond that is freed after use.
#[cfg(any(unix, windows))]
const POOL_MAX: usize = 4;

static DEEPEST: AtomicUsize = AtomicUsize::new(0);

/// Idle small blocks, then idle large ones.
#[cfg(any(unix, windows))]
static POOLS: [std::sync::Mutex<Vec<os::Stack>>; 2] = [
    std::sync::Mutex::new(Vec::new()),
    std::sync::Mutex::new(Vec::new()),
];

thread_local! {
    /// The block this thread is running on now, and its size, if any.
    #[cfg(any(unix, windows))]
    static ACTIVE: Cell<Option<(*const os::Stack, Tier)>> = const { Cell::new(None) };
    /// Set by `testing::pinned`: this thread keeps its own blocks.
    static PIN: Cell<bool> = const { Cell::new(false) };
    #[cfg(any(unix, windows))]
    static MINE: std::cell::RefCell<[Option<os::Stack>; 2]> =
        const { std::cell::RefCell::new([None, None]) };
    /// Set by `testing::unwiped`, in this crate's own tests only: leave the
    /// block as the call left it. No other build has the switch.
    #[cfg(test)]
    static SKIP_WIPE: Cell<bool> = const { Cell::new(false) };
    /// How deep this thread's last call on a block reached.
    static LAST: Cell<usize> = const { Cell::new(0) };
}

/// How a call's stack is kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// On a block locked out of swap.
    Locked,
    /// On a block; the system refused the lock (counted in
    /// [`crate::locked::report`]).
    Unlocked,
    /// Where the caller is, then scrubbed (best effort).
    Scrubbed,
}

/// Which size of block a call needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    /// A small block, [`LIGHT`] bytes: [`run`].
    Light = 0,
    /// A large block, [`SIZE`] bytes: [`run_deep`].
    Deep = 1,
}

impl Tier {
    #[cfg(any(unix, windows))]
    fn bytes(self) -> usize {
        match self {
            Tier::Light => LIGHT,
            Tier::Deep => SIZE,
        }
    }
}

/// Runs `f` on a small secret stack and wipes it before returning: for a
/// call that stays well within [`LIGHT`] bytes (Ed25519). See the module
/// notes for what `f` may return.
pub fn run<R>(f: impl FnOnce() -> R) -> R {
    run_on(Tier::Light, f)
}

/// [`run`] on a large block, for a call that goes deeper (ML-DSA-65, the
/// OPAQUE server).
pub fn run_deep<R>(f: impl FnOnce() -> R) -> R {
    run_on(Tier::Deep, f)
}

#[cfg(any(unix, windows))]
fn run_on<R>(tier: Tier, f: impl FnOnce() -> R) -> R {
    let on = ACTIVE.with(Cell::get);
    if let Some((_, at)) = on
        && at >= tier
    {
        return f();
    }
    // SAFETY: the block this thread is running on, alive until its own
    // call returns, which is after this one.
    let from = on.map(|(s, _)| unsafe { &*s });
    let Some(stack) = take(tier, from) else {
        return scrubbed(tier, f);
    };
    ACTIVE.with(|a| a.set(Some((std::ptr::from_ref(&stack), tier))));
    let out = stack.enter(from, f);
    ACTIVE.with(|a| a.set(on));
    give(tier, stack);
    match out {
        Ok(Ok(r)) => r,
        Ok(Err(panic)) => std::panic::resume_unwind(panic),
        // This thread can't switch (it is someone else's fiber).
        Err(f) => scrubbed(tier, f),
    }
}

#[cfg(not(any(unix, windows)))]
fn run_on<R>(tier: Tier, f: impl FnOnce() -> R) -> R {
    scrubbed(tier, f)
}

/// A block for this call: the thread's own when pinned, else one from the
/// pool, else a new one.
#[cfg(any(unix, windows))]
fn take(tier: Tier, from: Option<&os::Stack>) -> Option<os::Stack> {
    let i = tier as usize;
    if PIN.with(Cell::get)
        && let Some(mine) = MINE.with(|m| m.borrow_mut()[i].take())
    {
        return Some(mine);
    }
    let pooled = POOLS[i].lock().unwrap_or_else(|e| e.into_inner()).pop();
    pooled.or_else(|| os::Stack::new(tier.bytes(), from))
}

#[cfg(any(unix, windows))]
fn give(tier: Tier, stack: os::Stack) {
    let i = tier as usize;
    if PIN.with(Cell::get) {
        MINE.with(|m| m.borrow_mut()[i] = Some(stack));
        return;
    }
    let spare = {
        let mut pool = POOLS[i].lock().unwrap_or_else(|e| e.into_inner());
        if pool.len() < POOL_MAX {
            pool.push(stack);
            None
        } else {
            Some(stack)
        }
    };
    // Freed outside the pool's lock.
    drop(spare);
}

/// How a call of `tier` keeps its stack in this process now (makes a block
/// if the pool has none).
pub fn kind(tier: Tier) -> Kind {
    #[cfg(any(unix, windows))]
    if let Some(stack) = take(tier, None) {
        let kind = if stack.locked() {
            Kind::Locked
        } else {
            Kind::Unlocked
        };
        give(tier, stack);
        return kind;
    }
    let _ = tier;
    Kind::Scrubbed
}

/// A block's usable bytes for `tier`: [`LIGHT`] or [`SIZE`] rounded up to
/// whole pages, or 0 where calls are scrubbed.
pub fn size(tier: Tier) -> usize {
    #[cfg(any(unix, windows))]
    if let Some(stack) = take(tier, None) {
        let size = stack.size();
        give(tier, stack);
        return size;
    }
    let _ = tier;
    0
}

/// The deepest any call has reached on a secret stack in this process, in
/// bytes from the top of its block.
pub fn deepest() -> usize {
    DEEPEST.load(Ordering::Relaxed)
}

#[cfg(any(unix, windows))]
fn round_up(n: usize, to: usize) -> usize {
    n.div_ceil(to) * to
}

/// Where the first non-zero byte of `view` is. Whole chunks are compared
/// against zeros with the C library's `memcmp`, which is fast even in a
/// debug build; only the first chunk that differs is searched byte by byte.
#[cfg(any(unix, windows))]
fn first_nonzero(view: &[u8]) -> Option<usize> {
    static ZEROS: [u8; 4096] = [0; 4096];
    let mut at = 0;
    for chunk in view.chunks(ZEROS.len()) {
        if chunk != &ZEROS[..chunk.len()] {
            return chunk.iter().position(|b| *b != 0).map(|i| at + i);
        }
        at += chunk.len();
    }
    None
}

/// Measures and wipes `[low, high)`: finds the lowest non-zero byte,
/// records its depth below `top`, and zeroes from there up to `high`.
///
/// # Safety
///
/// `[low, high)` is mapped, writable, 8-byte aligned at `low`, and holds no
/// live value: a block after a call, or a fiber's stack below its own frame
/// on Windows.
#[cfg(any(unix, windows))]
unsafe fn measure_and_wipe(low: usize, high: usize, top: usize) {
    // SAFETY: the caller's contract; the range is read as plain bytes.
    let view = unsafe { std::slice::from_raw_parts(low as *const u8, high - low) };
    let Some(first) = first_nonzero(view) else {
        LAST.with(|l| l.set(0));
        return;
    };
    let from = low + first;
    LAST.with(|l| l.set(top - from));
    DEEPEST.fetch_max(top - from, Ordering::Relaxed);
    #[cfg(test)]
    if SKIP_WIPE.with(Cell::get) {
        return;
    }
    // SAFETY: the caller's contract. `write_bytes` is a plain memset; the
    // block stays reachable from its pool and is read again by the next
    // call, so the store is not dead and is not dropped.
    unsafe { std::ptr::write_bytes(from as *mut u8, 0, high - from) };
    std::hint::black_box(from);
    std::sync::atomic::compiler_fence(Ordering::SeqCst);
}

/// Runs `f` where the caller is, then zeroes stack below the caller: the
/// frames `f` used. Best effort (module notes).
fn scrubbed<R>(tier: Tier, f: impl FnOnce() -> R) -> R {
    struct Scrub(Tier);
    impl Drop for Scrub {
        fn drop(&mut self) {
            match self.0 {
                Tier::Light => scrub::<SCRUB_LIGHT>(),
                Tier::Deep => scrub::<SCRUB_DEEP>(),
            }
        }
    }
    let _scrub = Scrub(tier);
    f()
}

#[inline(never)]
fn scrub<const N: usize>() {
    let mut buf = std::mem::MaybeUninit::<[u8; N]>::uninit();
    // SAFETY: `buf` is this frame's own `N` bytes.
    unsafe { std::ptr::write_bytes(buf.as_mut_ptr().cast::<u8>(), 0, N) };
    std::hint::black_box(&mut buf);
}

#[cfg(unix)]
mod os {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use super::{GUARD, measure_and_wipe, round_up};
    use crate::locked::PageLock;

    /// A block: `guard` no-access bytes, then `size` usable ones.
    pub(super) struct Stack {
        // Field order is drop order: the pages are unlocked before the
        // mapping goes. The block is all zeros by then.
        pages: Option<PageLock>,
        map: *mut libc::c_void,
        map_len: usize,
        low: usize,
        size: usize,
    }

    // SAFETY: a block is plain memory this value owns; it moves between
    // threads only through a pool, never while a call runs on it.
    unsafe impl Send for Stack {}

    impl Stack {
        /// A block of at least `bytes`. `from` is the block this thread is
        /// on now, if any; on Unix a block is made the same from anywhere.
        pub(super) fn new(bytes: usize, from: Option<&Stack>) -> Option<Self> {
            let _ = from;
            let page = crate::locked::page_size();
            let size = round_up(bytes, page);
            let guard = round_up(GUARD, page);
            let map_len = guard + size;
            #[cfg(any(target_os = "linux", target_os = "android"))]
            let flags = libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_STACK;
            #[cfg(not(any(target_os = "linux", target_os = "android")))]
            let flags = libc::MAP_PRIVATE | libc::MAP_ANON;
            // SAFETY: a fresh anonymous mapping; nothing else refers to it.
            let map = unsafe {
                libc::mmap(
                    std::ptr::null_mut(),
                    map_len,
                    libc::PROT_READ | libc::PROT_WRITE,
                    flags,
                    -1,
                    0,
                )
            };
            if map == libc::MAP_FAILED {
                return None;
            }
            // SAFETY: the first `guard` bytes of the mapping just made.
            if unsafe { libc::mprotect(map, guard, libc::PROT_NONE) } != 0 {
                // SAFETY: the mapping just made, unused.
                unsafe { libc::munmap(map, map_len) };
                return None;
            }
            let mut stack = Self {
                pages: None,
                map,
                map_len,
                low: map as usize + guard,
                size,
            };
            // SAFETY: the usable part of the fresh mapping: mapped,
            // readable, and nothing holds a reference into it.
            stack.pages = PageLock::of(unsafe { stack.view() });
            Some(stack)
        }

        /// The whole block as bytes.
        ///
        /// # Safety
        ///
        /// No call is running on the block while the slice lives.
        pub(super) unsafe fn view(&self) -> &[u8] {
            // SAFETY: the usable part of the mapping; the caller's contract.
            unsafe { std::slice::from_raw_parts(self.low as *const u8, self.size) }
        }

        pub(super) fn locked(&self) -> bool {
            self.pages.is_some()
        }

        pub(super) fn size(&self) -> usize {
            self.size
        }

        /// Runs `f` on this block, then wipes the block. `from` is the
        /// block this thread is on now, if any; on Unix a switch works from
        /// any stack.
        pub(super) fn enter<R, F: FnOnce() -> R>(
            &self,
            from: Option<&Stack>,
            f: F,
        ) -> Result<std::thread::Result<R>, F> {
            let _ = from;
            // SAFETY: `low` and `size` are page-aligned (so aligned for any
            // target), the guard region lies below `low`, and the closure
            // cannot unwind out of `on_stack`: a panic is caught on the
            // block and resumed by the caller, on its own stack.
            let out = unsafe {
                psm::on_stack(self.low as *mut u8, self.size, || {
                    catch_unwind(AssertUnwindSafe(f))
                })
            };
            let top = self.low + self.size;
            // SAFETY: the whole block, no longer in use by any frame.
            unsafe { measure_and_wipe(self.low, top, top) };
            Ok(out)
        }
    }

    impl Drop for Stack {
        fn drop(&mut self) {
            drop(self.pages.take());
            // SAFETY: the mapping `new` made, no longer used by anything.
            unsafe { libc::munmap(self.map, self.map_len) };
        }
    }
}

#[cfg(windows)]
mod os {
    use std::cell::Cell;
    use std::ffi::c_void;
    use std::mem::MaybeUninit;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use windows_sys::Win32::System::Memory::{
        MEM_COMMIT, MEMORY_BASIC_INFORMATION, PAGE_READWRITE, VirtualQuery,
    };
    use windows_sys::Win32::System::Threading::{
        ConvertFiberToThread, ConvertThreadToFiber, CreateFiberEx, DeleteFiber,
        GetCurrentThreadStackLimits, IsThreadAFiber, SwitchToFiber,
    };

    use super::{GUARD, measure_and_wipe, round_up};
    use crate::locked::PageLock;

    /// `FIBER_FLAG_FLOAT_SWITCH`: the fiber keeps its own floating-point
    /// state (winbase.h).
    const FLOAT_SWITCH: u32 = 1;
    /// Stack the job runner reserves above the job, so the wipe's own frame
    /// sits where no job frame has been.
    const SPACER: usize = 4 * 1024;
    /// How far below its own frame the wipe stops: room for the wipe's
    /// callees, inside the spacer.
    const MARGIN: usize = 1024;

    type Job = (*mut u8, unsafe fn(*mut u8));

    /// What the fiber and the thread running it share. Boxed, so its
    /// address is fixed.
    struct Shared {
        /// The fiber to switch back to, set before each switch.
        back: Cell<*mut c_void>,
        /// The job to run, set before each switch.
        job: Cell<Option<Job>>,
        /// The fiber's committed stack, `[low, high)`, set on first entry.
        low: Cell<usize>,
        high: Cell<usize>,
    }

    /// A block: a fiber whose stack is committed in full.
    pub(super) struct Stack {
        // Field order is drop order: the pages are unlocked before the
        // fiber's stack is freed.
        pages: Option<PageLock>,
        handle: *mut c_void,
        shared: Box<Shared>,
    }

    // SAFETY: a fiber may be resumed by any thread while it isn't running
    // (SwitchToFiber); it moves between threads only through a pool, never
    // while a call runs on it.
    unsafe impl Send for Stack {}

    impl Drop for Stack {
        fn drop(&mut self) {
            drop(self.pages.take());
            // SAFETY: a fiber this process made, not running now.
            unsafe { DeleteFiber(self.handle) };
        }
    }

    /// The fiber's body: notes its stack, then runs each job it is given
    /// and wipes below itself before switching back. It never returns.
    unsafe extern "system" fn body(param: *mut c_void) {
        // SAFETY: `param` is the `Shared` the owning `Stack` keeps boxed.
        let shared = unsafe { &*param.cast::<Shared>() };
        let (mut low, mut high) = (0usize, 0usize);
        // SAFETY: fills the two values with this fiber's stack limits.
        unsafe { GetCurrentThreadStackLimits(&mut low, &mut high) };
        shared.low.set(committed_from(low, high));
        shared.high.set(high);
        loop {
            // SAFETY: back to the fiber that switched in.
            unsafe { SwitchToFiber(shared.back.get()) };
            run_job(shared);
            wipe_below(shared);
        }
    }

    /// The lowest committed read-write address of the stack reserved at
    /// `[low, high)`: walks up past the reserved pages and the system's
    /// guard page to the region that runs to the top.
    fn committed_from(low: usize, high: usize) -> usize {
        let mut at = low;
        while at < high {
            let mut info = MaybeUninit::<MEMORY_BASIC_INFORMATION>::zeroed();
            // SAFETY: queries one address of this fiber's own stack.
            let got = unsafe {
                VirtualQuery(
                    at as *const c_void,
                    info.as_mut_ptr(),
                    std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
                )
            };
            if got == 0 {
                return high;
            }
            // SAFETY: VirtualQuery filled it.
            let info = unsafe { info.assume_init() };
            let base = info.BaseAddress as usize;
            if info.State == MEM_COMMIT && info.Protect == PAGE_READWRITE {
                return base;
            }
            at = base + info.RegionSize;
        }
        high
    }

    #[inline(never)]
    fn run_job(shared: &Shared) {
        let mut spacer = MaybeUninit::<[u8; SPACER]>::uninit();
        std::hint::black_box(&mut spacer);
        if let Some((data, call)) = shared.job.take() {
            // SAFETY: `enter` set this job and waits for it.
            unsafe { call(data) };
        }
        std::hint::black_box(&mut spacer);
    }

    #[inline(never)]
    fn wipe_below(shared: &Shared) {
        let mark = 0u8;
        let here = std::hint::black_box(&mark) as *const u8 as usize;
        let end = (here - MARGIN) & !15;
        let low = shared.low.get();
        if end > low {
            // SAFETY: committed stack of this fiber below its own frame
            // and the spacer: the dead frames of the job just run.
            unsafe { measure_and_wipe(low, end, shared.high.get()) };
        }
    }

    unsafe fn call<F: FnMut()>(data: *mut u8) {
        // SAFETY: `enter` passes a pointer to its own `F`.
        unsafe { (*data.cast::<F>())() }
    }

    fn job_of<F: FnMut()>(f: &mut F) -> Job {
        (std::ptr::from_mut(f).cast(), call::<F>)
    }

    impl Stack {
        /// A fiber with at least `bytes` of stack. `from` is the fiber this
        /// thread is on now, if any: the first switch starts there.
        pub(super) fn new(bytes: usize, from: Option<&Stack>) -> Option<Self> {
            let page = crate::locked::page_size();
            let size = round_up(bytes, page);
            let shared = Box::new(Shared {
                back: Cell::new(std::ptr::null_mut()),
                job: Cell::new(None),
                low: Cell::new(0),
                high: Cell::new(0),
            });
            // SAFETY: the body only reads `shared`, which outlives the fiber.
            let handle = unsafe {
                CreateFiberEx(
                    size,
                    size + round_up(GUARD, page),
                    FLOAT_SWITCH,
                    Some(body),
                    std::ptr::from_ref(&*shared).cast(),
                )
            };
            if handle.is_null() {
                return None;
            }
            let mut stack = Self {
                pages: None,
                handle,
                shared,
            };
            // First entry: the body notes its stack and switches back.
            if !stack.switch(from, None) {
                return None;
            }
            if stack.size() == 0 {
                return None;
            }
            // SAFETY: the fiber's committed stack, suspended now; nothing
            // holds a reference into it.
            stack.pages = PageLock::of(unsafe { stack.view() });
            Some(stack)
        }

        /// The whole committed stack as bytes.
        ///
        /// # Safety
        ///
        /// The fiber is suspended (no call is running on it) while the
        /// slice lives.
        pub(super) unsafe fn view(&self) -> &[u8] {
            let (low, high) = (self.shared.low.get(), self.shared.high.get());
            // SAFETY: the committed stack; the caller's contract.
            unsafe { std::slice::from_raw_parts(low as *const u8, high - low) }
        }

        pub(super) fn locked(&self) -> bool {
            self.pages.is_some()
        }

        pub(super) fn size(&self) -> usize {
            self.shared.high.get().saturating_sub(self.shared.low.get())
        }

        /// Switches into the fiber to run `job` (or just to start it) and
        /// back. From one of these fibers (`from`), straight there and
        /// back; from a plain thread, by making it a fiber for the switch.
        /// `false` when this thread is someone else's fiber, whose handle
        /// isn't reachable without reading the thread block.
        fn switch(&self, from: Option<&Stack>, job: Option<Job>) -> bool {
            self.shared.job.set(job);
            if let Some(from) = from {
                self.shared.back.set(from.handle);
                // SAFETY: from the running fiber into one that isn't.
                unsafe { SwitchToFiber(self.handle) };
                return true;
            }
            // SAFETY: plain queries and conversions of this thread, and a
            // switch into a fiber no other thread is running.
            unsafe {
                if IsThreadAFiber() != 0 {
                    return false;
                }
                let me = ConvertThreadToFiber(std::ptr::null());
                if me.is_null() {
                    return false;
                }
                self.shared.back.set(me);
                SwitchToFiber(self.handle);
                ConvertFiberToThread();
            }
            true
        }

        /// Runs `f` on this fiber, which wipes its stack before switching
        /// back. `from` is the fiber this thread is on now, if any.
        pub(super) fn enter<R, F: FnOnce() -> R>(
            &self,
            from: Option<&Stack>,
            f: F,
        ) -> Result<std::thread::Result<R>, F> {
            let mut f = Some(f);
            let mut out = None;
            {
                let mut job = || {
                    if let Some(f) = f.take() {
                        out = Some(catch_unwind(AssertUnwindSafe(f)));
                    }
                };
                self.switch(from, Some(job_of(&mut job)));
            }
            match (out, f) {
                (Some(out), _) => Ok(out),
                (None, Some(f)) => Err(f),
                (None, None) => unreachable!("a job that ran leaves its result"),
            }
        }
    }
}

/// For tests (here and in the crates that wrap their keys): look into a
/// block between calls, and measure how deep a call reached. Nothing here
/// can turn the wipe off: the control that does (`unwiped`) exists only in
/// this crate's own test build.
pub mod testing {
    #[cfg(test)]
    use super::SKIP_WIPE;
    use super::{Cell, LAST, PIN};

    /// Runs `f` with this thread keeping blocks of its own instead of the
    /// pools', so [`holds`] looks at the blocks this thread's calls used.
    /// They are freed when the thread ends.
    pub fn pinned<R>(f: impl FnOnce() -> R) -> R {
        PIN.with(|p| p.set(true));
        let out = f();
        PIN.with(|p| p.set(false));
        out
    }

    /// Whether either of this thread's pinned blocks holds `needle` now
    /// (`None` outside [`pinned`], before any call, or where calls are
    /// scrubbed).
    pub fn holds(needle: &[u8]) -> Option<bool> {
        #[cfg(any(unix, windows))]
        {
            if !PIN.with(Cell::get) || needle.is_empty() {
                return None;
            }
            super::MINE.with(|m| {
                let mine = m.borrow();
                let mut any = None;
                for stack in mine.iter().flatten() {
                    // SAFETY: between calls, so no call runs on the block.
                    let hay = unsafe { stack.view() };
                    let found = hay.windows(needle.len()).any(|w| w == needle);
                    any = Some(any.unwrap_or(false) || found);
                }
                any
            })
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = (needle, Cell::new(()));
            None
        }
    }

    /// Runs `f` with every call on this thread leaving its block as the
    /// call left it: the control a wipe test compares against. Call
    /// [`wipe`] after looking. This crate's tests only.
    #[cfg(test)]
    pub(crate) fn unwiped<R>(f: impl FnOnce() -> R) -> R {
        SKIP_WIPE.with(|s| s.set(true));
        let out = f();
        SKIP_WIPE.with(|s| s.set(false));
        out
    }

    /// Wipes this thread's pinned blocks now.
    pub fn wipe() {
        super::run_deep(|| ());
        super::run(|| ());
    }

    /// How deep this thread's last call on a block reached, in bytes from
    /// the top (0 where calls are scrubbed).
    pub fn last_depth() -> usize {
        LAST.with(Cell::get)
    }
}

#[cfg(all(test, any(unix, windows)))]
mod tests {
    use std::hint::black_box;
    use std::time::Instant;

    use super::*;

    const PATTERN: [u8; 32] = *b"a secret left on the stack, 32b!";

    /// Leaves `PATTERN` in a dead frame below the caller.
    #[inline(never)]
    fn leave() {
        let mut buf = [0u8; 512];
        buf[200..232].copy_from_slice(&PATTERN);
        black_box(&mut buf);
    }

    fn here() -> usize {
        let mark = 0u8;
        black_box(&mark) as *const u8 as usize
    }

    #[test]
    fn a_call_runs_on_a_locked_block_and_leaves_it_zeroed() {
        testing::pinned(|| {
            for tier in [Tier::Light, Tier::Deep] {
                let size = size(tier);
                assert!(size >= tier.bytes() && size.is_multiple_of(crate::locked::page_size()));
                match kind(tier) {
                    Kind::Locked => assert!(crate::locked::report().pages_locked >= 1),
                    Kind::Unlocked => assert!(crate::locked::report().refused >= 1),
                    Kind::Scrubbed => panic!("this build has a secret stack"),
                }
            }
            // The closure runs on the block, not here.
            let outside = here();
            let inside = run(here);
            assert!(outside.abs_diff(inside) > 4096, "{outside:#x} {inside:#x}");
            run(leave);
            assert_eq!(testing::holds(&PATTERN), Some(false));
            assert!(testing::last_depth() > 0 && testing::last_depth() < LIGHT / 4);
            // The control: the same call with the wipe off leaves it there.
            testing::unwiped(|| run(leave));
            assert_eq!(testing::holds(&PATTERN), Some(true));
            testing::wipe();
            assert_eq!(testing::holds(&PATTERN), Some(false));
        });
    }

    #[test]
    fn calls_nest_and_the_outermost_call_on_each_block_wipes() {
        testing::pinned(|| {
            let nested = run(|| {
                let light = here();
                // A deep call inside a light one moves to a large block.
                let deep = run_deep(|| {
                    leave();
                    // A light call inside a deep one stays where it is.
                    (here(), run(here))
                });
                (light, deep)
            });
            let (light, (deep, inner)) = nested;
            assert!(light.abs_diff(deep) > 4096);
            assert!(deep.abs_diff(inner) < 4096);
            assert!(ACTIVE.with(Cell::get).is_none());
            assert_eq!(testing::holds(&PATTERN), Some(false));
        });
    }

    #[test]
    fn a_panic_inside_is_wiped_and_carries_on_outside() {
        testing::pinned(|| {
            let caught = std::panic::catch_unwind(|| {
                run(|| {
                    leave();
                    panic!("inside");
                })
            });
            assert!(caught.is_err());
            assert!(ACTIVE.with(Cell::get).is_none());
            assert_eq!(testing::holds(&PATTERN), Some(false));
            // And the block still works.
            assert_eq!(run(|| 7), 7);
        });
    }

    #[test]
    fn the_fallback_scrub_clears_the_frames_it_covers() {
        // Scrubbing is what WASI does; here it runs inside a block so its
        // effect can be read.
        testing::pinned(|| {
            testing::unwiped(|| run_deep(|| scrubbed(Tier::Light, leave)));
            assert_eq!(testing::holds(&PATTERN), Some(false));
            testing::unwiped(|| run_deep(leave));
            assert_eq!(testing::holds(&PATTERN), Some(true));
            testing::wipe();
            assert_eq!(testing::holds(&PATTERN), Some(false));
        });
    }

    #[test]
    fn the_pools_share_blocks_between_threads() {
        // Made on one thread, used on another: a block is plain memory.
        let a = std::thread::spawn(|| run(here)).join().unwrap();
        let b = std::thread::spawn(|| run_deep(here)).join().unwrap();
        let c = std::thread::spawn(|| run(here)).join().unwrap();
        assert!(a > 0 && b > 0 && c > 0);
        for pool in &POOLS {
            assert!(pool.lock().unwrap().len() <= POOL_MAX);
        }
    }

    /// Each wrapped call in this crate, measured on its own: its tier and
    /// how deep it reached.
    fn depths() -> Vec<(&'static str, Tier, usize)> {
        use crate::auth::{LoginClient, OpaqueServer, RegistrationClient};
        use crate::pq_sign::PqSigningKey;
        let mut out = Vec::new();
        let mut note = |name, tier, f: &mut dyn FnMut()| {
            f();
            out.push((name, tier, testing::last_depth()));
        };
        let ed = crate::SigningKey::from_bytes([1; 32]);
        note("ed25519 from_bytes", Tier::Light, &mut || {
            black_box(crate::SigningKey::from_bytes([2; 32]));
        });
        note("ed25519 from (clone)", Tier::Light, &mut || {
            black_box(crate::SigningKey::from(ed.inner().clone()));
        });
        note("ed25519 sign", Tier::Light, &mut || {
            black_box(ed.sign(b"a vault head"));
        });
        let pq = PqSigningKey::from_seed([4; 32]);
        note("ml-dsa-65 from_seed", Tier::Deep, &mut || {
            black_box(PqSigningKey::from_seed([3; 32]));
        });
        note("ml-dsa-65 sign", Tier::Deep, &mut || {
            black_box(pq.sign(b"a vault head", b"lockdown/test/v1").unwrap());
        });
        let server = OpaqueServer::generate().unwrap();
        note("opaque setup new", Tier::Deep, &mut || {
            black_box(OpaqueServer::generate().unwrap());
        });
        let bytes = server.to_bytes();
        note("opaque setup deserialize", Tier::Deep, &mut || {
            black_box(OpaqueServer::from_bytes(bytes.expose()).unwrap());
        });
        let login = b"me@example.com";
        let (client, request) = RegistrationClient::start("a long password").unwrap();
        let mut response = Vec::new();
        note("opaque registration answer", Tier::Deep, &mut || {
            response = server.registration_response(login, &request).unwrap();
        });
        let (upload, _) = client.finish("a long password", &response).unwrap();
        let file = OpaqueServer::password_file(&upload).unwrap();
        let (client, request) = LoginClient::start("a long password").unwrap();
        let mut answer = None;
        note("opaque login answer", Tier::Deep, &mut || {
            answer = Some(server.login_response(login, Some(&file), &request).unwrap());
        });
        let (response, state) = answer.unwrap();
        let finished = client.finish("a long password", &response).unwrap();
        note("opaque login finish", Tier::Deep, &mut || {
            black_box(OpaqueServer::login_finish(&state, &finished.finalization).unwrap());
        });
        out
    }

    #[test]
    fn every_wrapped_call_stays_well_clear_of_the_guard() {
        testing::pinned(|| {
            let depths = depths();
            for (name, tier, depth) in &depths {
                let room = size(*tier);
                println!("secret stack: {name} ({tier:?}): {depth} bytes of {room}");
                assert!(*depth > 0, "{name} ran on a block");
                // A quarter of its block spare at least: a call that gets
                // close to the guard fails here first.
                assert!(
                    depth * 4 <= room * 3,
                    "{name} reached {depth} of {room} bytes: too close to the guard"
                );
            }
            assert!(deepest() >= depths.iter().map(|d| d.2).max().unwrap_or(0));
        });
    }

    #[test]
    fn signing_time_on_and_off_the_secret_stack() {
        let key = crate::SigningKey::from_bytes([9; 32]);
        let n = 200;
        let message = [7u8; 256];
        let time = |f: &dyn Fn() -> [u8; 64]| {
            let start = Instant::now();
            for _ in 0..n {
                black_box(f());
            }
            start.elapsed() / n
        };
        let off = || ed25519_dalek::Signer::sign(key.inner(), &message).to_bytes();
        let on = || key.sign(&message);
        // Warm both paths (a block is made on first use).
        assert_eq!(off(), on());
        let (off_t, on_t) = (time(&off), time(&on));
        println!("ed25519 sign: {off_t:?} on the thread's stack, {on_t:?} on the secret stack");
    }
}
