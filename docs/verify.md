# Checking a Lockdown download

Every file in a Lockdown release can be checked three ways before you run it. Each takes a minute. The first needs nothing but your system's own tools. The second needs the GitHub CLI. The third needs a computer that can build the code.

The release workflow that made the files is [`.github/workflows/release.yml`](../.github/workflows/release.yml). How it all fits together is in [`docs/plan/24-release.md`](plan/24-release.md).

## 1. The checksum: the file is the one in the release

Each release has a `SHA256SUMS` file. It lists every file's SHA-256 hash (a fingerprint of its exact bytes). Download it into the same folder as your file, then:

```sh
# Linux
sha256sum --check --ignore-missing SHA256SUMS
# macOS
shasum -a 256 --check --ignore-missing SHA256SUMS
```

```powershell
# Windows (PowerShell): compare the two hashes by eye
(Get-FileHash .\Lockdown_0.1.0_x64_en-US.msi -Algorithm SHA256).Hash.ToLower()
Select-String "Lockdown_0.1.0_x64_en-US.msi" .\SHA256SUMS
```

`OK` (or two equal hashes) means your file has the same bytes as the one the workflow made. On its own, a checksum only shows the file matches the list. Step 2 shows the list and the file came from Lockdown's own build.

## 2. The provenance: the file was built by Lockdown's workflow, from Lockdown's code

Every file in a release carries a signed build attestation. It is a statement, signed through Sigstore with a short-lived certificate GitHub issues to that one workflow run, that says which repository, commit and workflow built the file. Nobody holds a long-lived signing key that could be stolen. The statement is also written to Sigstore's public transparency log, where anyone can look it up.

With the [GitHub CLI](https://cli.github.com):

```sh
gh attestation verify Lockdown_0.1.0_amd64.deb --repo snds/lockdown
```

A good answer ends with `✓ Verification succeeded!` and names the workflow (`.github/workflows/release.yml`) and the tag it ran for. Check that the tag is the version you meant to download. Anything else means the file did not come from that build. Do not install it.

To check offline later, save the attestation once with `gh attestation download <file> --repo snds/lockdown` and verify with `--bundle`.

## 3. Rebuild it: the file is what the code says

Some files are built so that anyone can rebuild them from the same commit and get the same bytes:

| File | Rebuild with |
|---|---|
| `lockdown-chromium-<v>.zip`, `lockdown-firefox-<v>.zip` (browser extension) | `export RUSTFLAGS="--remap-path-prefix=$PWD=/lockdown --remap-path-prefix=$HOME/.cargo=/cargo" SOURCE_DATE_EPOCH=$(git log -1 --format=%ct)`, then `cd extension && pnpm install --frozen-lockfile && pnpm build:wasm && pnpm build && pnpm run package` |
| `lockdown-<v>-x86_64-linux.tar.gz` (server and CLI) | `packaging/repro/build-server.sh out` (needs Docker) |
| `lockdown-server-<v>-image.oci.tar` (server image) | build the web vault (`apps/web`: `pnpm build:wasm && pnpm build`), then `packaging/repro/build-image.sh out lockdown-server:<v>` (needs Docker with Buildx) |
| `Lockdown_<v>_amd64.deb`, `Lockdown-<v>-1.x86_64.rpm` (Linux desktop app) | `packaging/repro/build-desktop.sh out` (needs Docker; a full release build, about half an hour) |

Check out the release's tag first. The extension also needs the same Rust release and `wasm-bindgen` CLI as the workflow used, which the workflow run's log names. The server files are built inside a Rust image pinned by digest, so Docker brings the right compiler. CI proves the same bytes from two machines with the checkout at the same path. A different path should give the same bytes too, because the paths are remapped, but CI does not test that yet. Then compare your hash with the one in `SHA256SUMS`. Lockdown's CI does the same on every change, building each file twice on two different machines (`.github/workflows/reproducible.yml`).

The deb and rpm are built inside an Ubuntu 22.04 image pinned by digest, with its packages from a fixed date of Ubuntu's snapshot archive and exact Rust and Node releases, so Docker brings every tool. The Flatpak's contents are reproducible by its OSTree commit: after `packaging/repro/build-flatpak.sh <stage> out` (in the flatpak-builder image), `out/flatpak-hashes.txt` names the commit, which must equal `flatpak info --show-commit design.snds.lockdown` for the installed release. The other desktop installers (the `.flatpak` file itself, dmg, msi, exe, msix) are not reproducible yet. Use steps 1 and 2 for them. [`docs/plan/29-release-polish.md`](plan/29-release-polish.md) §1 explains why, and what CI compares for each.

## Signed apps on macOS and Windows

Apple and Microsoft check their own signatures when you open the app:

- **macOS:** a release made with Lockdown's Developer ID is signed and notarised. To check it yourself, run `spctl -a -vvv -t exec /Applications/Lockdown.app`, which should say `accepted` and `source=Notarized Developer ID`. A build made without those certificates carries only an ad-hoc signature (D450): it says the files are intact, not who made them. macOS stops it the first time with "Apple could not verify Lockdown"; choose Done, then open System Settings → Privacy & Security and choose Open Anyway.
- **Windows:** a release made with Lockdown's code-signing certificate shows the publisher in the install prompt. Check the signature under the file's Properties → Digital Signatures tab, or with `Get-AuthenticodeSignature .\Lockdown_0.1.0_x64-setup.exe`.

Until those certificates exist, releases are unsigned on Windows and ad-hoc signed on macOS. Steps 1 and 2 still apply.

## What is in it: the SBOMs

`lockdown-<v>-sbom.tar.gz` holds CycloneDX SBOMs, a machine-readable list of every component in the build and its version. There is one for each Rust crate (every dependency in `Cargo.lock`) and one for each JavaScript app (the desktop UI, the web vault and the extension). Feed them to a vulnerability scanner, for example `grype sbom:lockdown-server.cdx.json`.
