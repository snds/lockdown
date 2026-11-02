# 46: The `lockdown` command as a Mac download

_Slice: `slice/mac-cli-download`, built 2026-11-02. Sean wants to try the command line on his Mac without installing Rust. The release workflow built the Mac app (D280, D450) but the command line only for Linux and inside the server image. This slice builds on D280 (one universal Mac app), D282 (releases only from a tag, as a draft), D283 (provenance), D284 (SBOMs), D286 and D353 (what is reproducible and what is measured), D450 (the ad-hoc seal), D481 and D513 (no developer tools and no passwordless install in anything that ships), D491–D493 (where a Mac keeps its files, and the signed future) and D510 (`init` always seals). Proposed decisions D540–D549 (§8)._

## For future agent

- **TL;DR:** The release workflow's `macos-cli` job builds `lockdown-cli` for Apple silicon and Intel, joins the two with `lipo`, and checks that the developer tools are not in it. It signs the result the way the app is signed. With the Apple secrets that means a Developer ID, the hardened runtime and a secure timestamp, then notarisation. Without them it is an ad-hoc signature. The job then runs the binary on the runner: `--version`, a sealed `init` in a fresh `--root`, `list`, and a wrong password refused. It packs `lockdown-<v>-macos-universal.tar.gz` with the licence and a README. That file is its own artifact (`macos-cli`), and it goes into the release bundle, `SHA256SUMS` and the provenance attestation like every other file. The SBOM job adds one SBOM per Mac slice. `reproducible.yml` measures the binary on two macOS images.
- **Read with:** `packaging/macos-cli/{build.sh,sign.sh,check.sh,pack.py,README.txt}`; the `macos-cli`, `sbom` and `publish` jobs in `.github/workflows/release.yml`; the `other-platforms` job in `.github/workflows/reproducible.yml`; the macOS bullet in `docs/verify.md`; doc 41 (where the files go) and doc 43 (the sealed `init`).
- **Do not:**
  - Give the command line's signature an entitlements file. Without one, the command keeps its files in Application Support (D492). A group-signed command line waits for doc 41's gap 1 (D493).
  - Staple the command line. A bare Mach-O binary can't hold a ticket. `stapler` refuses it, and macOS looks the ticket up online at first run.
  - Build it with features. `lockdown-cli` has none. The developer tools and `legacy-install` are dev-dependencies only, so they never reach a plain `cargo build`. `packaging/devtools/check.sh absent` fails the job if either one does.
  - Move the keychain step in `macos-cli` without moving the one in `macos` the same way. They are two copies of one routine (§4 gap 4).

## 1. The shape

```text
 meta ──┬── macos ─────────── macos-universal (dmg, .app.zip)          ─┐
        ├── macos-cli ─────── macos-cli (lockdown-<v>-macos-universal.tar.gz)
        │     build.sh   cargo build --release --target aarch64 + x86_64; lipo
        │     check.sh   devtools absent (packaging/devtools/check.sh)
        │     sign.sh    Developer ID + runtime + timestamp + notarytool | ad hoc + runtime
        │     check.sh   lipo, codesign, the signature kind, it runs (and under Rosetta)
        │     pack.py    the archive; unpacked again, same bytes, signature verifies
        ├── sbom ──────────── + lockdown-cli-{aarch64,x86_64}-apple-darwin.cdx.json
        └── … linux, windows, extension, server                         ─┤
                                                                         └── publish: SHA256SUMS, provenance, draft
```

**A job of its own, not a step in `macos`.** The app's job already spends most of its hour on the Tauri build of two architectures, the front end and the dmg. The command line needs neither Node nor Tauri. In its own job it runs at the same time on a second Mac runner, so the release takes no longer. The cost is a second runner and a second Rust cache (`release-macos-cli`), and the keychain step written twice (§4 gap 4).

## 2. The pieces

| Piece | Where | What it does |
|---|---|---|
| Build | `packaging/macos-cli/build.sh <out>` | `cargo build --locked --release -p lockdown-cli --bin lockdown` for `aarch64-apple-darwin` and `x86_64-apple-darwin` in one call, then `lipo -create` into `<out>/lockdown`. The release profile is the workspace's (`lto = "thin"`, `codegen-units = 1`). No features. In CI, `RUSTFLAGS` remaps the checkout and the cargo home (`/lockdown`, `/cargo`), so no runner path is left in the binary. |
| No developer tools | `packaging/devtools/check.sh absent out/lockdown` | The release check's strings (the devtools marker that `legacy-install` carries too, the plugin's command names and the demo pack's name) must not be in the binary. It runs on every build, dry run or tag: the command line never has them. |
| Sign | `packaging/macos-cli/sign.sh <binary>` | Ad hoc when `APPLE_SIGNING_IDENTITY` is unset or `-`: `codesign --force --options runtime --identifier design.snds.lockdown.cli --sign -`. Otherwise the same with `--timestamp` and the Developer ID. Then, with `APPLE_ID`, `APPLE_PASSWORD` and `APPLE_TEAM_ID`, it zips the binary with `ditto`, sends the zip with `xcrun notarytool submit --wait` and prints the notary's log. It fails unless the status is `Accepted`. |
| Check | `packaging/macos-cli/check.sh adhoc\|developer-id <binary> <version>` | `lipo -archs` names `arm64` and `x86_64`. `codesign --verify --strict --verbose=2` passes. The identifier is `design.snds.lockdown.cli`, the hardened runtime flag is set, and there are no entitlements. For `adhoc` the signature is `Signature=adhoc` and not only the linker's (`linker-signed` would mean `sign.sh` never ran). For `developer-id` it needs `Authority=Developer ID Application: …`, a `Timestamp=` and a team. Then `--version` must say `lockdown <version>`. `--master-password-stdin init --name Runner` in a fresh `--root` must say it made a sealed vault and leave `device-key.sealed.cbor` and no plain key. `list` opens with the password, and a wrong password is refused. Where the runner has Rosetta, the Intel slice prints its version and opens the same vault. |
| Gatekeeper, reported | the `Gatekeeper's view` step (notarised runs only) | `spctl -a -vvv -t open --context context:primary-signature`, printed but never failing: `spctl` has no settled verdict for a bare program, and the notary's `Accepted` is the gate. |
| Pack | `packaging/macos-cli/pack.py <binary> <version> <out>` | `lockdown-<v>-macos-universal.tar.gz` holding one folder, `lockdown-<v>-macos-universal/`, with `lockdown` (0755), `LICENSE`, `NOTICE` and `README.txt` (0644). The bytes are the same for the same inputs: a fixed order, every time `SOURCE_DATE_EPOCH`, owner 0 with no names, ustar, and a gzip header with no name or time. The job then unpacks it, compares the binary with `cmp` and verifies its signature again. |
| README | `packaging/macos-cli/README.txt` | Installing to `~/.local/bin` (no administrator) or `/usr/local/bin`. The first run: a notarised release just runs, and a test build has the Gatekeeper steps (§5). `init`, `account login` and `recover`. Where the files go (doc 41). Removing the command. Checking the download. `@VERSION@` is filled in by `pack.py`. |
| Release bundle | `publish` in `release.yml` | `needs` and the download pattern name `macos-cli`, so the archive is in `SHA256SUMS`, in `release-<v>`, in the provenance attestation (`subject-path: dist/*`) and in the draft release. |
| SBOM | `sbom` in `release.yml` | `cargo cyclonedx --manifest-path bins/cli/Cargo.toml --target <t>` for each Mac target, copied to `sbom/lockdown-cli-<t>.cdx.json`. Both must list components. The workspace run keeps making `lockdown-cli.cdx.json` on the Linux runner. |
| Measured | `other-platforms` in `reproducible.yml` (pushes to `main` and by hand) | `build.sh` runs after the app on macos-14 and macos-15 with Xcode 16.2 and remapped paths, and the report lists `cli:lockdown` beside the app's files, with its slices' UUIDs. It is reported, not required (D353). |
| Words | `docs/verify.md` (the macOS bullet) | How the command line is signed, why it can't be stapled, how to check its signature, and the Gatekeeper steps for a test build. |

## 3. How to run it and test it

On a Mac with Rust and both targets (`rustup target add aarch64-apple-darwin x86_64-apple-darwin`):

```sh
packaging/macos-cli/build.sh out
packaging/devtools/check.sh absent out/lockdown
packaging/macos-cli/sign.sh out/lockdown                  # ad hoc; APPLE_SIGNING_IDENTITY=… for a Developer ID
packaging/macos-cli/check.sh adhoc out/lockdown 0.1.0
SOURCE_DATE_EPOCH=$(git log -1 --format=%ct) python3 packaging/macos-cli/pack.py out/lockdown 0.1.0 release
```

In CI, every pull request runs the Release workflow as a dry run, so the `macos-cli` job builds, signs ad hoc, checks, packs and uploads the archive on each change. The Developer ID and notarisation branches run only where the secrets exist, which is Sean's tagged release. They sit behind the same `HAVE_SIGNING` and `HAVE_NOTARY` checks as the app's.

To try a dry run's archive on a Mac, download `macos-cli` from the run's artifacts. A file GitHub's artifact download gives you comes from a browser, so it is quarantined, and §5's steps apply.

## 4. Gaps

1. **The Developer ID and notarisation branches have not run.** No pull request has the secrets, and this slice never reads or adds them. The first tagged release with the secrets is their first run. If `notarytool` refuses, the job fails with the notary's log printed, and the release has no draft until it is fixed. The `spctl` step only reports.
2. **No staple.** A notarised command line needs the network the first time it runs, so macOS can look the ticket up. On an offline Mac the first run is treated like a test build's until it can look. An installer package (`.pkg`) can be stapled, which is the way out if that matters (it would also install to `/usr/local/bin` for you).
3. **Installing is by hand.** There is no Homebrew tap, no installer and no update check. The README gives the two copy commands.
4. **The keychain step is written twice,** in `macos` and in `macos-cli`, word for word. Moving it into a shared script would change the app's job, whose Developer ID path can't be tested on a pull request either, so this slice left the app's job alone.
5. **The archive is not required to be reproducible.** `pack.py` makes the same archive from the same binary, but the binary is measured, not required (D353). `release.yml` doesn't pin Xcode, and a Developer ID signature's timestamp differs on every run anyway. An ad-hoc signature has no timestamp.
6. **Rosetta.** The Intel slice runs only where the runner has Rosetta. Without it the slice is checked by `lipo` and `codesign` alone, and the log says so.
7. **The Linux command line isn't run through the developer-tools check.** The `server` job checks the web vault but not `out/lockdown`. It has the same dependencies and no features, so it is clean today. Adding the same line to `server` is a one-line follow-up outside this slice.
8. **A signed command line with the app group** (doc 41 gap 1.3, D493) is still future. Until then a Developer ID build of the app and of the command line both resolve Application Support, because neither carries the group.

## 5. The words (flagged for Sean)

In the archive's `README.txt` and in `docs/verify.md`:

> A release signed with Lockdown's Developer ID and notarised by Apple just runs.
>
> A test build is not notarised. It carries only an ad-hoc signature, which shows the file is intact but not who made it. If you downloaded it with a web browser, macOS blocks it the first time it runs:
>
> 1. macOS says "lockdown" Not Opened. Choose Done.
> 2. Open System Settings, then Privacy & Security.
> 3. Scroll down to Security. Next to the line saying "lockdown" was blocked, choose Allow Anyway.
> 4. Run `lockdown --version` again. If macOS asks once more, choose Open Anyway.
>
> If you're comfortable with Terminal, you can instead clear the download mark on the copy you installed: `xattr -d com.apple.quarantine ~/.local/bin/lockdown`.

`docs/verify.md` adds that a release's copy can't be stapled, so macOS looks the ticket up online the first time, and how to read the signature (`codesign -dv --verbose=2`: `Authority=Developer ID Application` and `Identifier=design.snds.lockdown.cli`, or `Signature=adhoc`).

## 6. unicron host-contract gaps

None. No view changed. This slice is CI and packaging only, and no Rust source changed.

## 7. Audit notes

- **What the ad-hoc signature proves:** that the file is intact since it was signed, not who signed it (as D450 says for the app). Checksums and the provenance attestation are what tie a test build to Lockdown's workflow.
- **The hardened runtime is on in both branches,** so the binary CI runs is the same kind of binary a Developer ID release ships. It asks for no exceptions: no entitlements at all.
- **No entitlements** means no app group, so the run-time rule (D492) keeps the command line in Application Support whatever the signature.
- **Secrets** reach only the signing and notarisation steps, through environment variables. `notarytool` gets the app-specific password as an argument, on a runner that lasts only for the job.
- **The test vault** in `check.sh` lives in a temporary folder that is removed afterwards. Its master password is a fixed test phrase, and its kit goes to a file in that folder.

## 8. Proposed decisions

For the coordinator to record. **Flagged** rows are user-visible and need Sean. "Audit item" rows go to the external audit under D38.

| ID | Decision | Owner |
|---|---|---|
| D540 | **The Mac command line ships as one universal archive.** `lockdown-<v>-macos-universal.tar.gz` holds one folder with `lockdown` (Apple silicon and Intel, `lipo`), `LICENSE`, `NOTICE` and `README.txt`. It is its own artifact (`macos-cli`) and goes into the release bundle, `SHA256SUMS`, the provenance attestation and the draft release like every other file. Plain version: Mac users can download the command line on its own and run it without Rust. | Under D375; **flagged for Sean** (the file's name and what is in it) |
| D541 | **It is built in its own job, `macos-cli`, beside the app's `macos` job.** It needs no Node or Tauri, and running on a second Mac runner keeps the release's time where it was. Plain version: making the Mac command line doesn't slow the release down. | Under D38 |
| D542 | **Signed like the app.** With the Developer ID secrets: the hardened runtime, a secure timestamp, then notarisation (a zip sent with `notarytool`, which must come back `Accepted`). Without them: an ad-hoc signature with the hardened runtime (D450's rule, applied to the command line). It isn't stapled, because a bare program can't be, so macOS looks the ticket up online at first run. Plain version: a release's command line just runs; a test build opens through Privacy & Security, as the app does. | Under D375; **flagged for Sean** (a notarised release needs the network on its first run); audit item |
| D543 | **The signing identifier is `design.snds.lockdown.cli`, with no entitlements.** Without an app group, the run-time rule (D492) keeps its files in Application Support with the app's. A group-signed command line waits for D493. Plain version: the command line and the app use the same folders on a Mac. | Under D38; audit item |
| D544 | **What CI proves on a real Mac, on every pull request.** The developer tools and `legacy-install` are absent. Both slices are there. `codesign --verify --strict` passes, with the identifier, the hardened runtime, no entitlements and the expected kind of signature. `--version` is right. A sealed `init` in a fresh `--root`, `list` with the password and a wrong password refused all behave. The Intel slice runs under Rosetta where the runner has it. The unpacked copy has the same bytes and its signature holds. Plain version: every change runs the Mac command line on a Mac before it ships. | Under D38 |
| D545 | **The Gatekeeper words** (§5) in the archive's README and in `docs/verify.md`: a notarised release just runs. For a test build, the steps are Done, then System Settings → Privacy & Security → Allow Anyway, then Open Anyway if asked, or `xattr -d com.apple.quarantine` for people comfortable with Terminal. Plain version: the README says exactly what to click the first time. | Under D375; **flagged for Sean** (the words) |
| D546 | **Install by copying to `~/.local/bin` (no administrator) or `/usr/local/bin`,** as the README says, with no installer, Homebrew tap or update check yet (gap 3). Plain version: you copy one file into place. | Under D375; **flagged for Sean** |
| D547 | **One SBOM per Mac slice of the command line** (`lockdown-cli-aarch64-apple-darwin.cdx.json`, `lockdown-cli-x86_64-apple-darwin.cdx.json`), beside the workspace's, in the release's SBOM archive. Plain version: the release lists what is inside the Mac command line too. | Under D38 |
| D548 | **The archive is built the same way every time; the binary is measured, not required.** `pack.py` fixes order, times, owners, modes and the gzip header. `reproducible.yml` reports the unsigned universal binary's hash on macos-14 and macos-15 beside the app (D353). The build remaps the checkout and cargo paths. Plain version: we check how close the Mac command line is to rebuildable, and say so, before promising it. | Under D38 |
