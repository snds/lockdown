# 91: Links, emergency access and sharing one item

_Slice: `edges-8-links-emergency` of the 2026-11 redesign, lane `edges` (branch `wt/lockdown-edges`), built 2026-11-15. Briefs S4 (Links), S5 (Emergency access) and I7 (Share an item) of the undrawn-areas brief, with X3 row 4 (someone asked for your vaults) and X5's Question table; no drawn frame. Decisions D990 to D999. Answers R12 for Links and Emergency; reaffirms D196 and D221._

## For future agent

- **TL;DR:** Sharing's LINKS and EMERGENCY tabs are now a list and a detail, like PEOPLE and VAULTS (doc 90). Links: SHARE TEXT and ASK FOR SECRET (secondary), one field that opens a share link or answers a request link, SHARE LINKS NN and REQUESTS NN as rows; what a link or an answer held opens in the detail. Emergency: the caption, ADD EMERGENCY CONTACT, YOUR EMERGENCY CONTACTS NN and PEOPLE WHO TRUST YOU NN; a grant's detail has where it stands, WAITING PERIOD, VAULTS, OPENS, its actions and WHAT HAPPENED. A request for your vaults is needs-you row 4, whose solid action on the grant's own screen is STOP IT (no gate). Sharing one item is two Forms (With someone, As a link); every link made is shown once with COPY LINK as the layer's one primary. Stopping an item's share asks first.
- **Do:**
  - Make every link through `LinkLayer` (`link-once.tsx`): the form, then the link once in the same layer, DONE secondary on a desktop and a text button at the phone top bar's start.
  - Put a sharing Form in `ShareLayer` (`share-layer.tsx`): a form dialog on a desktop, a full screen on a phone with the yes as an extended float button.
  - Keep `ItemShare`'s export and props (detail-pane imports it); `open` and `onOpenChange` were added for the item's ⋯.
  - Keep `EmergencyRequests` mounted in one place only (see §7).
- **Do not:**
  - Put STOP IT behind the gate or a Question: taking access away never asks for the gate (D196, D221).
  - Show a link again after its layer closes, or keep it anywhere but that layer's state.
  - Draw an emergency or item share in the web vault: its worker has no call for them (the W1 table).
- **Read with:** briefs S4, S5, I7, X3, X5 (workspace `07-projects/29-unicron/impl-2026-11/app-lockdown-undrawn.md`), doc 18 (sharing, §5 to §7), doc 17 (web vault), doc 90 (Sharing home), doc 68 (needs-you).

## 1. Links (brief S4)

| List pane | |
|---|---|
| Actions | SHARE TEXT, ASK FOR SECRET: secondary; off while offline with "Needs the server. You're offline." |
| SOMEONE SENT YOU A LINK | Text Field "Paste a share link or a request link", a `SecretField` (edit) "Its password, if it has one", OPEN (secondary). A `/r/` link opens the answer Form; anything else opens as a share link in the detail. A refusal (expired, used up, wrong password) is the field's error, in the core's words |
| SHARE LINKS NN | `ListRow`: `Created 2026.11.02`, `expires NOV 20 // opened 1 of 3 // password` (`expired` once past), DELETE as text, which asks |
| REQUESTS NN | `Asked NOV 01`, `waiting // expires NOV 08` or `answered NOV 03`; OPEN ANSWER and DELETE as text |
| None | Empty State `LINKS // 00`, "No links yet." |

| Form | Fields | Yes |
|---|---|---|
| Share text | What it is (they see this) · The text (6 lines) · Lasts · Opens at most · Password (optional), a `SecretField` | CREATE LINK, then the link once |
| Ask for a secret | What you're asking for (they see this) · Lasts | CREATE LINK, then the link once with "Send it to them. It works once, until it expires." |
| Answer a request | "They asked for: …" · A name for it (optional) · The secret (`SecretField`) · "Send it only if you know who sent you the link." | SEND IT |

- **The detail:** `OPENED // SHARED WITH YOU`, the title, one row per value: a `SecretField` for a secret (masked; shows and copies one value at a time on the shell's clipboard path), a Field Row for plain text, a `FileChip` with SAVE for a file. KEEP IN MY VAULT (secondary) leaves once pressed and "Kept in your vault." takes its place. Caption: "Kept here until you open another link or lock."
- **Deleting asks:** a ConfirmDialog, "Delete this link?" / "Delete this request?", with the consequence; DELETE in critical red only there.
- **Web vault:** the same tab over the worker (`apps/web/src/features/links-view.tsx`): text links, requests and answers. A share link pasted there opens on its own `/s/` page (same origin only), so there is no password field and no KEEP IN MY VAULT (the worker has no call for either).

## 2. The link shown once (briefs I7, S4)

- The link in a read-only mono line, cut in the middle: the start shrinks with an ellipsis and the last ten characters stay. The line's text is the whole link.
- COPY LINK is the layer's one primary, full width under it. After a copy: "Copied. The clipboard clears in {n} seconds.", `n` from the copy's `clears_in_secs`.
- Caption: "Send it in a way you trust. It isn't shown again after you close this."
- DONE closes the layer and drops the link: secondary in the desktop footer; on a phone a text button in the top bar's leading place, with nothing at its end.

## 3. Emergency access (brief S5)

| List pane | |
|---|---|
| Caption | "Someone you trust can ask for your vaults. You get a waiting period to stop it." |
| ADD EMERGENCY CONTACT | secondary; a Form: Who (compared people only), Waiting period (1, 2, 3, 7, 14, 30 days, with its hint), Which vaults (checkboxes), the caption, ASK THEM through the gate (extended "Ask them" on a phone). Nobody compared: an Empty State with OPEN PEOPLE |
| YOUR EMERGENCY CONTACTS NN, PEOPLE WHO TRUST YOU NN | `ListRow`: the other person; `wait 7 days // 02 vaults` (`wait 14 days` where no vault is sealed to you yet); the state's word: INVITED, ACCEPTED, ASKED FOR ACCESS, READY, OPEN, DECLINED, ENDED |
| None | Empty State `EMERGENCY // 00`, "No emergency contacts." |

| Whose | State | In the detail |
|---|---|---|
| Your contact | invited, accepted | END IT (critical text, asks) |
| Your contact | requested | the line "They open your vaults 2026.11.20 14:00 unless you stop it."; STOP IT is the needs-you's solid; END IT |
| Your contact | ready | "The wait ran out. They can open your vaults now."; STOP IT (secondary); END IT |
| Your contact | released | END IT; its Question says they already opened your vaults and you should change what matters |
| They trust you | invited | ACCEPT (the detail's primary; extended on a phone) and DECLINE (text) |
| They trust you | accepted | ASK FOR ACCESS (secondary; a Question says the wait starts and they are told); STEP DOWN (text, asks) |
| They trust you | requested | "It opens 2026.11.20 14:00."; STEP DOWN |
| They trust you | ready | OPEN THEIR VAULTS (primary); toast "alice@example.com's vaults are open to read, in the sidebar" |

Then WHAT HAPPENED NN, one `LogRow` per step. Offline, or while a split view pauses emergency steps, every action is off (offline says why; the page's critical Banner says why for a split).

## 4. Needs-you row 4 (brief X3)

- Registered by `EmergencyRequests` for each grant you named that is `requested`: `EMERGENCY ACCESS // OPENS NOV 20`, "alex@rivera.example asked for your vaults.", REVIEW, no text action, on every screen (`shows: "every"`).
- REVIEW switches to EMERGENCY and opens the grant (a REVIEW from another screen goes through `wantGrant` and `go("sharing-emergency")`).
- On that grant's own screen (`setShownGrant`) the solid action is STOP IT: it calls `emergencyStep(grant, "stop")` at once, with no gate and no Question, and a toast says "Stopped. alex@rivera.example can ask again later". Offline or paused, the action stays REVIEW.
- The phone shows it as the strip; its action is the same word.

## 5. Share one item (brief I7)

- **Entry:** `ItemShare` keeps its own SHARE button (secondary) with two rows, "With someone…" and "As a link…", until vault-1 opens it from the ⋯: with `open` and `onOpenChange` it draws no button and shows the two rows as a question-size layer. Only the person's own items, never a crown jewel or an item in the bin (`canShare`), never in the web vault.
- **With someone:** a Form, "Share Steam": With (one not compared reads `bob@example.com // not compared`), They can as two `RadioRow`s (View: "See, copy and fill"; Fill only: "Lockdown fills it in the browser. It is never shown or copied."), the caption, SHARE through the gate (extended "Share" on a phone). Toast: "Shared Steam with bob@example.com". Nobody pinned: an Empty State "Look someone up first." with OPEN PEOPLE.
- **As a link:** "A link to Steam": Lasts, Opens at most, Password (optional) in a `SecretField` with "Tell it to them another way.", the caption "Anyone with the link can open a copy of this item as it is now.", CREATE LINK through the gate, then the link once (§2).
- **Stopping asks:** `StopSharing`, a ConfirmDialog: "Stop sharing with bob@example.com?", "The item moves to a new key. What they already saw, they may have kept.", STOP SHARING. No gate (D196). Sharing → Vaults → ITEMS SHARED ONE BY ONE uses it now; the item's Details → SHARED WITH rows are vault-1's to mount.
- **Shared with you:** `AccessNote` says "Shared with you" and, for fill-only, "Fills in the browser only; never shown or copied." (`FILL_ONLY_WORDS`). The overline and FROM in Details are the detail pane's (vault lane).

## 6. Tests and what is not proven

- `features/sharing/item-share.test.tsx` (ItemShare mounted directly): with someone through the gate and its toast; `// not compared`; nobody pinned; as a link through the gate, the link once, COPY LINK the one primary and DONE secondary, the seconds from `clears_in_secs`; opened through `open`/`onOpenChange`; nothing offered on a shared item or a crown jewel; stopping asks with no gate; Shared with you and fill-only words.
- `features/sharing/sharing-view.test.tsx`: Links offline (each control off with its reason); a text link shown once and gone after DONE; a request link's copy seconds; rows and deleting asks; the Empty State; opening a share link (secrets masked until asked, KEEP IN MY VAULT then its caption); a refusal under the field; a request link in the same field opens the answer Form, whose secret bytes are wiped; needs-you row 4 with REVIEW, then STOP IT on the grant's screen at once, no Question, no gate; END IT asks with the released words; ACCEPT then OPEN THEIR VAULTS and its toast; nobody compared with OPEN PEOPLE; adding a contact through the gate; the steps offered by state.
- `apps/web/e2e/web-vault.spec.ts` (unchanged) through `steps/sharing.ts`: a share link with a password and a request link made in the web vault, opened and answered by a browser with no account, and the answer read back.
- **Not proven:** row 4 on screens other than Sharing (see §7); a real file in a link's detail; the phone's hold-to-show on an answer (the e2e holds it with the mouse); a screen reader.

## 7. Gaps

1. **Row 4 shows on Sharing only until the shell mounts `EmergencyWatch`.** App.tsx is frozen in the lanes. The integrator adds `<EmergencyWatch enabled={shares} onNotice={notice} />` beside `<PairingDialog/>`, and removes the `<EmergencyRequests …/>` line from `SharingView` in the same change (both register the same ids; two mounts would drop the block when Sharing closes).
2. **The item's ⋯ → Share… is vault-1's.** `detail-pane.tsx` still renders `ItemShare`'s own SHARE button. With `open` and `onOpenChange` it can move into the ⋯ menu.
3. **Details → SHARED WITH on the item** is the detail pane's; `StopSharing` is exported for it.
4. **The web vault has no offline state for Links:** its worker answers each call or refuses it, and the refusal shows under the field.
5. **A link's list row has no name** (wish W8): the list calls return no title.

### unicron host-contract gaps

1. **No full-screen form part.** A phone Form here is a full-height `Sheet` with `TopBar` and the yes in `SheetFooter`, as the editor does (doc 89). `AdaptiveDialog` has no leading-place slot for DONE. Screens: every Form in this slice.
2. **`SectionHeader` draws its children only when collapsible.** WHAT HAPPENED is a header and a list beside it. Screen: emergency.
3. **No middle-cut text.** The link's line is two spans (the start truncates, the end stays). Screen: link once.
4. **`ListRow` has one `action` slot.** OPEN ANSWER and DELETE share it in a span. Screen: links.
5. **A disabled `SecretField` (edit) has no `disabledReason`.** The password field is off while offline with no reason of its own; OPEN beside it says why. Screen: links offline.

## 8. Proposed decisions

| # | Decision | Status |
|---|---|---|
| D990 | Links is a list + detail: SHARE TEXT and ASK FOR SECRET secondary, one field that opens a share link or answers a request link, SHARE LINKS and REQUESTS as rows; what a link held opens in the detail (brief S4, R12). | **Flagged for Sean** (visible: "Open a share link" and "Answer a request link" become one field; two sections become one tab) |
| D991 | A link made here is shown once in the layer that made it: a mono line cut in the middle, COPY LINK the layer's one primary, full width, the copy's own seconds, DONE secondary (a leading text button on a phone). Changes the old "Copy the link" outline button (brief I7). | **Flagged for Sean** |
| D992 | Deleting a share link or a request asks first, with what it means; the routine DELETE in the list is a text button, red only in the Question (R12, X5). | **Flagged for Sean** (new Question) |
| D993 | Emergency access is a list + detail: grants as rows with `wait 7 days // 02 vaults` and the state's word; the detail has where it stands, WAITING PERIOD, VAULTS, OPENS, its actions by state and WHAT HAPPENED as Log Rows (brief S5). | **Flagged for Sean** |
| D994 | Someone you named asking for your vaults is needs-you row 4 on every screen: EMERGENCY ACCESS // OPENS {date}, REVIEW to the grant, and STOP IT on the grant's own screen, which acts at once. Replaces the critical "Emergency" badge line on the Emergency tab (X3, R12). | **Flagged for Sean** |
| D995 | STOP IT and DECLINE act at once; END IT, STEP DOWN and ASK FOR ACCESS ask a Question; nothing in emergency access but ASK THEM takes the gate. Reaffirms D196 and D221 (taking access away never asks for the gate). | Reaffirmed |
| D996 | The words of a grant move to the brief's: ACCEPT (was Agree), DECLINE (was Say no), END IT (was End this), OPEN THEIR VAULTS (was Open the vaults); STEP DOWN stays for a contact who accepted. | **Flagged for Sean** |
| D997 | Sharing one item is two Forms, full screens on a phone: With someone (With, They can as two RadioRows, SHARE through the gate) and As a link (Lasts, Opens at most, a password in a SecretField, CREATE LINK through the gate). `ItemShare` gains `open` and `onOpenChange` for the item's ⋯ (brief I7, Q19 for the worded phone buttons). | **Flagged for Sean** |
| D998 | Stopping an item's share asks first: "Stop sharing with bob@example.com?", the item moves to a new key, STOP SHARING, no gate (brief I7; D196 kept). | **Flagged for Sean** (new Question) |
| D999 | The web vault's Links tab is the desktop's Links list over the worker: text links, requests and answers; a pasted share link opens on its own page; no KEEP IN MY VAULT (brief S1's web row, W1). | **Flagged for Sean** |

R12 (Sharing split by what the person wants to do, removals quiet, red kept for the Question) is answered for Links and Emergency.
