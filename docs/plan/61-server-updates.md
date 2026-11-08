# 61: Update checks for the server

_Slice: `lane/servers/lockdown-server`, built 2026-11-08. The unified updates proposal (approved by Sean on 2026-11-08) gives servers the desktop apps' two actions in a server's form (U5): **Check for updates** against the release index, **Update** pulls the new image and restarts, **Reload** restarts the containers. Servers come second, after the desktop apps. This slice builds the parts that need no host access: the published image, the check and the admin page's section. Update and Reload need a helper on the host, which Sean installs; that is the next step (lane D item 3). It builds on doc 60 (the main channel, D680–D689) and doc 24 (the reproducible image). No wire, stored-record or IPC shape changes; one new `operator_settings` row. Proposed decision D690 (§5)._

_Second slice: `lane/servers/helper-wiring`, built 2026-11-08 (lane D, after the host helper merged in snds/app-updates PR #4). The admin page's **Update** and **Reload** buttons, through the host helper (§6). No wire, stored-record or IPC shape changes; no new table or setting (the helper's settings are environment variables). Proposed decision D691 (§7)._

## For future agent

- **TL;DR:** Every push to `main` publishes `ghcr.io/snds/lockdown-server` tagged `<x.y.z-main.<commit time>>` and `main` (release.yml `server-image`). A main-channel server reads `main-build`'s `latest.json` at start, every six hours (a switch on the admin page, on by default) and when asked, and its admin page says "Version … is out." with `docker compose pull && docker compose up -d`.
- **TL;DR (helper):** With `host-helper.env` set (`HOST_HELPER_URL`, `HOST_HELPER_STACK`, `HOST_HELPER_KEY`), the card offers **Update to {v}** and **Reload**; each asks the host helper through its signed client on a thread of its own, sends the operator to a waiting page that polls `/admin/updates/state`, and lands in the operator's log. Without it, the command and a link to the helper's install steps.
- **Read with:** `bins/server/src/updates.rs` (the check, the switch, the admin section and its forms, the waiting page), `bins/server/src/helper.rs` (the helper's settings, the `HostHelper` trait, refusals in words), `bins/server/tests/operator.rs` (the admin page's update tests), `.github/workflows/release.yml` (`meta`, `server`, `server-image`), `compose.yaml`, docs/self-host.md §13.
- **Do not:**
  - Give the server a Docker socket, host access or any way to restart itself. Update and Reload belong to the host helper; the server only asks it, signed, for the version on offer.
  - Log, show or store `HOST_HELPER_KEY`. It is read once at start into the helper's client, whose `Debug` hides it.
  - Wait for the helper's answer on the request. The helper's work replaces this process, so the answer usually never comes; the page polls instead.
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
| Status: available | "Version {v} is out." then "To update, run this in the folder with your `compose.yaml`:" and `docker compose pull && docker compose up -d`, then "It pulls `ghcr.io/snds/lockdown-server:main` (the same build as `…:{v}`) and starts the new container on the same data and settings. Take a backup first." and "See the build on GitHub". With the host helper, §6 instead. |
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

## 6. Update and Reload through the host helper

The host helper (snds/app-updates `host-helper/`, merged as `c0ff0bf`) runs beside the server on the host, holds the Docker socket, and does three things for a server that signs its request with its own key: status, update (pull an allowed tag of this server's own image, check its digest when given one, recreate the container) and restart. Its allow-list decides what it touches; its README holds the protocol, the threat model and Sean's install steps. The server depends on its client alone (`snds-app-updates-host-helper`, `default-features = false`, pinned by commit until the helper is tagged).

| Piece | What it does |
|---|---|
| `helper::Setting` | Read at start from `HOST_HELPER_URL`, `HOST_HELPER_STACK`, `HOST_HELPER_KEY` (the stack's `host-helper.env`, handed in by the helper's `compose.override.yaml`). None set: `Off`. Some set but unusable: `Broken`, with the client's sentence (never the key). All good: `Ready`, the helper's `Client`. |
| `helper::HostHelper` | `update` and `restart`, so tests give a fake; the real one is the client (signed requests, signed replies checked; an unsigned answer is ignored). |
| `Updates::begin` / `finish` / `refuse` | One action at a time (a stuck one stops blocking after the client's 20-minute limit plus a minute); the last one's outcome stays for the card. |
| `POST /admin/updates/update` | Form: `csrf`, `tag`. Signed in, the session's token, and `tag` must be the version on offer now (else "The version on offer has changed since the page was loaded. Look at the Updates card again."). Asks the helper for `{tag}` with no `expect_digest`: Lockdown's manifest has no digest yet (the release workflow would add it beside `version`). |
| `POST /admin/updates/reload` | Form: `csrf`. Asks the helper to restart. |
| The call | On a plain thread named `host-helper`, not the runtime's blocking pool, so shutting down never waits for it. The request answers at once with `303` to the waiting page. |
| `GET /admin/updates/progress` | The waiting page, with one script allowed by its hash (`connect-src 'self'`): every two seconds it reads `GET /admin/updates/state` (`{version, kind, state, message}`, signed in only). A failed request: "Reconnecting… the server is restarting." A sign-in asked for: the server restarted (sign-ins live in memory). A new version: it says so. A refusal: its words. It gives up after 15 minutes and points to the helper's log. |
| The operator's log | Kind `updates`: "the operator, from {address}, asked the host helper for an update to {v}" (or "a reload"), then "the host helper finished …" or "… didn't happen: {why}" when this process is still there to see it. The same line goes to the server's own log. |

**Every new string, for Sean.**

| Where | Words |
|---|---|
| Card, helper set up, newer build | **Update to {v}** · **Reload** · "Update asks the host helper to pull `ghcr.io/snds/lockdown-server:{v}` and start the server on it, on the same data and settings. Take a backup first. Reload restarts the server on the build it runs. Either way the server is gone for a moment and you sign in again." |
| Card, helper set up, up to date | **Reload** · "Reload asks the host helper to restart the server on the build it runs. The server is gone for a moment and you sign in again." |
| Card, no helper | (the command, as §3) · "One-click Update and Reload need the host helper beside the server: its install steps." (a link to the helper's README) |
| Card, helper settings unusable | "The host helper's settings aren't usable: {the client's sentence}" and the no-helper words |
| Card, last action | "The update to {v} is running. Follow it" · "The reload didn't happen. {why}" · "The host helper restarted lockdown." · "The host helper pulled {image} ({digest}) and recreated lockdown. If the version above hasn't changed, the server was already running that build." |
| Not started | "One-click Update and Reload need the host helper, and this server has no helper settings (host-helper.env)." · "An update or reload is already running. Wait for it to finish." · "The server couldn't start the request to the host helper. Try again." |
| Waiting page | Title "Updating" or "Reloading" · "Asking the host helper to update to {v}. Downloading the image can take a few minutes." · "Asking the host helper to restart the server." · "The server goes away while the host helper works and comes back on its own. This page waits for it. Sign-ins don't survive a restart, so you sign in again afterwards." · "Reconnecting… the server is restarting." · "The server restarted. Sign in again to see the version it runs now." · "Lockdown server {v} is running." · "This is taking longer than it should. Look at the host helper's log on the host (docker compose logs, in its folder)." · "Back to the admin page" |
| Refusals (`helper::explain`) | `tag_not_allowed`: "The host helper's allow-list doesn't allow that version, so nothing changed. {helper's sentence}" · digest refusals: "The image the host helper downloaded isn't the build the release names, so it wasn't used and nothing restarted. {…}" · `busy`: "An update or reload is already running on the host. Wait for it to finish, then try again." · `rate_limited`, `too_many_failures`: "The host helper has had too many requests in the last minute. Wait a minute and try again." · `stale`: "The server's clock and the host's differ by more than 90 seconds, so the host helper refused. Check the host's clock, then try again." · `bad_signature`, `unsigned`, `unknown_stack`: "The host helper doesn't recognise this server's key or stack name, so nothing changed. Make this server's key again (the helper's install step 2) and recreate the server. {…}" · `replayed`: "The host helper refused a repeated request. Try again." · `docker_failed`: "Docker on the host couldn't finish the update/reload. {…}" · anything else, and an unreachable helper: the client's own sentence. |

**`compose.yaml` is unchanged** but for a comment: the helper's README puts its network and `host-helper.env` in a `compose.override.yaml` it publishes, beside `compose.yaml`, so `docker compose up -d` works the same with or without the helper.

**Tests:** `helper::tests` (no settings is off, half settings say what's wrong without the key, refusals in words); `updates::tests` (the card without the helper: the command and the pointer; with it: Update for a newer build and Reload always, Reload alone when up to date; half set up; one action at a time and its outcome on the card; the waiting page's script); `tests/operator.rs` (no token or signed out asks nothing; a version not on offer is refused before the helper; Update asks for the offered tag with no digest and redirects to the waiting page; the page's script policy; the state endpoint; Reload; the operator's log lines; a `stale` refusal in words on the card and in the state).

**Not proven here:** the real helper on Sean's Unraid host: the signed round trip over `host-helper-lockdown`, the pull and recreate replacing this process under the request, the waiting page across a real restart (and behind his reverse proxy), and the clocks agreeing. Those are checked there after the merge, with the helper's install step 5 first.

## 7. Proposed decision

| # | Decision | Status |
|---|---|---|
| D691 | **The admin page's Update and Reload ask the host helper; the server still never restarts itself.** With the helper's `host-helper.env` set, the Updates card offers **Update to {v}** (only the version on offer) and **Reload**, each a form with the session's anti-forgery token. The server asks the helper through its client (`snds-app-updates-host-helper`, client only, pinned by commit `c0ff0bf`), signed with the stack's key, on a thread of its own; the operator goes to a waiting page that polls the server until it comes back. No `expect_digest` until Lockdown's manifest carries the image digest. Each request and its outcome go to the operator's log (who, from where, what, which version, how it went; never the key). Refusals are shown in plain words. Without the helper, the card shows the command and a link to the helper's install steps; `compose.yaml` is unchanged. Plain version: with the helper installed, you update or restart the server from the admin page with one press, and it tells you when it is back. | Proposed |
