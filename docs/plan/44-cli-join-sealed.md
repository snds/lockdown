# 44: Every computer the command line adds is sealed

_Slice: `slice/cli-join-sealed`, built 2026-11-01. Closes doc 43 §5 gap 1 and builds D514 (accepted by Sean on 2026-11-01: "a computer you add from the command line is protected by your master password like the first one"). Builds on D47 (no secret as an argument), D86 (the command line and the app are one device), D461 (the sealed device keys), D465 (the migration), D466 (the command line's master password), D467 (a forgotten password), D510–D515 (doc 43). Proposed decisions D520–D526 (§8)._

## For future agent

- **TL;DR:** `lockdown account login` and `lockdown recover` on an empty computer join the account through the app's own core, sealed in one step. The new device's keys stay in memory until the sign-in proves the password. They are sealed under that password before the identity names them, so a plain `device-key.cbor` is never written. The account's password is this computer's master password from then on, and every later command asks for it. The password comes from `--master-password-stdin`, `LOCKDOWN_MASTER_PASSWORD` or the echo-off prompt. `--password-stdin` and `LOCKDOWN_PASSWORD` keep their meaning: when given as well they must be the same password, and alone they never seal. With no terminal and no master password the command refuses and writes nothing. A wrong password writes nothing.
- **Read with:** `crates/app/src/join_sealed.rs` (the core's sealed join), `crates/app/src/session/second_factor.rs` (`sign_in_new_device`'s `seal` flag), `enrol_signed_in` and `recover_signed_in` in `crates/app/src/session.rs` (where the seal sits), `bins/cli/src/join.rs` (the password, the words, the vault pull), the `Account Login` and `Recover` arms in `bins/cli/src/main.rs`, `bins/cli/tests/cli_join_sealed.rs`; doc 43 §5, doc 39 §1.3 and §1.6.
- **Do not:**
  - Write device keys to disk before the sign-in has proved the password, and never as the plain file, on the sealed path. `join_keys` makes them in memory; `seal_for_join` is the only write.
  - Delete the sealed file after a refused kit or Secret Key. The server's log already recorded that device's sign-in. A retry with fresh keys shows up in the account's key transparency audit as "a device … logged in to your account that your identity doesn't list" (found while building this; §7).
  - Seal with `LOCKDOWN_PASSWORD` or `--password-stdin` alone (doc 43's "Do not" list; D466).
  - Print or log the password. It lives in `Zeroizing` strings until the core takes it.

## 1. What the two commands do now

| Case | What happens |
|---|---|
| Empty computer, the master password given (`--master-password-stdin` first line, or `LOCKDOWN_MASTER_PASSWORD`) | The core signs in with fresh keys held in memory. It pulls the identity's records, then seals the keys under the password: for `recover`, under the kit too (the kit slot). Then it opens the identity through the account slot (`account login`) or the kit (`recover`), which names this device. It publishes the records, keeps the session and records the lock-screen name. Prints the core's lines, then "this computer is locked with your account's password as its master password; commands here ask for it (--master-password-stdin, LOCKDOWN_MASTER_PASSWORD, or the prompt)". `account login` adds "run `lockdown sync` to pull the vault". `recover` pulls the vault right away with the same password (`vault …: pulled (n live items)`). |
| Empty computer at a terminal, nothing given | Asks once, with echo off: `Password for LOGIN at SERVER (this computer's master password from now on): `. There is no second ask, because the sign-in checks it. The rest of the run uses it without asking again. |
| `--password-stdin` or `LOCKDOWN_PASSWORD` given as well | Must be the same password. Otherwise: "Your account's password becomes this computer's master password, so they are one password. Give it once with --master-password-stdin or LOCKDOWN_MASTER_PASSWORD, or give the same one with --password-stdin or LOCKDOWN_PASSWORD." Exit 1, nothing written. |
| No terminal and no master password (including only `LOCKDOWN_PASSWORD` or only `--password-stdin`) | "Joining seals this computer with your account's password, which becomes its master password here. Give it with --master-password-stdin or LOCKDOWN_MASTER_PASSWORD, or run the command in a terminal to be asked." Exit 1. Nothing is written: the check runs before the core starts. |
| Wrong password | Refused at the sign-in ("… didn't go through"). Nothing is written: the keys were only in memory. |
| Wrong Secret Key, or wrong recovery code | Refused by the identity, with the words as before. No identity is made. The keys stay sealed under the (correct) password, so the next try is the same device (§7). Nothing is in the clear. |
| A join that stopped half way (a sealed key file, no identity) | The next try opens it with the password and carries on as the same device. Another password is refused: "This computer kept the keys of a join that didn't finish, sealed under another password. Join with the password you used then, or remove FILE to start afresh." |
| A half-done join from before this (a plain key file, no identity) | Opened, then sealed by this join. The plain file goes once the sealed one reads back (`DeviceKeys::seal`). |
| The account wants a security key or an emailed approval for a new device | "Your account asks for a security key or an emailed approval before a new device joins, and the command line can't wait for one yet. Add this computer from the Lockdown app instead. Nothing was kept here." Gap 2, §5. |
| A computer that already holds the identity | Unchanged: `account login` is a fresh session, and the vault opens as before (sealed: master password; plain: the notice). |

**The name** the lock screen shows (`onboarding.cbor`) is the account's login, cut to a valid name (64 characters, no control characters). This is what the desktop's join records through `setMasterPassword`. A login always exists on a join, so the `--name` and prompt rules of `init` are never needed here. A name already kept is left alone. `Master password for me@example.com:` is then the prompt on later commands.

**Existing installs.** A computer joined before this, with a plain key file, keeps opening. It says "This vault has no master password yet…" on every command, and `lockdown password set --password-stdin` seals it after checking the password at the account's server (the desktop's M3 migration, D465). This is unchanged and covered by a test.

## 2. The pieces

| Piece | Where | What it does |
|---|---|---|
| Sealed join, core | `crates/app/src/join_sealed.rs`: `App::account_login_sealed`, `App::recover_sealed` | Run on the core's worker: refuse a confined app, then `sign_in_new_device(…, seal = true)`. They wait for a security key the same way the unsealed join does (`start_waiting`, now `pub(crate)`), so a shell that can wait finishes through `App::second_factor_wait`. |
| The keys | `join_sealed::join_keys` | No key file: fresh keys, in memory. Sealed (half-done join): opened with the password. Plain (older half-done join): opened, to be sealed. |
| The seal | `join_sealed::seal_for_join`, called in `enrol_signed_in` and `recover_signed_in` right before `Identity::recover_with_account` and `Identity::recover_with_kit` | `DeviceKeys::seal` (the same routine the desktop's `setMasterPassword` reaches through `Session::seal_keys`): the password slot, plus the kit slot for a recovery, written through a temporary file, read back, and only then the plain file removed. |
| The finish | `second_factor::finish` | After a sealed join succeeds, `note_joined` records the name (the login) and the first run as done, as `set_master_password` does for an account on a server. |
| The wait | `WaitingSignIn::seal` | When a security key is needed, the password waits in memory (`Zeroizing`) next to the export key until the key answers. The keys are still not on disk, so a cancelled wait leaves nothing. |
| The command | `bins/cli/src/join.rs` | Works out the password (D466's routes; the account's routes must match), refuses before anything is written, runs the core's sealed join, maps a security-key wait and a missing Secret Key to command-line words, keeps the password for the rest of the run (`master::keep`) and, for `recover`, pulls the vault through the command line's own open. |
| Gone | `App::recover`, `App::enrol_from_account`, `binding_of` in `bins/cli/src/app.rs` | The command line's own copies of the joins, which wrote the plain key file. The core's versions also keep the Secret Key sealed on this device (D247), which the copies didn't. |

The desktop's join (`join.tsx`: `accountLogin` or `recover`, then `openVault(true)`, then `setMasterPassword`) is unchanged and still passes `seal = false` (gap 1, §5).

## 3. Who changed

| Caller | Change |
|---|---|
| `crates/app/src/session/second_factor.rs`, `session.rs`, `devices.rs`, `lib.rs` | The `seal` flag through the sign-in, the wait and the two finishes; `enrol_from_sign_in` (the web vault) and the desktop's `enrol_new_device` and `recover_new_device` pass none. |
| `bins/cli/src/main.rs` | The `Account Login` (empty root) and `Recover` arms call `join::join`; the account password there is optional (`password_if_given`). Help for `recover`, `account login` and their `--password-stdin`; the module doc. |
| `bins/cli/src/master.rs` | `keep`: the run's master password once a join has it. |
| `bins/cli/src/app.rs` | The joins removed; the file table names `device-key.sealed.cbor`. |
| `bins/cli/tests/sync_end_to_end.rs` | `LOCKDOWN_MASTER_PASSWORD` for every run (the laptop, an older install, ignores it). The wrong-password recovery uses `--master-password-stdin` and checks the whole phone folder stays empty. The wrong kit leaves only the sealed file. The recovered phone has only the sealed file. |
| `bins/cli/tests/tls_end_to_end.rs`, `kit_and_secret_key.rs` | `LOCKDOWN_MASTER_PASSWORD` for every run: the phone and the tablet join sealed. |
| `bins/cli/tests/cli_sealed.rs` | The phone's recovery gives the master password and checks the sealed file. |
| `bins/cli/tests/cli_join_sealed.rs` | New: the five tests of §4. |
| `bins/cli/tests/session_renewal.rs`, `security.rs` | Unchanged: no computer joins in them (their `account login` runs on a computer that holds the identity). |
| `bins/server/container/smoke.sh` | Unchanged flow (it already exported `LOCKDOWN_MASTER_PASSWORD` equal to the `--password-stdin` line). Now checks the second device holds only the sealed file. |
| `apps/web/e2e/*` | Unchanged: no spec recovers or logs in a second device through the command line (`pq-status.spec.ts` starts from `lockdown-legacy-install`). |
| `docs/plan/43-cli-sealed.md` | §5 gap 1 points here. |

## 4. How to run it and test it

- **By hand:** `LOCKDOWN_MASTER_PASSWORD=… LOCKDOWN_SECRET_KEY=A1-… lockdown account login --server https://host --login me`; `printf '%s\n%s\n%s\n' "$PW" "$SK" "$RC" | lockdown --master-password-stdin recover --server https://host --login me --kit-stdin`; or either one in a terminal, to be asked. Then `lockdown list` asks `Master password for me:`.
- **Tests:** `cargo test -p lockdown-cli --test cli_join_sealed` (5 tests), `cargo test -p lockdown-app --lib join_sealed` (4 tests):
  - refusal without a terminal: nothing given, `LOCKDOWN_PASSWORD` alone, `--password-stdin` alone (for `account login`), and the same for `recover`; an account password that differs from the master password; the temp folder empty after each, and the password never printed;
  - `account login` on an empty computer: a wrong password refused with the phone's folder empty; a wrong Secret Key refused, keys kept sealed and no identity; then the join, only `device-key.sealed.cbor`, the login as the lock-screen name, `sync` refused without the master password and fine with it (which also proves the retry kept the same device: key transparency passes), the item listed, and the desktop core opening it with `unlock_with_password` but not with `unlock()` or a wrong password;
  - `recover` the same from `--master-password-stdin`: a wrong password writes nothing, a wrong recovery code keeps the keys sealed, then the vault pulled, only the sealed file, its kit slot present (`local_unlock::profile_of`), `list` refused without the password, and the desktop core opening it;
  - a computer joined the old way (the core's unsealed join, a plain key file) opens with the notice, lists, and `password set --password-stdin` seals it;
  - at a pseudo-terminal: `recover` asks once, with the login and server named, opens the vault without asking again, never echoes the password, and `list` then asks `Master password for me@example.com:`.
- **Smoke:** `bins/server/container/smoke.sh` (Docker) checks the second device's key files.

## 5. Gaps

1. **The desktop's join is still two steps.** `join.tsx` signs in with the plain key file, opens, then `setMasterPassword`. A crash between those steps leaves a plain key file, which D465's M3 screen seals on the next open. Moving it to `account_login_sealed` and `recover_sealed` means new IPC commands, the client interface (the web vault compiles the desktop's views), mocks and harness fixtures, and replacing `openVault(true)` + `setMasterPassword` with `unlockWithPassword`. That isn't a small change, so it isn't done here (D526). The desktop's "let a device you already use approve this one" join (`devices_link.rs`, `finish_joined`) also starts from `open_or_create_plain`.
2. **No security key or emailed approval on the command line.** The core can wait for one, but the command can't hold the wait across runs. It refuses with words and keeps nothing (the keys were in memory). The old command-line join couldn't finish one either.
3. **`App::open_or_create(true)` stays (doc 43 §5 gap 2).** Production callers, which always open keys that already exist: the desktop's `openVault` IPC (`apps/desktop/src-tauri/src/commands.rs`, reached from `join.tsx` after its plain-key join) and the web vault's `opened` (`crates/web/src/host.rs`, after `enrol_from_sign_in`). The branch that makes a plain key file from nothing (`Session::open_with` with no keys in `Mode::Init`) is reached only by fixtures and tests: the extension's endpoint fixture (`apps/desktop/src-tauri/examples/browser-endpoint.rs`), the desktop's tests (`src-tauri/src/security.rs`, `codes.rs`, `browser/tests.rs`), the web crate's tests (`crates/web/tests/*.rs`) and the core's own tests (`crates/app/src/{lib,depth,browser}.rs`, `crates/app/tests/*.rs`). Fencing it behind a feature means a feature on `lockdown-app` turned on by four crates' dev-dependencies and the example. That is left for when the desktop's join moves (gap 1). Nothing user-facing reaches it without sealing after.
4. **The account's password is the master password, at the time of joining.** If the account's password is changed on another device, this computer's seal stays on the old one. Doc 39 §1.3 ("Password changed elsewhere") says a successful sign-in with the new password seals again; neither the core's sign-in nor the command line's `account login` does that yet. Until then the old password unlocks this computer, and `password reset --kit-stdin` works on a recovered one.
5. **Scripts that joined with `LOCKDOWN_PASSWORD` alone now stop** with the refusal above. That is deliberate (D466, D521): the account's variable never unlocks or seals by accident. Such a script adds `LOCKDOWN_MASTER_PASSWORD`, which it needs for every later command anyway.

## 6. unicron requests (host-contract gaps)

None. The slice is the core's join, the command line, its tests and the smoke script; no unicron surface or token is involved.

## 7. Audit notes

- **Seal before naming.** The seal (`DeviceKeys::seal`: Argon2id at or above the floor, XChaCha20-Poly1305, written through a temporary file, read back) runs after the OPAQUE sign-in proved the password and before `Identity::recover_with_*` writes the identity. No new crypto, no new file and no new wire shape.
- **Why refused joins keep their keys.** The server records each device's sign-in in its transparency log. If a refused kit or Secret Key threw the keys away, the retry would sign in as a new device, and the first one would stay in the log as a device the identity never lists. The next `sync` then stops with key transparency's stranger warning. The test for `account login` covers this: a wrong Secret Key, then the join, then a `sync` that passes. The kept keys are sealed under the password the server just accepted, so nothing is in the clear.
- **A wrong password writes nothing**, because the keys are in memory until after the sign-in. The tests list the phone's folder after each refusal.
- **The password's routes.** The master password is read only from the first stdin line, `LOCKDOWN_MASTER_PASSWORD` or the terminal with echo off. The account's `--password-stdin` and `LOCKDOWN_PASSWORD` are compared with it and never used alone. Neither reaches stdout or stderr: checked in the stdin, variable and terminal tests.
- **A recovered computer gets the kit slot**, so "Forgot your password?" (D467) works there. A computer that joined through `account login` has no kit at hand and gets none, as on the desktop.
- **The command line now keeps the Secret Key on a joined computer** (the core's `secret_key::keep`, sealed by the device key, D247), where its own copy of the join didn't.

## 8. Proposed decisions

For the coordinator to record. **Flagged** rows are user-visible and need Sean. "Audit item" rows go to the external audit under D38.

| ID | Decision | Owner |
|---|---|---|
| D520 | **A computer the command line adds is sealed in one step** (builds D514). `account login` and `recover` on an empty computer join through the core's `account_login_sealed` and `recover_sealed`: keys in memory until the sign-in proves the password, sealed under it (and the kit, for a recovery) before the identity names them, never a plain file. The command line's own copies of the joins are removed. Plain version: every computer you add from the command line asks for your master password, and never had its keys unprotected on disk. | Under D375; implements accepted D514; audit item |
| D521 | **The join's password comes from the master-password routes.** `--master-password-stdin`, `LOCKDOWN_MASTER_PASSWORD` or a single echo-off ask ("Password for LOGIN at SERVER (this computer's master password from now on):"). `--password-stdin` and `LOCKDOWN_PASSWORD` must match it when given and never seal alone. With neither and no terminal, the join refuses in words and writes nothing. Plain version: the command line asks for your password the same way everywhere, and an old script variable can't lock a computer by accident. | Under D375; **flagged for Sean** (the prompt and refusal words; scripts that joined with `LOCKDOWN_PASSWORD` alone now stop) |
| D522 | **A refused join keeps its sealed keys.** A wrong kit or Secret Key leaves no identity but keeps the keys sealed under the accepted password, so the retry is the same device and key transparency sees no stranger. A half-done join resumes with the same password; another password is refused, with words naming the file to remove. A wrong password writes nothing. Plain version: a typo in your kit doesn't make your own computer look like an intruder. | Under D38; audit item |
| D523 | **A recovered computer is sealed under the kit too** (the kit slot), so "Forgot your password?" works there; one joined by `account login` has no kit at hand and gets none, as on the desktop. Plain version: after recovering with your kit, the same kit can reset a forgotten password on that computer. | Under D38; audit item |
| D524 | **A joined computer's lock-screen name is the account's login**, cut to a valid name, as the desktop's join records it; a name already kept stays. Plain version: the command line asks "Master password for you@example.com". | Under D375; **flagged for Sean** (the name shown) |
| D525 | **The command line refuses a join that needs a security key or an emailed approval**, in words pointing to the app, and keeps nothing. Plain version: if your account wants a security key for new devices, add the computer from the app. | Under D375; **flagged for Sean** (the words) |
| D526 | **The desktop's join moves to the core's sealed join (proposed, not built).** `join.tsx` would call `account_login_sealed` or `recover_sealed` through new IPC commands and `unlockWithPassword` instead of `openVault(true)` and `setMasterPassword`; then the plain-from-nothing branch of `open_or_create(true)` can move behind a test feature (doc 43 gap 2). Plain version: the app also never writes a new computer's keys unprotected, even for a moment. | Under D38; audit item |
