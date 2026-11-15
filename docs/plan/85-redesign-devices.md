# 85: Devices, Add a device, Signed in and Security keys on Field Rows

_Slice: `edges-2-devices` of the 2026-11 redesign, lane `edges` (branch `wt/lockdown-edges`), built 2026-11-15. Brief T2 (Devices) of the undrawn-areas brief, with A1's compare review for Add a device and X5's Question table; no drawn frame (Desktop / 08 and Mobile / 12 draw only a three-row Devices group inside Settings). Decisions D930 to D939. Answers Q2's Devices side and R14 for this screen._

## For future agent

- **TL;DR:** Settings → Devices is four sections of unicron Field Rows: DEVICES (this one first, platform cased, last use from the session list, REVOKE), SIGNED IN (END, END ALL OTHERS), SECURITY KEYS (ADD KEY, REMOVE) and a collapsed DETAILS with the ids. ADD DEVICE opens a review dialog with unicron's `QrCode`, the code and a time-left rule; when the new device joins it becomes the compare review on `ApprovalSheet`. Every taking-away asks a `ConfirmDialog` first.
- **Do:**
  - Keep `DevicesView`'s export and props (`status`, `onChanged`, `onNotice`, `focus`): Settings and the web vault mount it unchanged.
  - Read sessions once per screen with `useSessionList()` (`features/sessions/sessions-section.tsx`) and hand the list to `<SessionsSection list={…}/>`; the device rows take their last use from it (`lastUse`, `features/devices/devices-model.ts`).
  - Use `FieldRow size="stacked"` when `useViewport().widthClass === "compact"`, so a date is never cut on a phone.
- **Do not:**
  - Put an id on a row (R14): ids live under DETAILS.
  - Add a gate in the app before revoke or key removal: the Question asks, and the core runs its own `Approve` gate (D101) when it wants one (§2, D931).
  - Put the six digits or the code in `DigitGroups` alone: the joining side and `apps/web/e2e/device-link.spec.ts` compare their text (§4, gap 1).
- **Read with:** brief T2, A1 and X5 (workspace `07-projects/29-unicron/impl-2026-11/app-lockdown-undrawn.md`), doc 84 (requests and `ApprovalSheet`), doc 78 (the joining side), doc 22 §2 and §4 (device link, security keys), doc 20 §6 (sessions).

## 1. The screen (brief T2)

| Section | Row | Value | Description | Badge | Action |
|---|---|---|---|---|---|
| DEVICES NN (live count) | the device's name | "Now" for this device; else its newest session's last use (`Today 09:02`, `NOV 02`, row dates of found-3); else "Added 2026.03.11" | the platform, cased: macOS, iOS, iPadOS, Windows, Linux, Android (anything else as it came) | THIS DEVICE, or REVOKED 2026.10.02 (critical) | REVOKE on a live device that is not this one |
| under the list | "Only this device. A second device is what gets you back in when this one is lost." when one live device is left | | | | ADD DEVICE (secondary). A local account: one sentence and SET UP SERVER (goes to Settings → Server) |
| SIGNED IN NN | the session's device (or "Device 1a2b3c4d") | "last used Today 09:02" | "Lockdown 0.9.0 on macOS // from 203.0.113.7 // signed in 2026.11.01" | THIS DEVICE | END on every other session; header END ALL OTHERS |
| SECURITY KEYS NN | the key's name | "last used NOV 02", or "not used yet" | "added 2026.08.14" | | REMOVE; header ADD KEY |
| DETAILS (collapsed) | each device, then "Session // {device}" | its full id | | | |

- Order: this device first, then the live ones in the core's order, the revoked ones last.
- The caption under SECURITY KEYS: "A new device asks for one of these after your password." (or, with none, "Add a security key or a passkey so a stolen password alone can't sign a new device in.").
- A local account draws neither SIGNED IN nor SECURITY KEYS (as today). SIGNED IN draws nothing when the server lists no session.
- Arriving with a device named (Health's quantum-safe line, `nav.device`): that row is primary-soft with the 2 px edge, scrolled to the middle and focused (no ring: the row is a focus target, not a control).
- The web vault mounts the same view: DEVICES and SIGNED IN, ADD DEVICE, SECURITY KEYS (the page opens in this tab).
- A "new sign-in" finding already links here (vault lane); there is no NEW mark, because no finding names its device (W4).
- Errors are a compact critical `Banner` with the core's sentence.

## 2. Questions and the gate (brief X5)

| Action | Question (unicron `ConfirmDialog`, CANCEL focused) | Then |
|---|---|---|
| REVOKE | overline "{name} // {platform}", "Revoke {name}?", "It can't open the vault or sync from now on. What it signed before still verifies. A revocation never comes back.", REVOKE (critical) | `gate.run(deviceRevoke)`: the core asks its `Approve` gate (D101), toast with the core's lines |
| END | overline the device, "End the session on {device}?", "It signs out at once and needs the master password to sign in again. The device stays one of yours; revoke it too if you don't know it.", END | `sessionEnd`; no gate; toast "Ended the session on {device}" |
| END ALL OTHERS | overline SIGNED IN, "End every other session?", the same line, END ALL OTHERS | `sessionEndOthers`; no gate; toast "Ended N other sessions" |
| REMOVE (a key) | overline SECURITY KEY, "Remove {name}?", "A new device can't use it to sign in from now on. You can add it again later.", REMOVE | `gate.run(securityKeyRemove)`: the core's gate (D101); toast "Removed {name}" |

**Revoke and key removal still meet a gate, from the core.** X5 and the slice say that taking access away never asks for a gate (D196, D221). `App::device_revoke` and `App::security_key_remove` call `approve(ApprovalKind::Approve, …)` themselves (D101: "device revoke … gated today"), and Rust does not change in this work. So the app adds no gate of its own (the old screen's sentence "You will be asked to confirm it is you" is gone), and the core's gate appears only when the core answers `approval_needed`. Backing out of it is not an error. Dropping it is a core change for its own slice (D931, question 1).

## 3. Add a device (brief T2 steps 1 to 3, A1)

1. ADD DEVICE → `linkOffer`. A review dialog (`AdaptiveDialog size="review"`, a sheet on a phone): DEVICE LINK // 09:58 LEFT, "Add a device", "On the new device, choose “I already use Lockdown”, then scan this code or type the one under it. It works once, for ten minutes.", unicron's `QrCode` from the core's module rows (dark on the stone ground in both themes), the code with COPY (says "Copied" for 2 s, no toast), a segmented rule of 30 for the time left (critical in the last 20 s), "Waiting for the new device", CANCEL. The poll is today's (1.5 s).
2. When the link reports `compare` with six digits, the dialog becomes unicron's `ApprovalSheet`: NEW DEVICE // MM:SS LEFT, "Add {device_name}?" (or "Add the new device?"), "Approve only if {name} shows these six digits.", the digits, THEY DON'T MATCH and THEY MATCH. THEY MATCH runs `linkApprove` through the gate; THEY DON'T MATCH, THIS WASN'T ME, the close button and Escape decline the link. Ran out: "This request ran out." and CLOSE.
3. The core's lines as a toast.

Closing declines here, unlike A1's requests (D921): this device opened the link, nothing waits in the needs-you for it, and a link left open would let a device join later with no screen showing the digits. A link the core reports `declined` or `closed` says "This link closed before a device joined. Make a new one to try again." with CLOSE.

## 4. Gaps

### unicron host-contract gaps

1. **`DigitGroups` has no character between its groups** (shared with doc 78, gap 1). Its text is `418902` plus the spelled screen-reader span, so `apps/web/e2e/device-link.spec.ts` (foundation, frozen) cannot compare the two sides' text, and `ApprovalSheet`'s compare mode draws an unlabelled `DigitGroups`. The code (XXXXX-XXXXX) and the six digits stay plain labelled text in the code display role with tabular figures, and the compare review uses `matching={{ mode: "none" }}` with the digits in its `note`. **For the integrator:** switch both sides together (this view and `join-device.tsx`) to `matching={{ mode: "compare", code: sas }}` / `<DigitGroups label="Digits to compare"/>`, and change `digitsToCompare` (screens' `steps/unlock.ts`) to read `[data-slot="digit-groups-group"]` texts joined by a space. Or ask unicron for a real space text node between groups. Screens: `settings-devices--add`, `--add-compare`.
2. **`ApprovalSheet` draws a TRUST row and THIS WASN'T ME** (doc 84 gap 1). A1 gives Add a device neither. Ask `tier={null}` and `report={false}`. THIS WASN'T ME declines the link here. Screen: `settings-devices--add-compare`.
3. **`FieldRow` has no selected state.** The row a link names is primary-soft with the 2 px edge through `bg-primary-soft edge-l` on a wrapper (ListRow's own classes). Ask `selected` on `FieldRow`. Screen: `settings-devices--from-health`.
4. **`FieldRow` takes no `role`.** Each row sits in a `<div role="listitem">` so the lists keep their names and the tests and e2e find rows. Ask a `render` prop or `role`. Screens: every Devices section.

### Lockdown gaps

- **The core gates revoke and key removal** (§2). A core change.
- **No device on a "new sign-in" finding** (W4): no NEW mark.
- **The joining side's words** (screens lane, `join-device.tsx`): "open Devices and choose “Add a device”" should become "choose ADD DEVICE" now that the button says ADD DEVICE (no article). Not edited here (screens owns it).
- **`features/depth/qr-code.tsx` is deleted** (plan note 9 had it for the fold): once unicron's `QrCode` is used, a local component of the same name counts as a look-alike. Its test in `depth.test.tsx` now draws unicron's `QrCode` from the same core rows, with the same assertions (five dark runs, one path, the image named).
- **Look-alike for the vault lane:** `features/vault/wifi-card.tsx` (vault, brief I9) has its own local `function QrCode`. Now that unicron's `QrCode` is used in the app, that local function reads as a look-alike (1.00, `QrCode = QrCode`), so the estimator says 1 on this branch alone (baseline 0). The fix is I9's own: the guest card's code through unicron's `QrCode` (`modules` from its grid), or the local function renamed. Not edited here (vault owns it); the integrator must see it at 0 after the merge.
- `PaneHeader` is no longer used by Devices.

## 5. Tests and what is not proven

- `features/devices/devices-view.test.tsx` (new, 9): order, cased platforms, last use from the session list, Added date, REVOKED badge, ids only under DETAILS; **a revoke asks first and adds no gate of its own** (CANCEL focused; the gate dialog never appears when the core does not ask; `deviceRevoke(id, {})` once); Cancel revokes nothing; only this device with ADD DEVICE, SIGNED IN and SECURITY KEYS; a local account with SET UP SERVER and neither section; a refusal as an alert; the row a link names; the words helpers.
- `features/sessions/sessions-section.test.tsx` (6): rows with `//` descriptions and THIS DEVICE; END and END ALL OTHERS each ask, **no gate follows**, toasts without full stops; live reload and a refusal; **a list handed by the screen is not read again**; nothing without a live client; client words.
- `features/depth/add-device.test.tsx` (4): the offer (sentence, code, unicron QR with one path, time-left rule and overline), the compare review on `ApprovalSheet` (NEW DEVICE overline, the digits, THEY MATCH through the gate with the core's lines); THEY DON'T MATCH declines; COPY and CANCEL (declines); a refusal.
- `features/depth/security-keys.test.tsx` (new, 3, split from the old untested section): rows; ADD KEY's toast; REMOVE asks, Cancel removes nothing, REMOVE removes with the core's gate input.
- `components/labelled-actions.test.tsx` (foundation file): its R1 revoke case rendered Devices' old dialog through `LabelledActions`. Revoke now uses unicron's `ConfirmDialog`, so the case moved into the unicron group (`expectComputedWords` on CANCEL and REVOKE). `LabelledActions` itself keeps its own case (a direct render with Cancel then Revoke, the same assertions), because other dialogs still use it until the fold. No assertion dropped.
- `features/security/pq-status-line.test.tsx` (vault lane) passes unchanged: the row a link names says `aria-current` and takes focus.
- Web e2e (`steps/devices.ts`): ADD DEVICE, the code by its label, the compare review by its title `Add …?`, THEY MATCH; SIGNED IN by its heading; a device row inside the Devices list. `device-link.spec` and `live.spec` unchanged.
- Capture states (ids kept, `built`): `desktop-devices` (`settings-devices--list`), `-only-this`, `-revoke`, `-add`, `-add-compare`, `-end-session`, `-from-pq` (`--from-health`), `web-devices` (`web-devices--list`).

**Not proven:** the real platform gate over the Question (the capture fakes routes); the joining device's own screen beside this one in the capture (the e2e drives both); a scan of the QR by a phone camera; the Tauri window; WebKit.

## 6. Proposed decisions

| Id | Decision | Status |
|---|---|---|
| D930 | **Devices on Field Rows** (T2, R14): this device first with "Now", the others with their last use from the session list or "Added {date}", platforms cased, THIS DEVICE and REVOKED {date} badges; the ids under a collapsed DETAILS. Plain version: the list says when each device was last used, in words people use. | Proposed; **flagged for Sean** (T2, R14) |
| D931 | **REVOKE and REMOVE ask a Question; the app adds no gate.** The core's `Approve` gate (D101) still runs for revoke and key removal; X5 and D196 / D221 would drop it, which is a core change proposed for its own slice. Plain version: you confirm once in words; the vault still asks for your password or Touch ID before a device is cut off, until Sean decides. | Proposed; **flagged for Sean** (X5; conflicts with D101) |
| D932 | **END and END ALL OTHERS** (D221 kept: a Question, no gate). Words: END ALL OTHERS (was "End all other sessions"), toasts "Ended the session on {device}" and "Ended N other sessions" (no full stop, one sentence). Plain version: same behavior, shorter words. | Proposed |
| D933 | **Add a device as two layers** (T2, A1): the offer (QR, code with COPY, time-left rule) and the compare review on `ApprovalSheet` (NEW DEVICE // MM:SS LEFT, "Add {name}?", THEY MATCH through the gate, THEY DON'T MATCH). Closing either declines the link (unlike D921), because this device opened it. Plain version: scan or type, then check six digits. | Proposed; **flagged for Sean** (T2) |
| D934 | **The code and the six digits stay plain labelled text** until both sides and the frozen e2e move to `DigitGroups` together (gap 1, with doc 78). | Proposed |
| D935 | **Only this device is one line under the list**, not an empty-state block; a local account shows one sentence and SET UP SERVER instead of ADD DEVICE. | Proposed; **flagged for Sean** (T2 states) |
| D936 | **Security keys**: ADD KEY opens the server's page and a toast says to come back; REMOVE asks (X5); "Removed {name}" toast. | Proposed |
| D937 | **One session read per screen**: Devices reads the session list once (`useSessionList`) and hands it to SIGNED IN; SIGNED IN draws nothing when the server lists no session. | Proposed |
| D938 | **Phone rows stack** (`FieldRow size="stacked"` under 600 px), so names and dates are never cut. | Proposed |
| D939 | **Devices stays a Settings section (Q2)** with its second ways in: Health's quantum-safe line selects a device here, and a new-sign-in finding opens it (W4: no NEW mark). | Proposed; **flagged for Sean** (Q2) |
