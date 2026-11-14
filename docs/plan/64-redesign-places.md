# 64: places: one table, three shells, a route for every place

_Slice: `found-3` of the 2026-11 redesign, on `claude/keen-ritchie-q2hgb2`, built 2026-11-14. The navigation of the redesign brief (§3 and X1 of the undrawn-areas brief): a sidebar at the expanded width, its rail at medium width, floating rows on a phone, and one table that names every place for all three. The screens inside keep their old layout; later slices rebuild them._

## For future agent

- **TL;DR:** `apps/desktop/src/app/places.ts` is the one table of places. The shell (`app/shell.tsx`) is unicron's frame parts and nothing else; the frame picks the shell from the window's width. Devices, Recovery and Status are Settings sections. People is a Sharing tab. Search is a place. Security is called Health.
- **Do:**
  - Move with `usePlaces().go("<place id>")`. Never build a `Nav` by hand in a screen.
  - Name a deeper place in the breadcrumb with `useCrumb(["Ticketmaster"])`.
  - Draw a screen's head with `ScreenHead` (`{ overline, title, count, meta, actions, switcher }`), or `PaneHeader` for a title and actions. The shell lays the vault switcher and the chips over it where they apply. `usePageHead` gives the same values to a screen that needs more of `AppFramePageHead`; pass them by name, never as a spread (the ratchet counts one).
  - Use `MasterDetail` for a list beside a detail and `PageColumn` for a screen that is one column. Both are the frame's own columns.
  - Open the editor with `useShell().edit(target)`. The shell mounts the one `ItemEditor` for new items.
  - Write counts, countdowns and row dates with `lib/format.ts`.
- **Do not:**
  - Write a media query for the shell, or read the window's width. Ask `useAppFrame()`.
  - Add a row to the sidebar by hand. Add a place to the table.
  - Give a screen inside Settings its own page title. `Embedded` drops it; the frame names the section.
- **Read with:** the undrawn-areas brief §3 and X1 (workspace, `07-projects/29-unicron/impl-2026-11/app-lockdown-undrawn.md`), doc 62 (unicron 1.0), doc 63 (the harness).

## 1. The table

`app/places.ts`. A place has an `id`, a `label`, a Carbon `icon`, a `group`, the `surfaces` that draw it (`desktop`, `web`), its `crumbs`, an optional `key`, and the `Nav` it goes to.

| Group | Places |
|---|---|
| search | `search` (key `/`) |
| items | `all`, then the six: `favourites`, `login`, `passkeys`, `codes`, `card`, `note` |
| more | `wi-fi`, every other kind (`ssh-key` … `custom`), `contacts` |
| aside | `archived`, `deleted` |
| tools | `health` |
| health | `expiring`, `attention` (today's two lists, reached from Health) |
| sharing | `sharing`, `sharing-people`, `sharing-vaults`, `sharing-links`, `sharing-emergency` |
| settings | `settings` (key `⌘,`), and 14 sections `settings-<section>` |

Four kinds of place are made from data and read from the id itself: `vault:<id>`, `folder:<id>`, `tag:<tag>` and `members:<vault id>` (one shared vault's members).

**`Nav`** keeps `area`, `view`, `scope` and `device`. It gains `section` (Settings), `tab` (Sharing) and `members`. `Area` is now `vault`, `codes`, `security`, `sharing`, `search`, `settings`. The ids `favourites`, `deleted`, `security` and `software-licence` stay as the core spells them; only the words people read changed (Favorites, Recently deleted, Health, Software licenses).

**`ListView` gains a value per kind** (`login`, `card`, `note` and eleven more): every kind that a smart view does not already list. `rowsIn` and `countViews` know them.

**What the table answers:**

| Question | Function |
|---|---|
| Where does this id go? | `navFor(id)` |
| Which place is this `Nav`? | `placeIdOf(nav)` |
| Which one row is current? | `currentRow(nav)` |
| Which of the three sections is it under? | `sectionOf(nav)`: `items`, `codes`, `health`, or none |
| What is the breadcrumb? | `crumbsOf(nav, vaults, surface, mac, extra)` |
| Which places does this build draw? | `placesFor(surface)`, `sharingTabs(surface)`, `settingsSections(surface)` |

## 2. The three shells

`app/shell.tsx`, `top-row.tsx`, `bottom-row.tsx`. One tree of frame parts: `AppFrame`, `AppFrameSidebar`, `AppFrameStatusLine`, `AppFrameTopRow`, `AppFrameMain`, `AppFrameBottomRow`. Each part draws itself only where it belongs.

| Width | Shell |
|---|---|
| Expanded (1024 px up) | The sidebar, 204 wide: lockup, `NEW ITEM`, Search with `/`, `ITEMS // NN TYPES`, `VAULTS // NN`, `TOOLS`, and Settings pinned at the foot beside Lock. The status line holds the breadcrumb as links. List beside detail. |
| Medium (600 to 1023 px) | The rail: the mark, create, Search, Items, Codes, Health with its count; Settings and Lock at the foot. One content column. A chosen row pushes its detail; the way back is in the status line's leading slot. |
| Compact (under 600 px) | The top row: lockup and the profile button, which opens Settings. The bottom row: Search, a tab bar of Items, Codes and Health, and create. A pushed screen gets a top bar with back and no tab bar. |

**The sidebar's rows.** `All items · Favorites · Logins · Passkeys · Codes · Cards · Notes · More`. More holds every other kind that has an item, then Archived and Recently deleted. It is closed until opened, its state is kept on the device, and it is open while the place showing is inside it. `ITEMS` and `VAULTS` are trees: Up and Down move, Right and Left open and close.

**The vaults group** (`features/organization/organization-nav.tsx`). A vault with folders opens its folder tree in place: when its chevron or Right is pressed, and while the vault or one of its folders is the place showing. Tags is one more row with the tag tree inside. Sharing is the last row, with a needs-you mark for what waits on the person (key resets and emergency requests, from `sharing.overview()` at unlock and every 60 s; not read with no account or a local-only vault). Every row keeps its menu under the same name (`Personal options`).

**Exactly one row is current**: the deepest real place.

| You are in | Current row |
|---|---|
| A view across all vaults | That view's row |
| A vault, folder or tag | That row; no view row |
| Codes | Codes |
| Health, Expiring, Needs attention | Health |
| Sharing, any tab, a vault's members | Sharing |
| Search | Search |
| Settings, any section | Settings |

In the rail and on a phone the three sections stand for the places inside them. On Sharing and Settings no section shows as current, because neither is one.

**Where the sidebar's groups don't show** (the rail, a phone), the Items page head is the switcher:

- Overline: the scope and the type count (`ALL VAULTS // 17 TYPES`, `FAMILY // 04 TYPES`). Title: the view's name (`Items` for All items). Count: the rows in view.
- Pressing it opens "Switch vault": All vaults, each vault with its folders, Tags, New vault, the `SHARED WITH` rows (each opens that vault's members), and Sharing. A sheet on a phone, a popover at medium width.
- A sideways swipe on it (or Left and Right while it has focus) steps the scope: All vaults, then each vault.
- Under it, the chip row named "Show": `ALL n · FAVORITES · LOGINS · PASSKEYS · CARDS · NOTES · MORE`. Only the active chip shows its count. `MORE` opens `TYPES NN` and `KEPT ASIDE`.
- **A departure from the brief:** a chip whose view is empty is left out of the row, as the brief says, and is listed under `MORE` instead of nowhere. Otherwise an empty Passkeys list could be reached from the sidebar and not from a phone.

**Create.** `NEW ITEM` is the shell's one primary. It opens today's list of kinds: a menu on a desktop, a sheet on a phone. On a phone a long press on create (or Shift+Enter, the Menu key, a right click) opens quick create over the menu scrim: Login, Card, Secure note, One-time code. The list pane's own "New item" button is gone, so a screen has one primary.

**Keys.** `/` opens Search and `⌘,` (Ctrl+,) opens Settings, from anywhere but a field. `⌘\` (Ctrl+\) collapses the sidebar to the rail; the choice is kept on the device. At medium width it opens the whole sidebar over the content.

## 3. Where each place went

| Was | Is |
|---|---|
| Sidebar → Sharing | `VAULTS` → Sharing, with tabs People, Vaults, Links, Emergency |
| Sidebar → Devices, Recovery, Status | Settings → Devices, Recovery, Status |
| Sidebar → Security | `TOOLS` → Health |
| Sidebar → Expiring, Needs attention | Health → Expiring, Needs attention |
| List → Find duplicates | Health → Duplicates (the list's button stays until `vault-2` drops it) |
| Sidebar → Wi-Fi, Contacts, Archived, Deleted | `ITEMS` → More |
| Sidebar → Lock | An icon button beside Settings; on a phone, in Settings' top bar |
| Sidebar → the persona select | Settings → Status → Persona (a fact; it was read-only) |
| Sidebar → the restart row | Settings → About → Restart to update. The mark on Settings is `found-5`'s |
| Sidebar → the backup line | Settings → Backups. The Banner for a failed backup is `found-5`'s |
| List → the search box | Still there. Search is also a place of its own |
| Web: Links, People | Sharing → Links, People |
| Web: `#/devices` | Opens Settings → Devices |

**Settings** (`features/settings/settings-places.tsx`, mounted by `settings-view.tsx` and the web's `web-settings-view.tsx`) is unicron's `SettingsFrame` over the sections the table lists: 14 in four groups on the desktop, 4 in the web vault.

| Group | Sections |
|---|---|
| This Mac (or This PC, This browser) | This Mac · Browsers · Quick access · Appearance |
| Account | Devices · Recovery · Status · Server |
| Vault | Import and export · Backups · Email aliases · Codes · Security checks |
| Lockdown | About |

Each section mounts today's component. Devices, Recovery and Status are today's screens inside `Embedded`, which drops their own title. Security checks is `SecuritySettings`; it also stays on Health until `vault-8` takes it off. Narrower than two columns, the device's own rows come first, then the list of the other sections, and a section is a pushed screen.

**Search** (`features/vault/search-view.tsx`) is basic: the field, the live items of every vault that match, grouped by vault, and an Empty State whose `NEW LOGIN` opens the editor with the query as the website (`EditorTarget.website`). `edges-6` finishes it.

## 4. What lanes build on

| API | Where |
|---|---|
| `usePlaces()` → `{ nav, surface, go(placeId), navigate(nav) }` | `app/places.ts` |
| `useCrumb(segments)` | `app/crumbs.tsx` |
| `usePageHead(own)`, `ScreenHead` | `app/page-head.tsx` |
| `useShell().edit(target)` | `app/shell.tsx` |
| `MasterDetail`, `PaneHeader`, `PageColumn`, `Embedded` | `components/master-detail.tsx` |
| `count`, `countName`, `countdown`, `rowDate`, `fullDate` | `lib/format.ts` |
| `QuickCreateItems`, `NewItemMenu` (`trigger`, `open`) | `features/vault/new-item-menu.tsx` |
| `EditorTarget.website` | `features/vault/item-editor.tsx` |
| `toFixCount(report, rows)` | `features/security/security-model.ts` |
| `SettingsPlaces` | `features/settings/settings-places.tsx` |
| `SharingView` `section`, `onSection`, `vault` | `features/sharing/sharing-view.tsx` |

**Words.** Counts follow one rule: `04`, `248`, `1,204`, and `99+` on a mark. A count's accessible name is the number and its noun ("61 items", "3 to fix"). Countdowns are `MM:SS`, and `NN S` under a minute. A row's date is `Today 09:02`, `Yesterday 18:40`, `MON 09:02`, `NOV 02`, then `2026.10.02`.

## 5. The harness

- `design/capture/src/states/steps/shell.ts`: `go(placeId)` looks at which shell is drawn and takes that shell's path. New ids: `settings-<section>`, `sharing-<tab>`, `search`, `health`, `logins`, `cards`, `notes`. `openSwitcher()`, `openMore()`.
- `apps/web/e2e/steps/nav.ts`: the same, for Playwright. New places: `appearance`, `server`, `this-browser`.
- States that walked one long Settings page now open the section they show (`settingsAt("settings-backups")`).
- The five sidebar-tree states (`desktop-scope-*`, `desktop-nav-*`) have phone pictures: they go through the switcher.
- New states, each with `built`: `shell-sidebar-more-open`, `shell-sidebar-folder-tree`, `shell-items-chrome`, `shell-switch-sheet`, `shell-more-sheet`, `shell-quick-create`, `shell-settings-list`.

## 6. Tests

| File | What it proves |
|---|---|
| `app/places.test.tsx` | Every place has one id, a label, an icon and a breadcrumb. Every area, view and kind is a place. Each id goes somewhere and reads back. The one current row. The sections. The web's places. |
| `app/shell.test.tsx` | At 1440, 800 and 390 px: every view and kind, every vault, folder and tag, every Sharing tab, a shared vault's members, Search, Health and its lists, and every Settings section are reached with the keyboard alone, and the navigation marks one current place each time. The sidebar's name, groups and counts. More, remembered. `⌘\`. The breadcrumb. The phone's rows, the switcher, the chips, create and quick create, Lock, a second press on a tab. |
| `components/master-detail.test.tsx` | One `h1` at every width; the pushed detail's way back; a screen inside another's frame draws no second title. |
| `lib/format.test.tsx` | Counts, countdowns, row dates. |
| `features/organization/organization-nav.test.tsx` | The vaults group as a tree: closed folders, Right opens, Tags, the menus, Sharing last. |
| `features/vault/list-model.test.tsx` | A view per kind; the US words. |

Changed with the control they name, none weaker: `theme-setting.test.tsx` and `web-settings-view.test.tsx` open on Appearance, which is a section now. `lib/format.test.tsx` is named `.tsx` because the desktop's test run only collects `*.test.tsx`.

## 7. Gaps

Asked of unicron, with the screen that needs each:

1. **`AppFrameSidebar`'s `action` cannot open a menu.** It is a label and a press; a menu needs a trigger to stand by. The shell lays an unseen trigger over the button the frame drew while the menu is open (as Vale does). Needs: `action.menu`, or the action as a render prop. Screen: the sidebar's `NEW ITEM`.
2. **A page head that is a switcher cannot anchor a popover.** Same work-around, over the head. Needs: the switcher as a popover trigger. Screen: the Items head at medium width.
3. **A breadcrumb part that is a button does not take the line's capitals** (a `button` does not inherit `text-transform`). The shell writes its crumbs in capitals. Screen: the status line.
4. **A `NavItem` with both `expanded` and an `action`:** under a pointer the action is drawn over the chevron, so the chevron cannot be pressed while the row is hovered. Screen: a vault row with folders.
5. **A sidebar drawn as its rail draws every child as a rail item.** The app has to know it is the rail to give it other children, and `AppFrameRailContext` is not exported; the shell works it out from `useAppFrame()`. Screen: the rail.
6. **`SettingsFrame` has no slot for a Lock button** in the phone's top bar; the app's own top row draws it. Screen: Settings on a phone.

Left for later slices, on purpose:

- The screens inside the shell keep their old layout. The list's head still carries Sort, Quick access and Find duplicates as today's buttons (`vault-1`, `vault-2`).
- The status line's right side is empty and toasts still stand at the bottom centre (`found-5`).
- An open item on a phone has a top bar with back and no bottom row yet (`found-4`).
- `RestartRow` and `BackupIndicator` are no longer mounted. `found-5` replaces them with the mark on Settings and the Banner.
- Devices' title used to carry its counts ("Devices (3, 1 revoked)"). Inside Settings the title is the section's, so the counts are in the lists only until `screens-4` (brief T2).
- Sentences that name a place by its old word ("stays readable in Deleted", "Log in again on Status") are their lanes' to change.

## 8. Proposed decisions

| # | Decision | Status |
|---|---|---|
| D720 | **Every place is one row of one table, and a screen moves by a place's id.** `places.ts` names each place once: its id, words, icon, group, breadcrumb and address. The sidebar, the rail, the tab bar, the chips, the Settings list and the breadcrumb read it. `Nav` gains a Settings section, a Sharing tab and one shared vault's members; Search is an area. Plain version: there is one list of everywhere you can go, and every part of the app uses it. | Proposed |
| D721 | **Sharing lives with the vaults.** It is the last row of `VAULTS`, and the last row of the phone's Switch vault sheet, with tabs People, Vaults, Links and Emergency. A shared vault's `SHARED WITH` row opens that vault's members. In the web vault the Links and People screens are its two tabs. Plain version: sharing is found next to the thing people share. (Brief Q1.) | Proposed |
| D722 | **Devices, Recovery and Status are Settings sections.** They leave the sidebar. Settings has 14 sections in four groups, drawn with unicron's `SettingsFrame`; the web vault has four. `#/devices` opens Settings → Devices. Plain version: set-up and proof are kept with the settings. (Brief Q2.) | Proposed |
| D723 | **The Security screen is called Health.** The area's id stays `security`. Its nav mark counts what there is to fix: the findings to fix now and soon, from the cached report, and the Needs attention list. The breach and alert checks still run only when Health opens. Plain version: one place for what to fix, named as the design names it. | Proposed |
| D724 | **Expiring, Needs attention and Duplicates are reached from Health.** The first two stop being sidebar rows and stay the lists they are today; Duplicates opens today's review. This changes where D149, D152 and D153 put them, not what they show. Plain version: lists of things to fix are in the place for things to fix. (Brief Q7.) | Proposed |
| D725 | **The sidebar shows seven lists and More.** All items, Favorites, Logins, Passkeys, Codes, Cards, Notes. More holds every other kind that has an item, then Archived and Recently deleted; so Archived and Contacts leave the top level (D149, D241). Every kind is a list of its own. More's open state is kept on the device. Plain version: a shorter sidebar, with nothing removed. | Proposed |
| D726 | **Search is a place.** A Search row above the lists, with `/` as its key; a rail item; the phone's search button. The list's own search box stays until the lists are rebuilt (the entry half of brief Q12). `⌘,` opens Settings. Plain version: press `/` and search everything. | Proposed |
| D727 | **Lock is a button beside Settings.** An icon button named "Lock" in the sidebar's foot and under the rail's Settings; on a phone it is in Settings' top bar. Plain version: Lock is always one press away on a desktop and two on a phone. | Proposed |
| D728 | **The persona, the restart row and the backup line leave the sidebar.** The persona is a fact on Settings → Status (the select was read-only). Restart to update stays in Settings → About and the backup state in Settings → Backups; the mark on Settings and the Banner for a failed backup come with `found-5`. Plain version: the sidebar is for going places. | Proposed |
| D729 | **Three shells from one composition, picked by the window's width.** Under 600 px the phone's floating rows; 600 to 1023 px the rail and one column; from 1024 px the sidebar with list beside detail. Where the sidebar's groups don't show, the page head switches the vault and a chip row picks the list; a view with nothing in it is listed under MORE. The words people read are in US spelling (Favorites, Software licenses) and the 30-day holding place is "Recently deleted". Plain version: the same app at every width, with every place reachable at each. | Proposed |
