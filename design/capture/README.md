# design/capture: the design-review harness

The desktop app, the web vault and the browser extension, drawn by their own components over one invented fixture, so every screen and its states can be looked at, captured and checked for accessibility without a vault, a server, Rust or a browser extension install.

It makes two things, both written outside the repo:

- **The capture**: a PNG of every state at 1280, 900 and 390 px wide (the popup and the in-page frames at their natural size), light and dark, the vault list in compact density too; `manifest.json` (one entry per image), `axe.json` (axe-core's violations per image) and `capture-log.json` (step failures, console errors, commands the fixture had no answer for).
- **The review build**: the same page as static files (`index.html`, the in-page frames' own `frame.html`, and `assets/`, relative paths, hash routing, no network, no service worker, no wasm) that a page host can serve, so the screens can be tapped through on a phone. Every state stays live: items open, menus and dialogs work, against the same fake core.

## Run it

```sh
cd design/capture
pnpm install --frozen-lockfile
pnpm build                  # dist/: the harness, built once
pnpm capture                # every state → /home/user/design-capture/2026-10-29/
pnpm shots --out <dir>      # build, then every state in a fixed window: 1440x900 and 390x844, light and dark
pnpm shots:built --out <dir> # build, then the finished screens for review, at twice the density
pnpm review:build           # dist/ checked and copied to /home/user/design-capture/review-build/
pnpm engine:check           # the desktop screens in WebKit and Chromium: icons, print, a smoke (CI's `engines` job)
pnpm dev                    # the same page with Vite's dev server, for working on states
pnpm typecheck && pnpm lint # what CI runs (the `design-capture` job)
```

Chromium comes from `PLAYWRIGHT_BROWSERS_PATH`; the capture never installs one. `pnpm engine:check --engines webkit,chromium --out <dir>` opens the main desktop screens in each engine named (the Mac app runs in WKWebView, so WebKit is the one that matters), fails when an icon doesn't draw as one glyph or a page that prints alone would print the app behind it, and writes `engine-check.json` and a PNG per screen and engine; WebKit is installed by CI on its runner, not here ([doc 38](../../docs/plan/38-mac-fixes.md)). `pnpm capture` takes `--out <dir>`, `--only <id prefix>` (repeatable), `--widths 1280,390`, `--themes light`, `--jobs 3` and `--skip-axe`; with `--only`, the re-taken states' images replace their earlier ones in the manifest, `axe.json` and the log, and every other entry stays. `pnpm review:build` takes `--out <dir>` and fails if the build breaks the host's limits (16 MB, 255 files, absolute paths).

**Shots** are the before-and-after pictures for a redesign: `pnpm shots` builds the harness, then takes every state in a window of exactly 1440×900 (desktop) and 390×844 (phone), at 1×, light and dark, never grown to the content, so a picture lines up with a design frame. A state drawn only at desktop widths gets no phone picture; the popup and the in-page frames stay at their natural size. Names are `<state>--w1440x900-dark.png`, `<state>--w390x844-light.png` and `<state>--natural-dark.png`, with the same `manifest.json`, `axe.json` and `capture-log.json`. It exits non-zero if any picture could not be taken, or its state never reported ready or its steps stopped, and prints which. Give `--out` an absolute path (a relative one is taken from `design/capture`; the default is `test-results/shots`), keep shots in a folder of their own, and add `--only <id prefix>` for a few states or `--skip-axe` for speed.

`pnpm shots` takes other windows too: `--sizes 768x1024` is the medium window and `--sizes 844x390` a phone on its side (several sizes go in one list, `--sizes 1440x900,768x1024,390x844,844x390`). A window under 600 px wide is taken for the states drawn at a phone's width, a wider one for the states drawn at a desktop's. `--scale 2` takes each picture at twice the pixel density; the names stay the same.

**Built shots** are the pictures of finished screens. A state says its screen is finished by naming its place: `built: { area: "items", screen: "list", kind: "drawn" }` (`drawn` has a design frame, `undrawn` a written brief). `pnpm shots:built --out <dir> [--only <id prefix>]` takes every such state at twice the density, at 390×844 and at 1440×900 where the state is drawn at that width, the popup and the in-page frames at their natural size, and writes `<dir>/<drawn|undrawn>/<area>--<screen>__<phone|desktop>__<graphite|stone>.png` (light is the stone ground, dark is graphite; a natural-size picture is named `desktop`). Several people can run it into one folder at the same time: a run writes only the pictures it took, deletes nothing, reads no earlier manifest and names its own (`manifest--<run>.json`, which also holds that run's log and axe results). Two states may not name the same place; the run stops and names both. A screen that is broken or half built gets no `built`.

In the review build, `#/` is the list of every surface, screen and state; `#/<state id>` opens one (`#/desktop-vault-filled`), and every image id in the manifest opens too (`#/desktop-vault-filled--w390-dark` lands on that state, dark, at that width). The bar at the bottom goes back to the list, switches light and dark, and switches the width: Phone draws the state in the page; 900 and 1280 draw it in a frame that wide, scaled to fit, so the apps' own breakpoints see a desktop window.

## How it works

Nothing in the apps changes. The harness imports their components from their sources and fakes only the edges where a host would answer:

| Surface | What is real | What is faked |
|---|---|---|
| Desktop | `apps/desktop/src/App.tsx` and everything under it, the `tauri*Client` adapters included | `@tauri-apps/api/core` (`invoke`) and `@tauri-apps/api/event` (`listen`), aliased in `vite.config.ts` to `src/host/tauri-*.ts` |
| Web vault | `apps/web/src/app/web-app.tsx`, its sign-in, unlock, key and link pages, and its real `webClient` / `webSession` / depth, live and server clients | the worker `Link` (`src/host/web-link.ts`) |
| Extension | `extension/src/popup/popup.tsx` and `extension/src/pages/frame.ts` (the in-page menu, save prompt and passkey sheet) on `static/tokens.css` and `static/ui.css` | `chrome.runtime.sendMessage` (`src/host/chrome.ts`), answering as `background/index.ts` would |

The in-page frames load their own page, `frame.html` (entry `src/frame-main.ts`), as the extension's frames load theirs: the test page's iframe opens it, and it gets the extension's two sheets and nothing of the apps' styles. (From one shared entry the bundler preloaded the apps' sheet into the frame as well, whose `--muted` and `--accent` are surfaces, so the 2026-10-29 review saw the frames' secondary text near invisible; doc 32 §2.) The plain test page around a frame is a page with its own `h1` and `main`, so axe's page rules judge the frame and not the stand-in.

All three reach one fake core (`src/host/core.ts`): a table from command name to answer, built over one fixture world (`src/fixture/`). A command with no answer refuses in the app's own error shape and is recorded in `capture-log.json`. The item templates are the core's own, read from `crates/vault/src/template.rs` at build time (`scripts/rust-templates.ts`), so the editor and every detail are drawn from the same table the core uses.

**The fixture** is Sam Rivera's vaults: 43 items across every kind (logins with several sites, passkeys and codes; a passkey-only login with one passkey hidden by its site; cards; a bank account; Wi-Fi and a router; notes; an identity, a passport and a contact; insurance and recovery codes; SSH and PGP keys; an API token; a server, a database, a licence, a membership, a custom item; a crypto wallet as the crown jewel; a Steam Guard code and a counter code), three vaults (Personal, Family shared with Alex and Jordan, Work), folders and tags, a conflict copy, expiring items, an archived and a trashed item, security findings in all three groups, six devices (one nobody recognises, one revoked), live sessions, backups, contacts with fingerprints, share and request links, emergency access, alias services. Every site is a `.example` domain or an invented brand; card numbers are the networks' published test numbers; keys, phrases and codes are made up (the codes are stand-ins, not real TOTP). Dates are relative to the moment the page opened, and the capture pins the browser's clock, so images are stable (the in-page frames alone run on the real clock: they arm their buttons by it).

**States are filed by who owns the screen**, so people who work at the same time never edit the same file:

| File | Owner | What |
|---|---|---|
| `states/shell.tsx`, `states/defs.tsx`, `states/steps/shell.ts` | foundation | What the frame draws by itself: navigation, messages, the gate, the lock, the backup line, toasts and reminders. `defs.tsx` mounts the apps and declares a state (`d` for the desktop, `w` for the web vault). |
| `states/vault.tsx`, `states/steps/vault.ts`, `fixture/lane-vault.ts` | vault | The list, views, scopes and trees, item detail, organizing, the palette, the quick window, search, Health. |
| `states/screens.tsx`, `devtools.tsx`, `developer-menu.tsx`, `server.tsx`, `states/steps/screens.ts`, `fixture/lane-screens.ts` | screens | Unlock, first run, join, migration, forgot, Settings, Recovery, Status, the server, import, export, backups, the developer tools; the web vault's sign-in, unlock, Settings, link and key pages. |
| `states/edges.tsx`, `extension*.ts(x)`, `states/steps/edges.ts`, `fixture/lane-edges.ts` | edges | Approvals, devices and sessions, pairing, codes, the scan, sharing, an item's Share, the editor, the new-item menu, templates; the web vault's Links and People; the extension. |

A state's id never changes, whichever file holds it: before and after pictures pair up by it. **Steps that press another owner's control go through that owner's helper** (`states/steps/`): any state reaches a place with `go("codes")` (the shell's), opens the app with `open` (the screens lane's unlock), opens an item with `openItem(title)` and presses its actions with `itemAction("Share")` (the vault's), starts a new item with `newItem(/^Login/)` (the edges lane's). When an owner renames a control, it fixes its own helper and every state follows. The fixture's seed, world, commands and story are the foundation's; a lane's own `setup` and `answers` go in its `fixture/lane-<lane>.ts`. `fixture/shape.ts` holds the fixture in the shape the design draws (a login with every method and a five-character code, a passkey-only login, a note with a network and a password, a vault of 300 rows, a code seconds from its end, a pairing and a passkey request), which a state opts into; the seed is unchanged by it.

**A state** (`src/states/*.tsx`) names its surface, screen, state, the design brief's row (`docs/plan/11-desktop-design-brief.md` §3.2–§3.5, or `null`), notes, the widths, any change to the fixture, any answers of its own, and the steps that reach it. The steps run in the page (`src/harness/driver.ts`: click by accessible name, type, wait), so the capture and the review build land on the same screen from the same address. The extension's frames ignore synthetic clicks by design (they need a trusted, uncovered click), so the few states one press deep inside a frame list `trusted` buttons that the capture presses with real clicks; in the review build, tap them yourself.

## What it can't reach, and why

- **The server's own pages** (admin sign-in, Overview, Accounts, Invites, Backups, Passkeys, the email approval page, the operator recovery kit, the `/s/` and `/r/` notices): `bins/server/src/admin.rs` and `pages.rs` write their HTML with `format!` per request. There is no template to render without running the Rust server, which this harness never builds.
- **OS and browser surfaces Lockdown doesn't draw**: OS notifications, macOS AutoFill and the system passkey sheet, Windows Hello, the browsers' own passkey dialogs, native open and save dialogs, the tray, and registering the global shortcut. The quick-access window is drawn (`?window=quick`'s component), not the OS window around it.
- **The camera**: the Sign in a TV dialog's Camera tab needs a camera; its Picture and Paste tabs are captured.
- **Behaviour only the core has**: real one-time codes, zxcvbn strength (a length rule stands in), OPAQUE, sync, key transparency checks. The words the screens show for them come from the fixture, written to match the core's sentences.
- **The covered in-page menu** is drawn with a page element over it, so the browser's IntersectionObserver v2 keeps the menu's buttons disabled, as it would on a hostile page; the "covers Lockdown's menu" message after a click on a covered frame needs a trusted click on a covered frame, which the browser refuses to deliver.
