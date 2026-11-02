# 43: Every vault sealed on the command line

_Slice: `slice/cli-sealed`, built 2026-11-01. Sean's call on the D466 flag review: "lockdown init without --name still makes a passwordless install for old scripts; that should become a refusal or a prompt, so a command-line-only install is sealed too." He wants every new vault sealed, on every surface. Builds on D38 (who decides security-model details), D47 (where the files live, and no secret as an argument), D86 (the command line and the app are one device), D460 (the local server inside the command), D461 (the sealed device keys), D463 (the name), D464 (the strength floor), D465 (the migration), D466 (the command line's master password), D467 (a forgotten password), D473 (moving to your own server) and doc 40 (the developer tools and their release check). Proposed decisions D510–D515 (§8)._

## For future agent

- **TL;DR:** `lockdown init` can no longer make a vault without a master password. At a terminal it asks for the name (the computer account's user name offered) and the master password twice with echo off, after one line on what the password is for. In a script, `--name` with the password from `--master-password-stdin` or `LOCKDOWN_MASTER_PASSWORD` runs without a prompt (`init --password-stdin` and `LOCKDOWN_PASSWORD` still work, as D466 had them). With neither, it refuses in words naming both ways, exits non-zero and writes nothing. The vault is made by the app's own first run (`App::first_run_create`), so it is the same account the desktop makes: the core opens it with the same name and password and not without. The kit is printed (or `--save`d) once, with a line saying where it went. `account register` on a sealed vault moves its local account to that server (the app's D473 move), with the master password as the account's password. Installs made before the master password keep opening, say so on every command, and `password set --name` seals them.
- **Read with:** `bins/cli/src/sealed.rs` (init, the move, the words), `bins/cli/src/master.rs` (`given`, `prompt`), `bins/cli/src/main.rs` (the `Init` and `Account Register` arms), `App::open` in `bins/cli/src/app.rs` (no plain key made any more), `crates/devtools/src/legacy.rs` and `crates/devtools/src/bin/lockdown-legacy-install.rs`, `bins/cli/tests/cli_sealed.rs`; doc 39 §1.2–§1.6 and §1.11; doc 40 §5.
- **Do not:**
  - Add a passwordless way back into `init`, behind a flag or a variable. A test or a tool that needs an old install uses `lockdown_devtools::legacy::make` (Rust) or `lockdown-legacy-install` (anything else), which only the `legacy-install` feature builds.
  - Make device keys anywhere in the command line's own open (`App::open`). A missing key file is refused in words; keys come only from `init` (sealed) or from joining an account (`account login`, `recover`).
  - Print or log the master password. It is read from the first stdin line, the variable or the terminal with echo off, and lives in `Zeroizing` strings until the core takes it.
  - Read `LOCKDOWN_PASSWORD` as a master password for anything but `init` (D466: it keeps its meaning; `init`'s local account is the one place where the account's password and the master password are the same by construction).

## 1. What `init` does now

| Case | What happens |
|---|---|
| A terminal, nothing given | Prints "Your master password opens your vault, and Lockdown can't see it or reset it. If you forget it, only your recovery kit gets you back in; you'll get the kit next." Asks `Your name, shown when Lockdown is locked [sam]:` (Enter takes the offered name, from `USER`, `USERNAME` or `LOGNAME` when it is a valid name). Asks `Choose a master password:` with echo off, refuses a weak one at once with the app's words, asks `Type it again:`, refuses a mismatch with "The two passwords don't match yet." |
| `--name` and the password on stdin (`--master-password-stdin`, or `init --password-stdin`) | No prompt. The name and the strength are checked before anything is made. |
| `--name` and `LOCKDOWN_MASTER_PASSWORD` (or `LOCKDOWN_PASSWORD`) | The same, from the variable. |
| A terminal with only part given | Asks only for what is missing. |
| No terminal, and the name or the password missing | "lockdown init needs a name and a master password. Give them with --name NAME and --master-password-stdin (or LOCKDOWN_MASTER_PASSWORD), or run lockdown init in a terminal to be asked." Exit 1. Nothing is written: the check runs before the core starts. |
| Already set up (a sealed or a plain key file) | Opens what is there and prints `identity attached, vault opened` and the status, as `init` always did; a sealed vault needs the master password, a plain one says it has none. With `--name`, `--password-stdin` or `--save` it refuses: "Lockdown is already set up on this computer. lockdown password set gives an older install a master password." |
| An identity at the root but no Lockdown keys here | Refused: "An ecosystem identity is already at ROOT, and this computer has no Lockdown keys for it. Join it with lockdown account login or lockdown recover, or choose another --root." |

The words for the name, the weak password and the mismatch are the app's first-run screen's (doc 39 §1.9 screen 1, §2.3), and the strength rule is the core's own `check_master_password` (zxcvbn 3 with the name as a guessable word, D464), applied before the second ask at a terminal and again by the core.

**The recovery kit.** The core's first run makes it, as the app's does, and `init` hands it on the way `password set` and `account register` already did (`deliver_kit`): printed on stdout, or written with `--save FILE` (readable only by the person, never over a file that exists, the target checked before anything is made). A printed kit is followed by "Your recovery kit is printed above, shown once. Print it or save it now and keep it away from this computer: if you forget your master password, it's the only way back in." A saved one says "recovery kit saved to FILE (readable only by you); print it and put the page away".

## 2. The pieces

| Piece | Where | What it does |
|---|---|---|
| `init` | `bins/cli/src/sealed.rs` `init` | Routes on the key file: none → the sealed first run; sealed or plain → `again` (opens, as before). Works out the name and the password from the flags, the variables or the terminal, refuses before anything is written when it can't, checks the name and strength, then calls `App::first_run_create` and delivers the kit. |
| The given password | `master::given` | The first stdin line (`--master-password-stdin`) or `LOCKDOWN_MASTER_PASSWORD`, without asking. `init` falls back to `init --password-stdin` and `LOCKDOWN_PASSWORD`, and refuses both stdin flags at once. |
| The terminal | `sealed::ask_name`, `sealed::ask_new_password`, `master::prompt` | The name with echo on and the offered default; the password twice with echo off (the existing `EchoOff`). |
| The move | `sealed::register`, from `Account Register` on a sealed vault | Takes the master password the usual way, refuses an account password that differs, a Secret Key or `--kit-file` (the vault keeps its kit), unlocks the core with the password and runs `App::move_to_server`: the same identity and kit registered on the server under the login, every vault pushed from its first head, then the server inside the command closed and its folder removed. A vault already on a server: "This vault already syncs with a server." |
| No plain keys in the CLI's own open | `App::open` in `bins/cli/src/app.rs` | A missing key file is refused ("this computer has no Lockdown keys in DATA yet; run `lockdown init`, or `lockdown account login` or `lockdown recover` to join an account") instead of making a plain one. `init` on an existing install still goes through `App::init`, so it pulls the account's vault or makes the persona's first vault as before. |
| Old installs for tests | `crates/devtools/src/legacy.rs` (feature `legacy-install`), `lockdown-legacy-install` | Makes a passwordless install the way the desktop's "This device only" and the old `init` did: the core opened with `create` and no password. Refuses a place that holds anything. Only the CLI's tests (a dev-dependency) and the tool turn the feature on. Anything that links it carries `lockdown_devtools::MARKER`, so `packaging/devtools/check.sh absent` would fail a release that shipped it. |

## 3. Who changed

| Caller | Change |
|---|---|
| `bins/cli/tests/end_to_end.rs`, `search.rs` | `init --name` with `LOCKDOWN_MASTER_PASSWORD`; the end-to-end test checks the sealed file and no plain one. |
| `bins/cli/tests/master_password.rs` (older install), `sync_end_to_end.rs`, `tls_end_to_end.rs`, `kit_and_secret_key.rs`, `session_renewal.rs`, `security.rs` | Start from `lockdown_devtools::legacy::make`. These are the plain-install account flows (register with a new kit, the kit typed back, recover, revocation, renewal, the PQ line read from the plain key file) that existing installs still run; they are now the regression suite for "old installs keep working". `shared_vault.rs` and `kit_and_secret_key.rs`'s other `init`s run on an install that exists, so they are unchanged. |
| `bins/cli/tests/cli_sealed.rs` | New: the eight tests of §4. |
| `apps/web/e2e/live.spec.ts`, `web-vault.spec.ts`, `finish.spec.ts` | `init --name` with `LOCKDOWN_MASTER_PASSWORD`, the Secret Key read from `init`'s kit, then `account register` (the move). The `beforeAll` hooks get a longer timeout: a debug CLI stretches at the desktop floor, about 3 s per open. |
| `apps/web/e2e/pq-status.spec.ts` | Starts from `lockdown-legacy-install`: the older-device tool signs with the plain key file. |
| `.github/workflows/ci.yml` (web vault job) | Builds `lockdown-legacy-install` on its own, as it builds `older-device`. |
| `bins/server/container/smoke.sh` | `--master-password-stdin init --name Smoke`, the kit from `init`, `account register` with the master password in the variable. |
| `packaging/mac-paths/check.sh` | Unchanged: it already ran `init --name Runner --password-stdin`. |
| `--help` and the module docs | `init`'s and `account register`'s help, `InitArgs`, the binary's module doc, `sealed.rs`. |
| `docs/plan/39-first-run.md` | §2.2's "`init` without `--name`" row points here. |

The extension suite and the desktop's harness fixtures don't run `lockdown init` (they open the core directly), so they are unchanged.

## 4. How to run it and test it

- **By hand:** `lockdown init` in a terminal; `printf '%s\n' "$PW" | lockdown --master-password-stdin init --name Sean`; `LOCKDOWN_MASTER_PASSWORD=… lockdown init --name Sean --save kit.txt`; `lockdown init < /dev/null` (refused, nothing made). Then `lockdown list` asks `Master password for Sean:`. `LOCKDOWN_MASTER_PASSWORD=… lockdown account register --server https://host --login me` moves the vault to a server.
- **An old install:** `cargo build -p lockdown-devtools --no-default-features --features legacy-install --bin lockdown-legacy-install`, then `lockdown-legacy-install --root DIR`.
- **Tests:** `cargo test -p lockdown-cli` (31 tests, 8 new in `cli_sealed.rs`):
  - sealed init from the first stdin line, never printing the password; locked without it; `init` again opens and refuses `--name`;
  - sealed init from the variable with `--save` (the file 0600, nothing on stdout);
  - refusal without a terminal for no name, no password, and neither, with nothing under the temp folder afterwards;
  - a weak password refused with the app's words and nothing written; the same rule through `lockdown_app::check_master_password`;
  - the vault `init` made opens in the desktop core (`App::unlock_with_password`) with the onboarding name, and not through the device keystore alone or with a wrong password;
  - at a pseudo-terminal: the expectation line, the offered name, a mismatch refused, a weak password refused before the second ask, then a sealed vault, and `list` asking by name;
  - a passwordless install still opens with the notice, then `password set --name … --new-kit` seals it and the items are the same;
  - `account register` on a sealed vault: a different account password refused, the move, the local server's folder gone, a second device recovered with `init`'s kit and the master password, and a second move refused.
- **Web vault:** `cargo build -p lockdown-server -p lockdown-cli`, `cargo build -p lockdown-vault --example older-device`, the legacy tool as above, then `pnpm e2e` in `apps/web` (private copies with `LOCKDOWN_SERVER_BIN`, `LOCKDOWN_CLI_BIN`, `LOCKDOWN_OLDER_DEVICE_BIN`, `LOCKDOWN_LEGACY_INSTALL_BIN` and a private `LOCKDOWN_E2E_PORT`).

## 5. Gaps

1. **Joining leaves unsealed keys.** `lockdown recover` and `lockdown account login` on an empty computer still make the plain key file (they join an account; they don't make a vault). The desktop's join seals right after with the account's password (`setMasterPassword`); the command line should too (D514, proposed). Until then such a device says on every command that it has no master password. Closed by doc 44 (`44-cli-join-sealed.md`): both now seal under the account's password.
2. **The core can still make a plain install.** `App::open_or_create(true)` with no keys makes the plain key file. The desktop's join calls it and seals at once; the desktop's tests and the extension's endpoint fixture use it for throwaway vaults. Nothing user-facing reaches it without sealing after, but the core doesn't refuse it. Closed by doc 45 (`45-desktop-join-sealed.md`): `open_or_create(true)` now refuses a computer with no key file, and the fixtures, the developer tools and the server's restore-drill account call `open_or_create_plain` by name.
3. **`init` beside another app's identity.** With an ecosystem identity at the root that Lockdown hasn't joined, `init` refuses and points to `account login` and `recover`. The old passwordless `init` tried to attach and failed later; the app's first run refuses too.
4. **One try at a terminal.** A weak or mismatched password ends the command; it doesn't ask again. Scripts and people both get a clear sentence and run it again.
5. **The offered name.** It is the login name (`USER`), not the full name the desktop offers from the OS account, which needs a platform call the command line doesn't make.
6. **Windows prompts.** `master::prompt` refuses on a non-Unix terminal (it can't hide what is typed yet), so on Windows `init` needs `--name` and the password from stdin or the variable, and says so.
7. **The web vault e2e's speed.** A debug CLI stretches Argon2id at the desktop floor in about 3 s, unoptimised; the sealed `init` takes about 25 s. `[profile.dev.package.argon2] opt-level = 3` would cut that, but rebuilds everything above it in a shared target; not done here.

## 6. unicron requests (host-contract gaps)

None. The slice is the command line, its tests and harnesses; no unicron surface or token is involved.

## 7. Audit notes

- `init` builds the vault only through `App::first_run_create`: the keys sealed before the identity names them, the local account, the kit slot. No new crypto and no new file.
- The refusal path runs before `App::start`, and the tests check the temp folder is empty after every refusal.
- The move is the core's `move_to_server` unchanged; the command line adds only the master password check (`unlock_with_password`) and refuses a different account password, so the account's password and the master password can't drift apart on a sealed vault.
- The master password never reaches stdout or stderr (checked in the stdin, variable and terminal tests). The terminal's echo is off for both asks.
- `LOCKDOWN_PASSWORD` is still accepted by `init` as the new master password (D466's shape). Everywhere else it still never unlocks.
- The passwordless maker is test tooling in `lockdown-devtools`, behind its own feature, never a dependency of the CLI binary, the desktop or the server; any binary that links it carries the release check's marker.

## 8. Proposed decisions

For the coordinator to record. **Flagged** rows are user-visible and need Sean. "Audit item" rows go to the external audit under D38.

| ID | Decision | Owner |
|---|---|---|
| D510 | **`lockdown init` always seals.** It makes the vault through the app's first run: at a terminal it asks for the name (the user name offered) and the master password twice with echo off, after one line on what the password protects and the kit; in a script it takes `--name` and the password from `--master-password-stdin` or `LOCKDOWN_MASTER_PASSWORD` (or `init --password-stdin` and `LOCKDOWN_PASSWORD`); with neither it refuses, naming both ways, exits non-zero and writes nothing. Amends D466's last clause. Plain version: every vault the command line makes has a master password, as the app's do. | Under D375; **flagged for Sean** (the prompt, the default name, the words); audit item |
| D511 | **`init` prints or saves the kit as the other commands do,** and says where it went: "Your recovery kit is printed above, shown once…" or "recovery kit saved to FILE…". Plain version: setting up on the command line leaves you the same way back as the app does. | Under D375; **flagged for Sean** (the words) |
| D512 | **`init` on a computer already set up opens what is there** (as before), and refuses `--name`, `--password-stdin` and `--save` there with a pointer to `password set`. Plain version: running `init` twice is safe and never makes a second vault. | Under D38 |
| D513 | **No passwordless install in anything that ships.** The command line's own open refuses a missing key file instead of making a plain one; a passwordless install for tests comes only from `lockdown_devtools::legacy` (feature `legacy-install`) or the `lockdown-legacy-install` tool, which carry the release check's marker. Old installs keep opening, say so on every command, and `password set --name` seals them. Plain version: nothing you install can make an unprotected vault, and an old one keeps working until you give it a password. | Under D38; audit item |
| D514 | **Joining should seal too (proposed, not built).** `lockdown recover` and `lockdown account login` on an empty computer seal the new device's keys with the account's password, as the desktop's join does, so every later command needs the master password. Plain version: a computer you add from the command line is protected by your master password like the first one. | Under D375; **flagged for Sean** (a joined computer starts asking for the password) |
| D515 | **`account register` on a sealed vault moves it** to the server named (D473's move): the master password becomes the account's password there (a different one is refused), the vault keeps its kit and Secret Key, every vault is pushed, and the server inside the command is closed and removed. Plain version: on the command line too, your vault moves to your own server with the same password and kit. | Under D375; **flagged for Sean** (the words); audit item |
