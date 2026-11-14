# 68: the needs-you slot, the gate, Lock and the Question rule

_Slice: `found-6` of the 2026-11 redesign, on `claude/keen-ritchie-q2hgb2`, built 2026-11-14. Briefs X3 (needs-you) and X5 (confirmations, the gate and Lock) of the undrawn-areas brief, and the cross-app decisions §1, §2, §4 and §10. The slice text named this doc 67 and its decisions D750 to D759; both were taken by `67-redesign-messages.md` (found-5), so this is doc 68 with D760 to D769._

## For future agent

- **TL;DR:** what waits on the person registers with `useNeedsYouSource(entry)` (`app/needs-you.tsx`); the shell draws the first true entry by the X3 table, once per screen. The gate is unicron's `CrownJewelGate` on the password route and a `ConfirmDialog` on the window route (`app/gate.tsx`). Lock draws the unlock screen alone and closes every layer.
- **Do:**
  - Register a source with `useNeedsYouSource({ id, priority, title, line, action, textAction, shows, expiresAt, expiredWords })`. Rows 1 to 3 (sign-in, pairing, device) are edges-1's; row 4 edges-8's; row 5 (conflict, `shows: { item }`) vault-7's; row 6 (reset, `shows: "sharing"`) edges-7's. A source outside React uses `addNeedsYou(entry)`.
  - Give `title` without the time; the shell adds `// 01:40 left` from `expiresAt` and says it once, and again at 30 s.
  - Put `<WaitingOnYou />` on the screen that owns rows 5 and 6 (the item detail, Sharing): it draws the screen's own lower entries as ordinary rows under WAITING ON YOU NN (`useWaitingRows()` for a custom layout).
  - Apply §3's table to your own confirms with `ConfirmDialog`.
- **Do not:**
  - Draw a second yellow shape, a close control, or a swipe-away on a needs-you.
  - Put a solid primary within 96 px of the block in the content.
  - Open a dialog by itself for a request: it waits behind REVIEW (Q3).
  - Hand-build a gate or a confirm: use `useGate().run` and `ConfirmDialog`.
- **Read with:** brief X3, X5 and 4.0 (workspace `07-projects/29-unicron/impl-2026-11/app-lockdown-undrawn.md`), cross-app decisions §1, §2, §4, §10, doc 67 (the other four channels).

## 1. Needs-you (brief X3)

| Width | Where | Part |
|---|---|---|
| expanded, medium | the top of the main content column: the detail pane beside a list (`MasterDetail`), the one column where the detail is pushed, under the page head on a page (`PageColumn`, through the head's `below`), the top of the section in Settings (`SettingsPlaces`) | `NeedsYou`, its text action `needs-you-text` |
| phone | the status strip's slot, ahead of the live status (`StatusStrip outranked`); scrolled into the bottom row it keeps its yellow (Q6) | `AppFrameAccessory tone="needs-you"`: the text area one button and the action a sibling (C5). REVIEW for rows 1 to 4 runs the entry's action; RESOLVE (row 5) and OPEN (rows 6 to 9) open a sheet with the line, the solid action as the layer's primary and the text action as text |
| a dialog or sheet is open | rows 1 to 3 only: the compact line (`NeedsYou size="compact"`, C19) stands on the layer's top edge in a portal of its own, put in a moment after the layer opened, so no dialog changes and the layer's modal does not count a press on REVIEW as a press outside. Unicron hides the block behind; the strip steps aside too | `NeedsYou size="compact"` |
| a full-screen form on a phone | the strip at the form's bottom edge, for every row | `Accessory tone="needs-you"` |

- The first true entry shows; `// +1` when more wait; a screen's own lower item (rows 5, 6) is a row under WAITING ON YOU NN.
- A request that runs out leaves by itself, and the toast says `expiredWords` ("Sign-in request ran out").
- The host lives in the unlocked branch: nothing of it draws while locked; what arrives while locked shows after unlock.
- **Sources wired here: the reminders, rows 7 to 9** (`features/depth/reminders.tsx`). They are no longer a dialog after unlock (Q4):

| Row | Title | Line | Solid | Text |
|---|---|---|---|---|
| 7 | RECOVERY KIT // NOT TESTED | Test your recovery kit. | TEST IT NOW (the kit test) | SKIP: a Question, "Skip the kit test?", whose one yes is "I understand an untested kit can fail" (D251, D383); it asks again in a week |
| 8 | MASTER PASSWORD // 14 DAYS | Type your master password once more. | TYPE IT: a one-field Form (`SecretField` edit, "Master password", CHECK); checked on this device | TOMORROW (D252) |
| 9 | RECOVERY DRILL // DUE | Time for the recovery drill. | RUN DRILL (the kit test) | NEXT WEEK (D253) |

A failed answer replaces the line with the core's words, on the block.

## 2. The gate (brief X5)

- Password route: unicron's `CrownJewelGate` (a question 480 dialog, a sheet on a phone), its field focused. The reason reads "To export the vault." from the core's message. A wrong try: "That isn't the master password. 2 tries left." The third locks the vault and the call fails with "The password was wrong three times, so the vault locked."
- Window route (no account, no platform check): a `ConfirmDialog`, overline CONFIRM IT'S YOU, the question "Export the vault?", one line that says nothing is checked, CANCEL (focused) and the action's own verb (EXPORT, COPY, BACK UP, LET THEM BACK IN; CONTINUE when the core names no action).
- Escape and Cancel back out; nothing is confirmed. Backing out of a copy or a show says "Nothing was copied" or "Nothing was shown".
- `useGate().dismiss()` closes it as a cancel; the app calls it on every lock.

## 3. The Question rule (brief X5; each lane applies it to its own confirms)

| Action | Asks? |
|---|---|
| Move to Recently deleted; archive; remove from favorites; out of a folder; delete an extra duplicate | No: a toast with UNDO (Q14) |
| Delete for good; delete a passkey; remove a file or folder; take a tag off every item; merge duplicates | Yes, destructive |
| Revoke a device; end a session or all others; remove a member; forget a person; delete a link; end emergency access; unpair a browser; remove a security key | Yes, destructive, never gated (D196, D221) |
| Keep this copy; restore a version; replace the kit; turn off backups; move the vault to a server | Yes, default |
| Close a form with typed text | "Discard changes?", DISCARD and KEEP EDITING (focus) |
| Name prompts (new vault, rename, new folder, rename tag) | No question: a one-field Form, CREATE VAULT, RENAME, CREATE FOLDER |

`ConfirmDialog` everywhere: question 480 or a sheet, focus on CANCEL, the yes repeats the verb. `LabelledActions` stays until the fold slice.

## 4. Lock

No question and no animation. The locked branch draws the unlock screen alone: `LockedWash` left `App.tsx` and `web-app.tsx` (Q15; screens-1 deletes `locked-wash.tsx` and its test). On a lock the gate is dismissed, the palette closes, and the unlocked tree (every dialog, sheet and menu in it) unmounts in the same render. The quick-access window hides when it sees the vault lock (polled every 3 s while open and unlocked).

## 5. Tests

`app/needs-you.test.tsx` (priority, `// +1`, one yellow shape, waiting rows, run out, strip with no close and no swipe, the compact line over a dialog), `app/gate.test.tsx` (three tries, words, window route, Escape, dismiss), `app/locked.test.tsx` (nothing of the shell while locked), `features/depth/reminders.test.tsx` (the words and skip rules). Tests that named the old window confirm ("Are you sure?", "Confirm") and the old wrong-try words were updated in this slice.

States: `desktop-reminder-kit-test`, `-password`, `-drill` and `desktop-item-wallet-gate` (ids kept), new `shell-needs-you-block`, `shell-needs-you-strip`, `shell-needs-you-under-dialog` (its sign-in request is the fixture's, added by the harness with `addNeedsYou` until edges-1 wires row 1).

## 6. Gaps

### unicron gaps

1. **`CrownJewelGate` fixes its words:** overline "Crown jewel", title "Confirm it's you", field "Account password". X5 asks for overline CONFIRM IT'S YOU, the reason as the title and "Master password". Ask: `overline`, `title` and `fieldLabel` props. Screen: the gate.
2. **The compact line needs each layer's `notice`.** A portal over the layer is outside the layer's focus trap, so Tab cannot reach REVIEW while the layer is open (a pointer and the announcement can). Ask: a frame-level notice slot that every open `DialogContent`/`SheetContent` draws by itself. Screen: a request under any dialog.
3. **No needs-you place in the frame:** the app puts the block in the detail, under the page head and in Settings itself. Ask: an `AppFrame` needs-you slot per column.

### Lockdown gaps

- The lock screen's sentence after three wrong tries: `UnlockScreen` takes only the core's `LockReason`, so it says "Locked." (screens lane, F1).
- Rows 1 to 6 are other lanes' sources.

## 7. Proposed decisions

| Id | Decision | Status |
|---|---|---|
| D760 | **The recovery reminders are needs-you rows 7 to 9, not a dialog after unlock** (Q4; D251, D252, D253 kept word for word). SKIP on the kit test asks a question whose one yes is "I understand an untested kit can fail". Plain version: the reminders wait in the yellow block instead of popping up when you unlock. | Proposed; **flagged for Sean** (Q4) |
| D761 | **One needs-you on every screen** (Q5, cross-app question 1): the block at the top of the content column on a desktop, the strip on a phone; `// +1` when more wait. Plain version: what waits on you is always in the same place, wherever you are. | Proposed; **flagged for Sean** |
| D762 | **The strip keeps its yellow in the scrolled bottom row** (Q6, departs from States / 02), and a request that runs out shows as a compact line over an open dialog or sheet. Plain version: a waiting request never turns grey or hides behind a dialog. | Proposed; **flagged for Sean** (Q6) |
| D763 | **The window route says nothing is checked, and its yes is the action's verb**; a wrong password says "That isn't the master password. 2 tries left." Plain version: the confirm on a computer with no check is honest about it. | Proposed |
| D764 | **Locked draws the unlock screen alone** (Q15; changes D107's wash). Every layer closes on lock, the gate and the quick-access window included. Plain version: nothing of your vault's shape shows while it is locked. | Proposed; **flagged for Sean** (Q15) |
