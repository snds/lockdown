# 59: Lockdown makes room in its Windows working set so its locks succeed

_Slice: `slice/win-working-set`, built 2026-11-05, on Sean's go (2026-11-05). Answers D664's open question (doc 58: on Windows the deep secret-stack block's lock was refused, so Settings showed "this computer refused 1"). Builds on D314 (secrets kept out of swap), D601 (one page table per process), D607 (a refused lock is counted and never fatal) and D660–D664 (doc 58). No wire, IPC, wasm, view, stored-format, wording or extension changes. No new dependency: two more `windows-sys` features on the line already in the lock file. Proposed decisions D670–D679 (§8)._

## For future agent

- **TL;DR:** On Windows, `VirtualLock` lets a process lock about as many pages as its minimum working set holds, about 42 by default. The deep secret-stack block alone is 256 pages. Now every `lockdown_crypto::locked::PageLock` first makes room for itself. It raises the process's own minimum working set (and the maximum with it) by what the lock needs plus a margin, with `SetProcessWorkingSetSizeEx`, and only then locks. Each release gives room back. The extra room is capped at 32 MiB. A lock past that is refused and counted as before. Linux and macOS are unchanged. People see one thing change: on a normal Windows computer, Settings no longer shows a refusal.
- **Read with:**
  - `crates/crypto/src/locked/working_set.rs`: the module notes, then `locking`, `unlocking`, `settle` and `set`.
  - `crates/crypto/src/locked.rs`: the `PageLock` wrapper.
  - `crates/crypto/tests/win_working_set.rs`: the Windows test, one function in five steps.
- **Do not:**
  - Call `SetProcessWorkingSetSize(Ex)` anywhere else. Room is counted in one place, under one mutex.
  - Pass `QUOTA_LIMITS_HARDWS_MIN_ENABLE` (or any flag). Flags stay 0, so the limits stay soft.
  - Lock through `ecosystem_identity::memlock::PageLock` from Lockdown code. Use `locked::PageLock`, which makes the room. (`one_page_table` calls the crate's lock directly on purpose, to test the shared table.)
  - Raise `working_set::BOUND` to cover a test. A lock past the bound should be refused (D607).

## 1. Architecture

```text
 Lockdown code                     locked::PageLock (Lockdown)                identity crate (one table, D601)
 ───────────────                   ─────────────────────────────              ─────────────────────────────────
 Key32, SecretBytes, SigningKey,   of / of_value(range)
 PqSigningKey, OpaqueServer,         │  Windows: working_set::locking(spans)   memlock::PageLock::of(range)
 DeviceKeys, secret-stack blocks     │    ① need = table pages + spans + SLACK      per page: VirtualLock
                                     │    ② need > room and ≤ BOUND?                refusal → REFUSED += 1
                                     │         raise min (and max) to room = need + STEP
                                     │    ③ lock ────────────────────────────────► (succeeds: room was made)
                                     │    ④ refused anyway? settle (give back)
 drop                                └► working_set::unlocking: release ──────► unlock per page
                                          then settle: room > need + 2 STEP? lower to need + STEP
 identity crate's own keys  ───────────────────────────────────────────────► memlock::PageLock (fits in SLACK)
```

| Piece | What it does |
|---|---|
| **`locked::PageLock`** | Was a re-export of the identity crate's `memlock::PageLock`. Now it is a small wrapper with the same API (`of`, `of_value`, `pages`) that holds the crate's lock inside. So it is still one table, and `report()` and Settings read it as before. On Linux, macOS and WASI it only calls through. On Windows it calls `working_set::locking` before locking and `working_set::unlocking` around its release. |
| **`working_set::locking`** | Takes the module's mutex. Reads the starting limits once (`GetProcessWorkingSetSizeEx`). Works out `need` = the bytes the table holds now + the whole pages the new lock spans + `SLACK` (256 KiB). If `need` is more than the room already made and within `BOUND`, it raises the room to `need + STEP` (256 KiB more), capped at `BOUND`. Then it runs the table's lock under the same mutex. If the system refused anyway, it gives back what was raised for it. |
| **`working_set::unlocking`** | Takes the mutex, runs the table's release, then calls `settle`. If the room is more than two steps above what the table needs (`held + SLACK + STEP`), the room is lowered to that. |
| **`set`** | Minimum = starting minimum + room. Maximum = the larger of the starting maximum and the new minimum plus the starting gap between them. `SetProcessWorkingSetSizeEx` with flags 0, which leaves the process's enforcement as it was (soft by default). If the system answers `ERROR_PRIVILEGE_NOT_HELD`, it enables `SeIncreaseWorkingSetPrivilege` in the process's own token (every user holds it) and tries once more. |
| **The bound** (`locked::WORKING_SET_BOUND`, 32 MiB) | The most room Lockdown adds above the starting minimum. A lock that would need more is given no room and goes to the system, which refuses it: counted, never fatal, and the secret still wiped (D607). |
| **`locked::working_set_room()`** | Windows only: the room added now, in bytes. The test compares it with what the system reads back. |

### Why the room is made before the lock, not after a refusal

The brief's first shape was to retry after `VirtualLock` fails with `ERROR_WORKING_SET_QUOTA`. That would have to happen inside the page table. The table is the identity crate's (`ecosystem_identity::memlock`, D601), pinned by commit and consumed, never forked. Its Windows arm adds 1 to the refusal counter before it returns `None`, so a retry made by Lockdown after that would still show the refusal on Settings. So Lockdown makes the room first. The lock then succeeds the first time, and nothing is counted. §5 records the change the identity repo can make so the retry happens inside the table.

### Why on demand, not a fixed size at start

What a process locks keeps changing:

- The secret stack's pools keep up to 4 idle blocks of each size (4 × 1 MiB and 4 × 48 KiB). A block is made only when that many calls run at the same time.
- Other `PageLock` users lock as they come and go: every `Key32` and `SecretBytes`, the ML-DSA key (17 or 18 pages), the Ed25519 keys, the OPAQUE setup, `DeviceKeys`, and the identity crate's keys.

A number chosen at start would be too small for a busy server, or a large raise for a desktop that runs one call at a time. Growing on demand raises the minimum by what is locked, plus at most two steps. A desktop that has run one deep call raises it by about 1.6 MiB.

### Why these numbers

- **`SLACK`, 256 KiB (64 pages):** the identity crate's own keys lock through the table directly, without coming through here. Each takes a page or two. The slack keeps room for them after each of Lockdown's locks.
- **`STEP`, 256 KiB:** a run of small locks (a `Key32` per item opened) raises the limits once, not once per page. Lowering waits until the room is two steps above what is needed, so a key made and dropped in a loop doesn't change the limits each time.
- **`BOUND`, 32 MiB:** holds both pools full, every key, and more than twenty deep calls at once. That is well past what the desktop or the CLI ever holds. It is four times Linux's usual 8 MiB `RLIMIT_MEMLOCK`, because there the limit is the system's and here it is Lockdown's own. A minimum of 32 MiB is small next to the memory of any computer Lockdown runs on, and it is soft, so Windows can still trim the process under pressure. It only can't page out the locked pages.

## 2. What each piece does, step by step

1. **First lock.** The mutex state is `Unread`. `locking` reads the starting minimum and maximum. If they can't be read, the state becomes `Off` and every lock goes ahead as before (D607).
2. **Each lock.** `need` is compared with the room. A raise sets both limits, then the room. If the raise is refused (by policy or memory pressure), the room stays where it was and the lock goes ahead, to be counted if refused.
3. **Each release.** The table's release runs first, under the mutex, then `settle`.
4. **Serialization.** Reading the room, raising it, the table's lock and the release all happen under one `Mutex` in `working_set`. Lockdown can't take the table's own mutex (it is private to the identity crate), so it holds its own around the table's calls. Lock order is always this mutex, then the table's. The table never calls back into Lockdown, so there is no deadlock. A poisoned mutex is taken anyway (`PoisonError::into_inner`): the state is a few integers that are never left half-written.

## 3. How to run and test

```sh
export CARGO_TARGET_DIR=/home/user/lockdown/target CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=line-tables-only
cargo test -p lockdown-crypto --test win_working_set
cargo test -p lockdown-app --test memory_report
# Windows type check without a Windows machine (blake3's build script needs
# ml64.exe to cross-build its assembly; the pure-Rust path skips it):
CARGO_FEATURE_PURE=1 cargo clippy -p lockdown-crypto --all-targets --target x86_64-pc-windows-msvc -- -D warnings
```

`win_working_set` is one test function, so nothing else in its binary moves the process-wide counts and limits. On Windows (CI `fmt · clippy · test (windows-latest)`) it does five steps:

1. **A deep call.** `run_deep` locks its block (`kind(Deep) == Locked`), `report().refused` stays 0 (the number Settings shows), and the minimum read back from the system has risen by at least 1 MiB. The maximum is at least the minimum and the starting maximum, the hard-minimum flag is off, and the minimum is exactly the starting one plus `working_set_room()`.
2. **Four deep blocks and four light ones at once.** Eight threads, each waiting on one barrier inside its call, so all eight blocks are live together. All lock, nothing is refused, and the minimum covers them all.
3. **Keys beside them.** The ML-DSA and Ed25519 keys lock, still with no refusal.
4. **The room comes back.** An 8 MiB lock raises the minimum by at least that much. Its release lowers the minimum by more than half of it, still above what stays locked, and the minimum matches `working_set_room()` again.
5. **Past the bound.** A lock of 40 MiB (the bound plus 8 MiB) is refused, counted once, holds nothing and is not fatal. The limits are not raised for it.

On Linux and macOS the same file runs steps 1 to 3 under the system's own limit: each block is locked, or refused and counted, as before. `memory_report` (app) adds, on Windows only: after opening the vault and a deep call, `App::memory_report().refused` is 0.

**Test counts:**

| Suite | Before | After |
|---|---:|---:|
| `lockdown-crypto` lib | 57 | 57 |
| `lockdown-crypto` `one_page_table` | 1 | 1 |
| `lockdown-crypto` `win_working_set` | — | 1 |
| `lockdown-app` `memory_report` | 1 | 1 (one Windows assertion more) |

## 4. Gaps

- **CI's Windows runner is an administrator account.** The privilege fallback (`ERROR_PRIVILEGE_NOT_HELD` → enable `SeIncreaseWorkingSetPrivilege`) is the path a standard user may take. Windows gives that privilege to every user by default, so it should succeed, but CI doesn't run as a standard user. It is type-checked, not run.
- **Something else in the process that changes the working set** (a library calling `SetProcessWorkingSetSize`, or `EmptyWorkingSet`) isn't seen. Lockdown keeps counting from the limits it read at its first lock. Nothing in Lockdown does this.
- **Locks past 32 MiB at once are refused**, by design (D672). Only a server running more than twenty deep calls at once could get there. The server already logs refusals once at start (D615), and later ones are counted.
- **The room stays raised by about 512 KiB** above what is locked, after the first lock. This is the hysteresis in §1. It is a soft minimum and costs nothing while the memory isn't used.
- **The identity crate's own direct locks don't make room.** They fit in `SLACK`. A burst of more than 64 pages of the crate's own keys between two of Lockdown's locks would be refused and counted. No such burst exists today: the crate's keys are a few 32-byte boxes. §5 has the fix.

## 5. The identity crate

The identity crate locks through the shared `PageLock` table. It owns that table (`ecosystem_identity::memlock`, D601) and has no other `VirtualLock` path: every one of its key types (the root key, Persona Keys, App Data Keys, the KEM seeds, the Identity Signing Key, `FileKeystore`'s seeds) goes through `memlock::PageLock`. Covered as follows:

- Lockdown's `PageLock` holds the crate's lock inside, so it is one table, one count and one report, as before.
- The crate's own direct locks don't go through `working_set::locking`. Lockdown keeps `SLACK` (256 KiB) of room above whatever the table holds after each of its own locks, and the crate's keys fit in that. On a fresh process, before Lockdown's first lock, they fit in Windows' default (about 42 pages).

**The change the identity repo can make** so the room is made inside the table (recorded as D676, not done here: the crate is external):

- In `src/memlock.rs`, Windows `os::lock`: when `VirtualLock` fails and `GetLastError()` is `ERROR_WORKING_SET_QUOTA`, read `GetProcessWorkingSetSizeEx`. Raise the minimum, and the maximum if needed, by the region plus a margin with `SetProcessWorkingSetSizeEx` (flags 0), within a bound. Then retry `VirtualLock` once.
- `PageLock::range` already calls it under `PAGES`, so it is serialized. In `release`, lower the minimum when the table's pages fall well below it.
- Count a refusal only after the retry fails.
- Then Lockdown's `working_set` module can go, and `locked::PageLock` can go back to a plain re-export.

## 6. Unicron host-contract gaps

None hit. No UI changed.

## 7. Gates run

From `/home/user/ld-wws`:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `CARGO_FEATURE_PURE=1 cargo clippy -p lockdown-crypto --all-targets --target x86_64-pc-windows-msvc -- -D warnings`
- `cargo clippy -p lockdown-web --target wasm32-wasip1 -- -D warnings`
- `cargo test -p lockdown-crypto`
- `cargo test -p lockdown-app`
- `cargo deny check`
- `cargo clippy -p lockdown-desktop --all-targets --features shell -- -D warnings`
- `cargo test -p lockdown-desktop --features shell`
- In `apps/desktop`: `pnpm typecheck`, `lint`, `test`, `build`, and `ds:ratchet` (baseline 4)

CI's `fmt · clippy · test (windows-latest)` job is the real Windows test. On the first push it passed (all 19 CI jobs green), with these lines:

```text
     Running tests\win_working_set.rs
test lockdown_makes_room_in_its_working_set_so_its_locks_succeed ... ok
     Running tests\memory_report.rs
test the_settings_report_counts_the_identity_crates_keys_too ... ok
     Running tests\one_page_table.rs
test lockdown_and_the_identity_crate_share_one_page_table ... ok
test secret_stack::tests::a_call_runs_on_a_locked_block_and_leaves_it_zeroed ... ok
```

## 8. Proposed decisions

| # | Decision | Status |
|---|---|---|
| D670 | **Lockdown raises its own Windows working-set limits so its locks succeed.** Before each `locked::PageLock`, Lockdown works out the room the lock needs: the bytes the one table holds, plus the pages the lock spans, plus 256 KiB of slack. If that is more than the room already made, it raises the minimum working set (and the maximum with it) by that much plus a 256 KiB step, with `SetProcessWorkingSetSizeEx`, and then locks. The deep secret-stack block, both pools full and every key all lock. Settings' refused count stays 0 on a normal Windows computer. **Closes D664's open question.** Plain version: on Windows the protected memory is now really kept out of swap, and the "this computer refused" line no longer appears. | Proposed; **flagged for Sean** (the visible change: the Windows refusal line goes away); audit item |
| D671 | **The room is made before the lock, not after a refusal.** The page table and its refusal counter are the identity crate's (D601), and its Windows arm counts a refusal before Lockdown could retry. So `locked::PageLock` became a wrapper that holds the crate's lock inside (still one table), and makes the room first. Plain version: the lock works the first time, so nothing is miscounted. | Proposed |
| D672 | **The growth is bounded at 32 MiB** of room above the starting minimum (`locked::WORKING_SET_BOUND`). A lock that would need more gets no room, and the system refuses it: counted, never fatal, and still wiped (D607 unchanged, no new wording). Plain version: Lockdown never asks Windows for more than 32 MiB of protected memory. | Proposed; audit item |
| D673 | **Soft limits, no admin rights.** Flags are passed as 0, so the process's enforcement stays as it was (soft), and `QUOTA_LIMITS_HARDWS_MIN_ENABLE` is never set. If a raise is refused with `ERROR_PRIVILEGE_NOT_HELD`, Lockdown enables `SeIncreaseWorkingSetPrivilege` in its own token (every user holds it by default) and tries once more. Nothing is asked of the person. Plain version: it needs no administrator and no prompt, and Windows can still reclaim Lockdown's other memory. | Proposed; audit item (a privilege enabled in the process token) |
| D674 | **Releases give the room back.** After each release, if the room is more than two 256 KiB steps above what the table needs, the minimum is lowered to the table's pages plus the slack and one step. The gap keeps a key made and dropped in a loop from changing the limits each time. Plain version: memory Lockdown stops protecting goes back to Windows. | Proposed |
| D675 | **One mutex serializes the limits.** Reading the room, raising it, the table's lock and its release all run under one mutex in `locked::working_set`, taken before the table's own. Plain version: two threads can't race each other's changes. | Proposed |
| D676 | **Recorded gap: the identity crate's own direct locks don't make room.** They fit in the 256 KiB slack Lockdown keeps after each of its locks. The identity repo can move the grow-and-retry into `memlock`'s Windows `os::lock`, under its table's lock, and count a refusal only after the retry fails (§5). Lockdown's wrapper can then go back to a plain re-export. Plain version: the shared identity library's own keys are covered by Lockdown's spare room, and the clean fix belongs in that library. | Proposed; recorded gap |
| D677 | **Scope.** No wire, IPC, wasm, view, stored-format, extension or wording change. No new dependency: `Win32_Foundation` and `Win32_Security` features on the `windows-sys` 0.61 line already in the lock file. Linux, macOS and WASI are unchanged: `PageLock` calls straight through. Plain version: nothing the apps send, store or say changes, apart from Windows no longer reporting a refusal. | Proposed |
