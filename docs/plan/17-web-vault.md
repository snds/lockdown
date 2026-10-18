# The web vault: first slice

_Status: built, 2026-10-18, under D122 ("desktop and web first"). Implements the web vault of `03-architecture.md` §4 ("same web UI, served by the server; wasm core in the page; Argon2id in a worker") and §9, over the self-host server of `05-self-host.md`, with the login of `04-security.md` §3 and the share-link pages that `18-sharing.md` §7 and D198 left to this slice. Reuses the desktop's views (`10-desktop-shell.md`, `11-desktop-design-brief.md`) through their client interface._

## For future agent

- **TL;DR:** The web vault is `lockdown-app` itself, the same crate the desktop runs, compiled for **`wasm32-wasip1`** and run in a **dedicated worker**. The worker gives it a small in-memory file system (WASI), a clock and a random source, and answers its HTTP requests with a synchronous `XMLHttpRequest` to the page's own origin. SQLite, the built-in HTTPS client and the worker thread are behind features (`sqlite`, `http`, `native`), so the WASI build leaves them out; nothing is forked. Each sign-in enrols the tab as a device of the account's identity with the password and the Secret Key; the tab's device key is sealed under the OPAQUE export key between unlocks, so a lock wipes it and an unlock is a fresh OPAQUE sign-in. The page is the desktop's React views over a `Client` that talks to the worker. The server serves the built page at `/vault/` under a strict Content Security Policy, and hands share links and request links (`/s/{id}`, `/r/{id}`) to it.
- **Read with:** `crates/web/` (the module: `host.rs` the session around the app, `commands.rs` the command table, `links.rs` share and request links, `abi.rs` the boundary), `apps/web/src/worker/` (the worker: `wasi.ts`, `core.ts`, `http.ts`, `worker.ts`), `apps/web/src/core/web-client.ts` (the desktop's `Client` over the worker), `bins/server/src/web_vault.rs` (serving and headers).
- **Do not:** let the worker call any origin but the page's own; keep a key, a token or a plain value anywhere but the worker's memory; let a call that can release a secret skip the core's gates; make an identity inside the browser (D176); add a third-party origin, an inline script or `'unsafe-eval'` to the policy; send a link's fragment to a server.

## 1. What was built, per row

| Row | State | What is real | What is missing |
|---|---|---|---|
| `web-vault` | done (the v1 web scope) | Sign in to an account with the password and the Secret Key (a new device each time), unlock, list and search across every vault, view an item with reveal and copy behind the same gates, create and edit every kind the desktop makes (one editor from the core's templates), folders, tags, favourites, vaults, archive, trash, history, duplicates, codes (TOTP, HOTP, Steam) with copy, the quick-access palette, devices, the lock timers (idle and tab hidden), sign out; served by the self-host server at `/vault/` with CSP, COOP, COEP, CORP and SRI. | Import, export, backups and attachments (refused in words); the security center's network checks (the page may call no other origin) and its report screen; recovery with the kit and making a kit; passkeys; sharing a vault or an item; offline use; any browser but Chromium in tests. |
| `share-links` | done (browser) | `/s/{id}#key` opens in a plain browser with no account: the server redirects to the web vault's page, which reads the link's metadata, asks for the password when there is one, opens it on a click (one counted view), and shows each value only on request; files are saved from the page. Signed in, the Links screen makes a text link (lifetime, opens, optional password) shown once. | Item links and file links made from the web vault (the desktop makes them; the web opens them). |
| `request-link` | done (browser) | `/r/{id}#hash` answers in a plain browser with no account: the asker's key is fetched and checked against the hash the link pins, the answer sealed in the tab and posted once. Signed in, the Links screen makes a request link and opens its answers. | Keeping an answer as a note from the web vault. |

Surfaces the web gains on existing rows (not new rows): `login-items`, `secure-notes`, `payment-cards`, `identities`, `typed-templates`, `custom-fields`, `password-generator`, `wifi-items`, `item-expiry`, `trash-recovery`, `item-archive`, `folders`, `tags`, `multiple-vaults`, `quick-access` (the in-page palette), `smart-categories`, `duplicate-finder`, `recency-ranking`, `totp-generator`, `steam-guard-codes`, `code-display-aids`, `large-type`, `high-risk-class` (crown jewels behind the password), `e2ee`, `pake-login`, `secret-key`, `key-transparency` and `anti-rollback` (every sync's checks run in the tab), `clipboard-hygiene` (the web's own copy rule, D178), `memory-hygiene` (the web's version, §4), `official-self-host` (the server serves the web vault). Each carries `web` as a surface now.

## 2. The shape

```text
 page (React: the desktop's views, the web's sign-in and Links)          main thread
   │  postMessage {id, cmd, args}  ·  answers: JSON, or bytes (transferred)
 worker: core.ts ── WASI shim (wasi.ts): in-memory files, clock, random    dedicated worker
   │       └─ lockdown.http_send ── sync XMLHttpRequest, this origin only (http.ts)
 lockdown_web.wasm  (crates/web, wasm32-wasip1)
   commands.rs ── host.rs (sign-in, seal, unlock, lock) ── lockdown-app ── vault · sync · store(snapshot) · crypto · identity
                                                                         │
 lockdown-server  ── /v1/… (the same API the desktop uses) · /vault/ (the built page) · /s/ /r/ → /vault/s/ /vault/r/
```

**Why WASI and not `wasm32-unknown-unknown`.** The identity crate keeps its records as files and reads the clock with `SystemTime`, and so does the vault; on `wasm32-unknown-unknown` the first fails and the second panics. On `wasm32-wasip1` both are WASI calls the worker answers, so the identity crate runs unchanged (it is consumed, never forked, D12) and so does every crate above it. The module's boundary is a plain C ABI (`ld_alloc`, `ld_free`, `ld_init`, `ld_call`), not wasm-bindgen; the extension's `lockdown-wasm` keeps its own target and is untouched.

**Why a dedicated worker.** The core's network calls are synchronous (`Transport` is blocking), and a dedicated worker may block on a synchronous `XMLHttpRequest`; the page's thread never does. It also keeps every key off the page's thread: the page holds item summaries, masked details and, briefly, a value the person revealed or copied. The worker dies with the tab, and every key in it.

## 3. What compiles where (feature gates)

| Crate | Gate | Native (default) | Web (`crates/web`, no default features) |
|---|---|---|---|
| `lockdown-store` | `sqlite` | `DeviceStore = SqliteStore` | `DeviceStore = SnapshotStore`: the same records (each vault record, item envelope and head as the canonical CBOR the device sealed or signed, and blobs by hash) in one file, written whole on each change; refused as tampered when a record doesn't decode. In the web vault the file is in the worker's memory, so it outlives a lock (unlock reopens without the network) and dies with the tab. |
| `lockdown-sync` | `http` | `HttpTransport` over blocking `reqwest` | `HttpTransport` over the hook the host installs (`install_http_hook`); a hook installed natively wins too. The methods are the transport's own `Method` (GET, PUT, POST, HEAD, DELETE). |
| `lockdown-security` | `http` | `HttpFetch` over `reqwest` | `HttpFetch` refuses every call in words ("calls no server but its own"); the breach range check and the change-password probe say so. |
| `lockdown-app` | `native` | the worker thread; the three above on | `App` runs calls in place (`Rc<RefCell<Core>>`), `App::tick` runs the timers; the device name comes from `LOCKDOWN_DEVICE_NAME`; additive calls `account_login_signed_in` and `renew_session` (§4); `Paths::memory_file`/`transparency_file`. |
| `lockdown-web` | — | its native tests run on the native backends | the module |

`zxcvbn` on any `wasm32` target reads the clock through wasm-bindgen, which traps under WASI; `crates/web` turns on its `custom_wasm_env` feature, which reads it from the import `zxcvbn.unix_time_milliseconds_imported` the worker provides (D184). `ring` (through `lockdown-passkey`) compiles its C for `wasm32-wasip1` with clang. A 32-bit overflow in a constant expression in `lockdown-cxf` (`CHUNK * u32::MAX`) is a saturating multiply now.

## 4. Sign-in, the tab's device key, lock and unlock

| Step | What happens |
|---|---|
| Sign in | The tab starts with an empty identity root and data directory. The host runs OPAQUE itself (`LoginClient`), which yields the export key before the device must prove itself; the tab's device key file (the identity crate's `FileKeystore`) is sealed under `K = HKDF-SHA-512(export_key, salt = device id, "lockdown/web/device-key/v1")` (XChaCha20-Poly1305, bound to the device id and the login); the device signs the login id; the app enrols from that same sign-in (`account_login_signed_in`: the account slot opens with the export key and the Secret Key exactly as on a new machine, the device record is signed and pushed, the vaults pulled under the log's checks). The password is stretched once. |
| Lock | Whichever timer did it (idle, tab hidden, or Lock), the next call or tick finds the core locked; the host overwrites and removes the key file and forgets the session token (it lived only in the in-memory `SecretStore`). The identity's device wrap and every vault stay sealed under that key. |
| Unlock | A fresh OPAQUE sign-in with the same device: the export key re-derives `K`, opens the sealed file, the device proves itself, `App::unlock` reopens from the snapshot store, `renew_session` takes the fresh token. No Secret Key: the tab is already a device. A wrong password fails inside OPAQUE before anything is sent; a password changed elsewhere opens the account but not the seal, and says to sign in again. |
| Sign out | Everything the tab holds is wiped. The device record stays in the identity until another device revokes it; its key no longer exists anywhere (D173). |
| A failed sign-in | The tab keeps one pending device key until a sign-in succeeds, so a first attempt that reached the server (a Secret Key forgotten) is the same device as the next. Without it the log would hold a sign-in by a device the identity never listed, which every device rightly reports as a stranger (D78); the browser test found this, through the worker's one-second tick wiping the pending key (D175). |

What a copy of the worker's memory taken while locked holds: the sealed key file (no password guess without the server's OPRF), the identity's records and the snapshot store (ciphertext and signed structure, like the desktop's disk). While unlocked it holds what the desktop process holds. The webview's strings of D91 apply here too: a typed password between keystroke and send, a revealed value while drawn.

## 5. The page

- **One client interface.** The desktop's views take a `Client` (`apps/desktop/src/core/client.ts`). The web provides `webClient`: the same method names and argument shapes over the worker, so `VaultView`, the editor, the detail pane, the organization sidebar, `CodesView`, `DevicesView`, the palette, the gate and the settings, templates and lock-watch providers run unchanged (imported through Vite's `@` alias; React, `@unicron/ui`, Base UI and sonner deduplicated to the web's copies). What the web vault does not do refuses in words the views show. The desktop's Tauri client import is pointed at a stub, so no Tauri code reaches the browser.
- **The desktop changes it needed (small):** `AppShell` takes a list of screens (the desktop's five by default); three menus put their label inside a `DropdownMenuGroup`, because Base UI 1.9's `Menu.GroupLabel` throws outside one: opening "New item", "Sort" or a vault's "Keep trashed items" crashed the desktop too; a regression test opens the menu (D183).
- **The web's own screens:** sign-in (no account creation, D176; the "No account yet?" tab says where accounts come from), the password-only unlock, the web's settings (lock timers without "never", the clipboard timer, density, reveal gesture, where the tab is signed in and the trust note, sign out), Links (§7).
- **Copy (D178).** No Rust side sits outside the page, so a copy (`copySecret`, `copyCode`) brings the value to the page as bytes, which writes it with the Clipboard API, wipes the bytes and keeps only its SHA-256. After the timer (30 s, 10–90) it is cleared if the clipboard still holds it; if the browser won't let the page read the clipboard, it is cleared anyway, unless a later Lockdown copy came since.
- **Sync.** A change syncs at once and the list shows what the sync left; an open vault also syncs every minute while the tab is in view; signing out syncs first and asks before leaving unsynced changes behind.
- **Theme.** Light or dark from the system, as a class on the root from the page's own bundle (next-themes' provider writes an inline script the policy refuses).

## 6. Serving it, and the trust problem

`lockdown-server --web-vault DIR` (or `LOCKDOWN_WEB_VAULT`) serves the built page (`apps/web/dist`) at `/vault/` (and `/vault` redirects there). Every answer under `/vault/`, found or not, carries:

| Header | Value |
|---|---|
| `Content-Security-Policy` | `default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; worker-src 'self'; connect-src 'self'; img-src 'self' data:; style-src 'self'; font-src 'self'; manifest-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'` |
| `Cross-Origin-Opener-Policy` / `-Embedder-Policy` / `-Resource-Policy` | `same-origin` / `require-corp` / `same-origin` |
| `X-Frame-Options`, `X-Content-Type-Options`, `Referrer-Policy` | `DENY`, `nosniff`, `no-referrer` |
| `Permissions-Policy` | every device API the vault doesn't use off |
| `Cache-Control` | `no-store` for the page, `immutable` for the hashed assets |

Only known file types, only regular files under the directory, no dot-files, no `..`, no symlink out. The build writes Subresource Integrity hashes on `index.html`'s script and stylesheet tags; the worker fetches the module and checks its SHA-384 against the hash baked into the worker's bundle before compiling it. Sonner's stylesheet is bundled at build time instead of injected at run time, so `style-src` stays `'self'` (the build fails if sonner's code changes shape, D184). The Playwright suite fails on any Content Security Policy violation.

**The trust problem.** Whoever serves the web vault's code can change it: a hostile server could send a page that keeps the password. Nothing a page does can prove itself against the origin that sent it; SRI and the module hash protect against a changed cache, proxy or disk between the build and the browser, not against the server. Lockdown's position (D180):

- **Now:** the web vault is served by the person's own server (self-host) or Lockdown's, the same code as the release; it talks to no other origin; the module is hash-checked; the page says plainly, in Settings, that it is only as trustworthy as the server that sends it, and to prefer the desktop app for crown jewels and recovery. A crown jewel still asks for the password to reveal (the core's gate); the web vault never makes an identity or a kit.
- **Later:** publish each release's page and module hashes in the key-transparency log's checkpoints (so a server sending another build disagrees with a public record), let the browser extension check the page it opens against those hashes, reproducible builds of the page (`reproducible-builds`), and a signed-bundle route if browsers ship one (Web Bundles with signatures / "isolated web apps").

## 7. Share links and request links in a plain browser

On a server that serves the web vault, `/s/{id}` and `/r/{id}` answer `303 See Other` to `/vault/s/{id}` and `/vault/r/{id}`, and those serve the page. The redirect names no fragment, so the browser keeps the link's own (the key, or the hash of the asker's key) and never sends it (RFC 9110 §10.2.2); without a web vault the server's own no-script notice stays. The page rebuilds the link from its address, removes the fragment from the address bar and history at once, and runs everything in its worker with no account (`crates/web/src/links.rs`):

- **Share link:** the metadata first (password or not, expiry, opens left); opening is a click, because it counts a view (so a chat app's preview can't use the link up), then the password's Argon2id at the WASM floor and the access proof, the ciphertext, the payload opened in the worker; values come one at a time, as bytes, on Show or Copy; a file is saved through a `blob:` download. The link's server must be the page's own.
- **Request link:** the label the asker set (shown as text only), then the asker's key fetched and checked against the hash the link pins, the answer sealed in the tab and posted once (`lockdown-app`'s own sessionless `answer_request`).
- **Signed in,** the Links screen makes a text link (lifetime 1–30 days, opens once, 3, 10 or until expiry, an optional password) and a request link, each shown once and copied on the clipboard timer, lists them, and opens a request's answer (D182).

## 8. Carried between visits

The rollback memory and the log memory are per device, and each visit is a new device. What a visit hands the next in the same browser (IndexedDB, keyed by login name) is the minimum that keeps the server honest across visits: the server's pinned log key and newest verified tree head, and each vault's newest accepted head (seq and hash). Never a key, a value, a title or a token. On the next sign-in the log must continue that tree head (or the sign-in stops with the audit's words) and each vault's chain must reach that head ("Rollback attempt blocked"). Item envelopes are not kept across visits: a new device re-pulls them anyway (D181).

## 9. How to run and test

```sh
rustup target add wasm32-wasip1
cd apps/web && pnpm install
pnpm build:wasm                 # cargo build -p lockdown-web --target wasm32-wasip1 (optimised dev profile), copied to wasm/
pnpm typecheck && pnpm lint && pnpm test && pnpm build
cargo build -p lockdown-server -p lockdown-cli
pnpm e2e                        # PLAYWRIGHT_BROWSERS_PATH as needed; LOCKDOWN_SERVER_BIN / LOCKDOWN_CLI_BIN to point at copies
cargo run -p lockdown-server -- --data /tmp/lockdown-server --web-vault apps/web/dist   # then http://127.0.0.1:8787/vault/
```

| Where | File | Tests | What they prove |
|---|---|---|---|
| Store | `crates/store/src/snapshot.rs` | 2 | a vault saved reopens from the snapshot file with no plain field in it; a damaged file is refused as tampered |
| Server | `bins/server/src/web_vault.rs`, `tests/web_vault.rs` | 6 | only plain known files under the root; the policy has no inline script and no other origin; the page, assets and module with every header; 404s locked down too; links go on to the page with no fragment, and without a web vault the notice stays |
| Web core, native | `crates/web/src/fields.rs`, `tests/web_vault.rs` | 9 | two tabs and the desktop on one account (the Secret Key needed; a failed attempt and the ticks leave no stranger); lock wipes the key, a wrong password changes nothing, unlock is a fresh sign-in, a timer lock wipes too, sign out forgets all; a crown jewel behind the password; carried memory holds a later visit to the same log and heads, and a forged head stops it; a share link and a request link with no account, the server check; refusals in words |
| Web page | `apps/web/src/**/*.test.ts(x)` | 24 | the WASI file system (create, read, truncate, refuse a climb, wipe on remove and replace, order, the clock, stderr, `ENOSYS`); the real module on the shim in Node (state, timers, the HTTP import's requests, the pending key kept through ticks); the clipboard rule; the client's command names and shapes; link addresses; sign-in and unlock |
| Desktop | `apps/desktop/src/features/vault/new-item-menu.test.tsx` | 1 | the menu opens (the Base UI group fix) |
| End to end | `apps/web/e2e/web-vault.spec.ts` | 2 | Chromium, the real server on a throwaway database serving the built page, accounts made with the CLI: a wrong Secret Key refused, sign in, the CLI's login found, create, edit, find, lock (wrong then right password), sign out after the edit reached the server; a share link (wrong then right password) and a request link opened and answered with no account, the answer read in the vault, the key never in a request; every request to the page's origin; no CSP violation |

CI: the `web` job in `.github/workflows/ci.yml` runs all of it.

Counts: Rust 407 → 424 (`cargo test --workspace`), desktop UI 129 → 130, web UI 0 → 24, web end to end 0 → 2.

## 10. Gaps

- **What the web vault refuses for now:** import, export, backups, attachments, the security center's report and checks, the recovery kit (making, drilling, recovering), passkeys, sharing vaults and items, item and file share links, the household and emergency access. Each refuses in words; the desktop does them.
- **Offline:** none. The vault opens only through the server; the snapshot store dies with the tab.
- **Device list growth (D173):** each sign-in in a new tab is a device. Revoking the tab's own device on sign-out needs the `Approve` gate (the password again) and is left for the next slice, with "this browser" remembered so the next sign-in can retire the last one.
- **Browsers:** only Chromium runs in CI. Firefox and Safari support synchronous XHR in dedicated workers, WASI-style modules and the Clipboard API, but untested; Safari's clipboard reads ask every time.
- **The webview's strings (D91)** apply as on the desktop, plus the copy (D178): a copied value passes through the page.
- **Settings last for the tab:** a new sign-in starts from the defaults.
- **The page's size:** the module is about 10 MB (2.7 MB gzipped), the main bundle 0.7 MB; no code splitting yet.
- **Rate limits:** a person who signs in, fails, unlocks and gates many times in a minute meets the server's per-login brake (D57) in the page's words.

## 11. Host-contract gaps (unicron and the identity crate)

- **The identity crate's Argon2id floor is the creating platform's (D176).** An identity made on `wasm32` records the WASM floor (32 MiB, 1 lane) for its kit; the server's record check and every desktop refuse it as below theirs. Proposal to the crate: `check_floor` accepts the lowest floor the crate lists, or the profile records the platform it was made on and readers apply that platform's floor; until then the web vault makes no identity.
- **`@unicron/ui`'s `DropdownMenuLabel` is Base UI's `Menu.GroupLabel`,** which must sit inside a group; unicron could export a label that works on its own, or document the group. The desktop now wraps its labels.
- **unicron's `Toaster` uses `next-themes`' `useTheme`** (fine without a provider), and sonner injects its stylesheet at run time, which a strict `style-src` refuses; unicron could import sonner's stylesheet itself.
- **`zxcvbn` on `wasm32` assumes wasm-bindgen;** its `custom_wasm_env` feature is the way out for a WASI host.

## 12. Proposed decisions

For the coordinator to record. **Flagged** rows are user-visible and need Sean.

| ID | Decision | Owner |
|---|---|---|
| D170 | **The web vault runs `lockdown-app` itself, built for `wasm32-wasip1`, in a dedicated worker**, behind a plain C ABI, with a WASI shim of an in-memory file system, the clock and the random source; not `wasm32-unknown-unknown` with wasm-bindgen, where the identity crate's files and the clock fail. Plain version: the browser runs the same core as the desktop, not a copy. | Accepted by default under D38 |
| D171 | **Feature-gate, never fork:** `lockdown-store/sqlite`, `lockdown-sync/http`, `lockdown-security/http` (default on); `lockdown-app/native` turns them on; `App` runs inline on WASI with the host ticking the timers; HTTP through `install_http_hook`. Plain version: the native-only parts switch off for the web build. | Accepted by default under D38 |
| D172 | **The web stand-ins:** the store is `SnapshotStore` (the SQLite file's records, ciphertext only, in one file in the worker's memory: it outlives a lock and dies with the tab); HTTP is a synchronous request from the worker to the page's own origin and no other; the session token lives only in memory. Plain version: nothing readable is kept in the browser, and the page talks only to its own server. | Accepted by default under D38 |
| D173 | **Each sign-in enrols the tab as a device** ("Lockdown web vault · Chrome"); signing out forgets its key, and the device stays in the list until another device revokes it. Plain version: each browser you sign in from shows up in your devices. | **Flagged for Sean** (the device list grows) |
| D174 | **Lock and unlock in the browser:** lock wipes the tab's device key (sealed under a key from the OPAQUE export key) and forgets the session; unlock is a fresh sign-in with the account password only, which needs the server. Plain version: the web vault asks for your password to unlock, and won't unlock offline. | **Flagged for Sean** |
| D175 | **A tab keeps one pending device key until a sign-in succeeds**, so retries are one device and the log never holds a sign-in by a device the identity doesn't list. Plain version: a mistyped Secret Key doesn't set off the "unknown device" alarm. | Accepted by default under D38 |
| D176 | **The web vault signs in to accounts and never creates one**; accounts come from the desktop app or the CLI, until the identity crate accepts the WASM Argon2id floor on other platforms (§11). Plain version: make your account in the app; then use it anywhere. | **Flagged for Sean** |
| D177 | **The web's lock timers** are the core's (idle 10 minutes, tab out of view 5 minutes, by default) with no "never" for idle; settings last for the tab. Plain version: an open browser tab locks on its own. | **Flagged for Sean** |
| D178 | **Copy in the browser** goes through the page (the Clipboard API), keeping only the value's SHA-256; cleared after the timer if the clipboard still holds it, or anyway if the browser won't let the page look, unless a later Lockdown copy came since. Departs from D85/D197 (copy never in the webview) because a page has no other side. Plain version: copied passwords still clear themselves; in a browser, that may also clear something you copied after. | **Flagged for Sean** |
| D179 | **Serving:** `--web-vault DIR` at `/vault/`, the headers of §6 on every answer, only known types under the directory, SRI on the page's tags, the module's SHA-384 checked in the worker, no inline script or style, no other origin. Plain version: the page can load nothing but its own files. | Accepted by default under D38 |
| D180 | **The trust statement for the web vault:** it is only as trustworthy as the server that sends it; now: own-origin only, hash checks, a plain sentence in Settings and "prefer the desktop for crown jewels and recovery"; later: release hashes in the key-transparency checkpoints, the extension checking the page, reproducible page builds. Plain version: the web vault is for convenience; the app is the one to trust most. | **Flagged for Sean** (positioning) |
| D181 | **What a visit carries to the next in a browser:** the server's pinned log key and newest tree head and each vault's newest accepted head, in IndexedDB by login; never a key, value, title or token; item envelopes are re-pulled. Plain version: the browser remembers just enough to notice a server going back in time. | Accepted by default under D38 |
| D182 | **Share and request links in a plain browser:** `/s/{id}` and `/r/{id}` hand over to the web vault's page by a redirect with no fragment; opening a share link takes a click (it counts a view); the fragment leaves the address bar at once; the link's server must be the page's own; signed in, the Links screen makes text and request links. Plain version: a link you're sent opens in any browser, and someone without Lockdown can answer your request there. | **Flagged for Sean** (the click to open) |
| D183 | **One client interface for both shells:** the web vault draws the desktop's views through the desktop's `Client`, refusing in words what it doesn't do; `AppShell` takes its list of screens; menu labels sit inside their groups (Base UI 1.9; it was a crash on the desktop too). Plain version: the web vault looks and works like the desktop app. | Accepted by default under D38 |
| D184 | **Two third-party adjustments for the strict page:** `zxcvbn`'s `custom_wasm_env` (the clock from the worker) and sonner's stylesheet bundled at build time instead of injected (the build fails if sonner changes shape). Plain version: none; the page keeps its no-inline rule. | Accepted by default under D38 |
