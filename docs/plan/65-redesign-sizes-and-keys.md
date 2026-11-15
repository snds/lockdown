# 65: sizes, long content and keys

_Slice: `found-4` of the 2026-11 redesign, on `claude/keen-ritchie-q2hgb2`, built 2026-11-14. Brief X6 of the undrawn-areas brief (keys, long content, short screens and folds) and the chrome of pushed phone screens from its §3.3. It builds the shell's part and writes the rules every lane applies to its own screens._

## For future agent

- **TL;DR:** `app/keys.ts` is the one table of keys; `?` opens it as a list (`app/keys-dialog.tsx`). A row with a menu takes `useRowMenuKeys()`. A pushed screen on a phone hands its buttons to the shell with `useDetailBottomRow({ start, end })` and `useDetailTopBar(actions)`. Short screens and folds are the frame's: nothing in the app picks them.
- **Do:**
  - Add a key to `shortcuts()` in `keys.ts` in the same change that makes it work, so the list and the keys never drift.
  - Give every row that has a menu the row-menu keys, each prop by name: `onKeyDown={menu.onKeyDown} onContextMenu={menu.onContextMenu} aria-keyshortcuts={menu.keyshortcuts}`.
  - On a pushed screen, call `useDetailBottomRow` and `useDetailTopBar`, and draw your own buttons for the same actions only when they return `false` (beside the list at 1440, or while the keyboard is up).
  - Apply the long-content table (§3) to your own rows, titles and Field Rows.
- **Do not:**
  - Read the window's height, a media query or the keyboard yourself. `useAppFrame()` says `phone`, `layout === "rail"`, `split`, `foldBook` and `keyboardOpen`.
  - Spread the hook's return onto a unicron part: the ratchet counts a spread.
  - Put a form's primary only in the bottom row. It goes in the top bar, because the keyboard hides the bottom row.
- **Read with:** brief X6 and §3.3 (workspace, `07-projects/29-unicron/impl-2026-11/app-lockdown-undrawn.md`), cross-app decisions §6, §10 and §14, doc 64 (places and the three shells).

## 1. Keys

`app/keys.ts`. `shortcuts({ mac, quickWindow })` returns the list in the order it is shown; `useAppKeys` runs the shell's three; `useRowMenuKeys` gives a row its menu keys.

| Keys (Mac / elsewhere) | Does | Where it is handled |
|---|---|---|
| ⌘K / Ctrl+K | Quick access: items, places and Lock | `features/palette/quick-palette.tsx` (unchanged) |
| The system shortcut, as the core names it | The quick-access window, from any app | the Tauri shell (unchanged); the list reads `hotkeyStatus()` |
| ⌘⇧L / Ctrl+Shift+L | Fill in the browser, with the extension | the extension's `fill-login` command (unchanged) |
| `/` | Search | `useAppKeys` (was in `shell.tsx`) |
| ⌘, / Ctrl+, | Settings | `useAppKeys` (was in `shell.tsx`) |
| ⌘\ / Ctrl+\ | Collapse or show the sidebar | unicron's `AppFrameSidebar` (unchanged) |
| `?` (new) | The keys list | `useAppKeys` |
| ↑ ↓, Enter | Move in a list; open the row | each list (unchanged) |
| Menu, Shift+F10 (new) | The row's menu | `useRowMenuKeys`, attached by each lane |
| Esc | Close the top layer; in Search, clear the field, then leave | each layer (unicron), Search (unchanged) |

- **Not in a field, not in a layer.** `/`, `?` and ⌘, do nothing while focus is in a text field or inside an open dialog, sheet or menu. Before this slice `/` and ⌘, did act inside a layer, which could move the page under an open dialog. That is the one change to keys that already existed.
- **The keys list.** An `AdaptiveDialog`, size `form` (640), overline `KEYS`, title "Keyboard shortcuts", one `FieldRow` per key with the key as a `Kbd` in its label and what it does as the value. No footer and no primary: it asks nothing. It reads the system shortcut from the core each time it opens. When that shortcut is off, or the computer would not give it, the row says so in its description line. The web vault has no quick-access window, so its list has nine rows, not ten. On a Mac the list writes the Mac's symbols; elsewhere the keys are spelled out (`Ctrl+Shift+L`).
- **Row menus.** `useRowMenuKeys(open)` returns `onKeyDown`, `onContextMenu` and `keyshortcuts`. The Menu key and Shift+F10 open the menu from the row (`point: null`, so it opens at the row's overflow button). A right click opens it at the pointer. Some browsers send a `contextmenu` event after the key; one press opens one menu. A key another part already handled is left alone.
- **Escape never confirms.** Every layer in the app closes on Escape as Cancel. The test proves it on the gate, the one layer whose yes the core waits on: with focus on Confirm, or with a password typed, Escape cancels and the call is not run again.

## 2. Pushed screens on a phone

`app/detail-bottom-row.tsx`, `app/top-row.tsx`, `app/bottom-row.tsx`, `components/master-detail.tsx`.

A screen is **pushed** on a phone when an item or a Settings section is open over its list, or the place is Settings or Sharing (`usePushed()`). A pushed screen has no tab bar.

| Part | What it holds | Who fills it |
|---|---|---|
| Top bar | Back; the small centred title once the large title has scrolled under it (the frame does this); up to two actions; on Settings, Lock after them | the screen, with `useDetailTopBar(actions)` |
| Detail bottom row | Up to two neutral `FloatButton`s at the start; one primary, or `FloatButton extended` when the yes shares, grants or approves, at the end | the screen, with `useDetailBottomRow({ start, end })` |
| Status strip | Above the detail bottom row, as above the tab bar | the frame |

- **The screen hands its buttons up.** The chrome is drawn by the shell, outside the screen's own part of the tree. So the buttons are made in the screen and drawn in the shell; a button reads its context where it is drawn. Make them with `useMemo` so they are not handed up on every draw.
- **Each hook returns whether its part is drawn.** At 1440 the list stands beside the detail and nothing is pushed: the hooks return `false`, and the screen keeps its own buttons. The same is true while the keyboard is up.
- **A screen with no primary has no bottom row.** It calls the hook with nothing, or not at all.
- **The keyboard.** While the on-screen keyboard is up the tab bar's row hides (the frame), and so does the detail bottom row (this slice: unicron keeps a `detail` row on the keyboard for a composer, and Lockdown has none). A form therefore keeps its primary in its top bar, as brief 4.0's Form layout says.

## 3. Long content: the rule each lane applies

| Thing | Rule | How |
|---|---|---|
| A row's title | One line, cut at the end | the row part's own cut; `TruncatedText` where the title is the app's own node |
| A site, an address, a host, a key id in a mono line | One line, cut in the middle; the whole value in a tooltip | `TruncatedText cut="middle"` |
| The detail's title | Card (drawn with card art): `type-title`. Every other kind: on desktop `type-display`, two lines, then cut at the end; on a phone `type-display-compact` on one line, and `type-title` up to two lines when it will not fit one | `TruncatedText lines={2}` in the detail's head |
| A Field Row's value | Never wraps inside a word. Longer than its column, cut in the middle; COPY copies all of it | a string value does this itself (`FieldRowValue truncate="middle"`); a sentence uses `truncate="wrap"` |
| A note | Wraps in a reading column of at most 648 px; the pane scrolls | the detail lane |
| A list over 200 rows | Draws only the rows in view; letter headers stay at the top while their rows scroll | the vault lane; `desktop-vault-long` (300 rows) shows it |
| More than three of something on an item (websites, passkeys, files) | Three, then a text action `SHOW ALL 12` | the detail lane |
| A chip row | Scrolls sideways; more than 12 chips never wrap | `FilterChipGroup scroll` |

**In the shell.** The shell's own words are the sidebar's names (`NavItem`), the breadcrumb (`AppFrameStatusLine` crumbs), the page head and the top bar's title. Each is a unicron part that cuts long words at the end. The names and the crumbs take a `string`, so the shell cannot put a `TruncatedText` in them, and today a cut name or crumb has no way to show its whole (gap 1). The shell draws no host, so the middle cut does not arise in it.

## 4. Short screens and folds

- **A phone on its side (844 × 390)** is a short screen. The frame draws the phone's content with the bars in a column at the trailing edge: Search, the tab bar turned vertical, and create; on a pushed screen, the detail bottom row's buttons. The top row's buttons move to the top of that column; a pushed screen's top bar stays at the top of the content. Quick create opens beside the create button. Nothing in the app picks this: `useAppFrame().layout === "rail"` says it.
- **A fold held like a book**, each half at least 320 px: list and detail split at the hinge whatever the width, each in one half, and "Nothing open." stands in the far half until a row is chosen. `MasterDetail` is on the frame's own columns, so it follows.
- **The harness.** `pnpm shots --sizes 844x390` takes any desktop-class state at that size. The four states of cross-app §14 are `desktop-vault-filled`, `desktop-item-login-full`, `desktop-editor-new-login` and `desktop-unlock-locked`.

## 5. The harness

| State | What |
|---|---|
| `shell-keys` (new) | `?` from the Items list: the keys list. Desktop widths only, `built` as `shell--keys` (undrawn). A phone has no keys, so it has no phone picture. |
| The 844 × 390 pictures | Of the four states above, with `--sizes 844x390 --only …`. |

`states/steps/shell.ts` gains `openKeys()`.

## 6. Tests

| File | What it proves |
|---|---|
| `app/keys.test.tsx` | At 1440: `/` opens Search and is a letter in a field; ⌘, and Ctrl+, open Settings; `?` opens the keys list with KEYS over its title, no button but Close, and no second layer on a second `?`; the list names every key, as this computer writes them (and the Mac's in `shortcuts`); the system shortcut's row says when it is off and is left out where there is none; `?` in a field is a letter; Escape closes the list and gives focus back, and the keys wait while it is open. Escape never confirms: the gate's confirm and its password, Escape is Cancel. The Menu key, Shift+F10 and a right click open a row's menu, F10 alone does not, and the browser's `contextmenu` after the key opens no second one. |
| `app/detail-bottom-row.test.tsx` | `useDetailBottomRow` draws at 390 (the buttons at the foot, no tab bar, the top bar's actions after Back) and not at 1440 (the screen keeps its own); the row goes with the screen. While the keyboard is up the bottom row hides, on a section's root and on a pushed screen, and the screen's own buttons stand in. At 844 × 390 the tab bar is vertical in the trailing column with Search and create, and a pushed screen's row is in that column. On a book fold at 717 px list and detail split at the hinge. |

`shell.test.tsx` is unchanged and still passes: the keys it pressed (`/`, ⌘,) work as before.

## 7. Gaps (unicron)

1. **`NavItem` and the status line's crumbs cut a long name with no way to see it.** Both take the label as a `string` and cut it with CSS; there is no tooltip and no way to pass a `TruncatedText`. Screens: the sidebar (a long vault, folder or tag name), the breadcrumb (`VAULTS // A LONG FAMILY VAULT NAME // FINANCE`). Ask: draw the label with `TruncatedText` inside the part, or take a node.
2. **`AppFrameBottomRow` keeps a `detail` row up while the keyboard is open.** That is right for a composer and wrong for float buttons over a form; Lockdown hides it itself. Ask: a prop for whether the detail row stays on the keyboard (default: only when it has `fill`).
3. **No hand-up for a pushed screen's chrome.** Each app has to carry its own context from the screen to the frame's rows (this slice's `detail-bottom-row.tsx`). Ask: let `AppFrameDetail` take `topBarActions` and `bottomRow` and give them to the frame's rows.
4. **`KbdGroup` is a `kbd` inside which each `Kbd` is another `kbd`.** It reads right, but a test or a script counting keys finds the group too. Noted, not asked.

## 8. Proposed decisions

| Id | Decision | Status |
|---|---|---|
| D730 | **`?` opens the keys list, in every width that has keys.** An `AdaptiveDialog` (form 640), KEYS over "Keyboard shortcuts", a Field Row per key with the key as a `Kbd`, no primary. The new keys need no decision (brief X6). Plain version: press `?` to see every key. | Proposed |
| D731 | **One table of keys.** `app/keys.ts` lists every key the app answers, in the words and symbols of this computer, and the keys list reads it; a new key is added there with the code that runs it. Plain version: the list of keys can't be out of date. | Proposed |
| D732 | **The shell's keys wait while a layer is open or a field has focus.** `/`, `?` and ⌘, do nothing inside a dialog, sheet or menu, or while typing. This changes `/` and ⌘, inside a layer, which before moved the page under it. Plain version: a key never changes the page behind the box you are looking at. | Proposed |
| D733 | **Every row with a menu opens it with the Menu key, Shift+F10 and a right click,** the same menu as its overflow button (cross-app §10). Plain version: a row's menu is never only for the mouse. | Proposed |
| D734 | **Escape is Cancel, everywhere.** It closes the top layer; it never says yes, not even with focus on the yes. Plain version: Escape is always safe. | Proposed |
| D735 | **A pushed phone screen's chrome is handed up by the screen.** A top bar of back, the small title and up to two actions; a detail bottom row of up to two neutral buttons and one primary, only when the screen has a primary; the status strip above it. Beside the list (1440) the screen keeps its own buttons. Plain version: on a phone, an open item's buttons sit where a thumb reaches. | Proposed |
| D736 | **While the on-screen keyboard is up, no bottom row shows; a form's primary is in its top bar.** Plain version: the keyboard never covers the button you need. | Proposed |
| D737 | **Short screens and folds are the shell's.** At 844 × 390 the bars stand in a trailing column with the tab bar vertical; on a fold held like a book, list and detail split at the hinge. Lockdown inherits both from unicron (cross-app §14) and writes no layout of its own for them. Plain version: a phone on its side and a folding phone get a layout made for them. | Proposed |
| D738 | **A list over 200 rows draws only the rows in view,** with its letter headers kept at the top while their rows scroll. Below 200 every row is drawn, so find-in-page and a screen reader's list of rows work as today. Plain version: a vault of thousands of items scrolls as smoothly as one of ten. | Proposed |
| D739 | **Long words are cut, never wrapped mid-word, and never lost.** A row title is one line cut at the end; a site, an address or a host is cut in the middle; a Field Row's value is cut in the middle and COPY takes all of it; past three of something an item shows SHOW ALL n; a chip row scrolls. A cut text shows its whole in a tooltip. Plain version: long names stay readable and nothing is hidden for good. | Proposed |
