# 73: Search, Quick access with GO TO, and the quick-access window

_Slice: `vault-5-search` of the 2026-11 redesign, on `wt/lockdown-vault` (for `claude/keen-ritchie-q2hgb2`), built 2026-11-15. The drawn frames Mobile / 14 Search, no results (`40:2171`; stone `53:10975`), Mobile / 15 Search, results (`40:2225`; stone `53:10989`) and Desktop / 11 Search, no results (`41:3804`; stone `53:11710`); brief X4; cross-app decisions §6. The slice text named this doc 72 with D800 to D809. vault-4 took both (`72-redesign-swipes.md`, D800 to D807), so this is doc 73 with D810 to D819. The vault lane runs two slots late now; the integrator settles the numbers at the fold._

Status: built. Search is the place X4 draws; Quick access keeps every key and gains GO TO and the last row; the quick-access window's locked state is the focus frame.

## For future agent

- **TL;DR:** `features/vault/search-view.tsx` is the Search place: the field (named for the vaults), the scope chips, RECENTLY USED before typing, results grouped by vault with `Highlight` on the item lists' own rows (`ItemRow` gains `highlight`), the Empty State with NEW LOGIN. `features/vault/search-words.ts` hands Search what it starts from: the words typed elsewhere (`searchWith`, now heard by an open Search too) and the list it was opened from (`rememberWhere` / `openedFrom`, written by `VaultView`). `features/palette/quick-palette.tsx` is Quick access: ITEMS as compact `ListRow`s rendered as `CommandItem`s, GO TO (`goToRows` in `palette-model.ts`), the last row, the keys under the list, a compact `CodeDisplay` for a shown code. `app/quick-window.tsx` draws the locked window in `AppFrameFocus`.
- **Read with:** brief X4, X5 (Lock), X2 (the status line, toasts), doc 64 (places, `go`), doc 70 (the lists, typing in All items opens Search), doc 72 (swipes), D111, D113, D118, D149.
- **Do not:** let Ctrl+Enter act on anything but an item (D118: it copies, so on a place it does nothing); put an interactive control inside a `CommandItem` (axe's nested-interactive: the shown code stands under the list for that reason); search archived or deleted items from EVERYWHERE (D149); show a crown jewel's mono line or match it by anything but its title (D113).

## 1. Search (brief X4)

| Part | What it does | Where from |
|---|---|---|
| Field | Focused. "Search Personal and Family"; "Search your vault" with one; "Search Personal, Family and Work" with three; "Search 7 vaults" past three | `vaults` |
| Scope chips | Only when Search was opened from a narrower list: `EVERYWHERE` and that list (`FAMILY`, `#WORK`, `CARDS`, `FAMILY // LOGINS`). From Archived or Recently deleted the second chip starts chosen | the list's own `Nav`, kept by `VaultView` while it shows |
| Before typing | `RECENTLY USED NN`: up to five by `last_used`, newest first; nothing when this device used nothing | `ItemSummary.last_used` (never synced) |
| Results | Every key narrows at once. Grouped by vault under a Section Header with its count, in the vaults' order; no headers with one vault or with a vault's chip chosen. The words marked in the title and the mono line; a crown jewel's in its title only | `searchRows` (title, site, username, subtitle, `#tag`, folder path; a crown jewel by title) |
| Rows | The item lists' `ItemRow`: the ⋯ row menu, the Menu key and right click; on a phone the swipes of doc 72 | `RowMenuItems`, `useRowSwipe` |
| No results | Empty State `SEARCH // 00`, "No item for {q}.", "Nothing in Personal or Family matches. Save it from the browser next time you sign in, or add it here.", `NEW LOGIN` (the editor with the words as the website) | `EditorTarget.website` |
| Escape | Clears the field; the second leaves for the list Search came from (All items when it came from elsewhere) | |
| Breadcrumb | `SEARCH`; `SEARCH // FAMILY` while the vault's chip is chosen | `useCrumb` |
| Status | `05 RESULTS` while words are typed | `useStatusItem` |

Desktop is Desktop / 11's frame: the list pane holds the field, the chips and the results or the Empty State; the detail sits beside with "Nothing open." until a row is chosen. Phone is Mobile / 14 and 15: the shell makes the search button current and no tab active (found-3).

## 2. Quick access (brief X4, D111)

- A `CommandDialog` of the form size (640), the field focused, "Search your items and places". Not offered on a phone (the compact width class): ⌘K does nothing there, and a window narrowed to a phone's width closes it.
- **ITEMS**: the core's ranked index, as before. Each result is a compact `ListRow` (56 px) rendered as the `CommandItem` itself: the title with `CrownJewelMark` and the favorite star, one mono line (`kestrel04 // github.com`; the kind's name when there is nothing else; nothing for a crown jewel), `CLOSE MATCH` as a Badge when `exact` is false, the words marked with `Highlight`. The copy is the row's `CommandItemAction`, as before.
- **GO TO** (from two letters: one letter names nearly every place): this build's places from `placesFor(surface)` whose name or group matches: All items and each view, each vault, Codes, Health and its two lists, Sharing and its tabs, Settings (with its key) and each section (This device by its real name); then Lock. The web vault lists only its own places. Search itself is the last row, not a GO TO row.
- **The last row**: "Search everything for “{q}”" opens Search with the words.
- **"No matches. Try fewer letters."** shows above GO TO when ITEMS found nothing; "Nothing saved yet." when the vault is empty.
- **Keys** under the list, for the highlighted row, with `Kbd`: ↵ Open, ⌘↵ Copy {copy.label}, then Show code ⌥↵ and Fill in the browser ⇧↵ as buttons (the web has no fill). On a place: ↵ Go; on Lock: ↵ Lock; on the last row: ↵ Search. ⌘↵ on anything but an item does nothing.
- **A shown code** is a compact `CodeDisplay` under the list for 30 s, kept out of screen captures; its copy is by id through `copy_code`, with the clipboard clear at the countdown's end (as `ItemCodeDisplay`).
- **A crown jewel's copy** closes the palette first, then the gate opens (unchanged).

## 3. The quick-access window

The same palette in its own window. GO TO offers Lock alone there: the window can open an item in the main window (`openMain`), but no place, and no host command for that exists (no backend changes in this pass). Locked, the window is `AppFrameFocus`: the mark alone (`AppLockup showName={false}`), "Lockdown is locked.", `UNLOCK` (the one primary, focused) and `NOT NOW` (text). Nothing of the vault draws.

## 4. Words

US spelling; counts in two digits (`RECENTLY USED 03`, `05 RESULTS`, `SEARCH // 00`); no articles on buttons (`NEW LOGIN`, `UNLOCK`, `NOT NOW`); toasts without a full stop ("Nothing was shown"). The drawn `RESULTS // 00` and `ITEMS // SEARCH` change to `SEARCH // 00` and `SEARCH` (undrawn 6.3).

## 5. Pictures

`$S/shots/built/lockdown/`, scale 2, graphite and stone: drawn `search--no-results` (phone, desktop) and `search--results` (phone); undrawn `search--recently-used` (phone, desktop), `search--results` (desktop), `quick-access--items`, `quick-access--go-to`, `quick-access--window-locked` (desktop only). States: `desktop-vault-search` (phone), `desktop-vault-search-wide` (new, desktop), `desktop-vault-search-none`, `desktop-vault-search-recent` (new), `desktop-vault-search-scoped` (new, not sent), `desktop-palette`, `desktop-palette-search`, `desktop-palette-go-to` (new), `desktop-palette-crown-jewel`, `desktop-quick-window-open`, `desktop-quick-window-locked`. The palette and window states have no phone size.

## 6. Tests, and what is not proven

- `features/vault/search-view.test.tsx` (new, 15): the label's four forms; recently used (five, newest first, none unused); grouping by vault in order; the chips' rules; in the real shell at 1440: focus and RECENTLY USED with the crown jewel by title, nothing when nothing was used, groups with counts and the marks (a crown jewel's in its title only), the status line, no headers with one vault, the Empty State and NEW LOGIN's website, Escape twice back to the vault, a vault's chip narrowing with the crumb, Archived's chip chosen, no chips from All items, words handed over while open, a row chosen.
- `features/palette/quick-palette.test.tsx`: every earlier test kept (counts now look inside ITEMS, the placeholder's new words, the shown code as a CodeDisplay), plus GO TO from two letters with places, vaults and Lock; Enter goes by id and closes; the web's places only; the last row opens Search with the words; ⌘↵ on a place does nothing; the marks; the keys line.
- `app/quick-window.test.tsx`: one main and one h1 before and after the core answers; locked: the focus frame, the mark, UNLOCK focused, NOT NOW, two buttons and no list.
- `apps/web/e2e/steps/search.ts`: `openPalette` finds the new placeholder; `paletteResults` counts ITEMS only (finish.spec's typo test).
- **Not proven:** the Lock command through the stand-in (§7) in a real window; the quick-access window's look at its real size (the capture draws it in a 1440 page); a screen reader through GO TO; WebKit.

## 7. Gaps

- **App.tsx and web-app.tsx are frozen** and mount `QuickPalette` outside the shell, so it cannot read `usePlaces()`. `QuickPalette` gains `onGo`, `onLock` and `surface` (additions only). Until the integrator passes them (`onGo={go}`, `onLock={lockNow}`; the web's `setNav` through `navFor`, its own lock, `surface="web"`), `palette-routes.ts` stands in: `VaultView` and `SearchView` hand over the shell's `go` and the surface while they show, and the Lock command asks the core to lock and fires a `visibilitychange`, so the lock watch reads the locked state at once. Delete `palette-routes.ts` once App passes the props.
- "Where the person was" for Escape is known only for the vault lists; from Codes, Settings or Sharing, the second Escape goes to All items. The shell's own `lastSection` would know; it is foundation code.
- The quick-access window's GO TO has Lock only (§3).

### unicron host-contract gaps

1. `CommandItem` has no compact-row layout of its own; Quick access renders `ListRow` as the `CommandItem` (`render`), which works but merges two class sets. A `CommandItem` that takes `ListRow`'s compact parts would say it plainly. Screen: Quick access.
2. `CodeDisplay layout="compact"` holds a real button, so it cannot sit inside a `CommandItem` (an option). The shown code stands under the list instead of in its row, as the brief draws it. Screen: Quick access with a code shown.
3. `AppFrameFocus` is always a `<main>`, so a window that has its own main swaps landmarks between states. Screen: the quick-access window.

## 8. Proposed decisions

- **D810.** Search is the place X4 draws: the field named for the vaults, RECENTLY USED before typing, results grouped by vault with `Highlight`, the item lists' rows, `SEARCH // 00` with NEW LOGIN. Answers Q12 as picked (the query box leaves every list but All items; the chips keep "search in here"). Flag for Sean (D38).
- **D811.** The scope chips come from the list Search was opened from, read while Search draws; Archived and Recently deleted start chosen (D149 keeps them out of EVERYWHERE). Flag for Sean.
- **D812.** Escape clears, then goes back to the list Search was opened from; from any other place, to All items (§7).
- **D813.** Quick access keeps D111's keys and gains GO TO (this build's places, then Lock) and the last row; GO TO starts at two letters. Flag for Sean.
- **D814.** Quick access is not offered on a phone (X4): ⌘K does nothing at the compact width.
- **D815.** ⌘↵ acts on items only; on a place or the last row it does nothing (D118's care, kept for the new rows).
- **D816.** A shown code is a compact `CodeDisplay` under the list, not in the row (unicron gap 2). Flag for Sean.
- **D817.** The quick-access window locked is the focus frame's mark, "Lockdown is locked.", UNLOCK and NOT NOW; its GO TO is Lock alone. Flag for Sean.
- **D818.** The palette's result line is who and where (`kestrel04 // github.com`), not the template's name and the username; the kind glyph goes, as on the lists (doc 70). Flag for Sean.
