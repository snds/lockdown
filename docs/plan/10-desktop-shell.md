# Desktop shell: the first Tauri window

_Status: built, first shell, 2026-10-17. Companion to [`11-desktop-design-brief.md`](11-desktop-design-brief.md), which owns how it will look; this page owns how it is built. What is real: `crates/app` (`lockdown-app`) and `apps/desktop` (web UI + `src-tauri`), with the gates in CI's `desktop` job._

## For future agent

- **TL;DR:** The desktop app is three layers: the core crates, one command layer (`lockdown-app`) that owns the lock timer and the secret-handling rules, and a Tauri 2 shell that maps each command to the webview and supplies only what a shell must (keychain, clipboard, browser, window). The UI is unicron primitives with Lockdown's app-extension roles and no visual design of its own yet. Copy never shows the webview a secret; reveal shows bytes for thirty seconds; the core locks on idle, hidden window and sleep whatever the UI does.
- **Reads before this:** [`03-architecture.md`](03-architecture.md) §8–§10, [`07-workspace.md`](07-workspace.md) §1, decisions D11, D15, D47, D61, D68, D69, D70, D73, D74, and the brief.
- **Not here:** the six security components (unicron deliverables, §9 of the architecture), the sidebar and smart views of the brief's §3, passkeys, codes, import/export, the quick-access palette, mobile.

## 1. Layout

```
crates/app/                      lockdown-app: the command layer (Rust, no UI)
  src/lib.rs                     App handle + the worker thread; AppError, ErrorCode
  src/session.rs                 the open session: identity, store, keys, vault, account, sync, kit, devices
  src/lock.rs                    LockPolicy, LockClock: idle / hidden / sleep
  src/revealed.rs                Revealed: secret bytes, zeroed on drop, never Display
  src/secrets.rs                 SecretStore: where the shell keeps the session token
  src/types.rs                   what crosses to a shell (Status, ItemSummary, ItemDetail, …)
apps/desktop/
  package.json, pnpm-lock.yaml   pnpm 10 root for the web UI (its own, like extension/)
  pnpm-workspace.yaml            the @unicron/tokens override (release tarballs)
  src/main.tsx, App.tsx          locked by default; the lock watch; two areas
  src/index.css                  @unicron/ui/styles, then the extension layer
  src/styles/app-extension.css   Lockdown's tier-2 roles (§4)
  src/core/client.ts             the command surface as the UI sees it (one method per command)
  src/core/secret-bytes.ts       decode once, wipe; encode for the bridge
  src/core/types.ts              the Rust types, field for field
  src/app/lock-watch.ts          visibility and activity → the core; lock-state poll
  src/app/shell.tsx              rail: Vault · Status · Lock
  src/components/master-detail.tsx
  src/features/unlock/           first run and lock screen (+ the Vitest error-path test)
  src/features/vault/            list, detail pane, secret field, login editor
  src/features/status/           status, account, sync, kit, devices, lock timers
  src-tauri/
    Cargo.toml                   lockdown-desktop; the `shell` feature (§7)
    src/lib.rs                   run(): plugins, keychain, App::start, window events
    src/commands.rs              one #[tauri::command] per operation, all off the main thread
    src/clipboard.rs             the clipboard rule (§3), unit-tested without Tauri
    tauri.conf.json              window, CSP, build.features = ["shell"]
    capabilities/default.json    core:default only
dsa.config.json                  measurement (root); baseline at apps/desktop/.dsa/baseline.json
```

`lockdown-app` runs the session on one worker thread: the vault holds a `Box<dyn Signer>` and the keystore an `Rc`, neither `Send`, so the `App` handle (clonable, `Send + Sync`) sends one closure at a time to that thread and waits. One call at a time is also the mutex; two windows never interleave on the vault.

**Where the data is.** The same places as the CLI (D47): the identity root (D15) and the `lockdown` directory beside it. The desktop app and `lockdown` on one machine share `device-key.cbor`, so they are one device named `Lockdown on <hostname>` (D68, D74). `LOCKDOWN_ROOT` and `LOCKDOWN_DATA` move both for a second profile. `sync.cbor` keeps the CLI's shape; with the keychain installed the session token lives there and the file holds an empty string in its place, and the file also gains `last_sync_at`, which the CLI ignores.

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
| `status` | `status()` | yes | `Status` | identity, device, persona, vault head, devices, account |
| `account_register` | `account_register(…)` | yes | notes + kit page as **bytes** | OPAQUE; makes the kit when none exists (D69, D70) |
| `account_login` | `account_login(…)` | no* | notes | *on an empty device enrols it from the account; else a fresh session |
| `recover` | `recover(…)` | no | notes | the whole kit on an empty device |
| `sync` | `sync()` | yes | `SyncOutcome` | identity records first (D71), then pull / merge / push / head |
| `kit_create` | `kit_create(replace, password?)` | yes | notes + page as **bytes** | D69; with an account the slot takes the new Secret Key (D70) |
| `kit_verify` | `kit_verify(sk, code, password?, drill)` | yes | notes | changes nothing unless `drill` |
| `device_list` | `device_list()` | yes | `DeviceInfo[]` | |
| `device_revoke` | `device_revoke(id)` | yes | notes | D73; needs the account |
| `open_site` | — | no | — | http and https only, through the opener plugin |

A call on a locked vault answers `{ code: "locked" }`; the UI shows the unlock screen. Other codes: `not_set_up`, `refused` (the core's words), `stopped`.

## 3. Secret handling, as implemented

| Rule (03-architecture §8) | Where it holds |
|---|---|
| No secret through a JavaScript string on the bridge | `reveal`, `account_register`, `kit_create` answer with raw bytes (`tauri::ipc::Response`); nothing hidden is ever in the JSON of `show`, `list` or `status` (tested: `show_is_masked_until_asked_and_reveal_comes_back_as_bytes`). Passwords typed into the UI are sent as byte arrays once and the form is reset. |
| Buffers wiped | `Revealed` zeroes on drop and has no `Display`; `Debug` prints the length (tested). The webview decodes a buffer once and `fill(0)`s it (`decodeAndWipe`); the string exists only in the component that shows it, for at most 30 s, and is dropped on hide, on window blur, on unmount and on lock. |
| Copy with a clipboard timer | `copy_secret` reveals on the Rust side and writes the clipboard from there. After the timer (default 30 s, range 10–90) the shell reads the clipboard back and clears it **only if it still holds what we wrote** (a BLAKE3 hash is kept, never the value) and **only if no later copy happened** (a generation counter). Tested in `clipboard.rs` without Tauri. A plain field (username, URL) copies in the webview with no timer. |
| Lock enforced by the core | `LockPolicy` + `LockClock` live in `lockdown-app`; the worker thread checks them before every call and once a second. Idle (no call or `touch`), hidden window past its timeout, and a wall-clock jump larger than the sleep gap each drop the session. The UI reports `visibilitychange` and activity and polls `lock_state` every 3 s so a lock the core decided alone reaches the screen. Tests: `the_idle_timer_locks_and_unlock_opens_again`, `a_hidden_window_locks_after_its_timeout_and_a_request_locks_at_once`, the four `lock::tests`. |
| Nothing secret in React state beyond the revealed field's lifetime | `App` keeps `Status` (no secrets) and `LockState`; `SecretField` keeps the one revealed string; the kit dialog keeps its page while open. |
| What a webview cannot do | A typed password is a webview string between keystroke and submit; a revealed value is a string while drawn. The native shells will use native fields; the desktop says so here instead of pretending. |
| Capabilities and CSP | `core:default` only; no `shell`, `fs`, `http` or `clipboard` permission reaches the webview. CSP: `default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self' data:; connect-src ipc: http://ipc.localhost; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'; frame-src 'none'` (`'unsafe-inline'` for styles only, because Base UI sets inline `style` attributes). Sync traffic is Rust-side `reqwest` over rustls, never from the webview. |
| Session token | `lockdown_app::secrets::SecretStore`; the shell installs the OS keychain (`keyring` 3.6: macOS Keychain, Windows Credential Manager, Secret Service on Linux) under service `design.snds.lockdown`, key `session:<login>@<server>`. |

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
| `--attention` | `--g-amber-500` | `--g-amber-400` | needs attention (kit not made, expiring, conflict copy) |
| `--area-codes` | `--g-blue-500` | `--g-blue-400` | the Codes view, later |
| `--c-crown-jewel-surface / -ink / -border` | empty | empty | reserved for the unicron crown-jewel spec; nothing may fill them first |

The UI uses only `bg-surface-*`, `text-secure` and `text-attention` from this layer today; everything else is unicron's defaults.

## 5. What the design brief decides (left open here)

The brief ([`11-desktop-design-brief.md`](11-desktop-design-brief.md)) owns all of this; the scaffold keeps a neutral layout so none of it is prejudged:

- the sidebar (persona switcher, smart views, vaults, folders, tags) and the three-column layout; today: a two-item rail and a master–detail;
- the unlock and first-run screens' look, the three-screen onboarding, the server field's place;
- the six security components (SecretField, CodeDisplay, LargeTypeView, ApprovalSheet, RiskTierBadge, CrownJewel) and their colours; today's `SecretField` is a placeholder built from `Item` + `Button`;
- the lock-as-UX details: the washed list with skeleton rows, the 10-second toast before an idle lock, no transition animation;
- copy and reveal controls (countdown ring, "Cleared"), hold-to-reveal as a variant;
- the Status screen's proof lines with "Check now", the Needs attention view, Devices and Recovery as their own screens;
- density, list grouping, the risk-tier words (brief §9).

## 6. How to run

```sh
cd apps/desktop
pnpm install                               # pnpm 10; the lockfile pins the unicron tarballs by hash
pnpm tauri dev                             # Vite on :5173 + the Tauri window; features = ["shell"] from tauri.conf.json
pnpm typecheck && pnpm lint && pnpm test   # tsc, oxlint, Vitest (jsdom)
pnpm build                                 # vite build → dist/ (what the shell embeds)
pnpm ds:scan && pnpm ds:patterns && pnpm ds:ratchet
cargo test -p lockdown-app                 # the command layer
cargo test -p lockdown-desktop             # the clipboard rule, no Tauri needed
cargo clippy -p lockdown-desktop --all-targets --features shell -- -D warnings
pnpm tauri build                           # bundles; never run with a shared, tight target dir
```

Linux needs the WebKitGTK stack before anything with `--features shell`: `libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libayatana-appindicator3-dev libdbus-1-dev libxdo-dev pkg-config`. CI's `desktop` job installs them; the root `check` job never needs them (§7). `LOCKDOWN_ROOT=/tmp/snds/identity pnpm tauri dev` runs against a scratch identity.

## 7. The `shell` feature

`lockdown-desktop` is a workspace member, but Tauri and the plugins are optional dependencies behind its `shell` feature, off by default. Without it the crate is the clipboard rule and a stub `main`; with it, the window. Reason: `cargo clippy --workspace` and `cargo test --workspace` run on CI without a platform SDK (the comment in the root `Cargo.toml`), and the existing `check` job must not start needing WebKitGTK. `tauri.conf.json` turns the feature on (`build.features`), so `pnpm tauri dev` and `pnpm tauri build` need no flag. Mailbird keeps its shell out of `default-members` instead; Lockdown's root jobs use `--workspace`, which ignores that list, hence the feature.

## 8. Measurement

`dsa.config.json` at the repo root scans `apps/desktop/src/**/*.tsx` against `@unicron/ui`; the ratchet baseline is committed at `apps/desktop/.dsa/baseline.json` (the reports beside it are regenerated and ignored). First scan, 2026-10-17: 21 `className` overrides on system components, 0 style overrides, 0 spread props, 0 look-alikes; one pattern candidate (`Field` + label + input, three times across two files). The scripts run from `apps/desktop` with `--config ../../dsa.config.json`; the pin on `@snds/dsa` is the same commit unicron uses, and pnpm 10.28 records it in the lockfile as the GitHub tarball of that commit.

## 9. Decisions proposed

Rows for `decisions.md`, numbered when accepted. Each is user-visible or a security default, so **flagged for Sean**.

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
