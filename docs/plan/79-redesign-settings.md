# 79: Settings on the SettingsFrame, This Mac, About and the developer tools

_Slice: `screens-4` of the 2026-11 redesign, on `wt/lockdown-screens` (from `claude/keen-ritchie-q2hgb2`), built 2026-11-15. Drawn frames Mobile / 16 Settings (`40:2401`) and Desktop / 12 Settings (`41:3911`); briefs T0, T1 and T10 of the undrawn-areas brief; cross-app decisions §3, §5, §10, §12, §13. Decisions D870 to D879._

## For future agent

- **TL;DR:** Settings is unicron's `SettingsFrame` (`features/settings/settings-places.tsx`) over the places table's 14 sections in four groups (`THIS MAC` or `THIS PC` or `THIS BROWSER`, `ACCOUNT`, `VAULT`, `LOCKDOWN`), with "Find a setting" from ten sections up, a mark on Recovery while the kit is untested and on About while an update is ready, and `SETTINGS // THIS MAC` as the breadcrumb when the first section shows unasked. This Mac (`features/settings/this-device.tsx`) is the drawn page in one column: SECURITY, PASSKEYS, DEVICES NN, ACCOUNT, all Field Rows, Toggle Rows and Radio Rows. Every setting saves at once and silently through `useSettingSave` (`setting-rows.tsx`); a save the core refuses leaves the control as it was, with one critical line under it. Five or more choices sit behind CHANGE (`ChangeSetting`, an `AdaptiveMenu`). About (`features/updates/about-section.tsx`) has THIS APP and SERVER. The demo's notice is a compact info `Banner`; the developer reset is a `ConfirmDialog` whose typed word arms the yes.
- **Do:**
  - Add a setting as a row of its section, and add its name to `SETTINGS_INDEX` (`settings-index.ts`) so "Find a setting" finds it.
  - Save through `useSettingSave(row, change)`; show `errorOf(row)` with `SaveError` under the row. Never toast a save.
  - Keep the other slices' section components and their props: `DevicesView` (edges-2), `ClockSetting` (edges-5), the import, export and backups parts (screens-8), Status and Server (screens-7), Recovery (screens-6), Browsers, Quick access, Email aliases and Security checks (screens-5). This slice only took the old card wrappers off them.
- **Do not:**
  - Offer a switch for Touch ID or Windows Hello until the core can turn quick unlock on: there is no such call (§2).
  - Offer the system-wide passkey provider (D122 defers it). The switch that exists is "Offer Lockdown's passkeys in browsers" (`browser_passkeys`).
  - Show a kit date or PRINT: a kit is shown once and never kept (D69).
- **Read with:** briefs T0, T1, T10 (workspace `07-projects/29-unicron/impl-2026-11/app-lockdown-undrawn.md`), doc 64 (places), doc 67 (messages, the update mark), doc 60 (auto-update), doc 40 (developer tools).

## 1. The frame (brief T0)

| Width | What shows |
|---|---|
| Expanded (1024 and up) | The page head "Settings", "Find a setting", then the section list (204) under four overlines, beside one content column up to 720 with the section head (title and one line). This Mac shows first; the breadcrumb says `SETTINGS // THIS MAC` (`useCrumb`). The status line's end is the running version (`LOCKDOWN // 1.8.0`, from the updater) |
| Medium (600 to 1023) | As on a phone: This Mac's rows first, then Find a setting, `ALL SETTINGS 13` and the list; a section pushes, and Back in the status line returns. The brief says "the list, then the section pushed"; the foundation's frozen shell test (`app/shell.test.tsx`, found-3) holds this medium form, so it stays until that test changes (question 8 in the hand-off) |
| Phone (under 600) | The top bar from found-3 (back, "Settings", Lock). The This Mac rows first (all four groups, so Account stays reachable), then "Find a setting", `ALL SETTINGS 13` and the 13 other sections under their overlines; each pushes |

- **Marks.** Recovery: a square while the kit is made but not tested, or not made (`status.recovery`). About: the update mark (`updateMark`, doc 67). Devices "when a device is new": not built, because the core has no "new device" fact (gap 1).
- **Find a setting.** A `SearchField` over the list. Each word typed must start a word of a setting's name, or of a section's name or line; the app's own name is left out of the match, since it is in many lines. What it finds shows under the field as `FOUND NN` and Nav Items (the setting, its section as the second line); a press goes to the section and empties the field. Nothing found: "No setting for {words}." The frame hides the field below ten sections, so the web vault (four) has none.
- **The web vault** has four sections: This browser, Appearance, Devices, Server (its "This server" part).

## 2. This Mac (brief T1)

| Group | Row | Value | Control |
|---|---|---|---|
| SECURITY | AUTO-LOCK | "After 5 minutes" … "Never" | CHANGE: 1, 5, 10, 30 minutes, 1 hour, 4 hours, Never (no Never on the web) |
| | Lock when the window is out of view | "After 5 minutes." | switch |
| | TOUCH ID or WINDOWS HELLO | "Not yet" with the core's `why`, or "Available" | none: read only (the core has no call to turn it on) |
| | APPROVALS | Touch ID, Master password or This window; today's sentence as the line; "Checking…" first | none |
| | CLIPBOARD | "Clears after 30 s"; "Cleared only if the clipboard still holds what Lockdown copied." | CHANGE: 10, 20, 30, 45, 60, 90 seconds |
| | REVEAL | Two Radio Rows with D97's sentences | radios |
| | YOUR NAME | The name on the lock screen; "Shown when Lockdown is locked. It stays on this computer." | CHANGE: a name prompt with its own SAVE NAME |
| | (caption) | "The vault also locks when this computer wakes from sleep." | |
| PASSKEYS | Offer Lockdown's passkeys in browsers | "A paired browser offers the passkeys in Lockdown and can save new ones here." | switch |
| DEVICES NN | The first three live devices, this one first | This one "Now"; the others "Added {row date}" (the core keeps no last-seen time) | REVOKE behind a Question (focus on Cancel), then the gate (D101); ALL opens Devices |
| ACCOUNT | RECOVERY KIT | Tested, Made, not tested, or None yet | OPEN (Recovery) |
| | OTHER SESSIONS | "n others", "None", "Checking…" | END ALL behind a Question: "End all other sessions?", "{n} devices must log in again with the password. This device stays signed in." |

A local account shows the kit row only. Other sessions come from the sessions client directly (`useLiveClient`), not from edges' `SessionsSection`.

**The web vault's This browser:** Auto-lock without Never, "Lock when this tab is out of view", Clipboard with the web's sentence, Reveal, then THIS WEB VAULT: SIGNED IN AS (the login; the server's host as its line), the trust note (D180) and SIGN OUT OF THIS TAB (secondary).

**Appearance** (screens-5 owns the section; the rows were inline here): the Theme row from found-1, and Density as two chips (cross-app §5: up to four short choices are chips).

## 3. About (brief T10, cross-app §3)

THIS APP: unicron's `UpdateStatusRow` and `AutoCheckSwitch` (version, channel, one status line, CHECK FOR UPDATES, RESTART TO UPDATE with "Restarting locks Lockdown…", RELOAD APP, "Get it from GitHub" where this copy cannot replace itself; "Check for updates automatically"). SERVER: the account's server host with OPEN (Settings → Server) and "Your server checks for its own updates. Its operator updates it from its admin page." The app has no reading of the server's version (doc 61 keeps updates on the admin page), so the row says where to look rather than inventing a status. A local vault has no SERVER group. The web vault has no About.

## 4. Developer tools (brief T10; development builds only)

- **The demo:** a compact info `Banner` in a box fixed at the top of the window, "You're exploring the demo. Nothing here is real." and LEAVE THE DEMO, no close button. Unicron's frame is fixed to the window, so `devtools.css` starts it below the notice; before this slice the notice covered the phone's top row and the desktop's lockup. On a phone it sits above the top row, not under it as the brief says (gap 2).
- **Reset:** `ConfirmDialog`, destructive, question size, "Reset Lockdown on this computer?", the plan as three lists (Erased, Your identity, Kept) and a Text Field "Type reset to confirm"; the yes ERASE AND RESTART is off with "Type reset to turn this on." until the word is typed. Focus starts on Cancel. On success the yes stays busy and the body says "Erased. Lockdown is restarting…"; nothing can close it. A refusal shows in the shell's words and keeps the question.
- **The developer menu** state draws the menu's items with unicron's roles (the OS draws the real one).
- `desktop-bare` (`LOCKDOWN_DEVTOOLS=off`, `packaging/devtools/check.sh absent dist`) still passes.

## 5. Tests and what is not proven

- `features/settings/settings-view.test.tsx` (new, 15): the four groups and drawn values; quick unlock read only with the core's reason; Auto-lock's seven choices with the current one marked and a silent save; each switch and Reveal save at once; a refused save leaves the value and the switch as they were with one critical line each; three devices, this one first, REVOKE behind a Question with focus on Cancel; the kit's words and OPEN; END ALL behind a Question; a local account; the name prompt; the section list's groups and marks; Find a setting (found, nothing found, word starts); Density chips.
- `features/updates/about-section.test.tsx`: two new tests (THIS APP and SERVER with OPEN; no SERVER for a local vault). The other ten unchanged.
- `devtools/reset-dialog.test.tsx`: the yes is now `aria-disabled` with its reason (it stays focusable, contract 5.1b #37) instead of `disabled`; the test checks that, that a press does nothing, and that focus starts on Cancel. Not weaker.
- `apps/web/src/features/web-settings-view.test.tsx`: one new test for This browser.
- Capture states: `desktop-settings` (built, drawn `settings--this-mac`), `desktop-settings-find` (`settings--find`), `desktop-settings-section-list` (phone only, `settings--section-list`), `desktop-settings-about` (`settings--about`), `desktop-settings-this-mac` (the lower half), `desktop-settings-theme-{system,stone,graphite}`, `desktop-devtools-demo` (`devtools--demo`), `desktop-devtools-reset` (`devtools--reset`). Ids kept: `desktop-settings`, `desktop-settings-compact`, `web-settings`, every `desktop-devtools-*`.
- Not proven: a real Mac (the capture says Mac through `navigator.platform`), real quick unlock, the real updater, a screen reader on the menus and the reset's reason line.

## 6. Gaps

1. No "new device" fact in the core, so Devices has no mark (brief T0). Wish: a device's first sync time, or "enrolled since you last looked".
2. Unicron: `AppFrame` has no slot for an app-wide notice (the demo's) under the phone's top row or above the status line; the app pins it above the frame.
3. Unicron: `SettingsFrame` has no filtered-list mode for "Find a setting"; filtering `sections` would hide the field below ten. The results show under the field instead, over the full list.
4. Unicron: `SettingsFrame` shows `search` only from ten sections, so a phone with fewer sections cannot draw `ALL SETTINGS NN` in that slot (the web vault's phone list has no header).
5. Unicron: a long `ConfirmDialog` body scrolls with its header; the reset's plan pushes the title off a 900 px window once the field has focus.
6. Core: no call turns quick unlock on, so the drawn Touch ID switch is a read-only row (app spec "no data" table).
7. The fixture's `status().recovery` is a sentence ("kit made, drilled 5 months ago"), not the core's `none`, `made` or `drilled`; Settings' states answer the core's word themselves (`kitState` in `fixture/lane-screens.ts`). Foundation's to fix in `fixture/story.ts`.

## 7. Unicron host-contract gaps

Gaps 2 to 5 above. Also: `UpdateStatusRow`'s actions are text buttons, so RESTART TO UPDATE is not a solid button even when it is the one thing to do (asked: a `primary` for the ready state).

## 8. Proposed decisions

| # | Decision | Status |
|---|---|---|
| D870 | **Settings is the shared `SettingsFrame` with 14 sections in four groups, and the drawn page is its first section, This Mac, in one column** (Q10; cross-app §5). Plain version: a list of sections on the left; your drawn page is the first one, one group under another. | Proposed; **flagged for Sean** (Q10, 6.3 Desktop / 12) |
| D871 | **Every setting saves at once and says nothing; a refused save puts the old value back with one critical line** (cross-app §5). Name changes, the server address and other typed text keep a Save beside the field. Today's "Name saved." and "Server saved." toasts go. Plain version: change it and it is changed; if it could not be, the row says why. | Proposed; **flagged for Sean** (cross-app §5) |
| D872 | **Five or more choices sit behind CHANGE, a menu on a desktop and a sheet on a phone; Reveal is two Radio Rows with D97's sentences; Density is two chips** (6.3: native selects and toggle groups go). Plain version: the row shows what is set, and CHANGE opens the list. | Proposed; **flagged for Sean** (cross-app §5, T1) |
| D873 | **The drawn "Lockdown as passkey provider" is built as "Offer Lockdown's passkeys in browsers"**, the switch that exists; the system provider stays deferred (D122). Plain version: the switch says what it really does. | Proposed; **flagged for Sean** (D122 wording, 6.3 Mobile / 16) |
| D874 | **Touch ID is a read-only row with the core's reason until quick unlock works**; no switch that does nothing. Plain version: it tells you why it is not on yet. | Proposed; **flagged for Sean** (app spec "no data") |
| D875 | **Devices on This Mac: the first three, this one first, "Now" for this one and "Added {date}" for the others, REVOKE behind a Question and the gate, ALL to Devices**; the drawn "Remove" says REVOKE and the drawn last-seen times are added dates, since the core keeps none. Plain version: the same revoke as Devices, from here. | Proposed; **flagged for Sean** (6.3 Mobile / 16) |
| D876 | **ACCOUNT: RECOVERY KIT with its state and OPEN (no date, no Print), and OTHER SESSIONS with END ALL behind a Question** in place of "Emergency kit … Print" and "Sign out everywhere". Plain version: the kit is never kept, so it can't be reprinted; ending sessions keeps this device signed in, so it says "other". | Proposed; **flagged for Sean** (6.3, D69) |
| D877 | **About has two groups, THIS APP and SERVER; a ready update is a mark on About and on the way into Settings, nothing more** (Q11, the About side; cross-app §3). SERVER names the server and sends you to Settings → Server, since the server updates from its own admin page. Plain version: updates live in About. | Proposed; **flagged for Sean** (Q11; cross-app question 3) |
| D878 | **"Find a setting" over the section list from ten sections; the phone lists ALL SETTINGS 13 under the drawn rows; Recovery carries a mark while the kit is untested.** Plain version: type a setting's name to jump to it; Settings tells you when the kit needs a test. | Proposed; **flagged for Sean** (T0) |
| D879 | **The demo notice is a compact info Banner with LEAVE THE DEMO, and the developer reset is a Question whose typed word arms ERASE AND RESTART, focus on Cancel** (development builds only). Plain version: the same words, in the new parts. | Proposed (development builds only; not flagged) |
