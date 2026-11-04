# 56: No shipped path makes an unprotected computer

_Slice: `slice/no-plain-joins`, built 2026-11-04. Sean's go on D536 (2026-11-04). Closes doc 45 §5 gap 1: the core's old `App::account_login` and `App::recover` on an empty computer still ran the unsealed join and wrote a plain key file, though nothing in any UI reached it. Builds on D62 (wire shapes only grow), D461 (the sealed device keys), D465 (the migration), D514 and D520–D526 (doc 44, the command line's sealed join), D530–D536 (doc 45, the desktop's sealed join), and D534 (`open_or_create_plain` for fixtures). Proposed decisions D640–D645 (§9)._

## For future agent

- **TL;DR:** `App::account_login` and `App::recover` never join any more. On a computer that hasn't joined an account (no identity here), both refuse with `not_set_up` and words that name the sealed join (`JOIN_SEALED_LOGIN`, `JOIN_SEALED_RECOVER`), and write nothing, even with the right password and kit. On a computer that holds the identity, `account_login` is the fresh session it always was (the account dialog's sign-in), and `recover` refuses as it always did. The old unsealed join lives on only as the test fixtures `App::account_login_plain` and `App::recover_plain` (`crates/app/src/join_plain.rs`), compiled only with lockdown-app's own unit tests or its `test-support` feature, which only `[dev-dependencies]` turn on. The desktop's IPC commands `account_login` and `recover` keep their shape and now carry the refusal on an empty computer.
- **Read with:** `App::account_login`, `App::recover`, `JOIN_SEALED_LOGIN`, `JOIN_SEALED_RECOVER` and `AppError::join_unsealed` in `crates/app/src/lib.rs`; `crates/app/src/join_plain.rs`; `crates/app/src/join_sealed.rs` (the sealed joins, doc 44); `crates/app/tests/no_plain_joins.rs`; the `desktop-bare` job in `.github/workflows/ci.yml`; doc 45 §1–§5.
- **Do not:**
  - Turn on `test-support` outside `[dev-dependencies]`, or from another feature. A normal dependency would put the plain join into every binary built beside it. The `no_plain_joins` test and the CI step both fail if you do.
  - Make `account_login` or `recover` join again, behind a flag or a variable. A computer joins sealed: `account_login_sealed` or `recover_sealed`, then `open_joined`; or `join_start_sealed` and `join_finish` for the device link.
  - Remove `account_login` or `recover` from the desktop's IPC. Their shapes stay under D62. The account dialog uses `account_login` on an enrolled computer.
  - Give the web vault's device-link join the password seal (doc 45 §3; §5 below).

## 1. What each call does now

| Call | A computer that hasn't joined (no identity) | A computer that holds the identity |
|---|---|---|
| `App::account_login` | **Refused**, `not_set_up`: "This computer hasn't joined your account yet, and a join now always protects it with your master password. Sign in with the sealed join (account_login_sealed, then open_joined)." Nothing is written, and the server isn't contacted. | Unchanged: a fresh session, the reminder noted, the account's stretching raised when this device is stronger. |
| `App::recover` | **Refused**, `not_set_up`: "…Recover with the sealed join (recover_sealed, then open_joined)." Nothing is written. | Unchanged: refused with the words it always had ("an ecosystem identity already exists at … but this device isn't enrolled in it…"), since there is nothing to recover onto. |
| `App::account_login_sealed`, `App::recover_sealed` | The join, sealed (doc 44). | Refused as before (the root isn't empty). |
| `App::account_login_plain`, `App::recover_plain` | **Test builds only.** The old unsealed join: the plain key file, then the enrolment or the recovery. | Refused as before. |

"Hasn't joined" is the same test the old code used to pick the join: the identity store at the root is missing. It covers a computer with nothing on it, and one an old join left half done (a plain key file and no identity). The refusal leaves that file exactly as it was; the sealed join then opens it, seals it under the password and carries on as the same device (doc 44's `join_keys`), which the tests check.

**Why `not_set_up`.** It is an error code the desktop (`core/types.ts`) and every shell already know, and it says the true thing: nothing is set up here for this call. No new code and no `detail` value were needed (D62). The words are aimed at whoever sends the call by hand, a script or a developer, because no screen sends it: they name the sealed calls by their command names.

## 2. The pieces

| Piece | Where | What it does |
|---|---|---|
| The refusals | `App::account_login`, `App::recover`, `AppError::join_unsealed` (`crates/app/src/lib.rs`) | `account_login` returns the refusal where it used to call `enrol_new_device`. `recover` runs `check_root_empty` inside the worker (the old words on an enrolled computer), then refuses. Its arguments stay for the shape and are unused. |
| The words | `lockdown_app::JOIN_SEALED_LOGIN`, `JOIN_SEALED_RECOVER` | Public constants, so a test or a shell can match them exactly. |
| The fixtures | `crates/app/src/join_plain.rs`: `App::account_login_plain`, `App::recover_plain` | The old `enrol_new_device` and `recover_new_device`, moved out of `devices.rs` under names that say what they make. The module is declared `#[cfg(any(test, feature = "test-support"))]`. |
| The feature | `crates/app/Cargo.toml` `[features] test-support = []` | Off by default, named by no other feature. Turned on by lockdown-app's own `[dev-dependencies]` (its integration tests) and `bins/cli`'s (the old-install test). Under the workspace's resolver, a dev-dependency's features reach only builds that build tests or examples, never `cargo build -p lockdown-desktop`, a Tauri build or a release. |
| `check_root_empty` | `crates/app/src/session.rs` | Now `pub(crate)`, so `recover` keeps the enrolled computer's old words. |
| Desktop IPC | `account_login`, `recover` in `apps/desktop/src-tauri/src/commands.rs` | Unchanged in shape and code; their doc comments say what an empty computer gets now. |
| The release check | `desktop-bare` job, step "No shipped binary has the core's old unsealed join" | `cargo tree -e features` for the desktop (`--features shell`), the server and the CLI, without dev-dependencies: none may show `lockdown-app feature "test-support"`. A dry run through the CLI's dev-dependencies must show it, proving the check looks. |

## 3. Who changed

| Caller | Before | Now |
|---|---|---|
| `crates/app/tests/sync_transparency.rs`, `pq_signatures.rs`, `live_sync.rs`, `vault_depth.rs`, `session_renewal.rs` (`two_devices`), `organization.rs`, `security_center.rs`, `passkeys.rs` | The second device joined with `account_login`, then `open_or_create(true)`. | **The sealed join:** `account_login_sealed`, then `open_joined(PASSWORD)`. None of them needed an old install; they needed a second device. |
| `crates/app/tests/recovery_depth.rs` (the fresh device that recovers under raised stretching; the phone that enrols under the raised set) | `recover` / `account_login`, then `open_or_create(true)`. | **The sealed join:** `recover_sealed` / `account_login_sealed`, then `open_joined`. |
| `crates/app/tests/local_account.rs` (a second computer recovers the moved vault) | `recover`, then `open_or_create(true)`. | **The sealed join:** `recover_sealed`, then `open_joined`. |
| `crates/app/tests/sharing_pause.rs` (`alice2`) | `account_login`. | **The sealed join.** |
| `crates/app/tests/shared_vault_quiet.rs` (the tablet whose own head is bent) | `account_login`. | **The sealed join.** |
| `crates/app/tests/sharing_pause.rs` (`alices_new_device`) | `account_login`. | **The fixture**, `account_login_plain`: the test locks and reopens Alice's laptop (a plain install) and the tablet in one loop through the device keystore. |
| `crates/app/tests/shared_vault_quiet.rs` (the tablet that has met nobody yet) | `account_login`. | **The fixture**: the test locks and reopens it through the device keystore. |
| `crates/app/tests/desktop_join_sealed.rs` (an install joined the old way) | `account_login`. | **The fixture**: the test is about an old install. |
| `bins/cli/tests/cli_join_sealed.rs` (a computer joined before this) | `account_login`. | **The fixture**: the test is about an old install. `bins/cli/Cargo.toml` turns on `test-support` in its dev-dependencies. |
| `crates/app/tests/session_renewal.rs`, `recovery_depth.rs` (signing in again on a device that holds the identity), `crates/web/tests/security_depth.rs` | `account_login` on an enrolled computer. | Unchanged. |
| `bins/server/src/drill_account.rs` | `read_back` signs in with `account_login` on a copy of the drill device, which already holds the identity; `ensure` makes the account with `open_or_create_plain` and `account_register`. | **Unchanged.** `read_back` is the enrolled branch (a fresh session), so it never reached the join. `ensure` makes a local install and registers it; it never joined. No sealed join fits: the drill opens its copy with the device keystore and has no master password to keep. |
| `crates/devtools` | No caller of `account_login` or `recover`. | Unchanged. |
| `crates/app/src/devices.rs` | Held `enrol_new_device` and `recover_new_device`. | They moved to `join_plain.rs` as the fixtures. |
| `apps/desktop/src-tauri/src/commands.rs` | | Doc comments only. |

**The desktop UI** never sends `account_login` on an empty computer: screen J joins through `accountLoginSealed` and `recoverSealed` (doc 45), and the account dialog (`features/account/account-dialog.tsx`) runs inside an open vault. `client.recover` has no caller in the UI. **The extension** calls neither (it talks to the browser host, never to these). **The web vault** answers `accountLogin` with "not here" (`apps/web/src/core/web-client.ts`) and has no `recover`; its own sign-in uses `App::account_login_signed_in`, which refuses unless the worker already made its key file, so it doesn't make keys from nothing.

## 4. How to run it and test it

```sh
export CARGO_TARGET_DIR=… CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=line-tables-only
cargo test -p lockdown-app --test no_plain_joins     # the refusals, the fixture, the manifests
cargo test -p lockdown-app                           # every moved caller
cargo test -p lockdown-cli --test cli_join_sealed    # the old install through the fixture
cargo tree -p lockdown-desktop --features shell -e normal,build,features --prefix none | grep 'lockdown-app feature'
```

| Test | What it proves |
|---|---|
| `no_plain_joins::the_old_joins_refuse_on_an_empty_computer_and_write_nothing` | With the right password, Secret Key and recovery code for an account that exists, `account_login` (with and without the Secret Key) and `recover` answer `not_set_up` in the exact words, with no `detail`; the computer's folder holds no file afterwards; the sealed join the words name then works on the same computer and leaves only the sealed key file. |
| `…::a_half_done_plain_join_is_refused_and_the_sealed_join_seals_it` | A plain key file with no identity: both calls refuse and leave the file byte for byte; `recover_sealed` then seals those same keys (the same device key) and removes the plain file. |
| `…::on_a_computer_that_holds_the_identity_nothing_changed` | `account_login` on a plain install and on a sealed one is a fresh session, and sync works; `recover` there refuses with the old words. |
| `…::the_fixture_makes_the_old_plain_join_by_name` | `account_login_plain` and `recover_plain` make the plain key file, and the install opens with `open_or_create(true)`. |
| `…::only_dev_dependencies_turn_on_the_fixture` | Reads every workspace member's manifest: `test-support` is off by default and named by no other feature, and the only manifests that turn it on are `crates/app` and `bins/cli`, both under `[dev-dependencies]`. |
| `desktop-bare` CI step | The desktop with only the shell feature, the server and the CLI resolve lockdown-app without `test-support`, so `account_login_plain` and `recover_plain` aren't compiled into them; the dry run through dev-dependencies sees it. |

Together these are the proof the slice asks for: in a release build of `lockdown-app`, `account_login` and `recover` refuse on an empty computer, and the only functions that still write a plain key file there by joining aren't compiled. The other plain maker, `open_or_create_plain`, makes a local install, not a join (D534, §5 gap 2).

## 5. Gaps

1. **The web vault's device-link join is plain on purpose.** `App::join_start` writes the plain key file before the approval, and `App::join_finish_signed_in` finishes it. In a tab that file lives in the worker's in-memory file system and is sealed under the sign-in's export key between unlocks (doc 17, doc 45 §3), so a password seal would break the tab's model. Both are public in a native build too, because the web crate's native tests run them and a native workspace build compiles the web crate. No native shell calls them: the desktop's `join_start` command calls `join_start_sealed`, and the command line and the server don't link the device-link join. A later slice could fence them to the wasm target plus `test-support` once the web crate's native tests turn the feature on (D644, proposed).
2. **`open_or_create_plain` still has no compile-time fence** (doc 45 §5 gap 5). It makes a passwordless local install, not a join, and the server's restore-drill account needs it in a production binary. Unchanged here.
3. **A workspace build with tests compiles the fixture into every binary of that build.** `cargo test --workspace` or `cargo build --workspace --all-targets` builds dev-dependencies, so the feature is unified into the desktop and the server binaries of that run. That is a test build; a release (`cargo build -p …`, `tauri build`, the tag workflows) never builds tests.
4. **The refusal's words name commands.** They are for whoever sends the call by hand, since no screen sends it. If a future screen ever could, it should route `not_set_up` to screen J instead of showing them.

## 6. unicron requests (host-contract gaps)

None. No screen, component, token or client interface changed.

## 7. Audit notes

- **No new path releases a secret.** The slice only removes a way to write keys. The refusals run before any network call or file write; `recover` checks the root inside the core's worker, as the join did.
- **The fixtures are the same code as before,** moved, not rewritten: `sign_in_new_device` with `seal = false`. In a build without `test-support` nothing calls it with `false`.
- **The passwords cross as before.** The IPC commands still hold them in `Zeroizing`; the refused calls drop them without use.
- **No wire shape changed.** The IPC commands keep their names, arguments and result type; the error is an existing code with words. No wasm, extension or schema file moved.

## 8. Test counts

| Crate | Before | After |
|---|---|---|
| `lockdown-app` (all targets) | 244 | 249 (5 new in `no_plain_joins.rs`) |
| `lockdown-cli` | COUNT_CLI | COUNT_CLI (one moved to the fixture) |

## 9. Proposed decisions

For the coordinator to record. **Flagged** rows are user-visible and need Sean. "Audit item" rows go to the external audit under D38.

| ID | Decision | Owner |
|---|---|---|
| D640 | **The core's old joins refuse on a computer that hasn't joined** (builds D536 on Sean's go). `App::account_login` and `App::recover` there answer `not_set_up` with words naming `account_login_sealed` / `recover_sealed` and `open_joined`, and write nothing, even with the right password and kit; a half-done old join's plain key file is left as it was for the sealed join to seal. On a computer that holds the identity, `account_login` is the fresh session it was and `recover` refuses as it did. Plain version: no part of Lockdown can add a computer without protecting it with your master password. | Under D38; closes doc 45 gap 1; audit item |
| D641 | **The old unsealed join is a test fixture by name.** `App::account_login_plain` and `App::recover_plain` (`join_plain.rs`) are compiled only for lockdown-app's unit tests and with its `test-support` feature, which only `[dev-dependencies]` may turn on; a test checks every workspace manifest and the `desktop-bare` CI job checks the desktop, server and CLI resolve without it. Tests that need a second device join sealed; only tests about old installs use the fixture. Plain version: the tests can still make an old-style install to prove old installs keep working, and nothing you install contains that code. | Under D38; audit item |
| D642 | **The desktop's `account_login` and `recover` IPC commands keep their shape** (D62) and carry the refusal on an empty computer; the account dialog's sign-in on an enrolled computer is unchanged, and no screen sends either call on an empty one. Plain version: nothing you see changed. | Accepted by default under D38 |
| D643 | **The restore-drill account stays as it is.** Its read-back signs in on a copy that already holds the identity (a fresh session, never a join), and its first run makes a local install with `open_or_create_plain` (D534) and registers it. Plain version: the server's backup check works as before. | Under D38 |
| D644 | **Fence the web vault's plain device-link join later (proposed, not built).** `App::join_start` and `App::join_finish_signed_in` stay public in native builds because the web crate's native tests run them; a later slice could compile them only for the wasm target and `test-support`, with the web crate's dev-dependencies turning the feature on. Plain version: the browser vault's own way of joining stays as it is for now. | Under D38; audit item |
| D645 | **The refusal uses the existing `not_set_up` code with words, and the words are exported** (`JOIN_SEALED_LOGIN`, `JOIN_SEALED_RECOVER`). No new error code or `detail` value. Plain version: older and newer parts keep understanding each other. | Accepted by default under D38 |
