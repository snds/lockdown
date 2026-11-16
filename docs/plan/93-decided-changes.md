# 93: the decided changes (Lockdown 1 to 38) and the three cross-app settlements

_Slice of 2026-11-16, on `claude/keen-ritchie-q2hgb2`, after doc 92 (unicron 1.1). Sean delegated the open questions of the redesign; the calls are in the workspace's `07-projects/29-unicron/impl-2026-11/rebuild-questions-decided.md` (Lockdown section) and are settled. This doc says what was built for each of the 24 that change something, where, and its test. Decisions D1001 to D1038 carry the item's number (D10NN is Lockdown NN); D1040 to D1042 are the settlements. D1000 and D1043 to D1045 are doc 92's. Where a build differs from the call, the row says why._

## For future agent

- **TL;DR:** every "Change" in the Lockdown list is built. Two needed the core: Add a device's COPY goes through a new core call that copies the link's own code on the clipboard path every secret takes (Lockdown 1), and the web core says "That isn't the master password." at the lock screen (Lockdown 32). The extension's frames keep a host's registrable domain whole, using the matcher's Public Suffix List through the wasm (Lockdown 26). The settlements: rows say clock time with the age as a tip; counts read three digits through unicron's formatter; what can wait (the reminders) is a needs-you count on Settings everywhere and a block on the home screen only.
- **Do not:**
  - Copy anything to the clipboard from the webview. Add a device's COPY asks the core to copy the code it made (`copy_link_code`); the webview never sends the text.
  - Show a Touch ID or Windows Hello button on the lock screen before a command opens the vault with it (D1032).
  - Put a reminder's needs-you entry back to `shows: "every"`. Reminders can wait: `shows: "home"` (D1042).
  - Cut a host's registrable domain in a frame. Only subdomains give way (D1026).

## What was built

| Ref | Built | Where | Test |
|---|---|---|---|
| Lockdown 1 | COPY on Add a device. The core hands over the open link's code (`App::link_code_to_use`, only while the link waits for a device); the desktop shell writes it with the clear timer and the clipboard-history marks (`copy_link_code`, the same `copy_with_timer` as a password); the web vault's page copies it through its own clearing clipboard (`link_code_to_use` in the web command table). "Copied. The clipboard clears in 30 seconds." under it. | `crates/app/src/devices.rs`, `session/devices_link.rs`; `apps/desktop/src-tauri/src/security_depth.rs`; `crates/web/src/commands/depth.rs`; `core/client.ts`, `apps/web/src/core/web-client.ts`; `features/depth/add-device.tsx` | `crates/app/tests/device_approval.rs` (no link: refused; waiting: the exact code; a device asked: refused); `add-device.test.tsx` (copies through the core with the settings' seconds, sends no text, never writes the system clipboard; a refusal in words) |
| Lockdown 2 | "One-time code" in every create menu opens Add a code (the paste form), never the empty editor; a phone's Codes head keeps SIGN IN TV only. A code added from there reloads any page showing codes and lands on Codes. | `app/shell.tsx` (`addCode`), `features/vault/vault-view.tsx`, `features/codes/codes-view.tsx`, `use-codes.ts` (`codesChanged`) | shell and vault tests; D962 amended (doc 88) |
| Lockdown 4 | The popup's mono line is in unicron 1.1's `type-mono-row` role (13 px, as written); the popup.css stand-in is gone. | `extension/src/popup/popup.tsx` (`monoRow`), `popup.css` | `extension.spec.ts` keeps the check: mono, not capitals, 13 px or more |
| Lockdown 6 | VERIFIED and NOT YET are both neutral badges; VERIFIED has a check glyph. | `features/status/status-view.tsx` | `status-view.test.tsx` (every proof badge `data-tone="neutral"`, a glyph beside Verified); harness `desktop-status` |
| Lockdown 7 | A downloaded update that has waited 7 days shows as the one dismissable info Banner on All items, RESTART TO UPDATE. The day it became ready is kept on this device per version; a newer version starts again; installed forgets it. | `features/updates/update-waiting.ts`, `App.tsx` | `update-waiting.test.ts` |
| Lockdown 8 | The needs-you block sits under an open item's title, as drawn; with nothing open it tops the column as before. | `components/master-detail.tsx` (`DetailNeedsYou`), `features/vault/detail-pane.tsx` | `master-detail.test.tsx` (under the title, once) |
| Lockdown 9 | USE ANOTHER DEVICE OR SECURITY KEY reads OTHER DEVICE OR KEY. | `extension/src/pages/frame.ts` | `passkeys.spec.ts`, `a11y.spec.ts` |
| Lockdown 10 | The core's templates show as "Card" and "Secure note"; ids and the core's names are unchanged. | `app/template-model.ts` (`displayTemplate`), `app/templates.tsx` | `template-model.test.tsx`, `new-item-menu.test.tsx`, `shell.test.tsx` |
| Lockdown 11 | LAST BACKUP reads "Backed up Today 09:02"; how long ago is the tip. | `features/backups/backups-model.ts`, `components/clock-time.tsx` | `backups-section.test.tsx` |
| Lockdown 12 | The keys list: what a key does in the body role, the key in mono; the core's shortcut label never shows a Mac glyph off a Mac (`Ctrl+Shift+Space`); the harness's fixture label is per platform. The Windows default is Ctrl+Shift+Space, not Alt+Space (`src-tauri/src/quick.rs`). `/` and ⌘, stay off under a dialog (already so). | `app/keys.ts` (`platformKeys`), `app/keys-dialog.tsx`, harness `fixture/commands.ts` | `keys.test.tsx` |
| Lockdown 18 | Rows swipe at the medium width when the pointer is a finger (a touch tablet), as on a phone. The harder full swipe on a three-tile side is unicron 1.1's own (320 at 390). | `lib/pointer.ts`, `features/vault/vault-view.tsx` | `pointer.test.ts`, `swipe.test.tsx` (tablet swipes, mouse at medium doesn't) |
| Lockdown 19 | A found item's one-time code shows inside its own row, under its words (compact CodeDisplay); Escape from Search opened on Codes, Settings or any place goes back there. | `features/vault/search-view.tsx`, `item-row.tsx` (`detail`), `app/shell.tsx` (`leaveSearch`) | `search-view.test.tsx` |
| Lockdown 20 | "Keep deleted items for…" opens its six choices as a menu of their own (a sheet on a phone), saved on pick, not a second page. Name prompts are the 480 question size on a desktop; a phone keeps the whole screen. | `features/organization/organization-nav.tsx`, `name-prompt.tsx` | `organization-nav.test.tsx` |
| Lockdown 22 | The web vault draws no export switch on a crown jewel. D827 amended (doc 74). | `features/export/export-opt-in.tsx` | `export-opt-in.test.tsx`; web e2e checks it is absent |
| Lockdown 25 | A device with a session reads "Last used Today 09:02", like a session's row; "Added {date}" with none. | `features/devices/devices-view.tsx`, `features/sessions/sessions-section.tsx` | `devices-view.test.tsx` |
| Lockdown 26 | A frame's host keeps its registrable domain whole (the matcher's Public Suffix List, through the wasm's new `registrable_domain`), and only the subdomains give way, from the left. "Stop:" leads refusals and covers only; a fact (an error, a closed request, an existing passkey) has no "Stop:". | `crates/wasm/src/lib.rs`; `extension/src/background/matcher.ts`, `index.ts`; `pages/frame.ts`; `static/ui.css`; harness `host/chrome.ts` | wasm unit test; `extension.spec.ts` (`evil.net` kept whole), `passkeys.spec.ts` (no "Stop:" on a fact) |
| Lockdown 27 | The popup's shown code is display only (`CodeDisplay copy={false}`); COPY CODE is the row's one copy, through the app. | `extension/src/popup/popup.tsx` | `extension.spec.ts` |
| Lockdown 28 | Large type follows the live code: a new code is read again when it turns over, the title counts down, it is still capture-protected and still closes two minutes after it opened. D963 amended. | `features/vault/large-type.tsx` (`CodeLargeType`), `detail-pane.tsx`, `codes-view.tsx` | `large-type.test.tsx` |
| Lockdown 29 | A new login's password is made by the core on save, by default: the field says "Made when you save" and shows nothing (masked), with TYPE ONE INSTEAD. The grabber goes with unicron 1.1 (only a sheet that drags has one). | `features/vault/editor-model.ts` | `editor-model.test.tsx`; editor tests; `finish.spec.ts` and the web e2e type one instead |
| Lockdown 30 | Each Sharing tab's count is in its name ("People, 1"), through unicron 1.1's `TabsTrigger mark`; no hidden count. | `features/sharing/sharing-view.tsx` | `sharing-view.test.tsx` |
| Lockdown 32 | No Touch ID or Windows Hello button on the lock screen until quick unlock works (D842 amended). The web core says "That isn't the master password." for a wrong password at the lock screen; signing in still says the login name and password didn't match. | `features/unlock/unlock-screen.tsx`; `crates/web/src/host.rs` | `unlock-screen.test.tsx`; `crates/web/tests/web_vault.rs`; web e2e |
| Lockdown 33 | Migration: M1's yes is Continue when the kit step follows, Lock my vault when M1 locks; M2's yes is Lock my vault. | `features/unlock/migration.tsx` | `unlock-screen.test.tsx`; harness `desktop-migration-kit` |
| Lockdown 37 | A passing CHECK NOW sets Vault history's line to "Checked Today 09:02"; no toast; the lines stay under Details. | `features/status/status-view.tsx` | `status-view.test.tsx` |
| Lockdown 38 | Backups' crown-jewel switch, off, adds "Backups you already have keep what they hold." Export: the locked kinds first (the Lockdown backup, the age backup where the host has it, KeePass), the unprotected ones under MORE KINDS, shut unless one of them is picked. | `features/backups/backups-model.ts`; `features/export/export-dialog.tsx` | `backups-section.test.tsx`, `export-dialog.test.tsx` |

## The three settlements

| Settlement | Built |
|---|---|
| **Rows use clock time** (D1040) | `ClockTime` (`components/clock-time.tsx`) writes `rowDate` (Today 09:02, MON 09:02, NOV 02) with how long ago as the tip and the description. Used by LAST BACKUP, device rows and session rows. Every other row already wrote `rowDate`. The status line's backup indicator ("Backed up 3 hours ago") is not a row and keeps its age. |
| **Counts read three digits** (D1041) | `lib/format.ts` `count()` is unicron 1.1's `formatNavCount` (`012`, `cap` keeps `99+`), so the app and unicron's parts never disagree; the popup's count too. Sentences keep `countName` (plain numbers). |
| **What can wait shows on the home screen** (D1042) | Rows 1 to 6 (requests, emergency access, a conflict, a reset) still show wherever they apply. The reminders (rows 7 to 9: kit test, master password, drill) are `shows: "home"`: their block shows on All items only, and Settings carries a count in the needs-you tone everywhere ("Settings, 1 waiting on you"), on the sidebar and the phone's Settings button, ahead of the update's quiet square. |

## Changed from the call, and why

- **Lockdown 1**: the call says "a plain-string copy command". Built as a copy of the link's own code, which the core holds: the webview sends no text at all. Same safety rules (timer, history marks), narrower surface.
- **Lockdown 2**: the call says phone. "One-time code" opens the paste form at every width, so one menu row means one thing; desktop keeps ADD CODE on Codes too.
- **Lockdown 10**: filter chips keep the plurals Figma draws ("Cards", "Notes"); the names people see for a kind (menu, editor, row words) are "Card" and "Secure note".
- **Lockdown 26**: the menu header and the passkey sheet header both cut this way; a host the list can't answer (an IP address, `localhost`) is never cut.
- **Lockdown 30**: unicron's mark draws nothing for 0, so an empty tab shows no `000`.
- **Lockdown 32**: the first run's own "Unlock with Touch ID?" step is unchanged (it is the set-up screen, not the lock screen); flagged below.

## Open

- First run step 3 still shows a disabled "Use Touch ID" with the core's reason. Same reasoning as D1032 would drop it until quick unlock works: Sean's call.
- Cross-app §3 (second exception: a ready update waiting a week) and §13 (clock time for all four apps) are amended here only; `cross-app-decisions.md` in the workspace needs the same lines.

## Decisions

| ID | Decision | Status |
|---|---|---|
| D1001 | Add a device's COPY copies the link's own code from the core on the secret clipboard path (Lockdown 1). | Built |
| D1002 | "One-time code" opens the paste form everywhere; the phone's Codes head drops ADD CODE (Lockdown 2). Amends D962. | Built |
| D1004 | The popup's mono line uses unicron's `type-mono-row` (Lockdown 4). | Built |
| D1006 | VERIFIED and NOT YET are neutral; VERIFIED has a check glyph (Lockdown 6). | Built |
| D1007 | A ready update that waited 7 days is news on All items with RESTART TO UPDATE (Lockdown 7). | Built |
| D1008 | The needs-you block sits under an open item's title (Lockdown 8). | Built |
| D1009 | OTHER DEVICE OR KEY (Lockdown 9). | Built |
| D1010 | "Card" and "Secure note" as the names people see (Lockdown 10). | Built |
| D1011 | LAST BACKUP as a clock time, the age as a tip (Lockdown 11). | Built |
| D1012 | Keys list: body words, mono keys, platform key names (Lockdown 12). | Built |
| D1018 | Rows swipe on a touch tablet at the medium width (Lockdown 18). | Built |
| D1019 | Search shows a code in its own row; Escape returns to where Search opened (Lockdown 19). | Built |
| D1020 | Trash days as a menu of its own; name prompts at 480 (Lockdown 20). | Built |
| D1022 | No export switch in the web vault (Lockdown 22). Amends D827. | Built |
| D1025 | "Last used {time}" on device rows with a session (Lockdown 25). | Built |
| D1026 | Frames keep the registrable domain whole; "Stop:" only on refusals and covers (Lockdown 26). | Built |
| D1027 | The popup's code is display only (Lockdown 27). | Built |
| D1028 | Large type follows the live code with its countdown (Lockdown 28). Amends D963. | Built |
| D1029 | A new login's password is made on save by default, masked (Lockdown 29). | Built |
| D1030 | Tab counts in the tab's name (Lockdown 30). | Built |
| D1032 | No quick-unlock button until it works; the web core's wrong-password words (Lockdown 32). Amends D842. | Built |
| D1033 | Migration's yeses: Continue on M1 when M2 follows, Lock my vault where the vault locks (Lockdown 33). | Built |
| D1037 | A passing CHECK NOW says "Checked {time}" on its row (Lockdown 37). | Built |
| D1038 | The crown-jewel switch's line; locked export kinds first (Lockdown 38). | Built |
| D1040 | Rows say clock time, the age as a tip. | Built |
| D1041 | Counts read three digits. | Built |
| D1042 | What can wait: a needs-you count on Settings, the block on the home screen. | Built |
