# 90: Sharing home, People and a vault's members as list + detail

_Slice: `edges-7-sharing` of the 2026-11 redesign, lane `edges` (branch `wt/lockdown-edges`), built 2026-11-15. Briefs S1 (Sharing home), S2 (People) and S3 (a vault's members, households and shared items) of the undrawn-areas brief, with X3 row 6 (someone reset their account) and X5's Question table; no drawn frame. Decisions D980 to D989. Answers R12 for Sharing, Q1's Sharing side and keeps D634's words._

## For future agent

- **TL;DR:** Sharing is one Page with four line tabs, PEOPLE · VAULTS · LINKS · EMERGENCY, each with its count. PEOPLE and VAULTS are a list and a detail: side by side at the expanded width, the detail pushed over the page narrower. People: your fingerprint in `DigitGroups` with SHOW CODE (`QrCode`), your code to paste with COPY, Look up, KEY CHECKS FOR YOU, PEOPLE NN with COMPARED or NOT COMPARED; a person's detail has COMPARE FINGERPRINTS as its one primary. Vaults: VAULTS NN (a household says so), NEW HOUSEHOLD, ITEMS SHARED ONE BY ONE NN; a vault's detail is its members, ADD SOMEONE and HISTORY. Someone who reset their account is needs-you row 6 on Sharing only. LINKS and EMERGENCY show today's sections until edges-8.
- **Do:**
  - Keep `SharingView`'s and `SharingLocalOnly`'s exports and props: App.tsx and the web vault mount them unchanged.
  - Put a tab's list and detail in `SharingPanes` (`features/sharing/sharing-panes.tsx`): it tells the frame when a detail is pushed (`setDetailOpen`, `setBack`), so the phone's top bar and the medium status line carry Back.
  - Say why a control is off with `disabledReason` and `OFFLINE_REASON` (`people-section.tsx`), never by hiding it.
  - Register a needs-you with `useNeedsYouSource` (`key-resets.tsx`); never draw a yellow shape in the screen.
- **Do not:**
  - Put a count in a tab's accessible name: the tabs are found-3's places and are found by their names ("People"); the count is `aria-hidden`.
  - Use red on a routine removal (Stop sharing, Remove, Change): they are text buttons; red is the Question's (R12).
  - Show controls while the core holds new members back (`held`, D572).
- **Read with:** briefs S1 to S3, X3 and X5 (workspace `07-projects/29-unicron/impl-2026-11/app-lockdown-undrawn.md`), doc 18 (sharing), doc 22 §9 (the phrase check), doc 35 §2 (held vaults), doc 55 (key resets), doc 64 (places), doc 68 (needs-you).

## 1. Sharing home (brief S1)

| Part | What shows |
|---|---|
| Page head | `SHARING // 03 PEOPLE // 01 SHARED VAULT`, "Sharing", the shell's Check again |
| Needs-you | the block under the head (X3 row 6), or the strip on a phone; at medium width with a detail pushed, at the top of the detail |
| Tabs (line) | PEOPLE NN · VAULTS NN · LINKS NN · EMERGENCY NN. LINKS counts `sends()` + `requests()` (a read the Links tab makes too); the counts are seen, not read in the name |
| Offline (`online` false) | a compact info Banner "You're offline." / "Sharing needs the server." under the tabs; lists stay; Look up, NEW HOUSEHOLD, STOP SHARING, ADD, CHANGE and REMOVE are off with "Needs the server. You're offline." |
| Split view (`paused`) | the critical Banner with the core's sentence and SEE STATUS (Settings → Status); nothing when the shell already shows its split-view Banner on Sharing (one Banner for one thing) |
| Loading | three skeleton rows |
| Failed read | nothing loaded: a critical Empty State with TRY AGAIN; loaded before: the lists stay with a critical Banner and TRY AGAIN |
| A failed yes (let back in, stop sharing) | a compact critical Banner "Not done." with the core's words, dismissable |
| On this computer only | the whole page is one Empty State: SHARING, "Sharing needs a server.", "You and the people you share with need a server you can all reach.", SET UP SERVER (Settings → Server) |
| Web vault | PEOPLE and LINKS only (web-app.tsx draws its tabs; People is this slice's `PeopleTab`) |

- A pull, and Check again, sync; when the rows come back, Sharing reads the overview again.
- Phone: Sharing is pushed from the Switch vault sheet; its top bar has Back. A chosen row pushes its detail; the page head and tabs step aside, and Back returns to the list.

## 2. People (brief S2)

| List pane | |
|---|---|
| YOUR FINGERPRINT | `DigitGroups size="sm"` in a stacked Field Row; SHOW CODE opens a question dialog (a sheet on a phone) with `QrCode` of your contact code and the fingerprint again |
| YOUR CODE | the code to paste (`lockdown:contact:v1:…`), COPY on the desktop (the clipboard path that clears, as links do); "Copied" for 2 s, no toast. The web has no COPY (its sharing client has no clipboard path) |
| Look someone up | Text Field "Look someone up by their sign-in", LOOK UP (secondary); the core's error under the field. A found person opens and the compare Form follows |
| KEY CHECKS FOR YOU NN | a row per check someone started, TYPE THEIR PHRASE |
| PEOPLE NN | a compact List Row per person: login, `NN DEVICES`, COMPARED or NOT COMPARED (read "Fingerprint compared") |
| Nobody yet | Empty State `PEOPLE // 00`, "Nobody yet.", "Look someone up to share with them or name them an emergency contact." |

| Detail | |
|---|---|
| Head | `PERSON // NOT COMPARED`, the login (h2) |
| Waiting on you | when this person reset their account: a compact info Banner with the hint "Say yes if you know they did…" (the question itself is the needs-you) |
| Rows | FINGERPRINT (`DigitGroups`, Badge COMPARED or NOT COMPARED), DEVICES |
| Actions | COMPARE FINGERPRINTS (the one primary, while not compared), CHECK WITH A PHRASE (secondary), FORGET (critical text → ConfirmDialog with today's words) |
| DETAILS (shut) | log entry, account id, quantum-safe keys yes or no |
| Phone | COMPARE as an extended float button at the end of the bottom row; CHECK WITH A PHRASE and FORGET in the top bar's ⋯ |

- **Compare fingerprints** is a Form (640; a sheet on a phone): "Ask them to read out their fingerprint, in person or on a call you trust.", "What they read out" with "Spaces don't matter. The code under their QR can be pasted here too.", SCAN THEIR QR CODE as text (edges-5's camera view, a square raised panel), NOT NOW, THEY MATCH (an extended float button on a phone). A mismatch is a critical line under the field in the core's words (`role="alert"`).
- **Check with a phrase** is a question dialog: KEY CHECK // login, "Tell {login} this phrase", "Say it in person or on a call you trust, never in a message. It works once.", the nine digits in `DigitGroups size="lg"`, then one line that follows the check (the core's words: waiting, matched, or why it failed). Closing a waiting check calls it off. The other side: "Check keys with {login}", Text Field "Their phrase", "You get one try. If it's wrong, they start a new check.", CHECK.
- **Key resets** (X3 row 6, D634): one needs-you per reset, on Sharing only: WAITING ON YOU, the core's question, LET THEM BACK IN (through the `Approve` gate) and NOT NOW (hides it until Sharing opens again). A second reset is an ordinary row under WAITING ON YOU in the People list. Offline nothing is registered: the yes needs the server.

## 3. Vaults and a vault's members (brief S3)

| List pane | |
|---|---|
| VAULTS NN | a compact List Row per vault: name, `Only you // 24 items`, `Yours // shared with 3 people // household`, `alex@…'s // 3 people`; OWNER or the right as a word |
| NEW HOUSEHOLD | the header's text action, while you organize none: a name prompt with D194's hint and CREATE HOUSEHOLD |
| ITEMS SHARED ONE BY ONE NN | sent: `With {login} // view // since 2026.10.02`, STOP SHARING (text); received: `From {login} // view`, OPEN (text) |

| Detail (found-3's members place, `members:{vault}`) | |
|---|---|
| Head | `VAULT // SHARED // 3 PEOPLE` (or `VAULT // ONLY YOU`), the vault, the caption |
| Notices | `blocked`: compact info Banner "Can't be shared." with the core's words; `held`: compact info Banner with the core's words, and the controls below are hidden (D572, D573) |
| MEMBERS NN | login, Badges YOU, OWNER, NOT COMPARED; the right as a word; a manager sees CHANGE and REMOVE as text (behind the row's ⋯ on a phone) |
| CHANGE | a question dialog of four `RadioRow`s with D187's words; a pick applies it |
| REMOVE | ConfirmDialog: "Remove {login}?", ""Family" moves to a new key now and everyone else gets it. What they already saw, they may have kept.", REMOVE AND CHANGE KEY (critical). No gate (D196) |
| ADD SOMEONE (managers) | Who (people pinned here, "(not compared)" after the unverified), They can (View by default), ADD (secondary; the gate). Nobody pinned: "Look someone up under People first." and OPEN PEOPLE |
| Only you | "Only you." and ADD SOMEONE |
| HISTORY NN (shut) | `LogRow` per signed change, row dates; the summary is the last change's date |
| A viewer | members only, nothing to press |

- Choosing a vault goes to its members place (`go(membersPlace(id))`), so the breadcrumb says `VAULTS // SHARING // FAMILY`; Back goes to `sharing-vaults`.
- Households are vaults (`kind: "family"`), so the separate Household section is gone; its create is NEW HOUSEHOLD and its members are the vault's detail.

## 4. Tests and what is not proven

- `features/sharing/sharing-view.test.tsx` (27): the fingerprint in groups (as a screen reader hears it) and your code; four tabs with counts, People first, "Nothing open."; SHOW CODE's QR; the split-view Banner with SEE STATUS and Look up off; a reset as the one needs-you (row 6), never a dialog, NOT NOW, asked again, yes through the gate; two resets: one block, the other under WAITING ON YOU; offline: the Banner, lists kept, Look up off with its reason, no needs-you, Links' button off; a vault's ADD, CHANGE and REMOVE off with the reason; look up then compare; the detail's one primary; a refused compare said under the field (`role="alert"`, `aria-invalid`); FORGET asks with CANCEL focused; a manager's CHANGE (RadioRows with D187's words) and REMOVE (text, then the Question); a viewer has no controls; a held vault hides its controls; a crown-jewel vault offers no ADD; a vault of your own: Only you and ADD through the gate with View; the links and emergency tests through their tabs.
- `features/depth/key-check.test.tsx` (3): the nine digits in groups and the match; closing a waiting check calls it off; KEY CHECKS FOR YOU with one try (nine digits checked first).
- `apps/web/e2e/finish.spec.ts`'s contacts check through `steps/sharing.ts` (`yourCode` reads "Your code to paste"); `shared-quiet.spec.ts` is untouched by this slice.
- Not proven: a real camera scan of a contact QR; the gate's platform prompt on LET THEM BACK IN; the phone's ⋯ on a member row under a real touch; a screen reader on the tab counts (hidden by design).

## 5. Gaps

### unicron host-contract gaps

1. **No list + detail inside a Page column.** App.tsx mounts Sharing in one `PageColumn`; `AppFrameList`/`AppFrameDetail` are frame columns. `SharingPanes` draws a two-column grid at the expanded width (the list at `--c-app-frame-list`, the split gap, a rule) and pushes the detail narrower through `setDetailOpen`/`setBack`. Unicron could offer `AppFramePanes` (a list + detail inside a column). Screens: `sharing--people`, `sharing--members`.
2. **`TabsTrigger` has no count slot.** The count is an `aria-hidden` span after the label. Screens: every Sharing tab row.
3. **No detail head below the page's h1.** `PageHead` is always the page's `h1`; the detail's head is a plain overline + `h2` (`paneHeading`). Screens: `sharing--people` (detail), `sharing--members`.
4. **`DigitGroups` has no character between groups** (doc 85 gap 1): tests read the spelled form. Screens: `sharing--people`, `sharing--phrase`.
5. **`TextField` error is not an alert**: the compare mismatch draws its own `role="alert"` line under an `invalid` field. Screen: `sharing--compare`.
6. **A disabled `DropdownMenuItem` has no reason line**: offline on a phone, CHANGE and REMOVE behind ⋯ are off with the page's offline Banner as the only reason. Screen: `sharing--members` (phone, offline).

### Left out

- **"No account" ("Sharing needs an account.")**: App.tsx shows `SharingView` with no account and the screen cannot tell it from offline; the core's overview fails or reads offline. The local-only state is built.
- **CREATE HOUSEHOLD through the gate**: `createHousehold` takes no gate input; adding a member to it does go through the gate. A core change.
- **The members' history as "the core's words" with the time first**: built; the time is a row date, not "ago".
- **LINKS and EMERGENCY as list + detail**: edges-8.

## 6. Proposed decisions

| # | Decision | Flag |
|---|---|---|
| D980 | Sharing is one Page of four line tabs, People · Vaults · Links · Emergency, each list + detail; the web has People and Links (brief S1). | **Flagged for Sean** (visible: one page of eight sections becomes four tabs) |
| D981 | Routine removals (Stop sharing, Remove, Change, Forget in a list) are text buttons; red is kept for the Question's yes (R12). | **Flagged for Sean** |
| D982 | Someone who reset their account is needs-you row 6 on Sharing only, with D634's question, LET THEM BACK IN through the gate and NOT NOW; never a dialog; nothing registered while offline. Changes doc 55's row in place. | **Flagged for Sean** |
| D983 | Offline (`online` false) shows "You're offline. Sharing needs the server." and every control that needs the server stays visible, off, with "Needs the server. You're offline." (#37). Replaces "Sharing needs an account on a sync server…". | **Flagged for Sean** |
| D984 | Your fingerprint is drawn in `DigitGroups`; SHOW CODE adds its QR (#53); your code to paste stays on the screen with COPY on the desktop (cleared like a link). | **Flagged for Sean** (Q1's Sharing side: the code stays visible for the web's paste path) |
| D985 | A person is compared in a Form (640, a sheet on a phone) whose one primary is THEY MATCH (an extended float button on a phone); it no longer shows the expected digits, so the check is what they read, not a match by eye. | **Flagged for Sean** |
| D986 | A vault's members are a detail pane (found-3's members place), not a dialog; CHANGE is four RadioRows with D187's words; REMOVE asks "Remove {login}?" with REMOVE AND CHANGE KEY and no gate (D196). | **Flagged for Sean** |
| D987 | Households are listed with the vaults (`// HOUSEHOLD`); NEW HOUSEHOLD is the Vaults header's text action with D194's hint; the separate Household section goes. | **Flagged for Sean** |
| D988 | While the core holds new members back (`held`), a vault's controls are hidden and its notice is an info Banner in the core's words (D572, D573 kept). | not visible beyond today |
| D989 | A split view's critical Banner shows once on Sharing: the shell's when it has one, else this page's with the core's sentence and SEE STATUS (X2). | **Flagged for Sean** |
