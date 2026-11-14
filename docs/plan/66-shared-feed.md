# 66: The shared feed, and a web vault that learns of a new build

_Slice: `wt/updates`, built 2026-11-14, at Sean's request: "set each app using the same release and update flow", and "as each release is updated … the releases get pushed to the releases repo so any apps with the auto-update feature get the new release". It follows the unified updates proposal (choices U1 to U6, approved 2026-11-08) and builds on doc 60 (the desktop app's updates), doc 61 (the server's) and doc 17 (the web vault). No wire, stored-record, IPC or wasm shape changes. No new dependency. No screen was changed and no word on any screen was added. Proposed decisions D740 to D746 (§8). Doc 65 and D730 to D739 are left free for the redesign slices running beside this one._

Words used here:

- **Feed:** the public file an app reads to learn its newest build (`latest.json`).
- **Shared feed:** a feed on the public repository `snds/releases`, where every snds app's feed lives.
- **Bridge:** the old feed, kept alive so copies already installed still find the next build.
- **Kit:** the shared repository `snds/app-updates` (the update rules, the release scripts, the host helper).

## For future agent

- **TL;DR:** Every merge to `main` still publishes `main-build` on this repository first, exactly as before. Then, with the secret `RELEASES_TOKEN`, it also publishes the same signed update files to `lockdown-main` on snds/releases, and names the server image in `lockdown-server-main` there. Without the secret both are skipped with a notice and the run passes.
- **TL;DR (not switched):** The app and the server still read `main-build`. Nothing installed changes what it trusts. Moving the address they read is Sean's decision (§6).
- **TL;DR (web vault):** The web vault's build writes `build.json` and its id into the page. An open tab asks for `build.json` again later and knows when the server has a newer page. It reloads by itself only when nobody is signed in in that tab. Signed in, it only offers.
- **Read with:** `.github/workflows/release.yml` (`main-channel` steps after "Replace the main-build pre-release's files", and `server-feed`), `packaging/update/publish-feed.sh`, `packaging/update/write-server-manifest.py`, `packaging/update/test_update_scripts.py`, `.github/workflows/ci.yml` (`updater`), `apps/web/build-stamp.ts`, `apps/web/src/core/build-watch.ts` and `build-watch-dom.ts`, `apps/web/e2e/build-watch.spec.ts`.
- **Do not:**
  - Remove, skip or reorder the `main-build` publish. It is what every installed app and every running server reads. If it is ever retired, its last `latest.json` stays in place.
  - Put `RELEASES_TOKEN` in any step but the ones that write to snds/releases, or pass it as an argument. `gh` reads it from `GH_TOKEN`.
  - Upload anything to the shared feed that the manifest does not name. The page people download from by hand stays on this repository (§2).
  - Loosen the hosts rule to make the move. It needs no change: snds/releases is on the same GitHub hosts (§6).
  - Make the web vault reload by itself while someone is signed in. A reload ends the tab's sign-in (§4).
  - Change the web vault's Content Security Policy for the watch. `connect-src 'self'` already allows it.

## 1. Where things are

| What | Where | Who can write it | Who reads it today |
|---|---|---|---|
| The desktop feed, the bridge | `main-build` on snds/lockdown | The `main-channel` job, with this repository's own short-lived token (D685) | Every installed desktop app, and every running server |
| The desktop feed, shared | `lockdown-main` on snds/releases | Anything that holds `RELEASES_TOKEN` | Nothing yet (§6) |
| The server feed, shared | `lockdown-server-main` on snds/releases | Anything that holds `RELEASES_TOKEN` | Nothing yet (§3) |
| The server image | `ghcr.io/snds/lockdown-server` | The `server-image` job (`GITHUB_TOKEN`, `packages: write`) | A server's host, on Update |
| The page people download from by hand | `https://github.com/snds/lockdown/releases/tag/main-build` | The `main-channel` job only | A person on Linux, or where the app can't replace itself (D688) |
| The web vault's build id | `build.json` beside the page, on each server | The web vault's build | Every open web vault tab on that server |

## 2. The desktop feed on snds/releases

`main-channel` gains three steps, after the `main-build` publish. They run only with the updater's signing key (as the whole job does) and with `RELEASES_TOKEN`.

1. **Read what is published.** `publish-feed.sh previous` downloads `lockdown-main`'s `latest.json`. No release yet, or no manifest yet, is fine: this run starts the feed. A manifest that is there and can't be read stops the step. Nothing is ever written over a manifest that couldn't be compared.
2. **Write the manifest again.** The three signed update files (the Mac `.app.tar.gz`, the Windows installer, the deb) and their signatures are copied to a folder of their own. `write-manifest.py` writes a second `latest.json` for them with the shared feed's address. It refuses a version that is not newer than the shared feed's. The step then compares the two manifests: the same version, notes, date, server image and signatures, and the same file names. Only the addresses differ.
3. **Publish.** `publish-feed.sh publish` makes or edits the pre-release (always a pre-release, never "latest"), uploads the files, uploads `latest.json` last, and removes files the folder doesn't hold.

The signatures are the ones the job already checked against `updater.pub` with `minisign`, before anything was published anywhere. The files are the same bytes that went to `main-build`.

**Only the files the manifest names go to the shared feed.** The dmg, the rpm, the Flatpak and `SHA256SUMS` stay on this repository. The shared feed is for the app's own updater, which checks every file's signature. It is not a place to download Lockdown by hand, and its notes say so and link to `main-build`.

**When things go wrong:**

| What | What happens |
|---|---|
| `RELEASES_TOKEN` isn't set | The steps are skipped. The run says so in a notice and in its summary, and passes. `main-build` is published as usual. |
| The token expired, or can't reach snds/releases | The step fails, after `main-build` was published. Installed apps keep updating. The run is red, so it is seen. |
| The shared feed already has this version or a newer one | `write-manifest.py` refuses and the step fails. Nothing is uploaded. |
| An upload fails half way | The old `latest.json` is still in place and still names the old files, which are still there. |
| The run is rerun after a failure here | The `main-build` step refuses first (that version is already published there), so the job stops before these steps. The shared feed catches up at the next merge. |

## 3. The server feed

A new job, `server-feed`, runs after `server-image` has pushed the image and moved `main`. It writes `latest.json` to `lockdown-server-main` on snds/releases, in the shape every server's feed has:

```json
{
  "app": "lockdown-server",
  "version": "0.1.0-main.20261107093000",
  "commit": "<40 hex>",
  "pubDate": "2026-11-07T09:30:00Z",
  "images": { "server": { "repository": "ghcr.io/snds/lockdown-server", "digest": "sha256:<64 hex>" } }
}
```

- `write-server-manifest.py` writes it. It names only `ghcr.io/snds/lockdown-server` (a test holds that to the server's own `IMAGE`), and only a well-formed digest.
- It never goes backwards. A rerun of the same version, or a late run of an older one, leaves the feed as it is and passes. A published manifest that can't be read fails the job.
- The job has `contents: read` and nothing else. It can't write to this repository or to the registry. The token is in one step.
- It is written last, so it never names an image that isn't in the registry.

**No running server reads it yet.** Servers read `server_image.digest` in `main-build`'s `latest.json` (D692), and that field stays: it is the bridge. The shared desktop manifest carries the same field, so the two manifests are the same build in every way. Reading the server feed is a later change to `bins/server/src/updates.rs` (§6).

## 4. The open web vault

A web vault tab left open kept the old page after its server updated, until someone reloaded by chance. Now it finds out.

**The build's id.** `apps/web/build-stamp.ts` is a Vite plugin. After the files are written it hashes every file in the build, by name and content, and keeps 20 hex characters. It writes that id into the page (`<meta name="lockdown-build">`) and into `build.json` beside it (`{"build": "<id>"}`).

- The id is the page's own. A server release that didn't touch the web vault changes nothing, and asks nobody to reload.
- The same source gives the same id, so the reproducible build is still reproducible (`reproducible.yml` hashes every file in `apps/web/dist`).
- The id is public. Anyone who can load the page can hash its files. It says nothing about a person.

**Serving it.** `bins/server/src/web_vault.rs` needed no change. It already serves `.json`, and already sends `no-store` for every file outside `assets/`. The policy needed no change either: the page asks its own origin, which `connect-src 'self'` allows.

**The watch.** `src/core/build-watch.ts` holds the rules, with no DOM. `src/core/build-watch-dom.ts` wires them to the page. `main.tsx` starts it, for the vault only.

- It asks for `build.json`: at load, when the tab is shown again, when the device is back online, every 15 minutes while visible, and when a lazy file fails to load.
- The request carries no cookie and no referrer. It says nothing but its address.
- An answer that is not JSON with a short, plain `build` string means "can't tell". It never means "new". A proxy's error page changes nothing.
- A different id means a new version is ready.

**When it reloads by itself.** A reload ends everything the tab holds. The worker dies, and with it the tab's device key and its sign-in (doc 17 §4). After a reload the person signs in again with the password and the Secret Key, as a new device. A locked tab loses that too: unlocking it needs only the password, and a reload turns that into a full sign-in. So the rule is stricter than "locked":

1. **Nobody is signed in in this tab.** The core is asked (`web_state`). If it can't be asked, the answer is taken as "signed in".
2. **The moment is quiet.** The tab has been hidden for a minute, or nobody has typed, clicked or touched for five.
3. **The page shows an empty form and nothing else.** At least one field to type in, every one empty, nothing open over the page, nothing busy, and no screen holding the reload. That is the sign-in screen at rest. A sign-in waiting for a security key or for the emailed link has no field, so it is left alone.
4. **It has not already reloaded for this id.** The id is kept in `sessionStorage`. If it can't be kept, the page never reloads by itself. A bad deploy can't loop.

The core's answer takes a moment. Whatever happened during it (a key, a click, the tab shown) is weighed again before the reload.

Signed in, open or locked, the page only offers, for as long as it takes. A share link, a request link and a security-key page hold a key from the address in memory, so the watch does not run there at all.

**What a screen uses** (the visible sign is the screens' work, not this slice's):

| From `core/build-watch-dom` | What it is |
|---|---|
| `useBuildWatch()` | `{ current, latest, ready, needed, checkedAt }`, or `null` where the watch doesn't run. `ready`: offer **Reload**. `needed`: a file is gone with the old build, say "Reload to continue". |
| `reloadForNewBuild()` | The Reload action. It reloads at once: the person chose it. |
| `holdReload(why)` | A screen calls it around work a reload would lose, and calls the function it returns when done. |
| `checkBuildNow()` | Asks the server now. |

## 5. The kit

Both pins in `Cargo.toml` name commit `2a2a63f0f9a4`, the kit's crate 0.2.0. It was tag `v0.1.0` for the rules and commit `c0ff0bf` for the host helper's client. Every snds app pins that one commit.

- 0.2.0 adds a third channel, `Local` (a build a computer makes for itself). Lockdown never follows it. The admin page's `match` names it because a match must name every value, and the desktop shell passes `local: None`.
- The host helper's `UpdateRequest` gains `expect_digests`, for a stack with several images. Lockdown has one image and sends `None`. The request on the wire is the same as before.
- The diff of the kit's rules from `v0.1.0` to `2a2a63f` adds the one channel. The signature check, the hosts rule and the version rule did not change.
- `Cargo.lock` changes only in the kit's two entries.

When the kit is tagged `v0.2.0` at that commit, the two lines become `tag = "v0.2.0"`.

## 6. What is not switched, and what Sean decides

The app (`apps/desktop/src-tauri/src/updater.rs`, `MAIN_ENDPOINT`) and the server (`bins/server/src/updates.rs`, `FEED`) still read `main-build`. This slice only makes the shared feed exist.

**What a move would and would not change:**

- **The hosts rule does not change.** It checks the host, not the repository: `github.com` and GitHub's three release-asset hosts, over https, on the usual port, with every redirect checked. snds/releases is on those same hosts. A test now says so (`updates_come_only_from_github_over_https`). D681 stays true word for word.
- **Who can write the feed does change.** Today one job can, with this repository's own short-lived token. After a move, anything that holds `RELEASES_TOKEN` can. That token lasts long, and it is stored on every app repository that publishes.
- **A holder still can't push an update.** The app needs Lockdown's own signature, bound to a strictly newer version. A swapped file is refused.
- **A holder could do two things.** Break the feed, so updates stop until it is fixed. And, only if the download page moved too, change the files people download by hand, which nothing checks.

**The choices:**

| | The app reads its manifest from | The by-hand download page | Notes |
|---|---|---|---|
| (a) Recommended | `lockdown-main` on snds/releases | stays on snds/lockdown | A holder of the shared token can stop updates, and nothing more. `main-build` stays as the bridge. |
| (b) | `lockdown-main` on snds/releases | moves to snds/releases | A holder could change what people download by hand. This slice would also have to upload the dmg, rpm and Flatpak there. |
| (c) | `main-build`, as today | stays | Lockdown is a written exception. The shared feed is a mirror only. |

**After a yes to (a), in this order:**

1. Add `RELEASES_TOKEN` to snds/lockdown. Merge. Wait for one merge to publish `lockdown-main` and check it by eye.
2. Change `MAIN_ENDPOINT` to `https://github.com/snds/releases/releases/download/lockdown-main/latest.json`. `MAIN_PAGE` stays. Fixtures and words that name the old address: `updater.rs` (the header and the constant's comment), `updater/shell.rs` (a test fixture's url), `docs/plan/60-auto-update.md` (§ endpoint, and a note under D681 and D685), `docs/self-host.md` §13, `README.md` (the row for slice 59).
3. The server: teach `bins/server/src/updates.rs` the server shape (`images.server.digest`) beside `server_image.digest`, read `lockdown-server-main`, and keep reading the bridge while one is missing or stale. Then `docs/plan/61-server-updates.md` §2 and D690.
4. Keep publishing `main-build`. Copies installed before the change read it, and update to a build that reads the new feed.

## 7. Tests and what is not proven

| Where | What it shows |
|---|---|
| `packaging/update/test_update_scripts.py` (32 tests; 16 are new) | The two manifests differ only in where the files are. Each feed has its own never-backwards check. The server manifest's shape, its refusals and its exit codes; it names the image the server runs. `publish-feed.sh` against a stand-in `gh`: the first run makes a pre-release that is never "latest"; files go up first and the manifest last, then old files go; a failed upload leaves the old manifest and removes nothing; a manifest alone is a whole feed; a folder without a manifest and a token that can't read are refused; `previous` reads what is published and fails when it can't; the token is never an argument. |
| `.github/workflows/ci.yml`, `updater` (every pull request) | The throwaway-signed bundle is written into a manifest for each address, and the two are compared, as `main-channel` does on a push. |
| `apps/web/src/core/build-watch.test.ts` (20) | What counts as a build id. When it asks. A reload by itself: only signed out, only when quiet, only with the page at rest, never when the core can't be asked, never under someone's eyes, once per build, never without session storage. Reload pressed. A lazy file that failed. |
| `apps/web/src/core/build-watch-dom.test.ts` (9) | The page at rest and every way it is not. The request: own origin, past the cache, no cookie, no referrer. In the page: signed out and idle it reloads once; signed in it never does. |
| `apps/web/src/build-stamp.test.ts` (5) | The id is the same for the same files and changes with any of them; `build.json` reads back; the tag is in the head, under the name the page reads. |
| `apps/web/e2e/build-watch.spec.ts` (3, Chromium, the real server in CI) | The server hands out the page's id with `no-store`. An idle sign-in page reloads once for a new build, never with something typed, and never a second time; no request is refused by the page's policy. An error page where `build.json` should be changes nothing. |
| `updater.rs`, `updates_come_only_from_github_over_https` | The shared feed's addresses pass the hosts rule as it is. |

**Not proven here:**

- No publish ran. They run only on a push to `main`. The first merge with `RELEASES_TOKEN` set is the end-to-end test of both shared feeds. The steps were run as written against a stand-in `gh`, not against GitHub.
- The pull-request dry run has not run on GitHub.
- `cargo test`, `cargo clippy`, `cargo vet` and `cargo deny` were not run for the kit's new pin. `cargo check --locked --all-targets` passed for `lockdown-server` and for `lockdown-desktop` with the `shell` feature.
- The browser test ran here against a stand-in server that sends the policy read from `web_vault.rs`, not against `lockdown-server` itself.
- Whether the kit's own CI is green at `2a2a63f`.

## 8. Proposed decisions

| # | Decision | Status |
|---|---|---|
| D740 | **Every merge also publishes the desktop app's signed update files to `lockdown-main` on snds/releases,** after `main-build` and only with `RELEASES_TOKEN`. The second `latest.json` differs from the first only in where the files are, and has its own never-backwards check. Without the secret the step is skipped with a notice. `main-build` is published first, is never skipped for it, and is never removed. Plain version: Lockdown's updates are now also where every other app's are, and nothing already installed changes. | Proposed |
| D741 | **Only the files the manifest names go to the shared feed, and the by-hand download page stays on this repository.** The shared token can be held by more than one repository's CI. What it can reach is checked by the app's signature check. What people download by hand is not, so it stays where only Lockdown's own CI can write. Plain version: the shared place holds only files the app itself verifies. | Proposed; **flagged for Sean** |
| D742 | **The server image is named in `lockdown-server-main` on snds/releases, in the shape every server's feed has,** by its own job with `contents: read`, after the image is pushed. `server_image` in `main-build`'s manifest stays as the bridge, and servers still read it. Plain version: the server's feed now also exists in the shared place and the shared shape; no server reads it yet. | Proposed |
| D743 | **The web vault learns of a new build from `build.json`, and reloads by itself only when nobody is signed in in the tab.** Also only in a quiet moment, with an empty form on screen, and once per build. Signed in, open or locked, it only offers. Never on a link page. This is stricter than the shared contract's "only when locked": a reload ends the tab's sign-in, so a locked tab would lose its password-only unlock. No policy change and no server change. Plain version: an open web vault notices a new version, and never throws away a sign-in to get it. | Proposed; **flagged for Sean** |
| D744 | **Both kit pins name commit `2a2a63f` (crate 0.2.0), the one commit every snds app pins.** They become `tag = "v0.2.0"` when that tag exists. Plain version: every app is on the same update kit. | Proposed |
| D745 | **The app and the server still read `main-build`.** Moving the address is Sean's decision between (a), (b) and (c) in §6; (a) is recommended. It follows only after the shared feed has published once. Plain version: nothing was switched; the switch is one small change after a yes. | Open; **for Sean** |
| D746 | **Recorded gaps:**<br>• the visible "new version" sign in the web vault, and its Reload row in Settings (the screens' work);<br>• screens that hold work don't call `holdReload` yet;<br>• a reload right after sign-out, when a new build is known;<br>• the server reading `lockdown-server-main`;<br>• one manifest writer in the kit for every app, so this repository's two can go;<br>• nothing warns before `RELEASES_TOKEN` expires: the run turns red on that day.<br>Plain version: what this slice leaves for later, written down. | Proposed; recorded gap |
