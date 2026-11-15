# 63: the redesign's harness: pictures of finished screens, states and tests filed by owner

_Slice: `found-2` of the 2026-11 redesign, on `claude/keen-ritchie-q2hgb2`, built 2026-11-14. No product change: no screen, no control, no word of the app differs. This slice rearranges the review harness, the web end-to-end suite and eight desktop test files so that three lanes (vault, screens, edges) can rebuild screens at the same time without editing one another's files._

## For future agent

- **TL;DR:** Every harness state, step helper, end-to-end selector and split test file now has one owner. `pnpm shots:built` takes the finished screens for review. `fixture/shape.ts` holds the fixture in the shape the design draws. Nothing the app draws changed.
- **Owners:** foundation (the shell; the slices named `found-*`), and the three lanes: **vault**, **screens**, **edges**. §2 has the table.
- **Do:**
  - Give a state `built: { area, screen, kind }` when its screen is finished and looked at, then run `pnpm shots:built --out <dir>`.
  - Reach another owner's control through that owner's helper (`design/capture/src/states/steps/`, `apps/web/e2e/steps/`). If the helper you need does not exist, ask the owner for it; do not write the selector in your own file.
  - Put a lane's fixture changes in `design/capture/src/fixture/lane-<lane>.ts`.
- **Do not:**
  - Change a state's id. Before and after pictures pair up by it.
  - Write a selector in an end-to-end spec. The six specs call helpers only.
  - Edit `fixture/seed.ts`, `world.ts`, `commands.ts`, `story.ts` or `shape.ts` from a lane. They are the foundation's.
  - Mark a broken or half-built screen `built`.
  - Run `pnpm shots` and another build of `design/capture/dist` at the same time.
- **Read with:** `design/capture/README.md` (how the harness works and every command), doc 62 (unicron 1.0).

## 1. Pictures

`design/capture/scripts/capture.ts`, `package.json`, `src/harness/types.ts`, `registry.ts`.

| What | How |
|---|---|
| The before-and-after set | `pnpm shots --out <dir>`, as before: 1440×900 and 390×844, light and dark, at 1×. |
| Other windows | `--sizes 768x1024` (medium) and `--sizes 844x390` (a phone on its side). Any `WxH` works. A window under 600 px wide is taken for states drawn at a phone's width; a wider one for states drawn at a desktop's. |
| Density | `--scale 2` (1 to 4). Names do not change. |
| Finished screens | `pnpm shots:built --out <dir> [--only <id prefix>]`. |

**`built`.** A state names its finished screen: `built: { area: "items", screen: "list", kind: "drawn" }`. `drawn` has a design frame; `undrawn` has a written brief. `shots:built` takes every state that has one:

- at twice the density;
- at 390×844 and 1440×900, each where the state is drawn at that class of width; the popup and the in-page frames at their natural size;
- light as **stone**, dark as **graphite**;
- to `<dir>/<drawn|undrawn>/<area>--<screen>__<phone|desktop>__<graphite|stone>.png` (a natural-size picture is named `desktop`).

**Three lanes write into one folder.** So a `shots:built` run:

- writes only the pictures it took, and deletes nothing;
- reads no earlier manifest, and names its own `manifest--<run>.json` (time and process id), which holds that run's pictures, its log and its axe results;
- stops before taking anything if two states name the same place, and names both;
- needs `--out`: there is no default folder.

**The clock.** Every state is now captured with the clock pinned, except the in-page frames (`live_clock: true`), which arm their buttons by the real clock. Before, no extension state was pinned, so `extension-popup-code` drew a different code in every run.

## 2. Who owns what

| Owner | States (`design/capture/src/states/`) | Step helpers | Fixture | Web e2e helpers (`apps/web/e2e/steps/`) |
|---|---|---|---|---|
| foundation | `shell.tsx`, `defs.tsx` | `steps/shell.ts` | `seed.ts`, `world.ts`, `commands.ts`, `story.ts`, `shape.ts`, `time.ts`, `ids.ts`, `demo-pack.ts` | `nav.ts`, `gate.ts` |
| vault | `vault.tsx` | `steps/vault.ts` | `lane-vault.ts` | `items.ts`, `search.ts`, `health.ts` |
| screens | `screens.tsx`, `devtools.tsx`, `developer-menu.tsx`, `server.tsx` | `steps/screens.ts` | `lane-screens.ts` | `unlock.ts`, `settings.ts` |
| edges | `edges.tsx`, `extension.tsx`, `extension-frame*.ts`, `extension-scenarios.ts` | `steps/edges.ts` | `lane-edges.ts` | `editor.ts`, `sharing.ts`, `codes.ts`, `devices.ts` |

`desktop.tsx`, `desktop-more.tsx` and `web.tsx` are gone; their 224 states are in the four files above, by this rule:

- **shell (foundation):** `desktop-backup-late`, `desktop-idle-toast`, `desktop-live-*`, `desktop-reminder-*`, `desktop-passkey-upgrade-toast`, `desktop-item-wallet-gate`, `desktop-locked-wash-narrow`. Entry, navigation, chrome, messages and needs-you states go here when the shell slice adds them.
- **vault:** `desktop-vault-*` (but the new-item menu), `desktop-view-*`, `desktop-scope-*`, `desktop-nav-*`, `desktop-item-*` (but Share and the wallet gate), `desktop-palette*`, `desktop-quick-window-*`, `desktop-security-*`, `web-vault`, `web-item-*`, `web-security`.
- **screens:** `desktop-unlock-*`, `desktop-first-run-*`, `desktop-migration-*`, `desktop-settings*`, `desktop-recovery*`, `desktop-status*`, `desktop-devtools-*`, and the web vault's sign-in, unlock, Settings, link and key pages.
- **edges:** `desktop-approval-*`, `desktop-devices*`, `desktop-codes-*`, `desktop-sharing*`, `desktop-item-share-*`, `desktop-editor-*`, `desktop-vault-new-item-menu`, `web-codes`, `web-devices`, `web-links`, `web-people`, and every `extension-*`.

**Proof that nothing moved but the files.** Before and after the split, every state's steps, set-up, answers and mount were written out from an unminified build and compared: 283 states, 0 differences.

### 2.1 Step helpers

A step that presses a control another owner draws goes through that owner's helper. A lane that renames its control fixes its own helper and nothing else.

| Helper | Owner | What it does today |
|---|---|---|
| `go(placeId)` | shell | Presses the place's sidebar entry. Returns steps, so the shell slice can make it work at every width (a tab, a pushed screen) without touching a state. Place ids: `favourites`, `passkeys`, `codes`, `wi-fi`, `contacts`, `expiring`, `attention`, `archived`, `deleted`, `security`, `sharing`, `devices`, `recovery`, `status`, `settings`, `links`, `people`, `vault-family`, `folder-finance`, `tag-travel`. |
| `lock()`, `answerGate(password)`, `DIALOG`, `MENU`, `VAULT_OPEN`, `starts` | shell | The lock, the gate's answer, the shared waits. |
| `open`, `webOpen`, `unlock`, `create`, `kitSaved`, `settings`, `toBackups` | screens | Unlock and the first run. Every state that needs the open app starts with `open`. |
| `openItem(title)`, `itemAction("Share" \| "Edit" \| "Trash")`, `reveal(label)`, `healthDevice(name)` | vault | The list row, the detail's actions, a masked field, a device named on Health. |
| `newItemMenu`, `newItem(entry)`, `choose` | edges | The New item menu and what it starts. |

The cases the plan named: the edges lane's item-share states call `itemAction("Share")`; the developer tools' demo states and every other state that opens an item call `openItem(title)`; `desktop-item-wallet-gate` (the shell's) opens its item with `openItem` and presses `reveal("Recovery phrase")`.

### 2.2 The fixture in the design's shape

`fixture/shape.ts` (foundation). A state opts in with `setup` or `answers`; the seed is unchanged, so no existing picture changes. Every brand is invented and every site a `.example`.

| The design draws | In the fixture |
|---|---|
| A login with a password, a passkey, a five-character code and a protected recovery note | `SHAPE.login` ("Harrier Games"), with `withShape` |
| A login that is a passkey and nothing else | `SHAPE.passkeyLogin` ("Larkspur Docs"), with `withShape` |
| A card, for the card art | `SHAPE.card` (the seed's "Bluefin Visa") |
| A secure note with a network and a password | `SHAPE.note` ("Cabin network"), with `withShape` |
| A code seconds from expiry | `expiringCode(title, secondsLeft)` as `answers` |
| Findings: weak, reused, no second step | the seed's report (`story.ts`), unchanged |
| Two vaults with folders and tags | the seed's Personal and Family, unchanged |
| A long title | `SHAPE.longTitle` (the seed's) |
| A pending passkey request and a pairing request | `PASSKEY_REQUEST`, `PAIRING_REQUEST` |
| A vault of 300 rows | `longVault` |
| The clock | pinned by the capture (§1) |

Five new states show them: `desktop-vault-long`, `desktop-item-login-methods`, `desktop-item-login-passkey`, `desktop-item-note-network` (vault) and `desktop-codes-expiring` (edges). The harness has 288 states.

The "protected recovery note" is a hidden field of the person's own on the login. The core has no per-field protection beyond masking, and this slice adds none.

## 3. Web end to end

`apps/web/e2e/`. The six specs hold no selector: every `getByRole`, `getByLabel`, `getByText`, `getByTestId` and `locator` moved, unchanged, into `e2e/steps/<file>.ts`, filed by the owner of the control (§2). The specs keep their set-up, their order and every assertion. A helper either returns a locator the spec asserts on, or does one thing a person does (`signIn`, `go(page, "devices")`, `answerGate`).

- Foundation owns `nav.ts` (go to a place, `vaultOpen` to wait on, `lock`, `alert`) and `gate.ts` (answer the gate).
- The link pages a person with no account opens are in `unlock.ts`, with sign-in and unlock: the screens lane owns those pages.

13 tests before, 13 after.

## 4. Desktop tests, one file per owner

A test file that rendered two owners' components is now one file per component. Test bodies were copied byte for byte by a script; each file keeps only the imports and helpers it uses.

| Was | Now | Owner |
|---|---|---|
| `depth/depth.test.tsx` | `depth.test.tsx` (the helpers; frozen) | foundation |
| | `add-device.test.tsx`, `key-check.test.tsx` | edges |
| | `join-device.test.tsx` (joining from another device, screen J's sealed join, the wait for a security key) | screens |
| | `reminders.test.tsx` | foundation, then screens |
| | `item-protection.test.tsx` | vault |
| `browsers/pairing-dialog.test.tsx` | `pairing-dialog.test.tsx` | edges |
| | `browsers-section.test.tsx` | screens |
| `passkeys/passkeys-block.test.tsx` | `passkeys-block.test.tsx` | vault |
| | `passkey-approval-dialog.test.tsx` | edges |
| `bridges/bridges.test.tsx` | `clock`, `federation`, `incidents`, `alias-field`, `alias-settings`, `fill-fix`, `emergency-sheet`, `setup-guide`, `backup-indicator` (`.test.tsx` each), and `export/export-dialog-age.test.tsx` | whoever owns the component |
| `updates/about-section.test.tsx` | `about-section.test.tsx` | screens |
| | `restart-row.test.tsx`, `use-update-notices.test.tsx` | foundation |
| `organization/organization.test.tsx` | `organization-nav`, `row-menu`, `place-panel`, `files-panel`, `duplicates-review` (`.test.tsx` each) | vault |
| | `vault/editor-organize.test.tsx` | edges |
| `security/security-view.test.tsx` | `security-view.test.tsx`, `item-security.test.tsx` | vault |
| `sharing/item-share.test.tsx` | `item-share.test.tsx`, which mounts `ItemShare` by itself | edges |
| | `vault/detail-share-entry.test.tsx`, which goes through the detail | vault |

**Two places where one test became several**, because it rendered every component at once:

- `bridges`: "say nothing where the host has no bridges client" rendered eight components and made one assertion (no text). It is now eight tests, one per component, each with that assertion. 10 tests became 17.
- `item-share`: the six tests all went through `DetailPane`. The two share flows now mount `ItemShare` directly, with every assertion kept. The four that are about the detail (what each right offers, and no sharing on a crown jewel) moved to `detail-share-entry.test.tsx` unchanged, with one new test that Share opens from the detail and offers both ways. `item-share.test.tsx` gained one test that `ItemShare` offers nothing for a shared item or a crown jewel. 6 tests became 8.

Desktop tests: 290 before, 299 after. No assertion was removed or loosened.

Older docs name the old files (`22-security-depth.md`, `23-bridges.md`, `45-desktop-join-sealed.md`, `31-design-fixes-1.md`, `16-browser-extension.md`). They are records of their slices and are left as written; this table is the map.

## 5. Tests and what is not proven

| Check | Result |
|---|---|
| Every state's steps, set-up, answers and mount, before and after the split | 283 states, 0 differences |
| `pnpm shots` strict, against the `found-1` set | 1,070 pictures (1,050 and the 20 of the five new states), 0 failures. Of the 1,050: 891 identical, 144 within 50 pixels, 13 that differed more under machine load and were identical or within 50 pixels when taken again, and the two of `extension-popup-code`, which now draw the same code in every run (§1, the clock) |
| Lane mode of the check script, in a second worktree | every step passes with no cargo build; the shared cargo target is the same size to the byte before and after; the web end-to-end run used its own port |
| `shots:built` with one state marked | exactly four files at twice the density, named as §1 says; a file and a manifest already in the folder untouched |
| Desktop, web (unit and end to end), capture, extension, `cargo fmt` | every step passes; desktop 299, web 60 and 13, extension 80 |

Not proven here: `pnpm ds:ratchet` (CI runs it; counted by reading: 4 and 0), WebKit, the Tauri shell, and the Rust workspace's clippy and tests. No Rust and no product source changed.

## 6. Decisions

None. D710 to D719 were set aside for this slice and are unused.
