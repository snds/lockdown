# 89: New item, the editor for every kind, the generator, and your templates

_Slice: `edges-6-editor` of the 2026-11 redesign, lane `edges` (branch `wt/lockdown-edges`), built 2026-11-15. Briefs E1 (Choose a type), E2 (The editor for every kind), E3 (Generator options) and E4 (Your templates) of the undrawn-areas brief; drawn frames Mobile / 06 New item, type sheet (`40:575`), Mobile / 07 New item, Login (`40:915`), Desktop / 05 New item, Login (`41:2089`) and States / 03 Quick create (`51:8659`). Decisions D970 to D979. Answers Q18 of the undrawn brief (no Passkey in the create lists), review R10 (the fewest fields in view) and keeps D232 (your templates) and D230 (the generator's words)._

## For future agent

- **TL;DR:** New item is one list with two looks: a menu under NEW ITEM on a desktop, a sheet on a phone. Five kinds first with their counts, then the four groups (submenus on a desktop, closed Section Headers on a phone), then YOUR TEMPLATES, then "Your templates". The editor is a Form: a 640 dialog from the medium width up, a full-height sheet with a top bar (close, the screen's name, the checkmark as the one primary) on a phone. Essentials first; More options holds the rest. Secrets are unicron's `SecretField` in edit mode. The generator's options are a picker (a popover with its header, a sheet on a phone) with no DONE. Your templates is one layer with a list and a designer.
- **Do:**
  - Keep `ItemEditor`'s props (`target`, `onClose`, `onSaved`) and `EditorTarget`'s fields, `website` included: the shell, the detail pane and Search open it.
  - Keep `NewItemMenu`'s props; `rows` is new and optional (without it the menu reads the list once when it opens, for the counts).
  - Keep `QuickCreateItems`' export and its rows' words: the shell's long press renders it, and `app/shell.test.tsx` names the rows by their template names.
  - Keep `editor-model.ts`'s rows and `buildChanges` (`keep`, `set`, `generate`, `remove`): a secret's bytes never come back to the window, a generated one is made by the core on save (D112).
  - Send every secret through `SecretField`; never draw one with a plain input.
- **Do not:**
  - Put Passkey in any create list (Q18): a site makes a passkey.
  - Show a generated password: the core makes it on save. The helper line says what the options make and the core's strength word.
  - Say "scan" for a one-time code in the editor: there is no camera capture for codes (W11).
  - Raise a toast for a failed save or a failed alias: a save error is a compact critical Banner; an alias error is the Username field's error line.
- **Read with:** briefs E1 to E4 and 3.5, 4.0 (workspace `07-projects/29-unicron/impl-2026-11/app-lockdown-undrawn.md`), doc 21 (vault depth: variants, your templates, the generator, links), doc 64 (places, the create button and quick create), doc 66 (toasts), doc 67 (the Question rule).

## 1. New item (brief E1)

| Part | Desktop and medium | Phone |
|---|---|---|
| Form | `AdaptiveMenu`: a dropdown under NEW ITEM (the sidebar's primary, or the rail's create button) | the same rows as a sheet (Mobile / 06), content height: overline `NEW ITEM // {VAULT} VAULT` (the scope's vault, else the primary one; a view-only shared vault gives the primary one), title "New item" (see §6) |
| First | Login · Payment card · Note · One-time code · Wi-Fi network, each with its icon and the count of items made from it (lib/format: `02`, `14`, `248`; said as the plain number) | the same |
| Leading kind | on Codes One-time code goes first; in a kind's own view that kind goes first (out of its group) | the same |
| Groups | SIGN-INS AND MONEY, IDENTITIES AND DOCUMENTS, NETWORKS AND MACHINES, OTHER (D231's groups) as submenus; each submenu's rows a group of the same name | closed `SectionHeader collapsible` (`aria-expanded`), each wrapped as a group of the same name |
| Inside a group | every other creatable template by its `name`, variants included (Passport, Router, Email account…), each with its count | the same |
| Your templates | `YOUR TEMPLATES 01`: the person's own, by name, only when there is one | the same |
| Foot | "Your templates", which opens §4 | the same |
| States | loading: one line "Loading the item kinds…"; failed: "The item kinds didn't load." | the same |

**Quick create** (States / 03): the shell's long press (doc 64) renders `QuickCreateItems`: overline `CREATE // LONG PRESS`, Login, Payment card, Note, One-time code, no counts; the create button turns to close over the menu scrim. Unchanged from found-3 except its comments; built here as `new-item--quick-create`.

## 2. The editor (brief E2)

**Layer.** A Form (3.5). Desktop and medium: `Dialog` with `DialogContent size="form"` (640), header with `DialogOverline` (`NEW LOGIN // PERSONAL VAULT`, `EDIT LOGIN // PERSONAL VAULT`) and `DialogTitle`, which follows what is typed (a login's website as its host, a Wi-Fi network's name, else the title; an edit shows the item's title), the body, a sticky `DialogFooter`: on an edit the caption "Saves a new version. The old one stays in History.", then CANCEL (secondary) and the primary with the checkmark: SAVE {KIND} for a new item (SAVE LOGIN, SAVE WI-FI NETWORK), SAVE for an edit. Phone: `Sheet` with `SheetContent height="full"` and a `TopBar` (close `FloatButton`, the screen's name in the subtitle role as the `h1`, the checkmark `FloatButton kind="primary"` as the form's submit); the top bar stays in place while the keyboard is up, so the primary does too.

**Zone 1, the essentials.** Always in view:

| Template | Essentials, in order |
|---|---|
| Login | Website · Username (NEW ALIAS at its end when an alias service is set up; desktop only) · Password · Toggle Row "Add a one-time code" ("Paste the set-up link the site gives you when you turn on two-step sign-in."), which reveals the code field · VAULT |
| Payment card | Card number · Name on card · Expires · Security code · VAULT |
| Note | Title · Note · VAULT |
| One-time code | the code's field · VAULT |
| Wi-Fi network, Router | Network name · Password · Security · VAULT |
| Every other template | Title (unless the core works it out) · the fields of its first section · every required field · VAULT |

The VAULT row is a `FieldRow` (stacked on a phone) with CHANGE, an `AdaptiveMenu` of the vaults this person may write to, on a new item with more than one vault; changing it starts the item unfiled. An edit shows the vault without CHANGE (moving is the detail's job).

**Zone 2, MORE OPTIONS.** A `SectionHeader collapsible` (`h2`). Shut on a new item; open on an edit when anything inside holds a value. In order: the caption "A field left empty is taken off the item." (said once; no field repeats it); Title for the kinds whose title is worked out ("Left empty, the website's name is used."); the template's other sections, each under its own Section Header, fields in template order (a login's first website's MATCHES leads its section; ADD WEBSITE adds another, each with its own MATCHES); ORGANIZE: Folder, Tags, Toggle Row "Favorite"; YOUR FIELDS: fields added by hand with their remove control, then ADD FIELD, which opens one line (a label, a select of kinds, ADD); the Toggle Row "Keep as a crown jewel" with today's sentence, or the caption for a template that always is one.

**Secrets** are `SecretField mode="edit"`:

| State | Shows | Text actions |
|---|---|---|
| Typed (and a new secret's start) | masked, SHOW; for a password the core's word on the 3-segment rule (D116) | GENERATE |
| Kept (an edit) | empty, "Left empty, it stays as it is." | GENERATE · REMOVE |
| Made when you save | the line "Made when you save" and the helper `{the core's words} // STRONG` (the template's words until the preview arrives) | OPTIONS · TYPE ONE INSTEAD (KEEP on an edit) |
| To be removed | "Removed when you save." | KEEP |
| A passkey's material | "Made by a site through a passkey provider; kept as it is." | none |

**Errors and states.** Templates loading: three skeleton rows. A kind this build can't edit, or the kinds failed: one line and CLOSE. A save the core refused: a compact critical `Banner` ("Not saved", the core's sentence) above the footer, at the top of the form on a phone; the form keeps what was typed. Saving: the primary keeps its words and shows its spinner, and the fields are read-only. A failed NEW ALIAS: the core's words as the Username field's error line.

**Closing.** Closing with anything changed (Escape, the close button, CANCEL, a press outside) asks the Question "Discard changes?" (unicron `ConfirmDialog`, question 480, a sheet on a phone): "What you typed here isn't saved.", KEEP EDITING (focused) and DISCARD (critical). "Changed" is read from what the form holds now, so a password the window cleared while away (D315) no longer counts. Nothing typed: it closes at once.

## 3. The generator's options (brief E3)

`GeneratorPicker`: unicron `AdaptivePopover` (a popover with `PopoverHeader` on a desktop, a sheet on a phone), overline GENERATOR, title "Password options", opened by OPTIONS beside a secret that is set to be made. Inside, `GeneratorOptions` (export and props kept; `onUsualRule` is new): Make (characters or words), then for characters Length (`TextField type="number"`, 8 to 128, with steppers), Toggle Rows Digits, Symbols, No look-alikes (its sentence), Text Field "Leave out"; for words Words (3 to 20, with steppers), Between words (Hyphens, Spaces, Periods, Underscores), Toggle Rows "A capital on each word", "A digit on one word". Under them one polite status line from the core's preview, `{words} // about {bits} bits` and a `Badge` with the strength word (critical, warning or success tone), and a site's own rules when they apply instead (D236). USUAL RULE goes back to the template's rule. No DONE: each change applies as made; Escape, a press outside or the sheet's close closes it.

## 4. Your templates (brief E4)

`TemplatesManager` (export and props kept): one `AdaptiveDialog` (form 640; a full-height sheet on a phone) in two steps.

1. **The list.** Overline `YOUR TEMPLATES 01`, title "Your templates", one line, then one compact `ListRow` per template: its name, `02 FIELDS`, what it's for, and CHANGE and DELETE (text buttons named "Change Bike", "Delete Bike"). Footer CLOSE and NEW TEMPLATE. None yet: `EmptyState` `YOUR TEMPLATES // 00`, "No templates yet.", the line, NEW TEMPLATE.
2. **The designer** (`TemplateDesigner`). Overline `YOUR TEMPLATES // NEW` (or `// CHANGE`), the title "New template" or the template's name; Text Fields Name and "What it's for"; `FIELDS 01` as a Section Header; one line per field: its label, a select of kinds, a Checkbox "Needed", Move up, Move down (named "Move field 2 up"; focus stays on the field that moved), drop; ADD A FIELD. Footer BACK and SAVE TEMPLATE.

DELETE asks first: unicron `ConfirmDialog` over the layer, "Delete Bike?", "It goes to Recently deleted like an item. Items you made from it stay as they are.", CANCEL focused, DELETE critical.

## 5. Hide your email (`features/bridges/alias-field.tsx`)

`AliasField` (export kept; `onError` is new) is now the Username field's NEW ALIAS: a ghost Button S at the field's end ("New alias from SimpleLogin" with one service; a menu of the services with more). The address goes into the user name and a toast names the service; a failure is the field's error line in the core's words. The web vault has no bridges client, so nothing is drawn there. The `Use` gate is unchanged.

## 6. Gaps

### unicron host-contract gaps

1. **`SecretField` text actions are plain buttons with no trigger hook.** OPTIONS cannot be the generator popover's trigger, so the popover stands by an unseen point at the field's foot and focus goes back to the field's row when it closes, not to OPTIONS. Ask: a `render` or `ref` on a text action. Screen: `new-item--generator`.
2. **`AdaptiveMenu` has no submenu part.** The groups are `DropdownMenuSub` on a desktop and `SectionHeader collapsible` in the sheet, chosen with `useAdaptiveSheet`. Ask: `AdaptiveMenuSub` (a submenu on a desktop, a disclosure in the sheet). Screens: `new-item--menu`, `new-item--type-sheet-group`.
3. **No full-screen form layer for a phone.** The editor is a `Sheet height="full"` without its close button, with a `TopBar` inside, padded by a plain wrapper; the sheet keeps its grabber, which Mobile / 07 does not draw. Ask: `AdaptiveDialog presentation="screen"` (Form 3.5: the TopBar with close, the title and the one primary FloatButton). Screens: `new-item--login`, `new-item--edit-login`, `new-item--save-error` (phone).
4. **The sheet's title names the dialog**, so the type sheet's title stays "New item" (see Lockdown gaps). Screen: `new-item--type-sheet`.

### Lockdown gaps

- **The type sheet says "New item", not "What are you saving?"** (Mobile / 06). `app/shell.test.tsx` (foundation, frozen in the lanes) finds the sheet by the name "New item". The fold can change the title to the brief's words with that one line of the test.
- **Make is a select, not CHARACTERS · WORDS tabs** (brief E3). `apps/web/e2e/web-vault.spec.ts` (frozen) chooses it with `selectOption`. The fold can swap it for `Tabs variant="line"` with that line of the spec.
- **The kinds didn't load: no TRY AGAIN.** `useTemplates()` (app/templates.tsx, foundation) has no way to ask again; a TRY AGAIN that did nothing would be a dead control. Ask the foundation for `retryTemplates`.
- **A new login's password starts empty, not "made when you save".** The brief makes generating the default; `finish.spec.ts` (frozen) types a password into a new login at once (the D91 memory test). GENERATE is one press away, and the helper says what it makes. See D974.
- **Counts read the list once each time the menu opens** because the shell (frozen) does not pass its rows to `NewItemMenu`. The fold can pass `rows` from `CreateMenu` and the read goes.
- **Template names are the core's** ("Payment card", "Note", "Driver's licence"): the brief and the frames say "Card" and "Secure note" (found-3's question to Sean stands).

## 7. Tests and what is not proven

- `new-item-menu.test.tsx`: five kinds first, then the groups; no Passkey anywhere; counts in two digits, said as the plain number, trashed items not counted; a kind first in its own view (and out of its group); a variant from its group's submenu; your templates and the foot's "Your templates"; **every creatable template is in the menu exactly once**.
- `item-editor.test.tsx`: Wi-Fi (the title follows the network name; GENERATE says what it makes; More options shut, the empty-field rule said once; SAVE WI-FI NETWORK; what is sent); a card's edit (the overline, the History caption, kept secrets, the core's refusal in the Banner); a field of your own and a wallet's crown-jewel caption; a typed password's strength word, once per pause, bytes zeroed; no meter for a card number; loading; **a login's essentials with the website prefill from Search and the code behind its switch, no "scan"**; **"Discard changes?" with focus on KEEP EDITING, Escape keeping the form, DISCARD closing**; closing at once with nothing typed.
- `editor-organize.test.tsx`: a second website with its own MATCHES, a folder and a tag inside More options; **CHANGE on the VAULT row moves a new item to another vault and starts it unfiled**.
- `vault-depth-editor.test.tsx`: a variant; your own template (its title in More options); **the generator as a picker with no DONE**, the core's preview and Badge, changes kept when it closes; a link to another item and a code of your own.
- `template-designer.test.tsx`: the Empty State; ordered fields with Needed as a Checkbox; **focus stays on the moved field**; a change keeps keys; **delete asks over the layer with focus on CANCEL**.
- `editor-model.test.tsx`: the essentials per kind; what holds a value; a kind's name in a sentence and a website as its host.
- `alias-field.test.tsx`: nothing without a bridges client; an alias through the gate into the user name.
- `features/hygiene/hygiene.test.tsx` (found-5, unchanged) and `apps/web/e2e/finish.spec.ts`'s typed-password memory test (through `steps/editor.ts`): the D315 wipe still clears the editor's password.
- `apps/web/e2e` through `steps/editor.ts`: create and edit a login, a passport from its group, your own template and an item from it, a Wi-Fi network made as a passphrase with the picker, a wallet.
- Not proven: a real phone keyboard over the full-screen form; VoiceOver and TalkBack on the sheet's disclosures; WebKit.

## 8. Proposed decisions

| # | Decision | Status |
|---|---|---|
| D970 | **New item lists five kinds first: Login, Payment card, Note, One-time code, Wi-Fi network** (Q18). Passkey is in no create list; Wi-Fi network takes its row. On Codes One-time code leads; in a kind's view that kind leads. | Proposed; **flagged for Sean** (Mobile / 06 draws Passkey) |
| D971 | **Each kind in the menu shows its count of items in two digits** (cross-app §13), counted from the rows; the frame's three-digit counts change (6.3). | Proposed; **flagged for Sean** |
| D972 | **Every other template sits in the code's four groups**: submenus on a desktop, closed Section Headers on a phone; YOUR TEMPLATES after them, "Your templates" at the foot (E1, D231, D232). | Proposed; **flagged for Sean** |
| D973 | **The editor is a Form**: a 640 dialog from the medium width up, a full-height layer with close, the screen's name and the checkmark on a phone (Mobile / 07, Desktop / 05). The dialog's title follows the website, the network's name or the title as typed; the primary says SAVE {KIND} on a new item and SAVE on an edit. | Proposed; **flagged for Sean** |
| D974 | **Essentials first, the rest under MORE OPTIONS** (R10), shut on a new item and open on an edit that holds anything inside; the empty-field rule said once. A new login's password still starts typed, not generated (the brief's default would need `finish.spec.ts` changed; the question stands for Sean, with Mobile / 07's password shown in clear, which is not built: app spec question 11). | Proposed; **flagged for Sean** |
| D975 | **Secrets in the editor are `SecretField` edit mode in five states** (typed, kept, made when you save, to be removed, a passkey's material); a generated value is never shown; the helper says what the options make and the core's strength word. | Proposed; **flagged for Sean** |
| D976 | **"Add a one-time code" is a Toggle Row that reveals the code's field**, with no "scan" (W11). | Proposed; **flagged for Sean** (Mobile / 07 says "Scan the QR code") |
| D977 | **Closing a form with changes asks "Discard changes?"** with KEEP EDITING focused (cross-app §10); "changes" are read from what the form holds, so a D315 wipe leaves nothing to discard. | Proposed |
| D978 | **The generator's options are a picker with no DONE** (E3): a popover with its header on a desktop, a sheet on a phone; the strength is a Badge beside the core's words; separators read Hyphens, Spaces, Periods, Underscores (D230's words, US spelling). | Proposed; **flagged for Sean** |
| D979 | **Your templates is one layer with a list and a designer** (E4, D232): compact rows with CHANGE and DELETE, NEW TEMPLATE; the designer's Needed is a Checkbox and moving a field keeps focus on it; DELETE asks first and sends it to Recently deleted. | Proposed; **flagged for Sean** |
