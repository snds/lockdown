# 58: Short-lived key copies on a locked, wiped stack

_Slice: `slice/stack-wipe`, built 2026-11-05, on Sean's go (2026-11-05). Closes D606 (doc 52: ML-DSA-65's 64 KiB by-value copy) and D617 (doc 53: the by-value copies of the Ed25519 key and the OPAQUE setup, and the Ed25519 key each device signature rebuilds). Builds on D314 (secrets kept out of swap), D601 (one page table per process) and D610–D612 (doc 53). There are no wire, IPC, wasm, view or stored-format changes. One new dependency: `psm`. Proposed decisions D660–D669 (§9)._

## For future agent

- **TL;DR:** `lockdown_crypto::secret_stack::run` and `run_deep` run a closure on a stack of their own. The stack is a block with a no-access guard below it, its pages locked out of swap through the one `PageLock` table. Before the call returns, the block is zeroed from the deepest byte the call touched up to the top. Every key constructor that returns by value runs inside one, and so does every signature. So the key's short-lived copies sit only on locked pages that are wiped at once. Unix switches with `psm::on_stack`. Windows uses a fiber. WASI runs in place and scrubs. People see no difference.
- **Read with:**
  - `crates/crypto/src/secret_stack.rs`: the module notes, then `run_on`, `measure_and_wipe`, `os::Stack` (Unix and Windows) and the tests.
  - `crates/crypto/src/sign.rs`, `pq_sign.rs`, `auth.rs`: the wrapped calls and their wipe tests.
  - `crates/app/src/local_unlock.rs`: the `DeviceKeystore` impl for `DeviceKeys`.
  - `crates/vault/src/identity.rs` (`identity_signing_key`), `handover.rs` (`root_key`), `pq.rs` (`identity_pq_key`).
- **Do not:**
  - Return a bare secret from a closure. What it returns is moved back to the caller's stack. Return a `Box`, a locked key type, or a public value.
  - Wrap anything that takes long or blocks (Argon2id, I/O, a lock other threads hold) in `run`. A block is held for the whole call.
  - Use `run` for a call that goes deeper than an Ed25519 operation. A light block is 48 KiB, and a call that outgrows it hits the guard and crashes the process. Use `run_deep`, then add the call to a depth test (§3).
  - Add a switch that leaves a block unwiped to any build but the crypto crate's own tests (D667).
  - Call `mlock`/`VirtualLock` yourself. The blocks lock through `locked::PageLock`, like everything else.

## 1. Architecture

```text
 caller's stack                       secret stack (one block, from a pool)
 ─────────────────                    ───────────────────────────────────────
 SigningKey::from_bytes(seed)  ──run──►  top ┌───────────────────────────┐ locked (PageLock)
   │                                         │ closure frame:            │
   │                                         │   ed25519 SigningKey      │ ← the by-value copy
   │                                         │   Box::new(…) ── heap ──► locked box (doc 53)
   │                                         │ dalek's frames: SHA-512,  │
   │                                         │   expanded secret, nonce  │
   │                                         ├ ─ ─ deepest byte touched ─┤
   │                                         │ zeros                     │
   │                                         └───────────────────────────┘
   │                                    guard  │ no access (256 KiB)       │ ← an overflow faults here
   ◄── Box (pointer) ──────────────────  zeroed from the deepest byte to the top, then back
```

| Piece | What it does |
|---|---|
| **Blocks** (`os::Stack`) | A small block (`LIGHT`, 48 KiB) for Ed25519 operations and a large one (`SIZE`, 1 MiB) for ML-DSA-65, the OPAQUE server and the identity derivations. Each is page-aligned, with a 256 KiB no-access guard below it. Its usable pages are locked through `locked::PageLock` when it is made. A refused lock is counted in `locked::report()`, and the block works unlocked. |
| **Pools** | Blocks are made on first use and kept in one pool per size, shared by all threads (at most 4 idle per size). A process holds about as many blocks as it runs key operations at once, usually one of each, not one per thread. That keeps locked memory small on a server with many worker threads and on Windows, whose lock budget is small. |
| **Switch** | Unix: `psm::on_stack(low, size, closure)`. `psm` is the stack-switching core of `stacker`, which rustc uses. Lockdown writes no assembly. Windows: the block is a fiber (`CreateFiberEx` with its stack committed in full, then `SwitchToFiber`). The thread is made a fiber for the switch and turned back after. No system call on either path, apart from making a block. |
| **Wipe** | A block is all zeros between calls. After a call, the first non-zero byte from the bottom marks the deepest byte touched. The block is zeroed from there to the top with `ptr::write_bytes` (memset), then `black_box` and a compiler fence. The search compares 4 KiB chunks against zeros with `memcmp`, so it stays fast even in a debug build. On Windows the fiber wipes itself before switching back: everything below its own frame, past a 4 KiB spacer the job runs under, so the wipe never touches a live frame. |
| **Nesting** | A call inside a call stays on the same block, and the outermost call on each block wipes it. A deep call inside a light one moves on to a large block: Unix switches again, and Windows goes fiber to fiber. |
| **Panics** | Caught on the block (`catch_unwind`), the block wiped, then resumed on the caller's stack. Unwinding never crosses a stack switch. |
| **Fallback** (`scrubbed`) | WASI, a thread that is already someone else's fiber (Windows), or a block that can't be made: the closure runs in place. Then a never-inlined function zeroes a stack buffer 64 KiB (light) or 256 KiB (deep) below the caller, which overwrites the frames the closure used. This is best effort (§5). |

### What runs where

| Call | File:line | Tier |
|---|---|---|
| `SigningKey::from_bytes` (`ed25519_dalek::SigningKey::from_bytes`) | `crates/crypto/src/sign.rs:51` | light |
| `SigningKey::sign` (each Ed25519 signature: the server's tokens and log, the ISK's records) | `crates/crypto/src/sign.rs:81` | light |
| `From<ed25519_dalek::SigningKey>` (the clone) | `crates/crypto/src/sign.rs:96` | light |
| `PqSigningKey::from_seed` (`ExpandedSigningKey::from_seed`, D606) | `crates/crypto/src/pq_sign.rs:66` | deep |
| `PqSigningKey::sign` | `crates/crypto/src/pq_sign.rs:101` | deep |
| `OpaqueServer::generate` (`ServerSetup::new`) | `crates/crypto/src/auth.rs:451` | deep |
| `OpaqueServer::from_bytes` (`ServerSetup::deserialize`) | `crates/crypto/src/auth.rs:475` | deep |
| `OpaqueServer::registration_response`, `login_response`, `login_finish` (each clones the OPRF seed or holds a login's secrets) | `crates/crypto/src/auth.rs:485`, `:512`, `:541` | deep |
| `DeviceKeys::open_plain` (decoding the plain key file) | `crates/app/src/local_unlock.rs:319` | light |
| `DeviceKeys::from_sealed` (decoding the opened secrets) | `crates/app/src/local_unlock.rs:384` | light |
| `DeviceKeys::device_pub` (the Ed25519 rebuild) | `crates/app/src/local_unlock.rs:546` | light |
| `DeviceKeys::sign` (the per-signature Ed25519 rebuild, D617) | `crates/app/src/local_unlock.rs:550` | light |
| `DeviceKeys::kem_pub` (X-Wing's ML-KEM-768 expansion) | `crates/app/src/local_unlock.rs:558` | deep |
| `DeviceKeys::kem_secret`, `DeviceKeys::unlock` | `crates/app/src/local_unlock.rs:570`, `:580` | light |
| `handover::root_key` (unwraps the root key, rebuilds the ISK to check it) | `crates/vault/src/handover.rs:41` | deep |
| `identity::identity_signing_key` (the ISK a record change rebuilds) | `crates/vault/src/identity.rs:262` | deep |
| `pq::identity_pq_key` (the identity's ML-DSA-65 key) | `crates/vault/src/pq.rs:76` | deep |

A keystore called from inside one of these, such as the identity crate's `FileKeystore::unlock` under `root_key`, runs on the same block.

## 2. The crates `psm` brings, and what each target does

| Target | `psm` | What `run` does |
|---|---|---|
| Linux, Android, macOS, iOS (Unix) | `on_stack` available (`x86_64.s`, `aarch_aapcs64.s`) | Block made with `mmap`; guard set with `mprotect(PROT_NONE)`; locked; `on_stack` |
| Windows | Not a dependency. `psm`'s build script turns its stack switching off on Windows, and its README says a stack allocated the usual way is not valid there and points to fibers. | Fiber from `windows-sys` (the 0.61 line already in the lock file). Committed in full; its committed range is found with `VirtualQuery` and locked. |
| `wasm32-wasip1` (the web vault's build) | Not a dependency. `psm` does ship a wasm32 `on_stack`, but it only moves the shadow stack (`__stack_pointer` in linear memory). | Scrub fallback. Linear memory has no protection, so a guard can't fault. A shadow-stack overflow would quietly overwrite the heap below. Nothing can be locked: no `mmap`, no `mlock`, and the engine owns the memory. Return addresses and most locals live on the engine's own stack, which Wasm code can't reach. Switching would cost a dependency and give a false sense of a guard. |

`psm` 0.1.32 builds its assembly with `cc` (1.8.0, already in the lock file) and packs it with `ar_archive_writer` 0.5.3, which uses `object` 0.39.1. Both are build-time only and new to the lock file. `memchr` 2.8.3 is already there. Each new crate has a `safe-to-deploy` exemption in `supply-chain/config.toml`, the pattern every other crate there follows. CI's `cargo vet --locked` checks them. `cargo deny check` passes: `psm` and `object` are MIT OR Apache-2.0, `ar_archive_writer` is Apache-2.0 WITH LLVM-exception, and all are on the allow list. The lock file changes only by the three new packages and two new edges. Cargo's own resolution also wanted to move 16 `windows-sys` edges from 0.60 to 0.61, and that was kept out.

## 3. Measured high-water marks

The deepest byte each call touched, measured by the wipe itself (`testing::last_depth`) in a debug test build on Linux x86_64. A debug build is the deepest case: the generic code that ML-DSA-65 and OPAQUE instantiate in `lockdown-crypto` is compiled unoptimised there.

| Call | Tier | Deepest (bytes) | Block | Used |
|---|---|---:|---:|---:|
| Ed25519 `from_bytes` | light | 4,976 | 49,152 | 10% |
| Ed25519 `From` (clone) | light | 760 | 49,152 | 2% |
| Ed25519 `sign` | light | 9,168 | 49,152 | 19% |
| `DeviceKeys::sign` (rebuild + sign) | light | 9,424 | 49,152 | 19% |
| `DeviceKeys::open_plain` (decode) | light | 12,488 | 49,152 | 25% |
| `DeviceKeys::from_sealed` (decode) | light | 5,424 | 49,152 | 11% |
| ML-DSA-65 `from_seed` | deep | 664,472 | 1,048,576 | 63% |
| ML-DSA-65 `sign` | deep | 182,760 | 1,048,576 | 17% |
| OPAQUE setup `new` | deep | 65,064 | 1,048,576 | 6% |
| OPAQUE setup `deserialize` | deep | 66,824 | 1,048,576 | 6% |
| OPAQUE registration answer | deep | 68,232 | 1,048,576 | 7% |
| OPAQUE login answer | deep | 92,040 | 1,048,576 | 9% |
| OPAQUE login finish | deep | 5,344 | 1,048,576 | 1% |
| `DeviceKeys::kem_pub` (X-Wing) | deep | 84,704 | 1,048,576 | 8% |
| `identity_signing_key` (unwrap + derive) | deep | 51,976 | 1,048,576 | 5% |
| `identity_pq_key` (unwrap + derive + expand) | deep | 666,152 | 1,048,576 | 64% |

**Sizes chosen.** Light: 48 KiB (a multiple of 16 KiB, so it is whole pages on Apple silicon too). Its deepest call uses a quarter of it. Deep: 1 MiB. Its deepest call, ML-DSA-65's expansion, uses under two thirds. The identity's signing-key derivation went past 48 KiB in a debug build (AEAD unwrap included). It overflowed into the light block's guard and faulted, as the guard is meant to, so it moved to the deep tier. That is the reason for the "do not" in the header.

**The guard test.** `every_wrapped_call_stays_well_clear_of_the_guard` (crypto) and the depth checks in the app and vault wipe tests fail when any call uses more than three quarters of its block. That catches a growing call while a quarter of the block is still spare, before it reaches the guard. CI runs it on Linux, macOS and Windows. On Windows the fiber's own frames and the 4 KiB spacer count against the block too.

**Memory locked.** A light block is 12 pages of 4 KiB (3 of 16 KiB). A deep block is 256 pages of 4 KiB (64 of 16 KiB), made only when a deep call first runs. A desktop that opens a vault with post-quantum sharing holds about 1.05 MiB more locked memory, and Settings' page count says so (§6). That fits Linux's 8 MiB default with room to spare. Older kernels' 64 KiB default refuses the deep block, which is counted and logged as before (D607).

## 4. Signing time

`signing_time_on_and_off_the_secret_stack` (crypto) signs 256 bytes 200 times each way with the same key and prints the mean. Debug build, this machine:

| | Mean per signature |
|---|---:|
| On the thread's stack (`ed25519_dalek` direct) | 115.7 µs |
| On the secret stack (`SigningKey::sign`) | 124.8 µs |

That is about 8% more in a debug build. It adds no system call: the switch is a few instructions, and the wipe is one `memcmp` pass over 48 KiB plus a memset of the 9 KiB touched. The first call on a fresh block pays for the `mmap` and `mlock` once. An earlier version scanned a 1 MiB block word by word after each Ed25519 call and took 378 µs per signature. That is why there are two sizes and the `memcmp` search.

## 5. How to run and test

```
export CARGO_PROFILE_DEV_DEBUG=line-tables-only
cargo test -p lockdown-crypto --lib secret_stack -- --nocapture   # depths and timing printed
cargo test -p lockdown-crypto --lib nothing_on_the_secret_stack   # Ed25519, ML-DSA-65, OPAQUE
cargo test -p lockdown-crypto --test one_page_table               # slice 52's page-lock test
cargo test -p lockdown-app --lib local_unlock -- --nocapture      # DeviceKeys
cargo test -p lockdown-vault --lib nothing_on_the_secret_stack -- --nocapture
```

**The tests:**

- **The scan.** `secret_stack::testing::pinned(f)` keeps this thread's blocks out of the shared pools while `f` runs. `holds(bytes)` then says whether either block holds those bytes, between calls.
- **After each wrapped constructor and after a signature,** the scan finds neither the seed nor a known slice of the expanded key:
  - Ed25519: the seed, and the nonce prefix, the upper half of SHA-512(seed) (RFC 8032 §5.1.6).
  - ML-DSA-65: the seed, and 64 bytes from the middle of the boxed expanded key.
  - OPAQUE: the OPRF seed and the server's private key, the first 96 bytes of the setup's serialization.
  - `DeviceKeys`: the device seed, the nonce prefix, the KEM seed and the unlock key.
  - The ISK: its seed.
- **The control.** The same calls run with the wipe off, through `testing::unwiped`, which exists only in the crypto crate's own test build (D667). The scan then finds the seed and the slice after:
  - Ed25519 `from_bytes` and `From`;
  - an Ed25519 signature;
  - the rebuild-then-sign pattern `DeviceKeys` uses;
  - ML-DSA-65 `from_seed`, the 64 KiB copy of D606;
  - OPAQUE `new` and `deserialize`.

  After `wipe()`, it finds none. So the scan can fail. The app and vault tests can't turn the wipe off. They check that their calls ran on a block (`last_depth() > 0`) and left nothing.
- **The mechanism:** a call runs inside the block, not on the caller's stack. Calls nest, and the outermost call on each block wipes. A panic inside is wiped and carries on. The fallback scrub clears the frames it covers (run inside a block, so its effect can be read). Blocks move between threads through the pools.
- **The guard:** the depth tests in §3.
- **Slice 52's page-lock test** (`one_page_table`) makes both blocks first, so its process-wide counts see only what each step makes. Otherwise it is unchanged and passes.

**Test counts:**

| Suite | Before | After |
|---|---:|---:|
| `lockdown-crypto` lib | 47 | 57 |
| `lockdown-crypto` `one_page_table` | 1 | 1 |
| `lockdown-app` lib | 125 | 126 |
| `lockdown-vault` lib | 127 | 128 |

## 6. Gaps

- **The identity crate's `FileKeystore` is external.** It comes from `snds/ecosystem-identity` at rev `35d2a56` as a git dependency and is not vendored here, so it is unchanged. Its `sign` and `device_pub` rebuild the Ed25519 key on the caller's stack (`keystore.rs` `signing_key()`, lines 152–183). So do `kem_pub` and `kem_secret` with the KEM seed, and `attach::signing_key` with the ISK. Lockdown's desktop and CLI use `DeviceKeys` (§1). `FileKeystore` serves the web vault's WASI host (`crates/web/src/host.rs`) and tests. Where Lockdown calls into the crate from a wrapped site, such as `root_key` and `identity_signing_key`, the crate's code already runs on the block. Its own direct callers don't.
  - **The change the identity repo needs:** a stack-wipe primitive in `memlock`, the same shape as this one: `psm::on_stack` onto a locked, guarded block on Unix, a fiber on Windows, a scrub elsewhere. Then:
    - wrap `FileKeystore::{sign, device_pub, kem_pub, kem_secret, unlock}` and `attach::signing_key` in it;
    - have `Identity`'s record signing call `sign` inside it.

    Better still, the crate could take Lockdown's primitive as a small public function both crates share, as `PageLock` is shared (D601), so there is one pool per process. D668 records it.
- **Copies made before the switch.** A seed passed by value (`from_bytes(seed: [u8; 32])`, `from_seed(seed)`) is copied into the callee's frame on the caller's stack before `run` starts. Lockdown wipes the copies it owns (`seed.zeroize()`). The caller's own copies, and moves the compiler makes before the call, are unchanged from doc 52 §6 and doc 53 §6.
- **What a closure returns** is moved to the caller's stack. Every wrapped call returns a box, a locked key type, a `Result` of one, or a public value. None returns a bare secret.
- **A light call that outgrows its block crashes.** It hits the guard, as designed, rather than writing past it. Release frames are smaller than the debug ones measured. The depth tests catch growth in CI first.
- **Windows:**
  - A fiber's stack is committed in full, but the reservation below it lets the system grow it past the committed part. A call that went that far would use unlocked pages that the wipe doesn't reach. It would have to pass three quarters of the block first, and the depth test fails at that point.
  - The deep fiber's lock (about 256 pages) is beyond what Windows' default minimum working set lets a process lock (about 42 pages). So on Windows the deep block is refused and counted, and Settings shows "this computer refused 1" once a deep call has run. The block is still wiped after every call. Raising the working-set minimum (`SetProcessWorkingSetSize`) would change that. It was not done here (D664, flagged for Sean). The light fiber (12 pages) fits.
  - A thread that is already someone else's fiber can't be switched from without reading the thread block, so it scrubs. Lockdown makes no fibers of its own apart from these.
- **WASI scrubs, best effort.** The compiler decides where frames go, so the scrub covers the closure's frames only if they lie within 64 or 256 KiB below the caller. On WASI the engine's own stack, with return addresses and most locals, can't be reached at all. The web vault's keys live under its export-key seal (doc 17), and its device keys are the identity crate's `FileKeystore`, above.
- **Signal handlers** that run while a thread is on a block use the block, and are wiped with it.
- **Settings' page count rises** by the blocks' pages: 12, or 268 once a deep call has run. The words are unchanged (D669).
- **Unchanged from doc 53 §6:** pending OPAQUE logins (`LoginServerState`, plain bytes in `AppState::pending`), the core-pattern pipe handler, low lock limits (D607), `Revealed`, the extension's JSON frame, and Argon2id's working memory.

## 7. The wipe-off switch (fix chosen)

The first push had a `test-support` feature on `lockdown-crypto`, so the app and vault tests could run calls with the wipe off. The guard on `test-support` features (`crates/app/tests/no_plain_joins.rs`, D652) failed on it, and a feature that switches off the wipe is a downgrade switch any manifest could turn on. **Fix (a):**

- The feature is gone.
- `SKIP_WIPE` and `testing::unwiped` exist only under `#[cfg(test)]` in `lockdown-crypto` (`pub(crate)`). No build of any other crate, test builds included, can leave a block unwiped.
- `secret_stack::testing` keeps only `pinned`, `holds`, `last_depth` and `wipe`. They read, or wipe again, and can't turn the wipe off.
- Every control with the wipe off is in the crypto crate. The app and vault tests check that their calls ran on a block and left nothing.

## 8. Unicron host-contract gaps

None hit. No UI changed.

## 9. Proposed decisions

| # | Decision | Status |
|---|---|---|
| D660 | **The secret stack.** `lockdown_crypto::secret_stack::run`/`run_deep` run a closure on a block of their own: page-aligned, a 256 KiB no-access guard below, its pages locked through `locked::PageLock` (the one table, D601; refusals counted). After each call the block is zeroed from the deepest byte touched up to the top, before the call returns. A panic is caught on the block, wiped, and resumed outside. Calls nest, and the outermost call on each block wipes. Plain version: the brief copies of keys that making or using a key leaves behind now sit only in memory kept out of swap, and are erased straight away. | Proposed; audit item |
| D661 | **Two block sizes, pooled.** Light, 48 KiB, for Ed25519. Deep, 1 MiB, for ML-DSA-65, the OPAQUE server and the identity derivations. Sized from measured debug high-water marks (§3: 12,488 and 666,152 bytes at most), with a test that fails past three quarters of a block on all three CI systems. Blocks sit in one pool per size shared by threads (at most 4 idle each), so locked memory follows concurrent key operations, not thread count. A light call that outgrows its block faults at the guard. Plain version: the protected memory is sized from measurements, with a test that warns well before it runs short. | Proposed |
| D662 | **Every by-value key constructor runs on the secret stack:** `ed25519_dalek::SigningKey::from_bytes`, the clone in `SigningKey::from`, `ExpandedSigningKey::from_seed`, `ServerSetup::new` and `ServerSetup::deserialize`. Tests scan the block for the seed and a known slice of the key after each, and find them with the wipe off. **Closes D606, and the constructor half of D617.** Plain version: making a key no longer leaves a stray copy of it in memory. | Proposed; audit item |
| D663 | **Every signature and key rebuild runs there too:** Ed25519 `sign`; `DeviceKeys`'s per-signature Ed25519 rebuild, `device_pub`, KEM rebuilds, unlock key and key-file decoding; ML-DSA-65 `sign`; the OPAQUE server's registration and login answers; and the identity derivations (`root_key`, `identity_signing_key`, `identity_pq_key`). The device's signing key exists only on locked pages, which are wiped after. Signing costs about 8% more in a debug build (115.7 → 124.8 µs) and no system call. **Closes the rest of D617.** Plain version: signing with this computer's key leaves nothing behind in memory, at no cost anyone notices. | Proposed; audit item |
| D664 | **Windows uses fibers.** `psm` has no stack switching on Windows, so a block is a fiber committed in full (`windows-sys`, already in the lock file), locked, and wiped by the fiber itself before it switches back. A thread that is already a foreign fiber scrubs instead. The deep fiber's lock exceeds Windows' default lockable working set, so it is refused and counted (Settings: "refused 1") but still wiped. Raising the working-set minimum would let it lock. That was not done. Plain version: on Windows the large protected area may not be kept out of swap, though it is still erased after each use. | Proposed; **flagged for Sean** (whether to raise the working-set minimum) |
| D665 | **WASI scrubs instead.** No switch on `wasm32-wasip1`: linear memory has no guard to fault and nothing to lock, and the engine's stack can't be reached. After the call, a never-inlined function zeroes 64 or 256 KiB of stack below the caller. Best effort, and the doc says so. Plain version: in the web vault, leftover copies are overwritten as far as a web page can reach. | Proposed |
| D666 | **New dependency: `psm` 0.1.32** (Unix only), with build-time `ar_archive_writer` 0.5.3 and `object` 0.39.1. Each has a `safe-to-deploy` exemption in `supply-chain/`. Licences are on the allow list, and `cargo deny check` passes. No assembly of Lockdown's own. Plain version: one small, widely used library (the one the Rust compiler uses to grow its stack) does the stack switch. | Proposed; **flagged for Sean** (a new third-party crate in the crypto crate); audit item |
| D667 | **No wipe-off switch outside the crypto crate's tests.** The control that leaves a block unwiped exists only under `#[cfg(test)]` in `lockdown-crypto`. There is no cargo feature for it, so no build of the apps, server, CLI or another crate's tests can turn it on. `secret_stack::testing` offers only read-only checks. Fix (a) of the coordinator's two (§7). Plain version: nothing shipped can switch the erasing off. | Proposed; audit item |
| D668 | **Recorded gap: the identity crate's `FileKeystore` and `attach::signing_key`** rebuild keys by value on the caller's stack. The crate is external (`snds/ecosystem-identity`), so it is unchanged here. The identity repo should add the same primitive to `memlock` (or share Lockdown's) and wrap `FileKeystore::{sign, device_pub, kem_pub, kem_secret, unlock}` and `attach::signing_key` (§6). Lockdown's own calls into it from wrapped sites already run on the block. Plain version: the shared identity library still leaves brief key copies when used on its own, and the fix belongs in that library. | Proposed; recorded gap |
| D669 | **Scope.** No wire, IPC, wasm, view, stored-format or extension change. Settings' memory line counts the blocks' locked pages (12, or 268 once a deep call has run), with no word change. On Windows it may also show one refused lock (D664). Plain version: nothing the apps send, store or say changes, apart from those numbers. | Proposed; **flagged for Sean** (the visible page count on Settings) |
