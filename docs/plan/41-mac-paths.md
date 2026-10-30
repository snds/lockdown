# 41: Where Lockdown keeps its files on a Mac

_Slice: `slice/mac-paths`, built 2026-10-30. Urgent fix for Sean's Mac. With the ad-hoc signed CI build from main, **Explore the demo** on Welcome said: `couldn't make /Users/sean.sands/Library/Group Containers/lockdown/demo/lockdown: Operation not permitted (os error 1)`. **Create a new vault** and every first run on that Mac failed the same way. This slice builds on D15 (the shared identity's places), D47 (Lockdown's data beside the identity), D86 (the app and the command line are one device), D450 (the ad-hoc macOS seal), D460–D489 (first run, the developer reset and the demo) and the coordinator's calls under D375. Proposed decisions D490–D499 (§8)._

## For future agent

- **TL;DR:** Since macOS 15, `~/Library/Group Containers` is protected. A program may use only the group containers that its signed `com.apple.security.application-groups` entitlement names. An ad-hoc signed app can't carry a usable one, so every write there fails with "Operation not permitted". Lockdown now keeps both its identity and its data in `~/Library/Application Support/snds/` (`identity` and `lockdown`, the same layout as Linux). It uses a group container only when the running program is validly signed with a Developer ID and names `<TEAM>.design.snds.identity`. No build is signed that way today. One crate decides the places (`lockdown-paths`), and the app, the command line, the browser host and the developer reset all ask it. An older build's folders are moved out of Group Containers once, at start. A folder Lockdown can't make is named in plain words, with a fix.
- **Read with:** `crates/paths/src/{lib,settle}.rs`; `crates/app/src/paths.rs`; `Paths::resolve` and `Paths::probe` in `crates/app/src/session.rs`; `bins/cli/src/{app,main}.rs`; `crates/native-messaging/src/endpoint.rs` (`data_dir_from_env`); `crates/devtools/src/shell.rs` (`home`); `own_app_groups` in `crates/codesign/src/macos.rs`; `crates/app/examples/paths.rs`; `packaging/mac-paths/check.sh` and the `mac-paths` job in `.github/workflows/ci.yml`.
- **Do not:**
  - Call `ecosystem_identity::default_root()` from Lockdown. Ask `lockdown_app::paths` (or `lockdown_paths` from a crate that links no vault code).
  - Write anything under `~/Library/Group Containers` in Application Support mode. The only thing ever done there is reading an older build's folders, once ([§2.3](#23-the-one-time-move)).
  - Ship a Developer ID build that carries the app group before the switch-over in gap 1 is built. Without it, that build would start in an empty group container.
  - Turn on the move when `--root`, `--data`, `LOCKDOWN_ROOT` or `LOCKDOWN_DATA` is given. In that case the person chose the places.

## 1. The shape

```text
                     ┌──────────────────────── lockdown-paths ────────────────────────┐
  app (Tauri setup) ─┤ default_dirs(): Os::current(), the environment, mac_check()     │
  devtools reset ────┤ dirs_for(os, env, store): the rule, pure, tested per platform   │
  command line ──────┤ settle(): the one-time move out of Group Containers              │
  browser host ──────┤ make_dir() / cant_make(): a folder, or a sentence naming it      │
                     └────────────────────────────────────────────────────────────────┘
                        mac_check(): lockdown_codesign::macos::{own, own_app_groups}
```

| Platform | Identity root | Data directory | Changed? |
|---|---|---|---|
| Linux | `$XDG_DATA_HOME/snds/identity` (else `~/.local/share/…`) | `…/snds/lockdown` | No (D15, D47) |
| Windows | `%LOCALAPPDATA%\snds\identity` | `%LOCALAPPDATA%\snds\lockdown` | No (D15, D47) |
| macOS, every build today | `~/Library/Application Support/snds/identity` | `~/Library/Application Support/snds/lockdown` | **Yes**: was `~/Library/Group Containers/group.design.snds.identity` and `~/Library/Group Containers/lockdown` |
| macOS, a Developer ID build naming `<TEAM>.design.snds.identity` | `~/Library/Group Containers/<TEAM>.design.snds.identity` | `~/Library/Application Support/snds/lockdown` | Decided at run time; no build does this yet (gap 1) |
| iOS, Android | The shell passes the container's path | — | No |

`--root` alone still puts the data beside it (D47). `--data` alone keeps the default root. `LOCKDOWN_ROOT` and `LOCKDOWN_DATA` work as before. An empty value now counts as unset in the app, as it already did in the host.

## 2. What was built

### 2.1 Every place that resolved paths, before and after

| Where | Before | Now |
|---|---|---|
| The app: `lockdown_app::Paths::resolve` (`crates/app/src/session.rs`) | `ecosystem_identity::default_root()` and its own `beside` | `lockdown_app::paths::default_paths()` → `lockdown_paths::default_dirs()`; `beside` is `lockdown_paths::beside` |
| The desktop shell's start (`apps/desktop/src-tauri/src/lib.rs` `paths()`) | `Paths::resolve(LOCKDOWN_ROOT, LOCKDOWN_DATA)` | `lockdown_app::paths::at_start()`: the same variables, otherwise the one-time move and then the defaults |
| The developer tools' home (`crates/devtools/src/shell.rs` `home()`): the reset's places and the demo's parent | The same copy as the shell | `lockdown_app::paths::at_start()`. The reset's guard and its identity rule are unchanged (D482, D483). |
| The command line (`bins/cli/src/app.rs` `Paths::resolve`) | Its own copy of the rule and of `beside` | `lockdown_app::Paths::resolve`, so the two can't drift (D86). `run()` does the one-time move when neither `--root` nor `--data` is given. |
| The browser host (`lockdown_native_messaging::endpoint::data_dir_from_env`) | `LOCKDOWN_DATA`, else `beside(LOCKDOWN_ROOT or ecosystem_identity::default_root())` | `LOCKDOWN_DATA`, else `beside(LOCKDOWN_ROOT)`, else `lockdown_paths::default_dirs().data`. `endpoint::beside` re-exports `lockdown_paths::beside`. The crate no longer depends on the identity crate. |

**The browser socket still fits.** Application Support is a longer path than Group Containers was, and a Mac caps a socket path at 104 bytes. The existing short-path fallback (`endpoint::socket_path`) still applies, unchanged:

- When `<data>/browser.sock` is at most 100 bytes, the socket lives inside the data directory. `/Users/sean.sands/Library/Application Support/snds/lockdown/browser.sock` is 73.
- Past 100 bytes (a home folder name over about 45 characters), it moves to `$TMPDIR/snds-lockdown-<hash>/browser.sock`, about 80 bytes on a Mac.

The endpoint file (`native-host.json`) is a regular file in the data directory and has no such limit. `the_mac_data_dir_keeps_its_socket_within_the_limit` tests both cases.

The host links `lockdown-paths` directly rather than `lockdown-app`, because it is the small program a browser launches and it links no vault code. `lockdown_app::paths` wraps the same function, so there is still one rule.

### 2.2 The mode, decided at run time

`mac_check()` reads this program's own signature once: `lockdown_codesign::macos::own()` for the kind and team, and a new `own_app_groups()` for the entitlement. `own_app_groups` uses `SecCodeCopySigningInformation` with `kSecCSRequirementInformation` and reads `com.apple.security.application-groups` from `kSecCodeInfoEntitlementsDict`. `MacStore::from_signature(signed, team, groups)` returns `GroupContainer(g)` only when all three hold:

- the signature is valid and made with a certificate (not ad hoc);
- it has a team;
- it names exactly `<team>.design.snds.identity`.

In every other case it returns Application Support. An ad-hoc build that names the group stays in Application Support: CI signs a copy that way to prove it (§3). The old unprefixed `group.design.snds.identity` never qualifies. It's a pure function, so the tests cover every case on any platform. There is no environment override, because the tests don't need one.

### 2.3 The one-time move

`lockdown_paths::settle()` runs at start, on a Mac in Application Support mode, when the default places are in use. It works on the old places (`~/Library/Group Containers/group.design.snds.identity` and `~/Library/Group Containers/lockdown`) and the new ones.

| Case | What happens |
|---|---|
| The note `~/Library/Application Support/snds/lockdown-moved-from-group-containers.txt` is there | Nothing; the old places aren't looked at (`Checked`). |
| Either new place holds anything, or can't be read | Nothing is read from the old places (`InUse`). |
| Neither old place holds anything | Nothing (`NothingOld`). |
| An old place exists but can't be read (macOS refused, or a file is where the folder should be) | **Nothing moves, not even the readable half**, because a device key without its identity reads as "not enrolled". The old copy stays where it is, untouched. Start-up goes on as on an empty computer, so the person lands on Welcome and makes a new vault (`Unreadable`). |
| The old places can be read and the new ones are empty | Each old folder is copied in full to `<new>.moving` beside its new place: folders and plain files only, with the files' permissions kept. Links and sockets are left behind, and the app makes its socket again. Then both are renamed into place. The identity's old copy stays, because other ecosystem apps may still look there. Lockdown's own old data folder is removed when macOS allows it (`Moved`). |
| A copy or a rename fails part way (a full disk, say) | What was copied or renamed is removed. The new places are as they were and the old ones untouched (`Failed`). |

Whatever happened, the note is written, saying what happened in one line. From then on the old places are never read again. That gives three things:

- macOS asks at most once whether Lockdown may read another app's data, never at every start.
- A developer reset, which empties the new places but not the note, doesn't bring the old copy back.
- A person who later grants access (System Settings → Privacy & Security) and wants the old copy can delete the note to get one more look at the next start.

The app and the command line write a line to standard error when something moved, failed or couldn't be read. The line names folders and never a secret.

**What happens to an unreadable copy:** it stays in Group Containers, as it was. Lockdown never writes there and never tries again unless the note is deleted. Sean has the developer reset for the new places. The old folder can be removed in Finder, or with `rm -rf ~/Library/Group\ Containers/lockdown` from a Terminal that has Full Disk Access.

`onboarding.cbor` (D478) keeps the identity root the first run created, as a path. After a move it still names the old root. Nothing makes a decision from that path (the reset's identity rule reads the records, D483), so it is left as is.

### 2.4 Sibling apps

Lockdown and the other ecosystem apps (Mailbird, Synapse) won't share one identity on a Mac until they are signed with a Developer ID that carries the team's group. Each unsigned app now keeps the identity in `~/Library/Application Support/snds/identity`. That folder is outside any sandbox, so unsandboxed sibling builds could still share it if they adopt the same rule. A sandboxed app, though, can only reach its group container. None ships today, so nothing breaks. The identity crate's `default_root()` still names the unprefixed group on a Mac (gap 2).

### 2.5 A folder that can't be made, in words

`lockdown_paths::make_dir` and `cant_make` replace each raw `couldn't make …: … (os error N)`:

| Cause | The sentence |
|---|---|
| Permission | "Lockdown couldn't create its folder *dir*: macOS didn't allow it. Check that your user account can write to *parent*, then try again." ("Windows" or "the system" elsewhere) |
| Disk full or over quota | "…: the disk is full. Free some space, then try again." |
| Read-only disk | "…: the disk it's on can only be read. Use a disk Lockdown can write to, then try again." |
| A file in the way | "…: a file is in the way where a folder should be, in *parent*. Move that file aside, then try again." |
| Anything else | "…: macOS said "*kind*". Check that the disk is connected and that your user account can write to *parent*, then try again." |

Where it applies:

- **The probe** (`Paths::probe`, native builds) makes the data directory first. A computer that won't allow it shows **Lockdown can't start** with that sentence on Welcome, before anything is asked.
- **The demo's open** probes first, so **Explore the demo** shows the same sentence in its toast.
- **Saving `onboarding.cbor`** and both device-key files (`local_unlock`).
- **The local server's folder** (`lockdown_server::local`).
- **The browser endpoint's start.**
- **The reset's pending note.**
- **`lockdown init`.**

The front end shows the core's words as before, so no view changed. The harness state `desktop-unlock-cant-start` carries the new sentence and was taken again.

## 3. How to run it and test it

- **Where this computer's Lockdown is:** `cargo run -p lockdown-app --example paths` prints the signature check, the store, the move's result, the root, the data directory, the browser host's data directory, and whether an identity is there. It exits non-zero if the host and the app disagree or the probe fails.
- **Tests:**
  - `cargo test -p lockdown-paths` (18 tests): each platform's places, the signature rule, the identity crate's places on the test machine (Windows and Linux unchanged), the move's cases, the sentences.
  - `cargo test -p lockdown-app --lib paths` (4 tests): the app's defaults are the rule's; `resolve`'s cases; a Mac never in Group Containers; the probe's words.
  - `cargo test -p lockdown-cli --bin lockdown agree`: the command line, the browser host and the app give the same places on the test machine.
  - `cargo test -p lockdown-cli --test mac_move`: a real vault made by the command line at the old places opens, with its login, at the new ones after the move.
  - The reset's own tests (`cargo test -p lockdown-devtools`) are unchanged and pass. Its places come from `Paths` as before.
- **On a real Mac (CI's `mac-paths` job, `macos-latest`):** `packaging/mac-paths/check.sh target/debug/lockdown target/debug/examples/paths` runs three checks:
  1. With a fresh `$HOME`, the app's resolution, the browser host's and the command line's `init` and `list` all put the identity and the data under `Library/Application Support/snds/`, and `Library/Group Containers` doesn't exist afterwards.
  2. A copy of the example, ad-hoc signed with `ABCDE12345.design.snds.identity` in its entitlements, reads the group from its own signature and still answers Application Support. If macOS refuses to run such a copy at all, the check says so and passes, since that refusal is the same point.
  3. A vault made at the old places (`--root …/Group Containers/group.design.snds.identity`) is moved by the first plain `lockdown list`, opens with its login, leaves no old data folder, writes the note, and isn't moved twice.
- **What a CI runner can't show:** the protection itself. A runner's fresh `$HOME` under a temporary folder isn't the real `~/Library/Group Containers` that TCC (macOS's privacy guard) protects. The runner's shell may also hold Full Disk Access. So CI can't reproduce "Operation not permitted". It proves the thing that fixes it: no path Lockdown resolves on a Mac is under `Library/Group Containers` unless a Developer ID signature carries the group, and the code that reads entitlements works on a real Mac. Sean's Mac is where the refusal happens, and with these places Lockdown never asks for one.
- **The harness:** `pnpm capture --only desktop-unlock-cant-start` in `design/capture` (6 images, no axe violations).

## 4. Gaps

1. **The signed build's switch-over (written down, not built).** A Developer ID build would carry `com.apple.security.application-groups = [<TEAMID>.design.snds.identity]`, the group Mailbird and Synapse share. The run-time choice in §2.2 already sends such a build's identity root to `~/Library/Group Containers/<TEAMID>.design.snds.identity` while the data stays in Application Support. What must exist before such a build ships:
   1. **The identity crate learns the team prefix.** `ecosystem_identity::default_root()` today returns the unprefixed `group.design.snds.identity` on a Mac. It needs `default_root_for_group(group)` (or to read the entitlement itself, as `mac_check` does), so every ecosystem app agrees on one container. `lockdown_paths::dirs_for` then calls it instead of joining the path itself.
   2. **The one-time switch-over.** On the first start of a group-signed build, if the group root is empty and `~/Library/Application Support/snds/identity` holds an identity, copy it in the same way as §2.3: a `.moving` folder, then a rename, with its own note (`identity-moved-to-group.txt`). The Application Support copy stays for the command line (below). Lockdown's data doesn't move: it is Lockdown's alone and stays in Application Support in both modes. An unreadable or empty source means nothing moves and the app starts at the group root.
   3. **The command line and the browser host.** Neither carries an entitlement, so both would still resolve Application Support. The host only needs the data directory, which doesn't move, so it keeps working. The command line needs the identity: either it is signed with the same group (a Developer ID command-line tool can carry it), or it reads the app's root from a small pointer file the app writes in the data directory. Until one of those exists, the command line on a signed Mac reads the Application Support copy, which goes stale.
   4. **The developer reset** follows `Paths` and needs no change. Its identity rule (D483) matters more there, because a group container is shared with siblings by design.
   5. **A release check** that fails a group-signed build whose identity crate doesn't know the prefix. Until then, `release.yml` signs with no entitlements file, which keeps every build in Application Support.
2. **The identity crate's own default** still names Group Containers on a Mac. Any other ecosystem app that calls it fails the same way Lockdown did. The request goes to the crate's owner: use Application Support until signed (this slice's rule), and add the team prefix after that (gap 1.1).
3. **Siblings don't share an identity on a Mac** until signed builds (§2.4). Acceptable while none ships.
4. **The old identity copy stays in Group Containers** after a move, because it may be shared. Lockdown never removes it. The developer reset doesn't list it, since it only erases the new places (D482).
5. **The identity root itself** is made by the identity crate, whose errors are still its own words. The probe makes the data directory first, beside the root under the same `snds` folder, so a computer that refuses one refuses at the probe, in Lockdown's words.

## 5. unicron requests (host-contract gaps)

None. No view changed. The can't-start state uses Welcome's existing heading and sentence.

## 6. Audit notes

- **No secret moves differently.** The move copies files byte for byte, keeping their permissions (0600 for the sealed keys). New folders are made 0700. The old data folder is removed afterwards when it can be. A failed move removes its partial copy.
- **Reading another app's container** happens at most once per computer, gated by the note, and only for Lockdown's own two old folders.
- **The signature check** reads only this process's own signature. A forged entitlement in an ad-hoc signature can't steer the identity into a group container: only a valid certificate signature whose team prefixes the group can.
- **Logged lines** name folders and outcomes only.

## 7. Test counts

Rust: 25 new tests (18 in `lockdown-paths`, 4 in `lockdown-app`, 2 in `lockdown-cli`, 1 in `lockdown-native-messaging`); the workspace totals are in the PR. Design harness: one state's words changed and taken again. Desktop vitest unchanged: no view changed.

## 8. Proposed decisions

For the coordinator to record. **Flagged** rows are user-visible and need Sean. "Audit item" rows go to the external audit under D38.

| ID | Decision | Owner |
|---|---|---|
| D490 | **One place decides Lockdown's paths.** `lockdown-paths` (`crates/paths`), wrapped by `lockdown_app::paths`: the app, the command line (through `lockdown_app::Paths::resolve`), the browser host's `data_dir_from_env` and the developer tools' home all ask it, and none calls `ecosystem_identity::default_root()`. Plain version: the app, the command line and the browser connection can't disagree about where your vault is. | Under D375 (coordinator's call) |
| D491 | **On a Mac, Application Support until a signed build carries the app group.** The identity in `~/Library/Application Support/snds/identity`, the data in `…/snds/lockdown`, mirroring Linux; nothing written under `~/Library/Group Containers`. Amends D15 and D47 on macOS. Plain version: Lockdown keeps its files where an unsigned Mac app is allowed to. | Under D375; **flagged for Sean** (where files are on his Mac) |
| D492 | **The mode is decided at run time from the program's own signature.** A group container only when the signature is a valid certificate signature with a team and names exactly `<team>.design.snds.identity`; ad hoc, unsigned, another team's group or the old unprefixed group mean Application Support. The data directory stays in Application Support in both modes. Plain version: only a properly signed Lockdown uses the shared Mac container. | Under D375; audit item |
| D493 | **The signed future is written down, not built** (gap 1): the identity crate learns the team prefix, a one-time copy of the identity into the group, the command line signed with the group or following a pointer, and a release check, all before a group-signed build ships. Plain version: the step to a shared container with Mailbird and Synapse is planned, and nothing ships half of it. | Under D375 |
| D494 | **The one-time move out of Group Containers.** At start, default places only: when both new places are empty and every old place that exists can be read, copy each to `.moving` and rename into place; the identity's old copy stays, Lockdown's old data folder is removed when allowed; an unreadable old place moves nothing and never fails start-up; a partial copy is undone; a note beside the data directory makes it once only, so a reset doesn't bring the old copy back. Plain version: an earlier test build's vault comes along on its own, once, if the Mac lets Lockdown read it. | Under D375; **flagged for Sean** (an unreadable old copy is left behind); audit item |
| D495 | **Sibling apps don't share an identity on a Mac until signed builds.** Each unsigned app keeps its own copy in Application Support (or none); acceptable while none ships. Plain version: Mailbird and Synapse will join your Lockdown identity on a Mac once they're all properly signed. | Under D375; **flagged for Sean** |
| D496 | **Windows and Linux are unchanged,** proved by a test that compares with the identity crate's places on the machine the tests run on, and by fixed-environment tests for each. Plain version: nothing moves on Windows or Linux. | Under D375 |
| D497 | **A folder Lockdown can't make is named in words, with a fix.** `make_dir`/`cant_make` for permission, a full disk, a read-only disk, a file in the way and anything else; the probe makes the data directory first, so Welcome says "Lockdown can't start" with that sentence before asking for anything, and the demo's open says it too. Plain version: if your Mac won't let Lockdown save, it tells you which folder and what to try. | Under D375; **flagged for Sean** (the words) |
| D498 | **The Mac proof runs on a real Mac in CI.** The `mac-paths` job: a fresh `$HOME`, the app's resolution, the browser host's and the command line's places under `Library/Application Support/snds/` and nothing under `Library/Group Containers`; an ad-hoc signature naming the group still kept out; an older build's vault moved once and opened. What a runner can't show (TCC's refusal) is written down (§3). Plain version: every change is checked on a Mac before it reaches you. | Under D375 |
| D499 | **Shape additions** (D62). New crate `lockdown-paths`; public `lockdown_app::paths::{default_paths, from_env, at_start, settle_defaults}` and its re-exports; `lockdown_codesign::macos::{own_app_groups, APP_GROUPS_ENTITLEMENT}`; `Paths::probe` makes the data directory; `lockdown_native_messaging::endpoint::beside` is now a re-export. No wasm or IPC wire shape changed. Plain version: new pieces only; nothing the app sends changed. | Under D375 |
