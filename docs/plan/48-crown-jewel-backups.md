# 48: Crown jewels in locked backups

_Slice: `slice/crown-jewel-backups`, built 2026-11-04. Builds D138 as Sean decided it on 2026-11-04 ("Include them"): crown jewels go into Lockdown's locked backups by default, sealed under the backup password, with a switch to leave them out. Plain exports stay as D121 says. Builds on D121 and D137 (the per-item opt-in), D140 (the published format), D142 (the plain-export yes), D143 (scheduled backups), D144 (the "left out" lists) and D62 (wire additions only). Companion to [`14-import-export.md`](14-import-export.md) and [`../format/backup-v1.md`](../format/backup-v1.md). Proposed decisions D560–D569 (§11)._

## For future agent

- **TL;DR:** A backup is what someone uses when the account and the server are both gone. Until now, a crown jewel (a recovery phrase, recovery codes, anything marked) was left out of every backup unless the person opted that one item in, so a restore brought back everything except the most valuable items. Now a **locked** Lockdown backup carries every crown jewel inside the same sealed file, under the same file key as everything else. That covers scheduled backups and the "Lockdown backup, locked with a password" export. A new backup setting, **Include crown jewels** (on by default, and on for installs that predate it), turns this off. Off, locked backups go back to the per-item opt-in and the count. Every plain file, KDBX, and a file locked to someone's public key still follow D121. A restore brings crown jewels back as crown jewels.
- **Read with:**
  - `crates/app/src/session/transfer.rs`: `CrownJewels`, `gather`, `export_preview_as`, `export_file_as`, `backup_document`, `crown_jewel_counts`.
  - `crates/app/src/session/backup.rs`: `crown_policy`, `BackupState::last_carried_crown_jewels`, `backup_status`, `order_key`.
  - `crates/app/src/transfer.rs`: `Core::locked_backup_crown`, `App::export_preview_for`, `App::export_to`, `App::backup_crown_jewels`.
  - `crates/app/src/settings.rs`: `BackupSettings::include_crown_jewels`.
  - `crates/cxf/src/archive.rs`: `Document::crown_jewels`, `CROWN_JEWELS_ALL`, `carries_every_crown_jewel`.
  - `apps/desktop/src/features/backups/`, `apps/desktop/src/features/export/`.
- **Not here:** the web vault has no backups, exports or restores (it refuses them in words), so it has nothing to switch (§6). The CLI makes and restores no backups (§7).

## 1. What was built

| Brief row | State | Where |
|---|---|---|
| 1. Locked backups carry crown jewels by default | **Done** for scheduled backups and the locked Lockdown export. Both are written with `CrownJewels::All`: every crown jewel in `items`, the document marked `crown_jewels: "all"`, nothing in `left_out` for that reason | `session/transfer.rs`, `session/backup.rs`, `transfer.rs` |
| 2. A switch to leave them out | **Done** on desktop: Settings, Backups, "Include crown jewels", on by default. It goes through the `Reveal` gate both ways (§4). Off brings back the opt-in and the count | `settings.rs`, `transfer.rs` (`backup_crown_jewels`), `features/backups/` |
| 3. Restore puts them back as crown jewels | **Done** (it already did for opted-in ones; now tested for every kind of locked backup). The class is kept, so the item key is wrapped under the Crown-Jewel Key again. The opt-in stays as the file has it, absent meaning off. The page-matching rule follows the class | `session/transfer.rs` (`import_commit`), unchanged; `tests/crown_jewel_backups.rs` |
| 4. Plain exports unchanged | **Done and tested**: plain Lockdown, CXF, CSV, KDBX, and an age file to a public key. The core narrows anything that isn't a sealed Lockdown file to the opted-in crown jewels, whatever the caller asks | `export_file_as`, `export_preview_as`, `session/bridges.rs` |
| 5. Existing installs | **Done**: a settings file without the field reads as on, so the next scheduled backup carries them. The Backups section says so once, under the switch, until that backup runs (§5) | `settings.rs`, `backup.rs` |
| 6. CLI | **Nothing to do**: `lockdown` has no backup, export or restore command (§7) | |
| 7. Tests | **Done** (§9) | |
| 8. Design capture | **Done**: two new Settings, Backups states, and the export and settings states taken again (§10) | `design/capture/` |

## 2. How crown jewels are sealed in a backup

**With the backup's own key, in the same document, with no extra layer.** A locked backup is one document sealed as `backup-v1.md` §3 says. The password goes through Argon2id (at the desktop floor), then HKDF-SHA-512 to the file key. The document is sealed with XChaCha20-Poly1305 in 1 MiB chunks, and the header is each chunk's associated data. Crown jewels are items in that document like any other. For a scheduled backup the file key is the one kept sealed in `backup-key.cbor` under the App Data Key (D143). For a locked export it is derived from the password typed in the dialog.

Why one layer:

1. **A second layer under the same password adds nothing.** Whoever has the backup password opens both layers, and whoever doesn't opens neither. It would only cost code, a second key derivation, and room for bugs.
2. **A second layer under another secret defeats the point.** Using the account password or the Crown-Jewel Key would mean the restore needs the account, the server or a second password. A backup is for the day those are gone. That is the case Sean's decision is about.
3. **Sean's words.** "Sealed under the backup password" is what D138's row now says.

What protects them in the file is therefore the backup password and its Argon2id cost: 64 MiB, 3 passes, 4 lanes at the floor, with a fresh salt per file. That is D138's original worry ("protected only by the backup password"), now accepted by decision. Two things soften it. Automatic backups keep the key on the device, so the password is typed once and can be long. And the Backups screen says plainly that crown jewels are "protected by its password".

What protects them on restore: they land as crown jewels. Revealing one needs the account password (the `Reveal` gate routes to the password whenever one exists). A page never gets the user name or address of a crown-jewel login. A later plain export leaves them out again, because their opt-in didn't change.

## 3. The format: version 1.1, additive

- **The change.** The document gets an optional member, `crown_jewels`. `"all"` means every crown jewel is in `items`, opted in or not. Absent means only the opted-in ones are, and the rest are in `left_out` (version 1.0's meaning). Any other value reads as absent. `v` stays `1`.
- **The spec.** §2 now says a writer may add, within a version, only members that are safe to ignore. §2.7 is new: crown jewels in a locked backup. A writer may carry crown jewels that weren't opted in only inside a sealed file, and must then write `"all"`. It must not add `lockdown.export` because a backup carries the item. A reader must bring a crown jewel in as a crown jewel. §4 lists the new vectors and §5 has a change table.
- **Test vectors.** `crates/cxf/tests/fixtures/backup-v1-crown-jewels.json` and `.lockdown` (written by the ignored test `write_crown_jewel_vectors`). They hold a login, a crown-jewel wallet and an archived crown-jewel document with a file. Neither crown jewel is opted in, and nothing is left out. The 1.0 vectors are untouched.
- **Older readers.** `a_reader_of_1_0_reads_every_item_of_the_crown_jewel_vector` decodes the new vector by the spec, then reads it with a struct holding only the members 1.0 listed. It gets every item, with each kind, class and state. An older Lockdown reads it the same way: `Document` ignores unknown members, and restore keeps the class.
- **Writers.** Only `backup_document(_, CrownJewels::All)` writes the member, and only `export_file_as` (sealed `lockdown`) and the backup worker call it with `All`. A plain Lockdown export is unmarked and lists its crown jewels as left out (tested).

## 4. The switch, and its gate

- **Setting.** `BackupSettings::include_crown_jewels` in `settings.cbor`. Its default is `true`, and a file written before it existed reads as `true` (the struct is `#[serde(default)]`). The webview's `set_settings` can't change it, because `settings.backup` is always copied from the core's own.
- **Call.** `App::backup_crown_jewels(on, gate)`, the desktop command `backup_crown_jewels` (appended to the handler list), and the client's `backupCrownJewels`.
- **Gate.** `Reveal` in **both directions**, the same gate `backup_setup` uses to change the other backup settings. The reasons are "put your crown jewels in your locked backups" and "leave your crown jewels out of your locked backups". On lets every crown jewel leave in each locked backup. Off changes what a restore can bring back on the worst day. Either change is worth the password, and the switch is rarely touched. The alternative is the pattern of D137 and `backup_off`: on asks, off asks nothing. It is recorded as D563 for Sean.
- **Effect.** The next backup follows the switch. Files already in the folder keep what they hold. The switch changes at once (no Save button) and doesn't write a backup by itself. It is refused while the vault is locked, or the app is confined.

**Proposed words (flagged for Sean, D564):**

| Where | Words |
|---|---|
| Switch label | **Include crown jewels** |
| Help, on | Recovery phrases, recovery codes and your other crown jewels go into every locked backup, protected by its password. |
| Help, off | Crown jewels stay out of locked backups unless you include them one by one. |
| Line above, off (unchanged from slice 14) | 2 crown jewels aren't in your backups. To include one, open it and turn on "Include in exports and backups". |
| Toast, on | Crown jewels will be in your next backup. |
| Toast, off | Crown jewels stay out of new backups. Backups you already have keep what they hold. |
| Export dialog, locked Lockdown backup | This file will hold 42 items, 4 attached files, 2 crown jewels, protected by the file's password. |
| Item's opt-in, off (the label is unchanged) | Crown jewels stay out of plain exports unless you include them here, one by one. Locked backups carry them unless you turn that off in Settings, Backups. |
| Item's opt-in, toast when turned off | This item stays out of plain exports. |

The switch is a unicron `Switch` in a horizontal `Field` (the same shape as the item's opt-in). It needs no new component, and the dsa ratchet stays under its baseline of 4.

## 5. Existing installs: said once

After the update the switch reads as on, so the next scheduled backup carries the crown jewels. **The Backups section says so once, as a line under the switch's help, until that backup runs.** There is no pop-up. The proposed words, flagged in D565:

> Your next backup will be the first to include your 2 crown jewels.

How it knows: `backup-state.cbor` gains `last_carried_crown_jewels`. A successful run sets it to the switch's value. A state file from an earlier Lockdown has none. `BackupStatus.crown_jewels_waiting` is the number of crown jewels not opted in. It is non-zero only when all of these hold: the switch is on, backups are on (a next run is due), a backup has run, and the last one didn't carry every crown jewel. That last condition covers a backup from before the update, and one made while the switch was off. The line goes away after the next backup. Turning the switch off and on again brings it back if a backup ran in between.

## 6. Desktop, web and CLI: what each got

**Desktop.** Settings, Backups has the switch, its help, the once-only line, and the count when the switch is off. The export dialog asks the core for the count with `sealed` (a new optional argument of `export_preview`; an older caller is counted as plain). So "A Lockdown backup, locked with a password" lists every crown jewel, and "A readable Lockdown file" still lists them as staying out. The item's crown-jewel opt-in has the new help line.

**Web vault.** It has no Settings, Backups section, no export and no restore. Its client refuses every backup call in words ("Backups isn't in the web vault yet; use the desktop app for it"), and `backupCrownJewels` joins them. A switch with nothing behind it would be a stub, so the web gets none. It shows the item's opt-in through the shared detail view, and the new words there are checked in Chromium by the web vault's e2e. When the web vault gets backups, it takes the same setting and call.

**CLI.** `lockdown` has no `export`, `backup` or `restore` command. Doc 14's note that `lockdown import` still carries its own copy is out of date: the command list has no import either. So there is no default or flag to add. A crown jewel restored on the desktop keeps its class in the vault the CLI reads, and the CLI's search lists it by title and kind only, as it does every crown jewel. Once the CLI gets backups, the flag is `--crown-jewels include|leave-out`, defaulting to the setting (D567).

## 7. Things found on the way

- **Two backups in one second listed in the wrong order.** `list` sorted names in reverse, and `…-000000-n2.lockdown` sorts before `…-000000.lockdown`, so the first file of a second came out "newest". `order_key` now sorts by stamp, then by the `-nN` number. Tested in `names_are_stamped_and_only_ours_are_touched`.
- **`backup_status` used to run a whole export preview** to count left-out crown jewels, which opened every item and every attached file. `crown_jewel_counts` opens only the crown jewels.
- **Files to a public key** (`export_to_recipients`, doc 23 §11) are Lockdown documents too. They stay under D121, because such a file may go to someone else's key (D566).
- **Lockfile and supply chain.** `async-executor` 1.14.0 (through zbus, from the desktop's notification and opener plugins) was yanked upstream, so `cargo deny check` failed on `main` too. This branch moves it to 1.14.1 in a lockfile-only commit, and points its cargo-vet exemption in `supply-chain/config.toml` at 1.14.1 with the same criteria.

## 8. Gaps

1. **The web vault has no backups** (§6). The switch arrives with them.
2. **The emergency sheet** (`bridges/emergency.rs`) still prints the left-out count only when there is one. It doesn't say that backups now carry crown jewels. One line would close that.
3. **Files to a public key** don't carry crown jewels that weren't opted in (D566). If Sean wants "locked" to mean every locked file, `export_to_recipients` takes `CrownJewels::All` and the age export's preview follows.
4. **Turning the switch off doesn't touch backups already in the folder.** They still hold crown jewels until pruning rotates them out (keep-last N). The toast says so. Removing them would need a delete the person asks for.
5. **No pop-up and no sidebar line** about the change (as asked). Someone who never opens Settings learns about it only from the emergency sheet (gap 2) or a restore.

**Unicron host-contract gaps:** none new. `Switch` in `Field orientation="horizontal"` with `FieldContent` was enough. The once-only line is a second `FieldDescription` with `role="status"`, because unicron has no "notice under a field" slot. That is the same stand-in the item's opt-in uses for its help.

## 9. Tests

| Where | What |
|---|---|
| `crates/app/tests/crown_jewel_backups.rs` (new, 3) | **Scheduled:** the worker's own tick writes a backup once due (the state file removed, nothing calling the app). The file holds no plaintext. A wrong backup password opens nothing, and nothing lands. With the password, a fresh vault with a master password gets all four items, three of them crown jewels: class kept, opt-in off, reveal refused without the password and with a wrong one, the phrase with the right one, the bank login seen by a page without user or address, a plain export from there leaving all three out. On the device that made it, the kept key opens it and everything is unchanged. **Locked export:** counts and the document (`crown_jewels: "all"`, `left_out` empty, no `lockdown.export` written), the same restore checks, then plain Lockdown, CXF, CSV and KDBX each leaving the three out, and a plain Lockdown file unmarked. **Switch off:** the gate both ways, `set_settings` can't flip it, a backup and a locked export leave three out by title, a restore says three were left out, the opt-in counts one in, on again shows two waiting until the next backup, and the switch is refused while locked |
| `crates/app/tests/import_export.rs` | The backups test now expects the wallet in (left out 0, restore of two items) |
| `crates/app/src/settings.rs` (+1) | On by default; an older file with backups on reads as on; off survives save and load; the settings file's backup keys |
| `crates/app/src/session/backup.rs` (+1, 1 extended) | A state file from before the switch reads `last_carried_crown_jewels` as none; two backups in one second sort newest first |
| `crates/cxf/tests/backup_format.rs` (+2, +1 ignored writer) | The 1.1 vector opens by the spec and carries crown jewels without an opt-in. A 1.0 reader reads every item and class. The crate reads the marker, and an unknown value reads as none |
| `apps/desktop` Vitest (+2, 2 updated) | `backups-section`: the switch through the gate both ways, the count coming back, the once-only line and the toasts; words for an older core (no field reads as on). `export-dialog`: a locked Lockdown backup counts every crown jewel and the readable one still leaves them out, with `sealed` passed. `export-opt-in`: the new help |
| `apps/web/e2e/web-vault.spec.ts` (+1) | In Chromium against the real server: a wallet made in the web vault shows the opt-in switch off with the new words, and no phrase on screen |

Counts: Rust +7 tests (+1 ignored). Desktop Vitest 265 → 267. Web Vitest 49 → 49. Web e2e +1.

## 10. How to run and test

```sh
export CARGO_PROFILE_DEV_DEBUG=line-tables-only
cargo test -p lockdown-app --test crown_jewel_backups --test import_export
cargo test -p lockdown-app --lib settings backup
cargo test -p lockdown-cxf --test backup_format
cargo test -p lockdown-cxf --test backup_format -- --ignored write_crown_jewel_vectors   # remakes the 1.1 vectors
cd apps/desktop && pnpm vitest run src/features/backups src/features/export
cd apps/web && pnpm build:wasm && pnpm build && LOCKDOWN_SERVER_BIN=… LOCKDOWN_CLI_BIN=… LOCKDOWN_E2E_PORT=… pnpm e2e
cd design/capture && pnpm build && pnpm capture --only desktop-settings-backups --only desktop-settings-backup-restore --only desktop-settings-export --only desktop-settings --widths 1280,390
```

The capture's new states are `desktop-settings-backups-crown-waiting` (the once-only line) and `desktop-settings-backups-crown-off` (switch off, count back). Both scroll Settings to Backups, its last section, because a capture is at most 3000 px tall. Their axe results are clean.

## 11. Wire and file additions (D62, additive)

| Where | Added |
|---|---|
| `BackupStatus` | `include_crown_jewels: bool`, `crown_jewels_waiting: usize` |
| `ExportPreview` | `crown_jewels_all: bool` |
| `BackupSettings` (`settings.cbor`) | `include_crown_jewels: bool`, default `true` |
| `BackupState` (`backup-state.cbor`) | `last_carried_crown_jewels: Option<bool>` |
| Desktop IPC | the new command `backup_crown_jewels { on, gate }`; `export_preview` takes an optional `sealed` (absent means plain) |
| Backup document | `crown_jewels: "all"` (format 1.1, `v` stays 1) |

No wasm export, server route, extension message or JSON schema under `extension/schema/` changed. The TypeScript types mark the new fields optional, so an older core reads as the defaults.

## 12. Proposed decisions

| ID | Decision | Status |
|---|---|---|
| D560 | **Locked Lockdown backups carry every crown jewel by default** (scheduled backups and the "locked with a password" Lockdown export), inside the same sealed document under the backup's own file key. There is no second layer: one under the same password adds nothing, and one under another secret would break the restore when the account and server are gone. Implements D138 as Sean decided it. Plain version: your backups now hold your recovery phrases too, locked with the backup password. | Proposed (implements Sean's decision); audit item |
| D561 | **Only a sealed Lockdown file may carry crown jewels that weren't opted in.** Plain Lockdown, CXF, CSV and KDBX files keep D121, and the core enforces it (`export_file_as` narrows to the opted-in ones whatever the caller asks). Plain version: an unprotected file still leaves your crown jewels out unless you chose them one by one. | Proposed; audit item |
| D562 | **Backup format 1.1**: the optional document member `crown_jewels: "all"`, written only into a sealed file. It comes with the §2.7 rules (no added `lockdown.export`; a reader brings a crown jewel in as a crown jewel) and a second pair of test vectors, and `v` stays 1. §2's rule for additions within a version now reads "members safe to ignore". Plain version: the published format says when a backup holds every crown jewel, and older readers still read it all. | Proposed |
| D563 | **"Include crown jewels" changes through the `Reveal` gate in both directions** (the gate `backup_setup` uses), only through `backup_crown_jewels`, never through `set_settings`. The alternative is on asks and off doesn't, like D137 and `backup_off`. Plain version: turning the switch on or off asks for your password. | Proposed; **flagged for Sean** (user-visible: a password prompt to turn it off as well) · audit item |
| D564 | **The switch's words**: "Include crown jewels". On: "Recovery phrases, recovery codes and your other crown jewels go into every locked backup, protected by its password." Off: "Crown jewels stay out of locked backups unless you include them one by one." Toasts as in §4. The item's opt-in help is reworded as in §4 (its label is unchanged). Plain version: the words on the new switch and around it. | Proposed; **flagged for Sean** (new words on a screen) |
| D565 | **Existing installs: one line, no pop-up.** A settings file from before the switch reads as on, so the next scheduled backup carries crown jewels. Until it runs, the Backups section shows "Your next backup will be the first to include your N crown jewels." under the switch (`crown_jewels_waiting`, from `last_carried_crown_jewels`). Plain version: after the update, Settings says once that your next backup will hold your crown jewels. | Proposed; **flagged for Sean** (user-visible: a behaviour change on update and its words) |
| D566 | **A file locked to a public key (age, doc 23 §11) keeps D121.** It may be for someone else's key, so crown jewels leave in it only when opted in. Plain version: a file you lock for another person's key still leaves your crown jewels out unless you chose them. | Proposed; **flagged for Sean** (what "locked" covers) |
| D567 | **The CLI gets nothing now.** It has no backup, export or restore command. When it gets them, they follow the same setting, with `--crown-jewels include\|leave-out` for one run. Plain version: the command-line tool doesn't make backups yet; when it does, it follows the same switch. | Proposed |
| D568 | **Backups made in the same second list newest first** (`order_key`: by stamp, then by the `-nN` number), so "restore the newest" and pruning keep-last N pick the right files. Plain version: two backups a moment apart are listed in the right order. | Accepted by default under D38 (a bug fix) |
| D569 | **Wire and file additions (D62, additive)**: §11's fields, the `backup_crown_jewels` command and `export_preview`'s optional `sealed`. Plain version: older and newer apps keep understanding each other. | Accepted by default under D38 |
