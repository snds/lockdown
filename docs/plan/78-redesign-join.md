# 78: Joining, existing installs and a forgotten password in the focus frame

_Slice: `screens-3` of the 2026-11 redesign, on `wt/lockdown-screens` (from `claude/keen-ritchie-q2hgb2`), built 2026-11-15. No drawn frame; briefs F3 and F4 of the undrawn-areas brief; cross-app decisions §4, §9, §10, §12. Decisions D860 to D869._

## For future agent

- **TL;DR:** "I already use Lockdown" (`JoinScreen`, `features/unlock/join.tsx`) is the focus frame with three `RadioRow`s (another device, sign in, the kit). The device way shows a folio of three steps (`Step 01 // 03` the code, `02` the six digits, `03` the password) through `JoinWithDevice`'s new optional `onStage`. Before anything is sent to a server, a `ConfirmDialog` (focus on Cancel) shows the server's facts as Field Rows and its key in `DigitGroups`; its yes is Connect. The second-factor waits (`SecondFactorWait`) have their own titles. Existing installs (`migration.tsx`) and a forgotten password (`forgot-password.tsx`) are the focus frame with no folio; every typed kit line is a `SecretField`. `ServerPicker` (shared with Settings → Server) is `RadioRow`s with text buttons.
- **Do:**
  - Keep `JoinWithDevice`'s export and props (the web sign-in's With a device tab renders it); add props only.
  - Keep one solid primary per screen: Sign in, Recover, Join, Finish, Lock my vault, Continue, Check kit, Save new password. Working buttons keep their labels (Button `loading`).
  - Say a refusal in the core's words: under its field when it is the field's (a malformed code, a foreign QR, a weak password), otherwise a compact critical `Banner` over the buttons.
- **Do not:**
  - Put the six digits in `DigitGroups` without changing `apps/web/e2e/device-link.spec.ts` and the approving side together (§4, gap 1).
  - Draw the server list and the device way's Server field together: one way to name the server.
  - Add a "Not now" to the migration (Q2 of the first-run plan).
- **Read with:** briefs F3 and F4 (workspace `07-projects/29-unicron/impl-2026-11/app-lockdown-undrawn.md`), doc 39 (first run), doc 45 (the sealed join), doc 76 (the closed frame), doc 77 (the first run).

## 1. The screens (brief F3)

| Screen | Folio | What is on it | Primary | Others |
|---|---|---|---|---|
| Ways in | none (one step), `Step 01 // 03` on the device way | Title "Add this computer to your Lockdown", one sentence, LEARN MORE. `RadioGroup` "How to add this computer" of three `RadioRow`s (today's words). The server list (`ServerPicker`). The way's fields | SIGN IN (sign in), RECOVER (the kit), JOIN (the device) | BACK (text) |
| Sign in | none | Login (`TextField`, "Often an email address."), Master password (`SecretField`, "The one you use on your other devices."), Secret Key (`SecretField`, mono, "On your recovery kit page. It starts with A1-.") | SIGN IN | BACK |
| The kit | none | The same, the Secret Key required, and Recovery code (`SecretField`, mono) | RECOVER | BACK |
| Another device: the code | `Step 01 // 03` | Today's line about Devices → Add a device; Server (`TextField`, only when no server is known: "Not needed when you paste the QR code's text."); Code from the other device ("Ten letters and digits, like 7KQ2M-9XH4P.") | JOIN | SCAN QR CODE (text, where there is a camera: the web), BACK |
| The six digits | `Step 02 // 03` | The core's words; the six digits (`type-code-display`, labelled "Digits to compare"). The ways, the list and BACK hide while the join runs | none | CANCEL (text): ends the join on both devices |
| Approved | `Step 03 // 03` | The core's words; Login name; Master password (`SecretField`) | FINISH | BACK |
| Confirm the server | dialog | `ConfirmDialog` "Connect to {host}?", "Only continue if you recognize this server. Lockdown checks this key every time it syncs.", Field Rows ADDRESS, FOUND, SERVER KEY (`DigitGroups size="sm"`), POLICY when set | CONNECT | CANCEL (focused) |
| Security key | none | "Use your security key.", today's sentence, a live line "Waiting for your security key." | none | OPEN PAGE AGAIN, CANCEL (text) |
| Email | none | "Check your email.", the core's words with the masked address, "Waiting for the approval." | none | CANCEL (text) |

States: the other device said no or the code ran out: a compact critical `Banner` in the core's words with TRY AGAIN, which empties the code field and puts the focus in it. A pasted QR that isn't Lockdown's: under the code field. A refused sign-in: a compact critical `Banner` over the buttons. When discovery found no server, the device way shows only its Server field (the list and its Look again stay on the other two ways); a found server hides the field.

## 2. The screens (brief F4)

All in the focus frame, no folio: this happens once, not as a flow.

| Screen | Title | Controls | Primary | Others |
|---|---|---|---|---|
| M1, an install with no account | "Lock your vault with a master password" | Your name; master password with the core's 3-segment strength; Type it again (`MasterPasswordFields`, doc 77) | LOCK MY VAULT | none (no "Not now") |
| M2, that install has a kit | "Type your recovery kit" | Secret Key, Recovery code (`SecretField`s, mono) | CONTINUE, disabled with "Type both lines from your kit first." | BACK (secondary); I DON'T HAVE IT (text) opens a `ConfirmDialog` "New recovery kit?", "Your old kit page stops working.", yes NEW KIT |
| M3, an account on a server | "Unlock with your password from now on" | Master password (`SecretField`), "The password you sign in to {host} with." | CONTINUE, disabled with "Type your password first." | CONTINUE FOR NOW (secondary) once the server can't be reached, under a compact critical Banner saying so |
| Forgot, K1 | "Use your recovery kit" | Secret Key ("It starts with A1-."), Recovery code | CHECK KIT, disabled with its reason | BACK (secondary) |
| Forgot, K2 | "Choose a new master password" | Master password with its strength, Type it again; the sentence "Your old password stops working once you save the new one." | SAVE NEW PASSWORD | BACK (secondary) |

A kit that doesn't open the vault comes back to K1 with the core's words in a compact critical Banner. LOCK MY VAULT, CONTINUE and SAVE NEW PASSWORD keep their labels while working (today they said "Locking your vault…" and "Saving…").

## 3. Shared parts this slice changed

- `features/depth/join-device.tsx`: `JoinWithDevice` gains `onStage?: (stage: "code" | "compare" | "finish") => void` (export `JoinStage`); its props are otherwise unchanged. `SecondFactorWait` draws its own `ClosedHeading`. `OptOutRegister` is untouched (nothing renders it today).
- `features/server/server-picker.tsx` (Settings → Server shares it; screens-7 owns the section): `RadioGroup` of `RadioRow`s under an overline, LOOK AGAIN and TYPE AN ADDRESS as text buttons, the typed address as a `TextField` with CHECK (secondary). Props unchanged.
- `ServerFacts` (join.tsx; Settings → Server's confirm draws it too): Field Rows and `DigitGroups` for the key. Props unchanged.
- `features/unlock/welcome.tsx`: `useFormFill()` moved here from `first-run.tsx` (no change in behaviour), so joining, the migration and forgot share it.
- No `--c-welcome-*` knob is used any more by the screens lane's files. The knobs stay in `app-extension.css` (foundation) for the integrator to delete.

## 4. Tests and what is not proven

- `depth/join-device.test.tsx`: the split tests, unchanged in meaning, with the new button words (Sign in, Recover, Open page again) and the wait titles. **New:** the six digits show, then the other device's no is an alert Banner whose Try again empties and focuses the code field; with no server found the device way shows the Server field, one Join and `Step 01 // 03`, and no Type an address; the server question opens with the focus on Cancel and Cancel sends nothing. "A cancelled wait opens nothing" is kept.
- `unlock/unlock-screen.test.tsx`: Check kit, Lock my vault (renamed controls).
- `server/server-settings.test.tsx`: a not-ready server is `aria-disabled` (Unicron's radio is a span, so `toBeDisabled` can't see it); the key is read from its `DigitGroups` groups.
- Capture: `desktop-first-run-join`, `-join-confirm`, `-join-device`, `-join-denied`, `desktop-migration-name`, `-server`, `desktop-unlock-forgot`, `-forgot-new` keep their ids and gain `built`. **New:** `desktop-first-run-join-no-server`, `-join-compare`, `-join-approved`, `-join-device-denied`, `-join-security-key`, `-join-email`, `desktop-migration-kit`.
- Not proven: a real camera scan (web only; untouched); a real security key or emailed link; WebKit; the Tauri shell; `pnpm ds:ratchet` (counted by reading: className 0, look-alikes 0).

## 5. Gaps

### unicron host-contract gaps

1. **`DigitGroups` has no separator in its text.** Its groups are spans with the gap in CSS and the spelled form in a screen-reader span, so `textContent` (and a copied selection) reads `418902Digits to compare: 4 1 8, 9 0 2`. The web's frozen `device-link.spec.ts` checks the joining side for `^\d{3} \d{3}$` and compares it with the approving side's text, so the six digits stay plain text here until both sides and the spec change together. Ask: a real space text node between groups (still `aria-hidden`), or a `data-value`. Screens: join, the six digits; Settings → Devices, Add a device.
2. **`ApprovalSheet` has no kind for trusting a server** and requires "This wasn't me" and a 60 s timeout; neither means anything for a server's key. The join's key review is a `ConfirmDialog` with Field Rows instead (brief T5 asks for an ApprovalSheet `matching="none"`). Ask: a `server` kind, or `onReport` optional. Screens: join, confirm the server; Settings → Server.
3. **`RadioRow` disabled is `aria-disabled` only.** Fine for assistive tech; a test or a form library that reads `disabled` sees an enabled radio. Ask: say so in the docs.
4. **A ghost text button can't hang its word on the column edge** (doc 76 gap 2): BACK, CANCEL, I DON'T HAVE IT, LOOK AGAIN use a `-ms-4` wrapper.

### Lockdown gaps

- `OptOutRegister` (join-device.tsx) is exported and rendered nowhere; it still uses `LabelledActions`. Left for its owner.
- `ServerPicker`'s typed-address form is a `<form>` of its own; it sits beside the join's form, never inside it.

## 6. Proposed decisions

| Id | Decision | Status |
|---|---|---|
| D860 | **"I already use Lockdown" is the focus frame with three Radio Rows**; the device way counts three steps in the folio (code, digits, password), the other two ways are one step and show none. Plain version: you pick how to add this computer, and the device way tells you which of three steps you are on. | Proposed; **flagged for Sean** (F3; cross-app §4) |
| D861 | **Each way's yes names what it does:** SIGN IN, RECOVER, JOIN, FINISH, where today all said CONTINUE; BACK is a text button. Plain version: the green button says what happens. | Proposed; **flagged for Sean** (cross-app §12) |
| D862 | **The server's key is a question, not a review sheet:** focus on Cancel, Field Rows, the key in DigitGroups, yes CONNECT. Plain version: before Lockdown talks to a server it shows you its key and waits. | Proposed; **flagged for Sean** (gap 2; brief T5) |
| D863 | **One way to name the server:** when discovery found none, the device way shows only its Server field. Plain version: you never see an empty list and a field for the same thing. | Proposed (F3, P17) |
| D864 | **A device join under way holds the screen:** the ways, the list and BACK hide while the digits show; CANCEL ends it. Plain version: while you compare digits, nothing else asks for your attention. | Proposed; **flagged for Sean** |
| D865 | **The second-factor waits get titles:** "Use your security key." and "Check your email.", with a live line and text buttons. Plain version: a wait says what it waits for. | Proposed (F3) |
| D866 | **Refusals sit where the person looks:** a field's under the field; the other device's no as a compact critical Banner with TRY AGAIN; a refused sign-in over the buttons. No toast. Plain version: an error shows next to what caused it. | Proposed (brief 4.0; cross-app §9) |
| D867 | **Existing installs and a forgotten password are the focus frame with no folio;** LOCK MY VAULT, CHECK KIT, SAVE NEW PASSWORD keep their labels while working; the new-kit question is "New recovery kit?" with NEW KIT. Plain version: these one-time screens look like the lock screen and never change their button words. | Proposed; **flagged for Sean** (F4; cross-app §12 "No Make") |
| D868 | **Every kit line typed here is a SecretField** (masked, with Show), where forgot and the migration showed the Secret Key and recovery code as plain text. Plain version: your kit is hidden while you type it, like a password. | Proposed; **flagged for Sean** (new question: masked kit entry) |
| D869 | **The server list is Radio Rows with text buttons,** in joining and in Settings → Server. Plain version: choosing a server looks like every other choice in Lockdown. | Proposed (T5; screens-7 continues) |
