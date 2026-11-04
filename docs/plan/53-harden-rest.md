# 53: The rest of the keys out of swap, and the server's core dumps off

_Slice: `slice/harden-rest`, built 2026-11-04. Closes the two gaps doc 52 recorded: D605 (Ed25519 signing keys and the OPAQUE server setup weren't locked) and D608 (the server never turned its core dumps off). Builds on D314 (secret pages out of swap, core dumps off in the apps), D601 (one page table per process) and D603 (the ML-DSA key locked beside its box). No wire, IPC, wasm, view or extension schema changes. One new server flag. Proposed decisions D610–D619 (§9)._

## For future agent

- **TL;DR:** Every key Lockdown holds for a session or for the server's life now sits in a heap block whose pages are locked out of swap, on the one page table (`lockdown_crypto::locked::PageLock`, the identity crate's). That covers the Ed25519 `SigningKey`, the OPAQUE `ServerSetup`, and the device's own secrets in `DeviceKeys`. The server turns its core dumps off as the first thing `main` does, for every command. A refused call is logged once and the server starts anyway. `--core-dumps on` leaves them on, to debug a crash.
- **Read with:**
  - `crates/crypto/src/sign.rs`: `SigningKey` (`is_locked`, `locked_pages`).
  - `crates/crypto/src/auth.rs`: `OpaqueServer` (`is_locked`).
  - `crates/app/src/local_unlock.rs`: `Secrets::locked`, `DeviceKeys::is_locked`.
  - `bins/server/src/harden.rs`: `core_dumps`, `CoreDumps::line`, `swap_line`.
  - `bins/server/src/main.rs`: `main` (the call), `serve` (the swap line), `--core-dumps`.
  - `bins/server/tests/binary.rs`: the last two tests.
- **Do not:**
  - Call `mlock`, `munlock`, `VirtualLock` or `VirtualUnlock` anywhere in Lockdown (doc 52). Lock through `locked::PageLock`.
  - Lock a value that can move. Each lock here is on a `Box`'s block.
  - Drop the lock before the secret. The secret field comes first and the `PageLock` field after it, so the secret is wiped while its pages are still locked.
  - Use `PR_SET_DUMPABLE` in the server without reading D618.

## 1. Architecture

```text
                       one page table per process (ecosystem_identity::memlock)
                       ─────────────────────────────────────────────────────────
 lockdown-crypto       Key32 · SecretBytes · PqSigningKey (doc 52)
                       SigningKey (Ed25519)         ← new: Box + PageLock
                       OpaqueServer (ServerSetup)   ← new: Box + PageLock
 lockdown-app          DeviceKeys.secrets           ← new: Box + PageLock, locked before filled
 identity crate        its own key types (D601)

 lockdown-server main  ① harden::core_dumps(keep)   RLIMIT_CORE 0, before any key is read
                       ② ServerKeys::load_or_create  token key, OPAQUE setup, log keys (all locked)
                       ③ harden::swap_line(report)   one line if the system refused any lock
```

The pattern is the same everywhere, and it is the one doc 52 used for the ML-DSA key: the secret lives in one heap block that never moves, and an `Option<PageLock>` made by `PageLock::of_value` sits beside it. `None` means the system refused (counted in `locked::report()`) or the build can't lock (WASI). The secret works either way.

## 2. The Ed25519 signing key

**Before.** `SigningKey(ed25519_dalek::SigningKey)`, inline, wherever its owner put it.

**Now.** `SigningKey { key: Box<ed25519_dalek::SigningKey>, pages: Option<PageLock> }`. The key is 224 bytes in memory (the 32-byte secret and the public key with its expanded point), so the lock holds one page, or two when the block crosses a boundary.

- `from_bytes(seed)` builds the key, moves it into the box, wipes the seed it was given, then locks the block.
- `From<ed25519_dalek::SigningKey>` (the Identity Signing Key a record change rebuilds, D72) clones the key into the box and drops the one passed in. `ed25519-dalek` wipes on drop, so the caller's copy is wiped where it was passed. A plain move would leave it.
- `is_locked()` and `locked_pages()` are new. Nothing else in the API changed, so no call site changed.

**What this covers:**

- The server's token key (`TokenSigner`) and log key (`AppState::log_key`), which live as long as the server.
- The Identity Signing Key, for the length of one record change.
- Every test and tool key, at no cost that matters: one allocation and one lock per key.

## 3. The OPAQUE server setup

`OpaqueServer { setup: Box<ServerSetup<Suite>>, pages: Option<PageLock> }`. `ServerSetup` is a plain sized type: the OPRF seed, the server's key pair and a dummy public key, a few hundred bytes. So it boxes and locks with no fork of `opaque-ke`. `opaque-ke` already wipes it on drop: `OprfSeed` and `PrivateKey` are `ZeroizeOnDrop`. `generate` and `from_bytes` box it and lock the block. `is_locked()` is new.

## 4. The device's own secrets

The brief named "the device signing key". On a desktop or the web vault that key is not a long-lived `SigningKey`. `DeviceKeys` (`crates/app/src/local_unlock.rs`) holds three 32-byte secrets for the whole session: the unlock key, the Ed25519 seed and the KEM seed. It rebuilds the Ed25519 key from the seed for each signature. Those secrets were inline and unlocked. That was the real gap.

**Now.** `DeviceKeys { …, secrets: Box<Secrets>, …, pages: Option<PageLock> }`. `Secrets::locked(fill)` makes a zeroed block, locks it, and only then lets `fill` write into it. So no secret sits on an unlocked page in the block, and no whole `Secrets` value is built on the stack:

- `generate` writes the random bytes straight into the block (`rand::random_bytes`).
- `from_plain` copies from the decoded plain file, which is wiped when it drops.
- `from_sealed` copies from the decoded `Secrets`, which is wiped when it drops at the end of the call.

If `fill` fails, the block is dropped (and wiped) before its lock. `DeviceKeys::is_locked()` is new.

The vault's signer is this `DeviceKeys` (an `Rc` of it), so the device's signatures on heads and item versions come from locked secrets. The Ed25519 key a signature rebuilds lives on the stack for that one call. `ed25519-dalek` wipes it when it drops (§6).

## 5. The server's start

`lockdown_server::harden::core_dumps(keep)` is the first thing `main` does after parsing flags, for every command (`serve`, `backup …`, `security-keys`, `admin`, `healthcheck`). The backup commands hold the Backup Key and read the server key file too.

| OS | What it does | What it logs |
|---|---|---|
| Linux | `RLIMIT_CORE` 0, soft and hard, through `lockdown_crypto::locked::harden_process` (the apps' call, D314) | nothing; on a refusal, one line, and the server starts anyway |
| macOS | the same | the same |
| Windows | nothing: a process writes no core file. A crash goes to Windows Error Reporting, which writes a dump only where an administrator set up `LocalDumps`. | nothing |
| `--core-dumps on` (any OS) | nothing: the limit stays as the server found it | one line: "core dumps left on (--core-dumps on); a crash file can hold the server's keys" |

The hard limit goes to 0 too, so nothing in the process can raise it again. Lowering a limit is always allowed, so a refusal is rare. It is still handled: "couldn't turn core dumps off; a crash file could hold the server's keys (starting anyway)".

**The swap line.** Once the keys are open (`serve`, after `AppState::new`), `harden::swap_line` reads `locked::report()`. If the system refused any lock, the server logs one line: "this computer refused to keep the server's keys out of swap (N locks refused; raise the locked-memory limit, `ulimit -l`); they work, and may reach swap". The server's keys take about 21 pages of 4 KiB (the ML-DSA key's 17 or 18, plus one each for the others), about 84 KiB. So a 64 KiB `RLIMIT_MEMLOCK` (kernels before Linux 5.16, some container runtimes) refuses the ML-DSA key (D607). `docs/self-host.md` §7 says how to raise it.

**The key file.** `KeyFile` (the decoded `server-key.cbor`: the token seed, the OPAQUE setup bytes, both log seeds) is now `ZeroizeOnDrop`. The bytes read from disk and the bytes written back are `Zeroizing`. So the seeds' short-lived copies are wiped once the keys are made from them.

**Operators.** `docs/self-host.md` gains the `--core-dumps` row in "Every setting" and one paragraph in §7:

- the keys stay out of swap and crash files;
- what to do when the log says the system refused;
- how to get a core dump to debug, and that the dump is as secret as the key file;
- what Windows does.

## 6. Gaps

- **Stack temporaries made by value.** `ed25519_dalek::SigningKey::from_bytes`, `ServerSetup::new` and `ServerSetup::deserialize` return their value by value. It is built on the stack and moved into the box, and that stack copy isn't wiped. The same goes for the clone in `SigningKey::from`. Neither library has an in-place constructor, and this was already so before (D617). Each copy is a few hundred bytes and lives until that stack is reused. Doc 52 recorded the same for the ML-DSA key (D606).
- **The Ed25519 key a device signature rebuilds** (`DeviceKeys::signing_key`, and the identity crate's `FileKeystore`) sits on the stack for one call. It is wiped on drop, but its stack page isn't locked. Locking a stack page per signature would cost two system calls for a few microseconds of exposure.
- **Pending OPAQUE logins.** `LoginServerState`, the server's half of a login between its two round trips, is a plain `Vec<u8>` in `AppState::pending`. That is up to 10,000 of them, each for at most 120 s. It isn't locked or wiped. It is short-lived, but it is keyed material, so it is listed here. Not in this slice's brief.
- **A core-pattern handler that ignores the limit.** With `core_pattern` set to a pipe, the Linux kernel hands the dump to the handler whatever `RLIMIT_CORE` says. `systemd-coredump` and `apport` read the limit and store nothing when it is 0. A handler that ignores it would still get the dump. `PR_SET_DUMPABLE` would stop that too. It was not chosen (D618).
- **`compose.yaml` sets no `memlock` limit.** The container gets its runtime's default: 8 MiB on Linux 5.16 and later, enough; 64 KiB on older kernels, which refuses the ML-DSA key. The server logs that once. Raising the limit in `compose.yaml` was left out, because rootless runtimes can't raise it above their own and the container would fail to start (D615).
- **The swap line counts refused locks, not keys.** A key is one lock, but other secrets made while the keys load (the OPAQUE setup's bytes in a fresh key file, say) count too. So the number is right as "locks refused" and is what the line says.
- **Unchanged from doc 52 §6:** low lock limits (D607); copies made before a value became a secret type; `Revealed`; the JSON frame to the extension; Argon2id's working memory.

**Unicron host-contract gaps:** none hit. No UI changed. Settings' memory line counts more pages while a vault is open (the device's secrets: one page, often shared with other keys). Its words and the IPC shape are unchanged.

## 7. How to run and test

```
export CARGO_PROFILE_DEV_DEBUG=line-tables-only
cargo test -p lockdown-crypto --lib sign
cargo test -p lockdown-crypto --lib auth::tests::the_server_setup
cargo test -p lockdown-crypto --test one_page_table
cargo test -p lockdown-app --lib local_unlock
cargo test -p lockdown-server --lib harden
cargo test -p lockdown-server --lib keys
cargo test -p lockdown-server --test binary
```

**Refusals for real (Linux).** Run as root with `CAP_IPC_LOCK` dropped, or as any unprivileged user:

```
(ulimit -l 0; setpriv --inh-caps=-ipc_lock --bounding-set=-ipc_lock -- target/debug/deps/one_page_table-<hash>)
setpriv --inh-caps=-ipc_lock --bounding-set=-ipc_lock -- target/debug/deps/binary-<hash> a_zero
(ulimit -l 0; setpriv --inh-caps=-ipc_lock --bounding-set=-ipc_lock -- target/debug/lockdown-server --data /tmp/x)
```

The last one prints the swap line, then "listening on".

**The tests:**

- `sign::the_key_is_locked_out_of_swap_or_counted_as_refused`: a key from a seed and one taken from `ed25519-dalek` hold one or two pages, or the refusal is counted, and they sign the same bytes.
- `auth::the_server_setup_is_locked_out_of_swap_or_counted_as_refused`: generated and read back, locked or counted, the same bytes either way.
- `one_page_table` gains step 3b: the Ed25519 key is locked, the kernel's `lo` flag is on its page (Linux), and it is released with the key. Step 4, under a zero lock limit, adds the Ed25519 key and the OPAQUE setup: they work and are counted.
- `local_unlock::the_device_secrets_are_locked_out_of_swap_or_counted_as_refused`: fresh, plain-file and sealed `DeviceKeys` are locked or counted, and each signs as the device.
- `keys::the_token_and_log_keys_and_the_opaque_setup_are_locked_or_counted_as_refused` (server): generated, made on first run and read back.
- `harden::each_outcome_logs_at_most_one_line_and_only_when_it_matters` and `harden::turning_core_dumps_off_sets_the_core_limit_to_zero` (Linux: `/proc/self/limits` reads `0 0`).
- `binary::the_server_turns_its_core_dumps_off_at_start_unless_asked_not_to`: the real binary's `/proc/<pid>/limits` reads `0 0` (Linux), it serves a sign-in, and it logs nothing about core dumps. With `--core-dumps on` the limit is the test process's own, and it says so once.
- `binary::a_zero_locked_memory_limit_is_logged_once_and_the_server_still_serves` (Unix): started under `ulimit -l 0`. The swap line appears at most once, and exactly once when the test lacks `CAP_IPC_LOCK` (Linux). An account still registers and signs in, and core dumps are still off.

**Test counts:**

| Suite | Before | After |
|---|---|---|
| `lockdown-crypto` lib | 45 | 47 |
| `lockdown-crypto` `one_page_table` | 1 | 1 (two steps more) |
| `lockdown-app` lib | 124 | 125 |
| `lockdown-server` lib | 60 | 63 |
| `lockdown-server` `binary` | 4 | 6 |

## 8. Unicron host-contract gaps

None hit (§6).

## 9. Proposed decisions

| # | Decision | Status |
|---|---|---|
| D610 | **The Ed25519 signing key is locked out of swap**, beside its box (`PageLock::of_value`). That covers the server's token and log keys, and the Identity Signing Key a record change rebuilds. `ed25519-dalek` wipes it before its pages are unlocked. `from_bytes` wipes the seed it is given, and `From<ed25519_dalek::SigningKey>` wipes the key passed in. Closes half of D605. Plain version: the server's signing keys and the identity's signing key stay out of swap like the others. | Proposed; audit item |
| D611 | **The OPAQUE server setup is locked out of swap**, beside its box, with no fork of `opaque-ke`. `opaque-ke` wipes its seed and private key on drop. Closes the other half of D605. Plain version: the secret that lets the server check passwords without knowing them stays out of swap. | Proposed; audit item |
| D612 | **The device's own secrets are locked out of swap**: the unlock key, the Ed25519 seed and the KEM seed `DeviceKeys` holds while a vault is open. They sit in one block that is locked before anything is written into it, and they are filled in place or from decoded copies that are wiped. The Ed25519 key each signature rebuilds stays a wiped, unlocked stack value for that one call. Plain version: this computer's own keys stay out of swap while the vault is open. | Proposed; audit item |
| D613 | **The server turns its core dumps off at start**, for every command, before any key is read: `RLIMIT_CORE` 0, soft and hard, on Linux and macOS, through the apps' `harden_process`. On Windows nothing is called: there is no core file, and Windows Error Reporting writes a dump only where an administrator set up `LocalDumps`. A refusal is logged once and never stops the server. Closes D608, on by default. Plain version: if the self-hosted server crashes, the crash file won't hold its keys. | Proposed; **flagged for Sean** (D608 asked for his call: this makes it the default); audit item |
| D614 | **`--core-dumps on` (`LOCKDOWN_CORE_DUMPS`) leaves core dumps on**, to debug a crash, for `serve` only. The server says so once at start, and `docs/self-host.md` says the dump is as secret as the key file. Plain version: an operator who needs a crash dump can turn it back on, knowingly. | Proposed; **flagged for Sean** (a new operator setting) |
| D615 | **A refused lock on the server is logged once at start** ("…N locks refused; raise the locked-memory limit…"), after the keys load, and never stops it. `compose.yaml` is unchanged: a `memlock` limit there would stop the container starting on a rootless runtime. `docs/self-host.md` says how to raise it. Plain version: if the computer won't keep the server's keys out of swap, the server says so once and runs anyway. | Proposed |
| D616 | **The server key file is wiped once the keys are made from it.** The decoded `KeyFile` is `ZeroizeOnDrop`, and the bytes read and written are `Zeroizing`. Plain version: the server leaves no stray copy of its key file in memory after start. | Proposed |
| D617 | **Recorded gap: by-value stack temporaries.** `ed25519_dalek::SigningKey::from_bytes`, `ServerSetup::new` and `deserialize` return by value, so one short-lived stack copy of a few hundred bytes is left where each was built (like D606 for the ML-DSA key). The Ed25519 key a device signature rebuilds is wiped but sits on an unlocked stack page for one call. Plain version: a few short-lived copies of keys aren't cleared or locked; each lives only while a key is made or used. | Proposed; recorded gap |
| D618 | **Not chosen: `PR_SET_DUMPABLE` on the server.** D155's reason (the native host reads the app's `/proc/<pid>/exe`) doesn't bind the server. But `PR_SET_DUMPABLE` also hides `/proc/<pid>` from the operator's own tools and stops a debugger attaching. `RLIMIT_CORE` 0 is what `systemd-coredump` and `apport` honour. Recorded: a `core_pattern` pipe handler that ignores the limit still gets the dump. Plain version: the server keeps the same crash protection as the apps, and an operator can still inspect it. | Proposed; audit item |
| D619 | **Scope.** No wire, IPC, wasm, view or extension schema change. Settings' memory line counts the device's secrets too: one page more while a vault is open, often shared, with no word change. The server gains one flag (D614). No new dependency. Plain version: nothing the apps send or show changes, apart from the page count. | Proposed |
