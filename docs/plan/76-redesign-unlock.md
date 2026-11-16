# 76: Locked and unlock, web sign-in and unlock, link pages and key pages

_Slice: `screens-1` of the 2026-11 redesign, on `wt/lockdown-screens` (from `claude/keen-ritchie-q2hgb2`), built 2026-11-14. Drawn frames Mobile / 13 Locked and Desktop / 10 Locked; briefs F1, W1 and W2 of the undrawn-areas brief; cross-app decisions §4, §9, §13, §14. Decisions D840 to D849._

## For future agent

- **TL;DR:** every screen before the vault, and every page without one, is drawn in `ClosedFrame` (`features/unlock/closed-frame.tsx`): unicron's `AppFrame` holding only `AppFrameFocus` (the 432 column, the lockup at its top) and, on a phone, a detail `AppFrameBottomRow` whose end is the primary float button. `ClosedHeading` draws the overline, the display title (the one `h1`) and the sentence. `lockedLine(reason, idleSecs, name)` writes the lock overline for the desktop and the web vault alike.
- **Do:**
  - Use `ClosedFrame` and `ClosedHeading` for the first run, joining, migration and forgot (screens-2, screens-3) so the closed app is one frame.
  - Draw a typed secret with `SecretField mode="edit"`; hold it in state only while the form is up and clear it as soon as it is sent.
  - Find these fields by their exact label in e2e and component tests: every `SecretField` has a "Show …" button whose name contains the label.
- **Do not:**
  - Draw anything of the shell, a list, a vault's name or a skeleton row behind Locked; do not animate the lock.
  - Make "Unlock with Touch ID / Windows Hello" the primary until a command opens the vault with it.
  - Raise a toast for an error on these screens.
- **Read with:** brief F1, W1, W2 and 4.0 (workspace `07-projects/29-unicron/impl-2026-11/app-lockdown-undrawn.md`), doc 39 (first run and unlock), doc 68 §4 (Lock), doc 17 (the web vault), doc 18 §7 (link pages), doc 22 §4 (key pages).

## 1. Locked (F1; Mobile / 13, Desktop / 10)

| Part | What it shows | From |
|---|---|---|
| Overline | `LOCKED`, the reason (`IDLE 10:00` with the timer that fired, `OUT OF VIEW`, `SLEEP`), then the person's name (Q15) | `LockState.reason` (prop), `lockState().idle_timeout_secs`, `Probe.name` |
| Title | "Lockdown is locked." | |
| Sentence | "Your vault is encrypted on this Mac. Your master password opens it." (`PC` off a Mac). After the gate's third wrong password: "The password was wrong three times, so the vault locked." | `triesLocked` (new optional prop) |
| Field | `SecretField` edit, "Master password", Show, focused; the core's words under it on a wrong try; "Type your master password." on an empty Unlock | |
| Caps Lock | a compact info `Banner` under the field | key events |
| Primary | `UNLOCK` (Button, keeps its label while it works). On a phone the primary `FloatButton` with `unlocked`, named "Unlock", in the bottom row | |
| Quick unlock | left out unless `quickUnlock().available`; then a secondary, disabled button with the core's `why` as a visible line (#37). Never the primary: no command opens the vault with it yet | `QuickUnlock` |
| Text button | `FORGOT YOUR PASSWORD? USE YOUR RECOVERY KIT`; after three wrong tries `STILL NOT IT? …` | |

States: starting (the lockup and one skeleton bar), can't start and can't open (`EmptyState tone="critical"`, the core's sentence, TRY AGAIN, which probes again), not enrolled (title and today's paragraph, no action), a device that opens with its own key (sentence, UNLOCK, no field). `locked-wash.tsx` is deleted; no code imported it after found-6.

## 2. The web vault (W1)

- **Sign in:** overline `WEB VAULT // <host>`, "Sign in.", the sentence, line `Tabs` SIGN IN · WITH A DEVICE · NO ACCOUNT YET?. Login name (`TextField`), master password and Secret Key (`SecretField` edit, the Secret Key with its hint). Any refused sign-in, too many tries included, is one compact critical `Banner` over SIGN IN in the server's words; the password is cleared.
- **Steps:** a security key: "Use your security key.", USE MY SECURITY KEY, CANCEL. The emailed link: "Check your email.", the server's line in a polite live region, CANCEL. Each step moves focus to its title.
- **Unlock:** as F1 with the tab's login in the overline, "Your vault is encrypted in this tab.", and the text button SIGN OUT OF THIS TAB. `sessionEnded` (new optional prop) puts the session-ended sentence in place of the usual one.
- **Ready for `web-app.tsx` (foundation-owned, not edited here):** `WebStarting`, `WebCantStart` (`EmptyState tone="critical"` with Try again) and `NotSyncedQuestion` (a `ConfirmDialog`, focus on Cancel, "Not synced, so still signed in.", SIGN OUT ANYWAY). Until the integrator swaps them in, starting and can't-start draw today's words inside the new frame, and sign-out with unsynced changes is today's toast with its action.
- **What the web shows (W1 table):** checked against found-3's web places in `app/places.ts`; this slice draws nothing the web client refuses. Every screen here is outside the vault.

## 3. Link pages and key pages (W2)

- **Share link:** "Someone shared something with you.", the sentence, the views left and the last day as mono lines (`01 VIEW LEFT`, `OPENS UNTIL 2026.11.20`), the link's password as `SecretField` edit when it has one, OPEN LINK.
- **Opened:** the title (or "Shared with you"), an info compact `Banner` "Keep what you need now. The link may not open again.", one row per value: `SecretField` read (reveal by a press, copy through the page's clipboard by reference to its copy number) for a secret, a `FieldRow` for text, a `FileChip` with a save button for a file. The values' own Show and Copy buttons and the copied toast are gone.
- **Request link:** "Someone asked you for a secret.", what they asked for, "What it is (optional)", `SecretField` "The secret" with its caption, SEND; then "Sent.".
- **Key pages:** "Confirm a new sign-in." with USE MY SECURITY KEY, then "Done."; "Add a security key." with Name and ADD KEY, then "Added.".
- **Gone:** a link or a key request that can't be read is `EmptyState tone="critical"` in the server's words with TRY AGAIN. Looking one up is a skeleton bar under the title.

## 4. Tests and what is not proven

- `unlock-screen.test.tsx`: the overline (idle timer, name, out of view), focus in the field, wrong tries in the core's words, Unlock keeps its label while it works, the empty field, Caps Lock's Banner, Touch ID left out where this build has none, quick unlock that isn't ready as disabled with its reason, the third-try sentence, starting, can't start with Try again, not enrolled, and **"renders nothing readable behind Locked"** (replaces the wash's test: the screen asks the core for `probe` and `lockState` only; no list, navigation, row, skeleton or sidebar; one heading).
- `sign-in-screen.test.tsx`: the overline and tabs, the compact Banner, Sign in keeps its label, the email step's title, the session-ended sentence and sign-out, `NotSyncedQuestion` with focus on Cancel.
- `key-pages.test.tsx`: ADD KEY, the done titles, a gone request as a critical Empty State with Try again and no key button, the key button keeps its label.
- `app/locked.test.tsx` (foundation): its "nothing of the shell" selector now allows the focus frame's own parts and asserts the focus frame is there. Recorded in the hand-off for the integrator.
- e2e helpers in `apps/web/e2e/steps/unlock.ts` find fields by exact label and the link value inside its SecretField. Sign-in, the device-link join, lock, unlock and sign-out run through them.
- Not proven: the phone float button on a real phone keyboard; WebKit; the Tauri shell; `pnpm ds:ratchet` (counted by reading: className 0, look-alikes 0).

## 5. Gaps

### unicron host-contract gaps

1. **`EmptyState` carries its own inset** inside `AppFrameFocus`, so the can't-start and gone-link states sit 8 px in from the lockup's edge. Ask: no outer padding (or a `flush` prop) for an Empty State that is a page's only content. Screens: Locked can't start, link pages gone.
2. **A text button can't hang its word on a column's edge.** The ghost Button's 16 px padding sets "Forgot your password?" in from the field's label; the app wraps it in a plain box with a negative start margin. Ask: a `hang` (or `flush`) option on ghost buttons. Screens: Locked, web unlock, the email step.
3. **`EmptyState`'s title is a `div`.** A full-screen state needs it as the page's `h1`; the app passes an `h1` as the title. Ask: a `titleAs` (heading level) prop.
4. **`AppFrameFocus` can't take a primary for the phone's bottom row by itself:** the app adds `AppFrameBottomRow variant="detail"`. Fine, but the contract says "one primary" without saying where it sits on a phone; worth a sentence in the migration guide.
5. **`SecretField` edit has no key events,** so Caps Lock is read from a box around it.

### Lockdown gaps

- `App.tsx` and `web-app.tsx` are foundation-owned: `triesLocked`, `sessionEnded`, `WebStarting`, `WebCantStart` and `NotSyncedQuestion` wait for the integrator to pass and mount them.
- The web core says "The password didn't match" where cross-app §4 says "That isn't the master password."; the screen shows the core's words as written. The words change in the core, not here.
- `desktop-locked-wash-narrow` lives in `states/shell.tsx` (foundation), so it carries no `built`.

## 6. Proposed decisions

| Id | Decision | Status |
|---|---|---|
| D840 | **The lock screen follows Mobile / 13 and Desktop / 10:** "Lockdown is locked." on plain ground in the focus frame; the reason and the person's name in the overline, never a vault's name (Q15; keeps D107's "nothing readable", drops its wash as D764 did). Plain version: the lock screen says Lockdown is locked and why, and shows nothing of your vault. | Proposed; **flagged for Sean** (Q15) |
| D841 | **The idle overline shows the timer that fired** (`IDLE 10:00`, from the core's setting), never a running time the core doesn't report. Plain version: it says which timer locked it. | Proposed |
| D842 | **Unlock is always the working primary; Touch ID and Windows Hello appear only where the build offers them, disabled with the core's reason until a command opens the vault with them.** Changes the drawn primary "Unlock with Touch ID" until quick unlock is built (doc 39 R4). Plain version: you unlock with your password until fingerprint unlock is really there. **Amended by D1032 (Lockdown 32):** no Touch ID or Windows Hello button at all until quick unlock is built. | Proposed; **flagged for Sean** |
| D843 | **On a phone, Unlock is the primary float button** (`unlocked`, named "Unlock") in the bottom row, as Mobile / 13 draws the fingerprint button. Plain version: on a phone the unlock button sits at the bottom right. | Proposed; **flagged for Sean** |
| D844 | **Caps Lock is a compact info Banner under the field; a wrong password is the core's sentence under the field; an empty Unlock asks for the password in words** instead of a disabled button. Plain version: the lock screen tells you what is wrong where you type. | Proposed |
| D845 | **The web sign-in is the focus frame with `WEB VAULT // host`, "Sign in." and three line tabs;** a security key or emailed link is its own step with its own title and Cancel. D176 kept: no account is made here. Plain version: signing in to the web vault looks like the app's lock screen. | Proposed; **flagged for Sean** (W1) |
| D846 | **A refused web sign-in, too many tries included, is one compact critical Banner over Sign in** in the server's words. Plain version: why sign-in failed shows right above the button. | Proposed |
| D847 | **The web unlock says the tab's login in the overline and keeps Sign out of this tab as a text button;** the session-ended sentence replaces the usual one when another device ended it (the shell passes it). Plain version: a locked tab says whose it is and how to leave. | Proposed |
| D848 | **Opened link values are SecretFields (reveal by a press, copy by reference to the page's clipboard copy), text values Field Rows, files a FileChip with Save;** the copy toast goes, since the field shows its own countdown. Plain version: a shared secret looks and copies like one in your vault. | Proposed; **flagged for Sean** (W2) |
| D849 | **A gone link or key request is a critical Empty State in the server's words with Try again;** key pages say "Done." and "Added." as titles. ADD KEY replaces "Add the key". Plain version: a link that no longer works says so plainly. | Proposed |
