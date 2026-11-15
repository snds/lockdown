# 70: the item lists: rows, views, kinds, sort and group, empty, loading and error

_Slice: `vault-2` of the 2026-11 redesign, on `wt/lockdown-vault` (for `claude/keen-ritchie-q2hgb2`), built 2026-11-15. Brief V1 of the undrawn-areas brief, with X6's long lists and 5.2's empty states; the drawn frames Mobile / 01 (Items, list) and States / 04 (loading), and the list panes of Desktop / 01, 03 and 04. The slice text named this doc 69 with D770 to D779; vault-1 took both (`69-redesign-item-detail.md`, D770 to D777), so this is doc 70 with D780 to D789. The integrator settles the vault lane's numbers at the fold._

## For future agent

- **TL;DR:** `features/vault/vault-view.tsx` draws every list of the vault (All items, each smart view, each kind, each vault, folder and tag) as unicron `ListRow`s (`item-row.tsx`) under unicron `SectionHeader`s. On a desktop with the sidebar's groups showing, All items starts with a `SearchField` (typing opens Search) and letter headers; any other list starts with a `SectionHeader` (name, count, SORT); a scope adds a chip row of the kinds it holds. On a phone and at medium width the shell's page head (switcher, chips) heads the list and the list adds its sort button. The row's ⋯ menu is `organization/row-menu.tsx` (`RowMenuItems`), its two dialogs `row-menu-layers.tsx`.
- **Read with:** brief V1, X6, 5.2; doc 64 (places, the page head and chips the shell owns); doc 67 (`notify` and the toast words); doc 69 (the detail's ⋯ menu, which the row menu mirrors).
- **Do not:** put a copy button, a kind glyph or method glyphs back on a row; read the network security check from the list (the cached report only); give a row menu entries the person's access does not allow; store anything but the sort and group choice in `localStorage` (key `lockdown.list-order`).

## 1. The row (brief V1)

`ItemRow` is `ListRow layout="inline" metaStyle="plain"` (92 px; `size="compact"`, 56 px, when Settings' density is compact), rendered as an `<li>`.

| Part | Built |
|---|---|
| Title | As written. Marks: `CrownJewelMark` for a crown jewel, a filled star (named "Favorite") for a favorite. |
| Mono line | `monoLine()`: a login's site host; a note's `edited NOV 08` (`lib/format` `rowDate`); any other kind's `subtitle`; nothing for a crown jewel (D113). |
| Right-hand words | `rowWords()`: a login's methods (`Passkey + code`, `Passkey`, `Login + code`, `Login`) or the kind's singular name (`Card`, `Wi-Fi`, `SSH key`, `Secure note`); in a kind view (or Wi-Fi, Contacts) across vaults, the vault's name, as Desktop / 03 draws. Then `// conflict`, `// archived`, `// deleted`, `// shared`, and, beside the list (not on a phone), `// weak`, `// reused` or `// breached` from the cached report (`securityReport()`, never `securityCheck()`), the worst one, never a dismissed one. A state the view already names is not said again. |
| ⋯ | ListRow's own overflow button, named "More for {title}" (plus the marks' words, see gaps), shown on hover and focus with a pointer and always on touch; the Menu key, Shift+F10 and a right click open the same menu (ListRow handles them; found-4's `useRowMenuKeys` is for parts that do not). |

**The row menu** (`RowMenuItems`): Add to / Remove from favorites, Folder and tags…, Move to vault (one item per other vault, under a label, not a submenu, so it reads the same at 390), Archive or Unarchive, History, then Move to Recently deleted in critical words. In Recently deleted: Restore, History, Delete for good. A view-only or fill-only share has no menu (as the detail). Archive and Move to Recently deleted ask nothing; their toast ("Archived Steam", "Moved Steam to Recently deleted") offers UNDO. Delete for good asks (`ConfirmDialog`, focus on Cancel, the detail's words). A failed action shows the core's sentence in a compact critical Banner at the top of the list (the menu closed, so there is no control to sit beside). **Share… is left out** until edges-8's `ItemShare` can be opened from a menu (as doc 69). The old "File in folder" submenu is gone; Folder and tags… files the item.

## 2. The list's head

| Where | Head |
|---|---|
| All items, desktop | A hidden `h1`; a `SearchField` "Search 41 items" whose first letter opens Search with the letter typed (`search-words.ts`); its trailing slot holds the sort button and the shell's Check again. Letter headers (`SectionHeader variant="index"`, `S // 04`, sticky). |
| A view or a kind, desktop | A hidden `h1`; `SectionHeader` with the name and count; its action slot holds SORT and Check again. No search field, no letters (Desktop / 03). |
| A vault, folder or tag, desktop | The scope's `SectionHeader`, then a single-choice `FilterChipGroup` of All and the kinds the scope holds (`scopeChips()`). |
| Phone, medium width | `AppFramePageHead` with what the shell lays over it (overline `ALL VAULTS // 17 TYPES`, the switcher, the chip row); the list adds its sort button beside Check again. |

**Sort and group** is one `AdaptiveMenu` named "Sort and group" (a sheet on a phone): Sort A to Z, Recently changed, Recently used; Group By letter (All items with A to Z only), By type, None. The choice is kept on the device for every list. Defaults: A to Z; by letter on a desktop, none on a phone (`defaultGroup()`). Expiring keeps its soonest-first sections.

**Gone from the list:** the per-list query box (Q12, recorded by vault-5); the Quick access button (⌘K stays; D111, recorded by vault-5); Find duplicates (Health reaches it since found-3; D152, recorded by vault-8) **except in the web vault**, whose Health has no way in yet (an icon button stays there until vault-8 and the foundation's `web-app.tsx` give `WebSecurityView` the rows); the pane's own New item (the shell's); the group toggle (merged into the menu); the row's copy button (copying from a list is ⌘K and the detail; the phone's swipe is slice 5).

## 3. States

| State | Built |
|---|---|
| Loading | Five `Skeleton variant="row"` in a `role="status"` with "Loading items"; no count in the head (States / 04). The strip's "Opening your vault" is found-5's. |
| A reload failed over rows | The rows stay; above them a critical `Banner` "Couldn't load your items." with the core's sentence and TRY AGAIN (it reloads). |
| Nothing loaded | `EmptyState tone="critical"`, `ITEMS // NOT LOADED`, the core's sentence, TRY AGAIN. |
| Empty | Every row of brief 5.2: All items (NEW ITEM opens the New item menu where the sidebar's groups don't show; beside the sidebar its own NEW ITEM is the one way in, so the Empty State does not repeat it), Favorites, a kind (`NEW CARD`, only where the template is creatable), Passkeys, Contacts, Archived, Recently deleted in one vault (its `trash_days`) or across vaults, a scope, a kind inside a scope (`FAMILY // CARDS // 00`, "No cards in Family."), Codes, Expiring, Needs attention. |
| A shared vault that did not verify | A compact info `Banner` with `VaultView.unverified` as written, inside a `role="note"`, nothing to press (D573). |
| Recently deleted | A caption: "Kept for 30 days, then deleted for good." from the vault, or "Each vault keeps deleted items for its own time." across vaults. |

**Long lists (X6).** Over 200 rows (`WINDOW_FROM`), each group draws only the rows in its scroller's view and eight either side; the rest are stood in for by space of their height, so the scroll bar and the sticky letters stay true. Each drawn row keeps `aria-posinset` and `aria-setsize`.

## 4. Tests, and what is not proven

- `list-model.test.tsx` (16): letter groups (accents folded, `#` first, counts written `03`), letters only in A to Z order, the device defaults, recently used, kind views and a scope's chips, the mono line, the methods' words, the right-hand words with the vault and the states, the report's worst finding.
- `vault-view.test.tsx` (23): loading draws skeletons and no count; letters on a desktop, regions named "M, 2 items"; By type keeps D93's named groups (R18); the sort menu keeps its choice and hides By letter out of A to Z; typing in All items opens Search; the row's parts and no copy button; the vault in a kind view, the Section Header and SORT; windowing over 200 rows; a failed reload keeps the rows with the Banner and Try again; the critical Empty State; the empty states of 5.2; the Recently deleted caption; a scope's chips; the unverified note (the three tests of doc 49, kept).
- `row-menu.test.tsx` (8, in the real shell): every action reachable at 390, 800 and 1440 px from the ⋯, and from the Menu key and Shift+F10; star, archive, move and Recently deleted through the core, no question for the last; Recently deleted offers Restore, History and Delete for good, which asks with Cancel focused; nothing on a view or fill-only share.
- `quick-palette.test.tsx`: the copy toast's words follow X2 ("Copied the password for Forum, clears in 30 s").
- Not proven by a test: the windowing's scroll math (jsdom has no layout; the 300-row state shows it), the sticky letters, and the compact density's look (the compact picture of `desktop-vault-filled`).

## 5. Gaps

- **Share… is not in the row menu**, as in the detail's (edges-8).
- **The web vault keeps Find duplicates in its list** until its Health can open the review.
- **Swipes** are slice 5 (the V1 table); the phone has no copy from the list until then.
- **One critical Banner per section** is not enforced between the shell's channel-4 Banner and this list's failed reload; both can show in the rare case both are true.

### unicron host-contract gaps

1. `ListRow`'s overflow button is named by `menuLabel` plus the whole title line, marks included, so a favorite's is "More for Bluefin Bank Favorite" where the brief says "More for Bluefin Bank". Screen: every list row with a mark.
2. `ListRow`'s title wraps; X6 wants one line cut at the end. A wrapped title also breaks the window's fixed row height a little. Screen: any list with a long title (`desktop-vault-long`, a long-title item).
3. `ListRow` keeps its menu's open state inside, so a row cannot take found-4's `useRowMenuKeys` (it handles the same keys itself, so nothing is lost).
4. No windowed list part: the window is app code (`useWindow` in `vault-view.tsx`). Screen: a list over 200 rows.

## 6. Proposed decisions

| # | Decision | Status |
|---|---|---|
| D780 | **All items groups by letter on a desktop and not at all on a phone, as drawn**; By type (D93's) and None stay in "Sort and group", which merges D93's sort menu and group toggle; the choice is kept per device. Supersedes D93's default. Plain version: the list reads A to Z with letters like an address book; other groupings are one menu away. | Proposed; **flagged for Sean** (app spec question 8, 6.2 D93) |
| D781 | **A row is title, mono line and words**: no kind glyph, no method glyphs, no copy button; the methods or the kind in words, the vault in a kind view across vaults, then state words, and on a desktop the report's finding. Plain version: rows look like the frames and say what each item is in words. | Proposed; **flagged for Sean** (6.3 Mobile / 01, Desktop / 01, 03) |
| D782 | **The row menu mirrors the detail's ⋯** (by access; Move to vault as a group, not a submenu; Archive and Move to Recently deleted with UNDO, Delete for good asks). Plain version: a row's menu does what the item's own menu does. | Proposed |
| D783 | **Typing in All items' field opens Search**, and emptying it there comes back to the list. Plain version: one search, wherever you start typing. | Proposed; **flagged for Sean** (X4) |
| D784 | **A failed reload keeps the rows** under a critical Banner with TRY AGAIN; nothing loaded is a critical Empty State. Plain version: a hiccup never empties your list. | Proposed |
| D785 | **Lists over 200 rows draw only what is in view** (X6). Plain version: a big vault scrolls as smoothly as a small one. | Proposed |
| D786 | **US spelling in the list's words** ("Favorites", "Remove from favorites", "Software licenses"); the core's ids keep theirs (`favourites`, `software-licence`). | Proposed |
| D787 | **The copy toast is X2's**: "Copied the password for Steam, clears in 30 s"; "Nothing was copied" when the gate is backed out of. | Proposed |

D788 and D789 are free.
