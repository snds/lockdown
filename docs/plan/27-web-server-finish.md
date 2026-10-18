# 27: Web and server finishing

_Slice: `slice/web-server-finish`, built 2026-10-18 under D122 ("desktop and web first"). Rows: `self-host-push` (the web vault's half), `official-self-host` (the last restore in the clients, a passkey on the admin page, a drill that signs a client in), `key-transparency` (log mirrors), `extension-coverage` (pairing and a fill in Firefox's run). Proposed decisions D320–D332 (§9). Operator guide: [`docs/self-host.md`](../self-host.md) §3, §11, §12._

## For future agent

- **TL;DR:** The web vault's **worker holds the live channel** itself (`fetch` on `/v1/events`), syncs on an event exactly as the desktop does, and closes within a second of a lock or a sign-out; the token never reaches the page. The clients show **the server's last successful restore drill**, read from a new endpoint. Each restore drill now **signs a real client in to the restored copy and reads an item back**, with an account the server keeps for that. The admin page takes **passkeys** beside the operator key. A server can run as a **mirror** of another server's public log (or write it for a static host), and every sync **compares the log the device verified with each mirror** it knows; a split view is said in words in Status, Settings and a toast. The extension **pairs from its inline menu**, which is how Firefox's run now drives pairing and a fill without the popup.
- **Read with:** `apps/web/src/worker/{live,worker,http}.ts`, `crates/web/src/commands/server.rs`, `crates/app/src/{live,server_checks}.rs`, `crates/app/src/session/mirrors.rs`, `crates/sync/src/mirror.rs`, `bins/server/src/{mirror,drill_account,admin_passkeys,drills}.rs`, `apps/desktop/src/features/server/`, `extension/src/pages/frame.ts`, the tests `apps/web/src/worker/live.test.ts`, `apps/web/e2e/live.spec.ts`, `crates/app/tests/log_mirrors.rs`, `bins/server/tests/web_server_finish.rs`, `crates/web/tests/web_server_finish.rs`, `extension/test/{firefox,menu-pairing}.spec.ts`.
- **Do not:** answer a `worker_…` command from the page; let the worker reach another origin except a mirror's `GET /v1/kt/mirror` with no token; treat a mirror that can't be reached, copies another server or stopped copying as agreement; let the drill account hold anything but its one marker login; show the drill report as a guarantee (it is the server's word); add a second inline script to the server's pages.

## 1. What was built, per row

| Row | State | What is real | What is missing |
|---|---|---|---|
| `self-host-push` | done (desktop and web; mobile relay deferred by D122) | The web vault's worker opens the live channel itself while the vault is open and signed in (§2): pings, shares and edits from other devices show in an open tab within seconds, without a reload; reconnect with a growing pause (2 s to a minute); closed at once on lock and sign-out; a session ended elsewhere locks the tab. The tab's minute sync now runs only while the channel is down. The web vault also gets the desktop's **Signed in** list (session commands under the desktop's names). | The mobile push relay (D229). |
| `official-self-host` | done (support is not code) | **Last successful restore** in desktop Settings and the web vault's Settings, read from `GET /v1/server/restore-drill` (§3), in the core's words. **Passkeys on the admin page** (§3.2): added while signed in with the key, then **Sign in with a passkey**; user verification required both times. **A drill that signs a client in** (§3.1): a restore-drill account of the server's own, a scratch server over the restored copy, `lockdown-app` signing in with the password, syncing under every check and reading the login back. | Support (not code); update availability; the two-media check of 05-self-host §4; cross-publishing checkpoints to a Lockdown-run log. |
| `key-transparency` | done | **Mirror mode** in the server (`--mirror-of`, `--mirror-out` for a static host), the mirror's own continuation check, **mirrors named** by the account server (`--kt-mirror`, `GET /v1/kt/mirrors`) and remembered once seen, **mirrors added in Settings**, the **comparison in the core** after every sync's last log check (`lockdown_sync::mirror::compare`), and the **warning** in desktop Status and Settings and the web vault's Settings, plus a toast on a split. Tested with two servers in one process (§4.4). | A public mirror run by Lockdown; in the web vault, mirrors the person adds are fetched only when the server's policy names their origin (§4.3). |
| `extension-coverage` | partial (store submission and Safari remain) | Pairing starts from the inline menu on any sign-in page (§5); Firefox's Playwright run pairs (the same code in the menu and the app, approved in the app) and fills a login through the content script's menu, green in CI's first round with `LOCKDOWN_REQUIRE_FIREFOX=1`; the same route has a Chromium spec. | Store submission (needs the listings), Safari (OS-level, D122). |

## 2. The web vault's live channel (`apps/web/src/worker/live.ts`)

```text
 page (React)  ←─ postMessage {id: 0, event: "live", notice} ─┐
                                                            worker
   LiveChannel ── fetch GET /v1/events (Bearer, credentials: omit, same origin) ── SSE parser
        │  target ← core "worker_live_target" (url, token) after every call and every tick
        │  sync event → gather 250 ms → core "worker_live_sync" (not activity) → notice "synced"
        │  alerts / sessions → notice; session_ended → core "worker_live_ended" → notice
```

- **Where the token is.** `worker_live_target` hands the worker the session token, which the worker's HTTP hook already carried on every request. `worker.ts` refuses every `worker_…` command from the page, so the page never asks for it. The channel goes only to the page's own origin (checked here, and by `connect-src`).
- **Open only while it should be.** After every command and every one-second tick, the worker shows the channel the core's target: none (locked, signed out, stopped) or another token closes the stream at once; an unlock wakes the loop rather than waiting for its three-second look. A closed-by-lock stream is not a drop: no notice, no pause.
- **Drops.** A stream that ends, errors, refuses (non-401) or goes quiet for 65 s (three keep-alives) is a drop: `disconnected` with the words, then a pause that starts at 2 s and doubles to a minute, reset by a stream that opened. A 401 or a `session_ended` event marks the token ended; the worker never knocks with it again, and the core forgets it (`App::live_session_ended`), so the next call asks for the password. The page locks the tab and says why.
- **Bursts and the idle lock.** Events within 250 ms make one sync. That sync is `App::live_sync`: every check `sync` has, but no activity for the idle lock, as on the desktop (D222).
- **The page.** `WorkerLink.onLive` feeds the desktop's `LiveClient` interface (`web-live-client.ts`), so the desktop's `useLive` and its Signed in section work unchanged. The vault list redraws on `synced`. The minute sync of D180 now runs only while the channel is down.

## 3. The server, for the person who runs it

### 3.1 The restore drill signs a client in (`drill_account.rs`, `drills.rs` step 6)

The table checks of D297 prove the backup holds the rows. Step 6 proves a person could use them:

1. **The account.** With `--drill-client on` (the default with `--backup-dir`), before the first scheduled backup the server makes a restore-drill account: a login name `lockdown-restore-drill-<12 hex>`, a random password, an identity and a vault made by `lockdown-app` (the desktop's own core), holding one login whose password is a random marker. It is registered on the live server through a listener on `127.0.0.1` that serves the live state for a few seconds; with sign-ups closed, the server invites that one name for an hour first. Its device files, password and marker are kept user-only in `<data>/restore-drill/`. The admin page marks the account.
2. **The drill.** After the table checks, the drill starts a scratch server over the restored database and the restored key file (its own loopback listener), copies the account's device folder to the scratch folder, and signs in exactly as a device that already holds the identity does (OPAQUE with the password; the identity's records; the log's tree head pinned on first sight and the account's own entries audited; the vault's head chain), syncs, and reveals the login. Only the marker coming back passes.
3. **Order.** A backup made before the account existed passes its table checks with the sentence "the restore-drill account is newer than this backup, so a client signs in from the next one". A failure names the step ("signing in to the restored copy failed: …").

**Why the client, not a copy of its flows.** `lockdown-server` now links `lockdown-app` (no new third-party crate; the binary grows). The point of the step is that the code people run reads the backup, so the drill uses that code rather than a third copy of the sign-in.

### 3.2 Passkeys on the admin page (`admin_passkeys.rs`)

| | |
|---|---|
| Add | Signed in with the operator key: Passkeys → Add a passkey. `POST /admin/passkeys/options` (JSON, the page's anti-forgery token) → `navigator.credentials.create` (ES256 or Ed25519, resident key preferred, user verification required) → `POST /admin/passkeys/add`, checked by `lockdown_passkey::verify::verify_registration` (RP ID = the request's host, origin `https://` it or `http://` on this machine); a key that didn't verify the person is refused. At most ten. |
| Sign in | The sign-in page's **Sign in with a passkey**: `POST /admin/passkey/options` (the operator brake) → `navigator.credentials.get` → `POST /admin/passkey/sign-in`, checked by `verify_assertion` against the stored COSE key and counter, user verification required; a right answer sets the same `__Host-` cookie the key does. Failures are recorded and slowed like a wrong key. |
| Kept | Credential id, COSE key, counter, name, dates, in `operator_settings` (so backups carry them). Challenges five minutes in memory, once, per kind. |
| Script | The pages stay script-free but for one inline script (`admin_passkeys::SCRIPT`), allowed by its SHA-256 with `connect-src 'self'`; no inline handlers. |
| The key | Stays: it adds the first passkey and is the way back in when every passkey is lost. `admin clear-key` closes the page to passkeys too. |

### 3.3 The last successful restore in the clients

`GET /v1/server/restore-drill` (any signed-in account; CBOR `RestoreDrill {scheduled, last_run_at?, last_passed?, last_success_at?, client_read_back}`), read from the operator's log, never made up. `App::restore_drill` turns it into one sentence ("The server last proved its backup restores on 2026-10-18, with a sign-in to the restored copy that read an item back." / "No restore drill has passed on this server yet." / "This server doesn't run restore drills. …" / "… The newest drill, on …, failed: tell whoever runs the server."). An older server (404) is "doesn't report restore drills". Desktop Settings and the web vault's Settings show it under **This server**, with a line saying it is the server's word. It is information, not a check: the vault's guarantees never rest on it.

## 4. Log mirrors (`lockdown_sync::mirror`, `bins/server/src/mirror.rs`, `session/mirrors.rs`)

### 4.1 The mirror

`--mirror-of URL` makes a server copy another's signed tree heads every `--mirror-every-secs` (300): fetch `/v1/kt/sth`, check the signatures under the log keys pinned on first sight (Ed25519, and the post-quantum ML-DSA-65 key of doc 28 from the first time the server offers one), and keep the head only if it continues the last one copied (same size and root, or a consistency proof from the upstream that verifies). It keeps the newest 256 verified heads in `<data>/kt-mirror.cbor`, serves the newest at `GET /v1/kt/mirror` (CBOR `MirrorHead {v, upstream, log_key, pq_log_key?, sth, checked_at, copied, problem?}`, `Access-Control-Allow-Origin: *`, `max-age=30`), and with `--mirror-out DIR` writes the same bytes to `DIR/v1/kt/mirror` for a static host. A shorter log, a second root at one size, a failed proof, a new log key, or a post-quantum key changed or dropped is a **problem**: recorded in words, logged, sticky until the operator deletes the file, and the last good head keeps being served. A head not signed by the pinned key proves nothing either way and is ignored. The mirror never sees a leaf: it learns sizes and roots.

### 4.2 The comparison (`compare`)

After every sync's second log check, with the head the device just verified:

| Check | Verdict |
|---|---|
| Mirror format not `v1`, or a mirror of another server | unusable |
| The mirror's log key isn't the one this device pinned | **split** |
| The mirror's post-quantum log key differs from this device's, or one of the two has none | **split** |
| The mirror's head isn't signed by those keys (both, when it names a post-quantum one) | unusable |
| Same size, other root | **split** |
| Different sizes: a consistency proof, smaller to larger, from the account server | verifies → go on; doesn't verify, or the server refuses to give one (4xx) → **split**; can't be had (5xx, network) → unreachable |
| The mirror reports a problem of its own | **split** ("seen by the mirror") |
| Its newest copy is more than seven days old | unusable |
| Otherwise | agrees (with the mirror's size and time) |

Two heads signed by one key that the server can't connect are the server's own proof that it signed two histories; a mirror can't forge one. The device's own remembered head and the citations in vault heads (D80) stay the first line; the mirror catches what they can't: a new device with nothing remembered, an account with one device.

**Which mirrors.** The person's (`Settings.kt_mirrors`, changed only through `kt_mirrors_set`: `https://`, or `http://` to this computer; at most eight), then the ones the server names at `/v1/kt/mirrors`, **remembered once seen** per server in `kt-mirrors.cbor`, so dropping one from the server's list doesn't stop the check. No token goes to a mirror. The last verdict per mirror is kept there too.

**What the person sees.** Each mirror's line joins the sync's lines ("mirror https://… : agrees (its copy has 12 entries)"); `TransparencyView.mirrors` carries the verdicts to Status (desktop: a proof line, or the split warning); Settings → This server lists each mirror with who named it and its word, and takes the person's own. A split is a warning in words with what to do ("This server's public log doesn't match a mirror. The server signed one history for you and another for the world. Your vault is unchanged, but don't share keys with anyone through this server until it's explained…"), and a toast that stays until dismissed. **The sync itself stands** (D328 asks Sean whether it should stop sharing).

### 4.3 The web vault and mirrors

The worker's one cross-origin request is a mirror's head: `GET`, no token, no body, no credentials, the path `/v1/kt/mirror` exactly (`isMirrorRead`). The browser still enforces the page's policy: the server adds the origins of the mirrors it names to `connect-src` on every `/vault/` file (`mirror::widen_csp`). So in the web vault the server's named mirrors are compared; a mirror the person adds there is checked only if the server names its origin too, and otherwise reports "couldn't read its copy". That is the web vault's standing limit (its code comes from the server, doc 17 §6): the desktop app checks any mirror.

### 4.4 Tested

`crates/app/tests/log_mirrors.rs`, two servers in one process: the account server on disk and a second server in mirror mode with a static folder. Before the first copy the mirror can't vouch (unreachable, not agreement); after it, a sync agrees and the static file holds the same head; the mirror follows a log that grows. The server rewrites its first leaf and stops naming the mirror: a new device (nothing remembered, the mirror added in Settings) gets **Split view** on the sync, in Status after a lock, and in `kt_mirrors`; the old device's own memory refuses the rewrite first; the mirror's own next copy records the problem and keeps the last good head. A mirror whose copy names another post-quantum log key, or none, is a split. A mirror of another server is unusable; a plain-`http` mirror address is refused in words; the general Settings save never drops the mirrors; a mirror the server stops naming is still compared. Server-side: `bins/server/tests/web_server_finish.rs` (the endpoints, the policy widened to the named origins only) and the unit tests in `mirror.rs` and `lockdown_sync::mirror` (continuation, a forged history, the address rules).

## 5. Pairing from the inline menu (`extension/src/pages/frame.ts`)

Doc 24 §6 recorded why Firefox's run couldn't pair: pairing started in the popup, and Playwright's Firefox can't drive a `moz-extension:` page. Pairing now also starts in the menu the content script opens on any sign-in field: an unpaired browser's menu says "Pair this browser with Lockdown to fill here." with a **Pair this browser** button, armed like every choice in the menu (on screen and uncovered for 400 ms, a trusted click; the background measures the menu's host on Firefox, where IntersectionObserver v2 is missing). The click sends `menu_pair` (frame-only, nonce-checked), which runs the popup's own `link.pair()`; the menu shows the code with its label and asks the person to compare it with the app and approve there, polls `menu_link` each second, and once paired redraws as the ordinary menu. Nothing about pairing itself changed: a fresh key, the same code on both sides, the app's approval. A page can make the menu appear but can't click inside it, and a pairing it somehow started would still need the person in the app.

`extension/test/firefox.spec.ts` now pairs and fills in Firefox (CI round 1: "pairing request from Firefox: {name: Firefox on Linux, same: true}", the login filled). `extension/test/menu-pairing.spec.ts` runs the same route in Chromium, so it is checked on machines without Playwright's Firefox.

## 6. Wire and shape additions (D62)

All additive. `lockdown_sync::wire::RestoreDrill`; `lockdown_sync::mirror::{MirrorHead, KtMirrors, MirrorVerdict}`; `Settings.kt_mirrors` (missing reads as empty); `TransparencyView.mirrors` (desktop `types.ts` field optional); `LiveNotice` unchanged. New Tauri commands at the end of the handler list: `server_restore_drill`, `kt_mirrors`, `kt_mirrors_set`; the web vault's table gains the same three, the desktop's session commands (`session_list`, `session_end`, `session_end_others`) and the worker's `worker_live_target`, `worker_live_sync`, `worker_live_ended`. The extension's messages gain `menu_pair` and `menu_link` (internal; no schema). No wasm boundary schema changed. New routes at the end of the server router: `/v1/kt/mirror`, `/v1/kt/mirrors`, `/v1/server/restore-drill`; the admin router gains `/admin/passkeys{,/options,/add,/remove}` and `/admin/passkey/{options,sign-in}`. New server flags: `--drill-client`, `--mirror-of`, `--mirror-every-secs`, `--mirror-out`, `--kt-mirror`. No table was added (passkeys live in `operator_settings`).

## 7. Running and testing it

```sh
# A mirror of vault.example, and vault.example naming it
lockdown-server --data /data-mirror --mirror-of https://vault.example --mirror-out /srv/www/mirror
lockdown-server --data /data --kt-mirror https://mirror.example

# Scheduled backups whose drills sign a client in (the default with --backup-dir)
LOCKDOWN_BACKUP_KEY_FILE=/run/secrets/backup lockdown-server --data /data --backup-dir /data/backups

# Tests
cargo test -p lockdown-sync mirror
cargo test -p lockdown-app --test log_mirrors          # two servers in one process: agree, split, remembered
cargo test -p lockdown-server --test web_server_finish # the drill's client check, admin passkeys, the mirror endpoints
cargo test -p lockdown-web --test web_server_finish    # the worker's commands natively
cd apps/web && pnpm vitest run src/worker               # the live channel, the mirror read rule
cd apps/web && pnpm e2e e2e/live.spec.ts                # a change on another device shows in an open tab
cd apps/desktop && pnpm vitest run src/features/server
cd extension && pnpm exec playwright test test/menu-pairing.spec.ts test/firefox.spec.ts
```

## 8. Gaps

1. The mobile push relay (D229) stays deferred with mobile (D122).
2. In the web vault, mirrors the person adds are reachable only when the server names their origin (§4.3).
3. A split is a warning; sharing and sync go on (D328).
4. No public mirror is run by Lockdown, and self-hosted servers don't cross-publish checkpoints to one (05-self-host §6); `--mirror-out` makes a mirror on any static host instead.
5. The admin page's passkey ceremonies are tested at the HTTP layer with a software authenticator (`lockdown_passkey::make_credential`), not in a browser.
6. The restore-drill account's sign-in runs the real Argon2id floor once a week (about a second); the server binary now carries the client core.
7. Store submission and Safari for the extension (`extension-coverage`).

**unicron host-contract gaps:** none hit. The new Settings section uses `Button`, `Field`, `FieldLabel`, `FieldDescription`, `Textarea` and `Icon`; the split warning is plain markup with the design tokens' destructive colour; the admin page's passkey parts are the server's own plain HTML, as before.

## 9. Proposed decisions

For the coordinator to record. **Flagged** rows are user-visible and need Sean.

| ID | Decision | Owner |
|---|---|---|
| D320 | **The web vault's worker holds the live channel**: `fetch` with a streamed body on `/v1/events`, same origin only, the token from a worker-only call (`worker_…` commands are refused from the page); closed within a second of a lock or sign-out (the worker shows the channel the core's target after every call and tick); 2 s to one-minute backoff; 65 s read timeout; bursts of 250 ms folded into one quiet sync that isn't activity; the minute sync only while the channel is down. Plain version: an open web vault tab shows changes from your other devices by itself, like the desktop app. | Accepted by default under D38; **flagged for Sean** (tabs now stay connected while unlocked, as D222 did for the desktop) |
| D321 | **The web vault gets the Signed in list**, and a session ended from another device locks the tab with a sentence. Plain version: you can see and end your sessions from the web vault too, and a tab you signed out remotely stops at once. | **Flagged for Sean** |
| D322 | **`GET /v1/server/restore-drill`** for any signed-in account: whether drills are scheduled, the newest drill's time and result, the last successful one, and whether it signed a client in; from the operator's log. Clients show one sentence in Settings → This server, marked as the server's word. Plain version: your app tells you when your server last proved its backups work. | **Flagged for Sean** (what accounts learn about the server; the wording) |
| D323 | **The restore drill signs a client in**: the server keeps one restore-drill account of its own (made once through its loopback address, invited through closed sign-ups, files in `<data>/restore-drill/`, one marker login); each drill starts a scratch server over the restored copy and `lockdown-app` signs in, syncs under every check and reads the login back; on by default with scheduled backups (`--drill-client off`); `lockdown-server` links `lockdown-app`. Plain version: the server's weekly drill now proves a real app could sign in to the restored copy and read your data back. | **Flagged for Sean** (an account the server makes; the server binary carries the client); audit item |
| D324 | **Passkeys on the admin page**: WebAuthn with user verification required to add and to sign in, up to ten, kept in `operator_settings`, the operator key kept as the way in and back; `admin clear-key` closes passkeys too; one inline script allowed by its hash, `connect-src 'self'`. Amends D294's "no passkey yet". Plain version: whoever runs the server can sign in to its admin page with a passkey. | **Flagged for Sean** (operator-facing) |
| D325 | **The mirror format and endpoints**: `GET /v1/kt/mirror` (CBOR `MirrorHead`, open to any origin, 30 s cache), `GET /v1/kt/mirrors` (the names a server gives), `--mirror-out` writing the same bytes for a static host. Plain version: anyone can keep a copy of a server's public receipt roll, on a server or as a plain file. | Accepted by default under D38 (structure) |
| D326 | **A mirror keeps a head only when the server proves it continues the last one**, signed by both pinned log keys once a post-quantum one is offered; a problem (going back, a second root, a failed proof, a new key, a post-quantum key changed or dropped) is sticky, logged and served beside the last good head; heads not signed by the pinned key are ignored. Plain version: a mirror never follows a server that rewrote its history, and says so. | Accepted by default under D38; audit item |
| D327 | **The client's comparison**: the same pinned log keys (the post-quantum one included: another, or one side without, is a split), valid signatures, and a consistency proof from the account server between the two heads, smaller to larger; a proof that fails or is refused (4xx) is a split; a network or 5xx failure is "not checked"; a mirror's own problem is a split; a copy older than seven days can't vouch. Plain version: your app holds the log your server shows you against copies kept elsewhere, and a server keeping two histories can't explain the difference away. | Accepted by default under D38; audit item |
| D328 | **A split view is a warning, not a stop**: the words in the sync's lines, Status, Settings and a toast that stays; the vault stays readable and syncing. Plain version: Lockdown tells you loudly if your server shows you a different history from everyone else; it doesn't lock you out. | **Flagged for Sean** (whether a split should also stop sharing keys through that server) |
| D329 | **Which mirrors a device checks**: the ones you add in Settings (`https://`, at most eight, changed only through their own call) and the ones the server names, remembered per server once seen; the last word per mirror kept on the device. Plain version: once your server names a mirror your apps keep checking it, even if the server later stops naming it. | **Flagged for Sean** (a new setting) |
| D330 | **The web vault and mirrors**: the server adds its named mirrors' origins to the web vault's `connect-src`; the worker's only cross-origin request is a mirror's head (GET, no token, the mirror path); mirrors added in the web vault are checked only where the policy allows. Plain version: the web vault checks your server's named mirrors too; the desktop app can check any. | Accepted by default under D38 |
| D331 | **Pairing from the inline menu**: an unpaired browser's menu offers "Pair this browser", armed like every menu choice, running the popup's pairing (same code on both sides, approved in the app); Firefox's run pairs and fills this way. Plain version: you can pair the extension from any sign-in page, not only from the toolbar button. | **Flagged for Sean** (a new entry point) |
| D332 | **Shape additions** (D62): `RestoreDrill`, `MirrorHead`, `KtMirrors`, `MirrorVerdict`, `Settings.kt_mirrors`, `TransparencyView.mirrors`, three Tauri and web commands at the ends of their tables, the worker-only and session commands in the web table, the extension's `menu_pair` and `menu_link`, five server flags; no table added. Plain version: everything new was added beside what was there. | Accepted by default under D38 (structure) |
