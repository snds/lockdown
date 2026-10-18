# 26: Desktop and extension finishing

_Slice: `slice/desktop-finish`, built 2026-10-18 under D122 ("desktop and web first"). Rows: quick-access, code-display-aids, device-code-approve, proximity-gate, conditional-create, extension-coverage (UI part), memory-hygiene, clipboard-hygiene, key-transparency (web contacts), pq-key-wrapping. Proposed decisions D305–D319 (§14)._

## For future agent

- **TL;DR:** Quick access is one ranked, typo-tolerant index in the core (`lockdown_vault::search`, over summary words only). The desktop palette, the new quick-access window, the web vault's palette, the extension popup and `lockdown search` all ask it. The palette can show a code in place and "Fill in the browser". The fill puts that login first in the extension's menu for two minutes. A system-wide shortcut opens the quick-access window. TV and console sign-ins are scanned from the camera (the core decodes the frames). They go through the proximity gate and a signed allow-list of activation pages, then the `Approve` gate. The real page opens and the login picked comes first there. Passkey and Steam QR codes go through the same gate and then stop with a sentence. The passkey upgrade follows any password sign-in the extension saw. The popup is on unicron. Key pages are locked out of swap. Typed secrets are cleared when the window hides. Copies carry the platform's marks that keep them out of clipboard history. Reveals block screen capture. The web vault has a People screen. The post-quantum identity key now waits until every device says it supports it.
- **Read with:** `crates/vault/src/search.rs`, `crates/app/src/{quick,scan,hygiene}.rs`, `crates/app/src/session/quick.rs`, `crates/approval/src/activation.rs`, `crates/crypto/src/locked.rs`, `crates/vault/src/pq.rs` (rollout rule), `apps/desktop/src-tauri/src/{quick,scan,hygiene}.rs`, `apps/desktop/src/features/{quick,scan,hygiene,palette}/`, `apps/desktop/src/app/quick-window.tsx`, `extension/src/popup/`, `apps/web/src/core/web-finish-client.ts`, `apps/web/src/features/people-view.tsx`.
- **Do not:** index anything a summary doesn't already show; open a scanned address verbatim; let a paste or a picture reach a yes for a device code; write to the clipboard through the plugin (use `hygiene::write_secret`); report the quick-access window's focus to the lock timers.

## 1. Quick access: one index in the core

**The index** (`lockdown_vault::search`) is a vocabulary of distinct words (a `BTreeMap`) with the docs and fields each word came from. Text is lower-cased and split at anything that is not a letter or a digit. A website's whole host is also kept as one word, without `www.`. Each query word must match some word of the doc, in any order. A match is scored as follows:

| Match | Score |
|---|---|
| the whole word | 100 |
| the start of a word | 60–90 (more of the word typed, more score) |
| inside a word, two letters or more | 40 |
| a typo: one letter off for 4–7 letters typed, two for 8 or more, against the whole word or its start | 15–25 |

Field weights: title and host 3, username and tag 2, subtitle, kind and folder 1. Results are ordered by: every word exact before any forgiven one, then score, then the caller's boost, then the order given. **Words of three letters or fewer are never matched loosely**, so `gml` finds nothing spread across GitHub's words (D118's case). A result whose match used a typo carries `exact: false`. Every shell shows that as "Close match". So Ctrl+Enter copies a typo's result only when nothing matched exactly.

**What goes in** (`session::quick::build`): the live rows of `list()` only (never trashed or archived). Each row adds its title, its website's host, its username, the template's subtitle, its kind and the kind's plain name, its tags and its folder's path. A crown jewel's summary has no username, site or subtitle (D113), and its doc leaves out tags and folder too. Nothing here reads a field: **no secret is ever a search word**. The tests search for a password and find nothing. The boost is the palette's existing order (D151): favourite above everything, then this device's uses, halved every two weeks, then A–Z.

**Cache:** the index lives on the `Session`, so a lock drops it. It is rebuilt when a stamp moves. The stamp is a BLAKE3 over every item of every open vault (id, revision, version count, state, class) and this device's use record. Computing it decrypts nothing.

**Who asks:**

| Shell | Path |
|---|---|
| Desktop palette (Ctrl+K) and quick-access window | `quick_search` command, `QuickClient` |
| Web vault palette | `quick_search` in `crates/web/src/commands/finish.rs`, `webQuickClient` |
| Extension popup | the existing `search` op, now through `App::search`; adds `exact` and `copy.field_id` (D62) |
| CLI | `lockdown search <words> [--limit N]`, over the vault's plain fields with the same `Index`; marks close matches |

**Actions** (`lockdown_app::quick`):

- **Show code** (Alt+Enter, or the action bar): `App::quick_code` returns the code valid now, the same value and `Use` gate as copying it. It counts as a use (D151). The palette shows it for 30 seconds, and the window is kept out of screen captures while it shows (§6). In the popup, `code_show` returns `{code, seconds_left, title}`, never the set-up.
- **Fill in the browser** (Shift+Enter): `App::quick_fill` works only on a live login. It refuses a crown jewel and an item kept to the app (D245). It answers the login's own `http(s)` site for the shell to open, and remembers `{item, host}` for two minutes. While remembered, the core's `page_matches` marks that login `asked` and puts it first, and the popup shows "From quick access". **The fill is the extension's ordinary fill**: the three decisions and the armed click (D158, D159). Nothing fills by itself. The request counts as a use. A lock forgets it. The web vault can't fill another tab, so it doesn't offer this.
- Copy is unchanged (`copy_secret` / `copy_code`).

The palette is now **driven by the core's results** (`Command shouldFilter={false}`) and draws its body only while open. So each opening starts empty with the search in focus. The TypeScript substring rule (`palette-model.matches`) stays for the vault list's own search box.

## 2. The system-wide shortcut and the quick-access window

`tauri-plugin-global-shortcut` 2.4.0 is registered from Rust (`quick::commands::plugin`). The shortcut is **Ctrl+Shift+Space (⌘⇧Space on macOS)**. No permission reaches either webview. Pressing it opens, or focuses, a small undecorated window labelled `quick`. The window is always on top, skips the taskbar, is content-protected, and loads `index.html?window=quick`. `main.tsx` then draws `QuickWindow`, which holds only the palette, in place. The window hides on blur, on Escape and after an action. Enter on a result opens the item in the main window (`lockdown://open-item`). Locked, it shows Unlock and nothing else. The shell's focus handler now acts on the main window only, so the quick window hiding is never "the app went out of view" for the hidden-window lock timer.

Settings → Quick access turns the shortcut off or on (`quick_access_hotkey` in the settings file, default on, D100's rules). Settings also shows why it isn't registered, when it isn't.

| Platform | Works? |
|---|---|
| macOS | yes (Carbon hot keys through the plugin) |
| Windows | yes (`RegisterHotKey`) |
| Linux, X11 | yes (`XGrabKey`) |
| Linux, Wayland | **no**: Wayland gives an app no global keys. Registration fails, and Settings says to bind a key to `lockdown-desktop` in the desktop's own keyboard settings. Ctrl+K still works in the window. |

This is the one D122 deferral the brief allowed back: it needs the plugin, not OS-level integration.

## 3. TV and console sign-ins (`device-code-approve`)

**The allow-list is signed data** (`crates/approval/src/activation.rs`). It is the matcher's signed envelope (D66, D260's pattern) with its own format (`lockdown-activation-pages`), its own signing prefix (`lockdown-activation-pages/v1\0`), the matcher's publisher keys (D63) and a version floor. The source is `crates/approval/data/activation-pages.json`. `cargo run -p lockdown-approval --example sign-activation-pages [-- --check]` rebuilds or checks the signed file, and a unit test fails when it is stale. A page entry has the service, its exact hosts (the first is the one opened), its exact path, the query parameters that may carry the code, the parameter Lockdown opens with (or none, when the person types the code) and whether the code may be the path's last segment. Nine pages are listed: Microsoft `/link`, YouTube `/activate`, Google `/device`, Epic Games `/id/activate`, Discord `/activate`, Netflix `/tv8`, Disney+ `/begin`, Amazon `/code` and Prime Video `/mytv`. **The parameter names come from open-source clients and community notes, not from each vendor.** They must be checked against the live pages before a release. They are data, so fixing them is a re-sign, not a code change.

**Reading a code** (`classify`) treats the text as hostile. A device code has to meet all of these:

- the scheme is `https`;
- there is no user name, port or fragment;
- the host is a domain exactly on the list (a look-alike, a sub-domain or a suffix trick is "unlisted");
- the path is exactly the page's;
- the code is 4–16 ASCII letters and digits, in groups.

**The address opened is rebuilt** from the entry and the code (`https://<first host><path>?<param>=<CODE>`), so extra parameters never reach the browser. `FIDO:/…` is a passkey QR. `https://s.team/q/…` is Steam's sign-in QR. Anything else is "other". Text longer than 2 KB is "other" too.

**The flow** (`lockdown_app::scan`):

1. *Read.* The desktop's `ScanDialog` (Codes → "Sign in a TV") has three tabs. Camera reads frames through the webview's own `getUserMedia`, one every 300 ms, drawn at most 640 px wide and turned grey. Picture reads an image file. Paste takes typed text. Frames cross as **raw grey bytes** (`invoke(…, Uint8Array, {headers})`, size and source in the headers), and the core decodes the QR with `rqrr`. Nothing is decoded in the webview.
2. *The gate first.* Whatever the code is, it goes through `ApprovalService::evaluate`: the proximity rule, the full domain shown, and the rate limit of five a minute. The approval service lives on the `Core`, so its rate limit outlives a lock. The same text read again within 20 seconds is answered from the last refusal, so a camera looking at one code doesn't spend the limit.
3. *The list.* An unlisted page is refused with its host named.
4. *The yes.* The outcome shows the service, the full domain and the TV's code. It lists the logins that fit the page (never a crown jewel). The person picks one, or "I'll sign in by hand". `activation_approve` runs the `Approve` gate (D101). It checks the login is one the scan listed, counts it as a use, and marks it `asked` for the extension (§1's mechanism). The shell then opens the page. If the page takes no code in its address, the code is shown large to type. An activation waits five minutes, is used once, and a lock or a close cancels it.
5. *On the page.* The extension's menu on the activation page lists the login picked first, and it fills through the ordinary path. Lockdown never calls a vendor's token endpoint (brief §2.7).

## 4. The proximity gate, wired (`proximity-gate`)

`CodeSource` gains `Image` ("read from an image file: it may have been sent; counts as a paste"). The shells map **camera → `ScannedFromScreen`**, picture → `Image`, paste → `Pasted`. The camera is the only path to a yes, and no setting changes that.

| Code | Camera | Picture or paste |
|---|---|---|
| Device code on a listed page | prompt, then the yes | refused: "didn't come from a screen in front of you…" |
| Passkey QR (`FIDO:/`) | gate passes, then **unsupported**: the hybrid flow needs a phone's Bluetooth link to the computer showing the QR | refused by the gate |
| Steam QR | gate passes, then **unsupported**: Steam approval is parked (D8, D263) | refused by the gate |

What the gate can't prove: that a frame really came from a camera. The webview labels the source, and the core can't check the label. A compromised webview could label a picture "camera". The rest of the defence stands anyway: the allow-list, the full domain shown, the `Approve` gate, and the person picking the login.

## 5. The passkey upgrade after any sign-in Lockdown saw (`conditional-create`)

Until now the upgrade (D210) followed only a password fill by this core. Now `save_proposal` also notes a sign-in when the password the extension captured equals the saved one for that login on that site (`Same`), and `save_login` notes one after a prompt saved a new or updated login. Both use the same five-minute, same-host, same-user record as a fill. The site's own conditional `create` call is the evidence the sign-in succeeded, and it is still checked against the caller's host and the user name. The OS-level automatic upgrade stays deferred (D122).

## 6. Clipboard and screen (`clipboard-hygiene`)

Copies are written by `hygiene::write_secret`, which uses `arboard` directly (the crate and version the plugin wraps) so each write carries this platform's marks:

| Platform | Marks | Tested here |
|---|---|---|
| Windows | `ExcludeClipboardContentFromMonitorProcessing`, `CanIncludeInClipboardHistory`=0, `CanUploadToCloudClipboard`=0 | type check for `x86_64-pc-windows-msvc` of the crypto crate only; the shell isn't built for Windows here |
| macOS | `org.nspasteboard.ConcealedType` (arboard), `org.nspasteboard.TransientType` (`mark_transient`, `objc2-app-kit`) | **not built here** (no Apple SDK); needs the macOS CI job |
| Linux | `x-kde-passwordManagerHint` = `secret` (Klipper and the managers that follow it) | the list of marks; CI has no display server, so the write itself isn't exercised |

The clear rule and its timer are unchanged (D85, D119).

**Screen capture.** Every reveal the webview draws holds a reference count (`CaptureCount`, `capture_protect`), and while the count is above zero the main window is content-protected (`set_content_protected`). The reveals that hold it: a field's reveal (35 seconds, longer than the field's 30-second lifetime), large type, the Wi-Fi guest card, a code shown in quick access, and the scan sheet once it shows a code. Windows uses `WDA_EXCLUDEFROMCAPTURE`, so screenshots, sharing and recordings show the window blank. macOS uses `NSWindowSharingNone`. **Linux has no such call**: there `set_content_protected` does nothing, and `capture_protection_available()` says so. The quick-access window is protected for its whole life. The web vault can't protect a tab, and answers `false`.

## 7. Memory (`memory-hygiene`)

**Pages out of swap** (`lockdown_crypto::locked`). Every `Key32` (vault, item, version and attachment keys, and every key Lockdown makes) and every `SecretBytes` locks the pages under it while it lives: `mlock` on Linux and macOS, `VirtualLock` on Windows. A page is locked once and released with the last secret on it (a count per page; the kernel doesn't count `munlock`). A refused lock (`RLIMIT_MEMLOCK`, the Windows working set) is counted and the secret works as before. `harden_process()` sets `RLIMIT_CORE` to 0 on Linux and macOS, called by the desktop's and the CLI's `main`. It deliberately avoids `PR_SET_DUMPABLE`, which would hide `/proc/<pid>/exe` from the native host's check of the app (D155). Settings → Quick access shows the pages locked now and the refusals (`memory_report`). The Linux unit test reads `VmLck` from `/proc/self/status`. The Windows code is type-checked for `x86_64-pc-windows-msvc`. WASI has no such calls: there every lock is a no-op and says so.

What it doesn't reach:

- the identity crate's own key types (root key, persona keys, App Data Key), which belong to that crate (D12), a proposal for `08-identity-proposal.md`;
- copies the allocator made before a value became a secret type;
- `Revealed` (short-lived);
- the JSON frame that carries a filled value or a shown code to the extension (as before, D158).

**Strings in the webview.** `useSecretWipe` clears every typed secret (each `input[type=password]` and anything under `[data-secret-input]`) at two moments:

- when the window or tab is hidden;
- after it has been out of focus for 60 seconds.

The value goes through the element's own setter and an `input` event, so React's copy goes too. A toast says how many fields were cleared. A lock unmounts the views anyway. unicron's SecretField already hides a reveal on blur, lock and leaving the viewport.

**Measured** (`apps/web/e2e/finish.spec.ts`, Chromium, CDP heap snapshots after a forced GC). The test types a distinctive password into the editor.

| Moment | Is it in the page's heap? |
|---|---|
| while typed | **yes** (as D91 says) |
| after the tab hides | **no** |
| after the vault locks | **no** |

This is V8 only. WebKitGTK, WKWebView and WebView2 aren't measured here, and the wasm worker's linear memory is measured by the core's own tests, not this one.

## 8. Contacts in the web vault (`key-transparency`)

The web vault gains a **People** screen (`features/people-view.tsx`): the desktop's own `PeopleSection` over a web sharing client that implements overview, lookup, verify and forget. The rest of that client refuses in words. It shows your fingerprint and your code to paste (`lockdown:contact:v1:…`). Comparing takes three forms, desktop and web alike:

- the digits read out;
- the code pasted;
- their QR code scanned with the camera. The core reads the frames (`qr_contact_frame`) and the result goes to `verify_contact` like a paste.

Two fixes came out of the browser test:

- **A fresh device's own code had no account in it.** The account id was learnt only when sharing first happened. `sharing_overview` now learns it from the directory, checked against the log, so the code is whole on any device.
- **The web vault's `Permissions-Policy` said `camera=()` and `publickey-credentials-get=()`.** That blocked the tab's camera (device linking and now scanning) and its own WebAuthn sign-in (D257). Both are now `(self)`. Everything else stays off, and a unit test holds both facts.

## 9. The popup on unicron (`extension-coverage`, UI part)

The popup is React 19 with `@unicron/ui` 0.12.0 on `@unicron/tokens` 0.6.0, the same releases and the same override as the desktop. Tailwind v4 is built by Vite (`vite.popup.config.ts`), and `scripts/build.mjs` runs it once per browser family into `dist/<family>/popup.html` and `popup/`. The popup keeps everything it did: pairing, the page's logins with Fill and Change password, Fix this page, Hide my email, the lookalike stop with "Copy anyway", and search with copy. It adds Close match, Show code and the "From quick access" mark, and its search has focus on open. It is built from unicron's Item, Button, Input, Badge, Checkbox, NativeSelect and Spinner, plus its token classes. It is never web-accessible. Its CSP is the extension's (`script-src 'self'`): one module, no inline script. The colour scheme follows the system with a class set from the bundle.

The design-system ratchet runs on it (`extension/dsa.config.json`, `pnpm ds:ratchet`, baseline **0** on every metric). It also runs in the CI `extension` job.

The in-page menu and prompts stay plain HTML and `ui.css`. They are drawn inside pages and load nothing a page could influence (16 §9). The bundle adds about 5.6 MB, most of it the Material Symbols font.

## 10. Post-quantum signatures (`pq-key-wrapping`)

**Built: the rollout rule** for the identity's ML-DSA-65 key (D248's audit item). Before, a device published the key on its next identity push. An older client then dropped `pq_sig` when it re-encoded a manifest and broke its own chain check. Now:

- Each device marks its own `Device` record `lockdown.pq = 1`, signed by the identity key like a rename. The identity crate keeps fields it doesn't know.
- The key is published only when every device not revoked carries the mark (`pq::roll_out` in `push_identity_records`). Once published, it stays.
- `devices_without_support` names the devices holding it back.

**Not built: ML-DSA on vault heads.** The plan:

- **The field.** Add `ManifestHead.pq_sig` (optional, skipped when absent). It sits outside the Ed25519-signed bytes and outside `hash()`, as `checkpoint` does, so an older client that drops it never breaks its chain. It is an ML-DSA-65 signature by the identity's post-quantum key over `signable()`, with context `lockdown vault head`.
- **Signing.** Sign with it in every commit and sync once `pq::published_key` is set; heads stay Ed25519 until the rollout rule publishes the key.
- **Checking.** On load and pull, verify every `pq_sig` present against the vault owner's published key. Once a chain carries one, require it on every later head. Also add a `HeadMemory` "post-quantum since seq N" field, so a server that strips the signatures from the whole chain is refused, not just ignored.
- **Shared vaults.** Defer them: their heads are signed by other people's devices, whose post-quantum keys would come from their pinned contacts.
- **Why it isn't in this slice.** It touches `vault.rs` commit and load, `HeadMemory`, sync and the server's head parser. That is more than this slice could test across every path.

## 11. Command and wire surface (all additive, D62)

- **Core.**
  - `lockdown_vault::search`;
  - `lockdown_app::{quick, scan, hygiene}` (`App::search`, `quick_code`, `quick_fill`, `pending_fill`, `scan_image`, `scan_text`, `activation_approve`, `activation_cancel`, `memory_report`, `harden_process`);
  - `lockdown_approval::activation`;
  - `CodeSource::Image`;
  - `lockdown_crypto::locked`;
  - `lockdown_vault::pq::{SUPPORT_FIELD, roll_out, mark_this_device, devices_without_support}`;
  - `LoginCandidate.asked`;
  - `Settings.quick_access_hotkey`.
- **Desktop commands**, appended to the handler list: `quick_search`, `quick_code`, `quick_fill`, `quick_hide`, `quick_open_main`, `quick_hotkey_status`, `quick_hotkey_set`, `scan_frame`, `scan_text`, `activation_approve`, `activation_cancel`, `qr_contact_frame`, `capture_protect`, `memory_report`. Window events: `lockdown://quick-shown`, `lockdown://open-item`. Capability: the `quick` window added to `default.json`.
- **Web commands** (`commands/finish.rs`): the same names, plus `sharing_overview`, `contact_lookup`, `contact_verify`, `contact_forget`. `quick_fill` and `capture_protect` answer in words or `false`.
- **Extension ↔ app:** op `code_show`; `search` items gain `exact` and `copy.field_id`; `matches` logins gain `asked`. Popup message `code_show`.
- **CLI:** `lockdown search`.
- **Server:** `Permissions-Policy` `camera=(self)` and `publickey-credentials-get=(self)`.
- **Dependencies:** `rqrr` 0.11 (no default features; MIT OR Apache-2.0 and ISC), `tauri-plugin-global-shortcut` 2.4.0 (Apache-2.0 OR MIT), `arboard` 3.7 directly (already in the tree), `objc2-app-kit` / `objc2-foundation` on macOS (already in the tree), `libc` / `windows-sys` in `lockdown-crypto` (already in the tree). The extension gains React, `@unicron/ui`, Tailwind and Vite at the desktop's versions, and `@snds/dsa` at the desktop's pin. `cargo deny check` passes.

## 12. How to run and test

```sh
export CARGO_TARGET_DIR=… CARGO_INCREMENTAL=0
cargo test -p lockdown-vault --lib search pq         # the index; the rollout rule
cargo test -p lockdown-approval                      # the allow-list, classify, the gate
cargo test -p lockdown-crypto --lib locked           # pages out of swap (VmLck on Linux)
cargo test -p lockdown-app --test quick_access --test device_code --test passkeys --test phrase_check --test scan_fixture
cargo test -p lockdown-cli --test search
cargo test -p lockdown-desktop                       # frames, marks, the capture count, the extension ops
cd apps/desktop && pnpm test                         # palette, scan dialog, wipe, capture protection
cd apps/web && pnpm test && pnpm e2e                 # e2e/finish.spec.ts: index, camera, paste, People, the memory measure
cd extension && pnpm build && pnpm ds:ratchet && pnpm test   # popup, show code, quick-access mark, TV page fill
cargo run -p lockdown-approval --example sign-activation-pages -- --check
```

The web vault's camera test points Chromium's fake camera at `apps/web/e2e/fixtures/tv-code.y4m`, a TV's QR in one YUV4MPEG2 frame. `crates/app/tests/scan_fixture.rs` makes it with the app's own QR encoder, checks the committed file is current and reads it back through the core's decoder. `LOCKDOWN_WRITE_FIXTURES=1` rewrites it.

By hand: run the desktop app, press Ctrl+Shift+Space anywhere (X11, macOS, Windows), then type a typo of an item's title. For a TV sign-in, open Codes → Sign in a TV and hold a phone showing a TV's QR in front of the camera.

**Tests added:**

| Layer | Added |
|---|---|
| Rust | search 7, quick index 1, approval 5, locked 2, pq rollout 1, quick-access app 2, device code 4, the passkey upgrade 1, scan fixture 1, CLI search 1, desktop 3 (shortcut words, frame headers, marks and capture count), web policy 1, plus extension-op assertions in the desktop's browser tests |
| Desktop vitest | 11 |
| Web vitest | 3 |
| Web e2e | 5 |
| Extension Playwright | 3 |

## 13. Gaps

- **The webview camera on desktop.** WebKitGTK denies `getUserMedia` unless the host answers its permission request, WKWebView needs `NSCameraUsageDescription` in the bundle's Info.plist, and WebView2 prompts. None of these is wired or tested here; the camera path is proven in Chromium (the web vault) and in the core. Until the shells handle the permission, the desktop's camera tab may say it can't use a camera, and the picture and paste tabs then only explain the refusal. **The desktop's TV flow needs this before it is usable without the web vault.**
- **Activation parameter names** are unverified against the vendors' live pages (§3).
- **Passkey QR and Steam QR** stop after the gate (§4).
- **macOS clipboard and capture code** isn't compiled here; Windows only type-checks in the crypto crate; Linux capture protection doesn't exist (§6).
- **The global shortcut doesn't work under Wayland** (§2).
- **Post-quantum vault heads** (§10).
- **Typed secrets in unicron's editing SecretField while shown**: the host can't ask it to re-mask on blur (§15).
- **The extension's Firefox build** of the popup is built but, as before, not driven by a test.

## 14. Proposed decisions

For the coordinator to record. **Flagged** rows are user-visible and need Sean.

| ID | Decision | Status |
|---|---|---|
| D305 | **One quick-access index in the core, ranked and forgiving a typo.** `lockdown_vault::search` over summary words only (title, host, username, subtitle, kind, tags, folder; a crown jewel by title and kind). Ordered: exact before forgiven, then field-weighted score, then favourite and recent use, then A–Z. One letter forgiven from four letters typed, two from eight; three letters or fewer never loosely. Forgiven results say "Close match". The desktop, quick-access window, web vault, popup and CLI all ask it. Amends D118. Plain version: Lockdown's search now finds what you meant despite a typo, ranks it the same everywhere, and still never searches a password. | Proposed; **flagged for Sean** (typos now find things; "Close match") |
| D306 | **Quick-access actions: Show code and Fill in the browser.** Alt+Enter shows the code valid now for 30 s, through the `Use` gate, counted as a use, kept out of screen captures. Shift+Enter opens a login's site, and for two minutes the extension's menu there lists that login first; the person still picks it. Never a crown jewel or an app-only item. Plain version: from quick access you can see a code without copying it, or send a login to the browser to fill. | Proposed; **flagged for Sean** (the keys; the two-minute wait) |
| D307 | **A system-wide quick-access shortcut, Ctrl+Shift+Space (⌘⇧Space on macOS), on by default**, through Tauri's global-shortcut plugin. It opens a small always-on-top window holding only quick access, which hides on blur or Escape; Settings turns it off. Works on macOS, Windows and X11, not Wayland (Settings says so). Takes back this item from D122's deferrals, since it needs no OS-level integration. Plain version: press Ctrl+Shift+Space anywhere to find a password without opening Lockdown's window. | Proposed; **flagged for Sean** (the keys; on by default; reverses part of D122) |
| D308 | **Search has focus whenever quick access opens**: the palette, the quick-access window and the popup. Plain version: start typing at once. | Proposed; **flagged for Sean** (minor) |
| D309 | **The activation-page allow-list is signed data, and the page opened is rebuilt.** Format `lockdown-activation-pages`, prefix `lockdown-activation-pages/v1\0`, the matcher's keys and version floor; nine pages; exact host and path; the code 4–16 letters and digits; the address rebuilt from the list, never the scanned text. Plain version: Lockdown opens only TV sign-in pages on its signed list, and only the way the list says. | Accepted by default under D38; audit item |
| D310 | **The proximity rule as wired: only the camera leads to a yes.** A picture file counts as a paste; both are refused with no override. Passkey and Steam QR codes go through the same gate and then stop, since this computer can't finish them. QR codes are decoded in the core from raw grey frames. Plain version: point your camera at the TV; a code someone sends you, even as a picture, is never approved. | Proposed; **flagged for Sean** (a picture is refused) |
| D311 | **Approving a TV sign-in.** It shows the service, the full domain and the TV's code. The person picks a login, or "by hand". The yes goes through the `Approve` gate, counts as a use, and opens the real page with the code (or shows it large to type); the login picked comes first in the extension's menu there. An activation waits five minutes and is used once. Plain version: scan, check it's your TV, say yes, and the real page opens ready to sign in. | Proposed; **flagged for Sean** (the sheet and its words) |
| D312 | **The passkey upgrade follows any password sign-in Lockdown saw.** A captured password equal to the saved one for that login on that site, or a sign-in the save prompt kept, counts as a fill would (D210's five minutes, same host, same user). Amends D210. Plain version: the site may add a passkey to your login after you sign in, even if you typed the password yourself. | Proposed; **flagged for Sean** (more upgrades without a prompt) |
| D313 | **The popup is on unicron**: React and `@unicron/ui` 0.12.0 on tokens 0.6.0 (the desktop's), Tailwind v4 by Vite, the design-system ratchet at baseline 0. The in-page frames stay plain HTML. Plain version: the toolbar popup now looks like the app. | Accepted by default under D38 |
| D314 | **Secret pages are kept out of swap, and core dumps are off.** `mlock` / `VirtualLock` under every `Key32` and `SecretBytes`, counted per page, refusals counted and shown in Settings; `RLIMIT_CORE` 0 on Linux and macOS; never `PR_SET_DUMPABLE`, which would break the native host's check of the app. The identity crate's own keys are out of reach (a crate proposal). Plain version: Lockdown's keys stay in memory and never reach the disk's swap or a crash file. | Accepted by default under D38; audit item |
| D315 | **Typed secrets are cleared when the window hides, and after a minute out of focus.** Every password field, through its setter so the app's copy goes too, with a toast. Measured in Chromium: the typed password is in the page's heap while typed, and gone after the hide and after the lock. Plain version: leave a half-typed password and switch away, and Lockdown clears it. | Proposed; **flagged for Sean** (unsaved typing is lost) |
| D316 | **Copies are marked to stay out of clipboard history, and reveals block screen capture.** Windows: the three exclusion formats. macOS: `ConcealedType` and `TransientType`. Linux: the KDE hint. While any reveal is drawn the main window is content-protected (Windows, macOS; Linux has no such call); the quick-access window always is. Plain version: copied passwords don't land in Windows' clipboard history or cloud sync, and screenshots of a shown password come out blank. | Proposed; **flagged for Sean** (screenshots of the app are blank while something is revealed) |
| D317 | **The web vault has a People screen**: look up, compare by digits, a pasted code or the tab's camera, forget. Your own code always names your account, learnt from the directory and checked against the log. The vault's page may use the camera and WebAuthn on its own origin (they were blocked). Plain version: check a friend's keys from any browser, as in the app. | Proposed; **flagged for Sean** (a new screen) |
| D318 | **The post-quantum identity key waits for every device.** Each device marks its own record `lockdown.pq`; the key is published only when every device not revoked has the mark, then stays. ML-DSA on vault heads is planned (doc 26 §10), not built. Plain version: the stronger signature switches on only when none of your devices would choke on it. | Accepted by default under D38; audit item |
| D319 | **Wire additions (D62, additive):** the commands, ops, fields and events of §11; `CodeSource::Image`; `Settings.quick_access_hotkey`; `LoginCandidate.asked`; the search item's `exact` and `copy.field_id`; the `code_show` op. Plain version: older and newer parts keep understanding each other. | Accepted by default under D38 |

## 15. unicron host-contract gaps

- **ApprovalSheet requires number matching.** A device code (RFC 8628) has no number to match, so the TV sheet is the host's own dialog. The sheet needs a `matching: none` mode for flows whose proof is the scan itself (its `origin="scan"` rule already fits).
- **A QR scanner primitive** (video, framing, permission states, cancel, a frame callback the host decodes) would serve the TV flow, contact checks and the web vault's device link. 22 §16 asked for this as well.
- **SecretField in edit mode has no `masked` or `onWipe` control.** The host clears typed values from outside (§7) and can't ask a shown field to mask on blur.
- **The palette's secondary actions.** `CommandItemAction` allows one action per item, so Show code and Fill live in a host action bar with their own keys. A `CommandItemActions` list, each with its key, would fit better.
- **A plain-CSS token sheet** is still missing (16 §9). The popup builds Tailwind with Vite instead, which costs the extension a second build tool.
