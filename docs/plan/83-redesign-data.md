# 83: Import, export and backups: the import Form in steps, the export Form, Backups as rows

_Slice: `screens-8` of the 2026-11 redesign, on `wt/lockdown-screens` (from `claude/keen-ritchie-q2hgb2`, after doc 82), built 2026-11-15. No drawn frame; briefs T7 and T8 of the undrawn-areas brief (workspace `07-projects/29-unicron/impl-2026-11/app-lockdown-undrawn.md`), with 4.0 (layouts by name, emphasis, states), T0 (how a setting behaves), X2 (table 4, the failed-backup Banner), X5 (questions), V3 (duplicates), W1 (the web refuses all three); cross-app decisions §1, §5, §9, §10, §12, §13, §14; contract 5.1b #2, #3, #23, #30, #37; 5.1c C11. Decisions D910 to D919. Status: built in the lane; not merged._

## For future agent

- **TL;DR:** Settings → Import and export is two Field Rows, IMPORT ("From another app or a backup", IMPORT…) and EXPORT ("One file of your vault", EXPORT…). The import is a Form in steps (`IMPORT // STEP 01 // 03`): choose (where from, into which vault, CHOOSE FILE… and the file as a `FileChip`), the file's own password when it has one, what the file holds (the core's summary as the title, a row per kind, WHAT CAN'T COME ALONG NN), done (the report as the title, FIND DUPLICATES to Health). The export is one Form: KIND OF FILE as Radio Rows, WHAT THE FILE WILL HOLD, then a password twice, public keys, or the warning and the Checkbox; CHOOSE WHERE TO SAVE… (gate); a toast "Saved to {folder}". Backups, off, is a short form whose one primary is TURN ON BACKUPS (on a phone, an extended FloatButton at the end of the bottom row); on, it is rows that save themselves, with TURN OFF BACKUPS at the foot asking first. A failed backup shows the critical Banner at the top with BACK UP NOW. The web vault draws none of it.
- **Files:** `features/import/import-wizard.tsx` (the Form), `import-model.ts` (the step and restore overlines, the yes and its reason), `import-export-section.tsx` (new: the section's two rows, and both Forms' mounts); `features/export/export-dialog.tsx`, `export-model.ts` (`folderOf`); `features/backups/backups-section.tsx`, `backups-model.ts` (the LAST BACKUP words, the CHANGE choices, toasts without full stops); `features/settings/settings-view.tsx` renders `ImportExportSection` in place of its inline buttons and mounts.
- **Do:**
  - Keep `ImportWizard`'s export and props: the first run renders it (doc 77). Its props only gain: `restoreMadeAt`, `onFindDuplicates`.
  - Keep `ExportDialog` and `BackupsSection` props.
  - Keep the file's bytes and every typed password out of anything but the one call: bytes are zeroed after the commit or on close, typed passwords go as bytes and are zeroed after the call, and the fields are emptied after every try.
- **Do not:**
  - Show FIND DUPLICATES where there is no Health to go to (the first run): the host passes `onFindDuplicates` only where it can.
  - Toast an error. A field's problem is under the field; a refused file or save is a compact critical Banner in the Form; a setting that failed is one critical line under its row.
  - Draw import, export or backups in the web vault (W1).
- **Read with:** doc 79 (the `SettingsFrame`, `setting-rows.tsx`), doc 67 (the five channels, the failed-backup Banner), doc 77 (the first run's import step), doc 48 (crown jewels in backups, D560–D569), doc 14 (import and export), D103, D121, D137, D138, D141, D142.

## 1. Import and export (brief T7)

**The section.** Two Field Rows, stacked on a phone: `IMPORT` "From another app or a backup" with `IMPORT…`; `EXPORT` "One file of your vault" with `EXPORT…`. The paragraph and the two outlined buttons go.

**The import Form.** An `AdaptiveDialog`, form 640, full height on a phone, closed only by CANCEL or the close button (a press outside does nothing: the dialog holds a file).

| Step | Overline | Title | Body | Footer |
|---|---|---|---|---|
| 1. Choose | `IMPORT // STEP 01 // 03` | "Import from another app" | "Where is it from?" (first: "Let Lockdown tell from the file"; the format's own how-to under it), "Into which vault" when there are several, the chosen file as a `FileChip` (name, size), `CHOOSE FILE…` / `CHOOSE ANOTHER FILE…`, the caption "Once it's in, delete the exported file. It holds your passwords in plain text." | `CANCEL`, `CHECK FILE` (waits for a file: "Choose the file first.") |
| 1b. The file's password | the same | "This file has its own password" (the file's name under it) | A `SecretField` "The file's password" with the hint for that kind of file; a wrong one says so under the field, which is emptied | `CANCEL`, `OPEN FILE` |
| 2. What it holds | `IMPORT // STEP 02 // 03` | The core's summary ("26 items in this file: 24 new, 2 already here as they are.") | "Read as …" under the title; `WHAT THE FILE HOLDS`: a compact List Row per kind with its count (largest first), then the attached files and crown jewels in a line each; `WHAT CAN'T COME ALONG NN`: the core's notes, the first 8 and `SHOW ALL NN` | `CANCEL`, `BRING IN 24 ITEMS`; with nothing new, `BRING IN` off with "Nothing to bring in" under it (#37) |
| 3. Done | `IMPORT // STEP 03 // 03` | The report in one sentence, said once | "You can delete the file you imported now." under the title; in Settings, "Items from another app can say the same thing twice. Health finds them." and `FIND DUPLICATES` | `DONE` |

While the core works, the step shows one live line ("Checking the file…", "Bringing it in…") and the yes keeps its label with its spinner. A file the core refuses shows its words in a compact critical Banner on step 1.

**Restore from a backup** is the same Form, entered at its password or its preview, with the overline `RESTORE // 2026.11.13 09:00` (the backup's time, local) and the title "Restore from a backup" until the preview names what it holds.

**FIND DUPLICATES** closes the Form and opens Health, whose Duplicates row opens the review (V3). There is no place for the review itself (it is a dialog inside Health, vault lane); a place `health-duplicates` would let the button land on it.

**The export Form.** The same dialog, overline `EXPORT`, title "Export your vault", "One file, saved where you choose. Lockdown asks it's you first. Export is always free."

1. "What to export" (Every vault, or one) when there are several.
2. `KIND OF FILE`: Radio Rows, each with its sentence, in today's order: the locked Lockdown backup (recommended) first, the age file second where the build has it, then KeePass, CXF, CSV and the readable Lockdown file.
3. `WHAT THE FILE WILL HOLD`: the core's one line, the crown jewels left out by name (D121), and `NOT IN THIS KIND OF FILE` with the core's lines. "Counting…" while it counts.
4. What the kind needs: two `SecretField`s ("Use at least 8 characters." in the first one's help; a short or unmatched password says so under its field); the public keys in a multiline Text Field (a missing key says so under it); or, for an unprotected file, a warning Banner "This kind of file isn't protected." and the Checkbox "I understand this file won't be protected: anyone who gets it can read every password in it." (D142).
5. `CHOOSE WHERE TO SAVE…`, the layer's one primary: off with its reason until the box is ticked, then the OS dialog, then the gate (D103, D141). Saved: the Form says "Saved to {folder}" with the path, what the file holds, what stayed out, and for an unprotected file a warning Banner; a toast "Saved to {folder}".

**The web vault** lists neither section (places.ts already leaves them out); its client refuses them (W1).

## 2. Backups (brief T8)

**Off.** "Backups are off. Choose a folder, how often and a password, and Lockdown keeps locked copies of your vault there." Then `FOLDER` "None chosen yet" with `CHOOSE…` (the OS dialog), `HOW OFTEN` (every hour, 6 hours, 12 hours, day or week) and `KEEP` (the newest 3, 5, 10, 20 or 50), each behind `CHANGE` (cross-app §5, five choices), and the backup password twice in `SecretField`s ("You'll need it to restore on another computer…"). A password kept from before is not asked again. The section's one primary, `TURN ON BACKUPS` (gate), waits for a folder with "Choose a folder first."; on a phone it is an extended FloatButton at the end of the detail bottom row, with the reason as a caption above.

**On.** Rows that save by themselves, silently; a failure puts one critical line under the row in the core's words.

| Row | Value | Action |
|---|---|---|
| `BACKUPS` | On | `BACK UP NOW` (secondary; on the Banner instead while one failed) |
| `LAST BACKUP` | "Backed up 9 hours ago", or "No backup yet"; under it "Next at 21:40, while Lockdown is unlocked." | none |
| Toggle Row "Include crown jewels" | D564's sentence for on or off; under it, when it applies, D565's line, said once, and when off the count left out | asks through the gate, both ways (D563) |
| `FOLDER`, `HOW OFTEN`, `KEEP` | as set | `CHOOSE…`, `CHANGE` |
| `BACKUP PASSWORD` | Set | `CHANGE…`: a Form (form 640): the password twice, CHANGE PASSWORD |
| `IN THE FOLDER NN` | a compact List Row per file: `2026.11.13 09:00` and its size | `RESTORE…` (the import Form) |

At the foot, `TURN OFF BACKUPS`, a critical text button. Its Question (480, a sheet on a phone): "Turn off backups?", "No new backups are made. The files in the folder stay where they are.", the Checkbox "Also forget the backup password on this computer" (C11), focus on CANCEL, `TURN OFF` (destructive).

**States.** Checking: three skeleton rows. A failed read: `EmptyState` critical, `BACKUPS // 00`, the core's words and TRY AGAIN. The last try failed: the shell's critical Banner (found-5's words, doc 67), "The last backup didn't work", the core's reason and `BACK UP NOW`, at the top of the section. It is drawn from the section's own status, so it shows wherever the section shows; the shell's `BannerSlot place="backups"` would carry OPEN BACKUPS, which on this page opens itself.

**Crown jewels (D138).** Built as today: on by default, so locked backups carry every crown jewel under the backup password (D138 as Sean decided on 2026-11-04, D560). The orchestrator lists D138 among the questions still open; nothing here changes it.

**Toasts** lose their full stops and name the thing: "Backups turned on", "Backed up", "Backups turned off", "Crown jewels go in your next backup", "Crown jewels stay out of new backups", "Restored: 0 brought in, 1 updated", "Saved to /Users/sam/Desktop".

## 3. Tests and what is not proven

- `import-wizard.test.tsx`: step 1 of 3 and CHECK FILE waiting for a file with its reason; the chosen file as a chip; the core's summary as the dialog's name, step 2 of 3, a row per kind with its count, the crown-jewel line, what can't come along; the commit, step 3 of 3, the report said once, and FIND DUPLICATES calling the host; the file's own password, a wrong one under the field with the field emptied, the bytes wiped; a restore from the folder with `RESTORE // 2026.11.13 09:00`; **new:** nothing new keeps BRING IN off with "Nothing to bring in"; a refused file is a critical Banner on step 1; the first run gets no FIND DUPLICATES.
- `export-dialog.test.tsx`: the kinds are Radio Rows with the recommended one first and chosen; what stays out; unmatched passwords under the field, nothing chosen; the gate, "Saved to /home/ada" and the toast without a full stop; an unprotected file: CHOOSE WHERE TO SAVE… off with its reason until the box is ticked; the save dialog closed writes nothing; the locked backup counts every crown jewel, the readable one leaves them out (doc 48); `folderOf`. `export-dialog-age.test.tsx`: the age kind picked by its Radio Row (the control it names changed).
- `backups-section.test.tsx`: off, TURN ON BACKUPS waits for a folder with its reason, CHOOSE…, a short password under its field, the gate, then the on rows, the wiped password and the toast; **new:** off, HOW OFTEN through CHANGE goes with TURN ON BACKUPS and a kept password is not asked again; a row saves by itself (KEEP through CHANGE, no Save button); **turning off asks**: focus on CANCEL, nothing sent before the yes, the Checkbox's answer sent, the toast; a failed backup's Banner at the top with the core's words and its one BACK UP NOW; the restore; the crown-jewel switch through the gate both ways with its lines and toasts; the words of LAST BACKUP and the next one.
- `apps/web/src/features/web-settings-view.test.tsx`: **new: the web vault draws neither import, export nor backups**: no section, no IMPORT…, EXPORT…, TURN ON BACKUPS, BACK UP NOW or RESTORE…, and no call to the client's backup status, import formats or export preview, even when the address names the section.
- Web e2e (`web-vault.spec.ts`, the foundation's): the crown jewel's opt-in words (plain exports leave it out, locked backups carry it) are the item's (`export-opt-in.tsx`, vault lane) and unchanged.
- Capture states (strict, `--sizes 1440x900,390x844,768x1024`): `desktop-settings-import` (step 1 with a file), `-import-password`, `-import-preview`, `-import-done` (new), `desktop-settings-export`, `-export-plain` (new), `desktop-settings-backup-restore`, `desktop-settings-backups-off`, `-backups-on` (new), `-backups-turn-off` (new), `-backups-crown-waiting`, `-backups-crown-off`, `-backups-failed` (new), and `desktop-first-run-import` (screens-2's, re-taken). Ids kept; `built` on eleven screens. The harness's CHOOSE FILE puts an invented CSV on the file input (`pickImportFile`); the fixture's core reads none of it.
- Not proven: the OS file, folder and save dialogs; a real core's refusals and previews; the bottom row with the keyboard up on a real phone; a screen reader on the steps.

## 4. Gaps

1. **FIND DUPLICATES lands on Health, not on the review.** The duplicates review is a dialog inside Health with no place of its own (vault lane). A place `health-duplicates` (or an `openDuplicates` on Health) would land on it.
2. **The phone Form is the AdaptiveDialog's full-height sheet**, with CANCEL and the yes sharing its foot, not a top bar with a checkmark Float Button; back does not go one step (4.0's Form on a phone, T7). The steps keep CANCEL as their way out.
3. **"Where is it from?" and "What to export" stay native selects** (Unicron's `NativeSelect`): each choice is a name, not a sentence. The kind of file became Radio Rows (T7).
4. The import's report notes (`ImportReport.notes`) are still not shown on the last step, as before.

## 5. Unicron host-contract gaps

1. `SecretField mode="edit"` is controlled only (doc 81 gap 6, doc 82 gap 3): the file's password, the export's and the backup's sit in React state while their Forms are open. They are emptied after every try and on close. Screens: the import's step 1b, the export Form, Backups off and CHANGE….
2. `FloatButton` has no `disabledReason` (doc 82 gap 2): TURN ON BACKUPS in the phone's bottom row says why only through a caption the section adds.
3. `FieldRow`'s `action` is drawn for a text action; T8 asks for a secondary BACK UP NOW there, a bordered button taller than the row's line (doc 82 gap 4's kind). Screen: Backups.
4. `FileChip` has no "choose a file" form: the window's file input is a hidden input opened by a Button. A FilePicker (a button, the chip, and the input) would serve the import, Vale and Synapse alike.
5. `AdaptiveDialog` has no step or folio slot: the overline carries `STEP 01 // 03` as words (`StepsFolio` is a phone line under the top row, not a dialog's).
6. `AdaptiveDialog` does not pass `DialogFooter`'s `sticky` through: in the long export Form at 1440 × 900 the footer (CHOOSE WHERE TO SAVE…) sits below the dialog's scroll. Screens: the export Form, any long Form.

## 6. Proposed decisions

- **D910.** Settings → Import and export is two Field Rows, IMPORT and EXPORT, each opening its Form; the paragraph and the two outlined buttons go. Seen by people (D38). T7, T0.
- **D911.** The import is a Form in three steps with the step in the overline (`IMPORT // STEP 01 // 03`), the file shown as a chip once chosen, the core's summary as step 2's title and the report as step 3's; its buttons drop their articles (CHECK FILE, OPEN FILE). Seen by people (D38). T7, cross-app §12.
- **D912.** With nothing new in a file, the yes stays BRING IN, off, with "Nothing to bring in" under it (before: a disabled button that said "Nothing to bring in"). Seen by people (D38). T7, #37.
- **D913.** A restore is the import Form entered at its password or preview, with `RESTORE // <the backup's time>` as its overline. Seen by people (D38). T7.
- **D914.** The import's last step offers FIND DUPLICATES, which opens Health, where the hosts can (Settings); the first run does not show it. Seen by people (D38). T7, V3. Gap 1.
- **D915.** The export's kinds are Radio Rows, each with its sentence, in today's order with the recommended one first (before: a native select with one sentence under it). A field's problem shows under the field; an unprotected file shows a warning Banner over the Checkbox, and CHOOSE WHERE TO SAVE… waits for the tick with its reason (before: a message after the press). Saved, the toast is "Saved to {folder}". Keeps D103, D121, D141, D142. Seen by people (D38). T7, 6.2 (D142's yes kept).
- **D916.** Backups, off, is a short form whose one primary is TURN ON BACKUPS, waiting for a folder with its reason; on a phone an extended FloatButton at the end of the bottom row. HOW OFTEN and KEEP are behind CHANGE. Seen by people (D38). T8, T0, Q13 (Backups' side: it lives in Settings, its trouble on Items and here).
- **D917.** Backups, on, is rows that save by themselves (no "Save changes"): BACKUPS with BACK UP NOW, LAST BACKUP "Backed up 9 hours ago" or "No backup yet" with when the next is due, the crown-jewel switch, FOLDER, HOW OFTEN, KEEP, BACKUP PASSWORD with CHANGE… (its own Form), IN THE FOLDER NN with RESTORE…. Seen by people (D38). T8, T0, Q20 (a relative time here: it says how fresh the copy is).
- **D918.** TURN OFF BACKUPS is a critical text button at the foot that asks first, "Turn off backups?", with the Checkbox "Also forget the backup password on this computer" in the question (before: the box and the button sat in the form and turned off at once). Seen by people (D38). T8, X5, C11.
- **D919.** A failed backup shows the critical Banner at the top of Backups with BACK UP NOW (found-5's words; before: one amber line); the toasts of the three sections lose their full stops and the crown-jewel toast its second sentence ("Crown jewels stay out of new backups"). D564's switch words are kept as written; D138 is built as today (on by default) and stays Sean's call. Seen by people (D38). T8, X2 table 4, cross-app §12, 6.2 (D274 and D138 rows).
