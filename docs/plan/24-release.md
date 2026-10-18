# Release engineering: platform builds, packages, signed and attested releases, SBOMs, supply chain, reproducible builds

_Status: built, 2026-10-18, under D122 ("desktop and web first"). Nothing in this slice publishes, signs with a real key, pushes a tag, makes a GitHub release or uploads anywhere. The release workflow runs for real only on a `v*` tag push, and every part of it that needs no secret also runs on each pull request as a dry run. Companion to `16-browser-extension.md` (the channel the Windows pipe completes), `20-self-host-server.md` (the image) and the user guide [`docs/verify.md`](../verify.md)._

## For future agent

- **TL;DR:** Windows joins the Rust matrix, and gets a desktop shell job and the browser channel's named pipe (`crates/pipe`). It is user-only and local-only, and each end checks the other by process id, user, executable path and SHA-256, the Unix socket's rules. `.github/workflows/release.yml` builds every artifact on tag pushes and, as a dry run, on pull requests:
  - deb and rpm, installed and smoke-tested in clean Ubuntu, Debian and Fedora containers;
  - a Flatpak;
  - a universal macOS app and dmg;
  - MSI and NSIS installers and an unsigned MSIX;
  - the extension's store zips;
  - the server, the CLI and the server image;
  - CycloneDX SBOMs and `SHA256SUMS`.

  On a tag only, it also adds GitHub build provenance (keyless Sigstore) and a **draft** release. `supply-chain/` holds cargo-vet with audits imported from six public sets. `.github/workflows/reproducible.yml` builds the extension zips, the server and CLI binaries and the server image on two runner images and requires the same bytes.
- **Read with:** `.github/workflows/{ci,release,reproducible}.yml`, `crates/pipe/`, `packaging/` (Linux smoke test, Flatpak, MSIX, reproducible scripts), `apps/desktop/src-tauri/tauri.bundle.conf.json` and `apps/desktop/scripts/stage-host.mjs`, `extension/scripts/pack.mjs`, `supply-chain/`, `docs/verify.md`.
- **Do not:**
  - invent or hard-code a secret: every signing step reads a named secret and is skipped without it;
  - let a pull request run attest, sign or release;
  - add a crate without `cargo vet` passing: the CI job fails until it is audited or exempted;
  - call `DisconnectNamedPipe` on a pipe the other end may still be reading (it throws away unread data);
  - put the `key` back into the Chromium store package.

## 1. What was built, per row

| Row | State | What is real | What is missing |
|---|---|---|---|
| `windows-app` (CI part) | done (CI part) | `windows-latest` in the Rust matrix: fmt, clippy and every workspace test on Windows. A `desktop shell (windows)` job runs clippy and the tests of `lockdown-desktop --features shell`. The named-pipe transport is used by the native host and the desktop endpoint, tested on the Windows runner. The release workflow builds MSI and NSIS installers and packs the MSIX layout, unsigned, with `makeappx` (§2). | Authenticode signing (needs the certificate, §3), the Store submission, the passkey plug-in and auto-type (OS-level, D122). |
| `macos-app` (CI part) | done (CI part) | A universal (arm64 + x86_64) `Lockdown.app` and dmg from `tauri build --target universal-apple-darwin`. The browser host is built for both and joined with `lipo`. A check confirms both binaries carry both architectures. Signing, notarisation and the checks on them (`codesign --verify`, `stapler validate`, `spctl`) are written and run only when the Apple secrets exist. | The Developer ID certificate and notarisation account (Sean's), and the native AutoFill and passkey extensions (OS-level, D122). |
| `linux-app` (packaging part) | done (packaging part) | deb and rpm from Tauri's bundler on Ubuntu 22.04, with the browser host beside the app in `/usr/bin`. Each is installed in a clean `ubuntu:24.04`, `debian:12` and `fedora:41` container and run through `packaging/linux/smoke.sh`: both programs, every shared library resolved, the desktop entry, and the host answering `refused` and `no_app` from the package. On Ubuntu the window must also stay up for 20 s under a virtual display. A Flatpak (GNOME 47 runtime) is built from the same binaries, installed from its bundle, and its host is run. | The browser connection from inside the Flatpak (§7), a Flathub submission (needs vendored crates), AppImage, and the SSH agent and TPM unlock (OS-level). |
| `signed-releases` | partial | `release.yml`: every artifact, CycloneDX SBOMs for every Rust crate and each JS app, `SHA256SUMS`. On tag pushes only: GitHub artifact attestations (build provenance, keyless Sigstore, no stored secret) for every file, and a draft release. Signing on macOS and Windows runs where the secrets exist. `cargo-vet` gates CI (§4). The user guide for checking a download is `docs/verify.md`. | The certificates, a first tag, and moving cargo-vet's 608 exemptions to audits over time. |
| `reproducible-builds` | partial | Reproducible: the extension's Chromium and Firefox zips, the `lockdown-server` and `lockdown` binaries, and the server image (OCI archive, with the web vault). `reproducible.yml` builds each on `ubuntu-24.04` and `ubuntu-22.04` and compares SHA-256. It also checks that the server inside the image is the published binary. | The desktop bundles (§5), Android (no app yet, D122), and an independent rebuilder. |
| `extension-coverage` (packaging part) | done (packaging part) | Store-ready packages from CI: `lockdown-chromium-<v>.zip` (without the development `key`) and `lockdown-firefox-<v>.zip`, plus the source archive addons.mozilla.org asks for. A Firefox Playwright run (`test/firefox.spec.ts`) installs `dist/firefox` as a temporary add-on over Firefox's remote protocol. Its inline menu then reaches the real desktop app through the real host and answers "not paired". Pairing and a fill are not driven in Firefox (§6). On Windows the extension reaches the app over the named pipe. | Pairing and filling driven in Firefox (Playwright can't open the popup there, §6), the store listings and the store ids (§6), the popup on unicron, and Safari (OS-level). |

## 2. The packages

| Platform | Files | Built by | Tested by |
|---|---|---|---|
| Linux | `lockdown_<v>_amd64.deb`, `lockdown-<v>-1.x86_64.rpm` | `tauri build --bundles deb,rpm --config src-tauri/tauri.bundle.conf.json` on ubuntu-22.04 (glibc 2.35, so older systems run it) | installed with `apt-get` / `dnf` in clean containers, then `packaging/linux/smoke.sh`; the window under `xvfb-run` on Ubuntu |
| Linux | `lockdown-<v>.flatpak` | `packaging/flatpak/design.snds.lockdown.yml` from the release binaries, in the flatpak-builder container | the bundle installed, its host run through `flatpak run --command` |
| macOS | `Lockdown_<v>_universal.dmg`, `Lockdown_<v>_universal.app.zip` | `tauri build --target universal-apple-darwin --bundles app,dmg` | `lipo -archs` on the app and the host; the signature checks when signed |
| Windows | `Lockdown_<v>_x64_en-US.msi`, `Lockdown_<v>_x64-setup.exe`, `Lockdown_<v>_x64.msix` (unsigned) | `tauri build --bundles msi,nsis`; `packaging/msix/build-layout.ps1` lays out and packs the MSIX with `makeappx` | `makeappx` validates the manifest and layout; the pipe and the shell by the CI jobs |
| Browsers | `lockdown-chromium-<v>.zip`, `lockdown-firefox-<v>.zip`, `lockdown-extension-source-<v>.zip` | `pnpm build:wasm && pnpm build && pnpm run package`; `git archive` for the sources | `test/package.spec.ts`; the Chromium and Firefox suites on the same build |
| Server | `lockdown-<v>-x86_64-linux.tar.gz` (server and CLI), `lockdown-server-<v>-image.oci.tar` | `packaging/repro/build-server.sh`, `packaging/repro/build-image.sh` | the CI `container` job's smoke test (same Dockerfile); `reproducible.yml` |
| All | `lockdown-<v>-sbom.tar.gz`, `SHA256SUMS` | `cargo cyclonedx --all-features`, syft from each pnpm lock | every SBOM parses as CycloneDX, and each shipped program's lists components |

**The browser host in every desktop package.** The app looks for `lockdown-native-host` beside its own executable (`EndpointConfig::for_app`). Tauri's `externalBin` puts it there: `/usr/bin` in the deb and rpm, `Contents/MacOS` in the app, and the install folder on Windows. `scripts/stage-host.mjs` builds the host and stages it as `src-tauri/binaries/lockdown-native-host-<target triple>`, the name the bundler wants. The `externalBin` line lives in `tauri.bundle.conf.json`, an overlay passed with `--config` only when packaging. `tauri-build` checks that every `externalBin` file exists, so with the line in `tauri.conf.json` a plain `cargo build --features shell` (CI's desktop job, `tauri dev`, the other builders) would fail without a staged host.

**MSIX.** The manifest (`packaging/msix/AppxManifest.xml`) turns off registry and file-system write virtualization. This needs the restricted `unvirtualizedResources` capability. Without it, the app's `HKCU\Software\…\NativeMessagingHosts` value and the manifest files under `%LOCALAPPDATA%` would go into the package's private hive, which browsers cannot see. Publisher is `CN=Sean Sands` until `WINDOWS_MSIX_PUBLISHER` names the signing certificate's subject.

## 3. The release workflow (`.github/workflows/release.yml`)

```text
 meta ──┬─ linux ──┬─ linux-install (ubuntu:24.04 · debian:12 · fedora:41)
        │          └─ flatpak
        ├─ macos (universal; signs and notarises only with the secrets)
        ├─ windows (msi · nsis · msix; signs only with the secret)
        ├─ extension (store zips, source archive)
        ├─ server (binaries, image)
        └─ sbom
                                       └──▶ publish: SHA256SUMS ─▶ [tag only] attest-build-provenance ─▶ [tag only] gh release create --draft
```

- **Triggers.** A `v*` tag push is real. On real runs, `meta` refuses a tag whose version differs from `Cargo.toml`, `tauri.conf.json` or the extension's manifest. `workflow_dispatch` and `pull_request` are dry runs: same builds, no attestation, no release. `permissions` default to `contents: read`. Only `publish` gets `contents: write` (the draft), `id-token: write` (the Sigstore certificate) and `attestations: write`.
- **Secrets** (none exists yet; each step that needs one is skipped without it):

| Secret | Used for |
|---|---|
| `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD` | the Developer ID Application certificate (base64 `.p12`), imported into a throwaway keychain |
| `APPLE_SIGNING_IDENTITY` | the identity Tauri signs the app and the host with (`Developer ID Application: … (TEAMID)`) |
| `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` | notarisation (an app-specific password) |
| `WINDOWS_CERTIFICATE`, `WINDOWS_CERTIFICATE_PASSWORD` | the Authenticode certificate (base64 `.pfx`); Tauri signs the exe and installers by thumbprint, signtool signs the MSIX |
| `WINDOWS_MSIX_PUBLISHER` | the MSIX `Publisher`, which must equal the certificate's subject |

  Nothing else is needed. Build provenance uses GitHub's OIDC token and Sigstore's public instance, and the draft release uses the workflow's own token.
- **What a user checks:** `docs/verify.md`. The steps are `sha256sum --check SHA256SUMS`, then `gh attestation verify <file> --repo snds/lockdown`, then, for the reproducible files, a rebuild.

## 4. Supply chain: cargo-vet (`supply-chain/`)

- `cargo vet init`, then audits imported from **bytecode-alliance, google, mozilla, isrg, zcash and embark-studios** (the public sets in cargo-vet's registry), then `cargo vet prune`. Result: **104 crates fully audited, 3 partially, 608 exempted**. The exemptions are honest. They are crates nobody in those sets has audited at the version we use, recorded under `[[exemptions.<crate>]]` as `safe-to-deploy`, and they are not audits.
- `imports.lock` pins the imported audits. CI runs `cargo vet --locked` (the `vet` job), which reads only that file and fetches nothing.
- **What this asks of every later change:** a new or updated third-party crate fails `vet` until someone audits it (`cargo vet certify`) or records an exemption (`cargo vet regenerate exemptions`). `cargo vet suggest` lists the cheapest audits to do next. A periodic `cargo vet regenerate imports` picks up new public audits and shrinks the exemption list.
- cargo-deny stays as it was (licences, advisories, sources). The two do different jobs.

## 5. Reproducible builds

| Artifact | How | Checked |
|---|---|---|
| Extension zips | `scripts/pack.mjs`: entries sorted by byte order, every time from `SOURCE_DATE_EPOCH` (the commit's time), one mode, no extra fields, entries **stored**, not deflated. The bytes then depend only on the files and not on the zlib a given Node ships. The wasm is built with `--remap-path-prefix` for the checkout and the cargo home; esbuild's output is deterministic. | `reproducible.yml` (two runners), `test/package.spec.ts` (two packings, one byte string) |
| `lockdown-server`, `lockdown` | `packaging/repro/build-server.sh`: built inside the Rust image the Dockerfile uses, pinned by digest, at the same paths (`/src`, `/src/target`, `/usr/local/cargo`), with `--remap-path-prefix`, `--locked`, the release profile (`codegen-units = 1`). The server is built on its own, as in the Dockerfile, so feature unification with the CLI cannot change it. | two runners; and the server inside the image hashes the same |
| Server image | `packaging/repro/build-image.sh`: `docker buildx build --build-arg SOURCE_DATE_EPOCH=… --output type=oci,…,rewrite-timestamp=true --provenance=false --sbom=false`. Both base images are pinned by digest in the Dockerfile. Provenance and SBOM attestations are off because they record the build's own time and host; the release workflow attests the archive instead. | two runners |

**Not reproducible yet, and why:**
- **Desktop bundles.** Tauri patches the main executable with the bundle type (`--no-binary-patching` exists, but the updater needs the patch). The deb's `ar` and `tar` members and the rpm's header carry build times. The dmg is an HFS+ image with its own timestamps and layout. macOS and Windows signatures carry signing times and differ by design. The MSI's package code is random per build. The binaries inside are likely close, since they come from the same toolchain and flags, but nobody has checked yet.
- **The web vault on its own** is not published as a file. It is inside the image, which is checked.
- **A rebuild at a different path** is expected to match, because paths are remapped, but CI builds at one path on both runners.

## 6. Store packages

- **Chromium (Chrome Web Store, Edge Add-ons):** `lockdown-chromium-<v>.zip` is `dist/chromium` with the manifest's `key` removed. The stores assign their own id, so the store build's id must be added to `CHROMIUM_IDS` in `crates/native-messaging` (and to the host manifest `allowed_origins`) once the listing exists. Until then the desktop answers only the development id.
- **Firefox (addons.mozilla.org):** `lockdown-firefox-<v>.zip` keeps `lockdown@snds.design`, so the id stays the same. AMO asks for sources with a bundled add-on, so `lockdown-extension-source-<v>.zip` is a `git archive` of what building the add-on reads.
- **The Firefox test run.** Playwright cannot load an add-on into Firefox with a launch option, so `test/firefox.spec.ts` uses the remote debugging protocol, as `web-ext` does:
  1. It starts Firefox with `-start-debugger-server`.
  2. It asks the root actor for `addonsActor`.
  3. It sends `installTemporaryAddon` with `dist/firefox`.

  The add-on's origin is fixed with `extensions.webextensions.uuids`. Firefox finds the host manifest in `$HOME/.mozilla/native-messaging-hosts`, so `HOME` is set to a scratch folder. What it proves: on a sign-in page, the content script opens the menu frame. The event page asks the app over native messaging, the host and the app check each other, and the menu says "Pair this browser". **What it does not drive, and why:** pairing starts in the popup, and Playwright's Firefox does not load a `moz-extension:` page as a page it controls. Three routes were tried:
  1. `page.goto` hangs until the test times out.
  2. A tab the add-on opens with `browser.tabs.create`, evaluated in its background over the remote protocol, opens but never shows up as a Playwright page.
  3. The descriptor's `getTarget` and `connect` requests don't exist in this Firefox; the watcher route does reach the background, but its tab is the one in route 2.

  So pairing and a fill run in Chromium only, on the same code. Driving them in Firefox needs either a Playwright release that controls extension pages in Firefox or a pairing start reachable from a web page (a design change, not made here). The suite is skipped where Playwright's Firefox is not installed. CI installs it and sets `LOCKDOWN_REQUIRE_FIREFOX=1`, which turns a skip into a failure.

## 7. The Windows named pipe (`crates/pipe`)

```text
lockdown-native-host ── \\.\pipe\snds-lockdown-browser-<16 hex of SHA-256(data dir)> ──▶ desktop app
  CreateFileW, SECURITY_SQOS_PRESENT|SECURITY_IDENTIFICATION        CreateNamedPipeW, FILE_FLAG_FIRST_PIPE_INSTANCE,
  GetNamedPipeServerProcessId → check the app                       D:P(A;;GA;;;<user SID>), PIPE_REJECT_REMOTE_CLIENTS,
                                                                    GetNamedPipeClientProcessId → check the host
```

- **Same contract as the socket.**
  - The desktop names the pipe in the endpoint file's `socket` field. The field's shape is unchanged; it now holds a path or a pipe name.
  - Frames are the identity crate's 4-byte big-endian length, at most 64 KiB.
  - `PipeTransport` and `PipeChannel` implement the identity crate's `PeerTransport` and `PeerChannel`, so `relay` and `serve` are unchanged.
  - The host refuses an endpoint file whose `socket` is not a Lockdown pipe name.
- **Who may connect.**
  - The DACL is protected and has one entry, generic-all for this user. Other users, including administrators, are not granted access. The default integrity label keeps low-integrity processes (a sandboxed renderer) from writing.
  - Remote clients are refused.
  - The first instance is made with `FILE_FLAG_FIRST_PIPE_INSTANCE`, so the app will not start on a name someone else already holds. It says "another program already holds the pipe" and the endpoint stays idle.
  - The client connects with identification-only impersonation, so the app learns who it is but cannot act as it.
- **The peer check** (`check.rs`, pure, tested on every platform):
  1. Take the peer's process id from the pipe.
  2. Open the process with `PROCESS_QUERY_LIMITED_INFORMATION` and hold it open for the channel's life, so the id cannot be reused.
  3. Compare the token's user SID with ours (`EqualSid`).
  4. Read the image path with `QueryFullProcessImageNameW` and compare it case-insensitively, after canonicalising, with the allowed binary.
  5. Hash the file at that path with SHA-256.

  These are the identity crate's `ProcessIdentityVerifier` rules on Unix.
- **Host contract gap (identity crate).** `PeerIdentity` is `non_exhaustive` and has no Windows variant, so a pipe channel reports `PeerIdentity::Unknown`, which every identity-crate verifier refuses. The pipe's own view travels beside it (`PipeChannel::peer_process`), and `PipeVerifier::bind` makes a verifier for that one channel. When the identity crate grows a `Windows { pid, user_sid, exe }` variant, `bind` goes away. The code-signature verifiers (Authenticode here, `SecCode` on macOS) are still the identity crate's to build (D33). Until then a same-user program that can swap the installed host file between our path read and our hash is the known weakness, the same one Unix has with its path check.
- **Registering with the browsers.** `Settings → Browsers → Connect` now also writes the per-user registry value (`HKCU\Software\<browser>\NativeMessagingHosts\design.snds.lockdown`, default value = the manifest's path; `lockdown_pipe::registry`). Only the user's own hive is written, so nothing needs elevation.
- **Closing.** A server channel closes its handle and never calls `DisconnectNamedPipe`, which would throw away a last `refused` frame the host has not read. The Windows run caught this.
- **Flatpak.** Inside the sandbox the app's data directory and `/app/bin/lockdown-native-host` are not where a browser on the host can reach them. The Flatpak app runs, but connecting a browser to it is not supported yet. It needs a host-side wrapper that calls `flatpak run --command=lockdown-native-host` and a socket exported from the sandbox.

## 8. How it is tested

| Layer | Where | What |
|---|---|---|
| Peer check (every platform) | `crates/pipe/src/check.rs`, `lib.rs` | 6: the allowed binary by path and hash; another user, an unreadable image, another path, another hash, an unhashable file refused; a dotted spelling matches; a bound verifier checks only its own peer; Windows paths compare without case; one short pipe name per data directory |
| Pipe (Windows runner) | `crates/pipe/tests/pipe.rs`, `src/registry.rs` | 7: frames both ways up to the 64 KiB limit with each side seeing this process as its peer; wrong path and wrong hash refused on a real pipe; a second server on a held name refused; accept times out and a later client still gets through; two clients in turn; no server fails fast, silence times out, an oversized send is refused, a gone peer is reported; the registry value set, read and removed |
| Endpoint (Windows runner) | `apps/desktop/src-tauri/src/browser/tests.rs` | 2: the endpoint on the real pipe checks this binary and answers `ready` and `status`, and its endpoint file goes with it; an endpoint that allows another host refuses this one with a `refused` frame |
| Host | `bins/native-host` | the "no endpoint file → `no_app`" test now runs on every platform |
| Packages | `extension/test/package.spec.ts` | 2: two packings are the same bytes and the date follows `SOURCE_DATE_EPOCH`; each zip is the tested build, sorted and stored, the Chromium one without the key |
| Firefox | `extension/test/firefox.spec.ts` | 1: the add-on installed in Firefox, its menu reaching the app through the real host and endpoint and asking to pair |
| Installed packages | `release.yml` · `linux-install`, `flatpak` | `packaging/linux/smoke.sh` in three distributions, the window under a virtual display, the Flatpak's host |
| Reproducibility | `reproducible.yml` | the same SHA-256 for both zips, both binaries and the image on two runner images, and the image's server equals the binary |
| Supply chain | `ci.yml` · `vet` | `cargo vet --locked` |

**Also fixed on the way** (the new runners found them):
- `crates/app`'s list test looked for the CVC as `123"`, which any random item id ending in 123 also matches (about one run in 4096 per id). It now looks for the whole JSON string `"123"`.
- The server's TLS renewal test sends SIGHUP. On Windows, which has no signals, it now relies on the file check (`--tls-reload-secs 1`).
- CI's Rust tests run with `--no-fail-fast`, so one run shows every platform's failures.

Counts: the Rust workspace on Linux went from 613 to 619 tests (`lockdown-pipe`'s 6). Windows adds 9 more that run only there (the pipe's 6, the registry's 1, the endpoint's 2). The extension's Playwright suites went from 48 to 51 (2 package tests, 1 in Firefox).

Run locally:
- `cargo test -p lockdown-pipe` (the Windows tests need Windows).
- `cd extension && pnpm build:wasm && pnpm build && pnpm run package && pnpm exec playwright test test/package.spec.ts`.
- `cd apps/desktop && node scripts/stage-host.mjs && pnpm tauri build --debug --bundles deb,rpm --config src-tauri/tauri.bundle.conf.json`, then `dpkg-deb -x <deb> /tmp/x && LOCKDOWN_PREFIX=/tmp/x sh packaging/linux/smoke.sh`.
- `cargo vet --locked`.

## 9. Merging after this slice

`cargo vet --locked` is a blocking CI job. A branch that adds or bumps a third-party crate fails it until the crate is covered. The merge step:

1. Install cargo-vet 0.10.2: `cargo install cargo-vet --version 0.10.2 --locked`.
2. Run `cargo vet` to list the crates that are missing.
3. Run `cargo vet regenerate exemptions`, then commit `supply-chain/`.

Each new exemption shows up in the diff, so it is reviewed with the change that brought the crate. Where an audit already exists in one of the imported sets, `cargo vet regenerate imports` picks it up instead.

## 10. Gaps

- **Certificates and accounts:** Apple Developer ID and notarisation, a Windows code-signing certificate, the Chrome Web Store, Edge Add-ons and AMO listings, and the store ids added to `CHROMIUM_IDS`. These are Sean's to obtain.
- **Firefox:** pairing and filling driven by a test (§6).
- **Flatpak:** browser connection from the sandbox (§7) and a Flathub build from vendored sources.
- **Reproducibility:** the desktop bundles (§5) and an independent rebuilder.
- **cargo-vet:** 608 exemptions to turn into audits over time.
- **Windows:** Authenticode or MSIX package-family checks in place of path and hash (the identity crate's `CodeSignatureVerifier`), the Store, the passkey plug-in (D122).
- **Linux arm64 and Windows arm64** builds are not in the matrix.
- **unicron:** nothing in this slice touches the UI, so there are no unicron gaps.

## 11. Proposed decisions

For the coordinator to record. **Flagged** rows are user-visible and need Sean.

| ID | Decision | Owner |
|---|---|---|
| D275 | **The browser channel on Windows is a named pipe** (`crates/pipe`): `\\.\pipe\snds-lockdown-browser-<hash of the data dir>`, first instance only, a protected DACL for this user alone, local clients only, identification-only impersonation. Each end checks the other by process id, token user, image path and SHA-256, the Unix socket's rules, holding the process open. The identity crate's missing Windows `PeerIdentity` variant is recorded as a host-contract gap. Plain version: on Windows the browser reaches the Lockdown app through a private pipe, and each checks the other is the real program. | Accepted by default under D38; audit item |
| D276 | **Connect writes the browsers' per-user registry value on Windows** (`HKCU\Software\<browser>\NativeMessagingHosts\design.snds.lockdown`); nothing is written outside the user's own hive and nothing asks for elevation. Plain version: connecting a browser on Windows needs no administrator. | Accepted by default under D38 |
| D277 | **Every file is checked out byte for byte** (`.gitattributes`: `* -text`): no line-ending conversion on any platform, so Windows tests read the same data files and a Windows checkout rebuilds the same bytes. | Accepted by default under D38 |
| D278 | **The browser host ships inside every desktop package, beside the app** (Tauri `externalBin`, through the `tauri.bundle.conf.json` overlay used only when packaging), so a development build needs no staged host. Plain version: installing Lockdown installs the browser helper with it. | Accepted by default under D38 |
| D279 | **Linux packages:** deb and rpm built on Ubuntu 22.04, installed and smoke-tested in Ubuntu 24.04, Debian 12 and Fedora 41 containers; a Flatpak (`design.snds.lockdown`, GNOME 47 runtime) from the same binaries, in which the browser connection is not supported yet. Plain version: Lockdown installs on Ubuntu, Debian and Fedora and as a Flatpak; in the Flatpak the browser extension can't connect yet. | **Flagged for Sean** (the distributions and the Flatpak limit) |
| D280 | **macOS ships one universal app and dmg** (Apple silicon and Intel, macOS 11 or later, Hardened Runtime). It is unsigned until the Developer ID secrets exist; with them Tauri signs the app and host and notarises, and CI checks it with `codesign`, `stapler` and `spctl`. Plain version: one download for every Mac; until Lockdown has an Apple certificate, macOS warns before opening it. | **Flagged for Sean** (the Apple account and the unsigned interim) |
| D281 | **Windows ships MSI and NSIS installers** (the WebView2 bootstrapper, silent) **and an MSIX packed unsigned as a dry run** with registry and file-system write virtualization off (`unvirtualizedResources`) so browsers see the host registration; Authenticode signs when the certificate secret exists. Plain version: two installers for Windows now, a Store-style package later; unsigned until there is a certificate, so Windows warns. | **Flagged for Sean** (the certificate, the Store, the restricted capability) |
| D282 | **Releases come only from a `v*` tag, as a draft:** the workflow checks every manifest's version against the tag, builds everything, and makes a draft GitHub release that Sean publishes by hand; pull requests and manual runs are dry runs that sign, attest and publish nothing. Plain version: nothing reaches users until Sean presses publish. | **Flagged for Sean** |
| D283 | **Build provenance through GitHub artifact attestations** (keyless Sigstore, the run's OIDC identity, Rekor transparency log) for every released file on tag pushes, plus `SHA256SUMS`; users check with `sha256sum` and `gh attestation verify` (`docs/verify.md`). No long-lived signing key exists to steal. Plain version: every download can be traced to the exact code and build that made it. | **Flagged for Sean** (the verification guide is user-facing) |
| D284 | **CycloneDX 1.5 SBOMs** for every Rust crate (`cargo cyclonedx --all-features`) and for the desktop UI, the web vault and the extension (syft over each pnpm lock), shipped as one archive per release. Plain version: each release lists every component inside it. | Accepted by default under D38 |
| D285 | **cargo-vet gates CI:** audits imported from bytecode-alliance, google, mozilla, isrg, zcash and embark-studios; the 608 crates nobody has audited are recorded as exemptions; `cargo vet --locked` fails a change that adds an unaudited, unexempted crate. Plain version: every third-party Rust library is either reviewed by a known auditor or listed as not yet reviewed. | Accepted by default under D38; audit item (shrinking the exemptions) |
| D286 | **Reproducible artifacts:** the extension zips (sorted, stored, `SOURCE_DATE_EPOCH`, remapped paths), the server and CLI binaries (built in the digest-pinned Rust image with remapped paths), and the server image (OCI, `rewrite-timestamp`, digest-pinned bases, no build attestations). CI builds each twice on different runner images and requires equal hashes, and the image's server equals the published binary. Plain version: anyone can rebuild these files from the code and get the same bytes. | Accepted by default under D38 |
| D287 | **Desktop bundles are not reproducible yet** (Tauri's binary patch, archive times in deb and rpm, dmg images, signatures, MSI package codes); they are verified by checksum and provenance only. Plain version: the desktop installers are checked by signature, not by rebuilding, for now. | Accepted by default under D38 |
| D288 | **Store packages:** the Chromium package leaves out the development `key` (the stores assign the id, which joins `CHROMIUM_IDS` when the listing exists); the Firefox package keeps `lockdown@snds.design` and ships with a source archive for review. Plain version: the extension is ready to upload to the Chrome, Edge and Firefox stores. | **Flagged for Sean** (the store listings and accounts) |
| D289 | **Firefox runs in CI** by installing the built add-on as a temporary add-on over Firefox's remote debugging protocol in Playwright's Firefox, with a fixed add-on origin and a scratch home for the host manifest. The run covers the menu, native messaging, the host and the app; pairing and fills stay Chromium-only because Playwright's Firefox can't drive extension pages. CI fails if Firefox is missing. Plain version: the extension is now tested in Firefox too, up to the point of pairing. | Accepted by default under D38 |
