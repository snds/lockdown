# 60: Automatic updates for the desktop app

_Slice: `slice/auto-update` (slice 59), built 2026-11-07, at Sean's request on the same day. Sean wants every merged change to reach his installed app without a download, and wants to check for an update whenever he likes. His answers:_

- _**Source:** a rolling "main" pre-release that CI publishes after every merge._
- _**Install:** download quietly, then offer "Restart to update". An update not taken installs on quit. Restarting locks the vault._
- _**Scope:** the desktop app only._

_It builds on D62 (wire and stored shapes only grow), D86 and D87 (the session token and the keychain), D88 (no webview permissions), D359 (CI proves signatures with throwaway keys), D450 (the ad-hoc macOS seal), D480 and D481 (the developer tools never ship), D485 (the demo's confined home) and D493 (the signed future). It adds one stored field (D62). There are no wire, wasm or IPC shape changes, only four new commands. New dependency: `tauri-plugin-updater` 2.14.1. Proposed decisions D680–D689 (§10)._

## For future agent

- **TL;DR:** Every merge to `main` runs the Release workflow. The build is stamped `0.1.0-main.<commit time, UTC>`. The macOS `.app.tar.gz` and the Windows NSIS installer are signed with the updater key. A job that runs only on a push to `main` then replaces the assets of one GitHub pre-release, `main-build`, including `latest.json`.

  The desktop app follows that manifest:
  - at launch, every six hours, or when asked;
  - the newer bundle is downloaded in the background and checked against the public key built in from `apps/desktop/src-tauri/updater.pub` (key ID `303459ECC0739D7D`);
  - then it offers **Restart to update** (Settings → About, and a quiet row in the sidebar);
  - restarting locks the vault the way Lock does, and an update not taken installs on quit.

  Linux packages, a machine-wide Windows install, and a Mac app in a folder the account can't write to can't replace themselves. There the app says a newer build exists and links to the release.

  On a Mac, an ad-hoc build no longer keeps the session token in the keychain, so an update never brings up the keychain dialog (§3).
- **Read with:**
  - `apps/desktop/src-tauri/src/updater.rs`: the rules, pure and tested.
  - `updater/shell.rs`: the plugin, the commands, the background check, install on quit, the macOS menu item, the e2e hooks and the signature tests.
  - `apps/desktop/src-tauri/src/keychain.rs`.
  - `apps/desktop/src/features/updates/`.
  - `.github/workflows/release.yml`: `meta`, `macos`, `windows` and `main-channel`.
  - `.github/workflows/ci.yml`: `updater`, `update-e2e` and `keychain-macos`.
  - `packaging/update/`.
- **Do not:**
  - Give the webview an updater permission. The page asks through the four commands.
  - Pass `TAURI_SIGNING_PRIVATE_KEY` to any step of a run other than a push to `main`. In the build jobs the expression yields an empty string unless `meta.publish` is true, and `main-channel` checks the event and the ref itself.
  - Mark `main-build` as latest, or make it a full release.
  - Let the Mac app keep anything in the keychain while its signature isn't an Apple-issued one (D684).
  - Read the keychain on a Mac with its dialogs on.
  - Turn on `update-test` in anything that ships. build.rs refuses it with `LOCKDOWN_DEVTOOLS=off`, and `packaging/devtools/check.sh` looks for its marker.

## 1. Architecture

```text
 merge to main ──▶ release.yml (push: main)
   meta: version = 0.1.0-main.<YYYYMMDDHHMMSS of the commit, UTC>; publish = (push && ref == main)
   linux · macos · windows · …: stamp-version.py, build as a dry run
     macos:   Lockdown_<v>_universal.app.tar.gz + .sig   (createUpdaterArtifacts; release key only when publish)
     windows: Lockdown_<v>_x64-setup.exe + .sig          (NSIS only; MSI takes no pre-release)
   main-channel (publish && secret; contents: write; concurrency main-build-publish)
     sign the deb · minisign -V every .sig against updater.pub · write-manifest.py (never backwards)
     main-build pre-release: tag moved to the commit, new files, latest.json last, old files deleted

 desktop app (lockdown-desktop, shell feature)
   updater::plan(version, key from updater.pub, test source) ──▶ Source{endpoint, pubkey, hosts}
   start(): at launch, then every 6 h while Settings.update_auto_check
   check(): GET latest.json (redirects held to GitHub hosts) ──▶ is_newer? ──▶ download ──▶ minisign
            check (signed version must match) ──▶ Ready { Update, bytes }
   update_restart: lock (as Lock) ──▶ install ──▶ relaunch   ·   RunEvent::Exit: lock ──▶ install, no relaunch
   page: Settings → About, sidebar "Restart to update", toasts for asked checks (UpdateClient)
```

| Piece | Where | What it does |
|---|---|---|
| The rules | `updater.rs` | `Channel::of` reads the channel from the version: a `main.*` pre-release follows "main", anything else will follow "stable". `plan` decides whether a build checks, and against what. `allowed` requires https on `github.com`, `objects.githubusercontent.com`, `release-assets.githubusercontent.com` or `github-releases.githubusercontent.com`, with no user name and no other port. `is_newer` is strict semver order. `key_id` and `key_is_valid` read the built-in key. `windows_per_user` and `Install` decide whether this copy may replace itself. `UpdateStatus` is what the page sees. |
| The plugin | `updater/shell.rs` | The plugin is driven from Rust, with the endpoint, the key, a strictly-newer comparator, a ten-minute timeout and a redirect policy that runs `allowed` on every hop. `plugins.updater` in `tauri.conf.json` sets `requireSignedVersion` (the version in the signature's trusted comment must equal the manifest's) and the passive Windows installer. Its `pubkey` stays empty, because the app passes the key in from build.rs. |
| Background | `start` | Checks at launch, then sleeps six hours. It reads `Settings::update_auto_check` before each check. Nothing is checked while the demo's confined home holds the network (D485). |
| Restart | `restart` | Takes the ready update, locks the vault and forgets the pending clipboard copy (exactly what `commands::lock` does), installs, and relaunches. On Windows the passive installer closes the app and starts the new one (`/P /UPDATE /R`). |
| On quit | `on_exit` | The app now uses `RunEvent::Exit`. A ready update installs there with no relaunch (`restart_after_install(false)`). |
| Menu | `install_menu` (macOS) | Adds "Check for Updates…" under "About" in the app menu. It shows the window and does what the button does. |
| Commands | `update_status`, `update_check`, `update_restart`, `update_auto_set` | Added at the end of the handler list. Changes arrive on the `lockdown://update` event. |
| Page | `features/updates/` | `UpdateClient` and its provider (only the desktop gives one, so the web vault shows nothing). Also `AboutSection`, `RestartRow` and `useUpdateNotices`, plus `app/tauri-updates.ts`, the only file that imports `@tauri-apps`. |
| The key | `updater.pub`, `build.rs` | build.rs reads the file (one line of base64 of the minisign `.pub` text, the form `tauri signer generate` writes and `plugins.updater.pubkey` takes) into `LOCKDOWN_UPDATER_PUBKEY`. Without the file, the app builds with updates off. |
| Keychain | `keychain.rs`, `lockdown_app::secrets` | Covered in §3. |

**What a check sends.** One `GET` of the fixed manifest URL, carrying only `accept: application/json`, `user-agent: tauri-plugin-updater/2.14.1` and `host`. This was captured from the real app in this slice (§5). There is no version, device, account or cookie. GitHub sees the address it comes from, as with any download. The bundle download sends the same headers.

## 2. The channel: version, key, publishing

**Version.** `meta` stamps every build that isn't a tag as `<x.y.z>-main.<YYYYMMDDHHMMSS>`. The time is the commit's committer time in UTC (`git log -1 --format=%cd --date=format-local:…`), so a rerun of a commit gets the same version. Semver compares the numeric field as a number, so each merge sorts after the last. `0.1.0-main.*` sorts before `0.1.0`. Once `v0.1.0` is tagged, `Cargo.toml` must move to the next version, or main builds would sort below the release (tested in `main_builds_order_by_commit_time_and_before_the_release`).

`packaging/update/stamp-version.py` writes the version into:
- `[workspace.package]` in `Cargo.toml`;
- every path dependency's requirement (`"0.1.0"` doesn't match a pre-release);
- the workspace crates in `Cargo.lock`, so `--locked` builds agree;
- `tauri.conf.json`, `apps/desktop/package.json` and `apps/web/package.json`.

Package managers order versions their own way, so each package gets its own form of the version (`packaging/update/version-forms.py`). The meta job checks every form before anything builds:

| Format | Main channel | Tag | Why |
|---|---|---|---|
| app, Cargo, npm, updater (semver) | `0.1.0-main.20261107025710` | `0.1.0` | |
| deb `Version` | `0.1.0~main.20261107025710` | `0.1.0` | With `-`, deb reads `main.…` as the revision, which sorts after a later plain `0.1.0`. `~` sorts before it. |
| rpm | Version `0.1.0`, Release `0.main.20261107025710` | `0.1.0`, Release `1` | rpm refuses `-` in Version. Tauri takes the rpm Version from the app's, and semver can't hold `~`, so the pre-release goes in Release, Fedora's convention for one. |
| MSIX | `0.1.0.0` | `0.1.0.0` | Four numbers only. |
| extension | `0.1.0` | `0.1.0` | Browsers take only numbers there, and the extension isn't on the channel. |

In the Linux container (`packaging/repro/desktop-container.sh`), a main build:
1. builds the deb with the app's version, then rewrites its control `Version` (`version-forms.py rewrite-deb`);
2. bundles the rpm from the same binary with `tauri bundle` and the rpm config;
3. runs the usual normalisation afterwards.

`test_update_scripts.py` checks the ordering with `dpkg --compare-versions` and `rpm.vercmp` where they are installed:
- older main build < newer main build < the release;
- the release < the next version's main build (deb).

A tag build keeps its plain version, and `meta` still refuses a tag that differs from the manifests.

**Windows installer.** MSI refuses a pre-release identifier that isn't a number up to 65535 (`tauri-bundler` `msi/mod.rs`). NSIS takes the version: `VIProductVersion` drops the pre-release. So main builds and dry runs make **NSIS only**, and the updater runs NSIS. A tag build still makes MSI and NSIS. One consequence: pull requests no longer exercise the MSI packaging. Only a tag does.

**Signing.** `tauri.updater.conf.json` turns on `createUpdaterArtifacts` for every build that isn't a tag. Who signs depends on the run:

| Run | Key that signs the update bundle | Published |
|---|---|---|
| push to `main`, with the secret | `TAURI_SIGNING_PRIVATE_KEY` (Sean's, ID 303459ECC0739D7D) | yes, by `main-channel` |
| push to `main`, without the secret | a throwaway key made in the job | no: `main-channel` says so and stops |
| pull request, `workflow_dispatch` | a throwaway key made in the job | never |
| `v*` tag | none: no update bundle, as before | draft release only, as before |

The bundler checks each signature against `plugins.updater.pubkey` right after signing. So the key step passes a config naming the key the bundle must match: `updater.pub` for the release key. A secret that doesn't match the app therefore fails the build before anything is published.

**How a pull request is kept from publishing.** There are three locks, and any one is enough:
1. `meta` sets `publish=true` only for `github.event_name == push` with `GITHUB_REF == refs/heads/main`.
2. `main-channel`'s `if:` requires `publish == 'true' && github.event_name == 'push' && github.ref == 'refs/heads/main'`.
3. The secret reaches a step only through `needs.meta.outputs.publish == 'true' && secrets.… || ''`, so a pull request's build never sees it, even though same-repository PRs can read secrets.

`main-channel` is the only job with `contents: write` outside the existing tag-only `publish` job.

**`main-channel`** runs only on a push to `main` with the secret, under `concurrency: main-build-publish`. The workflow's own group also queues `main` runs one behind another.
1. Signs the deb (the Linux app's link).
2. Checks every `.sig` with `minisign -V` against `updater.pub`.
3. Writes `latest.json` with `write-manifest.py`. The script refuses:
   - a version outside the scheme;
   - a base URL off `https://github.com`;
   - a missing or garbled signature, or one not bound to this version;
   - a version not newer than the published manifest's.
4. Moves the `main-build` tag to the commit (or creates the release on the first run), always as a pre-release with `--latest=false`.
5. Uploads the bundles, then `latest.json` last, then deletes the old files.

The manifest names:
- `darwin-aarch64`, `darwin-x86_64` (+ `-app`): one universal bundle;
- `windows-x86_64` (+ `-nsis`): the setup exe;
- `linux-x86_64` (+ `-deb`): the deb, which the app only links to.

**Endpoint:** `https://github.com/snds/lockdown/releases/download/main-build/latest.json`. The stable channel is designed for but not built (`Channel::Stable` has no endpoint yet, and a tag build says updates come later). When it is built, it is expected to read `releases/latest/download/latest.json` from published `v*` releases.

## 3. macOS: no keychain dialog after an update

**What happens.** `keyring` 3.6 (`apple-native`) writes the session token as a generic password in the login keychain through the file-based keychain API (`SecKeychainAddGenericPassword` via `security-framework`). That item's access list trusts the app that created it, named by its designated requirement.
- **Ad hoc (D450):** the requirement is the code hash, which changes with every build. The next build is not on the list, and reading the token makes macOS show "Lockdown wants to use your confidential information stored in 'design.snds.lockdown' in your keychain". Writing a new token into the existing item asks too.
- **Developer ID:** the requirement names the team and the identifier, which every build shares, so nothing is asked.
- **Windows Credential Manager and the Secret Service:** these have no per-program list, so an update reads the token as before.

**The options, weighed:**

| Option | Dialog after an update | Needs someone to act | Weakness |
|---|---|---|---|
| A stable signing identity held by CI (a self-signed code-signing certificate) | none while the certificate lasts | Sean makes the certificate and adds two secrets; a lapse or a change of certificate brings back one dialog per item | a self-signed identity proves nothing to Gatekeeper; it is D493 in miniature, without Apple |
| Keep the token out of the keychain in ad-hoc builds, in `sync.cbor` as the CLI keeps it | none | nothing | the token is protected like the device key file beside it (user-only, D87), not by the keychain's per-app list |
| Read with the UI suppressed, treat a refusal as "sign in again" | none | the person signs in with the master password after every update | a sign-in on every update; writing the new token would ask again |

**Chosen:** the second option, with the third as a safety net, and the first left for D493.
- A Mac build keeps the token in the keychain only when its own signature is a valid one from an Apple-issued certificate (it names a team), read at launch through `lockdown_codesign::macos::own`.
- Any other build installs `keychain::Keychain` in read-only mode. `SecretStore::keeps_values` (new, default `true`) is `false`, so `SyncConfig::save` leaves the token in `sync.cbor`, user-only, exactly as with no store and as the CLI does on every platform.
- The keychain is still read, for a token an older build left. That happens once, and the token then moves into the file on its next save.
- **Every** keychain read on a Mac now runs with `SecKeychainSetUserInteractionAllowed(false)`. A read macOS would have asked about comes back refused, and is treated as no token: the person signs in with the master password, and is never shown a keychain dialog.
- Nothing is deleted from the keychain in read-only mode, because deleting asks too. The developer reset finds those items (D482).

Sealing the token under the device key was considered and not done. On a Mac today the device key is a user-only file beside `sync.cbor` (D87), so a sealed token would be exactly as reachable, and the CLI would need a new format.

**One-time cost for an existing install.** A Mac that signed in with a pre-slice ad-hoc build has its token in the keychain, written by that build's identity. The first build with this slice reads it with dialogs off, is refused, and asks for the master password once (sign in again). From then on the token is in `sync.cbor`. That build had to be installed by hand anyway, because builds before it have no updater.

**What is proven, and what isn't.** The `keychain-macos` job (`packaging/update/keychain-check.sh`) runs on `macos-latest`, in a keychain made for the job and set as the default. It builds the same probe twice (`examples/keychain-probe.rs`, which goes through the app's own `Keychain` store), so the two binaries' code hashes differ:
1. Build A, signed ad hoc, writes an item and reads it back with dialogs off. Build B, a different ad-hoc build, reading the same item with dialogs off, is **refused**. With dialogs on, a person would have been asked.
2. Both ad-hoc builds report `read-only` mode, and B's read through the app's store says `absent`, with no dialog and no hang.
3. The same two builds signed with **one** throwaway self-signed certificate: B reads A's item with dialogs off. A stable identity is enough.

On this PR the job passed (run of 6562c53):
- The two ad-hoc code hashes were `73d8062d…` and `be8d36cc…`.
- A wrote and read its item back.
- B, with dialogs off, was refused with `-25293` (`errSecAuthFailed`): the read macOS turns into the dialog.
- Both builds ran read-only, and B's own store read said `absent`.
- With one shared certificate (designated requirement `identifier "design.snds.lockdown" and certificate root = H"…"`), B read A's item.

That job's result on this PR is in §5. It does **not** run the whole app across a real update with a signed-in account on a Mac, and it can't show a dialog on screen (the runner has no one to answer it). What it shows is the keychain's answer that macOS would have turned into a dialog. The full answer is still the Developer ID (D493), under which the token goes back into the keychain with nothing asked across updates. **Flagged for Sean.**

## 4. The test-only endpoint and key

The `update-test` Cargo feature (`["shell", "tauri/test"]`) adds:
- `TestSource::from_env` (`LOCKDOWN_UPDATE_TEST_ENDPOINT`, `LOCKDOWN_UPDATE_TEST_PUBKEY`), which replaces the channel's endpoint, key and hosts, and allows plain `http` on the loopback address only;
- the end-to-end hooks: a report file, restart when ready, and opening the run's throwaway vault with its password. Each setting comes from `LOCKDOWN_UPDATE_TEST_<NAME>`, or else from `update-test.json` in the data directory. On Windows the file is the only way the run's settings reach build B, because Tauri's NSIS installer starts the new build through Explorer (`nsis_tauri_utils::RunAsUser`), with Explorer's environment;
- Tauri's mock runtime for the plugin's own signature tests;
- the marker `lockdown-update-test-3b9d`.

It is never in `tauri.conf.json`'s features, so development builds, dry runs and main builds don't have it. Four things keep it out of a release:
1. `build.rs` panics if it is on with `LOCKDOWN_DEVTOOLS=off`. `desktop-bare` proves this, beside the same check for `dev-reset`.
2. `packaging/devtools/check.sh` has the marker as a needle, so the release's `absent` check fails a tag build that carries it.
3. `e2e-build.sh` greps the marker in an `update-test` build and runs `check.sh present` on it, which shows the needle is real.
4. Without the feature, `TestSource::from_env` is `None` and is tested to be.

## 5. How to run and test

| What | Command | Where |
|---|---|---|
| The rules | `cargo test -p lockdown-desktop --features shell updater` | every desktop job |
| The plugin's own signature check | `cargo test -p lockdown-desktop --features update-test --lib updater` | `updater` job (with `LOCKDOWN_UPDATE_TEST_SIGNED`, a bundle the job signed) |
| The scripts | `python3 packaging/update/test_update_scripts.py` | `updater` job |
| The views | `pnpm vitest run src/features/updates` (in `apps/desktop`) | `desktop` job |
| End to end | `packaging/update/e2e-build.sh <dir>` then `e2e-run.sh <dir>` | `update-e2e` on macOS and Windows |
| The keychain | `packaging/update/keychain-check.sh <out> <probe a> <probe b>` | `keychain-macos` |

**End to end.** `e2e-build.sh` builds A (`0.1.0-main.20260101000000`) and B (`…0102…`), debug, with `update-test`, signed by a key made in the job. It also builds the CLI. Then `e2e-run.sh`:
1. The CLI makes a throwaway vault holding one item in the account's default places (as an installed app has it), and the run writes `update-test.json` beside it.
2. A `latest.json` naming B is served on `127.0.0.1:8765`.
3. A is installed: copied to a folder the account owns on macOS; the NSIS installer run with `/S` (per user) on Windows.
4. A starts, opens the vault, finds B, downloads it, checks it, locks, installs and restarts.
5. The run passes when B reports it started and opened the vault with its one item. The CLI then lists the item again.

**Run locally on Linux in this slice:**
- A debug `update-test` build signed with a throwaway key: the bundler signed the deb.
- That build run under `xvfb-run` against a manifest served on 127.0.0.1. It reported `Checking`, then `Available` for B (Linux links, doesn't install).
- The request it sent, captured header by header, is the one in §1.

**Results on PR #56.** These are filled in from the CI run of the final commit; see the PR body. A platform not listed as passed was not proven.

## 6. Windows and Linux

- **Windows.** The NSIS installer runs with `/P /UPDATE /R`: passive (a progress bar, nothing to click), update mode, relaunch. Tauri's NSIS default is a per-user install under `%LOCALAPPDATA%`, which needs no administrator, so the update is silent. An install anywhere else (Program Files, an MSI from a tagged release) would need elevation. The NSIS update would also install a second, per-user copy beside it. So `windows_per_user` decides: outside `%LOCALAPPDATA%` the app only links to the release (`Install::MachineWide`). A quit-time install runs the same installer without `/R`.
- **Linux.** deb, rpm and Flatpak installs belong to the package manager, so the app never downloads there. A check still works. When a newer build exists, Settings says so in one line and offers "Get it from GitHub", which opens `main-build`. The manifest's Linux entry is the signed deb. **Gap:** self-updating Linux (an AppImage, a Flatpak remote, or apt/dnf repositories).
- **macOS.** The plugin swaps the bundle in place. Its fallback for a folder the account can't write asks for an administrator's password, so `mac_can_replace` checks the folder and the bundle first, and otherwise the app only links (`Install::NotWritable`). No quarantine flag is set on a bundle the updater downloads, so Gatekeeper raises nothing on relaunch.

## 7. What people see (every new string, for Sean)

| Where | Words |
|---|---|
| Settings section | **About** |
| Version line | "Lockdown {version} · main channel (every merged change)" / "… · stable channel" |
| Button | **Check for updates** |
| Button (ready) | **Restart to update** |
| Button (Linux, machine-wide, unwritable) | **Get it from GitHub** |
| Switch | "Check for updates automatically" |
| Status: idle, switch on | "Lockdown checks for updates when it opens and every six hours." |
| Status: idle, switch off | "Automatic checks are off. Check whenever you like." |
| Status: checking | "Checking for updates…" |
| Status: up to date | "Lockdown is up to date." |
| Status: downloading | "Downloading version {v}… {n}%" |
| Status: ready | "Version {v} is ready. Restart to update, or it installs when you quit." |
| Under it (ready) | "Restarting locks Lockdown. Unlock it with your master password as usual." |
| Status: available | "Version {v} is out." + one of: "Lockdown can't update itself when installed from a Linux package." · "Lockdown is installed for everyone on this computer, so it can't update itself." · "Lockdown can't update itself from a folder your account can't change." |
| Status: off | "Updates are off in this build: it was made without the update key." · "…: its update key isn't valid." · "…: automatic updates for tagged releases come later." |
| Status: failed | "The update was refused: its signature didn't match Lockdown's key." · "Lockdown couldn't reach GitHub. Check your connection and try again." · "The newest build has nothing for this computer yet." · "The update's description couldn't be read." · "Updates come only over https." · "The update was refused: updates don't come from {host}." · "Lockdown didn't check for updates: {the demo's reason}" · "The update didn't work: {detail}." |
| Sidebar row (ready) | **Restart to update** (spoken: "Restart to update to version {v}. Lockdown locks first.") |
| macOS app menu | "Check for Updates…" |
| Toast (a check someone asked for, outside Settings) | the status line above |

## 8. Gaps

1. **The CLI has no `lockdown update`** (D546's gap 3 stands). It is planned to follow the same manifest.
2. **Self-updating Linux.** The app links to the deb (§6).
3. **The stable channel** (`v*` releases) is designed for, not built.
4. **Developer ID (D493).** Until it exists, a Mac build keeps the session token in `sync.cbor`, and Gatekeeper's "Open Anyway" still applies to the first manual install.
5. **MSI in dry runs.** Pull requests no longer build the MSI (§2). Only a tag does.
6. **A machine-wide Windows install** (Program Files) only links. A stable-channel MSI updater would need elevation, which Lockdown never asks for.
7. **Release notes** in the manifest are one line naming the commit. There is no changelog.
8. **The locked screen** shows no "Restart to update". An update there installs on quit, or from Settings after unlocking.

**Unicron host-contract gaps:** none. The section is built from `Button`, `Field`, `FieldLabel`, `Switch` and `Icon` as they stand.

## 9. Gates run

Listed in the PR body with their final output.

## 10. Proposed decisions

| # | Decision | Status |
|---|---|---|
| D680 | **The desktop app updates itself from the rolling "main" pre-release.** `tauri-plugin-updater` 2.14.1 runs behind the `shell` feature and is driven from Rust only, with four commands and one event and no webview permission. It checks at launch and every six hours while "Check for updates automatically" is on (`Settings::update_auto_check`, default on, a D62 addition). It also checks whenever someone presses Check for updates or the macOS menu item. A newer build downloads in the background and becomes "Restart to update". An update not taken installs on quit. Restarting locks the vault as Lock does. Checks are held in the demo's confined home. Plain version: Lockdown keeps itself current, and you choose when it restarts. | Proposed; **flagged for Sean** (the wording, §7) |
| D681 | **What an update must pass.** The manifest and the bundle come only over https from `github.com` and GitHub's release-asset hosts, with every redirect checked. The version must be strictly newer in semver. The bundle's minisign signature must check against the public key built in from `updater.pub` (ID 303459ECC0739D7D), and the version signed in its trusted comment must equal the manifest's (`requireSignedVersion`), so an older bundle can't be passed off as newer. A check sends no version, device or account, only the plugin's user agent. Plain version: only Sean's signed builds, newer than yours and fetched from GitHub, ever install. | Proposed; audit item |
| D682 | **The version scheme.** Every build but a tag's is `<x.y.z>-main.<commit time, UTC, YYYYMMDDHHMMSS>`, stamped into `Cargo.toml`, the path dependencies, `Cargo.lock`, `tauri.conf.json` and both `package.json` files. The extension and MSIX keep `x.y.z`. A tag keeps its plain version. After a `v` tag, `Cargo.toml` moves to the next version so main builds sort after it. Plain version: each merge gets a higher number, worked out from the commit itself. | Proposed |
| D683 | **Windows updates through NSIS, per user, passive.** MSI can't take the pre-release version, so main builds and dry runs make NSIS only, and tag builds keep MSI and NSIS. The updater runs the installer with `/P /UPDATE /R`. An install outside `%LOCALAPPDATA%` only links to the release, so no update ever asks for an administrator. Plain version: on Windows an update shows a progress bar and reopens Lockdown, with nothing to click. | Proposed |
| D684 | **On a Mac without a Developer ID, the session token stays out of the keychain.** A build whose own signature isn't a valid Apple-issued one keeps the token in `sync.cbor` (user-only, as the CLI does). It reads the keychain only for a token an older build left. Every Mac keychain read runs with dialogs off, and a refusal means "sign in again". `SecretStore::keeps_values` is added (default true). Proven on `macos-latest` as far as §3 says. An existing ad-hoc install signs in once with the master password after its first updatable build. Plain version: updating never makes macOS ask about the keychain, and until Lockdown is signed by Apple's program your sign-in is kept the way the command line keeps it. | Proposed; **flagged for Sean** (the full answer is D493's Developer ID); audit item |
| D685 | **The main channel is published by one job, on a push to `main`, only with the secret.** `main-channel` (`contents: write`, its own concurrency group):<br>• checks every signature against `updater.pub` with `minisign`;<br>• writes `latest.json` (never older than the published one);<br>• moves the `main-build` tag, uploads the new files with `latest.json` last, and deletes the old ones;<br>• always marks the release as a pre-release, never "latest".<br>A pull request can't publish: `meta.publish`, the job's own `if`, and the secret passed only when publishing. Without the secret it stops cleanly. `v*` releases are unchanged drafts. Plain version: every merge becomes the newest test build on one GitHub page, and nothing else can put a build there. | Proposed; audit item |
| D686 | **Dry runs sign with a throwaway key.** Pull requests, `workflow_dispatch` and a `main` push without the secret sign the update bundles with a key made in the job. The bundler checks each signature against the key it should match (the app's own key when publishing). CI's `updater` job signs a bundle with a throwaway key and runs the updater's own check on it, which must accept it and refuse it changed or under another key. Plain version: every change proves the signing works without ever touching Sean's key. | Proposed |
| D687 | **The test-only endpoint and key live in the `update-test` feature.** It is off in every shipped build, build.rs refuses it in a release, and the developer-tools check looks for its marker. It lets the end-to-end job install build A, serve build B from this computer, and see A update itself and B open the vault (macOS and Windows). Plain version: the tests can point the app at a fake update server, and no real build can. | Proposed; audit item |
| D688 | **Where an app can't replace itself, it links.** Linux packages, a machine-wide Windows install, and a Mac app in a folder the account can't write to show "Version … is out" with "Get it from GitHub" and never download. Self-updating Linux is a recorded gap. Plain version: where Lockdown can't update itself without a password or the package manager, it tells you and links to the build. | Proposed; **flagged for Sean** (the wording) |
| D689 | **Recorded gaps:**<br>• `lockdown update` for the CLI (D546's gap 3);<br>• the stable channel;<br>• self-updating Linux;<br>• MSI in dry runs;<br>• release notes;<br>• "Restart to update" on the locked screen.<br>Plain version: what this slice leaves for later, written down. | Proposed; recorded gap |
