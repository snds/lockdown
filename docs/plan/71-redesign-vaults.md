# 71: vaults, folders and tags: the sidebar's trees, the Switch vault sheet and popover, their menus and name prompts

_Slice: `vault-3` of the 2026-11 redesign, on `wt/lockdown-vault` (for `claude/keen-ritchie-q2hgb2`), built 2026-11-15. Brief V2 of the undrawn-areas brief (and X1's VAULTS rows, X5's name prompts and questions); the drawn frame Mobile / 12 (Vaults, switch sheet, `40:1775`). Answers review finding R11. The slice text named this doc 70 with D780 to D789; vault-2 took both (`70-redesign-lists.md`, D780 to D787), so this is doc 71 with D790 to D799. The integrator settles the vault lane's numbers at the fold._

## For future agent

- **TL;DR:** `features/organization/organization-nav.tsx` draws the vaults, their folders and the tags twice: as the sidebar's `VAULTS // NN` tree (a unicron `NavSection tree` with a `+`), and as the switcher's rows (the phone's Switch vault sheet and the medium width's popover, which the shell owns and fills with this component). Every vault, folder and tag row is an `OrgEntry`: a `NavItem` whose trailing `action` is an `AdaptiveMenu` (a menu from the medium width up, a sheet over the switch sheet on a phone). `name-prompt.tsx` holds the two layers the menus open: `NamePrompt` (an `AdaptiveDialog` Form with one `TextField`) and `ConfirmPrompt` (unicron's `ConfirmDialog`). What each menu offers is `vaultMenuOffers` and `folderMenuOffers` in `org-model.ts`.
- **Read with:** brief V2, X1 (the sidebar's rows), X5 (questions and name prompts); doc 64 (the shell's switcher, places, `membersPlace`); doc 65 (`useRowMenuKeys`); doc 67 (`notify`).
- **Do not:** give the switcher `role="tree"` without changing the foundation's `steps/shell.ts` `tree()` and `shell.test.tsx`'s `treeRow`, which read its rows as a list named "Vaults" with `Expand {name}` buttons; offer a vault shared with you more than its right allows; toast a failure (it is a critical line, cross-app §9).

## 1. The rows

| Row | Sidebar (expanded) | Switcher (phone sheet, medium popover) |
|---|---|---|
| All vaults | (the ITEMS group's All items) | First, with the count of live items; current when no scope is set |
| A vault | `NavItem` in the tree, its folders by `expanded` (the tree's → and ←, or the chevron) | The same row; a chevron button `Expand {vault}` opens the folders under it at level 1, and a tap on the name picks the vault |
| A folder | Level `depth + 1`, `folder--open` while it is the scope | The same |
| Tags | A row that opens the tag tree in place (`work/clients` under `work`) | The same |
| New vault | The `+` beside `VAULTS // NN` | A row ending the vault list (cross-app §7) |
| SHARED WITH | (Sharing's Vaults tab) | Section Header and one `ListRow` per shared vault (`3 members // you own it`, `OWNER`) that opens its members (found-3, kept) |
| Sharing | The group's last row, its needs-you mark | The last row, the same mark |

Folders and tags start closed (R11). A branch opens by itself while the place showing is in it.

**Keys.** The sidebar's group is unicron's tree (C4): one tab stop, ↑ ↓ move, → opens or goes to the first child, ← closes or goes to the parent, Home and End. The switcher keeps found-3's list of buttons (a finger has no arrow keys, and the foundation's steps read it so), and this slice gives it the same arrow keys (`moveInList`). On any vault, folder or tag row the Menu key, Shift+F10 and a right click open its menu (`useRowMenuKeys`, cross-app §10). Tab from a row reaches its ⋯.

## 2. The menus (brief V2)

| On | Rows |
|---|---|
| A vault you can manage (yours, or shared to you with `manage`) | New folder · Rename · Keep deleted items for… · **Share…** while it is not shared, **Members** once it is |
| A vault shared with you to view | Who has it |
| A vault shared with you to edit | New folder · Who has it |
| A folder (in a vault you may file in) | New folder inside · Rename · Move to the top (a folder inside another) · Remove folder |
| A tag | Rename · Take off every item |

- The ⋯ is the row's `NavItem action`: shown on hover and focus with a pointer, always on touch. Its name is "{row} options" ("Family options", "#work/clients options"), as today.
- The menu's overline names the kind of row (`VAULT`, `FOLDER // PERSONAL`, `TAG`) and its title the row, which is the sheet's header on a phone.
- **Keep deleted items for…** swaps the menu's rows for its six choices (7, 14, 30, 60, 90 days, A year), the vault's own time checked (a radio group). A pick saves and closes. On a desktop the overline then reads `WORK // KEEP DELETED ITEMS FOR`, because a desktop menu shows no title.
- Share…, Members and Who has it go to `membersPlace(vault)` (edges-7 builds that screen). Sharing's absence (no account) leaves them out.
- What asks (X5): Remove folder ("Remove "Finance"?", the yes `Remove folder`) and Take off every item ("Take #travel off every item?", the yes `Take off`), both destructive `ConfirmDialog`s with focus on Cancel. Move to the top and the time pick ask nothing.

## 3. Name prompts (brief X5)

`NamePrompt` is an `AdaptiveDialog` (form, 640, on a desktop; a sheet on a phone) holding one `<form>` with one `TextField`; the footer's yes submits it (`form=`), so Enter saves. Overline: the thing (`VAULTS`, the vault, `PERSONAL // FINANCE`, `#travel`). Title and the one line under it are today's. Yes: `Create vault`, `Create folder`, `Rename`. An empty name says "Type a name." under the field; a refusal shows the core's sentence under the field (the field turns critical) and the prompt stays. A press outside does not close it (typed words); Cancel, Escape and the close button do.

## 4. Words and messages

- Toasts (channel 2, `notify`): "Created Household", "Renamed Personal to Home", "Created School", "Moved Taxes to the top", "Removed Finance, 3 items moved up", "Renamed #work to #jobs on 4 items", "Took #travel off 3 items", "Work keeps deleted items 60 days".
- A failed change that asked nothing (the time pick, Move to the top) shows the core's sentence in one compact critical Banner under the VAULTS group, or at the top of the switcher's rows, dismissable. It is never a toast.
- "Make vault" and "Make folder" become "Create vault" and "Create folder" (cross-app §12). "Keep trashed items" becomes "Keep deleted items for…" ("Recently deleted", §12). "Remove" on a folder becomes "Remove folder".

## 5. Tests, and what is not proven

- `organization-nav.test.tsx` (16): the sidebar's tree (kept); creating a vault; **in the real shell at 1440, 800 and 390 px**: every vault, folder and tag menu's rows, each reached from the keyboard alone (the tree's → in the sidebar, the Expand buttons in the popover and the sheet), a vault you manage, a shared one you own (Members) and one shared with you to view (Who has it only); Keep deleted items for… shows its six choices with the vault's time checked and saves a pick; Remove folder asks with Cancel focused, and Cancel removes nothing. Outside the shell: a rename saves, and a refusal keeps the prompt with the core's words under the field; an empty name says so and saves nothing; Take off every item asks first; Shift+F10 and the Menu key open a row's menu; the switcher's arrow keys (↓, →, →, ←, ←, End, Home).
- `org-model.test.tsx` (+4): `vaultMenuOffers` by right and sharing, `folderMenuOffers`, `trashDaysWords`.
- `app/shell.test.tsx` (foundation's, unchanged) still reaches every vault, folder and tag and the Family vault's Members from its ⋯ (that is why the item stays "Members" on a shared vault; D792).
- Not proven by a test: the sheet's height as a tree opens (the shell sets `height="content"`; the pictures show it growing), the ⋯ always showing on a touch screen (unicron's `pointer-coarse`; the capture's browser has a fine pointer, so the pictures show it on focus only).

## 6. Gaps

- **The switcher is a list, not a tree** (brief V2 says `role="tree"`). The foundation's step helper and shell test read it as a list named "Vaults" with Expand buttons, and both are frozen in the lane. It has the tree's keys. Proposal for the integrator: move both to `role="tree"` once a unicron `NavSection tree` can hide its heading under a Sheet Header (gap 1).
- **The sheet does not grow to full height on its own when a tree opens**: the shell's `AdaptivePopover` is `height="content"` on a phone (found-3); content height grows with the rows up to the screen, which reads right in the pictures.
- **Landscape (844 × 390):** the medium popover and a menu over it run past the screen's foot (the popover holds every row; SHARED WITH and Sharing sit below the fold). The popover is the shell's; the menu's placement is unicron's.
- **A vault's sharing word:** the brief's "Share…" shows only on a vault not yet shared; a shared one keeps "Members" (D792).

### unicron host-contract gaps

1. `NavSection tree` always draws its overline heading, so a tree cannot sit under a Sheet Header or PopoverHeader that already names it (VAULTS // 03). Screen: the Switch vault sheet and popover.
2. `AdaptiveMenu` has no submenu or second page, so "Keep deleted items for…" swaps the menu's rows in app code (`MenuPage`). Screen: a vault's menu.
3. A desktop `AdaptiveMenu` shows `overline` and not `title`, so the app writes the page's name into the overline there (`useAdaptiveSheet`). Screen: Keep deleted items for… on a desktop.
4. A `NavItem` with both `expanded` and `action` draws the ⋯ over the chevron while hovered (found-3's gap 4, still true). Screen: a vault row with folders, in the sidebar.
5. `AdaptivePopover` and `DropdownMenu` do not cap their height to a short screen and scroll. Screen: the switcher popover and a vault menu at 844 × 390.
6. The SHARED WITH `ListRow` sits 16 px in from the sheet's gutter (its own padding inside the Sheet Body's); Mobile / 12 draws its title on the gutter. Screen: the Switch vault sheet.

## 7. Proposed decisions

| # | Decision | Status |
|---|---|---|
| D790 | **Every vault, folder and tag row has a ⋯ at every width**: on hover and focus beside the sidebar, always on touch, a menu over the medium popover, a Menu sheet over the Switch vault sheet on a phone; the Menu key, Shift+F10 and a right click open it. Plain version: the menus that showed only from 768 px now work on a phone too. | Proposed; **flagged for Sean** (brief V2, R11; 6.3 Mobile / 12 adds the ⋯) |
| D791 | **A vault's menu offers only what the person's right allows**: a vault shared with you to view offers Who has it; to edit, New folder and Who has it; to manage, everything. Plain version: no menu row the core would refuse. | Proposed; **flagged for Sean** (brief V2 menus; 18-sharing "nothing the right doesn't allow") |
| D792 | **The sharing row of a vault's menu is "Share…" until the vault is shared, then "Members"**; both open its members. Plain version: the menu says what you'll find. The brief's single "Share…" would rename the Members entry the foundation's shell test reaches. | Proposed; **flagged for Sean** (brief V2 menus) |
| D793 | **Keep deleted items for… is a second page of the vault's menu** with six radio choices, the vault's time checked; a pick saves at once and closes. Plain version: the time each vault keeps deleted items is two taps from its row. | Proposed; **flagged for Sean** (brief V2; cross-app §5 "five or more = a menu") |
| D794 | **Name prompts are one-field Forms**: `AdaptiveDialog` with a `TextField`, the error under the field in the core's words, the prompt kept open on a refusal, `Create vault` / `Create folder` / `Rename`. Plain version: naming a vault or folder is one field and one button, and a mistake is shown where you typed. | Proposed; **flagged for Sean** (brief X5, cross-app §12) |
| D795 | **Removing a folder and taking a tag off every item ask**, focus on Cancel, the yes repeating the verb; moving a folder to the top and the time pick ask nothing. Plain version: only what changes many items asks. | Proposed (brief X5's table) |
| D796 | **A failed change from these menus is one critical line, never a toast**: under the VAULTS group, or at the top of the switcher's rows. Plain version: an error stays where you can read it. | Proposed (cross-app §1, §9) |
| D797 | **The switcher's rows keep their list of buttons and gain the tree's arrow keys.** Plain version: a finger taps the chevron; a keyboard moves as in the sidebar. | Proposed; integrator to revisit with gap 1 |
| D798 | **The toasts name the thing**: "Created Household", "Took #travel off 3 items", without a full stop. | Proposed (cross-app §12, X2) |

D799 is free.
