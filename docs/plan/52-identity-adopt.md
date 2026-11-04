# 52: Lockdown takes the identity crate's new version

_Slice: `slice/identity-adopt`, built 2026-11-04. The D378 follow-up. `snds/ecosystem-identity` PR #2 merged as `35d2a56` (its decision 0003): a Windows named-pipe handshake with a process-identity peer check, and `memlock`, the crate's own key types locked out of swap through one page table. Builds on D33 (the handshake socket and its process-identity check), D314 (secret pages out of swap, doc 26 §7) and doc 28 §9 (the ML-DSA key outside D314). No wire, IPC, wasm, view or extension schema changes. Proposed decisions D600–D609 (§10)._

## For future agent

- **TL;DR:** Lockdown pins the identity crate at `35d2a56`. `lockdown_crypto::locked::PageLock` is now the crate's `ecosystem_identity::memlock::PageLock`, so Lockdown's secrets and the crate's keys share one page table and one count per page. A page holding one of each stays locked until both are gone. `locked::report()` reads `memlock::report()`, so Settings counts both. `harden_process` and the core-dump policy stay in `lockdown-crypto`. The ML-DSA expanded signing key (`PqSigningKey`) is now locked beside its box. That also covers the server's post-quantum log key, which is a `PqSigningKey`. Lockdown's own Windows pipe (`crates/pipe`) is unchanged.
- **Read with:**
  - `crates/crypto/src/locked.rs`: the re-export, `report`, `harden_process`.
  - `crates/crypto/src/pq_sign.rs`: `PqSigningKey` (`pages`, `is_locked`, `locked_pages`).
  - `crates/crypto/tests/one_page_table.rs`: the shared-page case, the real key types side by side, the ML-DSA key, refusals.
  - `crates/app/tests/memory_report.rs`, and the `keys` unit tests in `lockdown-server`.
  - The crate's `docs/decisions/0003-windows-peer-and-locked-keys.md` and `src/memlock.rs`.
- **Do not:**
  - Call `mlock`, `munlock`, `VirtualLock` or `VirtualUnlock` anywhere in Lockdown. Lock through `locked::PageLock` (the crate's), or a page another secret still needs can be unlocked.
  - Lock a value that can move. `PageLock::of_value` is for a value behind a `Box` or another fixed heap block.
  - Drop the lock before the secret. Keep the secret field first and the `PageLock` field after it: fields drop in order, so the secret is wiped while its pages are still locked.

## 1. The pin

`Cargo.toml` names `rev = "35d2a56b9d5dab92ebf2ee447427bd083cef0a04"`. That is the crate's `main` after PR #2. `Cargo.lock` changed in two places:

- The crate gained `windows-sys` 0.61.2 as a Windows-only dependency.
- `lockdown-crypto` lost it, because its own `VirtualLock` code is gone.

`windows-sys` 0.61.2 was already in the lock file through `tempfile` and others. It is already exempt in `supply-chain/config.toml`. `cargo deny check` passes with no change to `deny.toml`. `cargo vet` needs no new entry: no new crate or version came in, and git dependencies are outside `cargo vet`.

The crate's API change is additive (decision 0003, "API"). No Lockdown call site had to change for the bump itself. `PeerIdentity` is `non_exhaustive`, and Lockdown matches it only with `matches!`.

## 2. One page table

**Before.** Two tables, one in each crate. Each counted the secrets on a page and called `munlock` when its own count hit zero. The kernel keeps one lock bit per page, not a count. So when a Lockdown `Key32` and a crate `AppDataKey` sat on one page (both are a boxed `[u8; 32]`, so they often do), whichever was dropped first unlocked the page under the other. The survivor was still wiped on drop, but it could reach swap.

**Now.** `lockdown_crypto::locked::PageLock` is a re-export of `ecosystem_identity::memlock::PageLock`. `Key32`, `SecretBytes` and `PqSigningKey` lock through it, and so does every crate key type. The table is the crate's: one count per page for both crates.

**What stayed in `lockdown-crypto`:**

- `harden_process()`: `RLIMIT_CORE` 0 on Linux and macOS, never `PR_SET_DUMPABLE` (D155). Process policy, so not the shared crate's job (decision 0003, "Out of scope").
- `MemoryReport` and `report()`, with the same fields as before. The page numbers are copied from `memlock::report()`. `core_dumps_off` is Lockdown's own flag.
- `supported()` and `page_size()`, now the crate's.

**Public API.** Unchanged for callers. `PageLock::of` is still there. The crate's type adds `of_value` and `pages`. `libc` stays a Unix dependency for `setrlimit`.

## 3. What Settings now counts

Settings → Quick access reads `memory_report`. The desktop and the web vault both call `App::memory_report`, which calls `locked::report()`. The numbers now count every page locked in the process:

- Lockdown's `Key32` and `SecretBytes`;
- the ML-DSA signing key: 17 or 18 pages of 4 KiB, or 5 or 6 of 16 KiB on Apple silicon;
- the crate's keys: the root key, persona keys, the App Data Key, the unlock keys, the KEM seeds, the Identity Signing Key an open `Identity` holds, and `FileKeystore`'s seeds.

Refusals are counted in the same place. The wording is unchanged: "Lockdown keeps N pages of keys out of swap now; this computer refused M." Every page counted holds keys Lockdown is holding, so the sentence stays true. The number is larger than before while the post-quantum key is held: by about 17 pages on a 4 KiB-page computer, about 5 on Apple silicon (D604). The IPC shape, the TypeScript `MemoryReport` and the design-capture fixture are unchanged.

## 4. The Windows handshake

Lockdown doesn't use the crate's Windows handshake. Its own Windows channel is `crates/pipe` (`\\.\pipe\snds-lockdown-browser-<hash>`, a user-only DACL, local clients only, first instance only). It carries the browser host's frames. It reports `PeerIdentity::Unknown` and checks the peer through `PipeVerifier::bind` / `BoundVerifier`. It stays as it is (D602). Only its module doc changed: it no longer says the crate has no Windows variant.

**Nothing in Lockdown calls the crate's handshake on Windows:**

- The desktop's browser endpoint and the native host use `UnixSocketTransport` behind `cfg(unix)`.
- Device linking uses only `handshake::wire`, which is platform-neutral.

So no Windows build path is new. CI's Windows job builds the crate's new `handshake::windows` module as a dependency.

The crate's own pipe, `\\.\pipe\snds-identity-handshake-<16 hex>`, is for its identity handshake between ecosystem apps (Mailbird, Synapse). D600 records it as D33 on Windows.

## 5. The ML-DSA key and the server's log key

**Size.** `ExpandedSigningKey<MlDsa65>` is 65,664 bytes in memory:

- `rho`, `K` and `tr`;
- `s1`, `s2` and `t0`, and their NTT forms;
- the expanded matrix `A_hat`.

That is 64 polynomials of 256 `u32` coefficients, held inline. The "about 4 KB" in doc 28 §9 was the encoded secret key (4032 bytes). The unit test `the_expanded_key_is_locked_out_of_swap_or_counted_as_refused` checks the size, at 64 KiB or more.

**How it is locked.** `PqSigningKey` holds `key: Box<ExpandedSigningKey<MlDsa65>>` and then `pages: Option<PageLock>`, made by `PageLock::of_value(&*key)`. The box is one heap block that never moves. Field order wipes the key first, through `ml-dsa`'s `Drop` (the `zeroize` feature is on), and unlocks the pages after.

`memlock::Locked<T>` would also work, but `Locked::new` takes the value by value. That moves the 64 KiB key across the stack once more before it reaches the heap. Locking beside the existing box avoids that copy (D603).

**The seed.** `PqSigningKey` used to keep a copy of its 32-byte seed. Nothing read it, so it is gone, and the custom `Drop` that wiped it went with it. `from_seed` now wipes its by-value argument and the `Seed` copy once the key is expanded.

**New:** `is_locked()`, `locked_pages()`, and the constant `EXPANDED_KEY_LEN` (the key's size in memory, for the page counts).

**The server's post-quantum log key** (`ServerKeys::pq_log_key`) lives as long as the server. It is a `PqSigningKey`, so the change above locks it. A server unit test checks it.

## 6. What still isn't locked

- **Lockdown's Ed25519 `SigningKey`.** It holds `ed25519_dalek::SigningKey` inline, wherever its owner puts it. That covers this device's signing key in the vault, and the server's token and log keys. Wrapping it in `memlock::Locked`, as the crate now does for the ISK, is proposed in D605, not built.
- **The OPAQUE server setup** (`OpaqueServer`), on the server. Also D605.
- **The stack temporary in `from_seed`.** `ExpandedSigningKey::from_seed` returns the key by value. It is built on the stack and then moved into the box, and that stack copy isn't wiped. `ml-dsa` has no in-place constructor (D606). This was already so before this slice.
- **Low lock limits.** Kernels before Linux 5.16, and some containers and service managers, set `RLIMIT_MEMLOCK` to 64 KiB. The ML-DSA key's 17 or 18 pages (68 KiB or more) are refused there. The refusal is counted in Settings, and the key works and is wiped as before (D607).
- **Server core dumps.** The server never calls `harden_process`, so its crash dumps can hold the log keys and the OPAQUE setup (D608).
- **Unchanged from doc 26 §7:**
  - copies made before a value became a secret type;
  - `Revealed`;
  - the JSON frame to the extension;
  - Argon2id's working memory (decision 0003).

## 7. How to run and test

```
export CARGO_PROFILE_DEV_DEBUG=line-tables-only
cargo test -p lockdown-crypto --lib locked
cargo test -p lockdown-crypto --lib pq_sign
cargo test -p lockdown-crypto --test one_page_table
cargo test -p lockdown-app --test memory_report
cargo test -p lockdown-server --lib keys
```

**Refusals for real (Linux).** Run the built test binaries with a zero lock limit. As root, also drop `CAP_IPC_LOCK`, which ignores the limit:

```
(ulimit -l 0; setpriv --inh-caps=-ipc_lock --bounding-set=-ipc_lock -- target/debug/deps/one_page_table-<hash>)
```

The `lockdown-crypto` unit tests, `one_page_table` and `memory_report` all pass this way, as well as normally.

**The tests:**

- `locked::pages_are_counted_once_and_released_with_the_last_secret`: two secrets on one page and a third across that page and the next. Both pages lie wholly inside one buffer, so nothing else shares them. On Linux the kernel's own answer is checked (the `lo` flag on the page's mapping in `/proc/self/smaps`). The page stays locked while the third secret lives, and both pages are released after it goes. This replaces the old test's look into Lockdown's private table, which no longer exists.
- `one_page_table::lockdown_and_the_identity_crate_share_one_page_table`, one test function so no other test thread moves the counts:
  1. **The case decision 0003 warns about.** A Lockdown lock and a crate lock on the same page: one lock for both. Dropping the crate's leaves the page locked, as the kernel reports it. Dropping Lockdown's releases it.
  2. **Real types, where the allocator puts them.** 64 `Key32` and 64 `AppDataKey` are made in turn. The test asserts that some share a page (Linux). Dropping every crate key leaves every live `Key32`'s page locked.
  3. **The ML-DSA key.** It holds `EXPANDED_KEY_LEN` over the page size, rounded up, or one page more (17 or 18 of 4 KiB, 5 or 6 of 16 KiB), the report rises by at least that much, and it signs and verifies. Every page-count assertion in this slice is computed from the real page size.
  4. **Refusals.** With the soft `RLIMIT_MEMLOCK` set to 0, `Key32`, `SecretBytes`, `AppDataKey` and `PqSigningKey` all still work, and a refused lock is counted. An unprivileged CI runner is refused here; a process with `CAP_IPC_LOCK` is not, and both paths are checked.

  At every step `locked::report()` and `memlock::report()` agree.
- `pq_sign::the_expanded_key_is_locked_out_of_swap_or_counted_as_refused`: the 64 KiB size, locked or counted as refused, and it signs the same either way.
- `memory_report::the_settings_report_counts_the_identity_crates_keys_too`: a crate key made by nothing of Lockdown's moves `App::memory_report`, and so does opening a vault. The page numbers are the crate's table.
- `keys::the_post_quantum_log_key_is_locked_out_of_swap_or_counted_as_refused` (server).

**Test counts:**

| Suite | Before | After |
|---|---|---|
| `lockdown-crypto` lib | 44 | 45 |
| `lockdown-crypto` `one_page_table` | new | 1 |
| `lockdown-app` `memory_report` | new | 1 |
| `lockdown-server` lib | 59 | 60 |

CI on Windows, macOS and Linux is the proof for the platform calls (`VirtualLock`, `mlock`). The crate's own CI runs its memlock tests on all three.

## 8. Unicron host-contract gaps

None hit. No UI changed, and the Settings sentence is the same.

## 9. Gaps

- The items in §6: Ed25519 keys, the OPAQUE setup, the `from_seed` stack temporary, 64 KiB lock limits, and server core dumps.
- **The shared-page test asks the kernel only on Linux.** On macOS and Windows it checks the shared table's counts, not the kernel's own per-page view.
- **The crate's code-signature verifier for Windows** (Authenticode, MSIX package family) is still declared and refuses every peer (D600). `lockdown-codesign`'s Windows module is where it would come from. Nothing in Lockdown needs it yet.

## 10. Proposed decisions

| # | Decision | Status |
|---|---|---|
| D600 | **The identity crate's Windows handshake is a named pipe** at `\\.\pipe\snds-identity-handshake-<16 hex of SHA-256(root)>` with a protected, user-only DACL, remote clients refused and a second server on the name refused. Its peer is checked by process identity: the same SID, an allowed image path (compared without case), and that image's SHA-256. That is D33 on Windows. The code-signature check (Authenticode, and the MSIX package family where packaged) stays declared in `CodeSignatureVerifier` and refuses every peer until it is built from `lockdown-codesign`'s Windows module. From the crate's decision 0003. Plain version: on Windows, the shared sign-in code only talks to the same person's installed ecosystem apps, checked the same way as on Linux and Mac. | Proposed; audit item |
| D601 | **D314 extends to the identity crate's own keys, on one page table per process.** The crate's `memlock` locks its key types. Lockdown's `locked::PageLock` is `memlock::PageLock`, so a page shared by secrets of both crates is counted once and unlocked only with the last one. Nothing in Lockdown calls `mlock` or `VirtualLock` itself. From the crate's decision 0003. Plain version: the shared sign-in code's keys stay out of swap too, and neither part of the app can undo the other's protection. | Proposed; audit item |
| D602 | **Lockdown's own Windows pipe stays as it is.** `crates/pipe` keeps its name (`snds-lockdown-browser-…`), its `PeerIdentity::Unknown` and its bound verifier. Its browser host already speaks it, and the crate's pipe is for the identity handshake between ecosystem apps. Not chosen: moving the browser channel onto `NamedPipeTransport`, which would change the host's endpoint for no gain in checks. Plain version: the browser extension's link to the app on Windows doesn't change. | Proposed |
| D603 | **The ML-DSA signing key is locked out of swap**, beside its box (`PageLock::of_value`), which covers the server's post-quantum log key too. It is wiped by `ml-dsa` before its pages are unlocked. Its unused seed copy is gone, and the seed is wiped once the key is expanded. Not chosen: `memlock::Locked<T>`, which moves the 64 KiB key across the stack once more. Closes doc 28 §9's secret-pages gap. Plain version: the post-quantum key, about 64 KB in memory, stays out of swap like the others. | Proposed; audit item |
| D604 | **Settings' memory line counts every locked page in the process**: Lockdown's secrets, the post-quantum key and the identity crate's keys. Its words and the IPC shape are unchanged. The number shown is larger while the post-quantum key is held: by about 17 pages on a 4 KiB-page computer, about 5 on Apple silicon. Plain version: the "pages of keys out of swap" count now includes all of Lockdown's keys, so it goes up. | Proposed; **flagged for Sean** (the number shown grows; no word change) |
| D605 | **Recorded gap: Ed25519 signing keys and the OPAQUE server setup aren't locked.** Lockdown's `SigningKey` holds `ed25519_dalek::SigningKey` inline: the device's key, and the server's token and log keys. Proposed fix: hold it as `memlock::Locked<ed25519_dalek::SigningKey>`, as the crate holds the ISK, and lock `OpaqueServer` beside a box. Not built here. Plain version: a few signing keys can still reach swap; a later change can close it the same way. | Proposed; recorded gap; audit item |
| D606 | **Recorded gap: the ML-DSA key's stack temporary isn't wiped.** `ExpandedSigningKey::from_seed` returns the key by value. Its 64 KiB stack copy is left behind when it moves into the box, and `ml-dsa` has no in-place constructor. This was already so before. Plain version: one short-lived copy of the post-quantum key isn't cleared; it lives only while the key is made. | Proposed; recorded gap |
| D607 | **A refused lock stays non-fatal, including for the ML-DSA key's 68 KiB or more** under a 64 KiB `RLIMIT_MEMLOCK` (kernels before Linux 5.16, some containers). The refusal is counted in Settings, and the key works and is wiped as before. Plain version: on a computer that won't lock that much memory, Lockdown still works and says how much it couldn't protect. | Proposed |
| D608 | **Proposed: the server turns its core dumps off too.** It holds the log keys and the OPAQUE setup for its whole life, but never calls `harden_process`, so a crash can write them to disk. Not built: an operator may rely on core dumps for a crash report, so it needs Sean's call (on by default, or a setting). Plain version: the self-hosted server could stop writing its keys into crash files. | Proposed; **flagged for Sean**; audit item |
| D609 | **Pin and supply chain only.** The identity crate is pinned at `35d2a56`. `windows-sys` 0.61.2 was already in the lock file and already exempt. `lockdown-crypto` drops its own `windows-sys` dependency. No `deny.toml` or `supply-chain` change, and no wire, IPC, wasm, view or extension schema change. Plain version: the app takes the shared code's new version with nothing new from outside. | Accepted by default under D38 |
