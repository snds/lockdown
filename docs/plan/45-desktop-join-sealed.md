# 45: Every computer the desktop adds is sealed

_Slice: `slice/desktop-join-sealed`, built 2026-11-02. Builds D526 (proposed in doc 44, accepted): the desktop's joins move onto the core's sealed join. It closes doc 44 §5 gap 1 and doc 43 §5 gap 2. It builds on D62 (wire shapes only grow), D99 (approving a new device), D461 (the sealed device keys), D465 (the migration), D467 (a forgotten password), D474 (the first-run screens and their words), and D520–D525 (doc 44). Proposed decisions D530–D536 (§8)._

## For future agent

- **TL;DR:** The desktop adds a computer in three ways: sign in, recover with the kit, or let another device approve it. All three now seal the new device's keys under the password the person typed, in the same step. The keys stay in memory until the server has accepted that password. They are sealed before the identity names this device, so a plain `device-key.cbor` is never written. That holds even across a security-key wait or an emailed-approval wait. Screen J calls `accountLoginSealed` or `recoverSealed`, then `openJoined`. The old two-step path is gone from the join: `openVault(true)` followed by `setMasterPassword` a moment later. The "approve from another device" join seals inside `join_finish`. The core's `open_or_create(true)` now refuses a computer with no key file. Fixtures, the developer tools and the server's restore-drill account make their plain installs through `open_or_create_plain`, by name.
- **Read with:** `crates/app/src/join_sealed.rs` (`App::open_joined`, plus doc 44's sealed join), `crates/app/src/session/devices_link.rs` (`Joining::finish_sealed`, `sealed_link_keys`), `crates/app/src/devices.rs` (`join_start_sealed`, `join_finish`, `join_finish_signed_in`), `App::open_or_create` and `App::open_or_create_plain` in `crates/app/src/lib.rs`, `apps/desktop/src-tauri/src/first_run.rs` (the three new commands), `apps/desktop/src/features/unlock/join.tsx`, and the tests in §4. Also doc 44 §1–§2 and doc 39 §1.3 and §1.9.
- **Do not:**
  - Call `open_or_create_plain` from anything a person runs. It exists for fixtures, the developer tools and the restore-drill account (§5 lists them).
  - Restore the identity before the seal on a sealed device-link join. The identity would then name keys that live only in memory, and a closed window would leave "This device isn't enrolled yet".
  - Give the web vault's join the password seal. A tab keeps its key file in the worker's memory and seals it under the sign-in's export key (`crates/web/src/host.rs`). Its join stays plain by design (§3).
  - Drop the password from screen J's state before `openJoined` has run. The security-key wait needs it to open the vault afterwards.

## 1. What each desktop join does now

| Join | Before | Now | On screen |
|---|---|---|---|
| **Sign in** (screen J, "Sign in with your login and password") | `accountLogin` wrote the plain key file. Then `openVault(true)`, then `setMasterPassword({password})`, which checked the password at the server again and sealed. | `accountLoginSealed` keeps the keys in memory and signs in. It pulls the identity's records, seals the keys under the password, and only then enrols through the account slot. `openJoined(password)` then opens the vault and pulls it. | Nothing visible changed. The same fields, words, Continue button, server-key dialog and spinner. There was never a separate "setting your password" screen. The one second-step round trip (the server check inside `setMasterPassword`) is gone, so the spinner ends sooner. |
| **Recover with the kit** ("Use your recovery kit") | `recover`, then the same two steps. | `recoverSealed` seals under the password and the kit (the kit slot, so "Forgot your password?" works here later), then `openJoined`. | Nothing visible changed. |
| **Either one, when the account asks for a security key or an emailed approval** | The wait finished the plain join. Then `openVault(true)` and `setMasterPassword`. | The core's wait keeps the password in memory (`WaitingSignIn::seal`, doc 44) next to the export key. When the key or the email answers, `second_factor_wait` seals and enrols. Then `openJoined(password)`. Cancelling the wait leaves nothing on disk. | Nothing visible changed. "Your account asks for a security key on a new device…" and the emailed-link sentence are unchanged. |
| **Approve from another device** (the device link) | `join_start` wrote the plain key file. When the approval came in, `join_poll` restored the identity, and `join_finish` signed in and opened with `Mode::Init`. A closed window anywhere after the approval left a plain key file and an identity. | The desktop's `join_start` command calls `join_start_sealed`. Fresh keys stay in memory. The approved hand-over (root key, records, Secret Key) waits beside them. `join_finish(login, password)` signs in, checks that the account is the approving device's identity, seals the keys under the password, restores the identity, brings the records in step, keeps the session and the Secret Key, records the login as the lock-screen name, and opens through `open_joined`. | Nothing visible changed. The approved screen already asked for "Login name" and "Master password". Its words already said "Sign in with your master password to finish". No new password moment was needed (task row 3). |
| **A join that stopped part way** (a sealed key file but no identity: a refused Secret Key or kit, D522) | The lock screen said "This device isn't enrolled yet…". That sentence is about another app's identity, so it was a dead end. | The lock screen goes back to screen J (`FirstRun start="join"`). Joining again with the same password carries on as the same device (`join_sealed::join_keys`). | **Changed:** screen J instead of the "isn't enrolled" sentence, for this one case. Flagged as D533. |

**What a wrong password does:** the sign-in refuses it, and the computer's folders stay empty, because the keys were only in memory. On the device-link join the approved join stays in memory, so the person can type the password again as the same device. **A cancelled wait or a cancelled device-link join** writes nothing. **A refused Secret Key or recovery code** after a sign-in that went through keeps the keys sealed under the accepted password (D522). The retry is the same device, so key transparency sees no stranger.

**Existing installs.** A computer joined before this, with a plain key file, still opens with the device keystore. The M3 migration (`setMasterPassword`, D465) then seals it with the account's password. `setMasterPassword` stays for exactly that, and a test covers it.

## 2. The pieces

| Piece | Where | What it does |
|---|---|---|
| Open after a sealed join | `App::open_joined`, `Core::open_joined` (`join_sealed.rs`) | Refuses unless the key file is sealed and the identity is here, so it never makes anything from nothing. It opens the keys with the password and opens the session in `Mode::Init`, which pulls the persona's vaults the way the first open after a join always did (or makes the first vault for an account with none). Then it adopts the session the way an unlock does. |
| Sealed device-link join | `Joining::start(…, sealed)`, `Joining::poll`, `Joining::finish_sealed` (`session/devices_link.rs`) | Sealed: fresh keys in memory (`sealed_link_keys`). A plain file from an older half-done join is opened and sealed at the finish. A sealed file with no identity is refused with words naming the file, because it can't be opened before the password is asked. `poll` keeps the `HandedOver` instead of restoring. `finish_sealed` signs in with OPAQUE, refuses a server that still wants a second factor, checks the profile, seals (`seal_for_join`), restores (`restore_from_root_key`), then runs the shared tail `finish_signed_in` and `note_joined`. |
| Plain device-link join | `Joining::start(…, false)`, `finish_joined` | The web vault's path, unchanged in behaviour. `finish_joined` now takes the tab's sign-in (export key and session) directly. Its `SignIn::Password` arm, used only by the desktop, is gone. |
| The core's calls | `App::join_start_sealed`, `App::join_finish`, `App::join_finish_signed_in` (`devices.rs`) | `join_start` stays plain (the web vault). `join_start_sealed` is the desktop's and refuses in a confined home, as `account_login_sealed` does. `join_finish` finishes only a sealed join and opens through `open_joined`. `join_finish_signed_in` finishes only a plain one. |
| No plain create from nothing | `App::open_or_create`, `App::open_or_create_plain` (`lib.rs`) | `open_or_create(true)` with no key file answers `not_set_up` ("Nothing is set up on this computer yet.") and writes nothing. `open_or_create_plain` is the old behaviour under a name that says what it makes. |
| Desktop commands | `apps/desktop/src-tauri/src/first_run.rs`, appended to the end of the handler list in `commands.rs` | `account_login_sealed(server, login, password, secretKey?) → string[]`, `recover_sealed(server, login, password, secretKey, recoveryCode) → string[]`, `open_joined(password) → Status`. Every password and kit field is held in `Zeroizing`. The existing `join_start` command now calls `join_start_sealed`. Its shape is unchanged. |
| Client | `FirstRunClient.accountLoginSealed`, `recoverSealed`, `openJoined` (`first-run-client.ts`, `app/tauri-first-run.ts`) | These sit on the first-run client because screen J is part of the first run. The web vault provides no first-run client (D176), so it compiles these views unchanged. |
| Screen J | `features/unlock/join.tsx` | `connect` calls the sealed join, then `openJoined`. The wait's `onDone` calls `openJoined`. The password fields are cleared once the vault is open. It no longer uses the core client at all. |
| Lock screen | `features/unlock/unlock-screen.tsx` | Keys with no identity lead back to screen J (§1, D533). |
| Capture harness | `design/capture/src/fixture/commands.ts` | Answers `account_login_sealed`, `recover_sealed` and `open_joined`. |

## 3. Who changed

| Caller | Change |
|---|---|
| `apps/desktop/src/features/unlock/join.tsx` | Calls `accountLoginSealed` or `recoverSealed`, then `openJoined`. No longer calls `accountLogin` or `recover`, `openVault(true)` or `setMasterPassword`. |
| `apps/desktop/src/features/unlock/unlock-screen.tsx` | The half-done join's route (D533). |
| `apps/desktop/src-tauri/src/security_depth.rs` | `join_start` calls `join_start_sealed`. `join_finish` is unchanged and now seals. |
| `apps/desktop/src-tauri/src/commands.rs` | Three commands appended. `open_vault` is documented as never making keys from nothing. |
| `apps/desktop/src-tauri/examples/browser-endpoint.rs`, `src/security.rs`, `src/codes.rs`, `src/browser/tests.rs` | `open_or_create_plain` (throwaway vaults for the fixture and the tests). |
| `crates/web/src/host.rs` | Unchanged. `opened` still calls `open_or_create(true)` after the tab's sign-in, when its key file exists. |
| `crates/web/tests/*.rs` | The desktop that makes each account uses `open_or_create_plain`. New: `desktop_join_sealed.rs`. |
| `crates/app/tests/*.rs`, `crates/app/src/{lib,depth,browser}.rs` | `open_or_create_plain` for the first device in each test. A device that joined the old way (the unsealed `account_login` or `recover`) still opens with `open_or_create(true)`. `device_approval.rs` and `session_renewal.rs` join sealed. New: `desktop_join_sealed.rs`. |
| `crates/devtools/src/{legacy,demo/mod}.rs`, `crates/devtools/tests/*.rs` | `open_or_create_plain`: the legacy-install tool and the demo vault are passwordless on purpose. |
| `bins/server/src/drill_account.rs`, `bins/server/tests/web_server_finish.rs` | `open_or_create_plain`: the restore-drill account lives in the server's own private folder, holds one marker item, and is never a person's vault. |
| `bins/cli/tests/shared_vault.rs` | `open_or_create_plain` for the desktop side of the test. |
| `docs/plan/43-cli-sealed.md`, `44-cli-join-sealed.md` | §5 gap 2 and gap 1 point here. |

**The web vault.** Screen J's sign-in and recovery joins are desktop only: the web vault provides no first-run client, and its own sign-in (`WebVault::sign_in`) already never writes a key in the clear outside the worker's memory. The web vault does share the core's device-link join (`Joining`), in the plain mode. A tab's key file lives in the worker's in-memory file system and is sealed under the sign-in's export key between unlocks. Sealing it under a password would break that model, so it stays as it is (`App::join_start`, `App::join_finish_signed_in`). Its UI tests and the e2e suite pass unchanged.

## 4. How to run it and test it

- **By hand:** open the desktop on empty folders (`LOCKDOWN_ROOT=/tmp/x/identity LOCKDOWN_DATA=/tmp/x/data pnpm tauri dev` in `apps/desktop`), choose "I already use Lockdown", and sign in, recover, or join with a code from another device. Afterwards the data folder holds `device-key.sealed.cbor` and no `device-key.cbor`, and the lock screen asks for the master password.
- **Core tests:**
  - `cargo test -p lockdown-app --test desktop_join_sealed` (3 tests):
    - sign-in: a wrong password writes nothing; a wrong Secret Key keeps only the sealed file; `open_joined` is refused until the join finishes; the retry finishes; the sealed file only, with the name and the password needed again; key transparency passes.
    - recovery: the same, plus the kit slot.
    - an old plain join: it opens, `open_joined` refuses it, and `set_master_password` seals it.
  - `cargo test -p lockdown-app --test device_approval` (4 tests):
    - nothing on the phone after the approval;
    - a wrong password writes nothing and the join stays;
    - the sealed file only;
    - the device keystore alone and a wrong password are refused after a lock;
    - a cancelled join after the approval writes nothing, and `open_or_create(true)` won't create there;
    - plus the declined, forged-QR and opt-out tests, now sealed.
  - `cargo test -p lockdown-app --test session_renewal` (the revoked machine joins again, sealed).
  - `cargo test -p lockdown-app --lib probe_then_create_then_open_again` (`open_or_create(true)` refuses nothing).
- **The security-key wait:** `cargo test -p lockdown-web --test desktop_join_sealed`. A real key is added and used on the server's pages. Nothing is written while the phone waits, an abandoned wait leaves an empty folder, and the answered wait leaves only the sealed file, which `open_joined` opens with the password.
- **UI:** `pnpm test` in `apps/desktop`. `depth.test.tsx` covers:
  - the sign-in and the recovery call the sealed join and `openJoined` with the typed password, never `accountLogin`, `openVault` or `setMasterPassword`;
  - a refusal is shown in the core's words;
  - the security-key and emailed-approval waits open with the same password;
  - a cancelled wait opens nothing;
  - a half-done join comes back to screen J.
- **Web vault and extension:** `apps/web` `pnpm test` and `pnpm e2e` (private binaries and port), and `extension` `pnpm test` with `LOCKDOWN_TEST_BINS` (the endpoint fixture now uses `open_or_create_plain`).
- **Visuals:** no screen's layout or words changed (the one routing change shows screen J, which is already captured), so the capture harness wasn't re-run. Its fixture answers the new commands.

## 5. Gaps

1. **The old IPC commands stay.** `account_login` and `recover` keep their shape (D62), and the account dialog uses `account_login` for a fresh session on an enrolled computer. On an empty computer they still run the core's unsealed join (`enrol_new_device` and `recover_new_device` with `seal = false`). The desktop UI no longer reaches that, and the core tests use it to make "an install joined the old way". A later slice could make the empty-computer branch refuse and point to `account_login_sealed`.
2. **A device-link join left half way with a sealed file can't resume by the device link.** The crash window is between the seal and the identity restore. The join can't open the keys before it asks for the password, so `join_start` refuses in words that name the file to remove. A sign-in join with the same password opens that file (doc 44's `join_keys`). Whether enrolment through the account slot then accepts a device the approver already listed isn't tested.
3. **A window closed after the approval but before the finish** leaves the device in the identity (the approver wrote it) with no keys anywhere. It never signs in and can be revoked from Devices. Before this slice the same moment left a plain key file and an identity on disk.
4. **A restore that fails after the seal** (an approver whose hand-over doesn't check out) keeps the sealed file and no identity (D522's reasoning). Gap 2 then applies.
5. **`open_or_create_plain` has no compile-time fence.** It is a clearly named public function, not a feature, because a production binary uses it (the server's restore-drill account). A cargo feature on `lockdown-app` would be unified into the desktop whenever a workspace build includes the server. Its callers are listed in §3, and nothing a person runs reaches it.
6. **Password changed elsewhere** (doc 44 gap 4) applies to every desktop join too: the seal stays on the password used at the time of joining.

## 6. unicron requests (host-contract gaps)

None. No screen or component changed, no token was added, and the views still reach the host only through the client interfaces.

## 7. Audit notes

- **Seal before naming, on all three joins.** The sign-in and recovery joins use doc 44's `seal_for_join`, unchanged. The device link now seals at the same point: after OPAQUE proved the password and the profile matched the approver's identity, and before `restore_from_root_key` writes the identity and this device's wrap of the root key. No new crypto, no new file and no wire shape changed.
- **What waits in memory on a device-link join:** the device's keys (`DeviceKeys`) and the `HandedOver` (root key, records, Secret Key). They live only between the approval and the finish, in the core's `Depth` (dropped by `join_cancel`, an error from `join_poll`, or the app stopping). Both copies of the Secret Key (the join's and the hand-over's) are zeroized on drop (`Joining::drop`); the root key is the identity crate's key type.
- **The password's route.** Typed on screen J or on the approved screen, it crosses the IPC once per call (`account_login_sealed`, `recover_sealed`, `open_joined`, `join_finish`), is held in `Zeroizing` on the Rust side, and is never logged. The security-key wait holds it in the React state of screen J, as it did before for `setMasterPassword`, and clears it once the vault is open.
- **No new release of a secret.** These calls make or open the person's own vault. They reveal, copy and fill nothing, so no `Reveal`, `Use` or `Approve` gate applies and nothing counts as a use (D151). Approving the device stays behind the existing device's `Approve` gate (D99).
- **Nothing made from nothing.** `open_or_create(true)` and `open_joined` both refuse a computer with no keys, and `open_joined` also refuses one with no identity. `Mode::Init` can therefore no longer create a fresh plain identity next to a sealed key file.

## 8. Proposed decisions

For the coordinator to record. **Flagged** rows are user-visible and need Sean. "Audit item" rows go to the external audit under D38.

| ID | Decision | Owner |
|---|---|---|
| D530 | **The desktop's sign-in and recovery joins are sealed in one step** (builds D526). Screen J calls `account_login_sealed` or `recover_sealed`, then `open_joined` with the same password. The new device's keys are sealed under it (and the kit, for a recovery) before the identity names them. `openVault(true)` and `setMasterPassword` leave the join. Plain version: a computer you add in the app is protected by your master password from its first moment. | Under D38; implements accepted D526; audit item |
| D531 | **The security-key and emailed-approval waits carry the seal.** The password waits in memory beside the export key until the key or the email answers, and a cancelled wait leaves nothing on disk. Plain version: waiting for your security key never leaves the new computer half protected. | Under D38; audit item |
| D532 | **The "approve from another device" join seals before it restores.** On the desktop the new device's keys and the approved hand-over stay in memory until "Finish". The finish signs in with the master password, checks the account is the approver's, seals the keys under that password, and only then restores the identity. The web vault's tab keeps its own export-key seal and the plain mode. Plain version: approving a new computer from your phone protects it with your master password, with no extra screen. | Under D38; audit item |
| D533 | **A join that stopped with its keys sealed comes back to screen J**, not "This device isn't enrolled yet". Joining again with the same password carries on as the same device. Plain version: if you typed your Secret Key wrong and closed the app, it picks up where you left off. | Under D375; **flagged for Sean** (a screen shown in a new case) |
| D534 | **The core never makes device keys from nothing on an open.** `open_or_create(true)` refuses a computer with no key file (`not_set_up`). Passwordless installs are made only by `open_or_create_plain`, whose callers are test fixtures, the developer tools and the server's restore-drill account. Plain version: there is no path left in the app that makes an unprotected vault. | Under D38; closes doc 43 gap 2; audit item |
| D535 | **Additive IPC** (D62): the desktop commands `account_login_sealed`, `recover_sealed` and `open_joined`, and the first-run client's `accountLoginSealed`, `recoverSealed` and `openJoined`. `join_start` keeps its shape and now seals. `account_login`, `recover` and `open_vault` keep theirs. No wasm or extension wire shape changed. Plain version: older and newer parts keep understanding each other. | Accepted by default under D38 |
| D536 | **The old unsealed joins stay in the core for now.** `account_login` and `recover` on an empty computer still write the plain key file. Nothing in the desktop UI calls them there any more, and they make the "joined the old way" installs the migration tests need. A later slice may refuse them there (§5 gap 1). Plain version: nothing you can click makes an unprotected computer, and old installs keep opening. | Under D38; audit item |
