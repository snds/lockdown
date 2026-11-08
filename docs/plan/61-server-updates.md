# 61: Update checks for the server

_Slice: `lane/servers/lockdown-server`, built 2026-11-08. The unified updates proposal (approved by Sean on 2026-11-08) gives servers the desktop apps' two actions in a server's form (U5): **Check for updates** against the release index, **Update** pulls the new image and restarts, **Reload** restarts the containers. Servers come second, after the desktop apps. This slice builds the parts that need no host access: the published image, the check and the admin page's section. Update and Reload need a helper on the host, which Sean installs; that is the next step (lane D item 3). It builds on doc 60 (the main channel, D680–D689) and doc 24 (the reproducible image). No wire, stored-record or IPC shape changes; one new `operator_settings` row. Proposed decision D690 (§5)._

## For future agent

- **TL;DR:** Every push to `main` publishes `ghcr.io/snds/lockdown-server` tagged `<x.y.z-main.<commit time>>` and `main` (release.yml `server-image`). A main-channel server reads `main-build`'s `latest.json` at start, every six hours (a switch on the admin page, on by default) and when asked, and its admin page says "Version … is out." with `docker compose pull && docker compose up -d`.
- **Read with:** `bins/server/src/updates.rs` (the check, the switch, the admin section and its two forms), `bins/server/tests/operator.rs` (the admin page's update tests), `.github/workflows/release.yml` (`meta`, `server`, `server-image`), `compose.yaml`, docs/self-host.md §13.
- **Do not:**
  - Give the server a Docker socket, host access or any way to restart itself. Update and Reload belong to the host helper.
  - Send anything about the server with a check: no version, account or address in the request (the user agent is a bare `lockdown-server`).
  - Copy the update rules. They come from `snds-app-updates` (no `tauri` feature), the same as the desktop app's.
  - Re-encode the image when pushing it. `--preserve-digests` keeps the published digest the reproducible build's.

## 1. The image

`server-image` runs only on a push to `main` (`meta.publish`, and its own `if` checks the event and the ref), after `server`. It downloads the `server` job's OCI archive, the same reproducible build CI already checks (doc 24 §5), and pushes it with `skopeo copy --preserve-digests`:

1. to `ghcr.io/<owner>/lockdown-server:<version>`, then checks the registry's digest equals the archive's;
2. to `:main`, only after the version tag is in place;
3. attests its build provenance to the registry (`actions/attest-build-provenance`, keyless).

It uses `GITHUB_TOKEN` with `packages: write`; no secret is stored. The Dockerfile's `org.opencontainers.image.source` label links the package to this repository. Its own concurrency group keeps two runs from moving `main` at once.

`compose.yaml` now names `ghcr.io/snds/lockdown-server:main` and keeps `build: .`, so `docker compose up -d` runs the published image and `up -d --build` builds the same commit from the checkout.

The image and the manifest are published by different jobs. A merge whose desktop builds fail publishes the image but no new manifest, so servers keep saying they are up to date until the next merge. A merge whose image push fails but whose desktop builds pass would offer a version that `:main` doesn't have yet; `server-image` failing shows on the run.

## 2. The check

| Piece | What it does |
|---|---|
| `FEED` | The desktop app's main channel: `releases/download/main-build/latest.json`, GitHub's hosts, no stable endpoint yet. |
| `running()` | The version the binary was built as (`CARGO_PKG_VERSION`, stamped by `stamp-version.py` on every build but a tag's). |
| `Channel::of` (kit) | `x.y.z-main.*` checks; a plain version (a tag build, or a build from source) doesn't, and says so. |
| `hop_allowed` | The kit's `allowed` (https, GitHub's hosts, no user name, the usual port) on the manifest's address and on every redirect, at most five. |
| `judge` | Reads `version` from the manifest (at most 256 KiB), and the kit's `is_newer` decides. |
| `GitHub` | A blocking `reqwest` client: 20 s timeout, the redirect rule, `accept: application/json`, user agent `lockdown-server`. |
| `Updates` | The status (`Idle`, `Checking`, `UpToDate`, `Available`, `Failed`, the kit's `UpdateState`), the offered version, the error, when it last checked. One check at a time. |
| `spawn` | A thread that checks at start and then every six hours (the kit's `CHECK_EVERY`) while the switch is on. A build that doesn't check starts none. The in-process local server (`local.rs`) never starts it. |
| The switch | `operator_settings.update_auto_check` (`on` or `off`; absent is on). |

## 3. What the operator sees (every new string, for Sean)

The admin overview gets an **Updates** card, after Setup checks, in the desktop app's About words (doc 60 §7):

| Where | Words |
|---|---|
| Version line | "Lockdown server {version} · main channel (every merged change)" |
| Status: idle, switch on | "Lockdown server checks for updates when it starts and every six hours." |
| Status: idle, switch off | "Automatic checks are off. Check whenever you like." |
| Status: checking | "Checking for updates…" |
| Status: up to date | "Lockdown server is up to date." |
| Status: available | "Version {v} is out." then "To update, run this in the folder with your `compose.yaml`:" and `docker compose pull && docker compose up -d`, then "It pulls `ghcr.io/snds/lockdown-server:main` (the same build as `…:{v}`) and starts the new container on the same data and settings. Take a backup first. One-click Update and Reload come later, with a small helper on the server's host." and "See the build on GitHub". |
| Status: failed | "The server couldn't reach GitHub. Check its connection and try again." · "No main-channel build is published yet." · "The update's description couldn't be read." · "Updates come only over https." · "The update was refused: updates don't come from {host}." |
| Status: off | "Update checks are off for this build: it was built from source or from a tagged release, and only main-channel builds (the published image) check for now." |
| Under it | "Last checked {date time} UTC." |
| Button | **Check for updates** |
| Switch | "Check for updates automatically" with **Save** |

The admin page is the server's own HTML (`pages.rs`, one hashed style sheet, no script), not the web vault, so it doesn't use Unicron; the card is built from the page's existing `card`, `muted` and button styles, plus `pre` and `label.check`. Both forms carry the session's anti-forgery token; a turned switch is in the operator's log.

## 4. Tests and what is not proven

- `updates::tests`: newer, same, older and garbled manifests; GitHub's hosts only, over https, no user name or odd port, at most five redirects; a main build reads the main manifest and says what it found; a tag or source build never reads anything; the switch's default and both settings.
- `tests/operator.rs`: the admin card before a check; a form from another page and a signed-out post check nothing; Check for updates reads the main manifest once and the page shows the version, the command, the pinned tag and the helper note; the switch turns off and on; a build from source offers no button.

**Not proven here:** the first push to ghcr.io (the package's first creation and its visibility), the real check against GitHub from a running container, and `docker compose pull && docker compose up -d` on Sean's server. Those happen after the merge.

## 5. Proposed decision

| # | Decision | Status |
|---|---|---|
| D690 | **The server checks for updates and says; it never updates itself.** Every push to `main` publishes the reproducible server image, digest unchanged, to `ghcr.io/snds/lockdown-server` as `<the main-channel version>` and `main`, with build provenance (release.yml `server-image`, `GITHUB_TOKEN` only). A main-channel server reads the desktop app's manifest (`main-build`'s `latest.json`) at start, every six hours while "Check for updates automatically" is on (admin page, on by default), and when asked, with the shared kit's rules (GitHub's hosts over https, every redirect checked, strictly newer only, nothing about the server sent). The admin page shows the version, the channel, the status and, for a newer build, `docker compose pull && docker compose up -d`. `compose.yaml` names the published image and still builds from source with `--build`. The server gets no Docker socket and no way to restart itself; one-click Update and Reload wait for the host helper (U5, lane D item 3). Plain version: the server tells you when a newer build is out and how to get it; it never changes itself. | Proposed |
