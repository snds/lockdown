# 88: Codes as a page, Add a code, the clock notice, and signing in a TV

_Slice: `edges-5-codes` of the 2026-11 redesign, lane `edges` (branch `wt/lockdown-edges`), built 2026-11-15. Briefs C1 (Codes) and C2 (Sign in a TV or console) of the undrawn-areas brief; drawn frames Mobile / 08 Codes (`40:954`) and Desktop / 06 Codes (`41:2487`). Decisions D960 to D969. Answers Q4 of the app spec's open questions (the blue of Codes) and the decision-record rows on `--area-codes` (undrawn §2, brief 1.4)._

## For future agent

- **TL;DR:** Codes is a page. The head says `CODES // NN` (phone: `CODES // NN // REFRESH EVERY 30 S` and the count beside the title) with ADD CODE and SIGN IN TV; each code is unicron's `CodeDisplay` straight from the package, `row` on a phone and `tile` from the medium width up, with a trailing ⋯ (Open item · Show large type · Copy). The clock notice is a compact `Banner`. Add a code is a Form. Sign in a TV is a Form with three ways in, then a Review: `ApprovalSheet` for a device code (only the camera gets a yes), or a Review with the core's words.
- **Do:**
  - Keep `CodesView`'s export and props (`rows`, `onChanged`); `onOpenItem` is new and optional.
  - Keep `ClockSetting`'s export and props (none): Settings → Codes (screens-5) renders it as it is.
  - Keep `CameraView`'s props: Sharing → People (`people-section.tsx`) reads a person's QR with it.
  - Set `ApprovalSheet`'s `origin` from the tab: `scan` for the camera, `push` for a picture or a paste. The component then draws no approve control (D99, D310).
- **Do not:**
  - Draw a code with anything but `CodeDisplay`, or copy a code any way but `copy_code` (the ⋯'s Copy is `CodeDisplayMenuItem action="copy"`, the code's own copy).
  - Toast "Copied" or "Cleared": the component says both, and "Cleared" only when the shell says it cleared.
  - Put `--area-codes` (or `data-area="codes"`) on anything here: the live colour is the component's, on the running countdown only (D960).
- **Read with:** briefs C1 and C2 (workspace `07-projects/29-unicron/impl-2026-11/app-lockdown-undrawn.md`), doc 26 §3 (the TV sign-in and its core), doc 23 §8 (the clock check), doc 84 (`ApprovalSheet` in this app), doc 64 (the page head and places).

## 1. Codes (brief C1)

| Part | Desktop (medium and expanded) | Phone (compact) |
|---|---|---|
| Head | overline `CODES // 07` (lib/format `count`), title "Codes"; actions: SearchField 348 wide (from ten codes up), ADD CODE, SIGN IN TV (text buttons), then the shell's Check again | overline `CODES // 07 // REFRESH EVERY 30 S`, title with the count `07`; icon buttons "Sign in TV" (`screen`) and "Add code" (`add`), then Check again |
| Status line | `REFRESH EVERY 30 S` (`useStatusItem`, X2 row 3) | none (row 3 has no strip); the overline carries it |
| Clock notice | a compact `Banner` under the head: warning while codes are corrected, critical once too far off; the core's words; CLOCK SETTINGS | the same |
| Codes | `CodeDisplayList layout="grid"` of `CodeDisplay layout="tile"`: three columns at 1280 and up, two below | `CodeDisplayList layout="list"` of `layout="row"`, full-bleed (the list steps out of the frame's 16; each row keeps its own) |
| A code | the issuer in ui-emphasis, the account in meta, the code (five-character Steam codes as one group, eight digits in fours), the 30-cell rule in secondary-live and `NN S`, critical near the end; a counter code shows `COUNTER 212` and NEXT CODE instead | the same, as a row |
| Copy | a click anywhere on the tile or row, or the ⋯'s Copy; "Copied", then "Cleared" only on a real clear | a tap |
| ⋯ | Open item (only where the host passes `onOpenItem`), Show large type (unicron `LargeTypeView`, two minutes, capture-protected), Copy | the same, as a sheet |
| Search | issuers, accounts and titles; nothing found: `SEARCH // 00`, "No code for {q}." | none on the page: the shell's search button (brief C1) |
| Empty | `CODES // 00`, "No codes yet.", "Add one from the set-up link a site gives you.", ADD CODE | the same |
| Loading | three `CodeDisplay loading` tiles | three loading rows |
| A failed read | `EmptyState tone="critical"`, `CODES // 00`, "Codes didn't load.", the core's words, TRY AGAIN | the same |
| Pull to refresh | | the page column's `onRefresh` (the shell's sync); Check again is the plain path |

- The digits are read one character at a time (the component's name: "Code for GitHub, sean: 4 8 2, 9 1 7. 8 seconds left."); the countdown is not announced each second. The list is Lockdown's own `ul` inside `CodeDisplayList`, so the one live region sits beside it (review R7).
- `useCodeHost` (`features/codes/use-code-host.ts`) is the page's host for one code: the same contract as the vault's `ItemCodeDisplay` (copy by reference through `copy_code`, the shell's copy number as the reference, `clipboard_clear` with that number at the end, NEXT CODE through `advance_code`, Fix clock). It exists because `ItemCodeDisplay` (vault lane) had no `layout` or `menu` in this branch; see §5.

## 2. Add a code (brief C1)

An `AdaptiveDialog` Form (640; a sheet on a phone): overline `CODES // NEW`, "Add a code"; the set-up link in `SecretField mode="edit"` (masked, SHOW), "Paste the otpauth:// or steam:// link the site gave you."; KEEP IT (`NativeSelect`: "As its own item", or "Inside {login}" for each live login), "A code usually belongs to the login it protects."; TITLE only for its own item, "Left empty, the issuer and account are used."; CANCEL and ADD CODE (primary, the form's submit). A link the core refuses shows the core's sentence on the field. The value is dropped on close and after a save. No camera (R5, W11): the words say "paste".

## 3. Sign in a TV or console (brief C2)

1. **The Form** (`AdaptiveDialog`, 640; full sheet on a phone): `CODES // TV SIGN-IN`, "Sign in a TV or console", the sentence, `Tabs variant="line"`: CAMERA · PICTURE · PASTE.
   - Camera: `CameraView`, a `video` in a square raised panel with a hairline and one status line ("Starting the camera…", "Hold the code steady in the square."). The camera's own trouble is a compact Banner on this tab only (R6), with TRY AGAIN.
   - Picture: CHOOSE PICTURE, "Lockdown reads it, and tells you why it won't approve it."
   - Paste: TextField "The code's address" and CHECK IT.
   - No primary: a read moves on by itself.
2. **The Review**, replacing the Form:
   - A device code the core approves: unicron `ApprovalSheet` (`kind="device-code"`, review 560, a sheet on a phone), overline `CODES // TV SIGN-IN // 05:00 LEFT` (the activation's five minutes from the read, D311), title "A TV or console sign-in", SERVICE and PAGE (the full domain, emphasised), the sheet's TRUST row, then the TV's code in `DigitGroups size="sm"`, SIGN IN WITH as `RadioRow`s (each login the core listed, then "I'll sign in by hand"), the caption, DON'T SIGN IN and SIGN IN ON {HOST} (gate). The camera's read is `origin="scan"`; a picture's or a paste's is `push`, and then the sheet draws no yes, says "Device codes are approved only when you scan the screen in front of you.", and its one answer is SCAN AGAIN.
   - Anything else (refused, a passkey QR, not a sign-in, no code): a Review (`AdaptiveDialog size="review"`) titled by the kind, a critical Banner "Refused" with the core's reason (info for "not on this computer"), SERVICE, PAGE and TV'S CODE as Field Rows, and SCAN AGAIN.
3. **After the yes:** the real page opens and a toast says what happens next. A code the page doesn't take in its address stays: "{service}'s page is open in your browser. Type this code on the page:", the code in `DigitGroups size="lg"`, DONE.
- The window is capture-protected while a code shows. Closing before the yes cancels the activation in the core. The web vault uses the browser's camera (`getUserMedia`), the same view.

## 4. Settings → Codes (brief T6)

`ClockSetting`: a `SectionHeader` "One-time codes" and one `ToggleRow` "Correct for a wrong clock" with its sentence; an error is a compact Banner. Export and props unchanged.

## 5. Gaps

### unicron host-contract gaps

1. **`ApprovalSheet` context rows take strings only.** TV'S CODE should be a context row in `DigitGroups size="sm"` (brief C2); it sits in the note under the facts instead, with its own label. Screen: `codes--tv-review`.
2. **`ApprovalSheet` has no slot for a choice.** SIGN IN WITH sits in the note line, a `<p>`, so the `RadioGroup` renders as a `span` (`render={<span />}`) to stay valid HTML. Screen: `codes--tv-review`.
3. **`ApprovalSheet` always draws THIS WASN'T ME** (doc 84 gap 1). Here it cancels the activation and closes. Screen: `codes--tv-review`.
4. **`AppFrameInset` has no way out for full-bleed rows.** The page column wraps a page in the frame's 16 on a phone; Codes' rows are drawn full-bleed, so the list steps back with `-mx-4`. Screen: `codes--list` (phone).
5. **The page head on a phone puts its actions on a line under the title.** The brief draws SIGN IN TV as an icon on the title line. Screen: `codes--list` (phone).

### Lockdown gaps

- **Open item** needs the shell's `openItem`; `App.tsx` and `web-app.tsx` are frozen in the lanes, so the row shows only once the integrator passes `onOpenItem` (§ For the integrator in the hand-off).
- **`useCodeHost` repeats `ItemCodeDisplay`'s host.** Once the vault lane's `ItemCodeDisplay` gains `layout` and `menu` (vault-1), Codes can draw through it and `use-code-host.ts` can go.
- **The search field shows from ten codes up**, so a person with nine scans by eye; the phone has no field (the shell's Search finds codes).
- **Copy and NEXT CODE failures are still `toast.error`**, as on an item (X2 says errors are Banners; kept the same as `ItemCodeDisplay` so the two hosts say the same thing).

## 6. Tests and what is not proven

- `codes-view.test.tsx`: the empty state and Add a code by paste (no camera words); the head's count through lib/format with ADD CODE and SIGN IN TV; digits read one by one; search from ten codes and "No code for …"; no search under ten; copy through `copy_code` and never the webview's clipboard; a refused copy; the clear names the shell's copy number and "Cleared" shows only on a real clear; NEXT CODE; the ⋯ (Show large type, Copy as the code's own copy, Open item only with a host for it); the list's live region beside the list.
- `scan.test.tsx`: grey frames; the camera's error on its tab only; a paste's refusal in the core's words with no approve control; the camera's read names the domain and the TV's code in groups, takes the picked login, and shows the code to type; closing before the yes cancels; **a paste the core would approve still renders no approve control** (the sheet's origin); **a picture renders no approve control**.
- `clock.test.tsx`: nothing without a bridges client; the notice only when the clocks disagree, as a warning Banner, critical when too far off; nothing while they agree; Settings → Codes as one Toggle Row that saves.
- `apps/web/e2e/finish.spec.ts` (frozen) through `steps/codes.ts`: a TV's code read by the tab's camera opens the real page after the password; a paste is refused.
- Not proven: a real camera on a Mac or PC (the e2e uses Chromium's fake camera); WebKit's `getUserMedia`; the gate's platform check after the yes; a screen reader on the tiles.

## 7. Proposed decisions

| # | Decision | Status |
|---|---|---|
| D960 | **`--area-codes` retires to secondary-live, on the running countdown only** (brief 1.4; Q4 of the app spec). The Codes page sets no area colour; `CodeDisplay` draws the rule and seconds in secondary-live and critical near the end. Plain version: blue means "counting down now", not "this is Codes". | Proposed; **flagged for Sean** (a colour people see) |
| D961 | **Codes is a page:** `CODES // NN` and the title, rows on a phone and tiles from the medium width up, on `CodeDisplay` `row` and `tile` (C1; Mobile / 08, Desktop / 06). | Proposed; **flagged for Sean** |
| D962 | **ADD CODE and SIGN IN TV are title-line actions**; on a phone they are icon buttons under the title, and ADD CODE stays beside the create button so the paste form is one press away. **Amended by D1002 (Lockdown 2):** on a phone, ADD CODE leaves the title line; the create button's "One-time code" opens the paste form. SIGN IN TV stays. | Proposed; **flagged for Sean** (C1's phone draws the create button only) |
| D963 | **A code's ⋯ is Open item · Show large type · Copy** (LA5, C18); Show large type draws the code already on screen, for two minutes. **Amended by D1028 (Lockdown 28):** large type follows the live code and its countdown, still capture-protected, still closing at two minutes. | Proposed; **flagged for Sean** |
| D964 | **Search shows from ten codes up, on desktop and medium only** (C1); `SEARCH // 00`, "No code for {q}." | Proposed |
| D965 | **The clock notice is a compact Banner** (warning when corrected, critical when too far), and Settings → Codes is one Toggle Row. | Proposed; **flagged for Sean** |
| D966 | **Add a code is a Form with the link in a SecretField** (masked, SHOW), KEEP IT and TITLE; paste only (R5, W11). | Proposed; **flagged for Sean** |
| D967 | **The TV sign-in's yes is unicron's `ApprovalSheet`, and its `origin` comes from the tab** (`scan` for the camera, `push` for a picture or a paste), so no path but the camera can draw a yes (D99, D310). A picture or paste the core refused, or anything that is not a device code, is a Review with the core's words and SCAN AGAIN. | Proposed; **flagged for Sean** (C2) |
| D968 | **The Review counts the activation's five minutes from the read** (D311), in the sheet's overline and rule. | Proposed; **flagged for Sean** |
| D969 | **The camera view is a square raised panel with one status line** ("Hold the code steady in the square."); the camera's trouble is a Banner with TRY AGAIN on the Camera tab only (R6). | Proposed; **flagged for Sean** |
