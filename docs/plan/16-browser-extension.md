# The browser extension: first slice (MV3, Chromium and Firefox)

_Status: built, 2026-10-18, under D122 ("desktop and web first"). Implements `03-architecture.md` §4 (the extension's thin-client mode) and §7 (the local channel, peer-authed), `04-security.md` §12 (clickjack-hardened autofill) and §13 (local IPC peer authentication), over the matcher boundary of `09-extension-boundary.md`. Companion to `10-desktop-shell.md` (the app the extension talks to), `13-security-center.md` (whose lookalike rule the extension reuses) and `15-organization.md` (several vaults, several websites per login, recency)._

## For future agent

- **TL;DR:** The extension is a thin client of the **running, unlocked desktop app**. The browser starts `lockdown-native-host`; the host connects to a Unix socket the app listens on in Lockdown's data directory; each side checks the other's process identity with the identity crate's `ProcessIdentityVerifier` (D4, D33); the extension proves a pairing the person approved in the app with a code they compared; the core decides every fill again and hands back only the value being filled. The content script measures, the extension's own wasm matcher pre-checks, the app decides. The menu lives in an extension-origin frame inside a closed shadow root in the top layer and arms only while IntersectionObserver v2 says nothing covers it.
- **Read with:** `extension/README.md` (how to build, run, test), `crates/app/src/browser.rs` (the core's half), `apps/desktop/src-tauri/src/browser/` (the endpoint and pairing), `crates/native-messaging/` (what both ends agree on), `bins/native-host/` (the host), `extension/src/` (the extension).
- **Do not:** let the host parse or keep anything beyond a request's `id`; answer a request before the connection proves its pairing; send a value before the core's `fill`; put vault data in the extension's storage; make `popup.html` web-accessible; trust a frame message that does not carry a nonce its own tab's content script registered.

## 1. What was built, per row

| Row | State | What is real | What is missing |
|---|---|---|---|
| `extension-coverage` | partial | One TypeScript source builds `dist/chromium` (MV3 module service worker; fixed dev id from the manifest's public key) and `dist/firefox` (MV3 event page; `lockdown@snds.design`). Popup: pairing, the logins for the page in front, search over item summaries (the desktop list's rule, D118; archived and trashed left out, D149), copy through the desktop's clipboard path. | Store packaging and signing (D122), the Safari Web Extension, the popup on unicron (§9), Firefox driven by a test (§8). |
| `url-matching` | done (extension) | Every candidate is decided in the core over every website of every live login in every open vault (`page_matches`), strongest first, and decided again by the extension's wasm `decide` before it is offered; the fill is decided a third time in the core. | The editor's per-website rule already exists (D154); nothing left for this slice. |
| `inline-menu` | done | A `div` host with a **closed** shadow root holding an `iframe` of `frame.html` (extension origin), raised with `popover="manual"` into the top layer; placed under or over the field and never past the frame's edge; closes on Escape, on blur, when the person types, or when it has nothing to offer. | Keyboard navigation inside the menu beyond Tab/Enter; a generated-password offer on sign-up fields. |
| `clickjacking-defense` | done | Every fill: the content script measures the field by the D67 rule (the harness's rule, ported), the background runs the wasm `fill_check` with the browser's own `top` (`tabs.get`) and frame URL (`sender.url`), the core runs `fill_check` again. The menu's buttons stay disabled until the frame has been visible and uncovered for 400 ms (IntersectionObserver v2 `trackVisibility` on Chromium), go disabled again when covered, and ignore untrusted events; on Firefox the content script hit-tests the menu host instead. The red team runs on the real fill path (§8). | IOv2 does not exist in Firefox; the fallback is a hit test, weaker against a `pointer-events:none` decoy over the menu itself (§7). |
| `autosave` | done | A trusted submit, submit-button click or Enter in a password field captures the form's username and new password (the `new-password` field, or the last of several); the password goes straight to the app (`offer_save`), which compares it in the core (`save_proposal`: new, update, same) and holds it in memory for three minutes; the extension keeps only the offer id. The prompt is an extension-origin frame in the top frame, shown at once or on the next page's load; Save / Update / Not now. A change-password form without a username uses the one login saved for the site. | "Never for this site"; detecting a sign-up from its fields alone (no submit). |
| `fill-hotkey` | done | `fill-login` (Ctrl+Shift+L, ⌘⇧L on macOS): fills the best login into the focused field's form, the next one on each press within 8 s; on a code, card or address field it opens the menu, where the second action is. | Firefox's shortcut is exercised only by its manifest test. |
| `totp-autofill` | done | A one-time-code field (`autocomplete=one-time-code`, the usual names, or four to eight one-character boxes) offers the page's logins that hold a code; the first tap answers `confirm`, the second fills the code valid now from the core's `code_now`; split boxes get one digit each. | Filling the code on its own right after a password fill (today it is the field's own menu). |
| `basic-auth-fill` | done (Chromium tested) | `webRequest.onAuthRequired` (Chromium `asyncBlocking` with `webRequestAuthProvider`; Firefox `blocking` with a promise): top-level navigations only, never a proxy, never twice for one request; the app answers only when exactly one login is saved for exactly that host, through the same `fill` decision. | Sub-resource and proxy challenges go to the browser's own prompt by design. |
| `card-address-fill` | done | Card fields (`cc-*` and the usual names, split month/year, `MM/YY` or `MM/YYYY`) and identity/address fields; a card asks for the second deliberate action (`fill_check`), an identity fills on one; both only on a secure page (https or this machine), and both refused on a page that imitates a known site. | Selects for country by code, address line 2, phone formats; card brand detection on the page. |
| `lookalike-warning` | done (extension) | A page whose host imitates a saved login (the matcher's `lookalike`) or a well-known site (the security center's rule, `lockdown_app::security::imitated_site`) gets a hard stop in the menu and a banner in the popup, which also makes a copy take a second, explicit "Copy anyway". | The full confusables table (09 §6 q.1). |
| `change-password-link` | partial | The popup's "Change password" opens `<page origin>/.well-known/change-password`. | Using the security center's checked link (`security_change_link`) from the extension. |

## 2. The channel

```text
 page ── content script (isolated world) ──runtime messages── background (SW / event page)
                                                                 │ runtime.connectNative("design.snds.lockdown")
                                                                 ▼ stdio: u32 native-endian length + UTF-8 JSON, ≤ 1 MiB
                                                    lockdown-native-host (launched by the browser)
                                                                 │ Unix socket <data>/browser.sock (0600 in the 0700 data dir)
                                                                 ▼ u32 big-endian length + JSON, ≤ 64 KiB (the identity crate's framing)
                                                    desktop app: browser/ endpoint → lockdown-app (the core)
```

- **Who meets where.** The app writes `native-host.json` in its data directory (D47): the socket path, its own executable and that file's SHA-256. The host reads it from `LOCKDOWN_DATA`, or beside `LOCKDOWN_ROOT`, or beside the platform's identity root, the app's own rule. No app running: the host says `no_app` and exits.
- **Both sides check each other.** The host checks the process on the socket is the executable the endpoint file names, by path and hash (`ProcessIdentityVerifier`; `SO_PEERCRED` and `/proc/<pid>/exe` on Linux, `LOCAL_PEERPID` and `proc_pidpath` on macOS). The app checks the connecting process is the installed `lockdown-native-host`, by path and the hash it took at start. Either check failing ends the connection before a byte is relayed.
- **Which extension.** The host reads its caller from the arguments the browser passes (Chromium: `chrome-extension://<id>/`; Firefox: the manifest path and the add-on id) and sends it first (`HostHello`). The app answers only the ids it knows (`CHROMIUM_IDS`, `FIREFOX_IDS`); the browser itself starts the host only for the ids its manifest lists. A paired client is bound to the id it paired from.
- **The host decrypts nothing.** It relays one request and one answer at a time, parses only `id` (to answer on the app's behalf when the app went away or a message is too large), and logs nothing.
- **Manifests.** `browser_install` (Settings → Browsers → Connect) writes `design.snds.lockdown.json` for each browser found for this user, naming the host beside the app (`LOCKDOWN_NATIVE_HOST` overrides). Places, per `crates/native-messaging/src/manifest.rs`: Linux `~/.config/{google-chrome,chromium,microsoft-edge,BraveSoftware/Brave-Browser}/NativeMessagingHosts/` (`$XDG_CONFIG_HOME` when set) and `~/.mozilla/native-messaging-hosts/`; macOS `~/Library/Application Support/{Google/Chrome,Chromium,Microsoft Edge,BraveSoftware/Brave-Browser,Mozilla}/NativeMessagingHosts/`; Windows the file under `%LOCALAPPDATA%\snds\lockdown\native-messaging\<browser>\` and the `HKCU\Software\…\NativeMessagingHosts\design.snds.lockdown` key returned for the installer to set. Linux is exercised in tests (a temporary home, and Chromium reading `<user-data-dir>/NativeMessagingHosts/` in the end-to-end suite).

## 3. Pairing

1. The popup's "Pair with Lockdown": the background makes a client id (16 random bytes) and a key (32), imports the key into WebCrypto as a **non-extractable** HMAC-SHA256 key kept in IndexedDB, sends id and key once in `pair_begin`, and zeroes the raw bytes.
2. The app answers a six-digit code from the OS random source and shows a dialog, wherever the person is in the unlocked app: "Connect Chrome on Linux to Lockdown?" with the code. The popup shows the code it was answered.
3. The person approves only if the codes match; approval goes through the `Approve` gate (D101: the platform check, the account password, or the window's confirm). "The codes differ" declines. A request lives two minutes; three may wait at once.
4. Every connection then starts with a 32-byte challenge from the app (`ready`), and the extension answers `HMAC-SHA256(key, "lockdown-browser-auth/v1\0" ‖ challenge ‖ client id)`. Until a connection proves a pairing, only `status`, `pair_begin`, `pair_status` and `auth` are answered. Removing a browser in Settings ends its sessions at their next request; the extension forgets its key when the app says it is not paired.

Another program on the machine that sends `pair_begin` gets a code of its own, which no extension window shows, so a person comparing codes declines it. Same-user malware that can read the browser profile could still read the IndexedDB record of a non-extractable key only through the browser's own process; that is the boundary this design accepts (§7).

## 4. Messages (extension ↔ app)

Requests are `{id, op, …}`; answers `{id, ok: true, result}` or `{id, ok: false, error: {code, message}}`; codes `not_paired`, `locked`, `approval_needed`, `refused`, `bad_request`, `unknown_op`, `not_found`, `expired`, `too_large`, `closed`. The host's own events: `ready`, `refused`, `no_app`, `untrusted_app`, `closed`, `unsupported`.

| op | Before pairing | In | Out | Secret in the answer |
|---|---|---|---|---|
| `status` | yes | — | `{locked, paired, app}` | no |
| `pair_begin` | yes | `client_id`, `client_key`, `name` | `{request_id, code}` | no |
| `pair_status` | yes | `client_id` | `{state: pending\|paired\|refused\|unknown}` | no |
| `auth` | yes | `client_id`, `proof` | `{paired: true}` | no |
| `matches` | no | `frame` | `PageMatches {host, logins[], lookalike_of[], secure}`; a crown jewel by title only (D113) | no |
| `wallet` | no | — | `{entries: [{item_id, kind, title, subtitle, crown_jewel}]}` | no |
| `fill` | no | `item_id`, `what` (`login\|code\|card\|identity`), `field?`, `top`, `frame`, `field_visible`, `user_confirmed` | `{verdict: fill, values: [{role, value}]}`, or `confirm` / `refuse` with `reason` | **only on `fill`**: the item's values for that one fill |
| `basic_auth` | no | `url` | as `fill`, or `{verdict: none, reason}` | only on `fill` |
| `search` | no | `query`, `limit?` (≤ 50) | `{items: [{item_id, kind, title, username, host, subtitle, crown_jewel, copy?}]}` | no |
| `copy` | no | `item_id` | `{label, clears_in_secs}`: the app copied it on its own clipboard path | no |
| `offer_save` | no | `page`, `username`, `password` (inbound) | `{offer_id?, kind: new\|update\|same, title, username}` | no |
| `save` / `dismiss` | no | `offer_id` | `{item_id, kind}` / `{}` | no |

Inside the extension (`extension/src/shared/messages.ts`) the background checks every sender: a content script's message must come from a web page's frame; `frame.html`'s from the extension's origin, in a tab, with a nonce that tab's content script registered (`#menu:<nonce>`, `#prompt:<nonce>`); `popup.html`'s from the popup itself. A pick is accepted only for an item the menu offered.

## 5. What the core decides (`crates/app/src/browser.rs`)

- **`page_matches(page)`**: every live login in every open vault, every website with its own rule, `decide_with` under the built-in signed data; `exact`, then `equivalent`, then `base_domain`, then the most recently edited; at most 20. Hosts the page imitates are named (a crown jewel's as "a protected login"); with no match, the security center's `imitated_site` names a well-known site the page copies.
- **`fill(item, what, field, facts)`**: the item must be live and of the kind asked; logins and codes are decided again against `facts.frame`; a login saved for `https` never fills a plain-`http` frame; cards and identities fill only on a secure frame (`https`, or this machine) and never on a page that imitates a known site; then `fill_check` on the facts; only on `fill` are values read, a password or card number through the same `Use`-gated path a copy takes (a crown jewel asks the platform check; with only a password or window route the extension is told to use the app), a code through `code_now`. A fill counts as a use for the recency ranking (D151).
- **`save_proposal` / `save_login`**: the typed password is compared with the saved one in the core; an update is a new version of that login through `edit_login` (its gate when a crown jewel); a new login is saved for the page's origin.

## 6. The extension's pieces

| Piece | Job |
|---|---|
| `background/index.ts` | Routes and checks messages, registers menus by nonce, runs a pick (measure → wasm `fill_check` → app `fill` → write), holds save-offer ids per tab, the shortcut, Basic Auth. |
| `background/link.ts` | The native port: connect, wait for `ready`, prove the pairing, one request at a time, the pairing poll. |
| `background/pairkey.ts` | The non-extractable HMAC key in IndexedDB; the proof. |
| `background/matcher.ts` | The wasm matcher; the data-version floor persisted in `chrome.storage.local` as 09 §4 asks (D64). |
| `content/detect.ts` | Field kinds: sign-in, code (single and split), card, identity; the save capture. |
| `content/visibility.ts` | D67's rule, measured; the menu host's own hit test for browsers without IOv2. |
| `content/fill.ts` | Writes through the prototype setters with `input`/`change` events; expiry formats; split code boxes. |
| `content/index.ts` | The closed-shadow, top-layer host; placement; closing rules; the save capture; answers `measure`, `apply`, `hotkey`, `target_login`. |
| `pages/frame.ts` | The menu and prompt; IOv2 arming; the second action; the lookalike stop; two frame heights only, so the page learns nothing but "something to offer". |
| `pages/popup.ts` | Pairing, the page's logins (Fill, Change password), search, copy, the lookalike banner and explicit copy confirm. |

## 7. Security notes and gaps

- **What the extension holds.** No vault data at rest: `chrome.storage.local` keeps the matcher-data floor; IndexedDB keeps the pairing key (non-extractable); memory keeps menu nonces and save-offer ids. A fill's values exist in the background and the content script for the moment of the write, then the references are cleared (JavaScript strings cannot be wiped; the same statement as D91).
- **Plain JSON on the socket.** The socket is user-only and both ends are verified; messages are not encrypted again and carry no replay counter (04-security §13 asks for a session key and a counter). Same-user malware that can connect could only reach a desktop that refuses it (process identity) or ask for pairing (a code nobody approves). A session key over the pairing key is the next step (D162).
- **Linux and macOS only.** The identity crate has no named-pipe transport yet, so on Windows the app runs with the endpoint idle and says so, and the host answers `unsupported`. The code-signature verifiers (macOS, Windows) are still the identity crate's to build (D33); macOS uses the path-and-hash check meanwhile.
- **Firefox.** Same code; no IntersectionObserver v2 (the menu host's hit test stands in, which does not see a `pointer-events:none` decoy over the menu itself); MV3 host permissions are optional there, so the person must grant site access once; not driven by a test in CI (Playwright cannot load an add-on into Firefox); `manifest.spec.ts` checks its manifest.
- **Crown jewels.** Filled through the platform check where one exists (Touch ID, Windows Hello: the OS prompt appears on the desktop); with only the password or window route the extension says to use the app, since the gate must never render in a page (D98).
- **Menus in small frames** are fitted to the frame (a clipped frame never counts as visible to IOv2); a frame too small for any menu gets one 40 px high.

## 8. How it is tested

| Layer | Where | Count |
|---|---|---|
| Core | `crates/app/src/browser.rs` | 8: page matches and lookalikes (saved and well-known), the fill decided again (other site, covered, framed, http, lookalike), codes' second action, cards (confirm, insecure, lookalike, kind), identities, save/update/same, every vault and website plus the use count, locked |
| Channel | `crates/native-messaging` | 11: framing, callers on both browsers, the hello, the endpoint file (0600, version), socket paths, data dir, manifests on every OS, install/uninstall on Linux |
| Host | `bins/native-host` | 5: relay, untrusted peer, refusal, too large and a gone app, no app |
| Endpoint | `apps/desktop/src-tauri/src/browser/` | 9: pairing store (code, proof, caller binding, file mode, limits, unpair); the protocol over the in-memory transport with a real `App` (nothing before pairing, every op, unpair ends a session, locked, other extension, untrusted program, no host installed); and once over the real Unix socket with the real process-identity check |
| Desktop UI | `apps/desktop/src/features/browsers/pairing-dialog.test.tsx` | 4: the code shown, approve through the gate, decline, Settings → Browsers |
| Browser | `extension/test/extension.spec.ts` | 19 against the unpacked build in Chromium, the real native host and the real endpoint over a throwaway vault (`apps/desktop/src-tauri/examples/browser-endpoint.rs`): fixed id; nothing before pairing; pairing by code; login fill; red team on the real path (cross-origin frame asks, opacity-0 and pointer-events:none overlays refused, a page covering the menu leaves it disabled, a lookalike stopped in menu and popup with copy's explicit confirm); code single and split; card with the second action; identity; the shortcut; Basic Auth; popup search, copy, fill and change-password link; locked; save a sign-up; update a changed password |
| Manifests | `extension/test/manifest.spec.ts` | 3 |

Run: `extension/README.md`. CI: the `extension` job builds the wasm, the host and the fixture, typechecks and builds the extension and runs all three spec files.

## 9. unicron

The popup and the in-page frames are plain HTML and one stylesheet (`static/ui.css`), not unicron: the in-page frame must be small and load nothing a page could influence, and the popup would need React and the token CSS inside the extension bundle. Gaps for unicron if the popup moves onto it: a published plain-CSS token sheet usable without Tailwind (the extension has no build step for Tailwind v4); a compact "pairing code" display (CodeDisplay is for one-time codes and counts down); a menu-row component sized for a 320 × 232 frame; the crown-jewel cue is never used here (D98), so nothing is needed for it.

## 10. Proposed decisions

For the coordinator to record. **Flagged** rows are user-visible and need Sean.

| ID | Decision | Owner |
|---|---|---|
| D155 | **The extension's channel**: browser → `lockdown-native-host` (native messaging, stdio) → a Unix socket in Lockdown's data directory (`browser.sock`, user-only) → the running desktop app; the app writes `native-host.json` (socket, its executable and SHA-256) and each side verifies the other's process identity before relaying; the host decrypts and keeps nothing. Plain version: the browser talks to the Lockdown app on your computer through a small helper, and each checks the other is the real thing. | Accepted by default under D38; audit item |
| D156 | **Pairing a browser**: a six-digit code from the app, shown in the extension and in the app, approved in the app through the `Approve` gate; the extension's key is a non-extractable WebCrypto HMAC key; each connection proves it with HMAC-SHA256 over a fresh 32-byte challenge, bound to the extension id it paired from; requests wait two minutes, three at once; removal in Settings ends its sessions. Plain version: you pair each browser once by checking two codes match. | Accepted by default under D38; **flagged for Sean** (the pairing step, the dialog's words "Pair" / "The codes differ") |
| D157 | **The extension needs the desktop app for now**: no app running, locked, or not paired, the extension says so and fills nothing; the standalone mode (the wasm core in an offscreen document, 03-architecture §4) comes later. Plain version: open and unlock Lockdown to fill in the browser. | **Flagged for Sean** |
| D158 | **Three decisions per fill**: the core picks candidates, the extension's wasm matcher must agree on each, and the core decides the fill again from the browser's own top URL and frame URL and the measured facts; only on `fill` does any value leave the core, and only that item's values for that one fill. Plain version: a fill happens only when both halves agree, and the browser never sees anything else. | Accepted by default under D38; audit item |
| D159 | **The menu is an extension-origin frame in a closed shadow root in the top layer**, armed only after 400 ms uncovered (IntersectionObserver v2 on Chromium; the host's own hit test on Firefox), disabled again when covered, deaf to untrusted events; two heights only; it closes on Escape, blur, typing, or when it has nothing to offer. Plain version: the fill menu can't be read, restyled or clicked through by the page. | Accepted by default under D38; **flagged for Sean** (the menu opens on focus of a sign-in, code, card or address field) |
| D160 | **Scheme and secure-page rules beside `fill_check`**: a login saved for `https` never fills a plain-`http` frame; cards and identities fill only on `https` or this machine, never on a page that imitates a saved or well-known site (the security center's rule). Plain version: no passwords into unencrypted copies of a site, no cards on insecure or fake pages. | Accepted by default under D38; audit item |
| D161 | **Save and update after a sign-in**: the typed password goes to the app at once and is compared there; the app holds an offer three minutes in memory and the extension only its id; the prompt is an extension-origin frame in the top frame, Save / Update / Not now; a form without a username uses the site's one saved login. Plain version: after you sign in somewhere new, Lockdown offers to save it; after a password change, to update it. | Accepted by default under D38; **flagged for Sean** (the prompt and its words) |
| D162 | **The socket carries plain JSON for now**, user-only and peer-verified on both ends, without the session key and replay counter 04-security §13 asks for; the next step derives a session key from the pairing key and the challenge. Plain version: the browser-to-app link is private to your user account, and gets its own encryption later. | Accepted by default under D38; audit item |
| D163 | **The fill shortcut is Ctrl+Shift+L (⌘⇧L on macOS)**, filling the best login and the next on each press within 8 s; on a code, card or address field it opens the menu for the second action. Plain version: one shortcut fills your login. | **Flagged for Sean** (the keys) |
| D164 | **Basic Auth is answered only for a top-level navigation, never a proxy, never twice for one request, and only when exactly one login is saved for exactly that host**, through the same fill decision. Plain version: sites that use the browser's own sign-in box get filled when the choice is obvious. | Accepted by default under D38; **flagged for Sean** (no picker when several logins match) |
| D165 | **Copy from the extension goes through the desktop's clipboard path** (core → clipboard, the person's timer, D85/D119); the popup never holds the value. Plain version: copying in the browser works like copying in the app. | Accepted by default under D38 |
| D166 | **The lookalike stop in the extension**: a page that imitates a saved login or a well-known site gets a hard stop in the menu and a banner in the popup; a copy there takes a second, explicit "Copy anyway". Plain version: on a fake site Lockdown says stop, and copying needs a deliberate second click. | Accepted by default under D38; **flagged for Sean** (the copy-anyway step) |
| D167 | **The extension's ids**: the development Chromium id is fixed by a public key in `manifest.base.json` (`dbdblnolpiehhlabpmhobmkbjipmebpe`, no private key kept), the Firefox id is `lockdown@snds.design`; store ids join the allow-lists when they exist. Plain version: the app only talks to Lockdown's own extension. | Accepted by default under D38 |
| D168 | **Where the manifests go**: written by the app on request (Settings → Browsers → Connect) for each browser found for this user, at each browser's per-user place on Linux and macOS; on Windows the file under Lockdown's folder and the registry key left to the installer. Plain version: one button connects your browsers. | Accepted by default under D38; **flagged for Sean** (connecting is a button, not automatic) |
| D169 | **A fill from the extension is a use** for the recency ranking (D151), as a copy is. Plain version: what you fill most comes first in quick access too. | Accepted by default under D38 |
