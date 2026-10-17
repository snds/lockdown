# Desktop shell: the first Tauri window

_Status: built, third slice, 2026-10-17 (the second slice's IA, now on unicron's six security components from `@unicron/ui` 0.4.0 / `@unicron/tokens` 0.2.0: SecretField, CodeDisplay, the CrownJewel treatment and gate, LargeTypeView, ApprovalSheet as a seam, RiskTierBadge; plus the locked wash, the idle-lock toast, restore from the trash and conflict resolution). Companion to [`11-desktop-design-brief.md`](11-desktop-design-brief.md), which owns how it will look; this page owns how it is built. What is real: `crates/app` (`lockdown-app`) and `apps/desktop` (web UI + `src-tauri`), with the gates in CI's `desktop` job._

## For future agent

- **TL;DR:** The desktop app is three layers: the core crates, one command layer (`lockdown-app`) that owns the lock timer and the secret-handling rules, and a Tauri 2 shell that maps each command to the webview and supplies only what a shell must (keychain, clipboard, browser, window). The UI is unicron primitives and unicron's six security components, bound to this host in one adapter module each, with Lockdown's app-extension roles and no visual design of its own. Copy never shows the webview a secret; reveal shows bytes for thirty seconds (twenty inside a crown jewel); the core locks on idle, hidden window and sleep whatever the UI does.
- **Reads before this:** [`03-architecture.md`](03-architecture.md) §8–§10, [`07-workspace.md`](07-workspace.md) §1, decisions D11, D15, D47, D61, D68, D69, D70, D73, D74, and the brief.
- **Not here:** the components themselves (unicron, `docs/specs/security/`; this page owns only how the shell binds them, §3 and §5), passkeys, the quick-access palette, persona switching, the extension frame, mobile.

## 1. Layout

```
crates/app/                      lockdown-app: the command layer (Rust, no UI)
  src/lib.rs                     App handle + the worker thread; AppError, ErrorCode; the gates
  src/session.rs                 the open session: identity, store, keys, vault, account, sync, kit, devices, audit
  src/session/codes.rs           codes(): current + next + seconds, never the seed; add_code; code_now (D102)
  src/session/transfer.rs        import / export through lockdown-cxf (D103)
  src/settings.rs                Settings: settings.cbor beside remembered.cbor (D100)
  src/approval.rs                ApprovalKind, Approver, PasswordApprover, route() (D101)
  src/views.rs                   the smart views, classified once for every shell (D104)
  src/lock.rs                    LockPolicy, LockClock: idle / hidden / sleep
  src/revealed.rs                Revealed: secret bytes, zeroed on drop, never Display
  src/secrets.rs                 SecretStore: where the shell keeps the session token
  src/types.rs                   what crosses to a shell (Status, ItemSummary, ItemDetail, CodeView, …)
crates/platform-auth/            lockdown-platform-auth: Touch ID / Windows Hello, no Lockdown deps (D101)
apps/desktop/
  package.json, pnpm-lock.yaml   pnpm 10 root for the web UI (its own, like extension/)
  pnpm-workspace.yaml            the @unicron/tokens override (release tarballs)
  src/main.tsx, App.tsx          locked by default; the lock watch; the areas; the shared item rows
  src/index.css                  @unicron/ui/styles, then the extension layer
  src/styles/app-extension.css   Lockdown's tier-2 roles (§4) and the density knobs
  src/core/client.ts             the command surface as the UI sees it (one method per command)
  src/core/secret-bytes.ts       decode once, wipe; encode for the bridge
  src/core/types.ts              the Rust types, field for field
  src/app/lock-watch.ts          visibility and activity → the core; lock-state poll
  src/app/settings.tsx           the settings file as React context; density → <html data-density>
  src/app/gate.tsx               the approval gate on unicron's CrownJewelGate: collects a proof, the core judges it, calls again
  src/app/locked.ts              the core's lock state as context, for the components that mask on `locked`
  src/app/shell.tsx              the sidebar: personas, All items, smart views, Devices · Recovery · Status · Settings
  src/components/master-detail.tsx
  src/features/unlock/           first run (first-run.tsx: three screens + the proof panel), the lock screen, locked-wash.tsx
  src/features/vault/            list (list-model.ts, item-row), detail layouts by kind, login editor
    item-field.tsx               one field: plain rows, or unicron's SecretField bound to reveal / copy_secret / clipboard_holds
    code-display.tsx             unicron's CodeDisplay fed by `codes`, copy through copy_code
    crown-jewel.tsx              unicron's CrownJewelSurface with host="app"; the proof routed through app/gate.tsx
    large-type.tsx               "Show large type": unicron's LargeTypeView fed by reveal (Wi-Fi) or the code valid now
  src/features/approvals/        unicron's ApprovalSheet bound to a PendingApproval; the seam until the core carries requests
  src/features/codes/            use-codes.ts (the clock) and the Codes view on CodeDisplayList
  src/features/devices/          device list, revoke behind the gate
  src/features/recovery/         kit status, drill entry point
  src/features/status/           the proofs with Check now (audit), account and sync, this device; once-page (the kit through LargeTypeView print)
  src/features/settings/         lock timing, clipboard, density, reveal gesture, approvals, server, import/export
  src/features/transfer/         import (File API → core) and export (gate → bytes → download)
  src/features/account/          the register / log-in and kit dialogs
  patches/@unicron__ui.patch     a consumer-side hotfix on the pinned release (§3, SecretField row); dropped when unicron ships the fix
  src-tauri/
    Cargo.toml                   lockdown-desktop; the `shell` feature (§7)
    src/lib.rs                   run(): plugins, keychain, App::start, the platform approver, window events
    src/approver.rs              PlatformApprover over lockdown-platform-auth (no Tauri needed)
    src/commands.rs              one #[tauri::command] per operation, all off the main thread
    src/clipboard.rs             the clipboard rule (§3) incl. clipboard_holds, unit-tested without Tauri
    tauri.conf.json              window, CSP, build.features = ["shell"]
    capabilities/default.json    core:default only
dsa.config.json                  measurement (root); baseline at apps/desktop/.dsa/baseline.json
```

`lockdown-app` runs the session on one worker thread: the vault holds a `Box<dyn Signer>` and the keystore an `Rc`, neither `Send`, so the `App` handle (clonable, `Send + Sync`) sends one closure at a time to that thread and waits. One call at a time is also the mutex; two windows never interleave on the vault.

**Where the data is.** The same places as the CLI (D47): the identity root (D15) and the `lockdown` directory beside it. The desktop app and `lockdown` on one machine share `device-key.cbor`, so they are one device named `Lockdown on <hostname>` (D68, D74). `LOCKDOWN_ROOT` and `LOCKDOWN_DATA` move both for a second profile. `sync.cbor` keeps the CLI's shape; with the keychain installed the session token lives there and the file holds an empty string in its place, and the file also gains `last_sync_at`, which the CLI ignores. `transparency.cbor` (D79) is shared too: the view of the server's log one of them verifies is the view the other continues from. `settings.cbor` (D100) sits beside them: density, reveal gesture, clipboard timer, lock timers, the suggested server; preferences only, user-readable only, read by the app at start and written through `set_settings`.

**The CLI still carries its own copy** of these flows (`bins/cli/src/app.rs`). Moving it onto `lockdown-app` is a later pass, kept out of this one because the CLI was mid-change.

## 2. Command surface

Every row is one `App` method (Rust), one `#[tauri::command]`, and one `Client` method (TypeScript). Blocking calls run on a worker thread; the webview never waits on the main thread.

| Command | Rust (`lockdown_app::App`) | Needs the vault open | Returns to the webview | Notes |
|---|---|---|---|---|
| `probe` | `probe()` | no | `Probe` | identity at the root? device key here? account remembered? |
| `open_vault` | `open_or_create(create)` | no | `Status` | `create` makes the identity and vault when missing (first run, or after join); with an account it also pulls the vault |
| `unlock` | `unlock()` | no | `Status` | opens through the device keystore, offline |
| `lock` | `lock()` | no | — | drops every key; the shell also forgets its clipboard note |
| `lock_state` | `lock_state()` | no | `LockState` | locked? why? the timers |
| `touch` | `touch()` | no | — | activity without a call (throttled, 10 s) |
| `set_hidden` | `set_hidden(bool)` | no | `LockState` | `visibilitychange`; may lock at once |
| `set_policy` | `set_policy(LockPolicy)` | no | `LockState` | idle / hidden / sleep-gap seconds, `null` = never |
| `list` | `list()` | yes | `ItemSummary[]` | nothing hidden in it |
| `show` | `show(id, false)` | yes | `ItemDetail` | hidden fields carry a mask only |
| `reveal` | `reveal(id, field)` | yes | **bytes** (`ArrayBuffer`) | one hidden field; refused for plain fields |
| `copy_secret` | `reveal(id, field)` + clipboard | yes | seconds until clear | the value never reaches the webview (§3) |
| `add_login` | `add_login(site, user, bytes, title)` | yes | item id | password arrives as bytes |
| `edit_login` | `edit_login(id, title?, user?, bytes?)` | yes | version number | a new version; history kept |
| `trash` | `trash(id)` | yes | — | soft delete; grace period |
| `restore` | `restore(id)` | yes | — | a trashed item comes back live, history kept; refused for a live or purged one |
| `status` | `status()` | yes | `Status` | identity, device, persona, vault head, devices, account, the server's log as last verified (`AccountStatus.transparency`) |
| `account_register` | `account_register(…)` | yes | notes + kit page as **bytes** | OPAQUE; makes the kit when none exists (D69, D70) |
| `account_login` | `account_login(…)` | no* | notes | *on an empty device enrols it from the account; else a fresh session |
| `recover` | `recover(…)` | no | notes | the whole kit on an empty device |
| `sync` | `sync()` | yes | `SyncOutcome` | identity records first (D71), then key transparency, pull / merge / push / head, key transparency again (D78, D80), as `lockdown sync`; `SyncOutcome.transparency` is the verified view; a refused proof or audit is `{ code: "refused" }` with the audit's own sentence |
| `kit_create` | `kit_create(replace, password?)` | yes | notes + page as **bytes** | D69; with an account the slot takes the new Secret Key (D70) |
| `kit_verify` | `kit_verify(sk, code, password?, drill)` | yes | notes | changes nothing unless `drill` |
| `device_list` | `device_list()` | yes | `DeviceInfo[]` | |
| `device_revoke` | `device_revoke(id, gate)` | yes | notes | D73; needs the account; through the `Approve` gate (D101) |
| `open_site` | — | no | — | http and https only, through the opener plugin |
| `get_settings` | `settings()` | no | `Settings` | `settings.cbor` as last read or written (D100) |
| `set_settings` | `set_settings(Settings)` | no | `Settings` | clamped into range, saved user-only, the lock timers applied from it |
| `approval_route` | `approval_route(kind)` | no | `{ route, method }` | which check `use` / `reveal` / `approve` would go to right now: `platform` (with its name), `password`, `window` |
| `reveal` | `reveal(id, field, gate)` | yes | **bytes** | as before; a crown jewel's goes through the `Reveal` gate first |
| `copy_secret` | `reveal_to_use(id, field, gate)` + clipboard | yes | seconds until clear | as before; a crown jewel's goes through the `Use` gate first |
| `codes` | `codes()` | yes | `CodeView[]` | every code item and every code inside another item: current, next, seconds left, period, digits, variant, issuer, account, warnings; never the set-up (D102) |
| `copy_code` | `code_now(item, field, next, gate)` + clipboard | yes | seconds until clear | the code valid now (`which: "next"`: the one after it, offered in a code's last seconds), on the clipboard path with the same timer; inside a crown jewel through the `Use` gate |
| `clipboard_holds` | — (shell) | no | `bool` | whether the clipboard still holds what Lockdown last copied, for the copy control's own countdown; `hash` names the copy (the component's SHA-256 of the bytes it was handed, or `null` when the shell copied by reference) and is compared with the shell's record only, never with the clipboard |
| `clipboard_clear` | — (shell) | no | — | clears now, under the same rule as the timer: only if the clipboard still holds our value |
| `open_clock_settings` | — (shell) | no | `bool` | the OS date pane for the code display's "Fix clock" (macOS, Windows); `false` on Linux and the UI says where to look |
| `add_code` | `add_code(uri, title?, into?)` | yes | item id | from an `otpauth://` or `steam://` URI, checked first; inside a Login (`into`) or as its own Code item |
| `personas` | `personas()` | yes | `PersonaInfo[]` | the identity's personas, the open one marked (read-only switcher for now) |
| `audit` | `audit()` | yes | lines | `lockdown audit`: the server's log verified from the start, in the audit's own words ("Check now" on Status) |
| `import_items` | `import_items(bytes, format)` | yes | `ImportReport` | CXF or CSV bytes from the webview's file picker, parsed as hostile; refused with the reason (D103) |
| `export_items` | `export_items(format, gate)` | yes | **bytes** | the live vault as CXF or CSV, after the `Reveal` gate (D103) |

A call on a locked vault answers `{ code: "locked" }`; the UI shows the unlock screen. Other codes: `not_set_up`, `refused` (the core's words), `stopped`, and `approval_needed` with `detail: "password" | "window"`: a gated call that nothing confirmed yet; `app/gate.tsx` asks and calls again with a `gate` (`{ password }` or `{ confirmed: true }`). The platform check (Touch ID, Windows Hello) never reaches the webview: the core runs it on the worker thread and answers confirmed or refused.

`list` now returns trashed items too (`state: "trashed"`), each row carrying the facts the smart views are built from (`methods`, `passkeys`, `codes`, `expires_at`, `conflict_copy`, `warnings`, `views`; D104). `ItemSummary.views` is computed in `lockdown_app::views` so every shell draws the same sidebar.

## 3. Secret handling, as implemented

| Rule (03-architecture §8) | Where it holds |
|---|---|
| No secret through a JavaScript string on the bridge | `reveal`, `account_register`, `kit_create` answer with raw bytes (`tauri::ipc::Response`); nothing hidden is ever in the JSON of `show`, `list` or `status` (tested: `show_is_masked_until_asked_and_reveal_comes_back_as_bytes`). Passwords typed into the UI are sent as byte arrays once and the form is reset. |
| Buffers wiped | `Revealed` zeroes on drop and has no `Display`; `Debug` prints the length (tested). The webview decodes a buffer once and `fill(0)`s it (`decodeAndWipe`); the string exists only in the component that shows it, for at most 30 s, and is dropped on hide, on window blur, on unmount and on lock. |
| Copy with a clipboard timer | `copy_secret` reveals on the Rust side and writes the clipboard from there. After the timer (default 30 s, range 10–90) the shell reads the clipboard back and clears it **only if it still holds what we wrote** (a BLAKE3 hash is kept, never the value) and **only if no later copy happened** (a generation counter). Tested in `clipboard.rs` without Tauri. A plain field (username, URL) copies in the webview with no timer. **Under unicron's SecretField** (`item-field.tsx`): `host.copy(bytes)` zeroes what it is handed and asks `copy_secret` by id and field, so the value still goes core → clipboard; `host.clipboardHolds` / `host.clear` are `clipboard_holds` / `clipboard_clear`, so "Cleared" on the control is only ever said about our own value. **Contract gap for unicron:** the component's copy path calls `getSecret` before `host.copy`, so on a standard field the bytes cross the bridge once (and are wiped unused) for every copy; a `host.copy` without bytes (copy by reference) would close it. Inside a crown jewel the adapter returns an empty buffer for a `use` proof, so nothing crosses (D105). |
| Lock enforced by the core | `LockPolicy` + `LockClock` live in `lockdown-app`; the worker thread checks them before every call and once a second. Idle (no call or `touch`), hidden window past its timeout, and a wall-clock jump larger than the sleep gap each drop the session. The UI reports `visibilitychange` and activity and polls `lock_state` every 3 s so a lock the core decided alone reaches the screen. Tests: `the_idle_timer_locks_and_unlock_opens_again`, `a_hidden_window_locks_after_its_timeout_and_a_request_locks_at_once`, the four `lock::tests`. |
| Nothing secret in React state beyond the revealed field's lifetime | `App` keeps `Status` (no secrets) and `LockState`; `SecretField` keeps the one revealed string; the kit dialog keeps its page while open. |
| What a webview cannot do | A typed password is a webview string between keystroke and submit; a revealed value is a string while drawn. The native shells will use native fields; the desktop says so here instead of pretending. |
| Capabilities and CSP | `core:default` only; no `shell`, `fs`, `http` or `clipboard` permission reaches the webview. CSP: `default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self' data:; connect-src ipc: http://ipc.localhost; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'; frame-src 'none'` (`'unsafe-inline'` for styles only, because Base UI sets inline `style` attributes). Sync traffic is Rust-side `reqwest` over rustls, never from the webview. |
| A code's set-up and a passkey's material are secrets | `show` masks `totp` and `passkey` fields like a password (tested: `codes_come_as_values_and_timing_never_the_seed` checks the seed is in no JSON a shell sees). The Codes view and a Login's detail draw values the core computed (`codes`); `copy_code` copies the current value on the Rust clipboard path. |
| Protected things ask first (D101) | `reveal`, `copy_secret`, `copy_code`, `device_revoke` and `export_items` run the gate in the core before the session is touched: the platform check on the worker thread, or the password the shell typed into its own window and sent once (checked by an OPAQUE sign-in, wiped after). The password is asked for through unicron's `CrownJewelGate` at its password level and the "are you sure" confirm is a plain dialog, both in `app/gate.tsx`, in the app window, never in a page (D98). A `CrownJewelSurface`'s `requestProof(kind)` asks the way `approval_route` says (platform: nothing to collect, the OS asks on the call; password: the gate; window: the confirm) and keeps what it collected for the next call; the core judges it when the call arrives, nothing is cached. A wrong password re-asks with the tries left; the third wrong one locks the vault (D106). Tests: `without_a_platform_check_or_an_account_the_gate_is_the_window`, `a_platform_check_takes_use_and_approve_and_stands_in_for_the_password`, `with_an_account_the_reveal_gate_is_the_password_checked_by_opaque` (against the in-process server), the `approval::tests`. |
| Export is one file of secrets | `export_items` comes back as bytes behind the gate; the webview wraps them in a Blob, triggers a download, zeroes its buffer and revokes the URL. The app keeps no copy. Import bytes come from the webview's own `<input type="file">` (the File API needs no Tauri capability) and are zeroed after the call. |
| Reveal gesture (D97) | unicron's SecretField with `revealMode` from `settings.cbor`: hold by default (fetched on pointer down / Space, drawn only if still held when the bytes arrive, dropped on release, leave, cancel, blur), toggle with the 30 s auto-hide. Inside a `CrownJewelSurface` the reveal is a tap with the 20 s crown-jewel lifetime, because a hold cannot outlive the gate's dialog or the OS prompt (D106). Tests: `item-field.test.tsx`, `crown-jewel.test.tsx`. **Patched for now:** SecretField 0.4.0 unmounts the span it writes the value into while `getSecret` is pending, then checks that span after the await, so any reveal slower than a microtask (every IPC call) was silently dropped; `patches/@unicron__ui.patch` keeps the span mounted and draws the skeleton inside it (pnpm `patchedDependencies`). Reported to unicron; the patch goes with the fix. |
| Session token | `lockdown_app::secrets::SecretStore`; the shell installs the OS keychain (`keyring` 3.6: macOS Keychain, Windows Credential Manager, Secret Service on Linux) under service `design.snds.lockdown`, key `session:<login>@<server>`. |
| The server is held to its log, as the CLI holds it (04-security §6) | `Session::sync` begins a `KtSession` from `transparency.cbor`, audits the account's entries with the shared `lockdown_sync::DeviceExpectations` (the identity's devices, the synced manifest, the vault's heads up to the confirmed `seq`), runs `sync_vault_with` under it, audits again and remembers the view; the first pull after a join goes through `bootstrap_vault_with` under the same session. A rollback, a split view, a swapped or unlisted device key stops the sync before a byte is written and reaches the Status screen as the audit's words. Tests: `crates/app/tests/sync_transparency.rs` (a clean sync and a join, a swapped device key, an older tree head) against the in-process server. |

## 4. The app-extension layer

`apps/desktop/src/styles/app-extension.css`, after `@unicron/ui/styles`. Tier-2 aliases only, swapped in `.dark`, exposed through its own `@theme inline` block. No raw values.

| Role | Light | Dark | Used for |
|---|---|---|---|
| `--surface-chrome` | `--g-neutral-50` | `--g-neutral-900` | rail and list pane |
| `--surface-canvas` | `--g-neutral-0` | `--g-neutral-950` | detail pane |
| `--surface-selected` | `--g-neutral-100` | `--g-neutral-800` | the selected row |
| `--surface-locked` | `--g-neutral-100` | `--g-neutral-900` | the wash behind the unlock screen |
| `--secure` | `--g-green-500` | `--g-green-400` | locked / secure / verified, always with a word |
| `--danger` | `--destructive` | `--destructive` | revoke, trash |
| `--attention` | `--warning` | `--warning` (swapped by the tokens) | needs attention (kit not made, expiring, conflict copy) |
| `--area-codes` | `--g-blue-500` | `--g-blue-400` | the Codes view |

The six components take no app-extension tokens: their roles (`warning`, `info`, `protected`) and knobs (`--c-crown-jewel-*`, `--c-secret-field-*`, `--c-code-*`, `--c-large-type-*`, `--c-approval-*`, `--c-risk-tier-*`) ship in `@unicron/tokens` 0.2.0, and nothing here redefines them.

The UI uses `bg-surface-*`, `text-secure`, `text-attention` and `text-area-codes` from this layer; everything else is unicron's defaults. The tier-3 knobs gained density (D92): `--c-row-height`, `--c-row-pad-y`, `--c-row-gap`, `--c-group-pad-y`, each a multiple of Tailwind's `--spacing` from the unicron stylesheet (comfortable rows 10 units = 40 px, compact 8 = 32 px), switched by `:root[data-density="compact"]` and applied to any element marked `data-row` or `data-group-header`; no raw lengths, no `className` on a primitive.

## 5. The brief's screens, as built

Sean's answers (D92–D99) are in the shell; the brief's screen inventory (§3.2 there) carries a status column. What each decision became:

- **Density (D92):** `settings.density`, `<html data-density>`, the knobs in §4. Settings → Appearance.
- **Sort and group (D93):** `features/vault/list-model.ts` (pure, tested): title A–Z grouped by kind with counts by default; the Sort menu (title / recently changed) and the group toggle (by kind / none) in the list header. A Login row keeps its methods inside it: password, passkey, code glyphs, each with its word.
- **First run (D94):** `features/unlock/first-run.tsx`: screen 1 the tagline and the doors (Create / Join / Recover), screen 2 the account and the kit (or "This device only"), screen 3 the recovery picks; one sentence each, detail behind "Learn more" (a Dialog). Tested: `first-run.test.tsx`.
- **Proof moments (D95):** one panel of three lines after an account was created, then only on Status with **Check now**, which runs `audit` and shows its words pass or fail. The OPAQUE and Secret-Key sentences were removed from the join form, the register dialog and the kit section.
- **Reveal (D97):** unicron's SecretField in `item-field.tsx`, hold by default, toggle by setting (§3).
- **Biometric approvals (D99):** `lockdown_app::approval` + `crates/platform-auth` + `src-tauri/src/approver.rs` (D101). Settings → Approvals says which check this computer has.
- **Sidebar and smart views:** `app/shell.tsx` (persona switcher read-only, All items, Passkeys, Codes, Wi-Fi, Expiring, Needs attention, Deleted with counts; Devices · Recovery · Status · Settings; Lock), classification in the core (D104).
- **Codes view:** `features/codes/` on unicron's `CodeDisplayList` / `CodeDisplay`, every code with its countdown on the host clock (`use-codes.ts`) and refreshed from the core at rollover; the next code in the last seconds; add a code from its URI, inside a Login or alone.
- **Devices and Recovery** are their own screens; Status is the proofs, account and sync, and this device.
- **Detail layouts (§3.1):** Login (methods → websites → details), Code (display full width with "Show large type", then the masked set-up), Wi-Fi (fields with "Show large type" on the hidden ones), Note (body), Card (number in groups of four, CVC and PIN masked); every other kind the generic field list. A crown jewel's detail sits in unicron's `CrownJewelSurface` (`host="app"`, `appName="Lockdown"`, the biometric named as the platform names it); rows carry `CrownJewelMark`. A trashed item offers Restore; a conflict copy and its original each offer "Keep this copy" / "Keep the other" (D108). An item carrying a bridge tier shows `RiskTierBadge` (none does yet: the vault model has no tier, the TypeScript `ItemDetail.tier` is the optional seam).
- **Kit page:** `status/once-page.tsx` sets the Secret Key and the recovery code in `LargeTypeView`'s print variant (every character captioned, groups of four), the page as written underneath; printing hides the app behind the dialog.
- **Locked:** `unlock/locked-wash.tsx` behind the unlock screen (brief §4); ten seconds before an idle lock, one toast with "Stay unlocked" (`app/lock-watch.ts`, D107).
- **Import / export:** Settings → Import and export, `features/transfer/transfer-dialog.tsx` (D103).

**The unicron components, as bound.** Each has one adapter module here and nothing else duplicates what the component owns: `item-field.tsx` (SecretField: `getSecret` = `reveal`, `host.copy` = `copy_secret` by reference, `host.clipboardHolds` / `clear` = the shell's rule, `revealMode` and `clipboardTimer` from the settings, `locked` from the core), `code-display.tsx` (CodeDisplay: the `codes` command's values and timing, `onCopy` = `copy_code` with `which`, `onFixClock` = `open_clock_settings`, no `onHaptic` on desktop, the skew notice dormant until the core measures skew), `crown-jewel.tsx` (CrownJewelSurface with the host-owned `requestProof` in `app/gate.tsx`, which also reuses `CrownJewelGate` for every password gate), `large-type.tsx` and `once-page.tsx` (LargeTypeView: the sheet with the 120 s lifetime, and the print variant; keep-awake and brightness are a phone's hooks), `approvals/approval-sheet.tsx` (ApprovalSheet bound to a `PendingApproval`, `origin="scan"` only for a device code read from the screen in front of the person, D99; **a seam**: nothing in `lockdown-app` produces a pending request yet, a device joins through the account and no agent or bridge asks this device anything, so the Devices screen mounts nothing until the core gains `approvals()` / `approve_request` / `deny_request`), and the detail header (RiskTierBadge on `ItemDetail.tier`, which no item carries yet).

Host-contract gaps found binding them, for unicron: (1) SecretField's copy path fetches the bytes before `host.copy` (§3); (2) SecretField 0.4.0 drops any reveal slower than a microtask (§3, patched here); (3) the gate is judged by the core at call time, not by `CrownJewelGate.onConfirm` at proof time, so the host re-asks with `error` / `attemptsLeft` itself; (4) `CrownJewelSurface.biometricName` has no "no biometric on this device" form, so on a computer without one the host passes "your account password" and the explain copy reads with it; (5) CodeDisplay's `onCopy` hands the host the value as a string, which this host ignores (it copies by id and field); (6) the SecretField shows its own generic error words, so the core's sentence is passed through `description`.

Still open for the brief: persona switching, item editors for kinds other than Login, the quick-access palette, approvals once the core carries requests, the skew notice.

## 6. How to run

```sh
cd apps/desktop
pnpm install                               # pnpm 10; the lockfile pins the unicron tarballs by hash and applies patches/
pnpm tauri dev                             # Vite on :5173 + the Tauri window; features = ["shell"] from tauri.conf.json
pnpm typecheck && pnpm lint && pnpm test   # tsc, oxlint, Vitest (jsdom)
pnpm build                                 # vite build → dist/ (what the shell embeds)
pnpm ds:scan && pnpm ds:patterns && pnpm ds:ratchet
cargo test -p lockdown-app                 # the command layer, incl. the audited sync against the in-process server
cargo test -p lockdown-desktop             # the clipboard rule and the approver's Linux path, no Tauri needed
cargo check -p lockdown-platform-auth --target aarch64-apple-darwin   # Touch ID path compile-checked
cargo check -p lockdown-platform-auth --target x86_64-pc-windows-msvc # Windows Hello path compile-checked
cargo clippy -p lockdown-desktop --all-targets --features shell -- -D warnings
pnpm tauri build                           # bundles; never run with a shared, tight target dir
```

Linux needs the WebKitGTK stack before anything with `--features shell`: `libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libayatana-appindicator3-dev libdbus-1-dev libxdo-dev pkg-config`. CI's `desktop` job installs them; the root `check` job never needs them (§7). `LOCKDOWN_ROOT=/tmp/snds/identity pnpm tauri dev` runs against a scratch identity.

## 7. The `shell` feature

`lockdown-desktop` is a workspace member, but Tauri and the plugins are optional dependencies behind its `shell` feature, off by default. Without it the crate is the clipboard rule and a stub `main`; with it, the window. Reason: `cargo clippy --workspace` and `cargo test --workspace` run on CI without a platform SDK (the comment in the root `Cargo.toml`), and the existing `check` job must not start needing WebKitGTK. `tauri.conf.json` turns the feature on (`build.features`), so `pnpm tauri dev` and `pnpm tauri build` need no flag. Mailbird keeps its shell out of `default-members` instead; Lockdown's root jobs use `--workspace`, which ignores that list, hence the feature.

## 8. Measurement

`dsa.config.json` at the repo root scans `apps/desktop/src/**/*.tsx` against `@unicron/ui`; the ratchet baseline is committed at `apps/desktop/.dsa/baseline.json` (the reports beside it are regenerated and ignored). First scan, 2026-10-17: 21 `className` overrides on system components, 0 style overrides, 0 spread props, 0 look-alikes; one pattern candidate (`Field` + label + input, three times across two files). Third slice: 15 overrides (the placeholders' rows went with them), 0 / 0 / 0; the baseline moved down to 15. The scripts run from `apps/desktop` with `--config ../../dsa.config.json`; the pin on `@snds/dsa` is the same commit unicron uses, and pnpm 10.28 records it in the lockfile as the GitHub tarball of that commit.

## 9. Decisions proposed

The first shell's rows landed as D83–D91. The second slice added D100 (the settings file), D101 (approval kinds, routes and fallbacks), D102 (what `codes` exposes), D103 (import/export, export gated) and D104 (smart views classified in the core). The third slice added D105 (copy stays core-side under unicron's SecretField; `clipboard_holds` answers from the shell's own record; a crown jewel's copy fetches nothing for the webview), D106 (the crown-jewel proof is collected by the component's gate and judged by the core at call time; three wrong passwords lock the vault; inside the surface a reveal is a tap with the 20 s lifetime; a code inside a crown jewel is gated for copy), D107 (the idle-lock toast with "Stay unlocked" and the locked wash), D108 (restore from the trash; a conflict is resolved by keeping one copy and trashing the other, after a confirm) and D109 ("Fix clock" opens the OS date settings where a URL exists; large type on desktop is a 120 s sheet; a code's large type is the code valid when pressed); see `decisions.md`. Each user-visible one is **flagged for Sean** there. One build choice, not a decision row: the pinned `@unicron/ui` release carries a pnpm patch (§3) until unicron ships the fix. The platform check is compile-checked for macOS and Windows and run only on Linux here (where it is absent by design); the first run on real hardware should confirm the prompt and that cancelling is a no.

The first shell's rows, as proposed then (numbered D83–D91 when accepted; each user-visible or a security default, so **flagged for Sean**):

| Proposed | Decision | Why | Tradeoff |
|---|---|---|---|
| D83 | **Local unlock on desktop is the device keystore, not a master password.** An enrolled device opens the vault with its own key (D11); the account password appears only to join or recover. The unlock screen has an Unlock button and says so. | The core has no local password slot; a field that checked nothing would be theatre. The keystore trait's `unlock` is where the platform's own check (Touch ID, Windows Hello) goes when the hardware backends land. | Until then, on Linux and the file keystore, Lock protects memory and the screen, not the disk; the OS login does that. |
| D84 | **Auto-lock defaults:** idle 10 minutes, window out of view 5 minutes, and at once when the machine wakes from a sleep longer than 30 seconds; idle is a setting (1 minute to 4 hours, or never), hidden-lock a switch. Enforced in `lockdown-app`. | Brief §4 says 10 minutes idle and on sleep; the hidden-window timer stands in for "on screen lock" until the OS events are wired. | A long hidden timeout means a minimised window stays open for five minutes. |
| D85 | **Clipboard clear: 30 seconds by default, 10–90 allowed, cleared only if the clipboard still holds our value and no later copy happened.** The copy goes core → clipboard on the Rust side. | Brief §4; never destroying the person's own later copy; never handing the value to the webview. | Reading the clipboard back at clear time is a read of whatever is there; it is compared by hash and dropped. |
| D86 | **The desktop app and the CLI share the data directory and the device key**, so one machine is one Lockdown device. The session token moves to the OS keychain when the desktop app is present; the CLI then logs in on its own. | D68 counts devices per app; the CLI and the window are the same app. | A CLI session and a desktop session are separate bearer tokens. |
| D87 | **Where the device keystore secret lives per OS**, today and intended: today the identity crate's file keystore (`device-key.cbor`, user-only) everywhere; intended macOS Secure Enclave, Windows TPM through Windows Hello, Android StrongBox (the crate's declared backends), Linux staying on the file. The desktop shell's keychain holds only the session token until then. | The crate owns the keystore backends; the shell must not grow a second one. | Until the hardware backends land, the desktop is as safe as the home directory, same as the CLI. |
| D88 | **CSP as in §3; capabilities `core:default` only; sync only from Rust.** | 03-architecture §8 and the brief's clickjacking rules. | Any future webview feature (a file picker for import) is a named capability added on purpose. |
| D89 | **Tauri behind the `shell` feature** (§7). | Keeps the root `--workspace` gates free of a platform SDK without editing the existing CI job. | A developer who runs `cargo run -p lockdown-desktop` without the feature gets a stub that says so. |
| D90 | **Five MPL-2.0 crates allowed by name in `deny.toml`** (`option-ext` via `dirs`; `selectors`, `cssparser`, `cssparser-macros`, `dtoa-short` via Tauri's build-time CSP injection). | Tauri cannot be built without them; MPL-2.0 is file-level weak copyleft and none of those files is modified or derived from. Not added to the permissive allow-list, which stays as it was. | If the project wants a permissive-only tree, the desktop shell needs another toolkit. |
| D91 | **The webview's unavoidable strings** are named, not hidden: a typed password between keystroke and submit, a revealed value while drawn. Both are dropped at once afterwards; the native shells use native fields. | Honesty about what a web engine can do. | None beyond the statement. |
