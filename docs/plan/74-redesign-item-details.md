# 74: An item's Details, its files and its history

_Slice: `vault-6-item-details` of the 2026-11 redesign, on `wt/lockdown-vault` (for `claude/keen-ritchie-q2hgb2`), built 2026-11-15. Briefs I4 (the details zone), I5 (files) and I6 (history); no drawn frame. The slice text named this doc 73 with D810 to D819. vault-5 took both (`73-redesign-search.md`, D810 to D818), so this is doc 74 with D820 to D829. The vault lane runs two slots late; the integrator settles the numbers at the fold._

Status: built. Details is one list of Field Rows under the collapsible Section Header; files are FileChips; history is a form-size dialog with a read-only version and a restore that asks.

## For future agent

- **TL;DR:** `features/vault/detail-pane.tsx` draws the Details zone (the rest of the pane is vault-1's): `PlacePanel` (VAULT with MOVE, FOLDER and TAGS with CHANGE), `ItemProtection` (WHO CAN GET ITS VALUES), `FillFixPanel` (FILLING ON THIS SITE, logins), `AwayBlock` (SIGNED IN ELSEWHERE, logins), CROWN JEWEL (its CHANGE opens the ⋯ menu's own question, `menu.askJewel`), ASK BEFORE THE SECURITY CODE (cards), VERSION with HISTORY, then for a crown jewel `EmergencyAccess` and `ExportOptIn` as Toggle Rows. The shut line is `vault // folder // tier` (`useExposure`, read once for the pane). `features/organization/files-panel.tsx` is the Files section; `history-view.tsx` the History dialog; `organize-prompt.tsx` the Folder and tags Form on `tags-editor.tsx` (ChipField when given a label; the item editor keeps its chips over an input until its lane moves, see Gaps).
- **Read with:** briefs I1 (the frame), I4 to I6, X5 (questions, the gate), 4.0 (layouts, states, data), D121, D237, D245, docs 21, 22, 23 §5 and 69.
- **Do not:** reveal or copy a value of an older version (its hidden values are `SecretField fillOnly`, which never fetches); toast a change a row already shows; draw Files in the web vault (W1); let a crown jewel into the fill fix (never filled).

## 1. Details (brief I4, R8)

Shut by default; the app remembers on this device (unchanged from doc 69). Shut, one line: `Personal // Finance // Fill freely` (the tier only when the person may change the item).

| Row | Value | Action | Shown when |
|---|---|---|---|
| Vault | the vault's name | `MOVE`: a menu of the other vaults (a sheet on a phone). Seals it again, new id, the detail follows; a crown jewel's asks first (gate) | the item has a vault; MOVE with two vaults or more, on a live item the person may change |
| Folder | the path, or "None" | `CHANGE`: Folder and tags | a live, unarchived item the person may change, or one with a folder |
| Tags | `#work` chips, or "None" | `CHANGE`: the same Form | as Folder |
| Who can get its values | Fill freely, Ask each time, Keep to the app; under it the core's reason while the tier is not chosen | `CHANGE`: a popover with its header (the item's title over "Who can get its values"; a sheet on a phone) of the tiers the core offers as RadioRows, each with its sentence. Asking less asks first (gate). Silent on success | the host has a depth client, the person may change the item, not in Recently deleted, not fill-only |
| Filling on this site | Lockdown decides, Never fills here, "2 fields fixed" | `CHANGE`: the Form below | logins, with a bridges client (not the web), not a crown jewel |
| Signed in elsewhere | "Nowhere", or one row each: `Hotel TV // reminds 2026.11.20` with the `DUE` Badge | `ADD` on "Nowhere"; `SIGNED OUT` on each | logins, or anything with a sign-in logged |
| Crown jewel | Yes, No, Always | `CHANGE`: the ⋯ menu's question (today's words; stopping asks for the master password through the gate) | a live item the person may change; no CHANGE for Always |
| Ask before the security code | Yes or No | none | cards |
| Version | `7 // edited 2026.11.08` | `HISTORY` | always; HISTORY only for someone who may change the item |
| Include in emergency access | Toggle Row, its sentence | the switch; on asks first (gate) | a live crown jewel |
| Include in exports and backups | Toggle Row, its sentence | the switch; on asks first (gate) | a crown jewel; in the web vault the switch stays still and says "Exports and backups are made in the desktop app." |

A share the person can only view shows Vault (when known), Version and nothing to press. SHARED WITH and FROM are not drawn: see Gaps.

**The three small Forms** (AdaptiveDialog: form 640 on a desktop, a sheet on a phone; one primary):

- **Folder and tags.** Folder (a select of the vault's folders as a tree; "No folder"), Tags (ChipField; known tags offered; "A slash nests one tag under another: work/clients."), Toggle Row Favorite. `SAVE`. A failure is a compact critical Banner at the top of the form.
- **Filling on {host}.** Toggle Row "Never fill on this site", Section Header `FIELDS FIXED 02` with one row per fix and REMOVE, then "Field name on the page", "What it is" and `ADD FIX`. The description says the extension's popup can set these too. `DONE`. Each change saves at once.
- **Signing in on a TV?** "Where" (placeholder "Hotel room TV"), "Remind me" (the code's five choices). `REMIND ME`.

## 2. Files (brief I5)

Section Header `FILES 02` with `ATTACH` (the shell's file dialog, a chunk at a time, D233; the system picker where that is not offered). One `FileChip` per file: the type tag (the name's extension, else the media type), the name, the size. Pressing the chip saves the file where the person chooses (named "Save {name}"; busy while it saves). Its trailing × removes it after a Question: "Remove {name}?", "The item saves a new version without it. Earlier versions in its history still hold the file.", REMOVE in the destructive tone, focus on Cancel. Under the chips: "1.2 GB of 5 GB used for files on your server." when the account has a server. None and editable: "No files."; none and view only: nothing. The web vault draws no Files section.

## 3. History (brief I6)

A dialog, form 640 (a full-height sheet on a phone). Overline `History // {title}`, title "7 versions", caption "Every save is a version. Going back saves the old one again as the newest." One `ListRow` per version, newest first: `Version 7` with `CURRENT`, the mono line `2026.11.08 14:02 // MacBook`, "Changed: Password, Website" (labels only). `SHOW` / `HIDE` opens the version under its row as stacked Field Rows; each hidden value is a `SecretField` in its fill-only form, "Hidden here. Restore this version to use it.", which never fetches. `RESTORE` (not on the current version, not without the right to change) opens a Question over the dialog in the default tone; a crown jewel's asks first (gate); then the toast "Version 4 of Steam is back as version 8". A version whose keys are gone shows `DELETED FOR GOOD` and no actions. Loading: three skeleton rows.

## 4. Words

- Toasts name the thing, no full stop: "Attached policy.pdf", "Removed policy.pdf", "Saved policy.pdf to Downloads", "Moved Bank to Family", "Lockdown reminds you to sign out of Meadowlark Streaming on Hotel TV", "Marked signed out of Meadowlark Streaming on Hotel TV".
- Silent: the tier, the fill fix, the two crown-jewel switches. The row or the switch is the answer (cross-app §5's save rule).
- US spelling: Favorite, Favorites.

## 5. Pictures

`$S/shots/built/lockdown/undrawn/item--{details,organize,fill-fix,protection,away,files,files-remove,history,history-version,crown-jewel-ask}__{phone,desktop}__{graphite,stone}.png`, from the states `desktop-item-login-away`, `desktop-vault-organize`, `desktop-item-login-away-fill-fix`, `desktop-item-login-away-protection`, `desktop-item-away-dialog`, `desktop-item-insurance`, `desktop-item-file-remove`, `desktop-item-history`, `desktop-item-history-version` and `desktop-item-jewel-on`.

## 6. Tests, and what is not proven

- `place-panel.test.tsx` (rewritten): the rows, a crown jewel's MOVE through the gate, CHANGE → Folder and tags → one `organize` call (a tag in other letters kept once), nothing to press without the right.
- `files-panel.test.tsx`: the chip's type and size; **a removal asks** (focus on Cancel, nothing removed before the yes); view only; the web draws nothing.
- `history-view.test.tsx` (new): the list newest first; SHOW never reveals; **a restore asks** (focus on Cancel) and says what came back; no restore without the right.
- `item-protection.test.tsx`: the picker's RadioRows; the default clears the choice and shows the core's reason; not drawn without the right; `EmergencyAccess` through the gate.
- `export-opt-in.test.tsx`: the gate as before; the web's still switch with its reason.
- `fill-fix.test.tsx`: the row's words, the Form, a fix by name, never fill here, remove; a crown jewel draws nothing; no CHANGE without the right.
- `detail-pane.test.tsx`: **Details lists only the rows that apply** (a login, a note, a card, a view-only share, a crown jewel, a fixed crown jewel); the shut line names the tier; the Security box test counts the tier row instead of the old Protection box.
- `vault-depth-detail.test.tsx`: the sign-in row with DUE and SIGNED OUT, a new one logged from the ⋯ menu; ATTACH.
- Not proven: the gate's own prompt on a real Touch ID; the OS save dialog; a screen reader through the tier popover.

## 7. Gaps

- **SHARED WITH** (`SentShareView`, STOP) is not drawn: sharing's sent list is edges-8's, and the item carries no list of its shares. ItemShare keeps its own button.
- **FROM** on a share this person can only view: the item carries no sender. `AccessNote` stays.
- **Files right after a Document's first section** (I5's entry) is the Use zone's order, vault-1's; Files stays after the fields.
- **The card's caption** "This card asks it's you before its security code…" in the Use zone now repeats the Details row; it is vault-1's part.
- The item editor's tags stay chips over an input: ChipField's own status region would be a second `status` in the editor dialog, which edges' web e2e step for the generator strength reads as the only one.
- The web vault draws the export switch, still, with its reason: the brief says not drawn, and the foundation's web e2e reads its words.

### unicron host-contract gaps

1. No adaptive radio picker: the tier picker is `AdaptivePopover` (overline the item's title, title "Who can get its values") holding a `RadioGroup` of `RadioRow`s, closed by the app on a choice. A picker part that closes on a choice and keeps focus on the chosen row would make this one call. Screen: Details → Who can get its values.
2. `FileChip` names its main button by its content (type, name, size), with no verb; the app passes `mainRender` with an `aria-label` "Save {name}". A `mainLabel` prop would keep the size in the name. Screen: Files.
3. `SecretField` has no "read-only, never revealable" mode of its own; the history uses `fillOnly` with its own sentence. Screen: History → one version.

## 8. Proposed decisions

- **D820.** Details is one list of Field Rows in the brief's order, each only where it applies and the person may change it; the shut line names the vault, the folder and the tier. Answers R8. Flag for Sean (D38).
- **D821.** The fill fix moves into a Form behind FILLING ON THIS SITE → CHANGE; each change saves at once and DONE closes. Answers R9. Flag for Sean.
- **D822.** Changing the tier, the fill fix or a crown jewel's two switches raises no toast: the row is the answer (cross-app §5's save rule applied to an item's settings). Flag for Sean.
- **D823.** SIGNED IN ELSEWHERE is one row per sign-in with SIGNED OUT; ADD shows on "Nowhere", and the ⋯ menu's "Signing in on a TV?" logs another once one is there. Flag for Sean.
- **D824.** The CROWN JEWEL row's CHANGE asks the ⋯ menu's question with today's words; one question, two ways in. Flag for Sean.
- **D825.** Files are FileChips: the chip saves, the × removes after a Question in the destructive tone; the web vault draws no Files. Answers I5. Flag for Sean.
- **D826.** History is a form-size dialog; SHOW draws an older version with its hidden values as fill-only SecretFields that never fetch; RESTORE asks in the default tone. Answers I6. Flag for Sean.
- **D827.** The web vault keeps the crown jewel's export switch, still, with "Exports and backups are made in the desktop app." (the brief says not drawn; the web e2e reads it). Question for Sean: draw it or not. **Amended by D1022 (Lockdown 22, decided 2026-11-16):** not drawn. The web vault shows no export switch; the e2e checks it is absent.
- **D828.** The tier picker marks the core's default as "(usual)" in its RadioRows. Flag for Sean.
