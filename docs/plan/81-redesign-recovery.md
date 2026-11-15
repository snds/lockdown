# 81: Recovery: the kit, the drill, the kit page shown once, the emergency sheet

_Slice: `screens-6` of the 2026-11 redesign, on `wt/lockdown-screens` (from `claude/keen-ritchie-q2hgb2`, after doc 80), built 2026-11-15. No drawn frame; brief T3 of the undrawn-areas brief (workspace `07-projects/29-unicron/impl-2026-11/app-lockdown-undrawn.md`), with 4.0 (layouts by name, themes, states), X3 (rows 7 and 9), X5 (questions); cross-app decisions §5, §9, §10, §12, §13, §14; contract 5.1b #17, #37. Decisions D890 to D899. Status: built in the lane; not merged._

## For future agent

- **TL;DR:** Settings → Recovery is rows now, not cards: RECOVERY KIT (NEW KIT), RECOVERY DRILL (RUN DRILL), MASTER PASSWORD REMINDER (chips), EMERGENCY SHEET (OPEN), EMERGENCY ACCESS (OPEN), then WHAT THE DRILL CHECKS NN. NEW KIT over a kit that exists asks first. The drill is a form of SecretFields. The kit page and the emergency sheet are viewers (960; the whole window below that) with the page on paper, black on the stone ground in both themes, printed through the existing path.
- **Files:** `features/recovery/recovery-view.tsx` (the rows, the question, the flow), `features/account/kit-dialog.tsx` (the drill form and the new-kit password form), `features/depth/reminders.tsx` (`useRecoveryFacts`, `DrillSection`, `PasswordReminderSetting`; `RemindersPrompt` is found-6's and unchanged), `features/status/once-page.tsx` (`OncePage`, `OnPaper`), `features/bridges/emergency-sheet.tsx` (`EmergencySheetRow`, `EmergencySheetViewer`).
- **Do:**
  - Keep `RecoveryView`'s props (Settings mounts it) and `OncePage`'s (`App.tsx`, Status and Recovery mount it). `KitDialog` only gains (`replace`); found-6's reminder rows 7 and 9 open its drill form.
  - Put a page to print inside `OnPaper` and the viewer's `DialogContent` with `data-print-page`: the print rules in `styles/app-extension.css` (foundation) hide the app, the overlay and the footer.
  - Show a core refusal where it belongs: under the drill field it names (`drillFieldOf`), else a compact critical Banner in the form; a failed read under the row or as a Banner with TRY AGAIN. Never a toast.
- **Do not:**
  - Toast a new kit's notes: they are on the kit page.
  - Let the kit page close on a press outside: it is shown once.
  - Draw a count of emergency contacts: the core sends none here (§6).
- **Read with:** doc 79 (`SettingsFrame`, `setting-rows.tsx`), doc 68 (needs-you rows 7 to 9), doc 39 (the kit at first run), doc 22 §7 (the reminders and the drill), doc 23 §10 (the emergency sheet), doc 38 §3 and §4 (print and Save as PDF).

## 1. The rows (brief T3)

| Row | Value | Action | Data |
|---|---|---|---|
| `RECOVERY KIT` | "None yet", "Made, not tested", or "Tested 2026.10.01" (the last drill's date; "Tested" when the reminders have not loaded) | `NEW KIT` (§2) | `Status.recovery`, `DrillReminder.last_drilled_at` |
| `RECOVERY DRILL` | "Passed 2026.10.01" or "Not run yet" | `RUN DRILL` (§3); with no kit, disabled with "Make a kit first." under the row (#37) | the same |
| `MASTER PASSWORD REMINDER` | chips 7 DAYS · 14 DAYS · 30 DAYS, "Only here, where unlocking doesn't ask for it." Only when `password.applies` | a pick saves at once; a refusal keeps the old chip with the core's words under the row | `PasswordReminder`, `setPasswordReminderDays` |
| `EMERGENCY SHEET` | "One page for a future you. No secrets on it." | `OPEN` (§5) | `bridges.emergencySheet()` |
| `EMERGENCY ACCESS` | "Someone you trust can ask for your vaults, after a wait you can stop." | `OPEN`: Sharing → Emergency access (S5) | `go("sharing-emergency")` |

Then, when a kit exists, `WHAT THE DRILL CHECKS NN`: one `ListRow` (compact) per check the core lists, its `words` and a `Badge` OK (neutral) or CHECK (warning); the word carries it. A failed read is a compact critical `Banner` with TRY AGAIN, over the rows when some loaded.

Rows are `FieldRow`s, stacked on a phone (`useRowSize`), inside doc 79's section head ("Recovery", "Your recovery kit and the ways back in."). No primary in the section: the frame's NEW ITEM is the shell's.

## 2. A new kit

- **No kit, no account:** NEW KIT makes it at once (the action keeps its label while working), then the kit page.
- **No kit, an account:** NEW KIT opens "New recovery kit" (form 640; a sheet on a phone): `SecretField` "Master password", because the account's slot takes the new Secret Key. CREATE KIT. Then the kit page.
- **A kit exists:** NEW KIT asks first: `ConfirmDialog` (question 480), overline RECOVERY KIT, "Replace your recovery kit?", "Your old kit page stops working.", focus on CANCEL, the yes REPLACE KIT in the default tone (X5: a choice worth a second look, not a loss of data). Then the password form with an account, or the kit at once without; then the kit page. A refusal shows in the question (or under the password).
- Before: one dialog with a switch "Replace the kit that exists (the old page stops working)", and a core error when the switch was off over an existing kit.

## 3. The drill (brief T3)

`KitDialog kind="kit-verify"`: overline RECOVERY DRILL, "Type your recovery kit", "Lockdown checks it opens this vault. Nothing changes unless you record the drill." `SecretField`s "Secret Key" and "Recovery code" (mono), `SecretField` "Master password" with an account ("Optional. Also checks the Secret Key against your account."), `ToggleRow` "Record a passing check as the drill" (on). CANCEL and CHECK KIT; CHECK KIT waits with "Type both lines from your kit first." until both are typed. It does not close on a press outside.

- **Fails:** the form stays; the core's sentence sits under the field it names first ("the recovery code didn't open this identity…" under Recovery code; "the Secret Key and password didn't open the account slot…" under Secret Key), or in a compact critical Banner above the buttons when it names none.
- **Passes:** the form closes; a toast with the core's lines (as before); the rows and the checks read again.
- The same form opens from needs-you rows 7 (TEST IT NOW) and 9 (RUN DRILL), found-6.

## 4. The kit page, shown once (brief T3)

`OncePage`, a viewer: `DialogContent size="viewer"` (960; below its width unicron's fitted size takes the whole window less the margin). Overline RECOVERY KIT // SHOWN ONCE, the title, "Shown once and never kept. Print it, save it as a file or write it down.", the core's notes. Then the page on paper (`OnPaper`): the Secret Key and the recovery code in `LargeTypeView`'s print variant, every character captioned. "The page as text" is a collapsible `SectionHeader`, shut, on screen; on paper the text prints whole (a copy that shows only in print). The file's note (before Save as PDF), "Saved where you chose…" and a refusal stay off the paper. Footer, sticky: PRINT and SAVE AS PDF… (secondary; Save only where the host can keep a file), I HAVE IT (the one primary).

- Opens at its top (focus on the dialog, named by its title), not on the first control further down.
- **Held out of screen captures while open** (`useCaptureProtect`, D316). Before this slice the once page did not ask for it.
- A press outside does not close it.
- Print and Save as PDF are unchanged (doc 38 §3, §4).

## 5. The emergency sheet (brief T3)

`EmergencySheetRow` in Recovery; OPEN reads the sheet and shows `EmergencySheetViewer`: the same viewer, overline EMERGENCY SHEET // NO SECRETS ON IT, the core's title and intro, then on paper "Made 2026.10.29", each section as a heading with its lines, and each blank with a rule to write on. PRINT (secondary) and DONE (primary). A sheet the core could not make is one critical line under the row (before: a toast). The web vault has no bridges client and draws no row.

## 6. Paper

`OnPaper` is an `article` with the `light` class, as unicron's `QrCode` scopes itself: the stone ground and its ink in both themes, so the page reads black on (near) white on screen, as on paper. The print rules make it `#ffffff` / `#000000` (foundation's `app-extension.css`). It adds no padding: `LargeTypeView`'s print variant has its own.

## 7. Tests and what is not proven

- `recovery-view.test.tsx`: the kit's words for none, made and drilled with a date; the rows, the drill's date and the checks as a list (R7); emergency access with no "later" and OPEN (R5); **replacing the kit asks first, Cancel changes nothing, the yes is the default tone and makes a kit with `replace: true`, then the kit page**; with an account, the question then the master password; with no kit, NEW KIT makes one at once and RUN DRILL is disabled with its reason; a failed drill keeps the form with the core's sentence under Recovery code only; a passing drill toasts the core's lines and closes.
- `kit-dialog.tsx` is tested through `recovery-view.test.tsx` and found-6's `reminders.test.tsx`.
- `reminders.test.tsx`, the drill part (new): the checks with OK and CHECK and their count; the interval chips, saved at once; a refused interval keeps the old chip with the core's words; no interval where unlocking asks anyway; a failed read of the checks is a Banner whose TRY AGAIN reads them again. Rows 7 to 9's tests are unchanged.
- `once-page.test.tsx`: the existing large-type, print and Save as PDF tests, unchanged in what they assert; the page as text now opens before it is read; new: the viewer size, the overline, the paper, one primary, focus on the dialog, and capture protection on while open and off on close.
- `emergency-sheet.test.tsx`: no row without bridges; the row's OPEN shows the sheet on paper, prints through the host and closes with DONE; a failed read is a critical line, not a toast.
- Capture states (strict, `--sizes 1440x900,390x844,768x1024`): `desktop-recovery`, `-drill`, `-drill-failed`, `-kit-replace`, `-kit-page` (new), `-emergency-sheet`; ids kept, each with `built`.
- Not proven: a real print on paper in WebKit (CI's `engines` job prints the kit), a real Save as PDF, a real account's slot check, a screen reader on the viewer's first focus.

## 8. Gaps

1. EMERGENCY ACCESS has no "2 contacts": the core sends no count here. The drill's checks name each contact, but "No emergency contact" is a contact check too, so counting them would lie.
2. "Find a setting" does not list "Emergency access" under Recovery: `features/settings/settings-index.ts` is outside this slice.
3. `RemindersPrompt` (found-6) opens the drill with `hasAccount` always true, so the optional master password shows on a local account too; the core ignores it there.
4. The kit's date: the core keeps no date for a kit that was made but never drilled, so "Made, not tested" has none.

## 9. Unicron host-contract gaps

1. `Dialog size="full"` keeps the dialog margin on a phone, so the viewers are not edge to edge. A `Sheet height="full"` would be, but the print rules target the dialog's slots. Screens: the kit page and the emergency sheet at 390. Ask: a `viewer` that is full bleed in the compact class, or a print contract on the sheet.
2. No paper surface: a page that is black on white in both themes borrows the `light` scope (stone ground, not white). Ask: a `surface-paper` role, or a `Paper` part, shared with the Wi-Fi card and the QR code. Screens: the kit page, the emergency sheet.
3. `LargeTypeView print` writes its date as "October 29, 2026"; decisions §13 and the frames write `2026.10.29`. Ask: a `date` prop or the shared format. Screen: the kit page.
4. `ListRow` has its own side padding, so the drill's checks sit 16 px in from the Field Rows above. Ask: `inset={false}` as Field Row has. Screen: WHAT THE DRILL CHECKS.
5. `AdaptiveDialog` passes no data attributes to its popup, so a viewer that prints uses `Dialog` directly (no sheet form on a phone).
6. `SecretField mode="edit"` is controlled only: the typed Secret Key, recovery code and master password live in React state while the form is open (screens-5 gap 1). They are cleared on close and after each answer. Before, the drill read uncontrolled password inputs through `FormData`. Screen: the drill and the new-kit form.

## 10. Proposed decisions

- **D890.** Recovery is a Settings section of rows (Q2): RECOVERY KIT, RECOVERY DRILL, MASTER PASSWORD REMINDER, EMERGENCY SHEET, EMERGENCY ACCESS, then WHAT THE DRILL CHECKS NN. The cards, their badges and the "Emergency sheet" button in the head go. Seen by people (D38). T3.
- **D891.** NEW KIT over a kit that exists asks "Replace your recovery kit?" / "Your old kit page stops working." with REPLACE KIT in the default tone; then the master password with an account; then the kit page. The switch in the form goes. Without an account, a first kit is made at once. Seen by people (D38). T3, X5 ("replace the recovery kit: yes, default action").
- **D892.** The drill is a form of `SecretField`s with the master password optional and "Record a passing check as the drill" on; CHECK KIT waits with its reason; a refusal is the core's sentence under the field it names. It does not close on a press outside. Seen by people (D38). T3.
- **D893.** A passing drill toasts the core's lines (kept); a new kit no longer toasts its notes, which are on the kit page. Seen by people (D38). X2.
- **D894.** The kit page is a viewer: RECOVERY KIT // SHOWN ONCE, the page on paper (black on the stone ground in both themes), "The page as text" shut on screen and printed whole, PRINT and SAVE AS PDF… secondary, I HAVE IT the one primary; it opens at its top and ignores a press outside. Seen by people (D38). T3, 4.0 Viewer.
- **D895.** The kit page holds the window out of screen captures while it is open (D316 applied to the once page, which did not ask before). Not seen, except in a screenshot that now comes out blank.
- **D896.** The emergency sheet is a row with OPEN and the same viewer: EMERGENCY SHEET // NO SECRETS ON IT, the date it was made, sections and blanks on paper, PRINT secondary and DONE primary (before: Close and a primary Print). A failed read is a critical line under the row, not a toast. Seen by people (D38). T3.
- **D897.** The drill's checks are rows with a Badge OK or CHECK (before: an icon named "fine" or "needs you"). A failed read is a critical Banner with TRY AGAIN. Seen by people (D38). T3, cross-app §9.
- **D898.** How often the master password is asked is three chips in a row (7, 14, 30 days), saved at once (cross-app §5 over the brief's CHANGE; before: a select). Seen by people (D38). T3.
- **D899.** Emergency access is a row with the S5 sentence and OPEN, with no "later" (review R5, kept) and no contact count (§8.1). Seen by people (D38). T3.
