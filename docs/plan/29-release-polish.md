# Release polish: reproducible Linux desktop packages, code-signature peer checks, a browser reaching the Flatpak

_Status: built, 2026-10-18, under D122 ("desktop and web first"). Nothing in this slice publishes, signs with a real key, pushes a tag, makes a GitHub release or uploads anywhere. Every certificate it signs with is made inside a CI job and thrown away with it. Follows `24-release.md` (release engineering), whose gaps §5, §7 and §10 this closes in part._

## For future agent

- **TL;DR:**
  - **Reproducible Linux packages.** The deb and the rpm are now built inside a pinned container, `packaging/repro/build-desktop.sh`. The container is Ubuntu 22.04 by digest, with its packages from `snapshot.ubuntu.com` at a fixed time, and exact Rust and Node releases. `packaging/repro/normalize-package.py` then rewrites the times and orders Tauri's bundler stamps into both packages. `reproducible.yml` builds them on two runner images and requires the same bytes. It also builds the Flatpak from each runner's binaries and requires the same files. The Flatpak's commit and bundle, and the macOS and Windows bundles, are compared and reported, not required (§1).
  - **Code signatures on the browser channel.** A new crate, `crates/codesign` (`lockdown-codesign`), checks the program on the other end by its code signature: the Security framework on macOS, and `WinVerifyTrust` plus the signer's certificate on Windows. The macOS peer is found by its **audit token**, carried by a Lockdown socket. A signed build requires the same developer on the other end, on top of the path and SHA-256 check every build keeps. An unsigned development build keeps that path and hash check alone and says so in Settings → Browsers. CI signs test builds with throwaway certificates and proves a matching signer, a mismatched signer, an unsigned peer and (macOS) another identifier, both ways (§2).
  - **A browser reaching the Flatpak.** When Lockdown runs as a Flatpak, Connect writes a host-side wrapper into the app's data folder. The browser's manifest names the wrapper. The wrapper uses `flatpak enter` to run the browser host inside the running app's sandbox, so each end can see and check the other's process. Release CI runs the installed Flatpak under a virtual display and connects to it through the wrapper. It also shows that a host in a sandbox of its own is refused (§3).
- **Read with:** `crates/codesign/`, `bins/native-host/src/lib.rs`, `apps/desktop/src-tauri/src/browser/mod.rs` (`listen`, `install`, `status`), `crates/native-messaging/src/flatpak.rs` and `flatpak-host.sh`, `packaging/repro/{build-desktop.sh,desktop-container.sh,normalize-package.py,build-flatpak.sh}`, `packaging/codesign/`, `packaging/flatpak/{design.snds.lockdown.yml,browser-test.sh}`, `.github/workflows/{ci,release,reproducible}.yml`.
- **Do not:**
  - let a signed build fall back to the path check alone (a signed host must never relay to an unsigned app);
  - check a macOS peer by process id where the channel has its audit token;
  - sign the rpm before `normalize-package.py` runs (the normaliser refuses a signed rpm; sign afterwards);
  - move the snapshot time, the image digests or the Rust and Node pins in `build-desktop.sh` without saying so in the release notes: they change the package bytes;
  - run the Flatpak's browser host in a sandbox of its own (`flatpak run --command`) while the app runs: the app can't see that process and refuses it.

## 1. Reproducible Linux desktop packages (`reproducible-builds`)

### What made two builds differ

`24-release.md` §5 listed the reasons as guesses. Reading Tauri's bundler (`tauri-bundler` 2.10.3, the version `@tauri-apps/cli` 2.12.3 pins) and comparing two bundlings of one build here gave the real list:

| Where | What differed | Fixed by |
|---|---|---|
| The binaries | Each runner image has its own C compiler, linker, WebKitGTK headers and zlib. The Rust release came from `rust-toolchain.toml`'s moving `stable`. | The pinned container: `ubuntu:22.04@sha256:0a98…8956`, packages from `snapshot.ubuntu.com/ubuntu/20261015T000000Z`, Rust 1.97.0 through a checksummed `rustup-init` 1.29.1, and Node 22.23.3 checksummed. pnpm comes from `packageManager`, and the Tauri CLI and every JS package from the lock file. The paths are remapped (`/src` to `/lockdown`, the cargo home to `/cargo`). |
| Tauri's binary patch | Nothing. It writes the bundle type (`__TAURI_BUNDLE_TYPE_VAR_DEB`/`_RPM`) into the executable. That is deterministic per package type, so `24-release.md`'s worry about it was unfounded. | Nothing needed. |
| deb: `ar` members | The build's time, and the file's owner and mode. | Set to `SOURCE_DATE_EPOCH`, 0, 0 and 0644. |
| deb: `control.tar.gz`, `data.tar.gz` | Entry order follows the file system's directory order (`WalkDir`). Every entry's time is the copy's time. | Sorted by name; times set to `SOURCE_DATE_EPOCH`; owner set to root:root (0:0). |
| deb: `md5sums` | Line order follows the same walk. | Sorted by path. |
| rpm: header | `RPMTAG_BUILDTIME` is the build's clock, and every `RPMTAG_FILEMTIMES` entry is the copy's time. The `rpm` crate supports `source_date`, but the bundler doesn't pass it. | Both set to `SOURCE_DATE_EPOCH`. |
| rpm: payload | Each cpio entry carries the copy's time. | Set to `SOURCE_DATE_EPOCH`, and the payload recompressed (gzip level 6, no name, time 0). Then every digest over those bytes is recomputed in place: the payload SHA-256 (compressed and not) in the header, the header's SHA-1 and SHA-256 in the signature header, and the size. Every field keeps its length, so no offset moves. |

`normalize-package.py` uses Python's standard library only, and runs inside the container, so the zlib that recompresses is pinned too. It refuses anything it can't keep true rather than half-rewriting it: a signed rpm, an rpm that names its build host, a payload compressor other than gzip, or a deb member that isn't a gzip'd tar.

**Checked here.** Two bundlings of one debug build, made seconds apart, gave four different files. After normalising, each package was the same bytes both times. The normalised deb unpacked to the same files as the original and passed `md5sum -c` against its own `md5sums`. The normalised rpm passed `rpm -Kv` (header SHA-256 and SHA-1, payload SHA-256). It listed the same files, sizes, modes and digests as the original, and its payload held the same file contents. `packaging/repro/test_normalize_package.py` covers the deb path: two debs that differ only in time and order become the same bytes, sorted, with their contents and modes kept.

### What CI checks

| Job (`reproducible.yml`) | What | Required |
|---|---|---|
| `desktop` × ubuntu-24.04, ubuntu-22.04 | `build-desktop.sh`: the deb, the rpm and the two release binaries, plus a hash of every file inside the deb | the same SHA-256 for all of it on both runners |
| `flatpak` × the same two runners | `build-flatpak.sh` in the digest-pinned flatpak-builder image, from that runner's own binaries | the Flatpak's **files**: a hash over `ostree ls -R -C` (path, mode and content checksum of every file; OSTree stores no file times) |
| `desktop-compare` | both of the above | as listed; the Flatpak's commit and bundle are reported as warnings |
| `other-platforms`, `other-report` (pushes to `main` and manual runs only) | the universal macOS app and dmg on macos-14 and macos-15; the MSI, the NSIS installer and both executables on windows-2022 and windows-2025; every file hashed | never: a table in the job summary says what matches |

The release workflow's `linux` job now runs the same `build-desktop.sh`, so the deb and rpm users download are the ones `reproducible.yml` checks, and the Flatpak packs the same binaries.

### Still not reproducible, and why

RESULTS-OTHER

## 2. Code-signature peer checks (`ipc-peer-auth`)

### The policy

Each end reads its **own** code signature once, at start, and decides what it requires of the other end (`lockdown_codesign::Policy::from_own`, pure, tested on every platform):

| This build is | The other end must be |
|---|---|
| signed with an Apple Team ID | signed under an Apple anchor by the same team, with the expected identifier: `identifier "lockdown-native-host" and anchor apple generic and certificate leaf[subject.OU] = "<team>"` |
| signed with any other certificate (Windows; a macOS certificate Apple didn't issue) | signed with the same leaf certificate, the system accepting the signature. macOS adds the expected identifier and `certificate leaf = H"<SHA-1>"`. |
| unsigned, or ad hoc (macOS; the linker signs every Apple silicon build this way) | checked by path and SHA-256 only, as before. Settings → Browsers says "This is a development build without a code signature, so the other program is checked by its path and SHA-256 only." |
| signed, but the system rejects its own signature | nothing: every peer is refused, and Settings says why |

The signature check comes **on top of** the path and SHA-256 check (`Both(process, signature)`), never instead of it. A signed build never falls back to the path check alone, so a signed host relays only to a signed app of the same developer, and the reverse. Identifiers (macOS): the app is `design.snds.lockdown` (the bundle identifier Tauri signs `Lockdown.app` with), and the host is `lockdown-native-host` (`codesign` names a bare executable by its file name, which is how Tauri signs the sidecar in `Contents/MacOS`). Linux has no OS code signatures, so there the path and hash check is the whole check, and Settings says so.

### macOS: the audit token, then `SecCode`

```text
host ──connect──▶ /…/browser.sock ◀──accept── app
  LOCAL_PEERTOKEN → audit token of the app       LOCAL_PEERTOKEN → audit token of the host
  SecCodeCopyGuestWithAttributes(kSecGuestAttributeAudit)
  SecCodeCheckValidity(guest, requirement)  ── the running code and its file, against the policy's requirement
  SecCodeCopySigningInformation            ── team, identifier, leaf certificate, compared again (Expected::check)
```

- **Why a new socket.** The identity crate's `UnixSocketTransport` reports the peer by process id only, and keeps its stream private. A process id can name another program by the time it is checked: a process connects, hands the socket to a helper, and replaces itself with a signed binary under the same id. The audit token the kernel records at `connect` also carries the id's version, and the Security framework refuses a token whose process has since changed (`errSecCSNoSuchCode`). So `lockdown_codesign::socket` is the identity crate's socket, re-made with the token captured. Everything else is unchanged: `0600` in a `0700` folder, stale-socket removal, the same 4-byte big-endian frames and 64 KiB limit (through `ecosystem_identity::handshake::wire`), and the same `PeerIdentity::Unix` for the path and hash check. Both the app and the host use it on macOS. Linux keeps the identity crate's socket. The socket's tests run on Linux too, and only macOS reports a token.
- **What the check covers.** `SecCodeCheckValidity` on a guest checks the kernel's view of the running code and the file it came from, so the TOCTOU the path and hash check has (swapping the file between reading the path and hashing it) doesn't reach it.

### Windows: `WinVerifyTrust` and the signer's certificate

`lockdown-pipe` already finds the peer's process from the pipe, holds it open for the channel's life, and reads its image path. `lockdown_codesign::windows` then:

1. Opens the image file for reading with `FILE_SHARE_READ` only, so it can't be written or deleted while it is checked.
2. Runs `WinVerifyTrust` (`WINTRUST_ACTION_GENERIC_VERIFY_V2`) on that handle. The signature must verify and chain to a root this machine trusts. Revocation is not fetched (`WTD_REVOKE_NONE`, `WTD_CACHE_ONLY_URL_RETRIEVAL`), so a connection never waits on the network.
3. Reads the signer's leaf certificate from the verified state (`WTHelperProvDataFromStateData`, `WTHelperGetProvSignerFromChain`) and compares its SHA-256 with this program's own signer.

The remaining Windows gap: the file at the image path is checked, not the image mapped in memory. A same-user program that renames a running executable and puts another file at its path still meets the signature check with the file at that path, so the pipe's hold on the process and the path rule carry that case, as before (§6).

### The identity crate (host-contract gap)

The identity crate lives in `snds/ecosystem-identity`, not in this repository (D34), so this slice did **not** add a Windows variant to its `PeerIdentity`. The gap stands as `24-release.md` §7 recorded it:

- A pipe channel still reports `PeerIdentity::Unknown`.
- Lockdown's verifiers are bound per channel (`PipeVerifier::bind`, and now `SignatureVerifier::new(policy, PeerCode::…)`) over what the transport reported beside it.
- The identity crate's own `CodeSignatureVerifier` still refuses every peer.

What the crate would need, so the next change can be made there:

1. `PeerIdentity::Windows { pid, user_sid, exe }`.
2. An audit token on the Unix variant, or a macOS variant, from `LOCAL_PEERTOKEN`.
3. A `CodeSignatureVerifier` built on the same two mechanisms as `lockdown-codesign`.

Then `lockdown_codesign::socket`, `PeerCode` and `bind` go away.

### Saying so

`EndpointStatus` gains `peer_check: { by, detail }`, an added field per D62's rule (no field renamed or removed, and readers ignore unknown fields). `by` is `signature`, `path_and_hash` or `refused`, and Settings → Browsers shows `detail` under the connection line, in the error colour when every host is refused. The native host's refusal says why: "the program on Lockdown's socket isn't the Lockdown app (the other program is signed by …, not …)".

### How CI proves it

`ci.yml` gains `codesign-macos` and `codesign-windows`. Each builds `crates/codesign/tests/signed.rs` (which plays the app) and `examples/peer.rs` (which plays the host), then signs:

| Binary | macOS (`packaging/codesign/throwaway-macos.sh`) | Windows (`packaging/codesign/throwaway-windows.ps1`) |
|---|---|---|
| the test (app) | certificate A, `design.snds.lockdown` | certificate A |
| `peer-same` | A, `lockdown-native-host` | A |
| `peer-other` | B, `lockdown-native-host` | B |
| `peer-unsigned` | left as the linker made it (ad hoc on Apple silicon) | unsigned |
| `peer-wrong-id` | A, `lockdown-something-else` | (Authenticode has no identifier) |

A and B are self-signed code-signing certificates made in the job. On macOS they sit in a keychain of their own. On Windows they are trusted in the runner's `LocalMachine\Root` for the job, and removed at its end. Neither is a real identity. Over the real channel (the audit-token socket on macOS, the named pipe on Windows), the signed test must:

- accept `peer-same`, which accepts it back;
- refuse `peer-other`, which refuses it back;
- refuse `peer-unsigned`;
- refuse `peer-wrong-id`.

Outside those jobs the test says it has nothing signed to check and passes. Every other platform's unit tests check that an unsigned or ad hoc test binary gets the path-and-hash policy.

RESULTS-CODESIGN

## 3. A browser reaching the app inside the Flatpak (`linux-app`)

```text
browser (host) ── starts ──▶ ~/.var/app/design.snds.lockdown/data/snds/lockdown/lockdown-native-host-flatpak
                              ├─ app running:  flatpak enter design.snds.lockdown /app/bin/lockdown-native-host <args>
                              └─ not running:  flatpak run --command=lockdown-native-host design.snds.lockdown <args>  → "no_app"
```

- **Why `flatpak enter`.** The app checks the program that connects by its process: the same user, the path `/app/bin/lockdown-native-host` and its SHA-256. A host in a Flatpak sandbox of its own has its own process namespace. The app's `SO_PEERCRED` then reports process id 0 for it, there is no `/proc` entry to read, and it is refused. The same happens to a host running outside the sandbox. `flatpak enter` runs the host in the running app's own namespaces: its processes, its files and its environment, so the same data folder and socket. Then both ends see each other and check each other as everywhere else. It needs unprivileged user namespaces, which every current desktop distribution has. Without them `flatpak enter` refuses, and the browser sees the host exit.
- **The wrapper** (`crates/native-messaging/src/flatpak-host.sh`, compiled into the app, the app id filled in). Connect inside a Flatpak (`/.flatpak-info` present and `FLATPAK_ID` set) writes it, `0700`, into the app's data folder. That folder is under `~/.var/app/<id>/`, which the sandbox and the host see at the same path. Connect refuses a data folder anywhere else. The manifests then name the wrapper instead of `/app/bin/lockdown-native-host`.
- **Where the manifests go.** Inside the sandbox, `XDG_CONFIG_HOME` is the app's own folder. So `Homes::from_env` uses the host's (`HOST_XDG_CONFIG_HOME`, else `~/.config`). The Flatpak manifest opens exactly the browsers' native-messaging folders, and nothing else of theirs:
  - `xdg-config/{google-chrome,chromium,microsoft-edge,BraveSoftware/Brave-Browser}/NativeMessagingHosts:create`
  - `~/.mozilla/native-messaging-hosts:create`

  `:create` makes the folder on the host if a browser hasn't. A side effect: inside the sandbox every browser's folder exists, so Settings lists all five browsers whether they are installed or not.
- **Tested in CI** (`release.yml` → `flatpak` → `packaging/flatpak/browser-test.sh`):
  1. The installed bundle runs under Xvfb with a session bus. WebKit's own process sandbox is turned off for the test, because it needs the Flatpak portal; the browser channel doesn't involve WebKit.
  2. The test waits for the endpoint file, then writes the wrapper exactly as Connect does.
  3. As a browser, it sends a framed `status` through the wrapper. It must get `ready` and then `"paired":false`, which means both peer checks passed across the sandbox boundary.
  4. It then runs the host with `flatpak run --command` while the app runs. That must not get `ready`, and must get `refused` or `untrusted_app`.

  The CI container runs as root, so it doesn't show `flatpak enter` without root; on a desktop it runs as the user.

RESULTS-FLATPAK

## 4. How to run it

- `cargo test -p lockdown-codesign -p lockdown-native-messaging -p lockdown-native-host`: the policy, the socket, the wrapper, the host.
- On a Mac: `cargo build -p lockdown-codesign --example peer`, then `cargo test -p lockdown-codesign --test signed --no-run`, then `packaging/codesign/throwaway-macos.sh /tmp/signed <test binary> target/debug/examples/peer`. Then `LOCKDOWN_CODESIGN_TEST=require LOCKDOWN_CODESIGN_PEER_SAME=… "$LOCKDOWN_CODESIGN_APP_TEST" --nocapture`, with the variables the script printed.
- On Windows, the same with `pwsh packaging/codesign/throwaway-windows.ps1 -Out … -TestBin … -PeerBin …`, as administrator, because it trusts the throwaway certificates in `LocalMachine\Root`.
- `packaging/repro/build-desktop.sh out` (Docker; a full release build, about 25 minutes): `out/SHA256SUMS`. Then `packaging/repro/build-flatpak.sh out/stage flat`, inside the flatpak-builder image.
- `python3 packaging/repro/test_normalize_package.py`; `SOURCE_DATE_EPOCH=… python3 packaging/repro/normalize-package.py <deb> <rpm>` on any Tauri bundle.
- `sh packaging/flatpak/browser-test.sh` after installing the bundle (`flatpak --user install --bundle lockdown-*.flatpak`).

## 5. Rows

| Row | State | What is real | What is missing |
|---|---|---|---|
ROWS

Counts: COUNTS

## 6. Gaps

GAPS

## 7. unicron host-contract gaps

None. The one UI change is a line of text in Settings → Browsers, in the section's existing muted and destructive text styles, with no new component. `pnpm ds:ratchet` stays at its baseline.

## 8. Proposed decisions

For the coordinator to record. **Flagged** rows are user-visible and need Sean.

| ID | Decision | Owner |
|---|---|---|
DECISIONS
