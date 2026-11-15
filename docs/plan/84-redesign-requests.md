# 84: requests wait behind the needs-you; reviews on unicron's ApprovalSheet

_Slice: `edges-1-requests` of the 2026-11 redesign, lane `edges` (branch `wt/lockdown-edges`), built 2026-11-15. Brief A1 (requests) and X3 rows 1 to 3 (needs-you) of the undrawn-areas brief, the drawn frames Mobile / 11 and Desktop / 09 (Sign-in request, review), and the cross-app decisions §1, §2, §10 and §12. Decisions D920 to D929 (D920 to D924 used here)._

## For future agent

- **TL;DR:** a passkey request, a pairing request and (when the core sends one) a device request each wait as a needs-you row and never open by themselves. REVIEW opens unicron's `ApprovalSheet`; closing it decides nothing. The two polls (1.5 s), the answers and the gate are unchanged.
- **Do:**
  - Use `features/approvals/requests.ts` for any new request kind: `usePolledRequests` (first-sight expiry, a run-out kept until the shell's toast), `useReview` (one review at a time; its row steps aside while it is open and for a moment after), `useApproveWords` (the yes named by the `approve` route), `isCancelled` (backing out of the gate is not an error).
  - Register the row with `<RequestSource entry={…} />` (`request-source.tsx`), priority 1 to 3.
  - Pass `dismiss="keep"` to every `ApprovalSheet`: closing never answers.
- **Do not:**
  - Open a review from a poll, a timer or an effect. Only REVIEW opens it.
  - Send an answer when the sheet reports `timeout`: the core runs the request out itself.
  - Add ACCOUNT, DEVICE, NEAR, TIME, "ago" or "locks this login": no request carries them (W5).
- **Read with:** brief A1 and X3 (workspace `07-projects/29-unicron/impl-2026-11/app-lockdown-undrawn.md`), doc 68 (the needs-you slot and the gate), doc 19 §5 (passkey approval), doc 16 §3 (pairing).

## 1. The needs-you rows (brief X3 rows 1 to 3)

| Row | Source | Title | Line | Solid | Text |
|---|---|---|---|---|---|
| 1 | `PasskeyApprovalDialog` (`browser_passkey_pending`) | SIGN-IN REQUEST // 01:57 LEFT | the request's `text` | REVIEW | DECLINE, at once |
| 2 | `PairingDialog` (`browser_pending`) | PAIRING REQUEST // 01:58 LEFT | "{name} wants to pair." | REVIEW | DECLINE, at once |
| 3 | `DeviceRequests` (no core source yet) | DEVICE REQUEST (with the time left when the request has one) | the request's own words, else its what and who | REVIEW | none |

- Every request that waits has its own row: two at once show one block with `// +1` (doc 68).
- A request keeps the time it runs out from the moment it was first seen, so the countdown never jumps with a poll. When the core stops listing it within 5 s of that time, it stays until its time is up, so the shell says "Sign-in request ran out" (or "Pairing request ran out") once. One that leaves earlier was answered elsewhere and leaves quietly.
- While a review is open, its own row steps aside (and for 300 ms after it closes), so the compact line of doc 68 never sits on the review of the same request. A second request still shows as the compact line over the review; its REVIEW opens its own review on top.
- DECLINE on the block answers at once (taking access away never asks for the gate, X3 rule 3). If that answer fails, its review opens with the core's sentence.
- The components keep their exports and props: `<PairingDialog onNotice/>` and `<PasskeyApprovalDialog onNotice/>`, mounted by `App.tsx` inside the needs-you host.

## 2. The reviews (brief A1)

All three are unicron's `ApprovalSheet` (#51): a sheet on a phone, a review 560 dialog wider, with `dismiss="keep"`.

| Request | Overline | Title | Rows | Matching | Yes | No | Sentence |
|---|---|---|---|---|---|---|---|
| Passkey | SIGN-IN REQUEST // 01:57 LEFT | the request's `text` | BROWSER | none | APPROVE WITH TOUCH ID (the `approve` route is the platform check, with `fingerprint-recognition`), else APPROVE and then the gate | DECLINE | "Approve only if you just started this sign-in in your browser." |
| Pairing | PAIRING REQUEST // 01:58 LEFT | "Pair {name}?" | BROWSER, KIND (Chromium or Firefox) | compare: the code in `DigitGroups`, "Approve only if the Lockdown extension in your browser shows this code." | THEY MATCH (gate) | CODES DIFFER | "A paired browser can ask to fill the sign-ins, codes and cards on this computer. It never sees your vault." |
| Device | DEVICE REQUEST // MM:SS LEFT | the request's words, else the kind's question | the request's facts (where, when) | enter the digits the other side shows | APPROVE (the host runs the gate) | DENY, and THIS WASN'T ME | where it came from |

States:

| State | What shows |
|---|---|
| Just opened | the yes disabled for the sheet's arming moment (`--c-approval-arm-delay`, 0.5 s) |
| Time running | the overline counts down; the 30-segment rule empties, critical in the last 20 s |
| Ran out while open | "This request ran out.", EXPIRED in the overline, the rule empty, CLOSE alone. No answer is sent |
| The gate asks | the gate opens over the review; CANCEL returns to it, nothing approved, no error |
| Working | the yes keeps its label with its spinner; the no is disabled; the sentence reads "Waiting for the answer" |
| The answer failed | the core's sentence in the sheet's error line; the review stays |
| Closed (close button or Escape) | nothing decided; the row comes back until the request runs out |
| Answered | a toast: "Approved: Sign in to github.com as kestrel04" · "Declined: …" · "Paired Chrome on Linux" |

Left out for lack of data (W5): ACCOUNT, DEVICE, NEAR, TIME, "00:40 ago", and "Deny, and Lockdown locks this login until you change the password" (the lock-on-deny does not exist; app spec question 6).

## 3. Tests

- `features/passkeys/passkey-approval-dialog.test.tsx` (12): the row; REVIEW opens the review; Approve through the gate with its toast; DECLINE at once; **never opens by itself** (three polls); **closing with the close button or Escape sends no answer** and the row comes back; arming and working; **the yes follows the gate's route** (Touch ID on the platform route, plain Approve on the password route, the no stays the secondary); backing out of the gate returns to the review with no error; a failed answer shows the core's sentence; ran out while open (CLOSE only, no answer); **a request arriving under an open dialog shows the compact line, and REVIEW opens the review over it**; the phone strip.
- `features/browsers/pairing-dialog.test.tsx` (5): the row and review (BROWSER, KIND, the code, both sentences); THEY MATCH pairs with "Paired …"; CODES DIFFER declines; DECLINE at once; never opens by itself and closing declines nothing; two requests, one block with `+1`.
- `features/approvals/approval-sheet.test.tsx` (7): the seam's three tests unchanged; new: row 3 with REVIEW only and no review by itself; closing sends no deny and no report; **a pushed device code has no approve control from the needs-you either**; the row's words.
- `components/labelled-actions.test.tsx` (R1, foundation file): the pairing and passkey cases now open from REVIEW and moved into the unicron group (`expectComputedWords`), because the actions are unicron's sheet's, not `LabelledActions`. Same assertions on the words at 390 px.
- Capture states (ids kept, `built`): `desktop-approval-passkey` (drawn `sign-in-request--review`), `desktop-approval-browser-pairing` (`sign-in-request--pairing`), `desktop-approval-sheet-seam` (`sign-in-request--device`). New: `desktop-approval-passkey-strip` (`needs-you--sign-in-strip`), `-block` (`needs-you--sign-in-block`), `-gate` (`sign-in-request--gate`), `-ran-out` (`sign-in-request--ran-out`, on the real clock), `desktop-approval-under-editor` (not sent).

**Not proven:** focus order and screen-reader speech in a real browser (unicron owns both, §4); the extension's end-to-end passkey and pairing specs drive the Rust endpoint, not this UI; the Tauri window.

## 4. Gaps

### unicron host-contract gaps

1. **`ApprovalSheet` always draws THIS WASN'T ME and the TRUST row.** A1 gives the passkey and pairing reviews only the no and the yes, and no trust tier. Ask: `report={false}` (or no `onReport`) to leave the report action out, and `tier={null}` to leave the row out. Until then the report action declines, and its one-second notice says the host will sign that session out, which is not true for these two. Screens: the passkey and pairing reviews.
2. **Focus and tab order.** A1 wants focus on the title and the tab order no, then yes. The sheet focuses the no (or the first digit) and its DOM order is yes, no, report. Ask: `initialFocus="title"` and the no before the yes in the DOM. Screens: every review.
3. **The time said at 30 s.** The sheet says "10 seconds left"; A1 and X3 say it at arrival and at 30 s. Ask: an `announceAt` knob. Screens: every review.
4. **An error as a compact critical `Banner`.** The sheet shows "Error: …" as a caption line. Ask: the error as `Banner size="compact" tone="critical"` in the body. Screen: the answer failed.
5. **Ran out replaces the body.** The sheet keeps the facts and shows the note; A1 replaces the body by "This request ran out.". Small; ask only if Sean wants it.
6. **The yes wraps** to two lines at 560 when three actions share the footer (APPROVE WITH TOUCH ID). Goes away with gap 1.
7. **A disabled no without a reason line.** "Waiting for the answer" is the sheet's sentence, not a `disabledReason` under the no (#37). Ask: `disabledReason` on the no while busy.

### Lockdown gaps

- No core source for row 3: `DeviceRequests` is wired and tested, mounted nowhere (`approvals()` does not exist).
- The compact line of doc 68 stands at the window's top edge over a tall layer (the editor), covering its header. Doc 68's placement (foundation).
- The foundation's `shell-needs-you-under-dialog` state still adds its sign-in request with `addNeedsYou`; it can now use `browser_passkey_pending` (`desktop-approval-under-editor` does).

## 5. Proposed decisions

| Id | Decision | Status |
|---|---|---|
| D920 | **Requests wait behind REVIEW and never open by themselves** (Q3; changes D156 and D208): a passkey or pairing request is a needs-you row with REVIEW and DECLINE; a device request has REVIEW only. Plain version: a request waits in the yellow block until you choose to look at it. | Proposed; **flagged for Sean** (Q3) |
| D921 | **Closing a review decides nothing** (Q3; changes D156 and D208, where closing declined): the request keeps waiting until it runs out; DECLINE and CODES DIFFER still say no at once. Plain version: closing the window no longer answers for you. | Proposed; **flagged for Sean** (Q3) |
| D922 | **The reviews are unicron's `ApprovalSheet`** with only what the request carries: the browser (and its kind), the code to compare, the time left; the yes named by the gate's route (APPROVE WITH TOUCH ID only on the platform check). No account, device, place or time row and no lock-on-deny (W5, app spec question 6). Plain version: the review shows exactly what the request knows. | Proposed; **flagged for Sean** (A1, 6.2 row D156/D208) |
| D923 | **Pairing words:** PAIRING REQUEST, "{name} wants to pair.", "Pair {name}?", THEY MATCH, CODES DIFFER, toast "Paired {name}" (D156's "Pair" and "The codes differ" become A1's words, no article). Plain version: the pairing buttons say what you checked. | Proposed; **flagged for Sean** |
| D924 | **Answer toasts name the request without a full stop:** "Approved: {text}", "Declined: {text}" (decisions §12). A request that runs out says "Sign-in request ran out" / "Pairing request ran out". | Proposed |
