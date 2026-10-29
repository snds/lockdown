# 40: Developer tools: the reset and the demo

_Slice: `slice/devtools`, built 2026-10-29. Sean asked for two things while he tests and demos the macOS app: "a way to do a full reset run of the app and the clearing of application data on the target device … self-contained for the app and can be removed without affecting the app later. I don't want this to be a security issue down the line", and "a default demo vault with dummy content for all content types". Later, on the demo's shape: "once we have a stable demo experience, we can look at making my own or importing from a 3rd party. I just want a demo experience to help drive improvements, new features, and QA." This slice builds on D11 (the shared identity), D38 (who decides security-model details), D62 (wire shapes only grow), D121 and D138 (crown jewels out of exports and backups), D151 (what counts as a use), D375 (the flag review) and D450 (the ad-hoc macOS seal), and on doc 39's plan for the new first run (§1.13 and its Q5). Proposed decisions D480–D489 (§9)._

## For future agent

- **TL;DR:** Two developer tools, each removable in one go, neither ever in a release. **Developer → Reset Lockdown…** erases everything Lockdown keeps on this device and relaunches it into the first run: the data directory, the shell's own folders, every OS keystore entry under `design.snds.lockdown`, the webview's storage, and the shared identity only when Lockdown alone uses it. **The demo** runs the app as a full, real session over a throwaway vault in `<data>/demo/`, confined: it never reaches the network, refuses exports and backups, and the browser extension sees Lockdown as locked. The demo is filled from a **data pack**: a CXF document (the format Lockdown already imports) plus a small set of extras, loaded through the core's real import. The default pack holds 158 items: every kind and variant, plus the edge cases QA needs. Both tools live in `crates/devtools` and `apps/desktop/src/devtools/`, behind the desktop's Cargo features `dev-reset` and `demo-vault` and the front end's `__LOCKDOWN_DEVTOOLS__` define. They are on in development builds, CI and release dry runs, and off in a `v*` tag build, which CI checks by searching the shipped files.
- **Read with:** `crates/devtools/src/{reset,keystore,menu,shell}.rs`, `crates/devtools/src/demo/{mod,pack,load,fresh}.rs`, `crates/devtools/demo/default-pack.json`, `crates/devtools/examples/make-default-pack.rs`; `crates/app/src/confine.rs`; `apps/desktop/src/devtools/`; `apps/desktop/src/features/unlock/demo-entry.ts`; `packaging/devtools/check.sh`; the `desktop-bare` job in `.github/workflows/ci.yml` and the devtools steps in `release.yml`.
- **Do not:**
  - Reach the devtools' commands from anything but the main window. They are a plugin's (`plugin:devtools|…`), granted by a capability the plugin adds at start for `main` only. No capability file names them.
  - Let a pack carry key material. Passkeys and SSH keys are made when the demo opens.
  - Import `@/devtools/*` anywhere but the two lazy mounts in `App.tsx`. The first run reaches the demo through `features/unlock/demo-entry.ts`, never through an import.
  - Build a tag without `LOCKDOWN_DEVTOOLS=off` and `src-tauri/tauri.release.conf.json`. `build.rs` and `packaging/devtools/check.sh` both fail it.
  - Trust a reset plan from the page. The shell works the plan out again when it runs.

## 1. The shape

| Piece | Where | What it does |
|---|---|---|
| Features | `apps/desktop/src-tauri/Cargo.toml` `dev-reset`, `demo-vault`; `tauri.conf.json` `build.features` | On for `pnpm tauri dev` and `tauri build` (dry runs). A tag build passes `--config src-tauri/tauri.release.conf.json` (`features: ["shell"]`). |
| Define | `apps/desktop/vite.config.ts` `__LOCKDOWN_DEVTOOLS__` | True unless `LOCKDOWN_DEVTOOLS=off`. Off, the two `import("@/devtools/…")` calls are dropped and the bundle carries none of the folder. |
| Guard | `apps/desktop/src-tauri/build.rs` | `LOCKDOWN_DEVTOOLS=off` with either feature on fails the build. |
| Crate | `crates/devtools` (`lockdown-devtools`) | `reset`, `demo` (default features, so the workspace builds and tests them), `os-keystore`, and `shell` (the Tauri plugin). The desktop takes it with `default-features = false`. |
| Plugin | `crates/devtools/src/shell.rs` | The Developer menu, the commands, the reset's relaunch half, and the demo. The desktop registers it with one `#[cfg(any(feature = "dev-reset", feature = "demo-vault"))]` line. |
| Core seam | `crates/app/src/confine.rs`; `lockdown_sync::hold_network`; `lockdown_security::http::hold_network` | `App::rehome(paths, confined)` moves the app to other paths and back without a restart. While confined, the network is held and exports and backups are refused. The browser endpoint reads a confined app as locked (`browser/handler.rs`). |
| Front end | `apps/desktop/src/devtools/` | `reset-mount.tsx` with the reset dialog, and `demo-mount.tsx` with the demo's bar and the menu's demo items. `App.tsx` mounts each lazily. |
| First-run seam | `apps/desktop/src/features/unlock/demo-entry.ts` | A provider the demo installs. While one is installed, the first run shows **Explore the demo**. The first-run slice moves it onto Welcome. |
| Release check | `packaging/devtools/check.sh` | Searches files and folders for the tools' marker, command names and pack name. It requires `absent` on a tag and `present` in a dry run, so the check is shown to work. |

## 2. The developer reset (`dev-reset`)

### 2.1 Where it is

**Developer → Reset Lockdown…** sits at the end of the Developer menu, after a separator. That menu is in the macOS menu bar, and in the main window's own menu bar on Windows and Linux. It works locked or unlocked, at the first run too, and while the demo is open. It is reachable only from that menu: the menu sends the main window an event (`devtools://menu`), the page shows the dialog, and the dialog calls `plugin:devtools|reset_plan` and `reset_run`. Those calls are granted to the `main` window only, by a runtime capability the plugin adds in its setup. The quick-access window, a web page, the extension, the native host and the CLI have no way in. The CLI has no devtools code at all.

### 2.2 The dialog

- **Title:** Reset Lockdown on this computer?
- **Description:** "This erases everything Lockdown keeps on this computer and starts it again as if it had never run. It can't be undone. An account on a server is not deleted: sign in again afterwards to get its vaults back."
- **Erased:** each folder found, with its path; the keychain entries found, by name; "Everything this app's web view has stored."
- **Your identity:** the verdict of §2.4 and the root's path.
- **Kept:** see §2.3, plus any folder the guard refused, with why.
- **Confirm:** a field labelled "Type reset to confirm". **Erase and restart** stays disabled until the word is typed, and the shell checks the word again. Afterwards the dialog says "Erased. Lockdown is restarting…".

The dialog opens empty each time, and its plan is worked out when it opens.

### 2.3 What it erases and what it leaves

| Erased | Found by |
|---|---|
| **Lockdown's data directory** (`Paths::data`): the vault store, the device key, settings, `remembered.cbor`, `transparency.cbor`, `sync.cbor`, the security cache, browser pairings, the endpoint file (`native-host.json`) and its socket, the demo's folder, and anything later slices put there. That includes doc 39's in-process server (`<data>/local-server/`) and `onboarding.cbor`, without the reset knowing about them. | The app's own `Paths` (`LOCKDOWN_ROOT` and `LOCKDOWN_DATA` respected). |
| **The shell's own folders:** Tauri's app config, data, local data, cache and log directories for `design.snds.lockdown`, and on macOS `~/Library/WebKit/design.snds.lockdown`. They hold the webview's storage, caches and debug logs. | `app.path()`, deduplicated. |
| **Every OS keystore entry under the service `design.snds.lockdown`:** today the session tokens (`session:<login>@<server>`), and doc 39's quick-unlock items (`quick-unlock:<device id>`) when they land. | Each platform's own search (`keystore.rs`): the macOS keychain by service, Windows Credential Manager by `*.design.snds.lockdown`, and Linux's Secret Service by the `service` attribute. Each entry is removed through `keyring`. No secret is read. |
| **The webview's stored data** (local storage, IndexedDB, caches, cookies). | Tauri's `clear_all_browsing_data` on every open webview. |
| **The shared identity root**, only under §2.4. | `identity_plan`. |

| Left, and the dialog says so |
|---|
| The account on a server: nothing is sent to any server. |
| Exports and backups in folders the person chose. |
| The native-messaging host registrations in each browser's folder. Lockdown writes them again at launch. Each browser has to pair again, because the pairings were in the data directory. |
| Anything of Mailbird, Synapse or another app that shares the identity, including their keystore entries (other service names) and the identity root itself when they use it. |

Two more things are worth knowing. The CLI shares the data directory (D86), so a reset resets the CLI on this machine too. And a path the **guard** dislikes is never touched: a relative path, one with `.` or `..` in it, one fewer than three folders deep, or the home folder or one that holds it. The guard runs when the plan is made and again at removal. That matters when `LOCKDOWN_DATA` points somewhere odd.

### 2.4 The shared identity (D11)

The identity root (`…/snds/identity`, the macOS app group on a Mac) is shared by design with the other ecosystem apps. Its manifest is signed by the identity's own key, so removing part of it (say, Lockdown's envelopes) would need that key, and a reset runs locked. The rule is therefore all or nothing:

- The reset reads the store (`Store::load`, which verifies every record) and collects every app the records name: the profile's `manager`, each persona's `created_by_app`, and each key envelope's `app`. The IRK's own envelopes name no app and are ignored.
- **When Lockdown is the only app named, the whole root is erased**, and the next launch is a true first run.
- **When another app is named, the root is kept, Lockdown's parts included.** The dialog names the apps and says the next launch will say this device isn't enrolled, and that joining from that app or recovering with the kit gets back in. Lockdown's device record and sealed envelopes stay in the identity, harmless without the device key the reset erased.
- **A store that doesn't verify** is read record by record without signatures. If no record names another app, it is erased. If one does, it is kept. If nothing in it decodes at all, it is kept and the dialog says to remove it by hand.

When doc 39's `onboarding::created_identity()` lands, the rule can also require that first run created the root. That is the doc's own proposal, and it would only make this rule stricter.

### 2.5 How it runs

1. In the running app, after the confirm: lock the vault, clear every webview's storage, and remove the keystore entries. Then write `devtools-reset-pending` into the data directory and ask Tauri to restart.
2. In the new process, in the plugin's setup: Tauri runs it before any window or webview exists and before the app's own setup opens the store, the log or the endpoint. The plugin sees the pending file, works the plan out again, erases the keystore entries (again, idempotently) and every folder, and removes the pending file. Failures are printed to stderr.
3. The app's setup then starts on nothing: the first run.

Erasing in the new process rather than the old one means no file is open when it goes. That matters on Windows, where an open SQLite file or log can't be deleted. It also means no thread of the old app can write a file back.

## 3. The demo (`demo-vault`)

### 3.1 Demo mode

The demo is a mode, not a vault beside the person's own (doc 39 §1.13, Q5, accepted).

- **Entry points:** **Explore the demo** on the first run, with no account and no master password. Today it is a link under the three doors. The first-run slice moves it to Welcome through the same provider. **Developer → Open the demo** opens it from anywhere, locked or unlocked.
- **How it runs:** `Demo::open` locks the vault that is open (its keys drop as on any lock) and calls `App::rehome` to `<data>/demo/identity` and `<data>/demo/lockdown`, confined. It then creates a fresh identity and vault there (`open_or_create(true)`, a device key in a file there), turns off the breach check and the change-password check in the demo's own settings, and loads the pack. Every screen then runs the real core: read-write, gates and all.
- **The bar:** while the demo is open, a bar sits above every screen: "**You're exploring the demo.** Nothing here is real." with **Leave the demo**. The app's root moves down by the bar's measured height, and the bar doesn't print.
- **Leaving** moves the app back to its own home, locked, wipes `<data>/demo/`, and goes back to where the person came from: the first run, or their locked vault.
- **Developer → Reset demo data** builds the demo again from the pack it was opened with: new keys, edits gone.
- **Developer → Load a demo pack…** picks a file in the OS's own dialog (JSON, at most 16 MiB). A pack that doesn't read is refused with the reader's sentence (§3.3), and the demo stays as it was.
- **A demo left open when the app quit** is wiped at the next launch, when the plugin first makes the demo.

### 3.2 Isolation

| Promise | How |
|---|---|
| Never touches the real vault, identity or keystore | Its own root and data directory. The device key is in a file there. It has no account, so no session token is written to the keystore. The test snapshots every byte of the real data and identity before and after, and checks a keystore double. |
| Never syncs, shares or goes online | `rehome` holds every request through `lockdown_sync::HttpTransport` and `lockdown_security::HttpFetch` for the whole process, including requests through transports made before the hold. That covers sync, sign-in, sharing, breach ranges, change-password pages, alias services, log mirrors, the clock check and the live channel. Each fails with "the demo never syncs, shares, exports, backs up or goes online". |
| Never exports or backs up | `export_preview`, `export_to`, `export_items`, `export_to_recipients`, `backup_setup` and `backup_now` refuse before any gate, and the scheduled backup tick skips. |
| No fills into real pages | The browser endpoint answers a paired extension as locked, and refuses everything past pairing ("Lockdown is showing its demo; leave the demo to fill from your vault"). |
| Watchtower's local findings still show | Weak, reused and lookalike findings, and the 2FA directory, are local, and run. |

### 3.3 The pack format, version 1

One JSON file:

```json
{
  "format": "lockdown-demo-pack",
  "v": 1,
  "name": "Lockdown QA pack",
  "about": "…",
  "cxf": { "version": 1, "exporterRpId": "…", "accounts": [ { "items": [ … ], "collections": [ … ] } ] },
  "vaults": ["Work"],
  "items": {
    "<CXF item id>": {
      "vault": "Work",
      "crown_jewel": true,
      "state": "trashed",
      "history": [ [ { "id": "pw", "kind": "hidden", "label": "Password", "value": { "text": "…" } } ] ],
      "files": [ { "name": "a.txt", "mime": "text/plain", "text": "…" }, { "name": "big.bin", "mime": "application/octet-stream", "size": 32505856 } ],
      "fresh": ["passkey", "ssh-key"],
      "dismiss": [ { "finding": "weak:pw", "snooze_days": 30 } ]
    }
  },
  "templates": [ { "template": { "name": "Bike", "fields": [ … ] }, "items": [ { "title": "Road bike", "values": { "frame": "…" } } ] } ]
}
```

- **`cxf`** is a CXF v1.0 document as Lockdown's export writes it. It carries items, standard credentials (basic-auth, totp, credit-card, note, wifi, person-name, address, passport, drivers-license, identity-document, file), collections (folders: a title `Work/Servers` nests), tags and favourites, plus Lockdown's own extension `design.snds.lockdown.fields` for the exact kind and every field CXF has no word for.
- **The extras** say what CXF can't. `vault` is a name from `vaults`. `crown_jewel`. `state`: `live`, `archived` or `trashed`. `history`: earlier versions, oldest first, as fields in the backup format's shape (`docs/format/backup-v1.md` §2.2). `files`: inline text, or a size of placeholder bytes made at open, from 1 byte to 32 MiB, never on a crown jewel (attaching to one would ask for Touch ID mid-load). `fresh`: `passkey` gives every `passkey.<n>` the item names a new P-256 key, credential id and user handle; `ssh-key` gives a new Ed25519 key in OpenSSH form with its public line and fingerprint. `dismiss`: a Watchtower finding dismissed, or snoozed for some days.
- **`templates`:** templates of the person's own, as `save_user_template` takes them, and items made from them.
- **A bare CXF document is a pack too**, with no extras. Another manager's CXF export can be tried as it is.
- **Checks:** unknown members are refused (`deny_unknown_fields`, so a typo is caught); `v` above 1 is refused; the CXF document must read with the import's own reader; extras must name items that exist, and states, vaults, history fields, file sizes and template fields must be valid. Each fault is one sentence, such as "The pack's item 'demo-004': there is no CXF item with that id." or "The pack is version 9; this build reads up to 1."

### 3.4 How a pack loads

Everything goes through the core's real paths (`demo/load.rs`):

1. The CXF document is read by `lockdown_cxf::to_items`, exactly as an import of a CXF file is.
2. Each item gets its extras: class, state, history, inline files and fresh keys. Its filing (folders made from the CXF collections, normalized tags, favourite) goes on as the `org.*` fields. Each vault's items are written as one **Lockdown backup document**, which `App::import_preview(…, "lockdown", …)` and `App::import_commit` land. That is the restore path, which keeps history, class, state, folders and files.
3. Then come the app's own calls: placeholder files (`attach`), Watchtower dismissals (`security_dismiss`), templates (`save_user_template`) and the items made from them (`add_item` with `user:<id>`).

### 3.5 Making and swapping a pack

- **Try one without a rebuild:** **Developer → Load a demo pack…** and pick the file. A bare CXF export from another manager works as it is.
- **Make your own:** start from `crates/devtools/demo/default-pack.json`, or export a vault you built by hand from Lockdown as CXF and wrap it (`{"format":"lockdown-demo-pack","v":1,"name":"…","cxf": <the export>}`), then add extras by CXF item id.
- **Change the built-in default:** replace `crates/devtools/demo/default-pack.json`. The default is compiled in (`include_bytes!`). Its tests check that it reads and round-trips, holds every kind and variant, and uses only safe addresses and no keys. The current file was written by `cargo run -p lockdown-devtools --features demo --example make-default-pack`, which builds the items from the core's own templates and writes them with the core's own CXF exporter. It makes the same file every time.

### 3.6 The default pack

158 items in two vaults (Personal, and Work), made fresh at each open. One line per kind:

| Kind | In the pack |
|---|---|
| Login | Example Mail (password, an RFC 6238 code, a favourite, three earlier versions); a crown-jewel recovery account; three logins for one bank; weak, reused and lookalike ones; a login with a password and a passkey; a passkey-only login with two passkeys; one signed in with another account; an 8-digit code with a 60-second period; one with only a title; one with every field filled (three websites, password rules, Markdown notes, seven custom fields of every kind); a very long title, address and notes; ten in other scripts, right-to-left and emoji; an archived one; two in Deleted; 60 in bulk |
| Passkey | Example Docs passkey (made at open) |
| Code | Example VPN (RFC 6238 test secret), an 8-digit 60-second code, a counter (HOTP) code, 8 in bulk |
| Wi-Fi | Home (WPA2/WPA3), Guest (open), a hidden Enterprise lab network; the Router variant |
| SSH key | Build server key (Ed25519, made at open); the PGP variant (a placeholder, not a key) |
| API credential | Example API token, expiring |
| Document | Lease agreement (a text file and a 48 KiB placeholder PDF); Large archive (a 31 MiB placeholder, just under the import's 32 MiB attachment cap); the Medical record, Insurance, Recovery codes and Outdoor licence variants |
| Identity | Sam Rivera (every field); the Address, Business (in Work), Passport (expires within the month), Driver's licence, National ID, Vehicle and Contact variants |
| Card | Visa 4242 4242 4242 4242; Mastercard 5555 5555 5555 4444, expired; Amex 3782 822463 10005; Visa debit 4000 0566 5566 5556; Discover 6011 1111 1111 1117 with only its number |
| Bank account | Example Savings (test routing number, the published example IBAN) |
| Note | The welcome note (Markdown), a 200-paragraph note, a right-to-left note, an empty note, 12 in bulk |
| Crypto wallet | Test wallet: the BIP-39 test phrase "abandon … about", a crown jewel |
| Server | Build server (in Work, with history); the Email account variant |
| Database | Orders database (PostgreSQL, in Work) |
| Software licence | Example Editor licence |
| Membership | Example Gym, expiring |
| Custom | Locker combination (custom fields); two items made from a template of Sam's own, Bike |
| Template (hidden) | Bike, a template of Sam's own |
| Vault details (hidden) | Each vault's folders: Email, Finance, Finance/Cards, Home, Home/Network, Home/Network/Lab, Travel, Health, International, Shopping, Work/Engineering, Engineering/Servers, Engineering/Databases, Bulk, Bulk/Notes, Bulk/Codes, Bulk/Test sites |
| Consent (hidden) | Not in the pack: no path in the core makes one yet. The test fails when one exists. |

**Edge cases:** long titles, addresses and notes; Cyrillic, Japanese, Chinese, Arabic, Hebrew, Hindi, Thai and Korean, mixed left-to-right and right-to-left text, and emoji, including ZWJ sequences and flags; every optional field empty and every field filled; several logins for one site; weak (`password`, `123456`, `letmein`) and reused (`Summer2019!`) passwords, with one weak finding snoozed and one reused finding dismissed; a lookalike pair (`examplepay.test`, `examp1epay.test`); an expired card, a passport expiring this month, a 60-second 8-digit code and a counter code; nested folders three deep, tags (nested `work/clients`), favourites and crown jewels; items in Deleted and an archived one; item history with three earlier versions; a 31 MiB attachment.

**Safe content only:** every address is `example.com`, `example.org` or a `.test` name (a test checks every URL). Card numbers are the networks' published test numbers. The phrase is the BIP-39 test vector. The code secret is RFC 6238's. Passkeys and SSH keys are made at open, and the test checks that the file holds no key. Files are placeholders that say so.

## 4. The plugin and the menu

| Command (`plugin:devtools|…`) | Feature | Does |
|---|---|---|
| `status` | either | `{marker, reset, demo?}`: which tools this build has, and the demo's state |
| `reset_plan` | `dev-reset` | The plan of §2.3, worked out now |
| `reset_run` (`confirm`) | `dev-reset` | §2.5 step 1, then the restart. Refused unless `confirm` is `reset`. |
| `demo_open` | `demo-vault` | Opens the demo from the default pack, or answers the open one |
| `demo_reset` | `demo-vault` | Builds it again from its pack (or opens it) |
| `demo_leave` | `demo-vault` | Leaves; answers `first_run` or `vault` |
| `demo_load_pack` | `demo-vault` | The file dialog, the checks, then the demo from that pack; `null` when cancelled |

The permissions (`allow-<command>`, set `devtools:default`) are generated by `tauri-plugin` into `crates/devtools/permissions/`. They reach the desktop's build through the crate's `links = "tauri-plugin-devtools"`. The plugin grants the set to the `main` window in its setup (`CapabilityBuilder`, local content only).

The menu's items, in order: Open the demo, Reset demo data, Load a demo pack…, a separator, Reset Lockdown…. They are in `crates/devtools/src/menu.rs`, with a front-end copy (`devtools/menu.ts`) that a test keeps equal. On macOS the Developer menu is added to the menu bar after the default menus. On Windows and Linux it is the main window's own menu bar. Picking an item brings the main window forward and sends it the action.

## 5. Never in a release

| Build | Features | How it's enforced |
|---|---|---|
| `pnpm tauri dev`, `tauri build` with no override | both on | `tauri.conf.json` `build.features` |
| CI `desktop`, `desktop-windows` | both on | `--features shell,dev-reset,demo-vault` |
| CI `desktop-bare` | both off | `LOCKDOWN_DEVTOOLS=off`; `--features shell`; `check.sh absent dist`; `cargo tree` must not list `lockdown-devtools`; `build.rs` must refuse `dev-reset` with the variable off |
| Release dry run (pull request, `workflow_dispatch`) | both on | `check.sh present` on each desktop binary and `apps/desktop/dist`: the check is shown to see them |
| **`v*` tag** | **both off** | `LOCKDOWN_DEVTOOLS=off` on the Linux (through the container), macOS and Windows jobs; `--config src-tauri/tauri.release.conf.json`; `build.rs` refuses a mismatch; `check.sh absent` on each shipped desktop binary and `apps/desktop/dist` |
| Web vault, extension | never | Neither depends on the crate or compiles the folder. The release workflow runs `check.sh absent` on `apps/web/dist` and `extension/dist` every time. |

`check.sh` searches for `lockdown-devtools-6f1c2a` (the marker the Rust status command and the demo's front end both carry), `plugin:devtools|`, `demo_load_pack`, `reset_plan`, `lockdown-demo-pack` and `Lockdown QA pack`. It searches the front end before Tauri compresses it into the binary, and the binary as bytes.

## 6. The removal checklist

To remove the developer tools, delete or revert exactly these. CI's `desktop-bare` job already builds and tests the app in this shape (features and define off). The first group is required. The second is generic code the tools use, which is harmless to keep.

**Delete:**

1. `crates/devtools/` (the whole crate: sources, `demo/`, `permissions/`, `examples/`, `tests/`).
2. `apps/desktop/src/devtools/` (the whole folder).
3. `packaging/devtools/` (the check script).
4. `apps/desktop/src-tauri/tauri.release.conf.json`.
5. `design/capture/src/states/devtools.tsx`, `design/capture/src/states/developer-menu.tsx`, `design/capture/src/fixture/demo-pack.ts`.

**Edit:**

6. `Cargo.toml` (root): remove `"crates/devtools",` from `members` and from `default-members`. Remove the two comment lines starting `# The developer reset and the demo` and the line `lockdown-devtools = { path = "crates/devtools", version = "0.1.0", default-features = false }`.
7. `apps/desktop/src-tauri/Cargo.toml`: remove the four comment lines starting `# The developer tools (docs/plan/40-devtools.md)` and the lines `dev-reset = [...]` and `demo-vault = [...]`. Remove `lockdown-devtools = { workspace = true, optional = true }`.
8. `apps/desktop/src-tauri/build.rs`: remove everything above `#[cfg(feature = "shell")]` inside `main` (the two comment lines, `println!("cargo:rerun-if-env-changed=LOCKDOWN_DEVTOOLS");` and the `if … { panic!(…) }`).
9. `apps/desktop/src-tauri/src/lib.rs`: remove the three lines `// The developer reset and the demo (docs/plan/40-devtools.md).`, `#[cfg(any(feature = "dev-reset", feature = "demo-vault"))]` and `let builder = builder.plugin(lockdown_devtools::shell::plugin());`. The `let builder = …; builder.setup(…)` split may stay or be joined back.
10. `apps/desktop/src-tauri/tauri.conf.json`: `"features": ["shell", "dev-reset", "demo-vault"]` becomes `"features": ["shell"]`.
11. `apps/desktop/vite.config.ts`: remove the two comment lines and the `define: { __LOCKDOWN_DEVTOOLS__: … },` line.
12. `apps/desktop/src/App.tsx`: remove the two comment lines and the `const ResetMount = …` and `const DemoMount = …` lines; the comment `// The demo moved the app to its own home and back`, the `const { reload: reloadSettings } = useSettings()` line and the whole `const devtools = (…)` block; and both `{devtools}` lines. Revert the imports (`lazy`, `Suspense`, `useSettings`).
13. `design/capture/src/env.d.ts`: remove the comment and `declare const __LOCKDOWN_DEVTOOLS__: boolean`. `design/capture/vite.config.ts`: remove the comment and the `__LOCKDOWN_DEVTOOLS__: JSON.stringify(true),` line. `design/capture/src/harness/registry.ts`: remove the `DEVTOOLS_STATES` import and `...DEVTOOLS_STATES,`. `design/capture/src/fixture/commands.ts`: remove the `MARKER as DEVTOOLS_MARKER` import, the comment, and the `"plugin:devtools|status"` answer.
14. `.github/workflows/ci.yml`: in `desktop` and `desktop-windows`, `--features shell,dev-reset,demo-vault` becomes `--features shell` (four lines), and the comment above it goes. Delete the `desktop-bare` job.
15. `.github/workflows/release.yml`: remove the header paragraph starting `# - The developer tools`; the `LOCKDOWN_DEVTOOLS:` env line in `linux`, `macos` and `windows`; the `$([ "$LOCKDOWN_DEVTOOLS" = off ] && echo --config src-tauri/tauri.release.conf.json)` suffix on the two `pnpm tauri build` lines; and the five steps named `The developer tools are out of a release, in a dry run`, `The extension never carries the developer tools` and `The web vault never carries the developer tools`.
16. `packaging/repro/build-desktop.sh`: remove `-e LOCKDOWN_DEVTOOLS="${LOCKDOWN_DEVTOOLS:-on}" \`. `packaging/repro/desktop-container.sh`: remove the `release_config` comment and block, and the `"${release_config[@]}"` argument.

**Optional (generic, harmless, tested on their own):**

17. The confined home: `crates/app/src/confine.rs`, `mod confine;` and the `confined` field in `crates/app/src/lib.rs`, the `refuse_confined()?` lines in `lib.rs`, `transfer.rs` and `bridges/age_export.rs`, and the `self.confined.is_some()` check in `backup_tick`; `crates/app/tests/confine.rs`; `hold_network` and `network_held` in `crates/sync/src/transport.rs` (and their `lib.rs` re-export) and `crates/security/src/http.rs`; the `confined` arm and helper in `apps/desktop/src-tauri/src/browser/handler.rs` and their test.
18. The first-run seam: `apps/desktop/src/features/unlock/demo-entry.ts`, the **Explore the demo** block and `useDemoEntry` in `unlock-screen.tsx`, and the test in `unlock-screen.test.tsx`. The first-run slice owns this seam (doc 39 §1.13), so keep it if that slice has landed.
19. `reload` in `apps/desktop/src/app/settings.tsx`.

## 7. Running and testing

```sh
export CARGO_TARGET_DIR=… CARGO_PROFILE_DEV_DEBUG=line-tables-only
cargo test -p lockdown-devtools                       # the reset, the demo, the pack, keys, the menu list
cargo test -p lockdown-app --test confine             # the confined home
cargo test -p lockdown-desktop --features shell,dev-reset,demo-vault
cargo run -p lockdown-devtools --features demo --example make-default-pack   # write the default pack
cd apps/desktop && pnpm test src/devtools src/features/unlock
LOCKDOWN_DEVTOOLS=off pnpm build && ../../packaging/devtools/check.sh absent dist
pnpm tauri dev                                         # both tools on; Developer menu in the menu bar
```

| Test | Proves |
|---|---|
| `crates/devtools/tests/reset.rs` `a_reset_of_a_fully_used_profile…` | A profile with an account on a real in-process server, items, settings, a kit, browser pairings, the endpoint file and the shell's folders, in a temporary home with a keystore double. After the reset, nothing of Lockdown is left; a sibling's keystore entry and the person's own file are; the second launch probes as the first run. The pending file runs once. |
| `…an_identity_another_app_uses_is_kept` | An identity naming another app is kept (through the unverified reading) while Lockdown's data goes; an unreadable one is kept with why. |
| `…the_guard_refuses…`, unit `the_guard_keeps…` | The guard on Unix and on Windows paths. |
| `crates/devtools/tests/demo.rs` | The default pack reads and round-trips, byte for byte and through CXF export and import. Every template, its own and each variant, has an item. Only safe addresses, no keys, the test vectors present. A bad pack is refused with the reader's sentence. The demo holds every kind the core makes (Consent asserted unmakeable), two vaults, trash, archive, crown jewels, history, fresh passkeys and SSH keys, the 31 MiB file, and weak and reused findings. It is read-write; it refuses exports and the network; reset builds it again with new keys; leaving restores the real profile byte for byte, with the keystore never written; from a first run it leaves to the first run. |
| `crates/app/tests/confine.rs` | Rehome there and back; exports and backups refused before any gate; both HTTP clients held; released at home. |
| `browser::tests::a_confined_home_reads_as_locked…` | The extension sees a confined app as locked. |
| `apps/desktop/src/devtools/*.test.tsx`, `features/unlock/unlock-screen.test.tsx` | The dialog's lists, the word arming the button, the shell's refusal shown, a fresh dialog each time; the demo's bar, leave, the menu's demo items, nothing drawn without the demo; the first run's link only while a demo is installed. |

The review harness (`design/capture`) draws the first run with **Explore the demo**, the Developer menu's list (the OS draws the real menu), the reset dialog with the word typed, its restart over the lock screen, the demo open with its bar, and one item of each of the 17 visible kinds, read from the default pack itself (`fixture/demo-pack.ts`).

## 8. Gaps

- **The keystore search runs on Linux only here.** The macOS and Windows halves were checked with `cargo clippy` for `aarch64-apple-darwin` and `x86_64-pc-windows-msvc` and are built by the release dry runs. They haven't run against a real keychain or Credential Manager.
- **The Developer menu is a top-level menu on macOS**, after the default ones, not an item inside the Lockdown menu.
- **Old passwords.** Watchtower has no password-age finding, and the restore path stamps every version with the time of the import, so the pack's earlier versions (meant to be a month apart) all read as made when the demo opened. The pack still has history with several versions.
- **Opening the demo takes about 20 s in a debug build**, mostly the 31 MiB placeholder and 158 signed items. A release build wasn't measured here.
- **The lookalike pair is in the pack, but its finding isn't asserted.** Whether `examp1epay.test` is flagged depends on the matcher's rules for `.test` names.
- **Consent items** aren't in the demo, because no path in the core makes one.
- **A reset while another ecosystem app shares the identity** leaves this device unenrolled in it (§2.4). That is by design, but it's a dead end until the person joins from that app or recovers with the kit.
- **The harness draws the demo from a TypeScript reading of the CXF document** that follows `to_item`. Fresh keys are drawn as stand-ins.

## 9. Proposed decisions

For the coordinator to record. **Flagged** rows are user-visible and need Sean. "Audit item" rows go to the external audit under D38.

| ID | Decision | Owner |
|---|---|---|
| D480 | **The developer tools are one crate and one folder, removable in one go.** `crates/devtools` and `apps/desktop/src/devtools/`, behind the desktop's features `dev-reset` and `demo-vault` and the front end's `__LOCKDOWN_DEVTOOLS__` define. The app touches them in one plugin line and two lazy mounts. A removal checklist (doc 40 §6) lists every line, and CI's `desktop-bare` job builds and tests the app without them. Plain version: the reset and the demo can be taken out later without touching the rest of the app. | Accepted under D38 (structure, not security) |
| D481 | **Never in a release, and CI proves it.** They are on in development builds, CI and release dry runs, and off in a `v*` tag build (`LOCKDOWN_DEVTOOLS=off` and `tauri.release.conf.json`). The desktop's `build.rs` refuses a mismatch. `packaging/devtools/check.sh` fails a tag build whose binaries or front end carry the tools' marker, commands or pack, and fails a dry run that doesn't (so the check is shown to work). The web vault and the extension never get them, and are checked on every run. Plain version: a real release can't ship the reset or the demo. | Accepted under D38; audit item |
| D482 | **What the developer reset erases.** Lockdown's data directory, the shell's own folders, every keychain entry under `design.snds.lockdown` (found by each platform's search), the webview's storage, and the endpoint file. It leaves the account on a server, exports and backups in the person's folders, the browsers' host registrations (written again at launch), and anything of other apps. The keystore and web storage go at once; the folders go at relaunch, before anything opens; a guard never touches the top of the disk or the home folder. The dialog lists all of it, says it can't be undone and that a server's account isn't deleted, and needs `reset` typed. Plain version: one menu item puts Lockdown on this computer back to new. | **Flagged for Sean** (the words and the scope) |
| D483 | **The reset and the shared identity.** The identity root is erased only when every record in it names Lockdown alone (the profile's manager, each persona's maker, each key envelope's app). One that doesn't verify is read record by record, and kept if any record names another app. Otherwise it is kept whole, and the dialog says so and names the apps. It is never partly edited, because that would need the identity's key. Plain version: if Mailbird or Synapse use your identity, a Lockdown reset leaves it for them. | **Flagged for Sean** (after a reset beside a sibling app, this device reads as not enrolled); audit item |
| D484 | **Only the main window reaches the tools.** Their commands are a Tauri plugin's, granted at run time to the `main` window by the plugin itself, with no capability file. The menu is the only way in. The shell checks the typed word again and works the plan out again when it runs; nothing from the page names a path. Plain version: no web page, extension or other program can reset Lockdown. | Accepted under D38; audit item |
| D485 | **The demo is a confined home.** `App::rehome` locks the person's vault and moves the app to `<data>/demo/` with its own identity and device-key file, no account and nothing in the keychain. While confined, every request through Lockdown's two HTTP clients is held for the process, exports and backups are refused before any gate, and the browser endpoint reads as locked. Leaving moves back, locked, and wipes the folder, and a folder left by a crash is wiped at the next launch. Plain version: the demo can't touch your real vault, go online or leave anything behind. | Accepted under D38; audit item |
| D486 | **The demo pack format, version 1.** A CXF document plus extras for what CXF can't say (vault, crown jewel, state, history, files, fresh keys, Watchtower states, templates of one's own). A bare CXF file is a pack. Packs are checked before loading, unknown members are refused, and faults are one sentence. A pack loads through the real import: a backup document landed by the restore path, then the app's own calls. Keys are never in a pack. Plain version: the demo's contents are a file you can swap, in the format other password managers export. | Accepted under D38 (structure, not security) |
| D487 | **The default QA pack.** 158 items in two vaults: every kind and variant, the edge cases of doc 40 §3.6, and bulk for scrolling and search. Only `example.com`, `example.org` and `.test` addresses, the networks' test card numbers, the BIP-39 test phrase and the RFC 6238 secret; keys are made at each open. Plain version: the demo shows everything Lockdown can hold, with nothing real in it. | **Flagged for Sean** (the demo's contents) |
| D488 | **The demo's ways in and out, and its words.** **Explore the demo** on the first run (on today's intro screen until the first-run slice puts it on Welcome); **Developer → Open the demo**, **Reset demo data** and **Load a demo pack…**; the bar "You're exploring the demo. Nothing here is real." with **Leave the demo**, which returns to the first run or the locked vault. Records Sean's call on doc 39 Q5 (demo mode, not a vault beside one's own). Plain version: you can look around without setting anything up, and leave in one click. | **Flagged for Sean** (the words) |
| D489 | **Shape additions** (D62). Public `lockdown_app::App::rehome` and `App::confinement`; `lockdown_sync::{hold_network, network_held}`; `lockdown_security::http::{hold_network, network_held}`; a new IPC namespace `plugin:devtools|…` (`status`, `reset_plan`, `reset_run`, `demo_open`, `demo_reset`, `demo_leave`, `demo_load_pack`) and the event `devtools://menu`, all in builds with the tools only; the front end's `useSettings().reload` and `features/unlock/demo-entry.ts`. No existing wire shape changed. Plain version: new pieces only; nothing the app already sends changed. | Accepted under D38 (structure, not security) |

### unicron host-contract gaps

- **No app-level banner.** unicron has no component for a persistent strip above the whole app (the demo's bar). It is a plain element on the theme's tokens (`bg-primary`, `text-primary-foreground`) holding unicron's `Button`. A `Banner` or `AppBar` with a "notice" tone, which reserves its own height, would replace the CSS offset in `devtools/devtools.css`.
- **Native menus can't be drawn.** The review harness draws the Developer menu's list in the menu's place. A unicron stand-in for an OS menu (for reviews only) would make that capture match the other surfaces.
