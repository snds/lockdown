# Desktop shell: the first Tauri window

_Status: built, fourth slice, 2026-10-17: every kind a person makes by hand is creatable, viewable and editable through one generic path driven by the core's per-kind templates, with a password generator in the core and the in-app quick-access palette (§10), on `@unicron/ui` 0.6.0 / `@unicron/tokens` 0.3.0; a follow-up moved it to `@unicron/ui` 0.7.1 / `@unicron/tokens` 0.5.0, split the app's text roles from its fills to clear the new contrast floors, and adopted the security components' by-reference host API (§11); a second follow-up moved it to `@unicron/ui` 0.8.0 / `@unicron/tokens` 0.6.0, replaced the shell's workarounds for the seven host-contract gaps of §10.6 with what 0.8.0 added, gave the editor a strength word from zxcvbn in the core, and made HOTP's Next code work (§12). The third slice put the second slice's IA on unicron's six security components (SecretField, CodeDisplay, the CrownJewel treatment and gate, LargeTypeView, ApprovalSheet as a seam, RiskTierBadge) with the locked wash, the idle-lock toast, restore from the trash and conflict resolution. Companion to [`11-desktop-design-brief.md`](11-desktop-design-brief.md), which owns how it will look; this page owns how it is built. What is real: `crates/app` (`lockdown-app`) and `apps/desktop` (web UI + `src-tauri`), with the gates in CI's `desktop` job._

## For future agent

- **TL;DR:** The desktop app is three layers: the core crates, one command layer (`lockdown-app`) that owns the lock timer and the secret-handling rules, and a Tauri 2 shell that maps each command to the webview and supplies only what a shell must (keychain, clipboard, browser, window). The UI is unicron primitives and unicron's six security components, bound to this host in one adapter module each, with Lockdown's app-extension roles and no visual design of its own. Copy never shows the webview a secret; reveal shows bytes for thirty seconds (twenty inside a crown jewel); the core locks on idle, hidden window and sleep whatever the UI does.
- **Reads before this:** [`03-architecture.md`](03-architecture.md) §8–§10, [`07-workspace.md`](07-workspace.md) §1, decisions D11, D15, D47, D61, D68, D69, D70, D73, D74, and the brief.
- **Not here:** the components themselves (unicron, `docs/specs/security/`; this page owns only how the shell binds them, §3 and §5), passkey creation, the system-wide hotkey, persona switching, the extension frame, mobile.

## 1. Layout

```
crates/app/                      lockdown-app: the command layer (Rust, no UI)
  src/lib.rs                     App handle + the worker thread; AppError, ErrorCode; the gates
  src/session.rs                 the open session: identity, store, keys, vault, account, sync, kit, devices, audit
  src/session/codes.rs           codes(): current + next + seconds, never the seed; add_code; code_now (D102)
  src/session/transfer.rs        import / export through lockdown-cxf (D103); crown jewels classed on import
  src/session/items.rs           add_item / edit_item on every kind, templates to shells, a bare set-up key made a link (D110)
  src/settings.rs                Settings: settings.cbor beside remembered.cbor (D100)
  src/approval.rs                ApprovalKind, Approver, PasswordApprover, route() (D101)
  src/views.rs                   the smart views, classified once for every shell (D104)
  src/lock.rs                    LockPolicy, LockClock: idle / hidden / sleep
  src/revealed.rs                Revealed: secret bytes, zeroed on drop, never Display
  src/secrets.rs                 SecretStore: where the shell keeps the session token
  src/types.rs                   what crosses to a shell (Status, ItemSummary, ItemDetail, CodeView, …)
crates/vault/src/template.rs     every kind's fields as data: control, required, crown jewel, fill rule, generator, section (§10)
crates/vault/src/compose.rs      a shell's add or edit checked against the template; an edit names every field (D110)
crates/crypto/src/generator.rs   the password generator (D112)
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
  src/app/templates.tsx          the core's templates as context, loaded once (D110)
  src/app/shell.tsx              the sidebar: personas, All items, smart views, Devices · Recovery · Status · Settings
  src/components/master-detail.tsx
  src/features/unlock/           first run (first-run.tsx: three screens + the proof panel), the lock screen, locked-wash.tsx
  src/features/vault/            list (list-model.ts, item-row on ItemMain with its copy button), detail layouts by kind
    item-editor.tsx              the one editor for every kind (editor-model.ts: rows and the change list; field-control.tsx)
    new-item-menu.tsx            "New item": every creatable kind from the templates
    quick-copy.ts                a row's one copy through copy_secret / copy_code and the gates
  src/features/palette/          the quick-access palette on CommandDialog (palette-model.ts: what it searches)
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
  src-tauri/
    Cargo.toml                   lockdown-desktop; the `shell` feature (§7)
    src/lib.rs                   run(): plugins, keychain, App::start, the platform approver, window events
    src/approver.rs              PlatformApprover over lockdown-platform-auth (no Tauri needed)
    src/commands.rs              one #[tauri::command] per operation, all off the main thread
    src/items.rs                 add_item / edit_item's field changes from JSON (secrets as byte arrays), tested without Tauri
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
| `add_login` | `add_login(site, user, bytes, title)` | yes | item id | password arrives as bytes; now the narrow form of `add_item` |
| `edit_login` | `edit_login(id, title?, user?, bytes?, gate)` | yes | version number | a new version; history kept; every other field kept; a crown jewel's through the `Use` gate (D110) |
| `templates` | `templates()`, `custom_kinds()` | no | `{ kinds: TemplateView[], custom_kinds }` | every kind's fields as the core holds them (§10); static |
| `password_strength` | `password_strength(candidate, context)` | no | `0`–`4` or `null` | zxcvbn's guess score for a password being typed in the editor, the form's plain words as context; the candidate as bytes, wiped; never a stored secret (D116, §12) |
| `add_item` | `add_item(kind, title?, crown_jewel, changes)` | yes | item id | any creatable kind; `fields` is a list of `{ field_id?, label?, kind?, action, text? \| secret? \| flag? }`, a secret always a byte array (D110) |
| `edit_item` | `edit_item(id, title?, changes, gate)` | yes | version number | names every field the item has: `keep` (a secret not retyped never crosses back), `set`, `generate`, `remove`; a crown jewel's through the `Use` gate |
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
| `codes` | `codes()` | yes | `CodeView[]` | every code item and every code inside another item: current, next, seconds left, period, digits, variant, issuer, account, warnings, and an HOTP code's counter; never the set-up (D102) |
| `advance_code` | `advance_code(item, field, gate)` | yes | `{ code, counter, version }` | HOTP's Next code: the counter moves on in a new item version, so it syncs; a time-based code is refused; a crown jewel's through the `Use` gate (D117) |
| `copy_code` | `code_now(item, field, next, gate)` + clipboard | yes | seconds until clear | the code valid now (`which: "next"`: the one after it, offered in a code's last seconds), on the clipboard path with the same timer; inside a crown jewel through the `Use` gate |
| `clipboard_holds` | — (shell) | no | `bool` | whether the clipboard still holds what Lockdown last copied, for the copy control's own countdown; `hash` names the copy (the component's SHA-256 of the bytes it was handed, or `null` when the shell copied by reference) and is compared with the shell's record only, never with the clipboard |
| `clipboard_clear` | — (shell) | no | — | clears now for copy number `copy` (a `Copied.copy`): only while that copy is still the shell's last write and still on the clipboard (D119) |
| `open_clock_settings` | — (shell) | no | `bool` | the OS date pane for the code display's "Fix clock" (macOS, Windows); `false` on Linux and the UI says where to look |
| `add_code` | `add_code(uri, title?, into?)` | yes | item id | from an `otpauth://` or `steam://` URI, checked first; inside a Login (`into`) or as its own Code item |
| `personas` | `personas()` | yes | `PersonaInfo[]` | the identity's personas, the open one marked (read-only switcher for now) |
| `audit` | `audit()` | yes | lines | `lockdown audit`: the server's log verified from the start, in the audit's own words ("Check now" on Status) |
| `import_items` | `import_items(bytes, format)` | yes | `ImportReport` | CXF or CSV bytes from the webview's file picker, parsed as hostile; refused with the reason (D103) |
| `export_items` | `export_items(format, gate)` | yes | **bytes** | the live vault as CXF or CSV, after the `Reveal` gate (D103) |

A call on a locked vault answers `{ code: "locked" }`; the UI shows the unlock screen. Other codes: `not_set_up`, `refused` (the core's words), `stopped`, and `approval_needed` with `detail: "password" | "window"`: a gated call that nothing confirmed yet; `app/gate.tsx` asks and calls again with a `gate` (`{ password }` or `{ confirmed: true }`). The platform check (Touch ID, Windows Hello) never reaches the webview: the core runs it on the worker thread and answers confirmed or refused.

`list` now returns trashed items too (`state: "trashed"`), each row carrying the facts the smart views are built from (`methods`, `passkeys`, `codes`, `expires_at`, `conflict_copy`, `warnings`, `views`; D104). `ItemSummary.views` is computed in `lockdown_app::views` so every shell draws the same sidebar. Since the fourth slice a row also carries `subtitle` (the template's plain field: a network's name, a card's holder) and `copy` (the quick copy's field and how: `secret` or `code`); a crown jewel's row carries no field at all (D113).

## 3. Secret handling, as implemented

| Rule (03-architecture §8) | Where it holds |
|---|---|
| No secret through a JavaScript string on the bridge | `reveal`, `account_register`, `kit_create` answer with raw bytes (`tauri::ipc::Response`); nothing hidden is ever in the JSON of `show`, `list` or `status` (tested: `show_is_masked_until_asked_and_reveal_comes_back_as_bytes`). Passwords typed into the UI are sent as byte arrays once and the form is reset. |
| Buffers wiped | `Revealed` zeroes on drop and has no `Display`; `Debug` prints the length (tested). The webview decodes a buffer once and `fill(0)`s it (`decodeAndWipe`); the string exists only in the component that shows it, for at most 30 s, and is dropped on hide, on window blur, on unmount and on lock. |
| Copy with a clipboard timer | `copy_secret` reveals on the Rust side and writes the clipboard from there. After the timer (default 30 s, range 10–90) the shell reads the clipboard back and clears it **only if it still holds what we wrote** (a BLAKE3 hash is kept, never the value) and **only if no later copy happened** (a generation counter). Tested in `clipboard.rs` without Tauri. A plain field (username, URL) copies in the webview with no timer. **Under unicron's SecretField** (`item-field.tsx`, a `SecretFieldReferenceHost` since 0.7.1): `host.copyRef()` asks `copy_secret` by id and field, so the value goes core → clipboard and no bytes cross the bridge for a copy, on any field; it resolves with the shell's number for that copy, which the component hands back to `host.clipboardHolds` (`clipboard_holds(copy)`) at expiry, and `host.clear` is `clipboard_clear(copy)` with the same number, so "Cleared" on the control is only ever said about, and the clear only ever runs for, our own copy (D105, D115, D119). CodeDisplay's clear at the end of its countdown names the number `copy_code` gave it the same way. |
| Lock enforced by the core | `LockPolicy` + `LockClock` live in `lockdown-app`; the worker thread checks them before every call and once a second. Idle (no call or `touch`), hidden window past its timeout, and a wall-clock jump larger than the sleep gap each drop the session. The UI reports `visibilitychange` and activity and polls `lock_state` every 3 s so a lock the core decided alone reaches the screen. Tests: `the_idle_timer_locks_and_unlock_opens_again`, `a_hidden_window_locks_after_its_timeout_and_a_request_locks_at_once`, the four `lock::tests`. |
| Nothing secret in React state beyond the revealed field's lifetime | `App` keeps `Status` (no secrets) and `LockState`; `SecretField` keeps the one revealed string; the kit dialog keeps its page while open. |
| What a webview cannot do | A typed password is a webview string between keystroke and submit; a revealed value is a string while drawn. The native shells will use native fields; the desktop says so here instead of pretending. |
| Capabilities and CSP | `core:default` only; no `shell`, `fs`, `http` or `clipboard` permission reaches the webview. CSP: `default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self' data:; connect-src ipc: http://ipc.localhost; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'; frame-src 'none'` (`'unsafe-inline'` for styles only, because Base UI sets inline `style` attributes). Sync traffic is Rust-side `reqwest` over rustls, never from the webview. |
| A code's set-up and a passkey's material are secrets | `show` masks `totp` and `passkey` fields like a password (tested: `codes_come_as_values_and_timing_never_the_seed` checks the seed is in no JSON a shell sees). The Codes view and a Login's detail draw values the core computed (`codes`); `copy_code` copies the current value on the Rust clipboard path. |
| Protected things ask first (D101) | `reveal`, `copy_secret`, `copy_code`, `device_revoke` and `export_items` run the gate in the core before the session is touched: the platform check on the worker thread, or the password the shell typed into its own window and sent once (checked by an OPAQUE sign-in, wiped after). The password is asked for through unicron's `CrownJewelGate` at its password level and the "are you sure" confirm is a plain dialog, both in `app/gate.tsx`, in the app window, never in a page (D98). A `CrownJewelSurface`'s `requestProof(kind)` asks the way `approval_route` says (platform: nothing to collect, the OS asks on the call; password: the gate; window: the confirm) and keeps what it collected for the next call; the core judges it when the call arrives, nothing is cached. A wrong password re-asks with the tries left; the third wrong one locks the vault (D106). Tests: `without_a_platform_check_or_an_account_the_gate_is_the_window`, `a_platform_check_takes_use_and_approve_and_stands_in_for_the_password`, `with_an_account_the_reveal_gate_is_the_password_checked_by_opaque` (against the in-process server), the `approval::tests`. |
| Export is one file of secrets | `export_items` comes back as bytes behind the gate; the webview wraps them in a Blob, triggers a download, zeroes its buffer and revokes the URL. The app keeps no copy. Import bytes come from the webview's own `<input type="file">` (the File API needs no Tauri capability) and are zeroed after the call. |
| Reveal gesture (D97) | unicron's SecretField with `revealMode` from `settings.cbor`: hold by default (fetched on pointer down / Space, drawn only if still held when the bytes arrive, dropped on release, leave, cancel, blur), toggle with the 30 s auto-hide. Inside a `CrownJewelSurface` the reveal is a tap with the 20 s crown-jewel lifetime, because a hold cannot outlive the gate's dialog or the OS prompt (D106); since 0.7.0 the component does this itself and the shell forces nothing. Tests: `item-field.test.tsx`, `crown-jewel.test.tsx`. **Patched for now:** SecretField 0.4.0 unmounted the span it writes the value into while `getSecret` was pending, so any reveal slower than a microtask (every IPC call) was silently dropped; fixed upstream in `@unicron/ui` 0.5.0 (the span stays mounted, skeleton inside it), which this shell now pins. Reported to unicron and fixed in 0.5.0, which this shell pins. |
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
| `--secure` | `--g-green-500` | `--g-green-500` | fill: locked / secure / verified, always with a word |
| `--secure-foreground` | `--g-neutral-0` | `--g-neutral-0` | a label on that fill |
| `--secure-text` | `--g-green-600` | `--g-green-300` | the same as words (Status's facts) |
| `--danger` | `--destructive` | `--destructive` | revoke, trash |
| `--attention` | `--warning` | `--warning` | fill: needs attention (kit not made, expiring, conflict copy) |
| `--attention-foreground` | `--warning-foreground` | `--warning-foreground` | a label on that fill |
| `--attention-text` | `--g-amber-600` | `--g-amber-300` | the same as words (sidebar entry, Status) |
| `--area-codes` | `--g-blue-500` | `--g-blue-500` | fill: the Codes view |
| `--area-codes-foreground` | `--g-neutral-0` | `--g-neutral-0` | a label on that fill |
| `--area-codes-text` | `--g-blue-600` | `--g-blue-300` | the same as words (the Codes sidebar entry) |

The six components take no app-extension tokens: their roles (`warning`, `info`, `protected`) and knobs (`--c-crown-jewel-*`, `--c-secret-field-*`, `--c-code-*`, `--c-large-type-*`, `--c-approval-*`, `--c-risk-tier-*`) ship in `@unicron/tokens` 0.2.0, and nothing here redefines them.

The UI uses `bg-surface-*`, `text-secure-text`, `text-attention-text` and `text-area-codes-text` from this layer (a hue as words is always its `-text` role; the base role is for fills, §11); everything else is unicron's defaults. The tier-3 knobs gained density (D92): `--c-row-height`, `--c-row-pad-y`, `--c-row-gap`, `--c-group-pad-y`, each a multiple of Tailwind's `--spacing` from the unicron stylesheet (comfortable rows 10 units = 40 px, compact 8 = 32 px), switched by `:root[data-density="compact"]` and applied to any element marked `data-row` or `data-group-header`; no raw lengths, no `className` on a primitive.

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

**The unicron components, as bound.** Each has one adapter module here and nothing else duplicates what the component owns: `item-field.tsx` (SecretField: `getSecret` = `reveal`, `host.copyRef` = `copy_secret` by reference, `errorText` = the core's sentence, `host.clipboardHolds` / `clear` = the shell's rule, `revealMode` and `clipboardTimer` from the settings, `locked` from the core), `code-display.tsx` (CodeDisplay: the `codes` command's values and timing, `onCopy` = `copy_code` with `which`, `onFixClock` = `open_clock_settings`, no `onHaptic` on desktop, the skew notice dormant until the core measures skew), `crown-jewel.tsx` (CrownJewelSurface with the host-owned `requestProof` in `app/gate.tsx`, which also reuses `CrownJewelGate` for every password gate), `large-type.tsx` and `once-page.tsx` (LargeTypeView: the sheet with the 120 s lifetime, and the print variant; keep-awake and brightness are a phone's hooks), `approvals/approval-sheet.tsx` (ApprovalSheet bound to a `PendingApproval`, `origin="scan"` only for a device code read from the screen in front of the person, D99; **a seam**: nothing in `lockdown-app` produces a pending request yet, a device joins through the account and no agent or bridge asks this device anything, so the Devices screen mounts nothing until the core gains `approvals()` / `approve_request` / `deny_request`), and the detail header (RiskTierBadge on `ItemDetail.tier`, which no item carries yet).

Host-contract gaps found binding them, for unicron (all but (2) closed by `@unicron/ui` 0.7.0, §11; (2) by 0.5.0): (1) SecretField's copy path fetches the bytes before `host.copy` (§3); (2) SecretField 0.4.0 dropped any reveal slower than a microtask (§3; fixed upstream in unicron 0.5.0); (3) the gate is judged by the core at call time, not by `CrownJewelGate.onConfirm` at proof time, so the host re-asks with `error` / `attemptsLeft` itself; (4) `CrownJewelSurface.biometricName` has no "no biometric on this device" form, so on a computer without one the host passes "your account password" and the explain copy reads with it; (5) CodeDisplay's `onCopy` hands the host the value as a string, which this host ignores (it copies by id and field); (6) the SecretField shows its own generic error words, so the core's sentence is passed through `description`.

Still open for the brief: persona switching, the system-wide palette hotkey, approvals once the core carries requests, the skew notice, the Wi-Fi guest QR card, attachments in the editor.

## 6. How to run

```sh
cd apps/desktop
pnpm install                               # pnpm 10; the lockfile pins the unicron tarballs by hash
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

`dsa.config.json` at the repo root scans `apps/desktop/src/**/*.tsx` against `@unicron/ui`; the ratchet baseline is committed at `apps/desktop/.dsa/baseline.json` (the reports beside it are regenerated and ignored). First scan, 2026-10-17: 21 `className` overrides on system components, 0 style overrides, 0 spread props, 0 look-alikes; one pattern candidate (`Field` + label + input, three times across two files). Third slice: 15 overrides (the placeholders' rows went with them), 0 / 0 / 0; the baseline moved down to 15. Fourth slice: 4 (the Login sheet's three went with it; "Add a code" moved to the same sized dialog as the editor; the kit page takes `DialogContent size`; the list skeletons take the `data-row` knob; the detail's `ItemGroup` keeps its own gap), 0 / 0 / 0; the baseline moved down to 4. The four left: the sidebar's `Separator` and Lock `Button`, the master–detail `Empty`, the unlock screen's `TabsList`. The 0.8.0 follow-up (§12) replaced workarounds that were not overrides (the masked `Textarea`, the `IconButton` in a `CommandShortcut`, the skeletons' `data-row`), so the count stays at 4; none of the four is touched by 0.8.0. The scripts run from `apps/desktop` with `--config ../../dsa.config.json`; the pin on `@snds/dsa` is the same commit unicron uses, and pnpm 10.28 records it in the lockfile as the GitHub tarball of that commit.

## 9. Decisions proposed

The fourth slice added D110 (every kind through one generic path from the core's templates; an edit names every field), D111 (the quick-access palette), D112 (the password generator), D113 (a crown jewel's summary carries no field; rows carry their quick copy) and D114 (the editor's behaviour); see §10. The 0.7.1 follow-up added D115 (a copy is named by the shell's copy number, not a digest of the secret); see §11. The 0.8.0 follow-up added D116 (the strength word from zxcvbn in the core, for a typed password only), D117 (HOTP's Next code is a new item version), D118 (the palette keeps the list's matching rule) and D119 (a clear from the webview names its copy); see §12. The first shell's rows landed as D83–D91. The second slice added D100 (the settings file), D101 (approval kinds, routes and fallbacks), D102 (what `codes` exposes), D103 (import/export, export gated) and D104 (smart views classified in the core). The third slice added D105 (copy stays core-side under unicron's SecretField; `clipboard_holds` answers from the shell's own record; a crown jewel's copy fetches nothing for the webview), D106 (the crown-jewel proof is collected by the component's gate and judged by the core at call time; three wrong passwords lock the vault; inside the surface a reveal is a tap with the 20 s lifetime; a code inside a crown jewel is gated for copy), D107 (the idle-lock toast with "Stay unlocked" and the locked wash), D108 (restore from the trash; a conflict is resolved by keeping one copy and trashing the other, after a confirm) and D109 ("Fix clock" opens the OS date settings where a URL exists; large type on desktop is a 120 s sheet; a code's large type is the code valid when pressed); see `decisions.md`. Each user-visible one is **flagged for Sean** there. One build note, not a decision row: between 0.4.0 and 0.5.0 the pinned `@unicron/ui` carried a pnpm patch for the slow-reveal bug (§3); 0.5.0 ships the fix and the patch is gone. The platform check is compile-checked for macOS and Windows and run only on Linux here (where it is absent by design); the first run on real hardware should confirm the prompt and that cancelling is a no.

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

## 10. Fourth slice: every kind, one editor, the palette

**What changed.** Before this slice the shell could make and change Logins and add Codes. Now every kind a person makes by hand (sixteen of the eighteen in `ItemKind`) is made, shown and changed through one path: `lockdown_vault::template` holds each kind's fields as data, `lockdown_vault::compose` checks what a shell sends against it, `Session::add_item` / `edit_item` and `App::add_item` / `edit_item` carry it, and the `templates`, `add_item` and `edit_item` commands expose it. `add_login` and `edit_login` stay and now ride the same path; `add_code` stays as it was (it can put a code inside an existing Login). A Passkey is view only (a site makes it through a provider); a Consent record is hidden. Rows: D110–D114.

### 10.1 The templates

The table is data in `crates/vault/src/template.rs` (tested against the crown-jewel classifier and the fill rules, and its field ids are the ones `lockdown-cxf` already wrote, so imported and hand-made items have the same shape). Every template but a Note's ends with **Notes** (Markdown, several lines). `title` is common to every item and passed on its own. **CJ** marks the field (or kind) that is a crown jewel by default (02-model §3); **Gen** the fields the generator offers, with its rule (D112); **Fill** the fields ever put into a page (`fill`, or `confirm` = only after a second deliberate action); everything else is never filled.

| Kind (shown as) | Fields, in order (required in **bold**) | CJ | Gen | Fill | Quick copy |
|---|---|---|---|---|---|
| Login | Username, Password, One-time code (link or bare key), Website | — | Password: strong | Username, Password: fill; code: confirm | password (else its code) |
| One-time code | **Set-up key** (link or bare key) | — | — | confirm | code |
| Payment card | **Card number** (groups of four), Name on card, Expires (month), Security code, PIN, Brand (worked out from the number when empty) | — | — | number, name, expiry, code: confirm | card number |
| Note | Note (Markdown body) | — | — | never | — |
| Wi-Fi network | **Network name**, Password, Security (WPA3 · WPA2/WPA3 · WPA2 · WPA · WEP · Open · Enterprise), Hidden network, Enterprise identity | — | Password: readable | never | Wi-Fi password |
| Identity | Given, middle, family name; Email; Phone; Street, City, State or region, Postal code, Country | — | — | all: fill | — |
| Document | Document type (Passport · Driver's licence · ID card · Contract · Certificate · Other), Number, Issued by, Issued, Expires | — | — | never | — |
| Bank account | Bank, Account holder, Account number (secret, groups of four), Routing or sort code, IBAN (secret), SWIFT or BIC, Branch, PIN | — | — | never | account number |
| Crypto wallet | **Recovery phrase** (several lines, masked), Wallet password, Wallet app, Network, Address | **the kind, and the phrase** | Wallet password: strong | never | — |
| SSH key | **Private key** (several lines, masked), Passphrase, Public key, Fingerprint, Key type (Ed25519 · ECDSA · RSA · Other), Allowed hosts | — | Passphrase: strong | never | — |
| API key | **Key or token**, Secret, Type (Bearer token · Basic · HMAC · Other), Host name, Scopes, Expires, Rotation note | — | — | never | key |
| Server | **Address**, Port, Username, Password, Admin console | — | Password: strong | Username, Password: fill | password |
| Database | Database type (PostgreSQL · MySQL · MariaDB · SQL Server · Oracle · SQLite · MongoDB · Redis · Other), Server, Port, Database name, Username, Password, Connection options | — | Password: strong | never | password |
| Software licence | **Licence key** (several lines, masked), Licensed to, Registered email, Version, Publisher, Download page, Purchased, Expires | — | — | never | licence key |
| Membership | Organization, Member name, Member number, PIN, Member since, Expires, Website | — | — | never | — |
| Custom item | (nothing but Notes; the person adds fields) | — | — | never | — |
| Passkey (view only) | Website, User name, Display name; credential, user handle and private key (never shown, never typed) | — | — | key: confirm | — |
| Consent record (hidden) | — | — | — | never | — |

Controls: text, several lines, secret, secret over several lines, web address, email, phone, number, date, month, a choice, yes or no, one-time code, passkey material. Number, several lines and a choice are stored as `text` and told apart by the template (no schema change; D110). **Fields of your own** ("Add a field of your own", on any item) take one of seven kinds: Text, Secret, Web address, Email, Phone, Date, Yes or no; they are numbered `custom.<n>` and a secret one offers the strong generator. A template designer (a person's own kinds with their own field lists) is a follow-up. An "Expires" date puts the item in the Expiring view, as before.

### 10.2 Making and changing an item

- **The editor** (`features/vault/item-editor.tsx`) is unicron's `DialogContent size="lg"`, one form for every kind: Title, then the template's sections in order, then any other field the item holds (an import's second website, passkey material: keep or remove only), then the fields added by hand, then "Add a field of your own". On a new item a switch "Keep as a crown jewel" (opt-in; cannot be undone later, because moving an item between key branches is a re-wrap the vault doesn't do yet); a Crypto wallet says instead that it is always one. Title may be left empty for a Login (the website's host), a Server (its address), a Code (its issuer) and a Wi-Fi network (its name).
- **Secrets** are unicron's `SecretField` in edit mode. On an item that holds one it starts empty and says "Left empty, it stays as it is", and the edit sends `keep` for it, so its bytes never cross back to the core and were never sent to the webview. **Generate** replaces the control with the rule in words and Undo; the core makes the value on save, so this window never holds it until a reveal or a copy (D112). **Remove** is explicit. A secret over several lines (a private key, a recovery phrase, a licence key) is a masked textarea (`data-masked`, `-webkit-text-security`), because a password input drops line breaks; a Show control lifts the mask. Typed secrets go as byte arrays once and the arrays are zeroed when the call returns; the rows are wiped on save and on close (D91 still holds for keystrokes).
- **The edit names every field** the item has (`keep`, `set`, `generate`, `remove`); the core refuses an edit that leaves one out, a secret sent as text, a value that doesn't fit its control (a date, a month, a choice, an email, a phone, digits, a web address), a required field removed or emptied, and Keep or Remove on a field the item doesn't have. A refused edit writes nothing; the core's words show under the form.
- **A one-time code** is pasted as its `otpauth://` / `steam://` link or as the bare set-up key; the core turns a bare key into a standard six-digit, thirty-second link named after the item, after `lockdown-codes` has checked it.
- **A crown jewel's edit** goes through the `Use` gate, like its copy (D110).
- **"New item"** lists the sixteen creatable kinds with their plain names and an icon each (unicron `Icon`, Material Symbols: key, timer, credit_card, sticky_note_2, wifi, badge, description, account_balance, account_balance_wallet, terminal, api, dns, database, license, card_membership, category). "Add a code" in the Codes view stays, for putting a code inside a Login, now in the same sized dialog.

### 10.3 What each kind looks like

Every detail is drawn from the template: fields under their section headings in the template's order, the template's labels (an older item's lower-case labels read the same), then "Other fields", then "Your fields", then Notes. Particular layouts: **Login** keeps its methods block (username, password, passkeys, codes), then Websites, then the rest; **Code** the code display full width with "Show large type", then its masked set-up; **Note** the body as text (Markdown is shown as written, not rendered; no rich editor); **Wi-Fi** Network with "Show large type" on the password, Hidden network as Yes / No; **Card** the number in groups of four, security code and PIN masked, and the line on the second press before any fill; **Passkey** its facts, the key said to be kept by the provider, no Edit. A kind with no template falls back to the plain field list. Empty states: "Nothing filled in yet", "An empty note"; a failed open names the item id and the core's words; the editor says "Loading the item kinds…" until the templates arrive and shows their error if they don't. Every secret copies by id and field through `copy_secret` (codes through `copy_code`); nothing in this slice copies a secret in the webview.

### 10.4 The quick-access palette

⌘K on macOS, Ctrl+K elsewhere, anywhere in the unlocked app (and the bolt button in the vault's header), on unicron's `CommandDialog` + `Command`. It searches the item summaries the list already holds, with the list's rule (every word a plain substring): title, username, the website's host, the template's subtitle and the kind's name. It never sees a field value; a crown jewel's summary has no field, so it is found by title and kind (D113). Trashed items are left out. **Enter** opens the item in All items. **⌘↵ / Ctrl+Enter**, or the copy control on a result, copies the item's one quick copy: the password (a Login, a Server, a Database), the code (a Code, or a Login with only a code), the Wi-Fi password, the card number, the API key, the account number, the licence key, through `copy_secret` / `copy_code` with the settings' clipboard timer. The palette closes first, so a crown jewel's ask (the `Use` gate: the platform check, the password, or the window confirm) opens over the app; backing out copies nothing and says so. While locked the palette isn't mounted. The vault list's rows carry the same copy button, beside unicron's `ItemMain`. A system-wide hotkey (the palette without the window) is a follow-up: it needs the global-shortcut plugin, a named capability beyond `core:default` (D88), and the palette in a window of its own (D111).

### 10.5 Export and import of the new kinds

Building this showed that a CXF export carried only what CXF's credential types hold, so a Wi-Fi password, an SSH key, a wallet's phrase, Notes on most kinds and every field added by hand were dropped silently. Each item now carries a Lockdown extension (`design.snds.lockdown.fields`) with its kind and every field no credential type holds; an import from Lockdown reads it back (the kind, the labels, the values), and a broken one is reported by title and left out, never guessed at. Import also classes a crown jewel by the template (a wallet comes back as one).

### 10.6 Gaps found

**In the plan, not built here:** the Wi-Fi guest QR and printable card; attachments in the editor (the vault has them, no UI); Identity subtypes (person, address, business, vehicle, passport, licence) with their own field sets; per-URL match rules in the editor (new websites get `exact_host`); generator options (length, classes) and a strength word (`SecretField`'s `strength` wants a core estimator, none exists yet; it landed in §12); moving an existing item into or out of the crown-jewel class; Markdown rendering for notes. **Still open from 02-model §3:** crown jewels are in the bulk export today (behind the `Reveal` gate), where the plan keeps them out unless opted in per item; that is a user-visible change and is left for Sean.

**Host-contract gaps in unicron, found binding this slice** (component, what it lacks, what the shell did):

1. `SecretField` edit mode is one line only: no several-line secret (private keys, recovery phrases). The shell uses `Textarea` with `data-masked` and its own Show control.
2. `SecretField` edit mode takes no `required` / `name`, so the browser's form check doesn't see it. The shell marks the label "(needed)" and the core refuses a missing required field.
3. `SecretField` edit mode's `strength` is "computed by the host's core"; with no estimator in the core the meter is left off.
4. `CommandItem` has no slot for a secondary action. The shell puts an `IconButton` (out of the tab order, its click stopped from selecting the row) inside a `CommandShortcut`, and handles ⌘↵ / Ctrl+Enter on the `Command`.
5. `CommandItem`'s `value` doubles as its search text, so an item keyed by id matches on the id's letters; the shell passes `Command`'s `filter` with its own word list.
6. `Skeleton` has no size for a list row; the shell uses the `data-row` density knob instead of a `className`.
7. `DialogContent`'s `size` names skip a step against Tailwind's (`xl` is `max-w-2xl`), so the kit page, which was `max-w-xl`, is a little wider at `size="xl"`.

## 11. Follow-up: unicron 0.7.1, text roles, the by-reference host

**Versions.** `@unicron/ui` 0.7.1 and `@unicron/tokens` 0.5.0 (release tarballs, pinned in `apps/desktop/package.json` and the tokens override in `pnpm-workspace.yaml`). What moved for this app: control borders (`--input`) now 3:1; dark `--muted-foreground` lighter; `--destructive` on red 600 / 300 so it reads as text; light `--warning` on amber 400, a fill; dark `--info` on blue 500 with white; new text steps (`amber`, `green`, `blue`, `red` 600 light, 300 dark) held to APCA Lc 75. `IconButton disabled` now disables the control itself; no `IconButton` in `apps/desktop/src` passes `disabled`, so nothing here changed behaviour. Inside the components, SecretField's reveal and copy buttons are now really inert while a gate runs, while loading, in the error state (Retry is the way out) and while locked.

**Text roles split from fills (the regression).** `--secure`, `--attention` and `--area-codes` were drawn both as fills and as words. As words on the new tokens they read, light / dark, on the page: `--attention` Lc 32 / 70, `--secure` 60 / 56, `--area-codes` 67 / 50, under the Lc 75 a reading word needs. Each hue now has three roles (§4): the base role is a fill, `-foreground` its label, `-text` the hue as words on a surface (600 light, 300 dark). Every text use points at `-text` (`shell.tsx`'s Needs attention and Codes entries, `status-view.tsx`'s failed-check box, the sync refusal and the transparency fact). Green and blue fills stay on 500 in dark mode with a white label, as the core's dark `info` does: on 400, neither a white nor a near-black label reached Lc 60 (51-58). No fill is drawn yet; the roles are there so a badge or a dot never borrows the text step. Measured with the reference `apca-w3` from the built `tokens.css` and `app-extension.css` (var chains resolved, OKLCH → sRGB), APCA |Lc|:

| Role (reading text, floor Lc 75) | Light: page | card | chrome | selected row | Dark: page | card | chrome | selected row |
|---|---|---|---|---|---|---|---|---|
| `--attention-text` (amber 600 / 300) | 87.1 | 87.1 | 81.9 | 76.8 | 79.8 | 79.3 | 79.3 | 77.2 |
| `--secure-text` (green 600 / 300) | 86.3 | 86.3 | 81.1 | 76.0 | 80.3 | 79.8 | 79.8 | 77.7 |
| `--area-codes-text` (blue 600 / 300) | 86.6 | 86.6 | 81.4 | 76.3 | 80.5 | 80.1 | 80.1 | 77.9 |
| `--danger` (the core's `destructive`, reported) | 84.4 | 84.4 | 79.2 | 74.1 | 77.1 | 76.6 | 76.6 | 74.5 |

| Fill (label floor Lc 60) | Label | Light | Dark | Fill against page / chrome, light | dark |
|---|---|---|---|---|---|
| `--attention` (amber 400) | `--attention-foreground` (neutral 950) | 71.3 | 71.3 | 32.3 / 27.1 | 70.2 / 69.7 |
| `--secure` (green 500) | `--secure-foreground` (white) | 65.8 | 65.8 | 60.3 / 55.1 | 41.2 / 40.7 |
| `--area-codes` (blue 500) | `--area-codes-foreground` (white) | 72.2 | 72.2 | 66.7 / 61.5 | 34.8 / 34.4 |

`--card` is `neutral-0` light and `neutral-900` dark, the chrome (`--sidebar`) `neutral-50` / `neutral-900`, the selected row `neutral-100` / `neutral-800`. Two numbers belong to the core, not this layer, and are reported to unicron rather than overridden here: `--destructive` on a selected row (74.1 / 74.5, just under 75; this app draws no destructive words on a selected row) and the light amber fill against the chrome (Lc 27, under the Lc 30 a solid wants; this app draws no amber fill on the chrome).

**The 0.7 security API.** The host-contract gaps of §5 that unicron 0.7.0 closed (its #26), and what the desktop stopped doing:

- (1) Copy by reference: SecretField takes `host.copyRef()`. The desktop binds it as a `SecretFieldReferenceHost` with no `copy`: `copyRef` runs `copy_secret` and resolves with `{ hash: String(copy) }`, the shell's generation for that copy (`Copied { clears_in_secs, copy }` from `copy_secret` and `copy_code`). `clipboard_holds(copy)` answers yes only while the shell's last write is still on the clipboard and that write is the one named, compared with the shell's own record and never with the clipboard (`clipboard.rs`, its unit test rewritten for the number). **Stopped:** fetching the bytes on every copy of a standard field and wiping them unused; the crown-jewel `pendingProof() === "use"` empty-buffer workaround; the component's SHA-256 of the bytes, and with it `sha2` in the desktop crate (D115).
- (3) The call-time verdict: unicron documents and tests the pattern this host already runs (a host `requestProof` that collects, the core judging on the call, the host re-asking). `app/gate.tsx` now says the gate's own sentence, `crownJewelGateErrors.wrongPassword(left)`, instead of its copy of it.
- (4) No biometric: `biometricName={null}` on a computer whose `use` route is not the platform's check, so the announcement reads "Using it or showing it needs your account password." and the explain list says the password is typed for use, reveal and export. **Stopped:** passing "your account password" as a biometric's name.
- (5) CodeDisplay: a by-id host may ignore `onCopy`'s value (this one always did), a rejected copy starts no countdown and announces nothing (the host's toast says the core's words), copy waits while an HOTP advance is pending. Unchanged here: copy by id through `copy_code`. HOTP's "Next code" stays disabled, as the spec says when `onAdvance` is absent; the core has no advance command yet (§12 adds it).
- (6) Error words: SecretField takes `errorText`; the desktop passes the core's sentence for a failed reveal or copy, shown in the error state with Retry. **Stopped:** routing it through `description` beside the component's generic words.
- Also from #26: a gated reveal is always a tap with the lifetime. **Stopped:** forcing `revealMode="toggle"` and `revealLifetime={20}` inside a `CrownJewelSurface`; the shell passes the person's gesture setting and the component overrides it inside the surface.

**Tests.** Web UI 46 → 52: `item-field.test.tsx` (a copy calls neither `reveal` nor `getSecret` and goes by reference; at expiry `clipboardHolds` gets the shell's copy number and the clear runs only on yes; the core's sentence for a refused reveal and a refused copy), `crown-jewel.test.tsx` (a copy on a password-only computer carries the proof and fetches nothing; the explain copy with a named check and with `biometricName={null}`), `codes-view.test.tsx` (a refused copy starts no countdown and toasts the core's words). Rust stays at 216; `clipboard_holds_answers_for_the_shells_own_copy_only` now names copies by number.

## 12. Follow-up: unicron 0.8.0, the host-contract gaps closed, strength, HOTP

**Versions.** `@unicron/ui` 0.8.0 and `@unicron/tokens` 0.6.0 (release tarballs, pinned as before in `apps/desktop/package.json` and the tokens override in `pnpm-workspace.yaml`). 0.8.0 is unicron's slice 4, which closed the seven gaps of §10.6; nothing in it changes an existing default, so only what this shell adopted moved. Rows: D116–D119.

**What the shell stopped doing** (the §10.6 gap, what replaced it):

| Gap | Was | Now |
|---|---|---|
| 1. several-line secrets | `Textarea` with `data-masked`, the shell's own Show control, and a `textarea[data-masked]` rule in `app-extension.css` | `SecretField mode="edit" multiline`: unicron's masked textarea, its Show control, copy, cut and drag blocked while masked. The CSS rule is gone (unicron's stylesheet owns it). The crown-jewel note rides in the field's `description` |
| 2. `required` / `name` | "(needed)" in the label; only the core refused a missing value | `required` (native, plus `aria-required`) and `name` on the field. The editor is a native form, so the browser's check stops an empty save; the label hack is gone and the core still refuses |
| 3. no estimator | meter left off | zxcvbn in the core, `secretStrengthFromScore` in the editor (below) |
| 4. a result's second action | `IconButton` inside a `CommandShortcut` (out of the tab order, its click stopped) and a hand-rolled ⌘↵ / Ctrl+Enter `onKeyDown` on the `Command` with a controlled highlight | `CommandItemAction`: a pointer clicks it without opening the item, ⌘↵ / Ctrl+Enter runs it for the highlighted result, the result is described "Copy password, Control Enter" and carries `aria-keyshortcuts`; the dialog's hint takes its key from `commandItemActionShortcut()` |
| 5. id-keyed search text | `Command filter` over a word map keyed by id | every `CommandItem` has an explicit `value` (the id, so no icon glyph name leaks into search) and its `keywords`; the filter reads the keywords only (the matching rule below) |
| 6. a list-row skeleton | `Skeleton data-row` (the density knobs on a div) | `Skeleton variant="row"`, with `--c-skeleton-row-height: var(--c-row-height)` mapped once in `app-extension.css`, so compact density shortens it too (vault list, locked wash) |
| 7. `DialogContent size` names | noted | documented and pinned upstream; nothing to change here (the kit page stays `size="xl"`) |

**The palette's matching (D118).** unicron offers `Command match="keywords"`: cmdk's own fuzzy ranking over each item's keywords. The palette keeps the vault list's rule instead (every word of the search a plain substring of the keywords, in any order), passed as `Command`'s `filter`, which cmdk hands the item's keywords. Measured with cmdk 1.1.1's `commandScore` on two logins (GitHub: `sean@example.com`, `github.com`; Mail: `sean`, `mail.google.com`): "git" and "github.com" find GitHub either way; "example sean git" scores 0 under fuzzy (word order matters to it) and matches under the list rule; "gml" ranks GitHub (0.148) above Mail (0.120) under fuzzy, and the list rule finds nothing. Since ⌘↵ copies the highlighted, first-ranked result, a scattered-letter near miss ranked first copies the wrong item's password; the list rule also keeps one meaning of "search" across the app. Cost: no typo tolerance. `quick-palette.test.tsx` pins it through the component: a title word ("github"), a host ("mail.google"), words in any order ("example sean git") each find exactly the one item; the id's letters ("4f3a" of `4f3a9c1e-…`) and scattered letters ("gml") find nothing, and Ctrl+Enter then copies nothing; every option's `data-value` is the id and its description names the copy and its key. With `match="keywords"` in its place the any-order case fails, as measured.

**Strength (D116).** The `zxcvbn` crate 3.1.1 (MIT; released 2026-04-24, the Rust port's maintained line since 2016, about 1.8 million downloads in the last 90 days), with `default-features = false` (its builder macro is not needed). It adds five crates to the lock file: `zxcvbn`, `fancy-regex` 0.18, `itertools` 0.14, `either`, `lazy_static`; `regex`, `time` and `chrono` were already in the tree. `cargo deny check` passes (advisories, bans, licences, sources) with no new duplicate. Size: a 436 kB crate download, most of it the word-frequency lists (795 kB of Rust source); in the debug desktop binary its named code and fancy-regex's come to about 580 kB, plus the lists' string data. `lockdown_app::strength::score(candidate, context)` answers the 0–4 score (`None` when empty) on the first 100 characters, with up to eight context words; `App::password_strength` and the `password_strength` command carry it, reading nothing from the vault. In the editor (`use-strength.ts`, `field-control.tsx`) the one-line secrets the generator is offered for get the meter while typed: the candidate goes once per 150 ms pause as a byte array zeroed when the call returns, with the form's title and plain text, email and web-address values as context; the reply for an older candidate is dropped and the last word stays until the next lands. A card number, a PIN, an API key, a secret left as it is (`keep`), a generated one and every stored secret are never scored. Tests: `strength.rs` (common and patterned passwords score 0–1, the generator's strong rule and a four-word phrase score 4, a surname and year scores 4 alone and at most 1 beside the username it repeats, empty is `None`, non-UTF-8 is refused, a 10,000-character paste is scored on its prefix), `item-editor.test.tsx` (one call per pause, the bytes and context sent, the array zeroed after, Weak then Strong, the meter as the field's description, gone when cleared; no call and no meter for a card number).

**HOTP's Next code (D117).** `Session::advance_code(item, field)` parses the code's set-up, raises the counter, writes the set-up back (`Code::to_otpauth`, issuer, account, digits and algorithm kept), commits a new item version and saves, then answers `CodeAdvanced { code, counter, version }`; `App::advance_code` runs it on the worker after the `Use` gate for a crown jewel; the `advance_code` command (`src-tauri/src/codes.rs`, outside the `shell` feature so it is tested without Tauri) carries it. `CodeView.counter` is the counter an HOTP code's `current` was made from. `ItemCodeDisplay` passes `onAdvance` and `counter` for an HOTP code only; it shows the code the core answered with until the next `codes` read carries it, and a refused step toasts the core's words and leaves the code as it was. Copy still goes by id (`copy_code`), so it copies the code for the stored counter, which is the one shown; the component holds copy while the step is pending. Tests: `session::codes` (RFC 4226 counters 3 → 4 → 5; a new version each step; the counter survives dropping the session and opening it again; the rewritten set-up keeps its parts; TOTP and Steam refuse and write nothing; a trashed item refuses), `lib.rs` (`an_hotp_code_moves_on_when_asked_and_keeps_its_counter_through_a_lock`: through `App::lock` / `unlock`, a locked call refused as locked, `code_now` copies the new code, time-based codes have no counter and refuse, a crown jewel's step is `approval_needed` until confirmed and nothing moves before), `src-tauri/src/codes.rs` (the JSON the webview gets is exactly `{ code, counter, version }` with no seed or link; refusals in the core's words), `codes-view.test.tsx` (Next code calls `advanceCode` by id and field, the new code and "#4" are drawn, the copy after it goes by id; a refused step).

**The code's clipboard clear (D119).** CodeDisplay's `onClipboardClear` used to call `clipboard_clear` with nothing, which cleared whatever the shell last wrote if it was still there, so a code's countdown could clear a password copied after it. `clipboard_clear` now takes `copy`, the number `copy_code` (or `copy_secret`) returned, and clears only through `ClipboardGuard::may_clear(copy, current)`: that copy is the shell's last write and is still on the clipboard. `ItemCodeDisplay` keeps the number of its last copy; `ItemField` does the same for SecretField's `host.clear` after `clipboard_holds(copy)` answers yes. Tests: `clipboard.rs` (`a_clear_from_the_webview_runs_only_for_the_copy_it_names`: a code's clear after a later password copy does nothing, the password's clears, a number never given and the person's own text are left alone, nothing after a lock), `codes-view.test.tsx` (the countdown's end calls `clipboardClear(7)` for copy 7, once), `item-field.test.tsx` (the clear after `clipboard_holds(7)` names 7).

**New unicron gap, found binding this slice** (component, what it lacks, what the shell did): `CodeDisplay` has no copy reference and no `clipboardHolds`, unlike SecretField since 0.7.0. `onCopy` resolves with nothing the component hands back, so `onClipboardClear()` arrives without saying which copy it is about (the shell keeps the last number itself), and the component says "Cleared" at the end of every countdown even when the shell declined to clear because the person had copied something else. The shell clears only its own copy; the word stays the component's. Proposed upstream: `onCopy` may resolve with `{ hash }` as `copyRef` does, handed back to `clipboardHolds(hash)` and `onClipboardClear(hash)`, with "Cleared" said only on a yes.

**Closed upstream in `@unicron/ui` 0.9.0 (unicron #36), and the shell is on it.** `onCopy` resolves the shell's copy number as `{ hash }`, the component hands it back to `onClipboardClear` at the countdown's end, and `clipboard_clear` now answers whether it cleared: `true` only for the shell's own copy still on the clipboard. The component says "Cleared" only on that `true`, so a code the person copied over ends its countdown silently. SecretField's `host.clear` keeps its own path (`clipboardHolds` by copy number, D115).

**Tests.** Rust 216 → 225 (`cargo test --workspace`): two in `session::codes`, one in `lockdown-app`'s lib, three in `strength`, one in `clipboard.rs`, two in the desktop crate's `codes`. Web UI 52 → 60: the palette's matching and its copy control (2), the locked wash's row skeletons (1), HOTP Next code and a refused step (2), the code's clear by number (1), the editor's strength meter and its absence (2); the editor's wallet test now checks the masked textarea (`data-masked`, `required`, `name`, the form's validity, line breaks kept, Show lifting the mask) and the SecretField expiry test the number its clear names. `src/test/setup.ts` stubs `scrollIntoView`, which cmdk calls when it moves the highlight itself and jsdom lacks. The dsa ratchet stays at 4 (§8).
