# 72: row swipes on a phone: Lockdown's actions and their safety rules

_Slice: `vault-4-swipes` of the 2026-11 redesign, on `wt/lockdown-vault` (for `claude/keen-ritchie-q2hgb2`), built 2026-11-15. The drawn frame States / 01 Swipe actions (`51:8213`; stone `53:11782`); brief V1's swipe table; rules.md 7.1 and 7.3; unicron contract 6.4. The slice text named this doc 71 with D790 to D799; vault-3 took both (`71-redesign-vaults.md`, D790 to D798), so this is doc 72 with D800 to D809. The vault lane runs one slot late; the integrator settles the numbers at the fold._

Status: built. A phone's item rows swipe; nothing else in the list changed.

## For future agent

- **TL;DR:** on a phone (unicron's `phone` and `rail` layouts, so a phone on its side too) each item row is unicron's `SwipeRow` around the same `ListRow`. `swipeOffer(row)` in `features/organization/row-menu.tsx` names the tiles from brief V1's table; `useRowSwipe` turns them into `SwipeRowAction`s that make the same core calls, with the same toasts, as the row's menu. `ItemRow` (`features/vault/item-row.tsx`) takes a `swipe` prop (the list's handlers); `VaultView` passes it only on a phone. Copies go through `useQuickCopy` / `useCodeCopy` (`features/vault/quick-copy.ts`), which call the core through the gate.
- **Read with:** brief V1 (the row, the row menu, the swipe table), X2 (the copy toast), doc 70 (the lists), doc 68 (the gate's window route), unicron `swipe-row.tsx`.
- **Do not:** run anything on a swipe that is not reversible; let a swipe call `reveal` or hold a value; add a swipe where the row has no menu path for the same action; put a swipe on the needs-you strip (it is not a list row, and unicron's `Accessory` has no swipe away).

## 1. What each row offers (brief V1)

| Row | Left swipe, from the row outwards | Full swipe | Right swipe |
|---|---|---|---|
| A live item | `MORE` (neutral), `CODE` when `codes > 0` (neutral), the quick copy named by `copy.label` (`PASSWORD`, `CARD NUMBER`, `WI-FI PASSWORD`, primary, outermost) | Copies | `FAVORITE`, or `UNFAVORITE` on a favorite |
| A code item (its copy is the code) | `MORE`, `CODE` once | Copies the code | The same |
| Nothing to copy (`copy` is null) | `MORE` only | None: the row opens and stops | The same |
| A crown jewel, an "Ask each time" item | The same tiles | Asks: the gate opens before any value moves | The same |
| Archived | `MORE`, `UNARCHIVE` | Unarchives | None |
| Recently deleted | `MORE`, `RESTORE` (never a copy) | Restores | None |
| Shared to view (no menu) | Its copies only (no `MORE`) | Copies | None |
| Shared to fill only | No swipe | | |

- **MORE** opens the row's own ⋯ menu (the same menu as its right click), so on a phone it is the way to Folder and tags…, Move to vault, Archive, History and Move to Recently deleted.
- **A full swipe** runs the outermost tile only when it is declared reversible: a copy (the clipboard clears itself), unarchive, restore, favorite. `MORE` is not, so a row with nothing to copy has no full swipe.
- **The leading swipe** starts 24 px clear of the screen's left edge (unicron's `--c-swipe-row-edge`), so the system's back gesture wins there (rule 7.3.4).
- **Reduced motion:** the row snaps (unicron drops the spring-settle motion).
- **Desktop and medium:** no swipe. The rows keep their ⋯ menu, as doc 70 built them.

## 2. Safety (rules.md 7.3, contract 6.4)

- **A swipe never releases a value by itself.** The copy is the same call as the palette's and the detail's: `copy_secret` or `copy_code` through `gate.run`. The first call carries no proof; for a crown jewel or an "Ask each time" item the core refuses it (`approval_needed`) and the gate asks in the app window. Nothing reaches the clipboard until the person says yes. Escape backs out and the toast says "Nothing was copied". The value never reaches the webview (the shell copies on the Rust side).
- **CODE** finds its field in the core's codes list (as the detail does) and keeps only the field id.
- **No approval is a swipe action** in Lockdown, so no tile is `confirm`. The sign-in strip is unicron's `Accessory`, not a row: it cannot be swiped away (found-6's `needs-you.test.tsx`, unchanged and green).
- **Every swipe action is also in the row's ⋯ menu** (unicron's `ListRow` lists them at the top of its menu), on the Menu key and Shift+F10 (WCAG 2.5.7). The row menu then leaves out the items the swipe repeats (Add to favorites, Unarchive, Restore), so none shows twice. The detail keeps COPY.

## 3. Words

- Tiles: unicron sets the capitals; the menu shows the same words in sentence case: Favorite, Unfavorite, More, Code, Password, Card number, Wi-Fi password, Unarchive, Restore.
- Toasts (X2), the same as the row menu's and the palette's: "Copied the password for Bluefin Bank, clears in 30 s", "Copied the code for …", "Added … to favorites", "Removed … from favorites", "Unarchived …", "Restored …", "Nothing was copied". A failed change is the list's critical compact Banner, as the row menu's is.

## 4. Pictures

- `desktop-vault-swipe-trailing` (built `items--swipe-actions`, drawn) and `desktop-vault-swipe-leading` (built `items--swipe-favorite`, drawn), 390 wide only: a phone-only gesture, so there is no desktop picture. Both scroll the list first, as States / 01 draws the scrolled chrome. The frame draws both rows open at once; the app closes an open row when another is pressed, so each is its own state.
- `steps/vault.ts` gains `swipeRow(title, side)`: pointer events on the row's `SwipeRow`, far enough to open its tiles and short of a full swipe.
- `desktop-vault-reload-failed` picks `Favorite` from the phone's menu where a desktop's says `Add to favorites`.

## 5. Tests, and what is not proven

- `features/vault/swipe.test.tsx` (new, 12), in the real shell at 390 px unless named: `swipeOffer` for every kind of row; MORE, CODE and PASSWORD with the primary outermost, FAVORITE on the right; a full swipe copies through the core with X2's toast; **a gated item never yields a value from a swipe alone** (the core's refusal, the gate's question, nothing copied, Escape copies nothing, a yes copies); **an archived row's full swipe unarchives**, with no leading tile; **a row in Recently deleted offers no copy** (MORE and RESTORE; a full swipe restores; neither copy call runs); **no full swipe on a row with nothing to copy** (it stops open at 80 px and runs nothing); **every swipe action is in the overflow menu**, once, and MORE opens it; Restore once in Recently deleted's menu; a fill-only share does not swipe; **reduced motion** snaps; at 1440 the rows do not swipe.
- `features/organization/row-menu.test.tsx`: at 390 px the menu's first rows are the swipe's (Favorite, More) in place of Add to favorites; the desktop and medium widths are unchanged.
- `app/needs-you.test.tsx` (found-6's): the strip cannot be swiped away. Unchanged, green.
- Not proven: a real finger on a real phone (the capture and jsdom use synthetic pointer events), haptics (unicron has none), a screen reader's announcement of the tiles (they are `aria-hidden`; the menu is the path).

## 6. Gaps

- **Medium width (768) does not swipe.** The slice and the brief say phone rows; a touch tablet at medium width has the ⋯ menu only.
- **At 390 px the three tiles open past the full-swipe line.** The tiles take 240 px and unicron's full swipe is 60% of the row (234 px), so a drag that shows all three tiles fully and a little more copies. The copy is reversible and a gated item still asks, so this is safe, but it may surprise. See unicron gap 3.
- **The drawn frame opens two rows at once**; the app opens one at a time (two states).

### unicron host-contract gaps

1. `SwipeRow` lists every action in the row's menu, `MORE` included, so a phone row's menu starts with a "More" that opens the same menu again. Ask: a way to mark an action as the row's own menu (`menu: false`, or a `more` kind) that the menu leaves out. Screen: any phone item row's ⋯ menu.
2. `SwipeRowAction` has one `label` for the tile and the menu, so the menu says "Password" where "Copy password" would read better. Ask: an optional `menuLabel`. Screen: a phone item row's ⋯ menu.
3. The full-swipe line (60% of the row) sits inside the open position when a row has three 80 px tiles on a 390 px phone. Ask: put the line at least one tile past the open width. Screen: States / 01's trailing swipe.
4. `SwipeRow` has no `render` prop, so in a list it needs a plain `<li>` around it (the `ListRow` inside renders a `div`). Screen: every phone list.
5. `SwipeRow` has no way to open its row's menu from an action, so `MORE` dispatches a `contextmenu` event on the row (what a right click does). Ask: an `openMenu()` on the row's context. Screen: the MORE tile.

## 7. Proposed decisions

| # | Decision | Status |
|---|---|---|
| D800 | **A phone's item rows swipe**, per brief V1's table: MORE, CODE when the item has codes, then the quick copy, outermost and primary; FAVORITE or UNFAVORITE on the right. Archived rows offer UNARCHIVE and rows in Recently deleted RESTORE, with no right swipe; a fill-only share does not swipe. Plain version: on a phone, slide a row left to copy its password or code, right to star it. | Proposed; **flagged for Sean** (States / 01; undrawn §6.2 row 142; Q8 the row menu's home) |
| D801 | **A full swipe runs only the outermost tile, and only a reversible one**: a copy, unarchive, restore. A row with nothing to copy has no full swipe. Plain version: a long slide does the obvious safe thing, or nothing. | Proposed (rules.md 7.3.3, contract 6.4) |
| D802 | **A swipe copy goes through the core's gate like every copy**: a crown jewel or an "Ask each time" item asks in the app window first, and backing out copies nothing. Plain version: a swipe cannot get round a protected item's question. | Proposed (app-lockdown.md slice 5; D101) |
| D803 | **MORE opens the row's own menu**, the one its ⋯ and right click open, and the menu lists every swipe action at its top; the items the swipe repeats are not listed twice. Plain version: everything a swipe does is also in the row's menu. | Proposed; **flagged for Sean** (WCAG 2.5.7; brief V1 row menu; unicron gap 1) |
| D804 | **CODE copies the code valid now** through `copy_code`, finding its field in the core's codes list; a code item has one CODE tile, not two. Plain version: the code is one slide away. | Proposed |
| D805 | **The tiles' words come from the item**: `copy.label` in capitals (PASSWORD, CARD NUMBER, WI-FI PASSWORD, RECOVERY PHRASE). Plain version: the tile says what it copies. | Proposed; **flagged for Sean** (brief V1; a long label may wrap in its 80 px tile) |
| D806 | **A phone on its side swipes too** (unicron's `rail` layout is a phone); medium and wider widths do not. Plain version: swipes are for phones. | Proposed |
| D807 | **The toasts are the row menu's and the palette's** (X2): "Copied the password for Bluefin Bank, clears in 30 s", "Added … to favorites", "Unarchived …", "Restored …". Plain version: the same action says the same thing wherever it starts. | Proposed (cross-app §12, X2) |

D808 and D809 are free.
