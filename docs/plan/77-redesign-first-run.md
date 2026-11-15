# 77: First run in the focus frame

_Slice: `screens-2` of the 2026-11 redesign, on `wt/lockdown-screens` (from `claude/keen-ritchie-q2hgb2`), built 2026-11-15. No drawn frame; brief F2 of the undrawn-areas brief; cross-app decisions §4, §9, §12, §13. Decisions D850 to D859._

## For future agent

- **TL;DR:** every first-run screen is Unicron's focus frame through `WelcomeColumn` (`features/unlock/welcome.tsx`), which is now `ClosedFrame` (doc 76) with a `folio`. `stepFolio([n, of])` writes `Step 01 // 04` for the folio at the end of the lockup's row. `WelcomeScreen` draws the title (`type-title`, the one `h1`, focused on each step), one sentence, and LEARN MORE, a text button that opens a question dialog with GOT IT. `WelcomeActions` stacks full-width buttons, and on a phone pushes them to the foot of the column. Joining, the migration and forgot draw in the same frame now; screens-3 restyles their insides.
- **Do:**
  - Keep one solid primary per step; a disabled primary shows its reason under it (`disabledReason`, #37) and stays in the tab order.
  - Use Button `loading` for work in flight: the label never changes.
  - Say skip as "Not now" and next as "Continue" (cross-app §4).
- **Do not:**
  - Add a server word on the create path (the test checks every screen).
  - Bring back the bordered kit card or the Lockdown mark: the lockup is the frame's.
  - Draw a fake strength meter: the field's own 3-segment rule takes the core's word.
- **Read with:** brief F2 and 4.0 (workspace `07-projects/29-unicron/impl-2026-11/app-lockdown-undrawn.md`), doc 39 (first run), doc 76 (the closed frame).

## 1. The screens (brief F2)

| Screen | Folio | What is on it | Primary | Others |
|---|---|---|---|---|
| Welcome | none | Title, sentence, LEARN MORE. What discovery found, as a compact info `Banner` (the words of doc 39: network, policy, settings, this computer) | NEW VAULT | I ALREADY USE LOCKDOWN (secondary); EXPLORE DEMO (text) in a build with the demo |
| Master password | `Step 01 // 0N` | `TextField` "Your name" with its helper; `SecretField` edit "Master password" with the core's strength in the field's 3-segment rule and word, today's help line as its description, Caps Lock as a compact info Banner; `SecretField` "Type it again"; the caption "If you forget it, only your recovery kit gets you back in. You'll make the kit next." | CREATE MY VAULT (keeps its label while working; "This takes a few seconds." under it) | BACK (secondary) |
| Recovery kit | `Step 02 // 0N` | `SectionHeader` "Recovery kit for {name}" (`h2`); the Secret Key and recovery code in `LargeTypeView`; where to keep it; SAVE AS PDF and PRINT (secondary, side by side); the saved line; a failed PDF as a compact critical Banner; the checkbox | CONTINUE, disabled with "Save, print or write down the kit first." until the box is ticked | none |
| Quick unlock | `Step 03 // 04` | Only where the computer has Touch ID or Windows Hello. Title, sentence, LEARN MORE, "You can change this later in Settings → Lock." | USE TOUCH ID / USE WINDOWS HELLO, disabled with the core's `why` where it doesn't work yet | NOT NOW (secondary) |
| Bring your passwords | `Step 0N // 0N` | A `NavSection` "Bring them in from": one `NavItem` per importer the build has (its name, its how-to line as the description), then "Other apps and authenticator files" ("Bring in another" once one is in). Each opens today's `ImportWizard` with that app chosen. After one: "{n} items brought in. Delete the exported file now: it isn't protected." | CONTINUE, disabled with "Nothing brought in yet. Pick an app above, or Not now." until something is in | NOT NOW (secondary), until something is in |
| The proof | none | Three lines, each with a `checkmark` | OPEN VAULT (keeps its label while the vault opens) | none |

The folio counts four steps where the computer has quick unlock (working or not), three where it has none. A first run that stopped after the vault was made still opens again at the kit (`UnlockScreen`, unchanged).

States kept: the name and the two passwords are checked before anything is made, each under its field; the core's floor goes under the password; any other failure is a compact critical Banner over the buttons (brief 4.0, a form that could not save). The kit's values hide after a minute or on blur, with "The kit is hidden while you're away from it." and SHOW AGAIN. Capture protection stays on while they show.

## 2. Shared parts this slice changed

- `welcome.tsx`: `WelcomeColumn` is `ClosedFrame`; its `locked` prop is kept and does nothing (the wash went with Q15). `LockdownMark` is gone (only the old Welcome used it). `WelcomeScreen` lost `step`, `mark` and `centered` (only the first run passed them); the folio moved to `WelcomeColumn`.
- `master-password-fields.tsx`: the hand-made meter (`StrengthMeter`) and the shared Show button are gone; each password is a `SecretField` with its own Show. The migration and forgot use it unchanged in their props.

## 3. Tests and what is not proven

- `first-run.test.tsx`: the whole walk with every folio (`Step 01 // 04` to `04 // 04`, none on Welcome and the proof), one primary on each step, the strength word from the field's rule, Continue's reason on the kit and on import, the disabled Use Touch ID with the core's reason; **new:** Skip is "Not now" and Next is "Continue" with three steps where the computer has no quick unlock; Create my vault keeps its label while it works; the core's floor marks the password field invalid; any other failure is an alert Banner; Explore demo keeps its label while it opens.
- `unlock-screen.test.tsx` (resume at the kit, the migration) passes unchanged.
- Capture: `desktop-first-run-*` states carry `built` (welcome, learn-more, password, weak-password, kit, kit-saved, import, proof) and a new `desktop-first-run-quick` (quick-unlock). The steps helper's `create` presses NEW VAULT and CREATE MY VAULT; `quickSkipped` passes the Touch ID step. `desktop-devtools-first-run` waits for "Explore demo".
- Not proven: a real Touch ID or Windows Hello step (no command turns it on; the button continues as before where the core says it is available); printing on paper; WebKit; the Tauri shell; `pnpm ds:ratchet` (counted by reading: className 0, look-alikes 0).

## 4. Gaps

### unicron host-contract gaps

1. **`LargeTypeView` in a 432 column** sets one group of four per line, so the kit runs well past one screen (as before). Ask: a two-column layout for print mode in a narrow column, or a `compact` size. Screen: first run, kit.
2. **The folio role is mixed case** (`Step 01 // 04`); the brief writes `STEP 01 // 04`. The contract's own example is mixed case; kept. Ask: confirm.
3. **A ghost text button can't hang its word on the column edge** (doc 76 gap 2). Screens: LEARN MORE on every step, SHOW AGAIN.
4. **No secondary-pair layout** for two equal secondary buttons in a column (SAVE AS PDF, PRINT); the app uses a two-column grid of block buttons.

### Lockdown gaps

- USE TOUCH ID where the core says quick unlock is available continues without turning anything on, as before this slice; nothing in the core enables it yet (doc 39 R4).
- `--c-welcome-*` knobs in `app-extension.css` (foundation) are still used by join, migration, forgot and the server picker; screens-3 can drop them.

## 5. Proposed decisions

| Id | Decision | Status |
|---|---|---|
| D850 | **The first run is the focus frame,** with the step folio at the end of the lockup's row (`Step 01 // 04`), a `type-title` heading, one sentence and LEARN MORE as a text button under it. Plain version: setting up Lockdown looks like the lock screen, one question at a time. | Proposed; **flagged for Sean** (F2) |
| D851 | **Button words follow cross-app §4 and §12:** NEW VAULT, CREATE MY VAULT, CONTINUE, NOT NOW, OPEN VAULT, EXPLORE DEMO (no articles). "Skip for now" is gone. Plain version: the buttons say the same things in every app. | Proposed; **flagged for Sean** |
| D852 | **What discovery found on Welcome is a compact info Banner, not a link;** I ALREADY USE LOCKDOWN is the one way to join. Plain version: Welcome tells you about a server it found without making it a second door. | Proposed; **flagged for Sean** (F2) |
| D853 | **The quick unlock step shows wherever the computer has Touch ID or Windows Hello, its yes disabled with the core's reason until it works;** the folio counts it. Plain version: you see the fingerprint step on a Mac, and it says why it can't be turned on yet. | Proposed; **flagged for Sean** (#37; answers F2's "only where the computer has it") |
| D854 | **CONTINUE on Bring your passwords waits for something brought in, with its reason; NOT NOW is the way past an empty step.** Plain version: Continue means you brought passwords in; Not now means later. | Proposed; **flagged for Sean** (#37 "nothing to bring in") |
| D855 | **The master password fields are SecretFields** with the core's 3-segment strength and word in the field, each with its own Show; the hand-made meter goes. Plain version: the password boxes look and work like every secret box in Lockdown. | Proposed |
| D856 | **A first-run failure that isn't a field's is a compact critical Banner over the buttons** (brief 4.0); a field's error stays under its field. Plain version: a problem shows where you are looking. | Proposed |
| D857 | **The kit's two save buttons are secondary, side by side; CONTINUE is the one primary,** disabled with "Save, print or write down the kit first." until the box is ticked. Plain version: one green button per step, and it says why it is waiting. | Proposed; **flagged for Sean** |
| D858 | **Importers are Nav Items under "Bring them in from",** each with its how-to line. Plain version: each app you can bring passwords from is one row. | Proposed |
| D859 | **The proof's OPEN VAULT keeps its label while the vault opens.** Plain version: the last button doesn't change its words. | Proposed |
