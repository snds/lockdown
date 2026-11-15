# 87: The toolbar popup on unicron 1.0

_Slice: `edges-4-popup` of the 2026-11 redesign, lane `edges` (branch `wt/lockdown-edges`), built 2026-11-15. Brief B1 (popup) of the undrawn-areas brief; inventory rows 123 to 127 (§2.12); §3.7 question 13 (the extension is designed, not tokens only). No drawn frame: the extension has no Figma screen. Decisions D950 to D959. Answers design review R16 again, in the new look, and keeps D166 (copy on a look-alike page asks first)._

## For future agent

- **TL;DR:** `extension/src/popup/popup.tsx` is rebuilt on unicron 1.0 parts only (the vendored `1.0.0-pre.2`, which found-1 already gave the extension): `AppLockup`, `TruncatedText`, `SectionHeader`, `ListRow size="compact"`, `Banner`, `SearchField`, `CodeDisplay layout="compact"`, `DigitGroups`, `ToggleRow`, `NativeSelect`, `DropdownMenuItem`, `Button`, `Badge`, `Spinner`. Every message to the background script is the same message with the same fields. Layout wrappers are plain elements; no `className`, `style` or spread on a unicron part (the popup's ratchet stays 0).
- **Do:**
  - Keep one action per row, at the row's trailing edge (`ListRow action`). A second thing a row can do goes in its ⋯ menu (`ListRow menu`), as Change password does.
  - Keep reading text at 13 px or more. The only smaller text is the overline (mono capitals): a row's mono line, the section titles, the host in the top line, badges and button labels. `extension.spec.ts` proves it on the search's rows.
  - Keep the status line the popup's one `role="status"`: the specs find it by role.
- **Do not:**
  - Put a count in `SectionHeader`'s `count` here: it is the 12 px meta role, under the floor. Write it into the title (`For this page 02`).
  - Read the page (`site`) before the popup can act: it asks the matcher of an app that may not be there.
  - Copy a shown code with anything but the app's `copy` message: the app reads the clipboard's seconds.
- **Read with:** brief B1 (workspace `07-projects/29-unicron/impl-2026-11/app-lockdown-undrawn.md`), doc 26 §1 and §9 (quick access, the popup's build), doc 23 §5 and §7 (fill fixes, Hide my email, the change-password link), doc 37 §2 (R16, Open Lockdown), doc 86 (the in-page frames).

## 1. The frame

- `main`: 360 px wide, as tall as its content up to 600 px (then it scrolls), `bg-surface-ground`, 12 px in (`p-3`), 12 px between blocks. `popup.html` keeps `data-theme="lockdown"`; `main.tsx` keeps the `.dark` class following `prefers-color-scheme`, so the browser's scheme picks graphite or stone. `popup.css`'s body is the ground surface too.
- **Top line:** `AppLockup name="Lockdown" showName={false}` on the left (the placeholder L mark until Lockdown has a real one); on the right the page's host in the overline role, cut in the middle by `TruncatedText cut="middle"` (the whole host in its tooltip and for a screen reader); a hairline under it (`rule-b`). The host shows only once the popup can act (it comes from the `site` message).
- **One status line** at the foot, `type-caption`, `role="status"`, polite: the last thing that happened, with the seconds from the copy's `clears_in_secs`. It is centred under a centred column.

## 2. The states

| State | What it draws | Its one primary |
|---|---|---|
| Connecting | A `Spinner` and "Connecting to Lockdown…" in the centre; an h1 for screen readers only | none |
| Not reachable | h1 "Lockdown isn't reachable.", the link's `message`, `OPEN LOCKDOWN`, `TRY AGAIN` (text) | Open Lockdown |
| Not paired | h1 "Pair this browser.", the sentence (or the link's message), `PAIR WITH LOCKDOWN`, `OPEN LOCKDOWN` (text) | Pair with Lockdown |
| Pairing | h1 "Check the code.", the code in `DigitGroups size="lg" label="Pairing code"`, the sentence; the status line says "Waiting for your approval in Lockdown…" until the app answers | none |
| Locked | h1 "Lockdown is locked.", "Unlock it in the app to fill and search.", `OPEN LOCKDOWN` | Open Lockdown |
| Can act | Look-alike stop, the page's logins, Hide my email, Search (below) | the first `FILL`, when there is a login |
| Fix this page | The second view (below) | Save fix |

A working button keeps its label and shows its spinner (`Button loading`): Pair, Open Lockdown, New alias, Save fix.

**When it can act,** top to bottom:

1. **Look-alike page (D166):** `Banner size="compact"` (critical, `role="alert"`): "Stop: {host} isn't {site}. It only looks like a site you have a login for. Lockdown won't fill here." No `FILL` is drawn on such a page.
2. **The page's logins:** `SectionHeader inset` "For this page 02" with `FIX THIS PAGE` (text) as its action. Then one compact `ListRow` per login, in a plain list named "Logins for {host}": the title; the mono line (the username, or "Protected" for a crown jewel, D113); a `Badge` "From quick access" after the title on the login the app just asked to fill; a trailing `FILL` (the first row's is the popup's primary, the others secondary; each named "Fill {title}"); and the row's ⋯ menu holding "Change password".
   The section names the page, not its host: the host is in the top line, and `SectionHeader`'s heading does not shrink, so a long host pushed `FIX THIS PAGE` out of the 336 px column (seen with `secure.bluefin-bank.example`).
3. **Hide my email** (a web page with an alias service set up): `SectionHeader inset`, then one compact row per service with `NEW ALIAS` (secondary, named "New alias from {service}").
4. **Search:** `SearchField` "Search Lockdown", focused on open; Escape clears it (the field's own rule). Results are compact rows in a plain list named "Results": the title, a `Badge` "Close match" when the core forgave a typo, the mono line (`samrivera // secure.bluefin-bank.example`, or `card // protected`), and one trailing text action: `COPY PASSWORD` (the label follows the item's `copy.label`), or `SHOW CODE`. A shown code appears under its row's words as `CodeDisplay layout="compact"` for its seconds (at most 30) and the action becomes `COPY CODE`. On a look-alike page the first press explains in the status line and the action becomes `COPY ANYWAY` (D166).

**Fix this page** is a second view inside the popup, in place of the old disclosure: a Back button (icon, named "Back"), the h1 "Fix this page" (focus moves to it), one sentence, `ToggleRow` "Never fill on {host}", the list's community note when there is one, then one compact row per field the page has: its name (a label for the select), the mono line (`password // now password`), and a `NativeSelect` of what it is under the words; `SAVE FIX` (primary). With no fields: "This page has no fields Lockdown can name." Back or Escape returns to the main view and puts focus on `FIX THIS PAGE`. The select sits under the words, not at the row's end: at the end it left a field's name three lines tall and its mono line cut. It is the native select because a Base UI list cannot leave the popup's window, which the browser sizes to the content.

**Keys:** Down in the search field moves to the first result's action; Up and Down move between the actions of a list's rows; Enter runs the focused action. Each state has one h1 (the can-act view's is for screen readers only). Section headers are h2. Lists are plain lists of their rows.

## 3. What did not change

- Every message and its fields: `status`, `pair`, `open_app`, `site`, `fill_tab`, `change_link`, `fix_fields`, `fix_set`, `alias_options`, `alias_fill`, `search`, `copy`, `code_show`. The background script, the protocol and the in-page frames are untouched.
- The words of every status sentence, the 1 s polling while pairing or locked, the 20 s wait for an app the host started, the 120 ms search debounce and the late-answer guard, the 30 s shown code, D166's second press, D113's titles only.
- **The shown code's copy.** `CodeDisplay` draws the code and its own copy control; both it and the row's `COPY CODE` send the app's `copy` message, which copies the code valid now and answers with the clipboard's seconds. `CodeDisplay` takes its countdown's seconds before the copy (`clipboardTimer`), so the popup lets its `onCopy` reject after the app's copy: the component draws no countdown of its own (it would show its default 30, a number the app did not say), and the status line carries the app's seconds.

## 4. States (design/capture)

`states/extension.tsx`: the fourteen popup states keep their ids and gain `built: { area: "extension", screen: "popup-<state>", kind: "undrawn" }`. Steps follow the new names: results waits for a `Fill …` button; pairing for "Check the code."; locked for Open Lockdown; code waits for `COPY CODE`; fix clicks the `FIX THIS PAGE` button; hide-email waits for `New alias from …`. The popup sits on the ground surface with a hairline standing in for the browser's window edge. `states/extension-scenarios.ts`: the results scenario's page has a second login (a crown jewel, "Bluefin Bank (business)"), so the first `FILL` is primary and the second secondary.

## 5. Tests and what is not proven

- `extension.spec.ts` (the popup's tests, same claims, the controls' new names): the pairing code is read from `DigitGroups`' groups; the page's section is "For this page" with the host in the top line; the shown code is the `CodeDisplay` group "Code for this account: …" and Copy code is the row's action; the row-anatomy test reads `ListRow`'s slots (title, mono line, action beside them and inside the row) and now also proves the mono line is the overline (mono capitals), the one role allowed under 13 px; locked is a heading and a sentence.
- `a11y.spec.ts`: the popup test now runs axe (light and dark) on the page's logins with the top line, Fix this page (and that focus moves to its h1 and back to its button), the search's rows, and the locked state; it checks one h1 and that rows are list items.
- `bridges.spec.ts` (edges lane, outside this slice's listed files, following the controls): Fix this page is a button; "Never fill on …" is a switch (`ToggleRow`); the never-fill test goes Back before pressing Fill; Change password is opened from the row's ⋯ menu.
- `open-app.spec.ts`: unchanged; its names (Try again, Open Lockdown, the status line) are the same.
- The capture: 14 states × 2 schemes, strict, no console error, no failed step. axe: one serious violation in `extension-popup-code` (both schemes), `label-content-name-mismatch` on `CodeDisplay`'s own copy control (§7).
- A probe (not committed) drove the capture build's popup over the fake background through the specs' claims: the pairing groups, one h1, the search focused, Fill and Change password and their status lines, Down into the results, the 13 px floor on every word, the row geometry, Fix this page (switch, select, save, focus there and back), the code group and Copy code, the stop with no Fill, Open Lockdown while locked, New alias. All passed.
- The reproducible zip: two clean builds packed with `SOURCE_DATE_EPOCH=1700000000` give the same sha256 (chromium `b6c9037a…409f`, firefox `244d4801…d5d9`, 30 files each).
- **Not proven here:** the specs that start the desktop endpoint (extension, a11y, bridges, passkeys, menu-pairing, firefox, open-app) did not run: this session's shared Rust target has no `examples/browser-endpoint`, and lanes never run cargo. `pnpm ds:ratchet` cannot run (snds/dsa answers 403); the popup's count is 0 by reading. A screen reader on the new popup.

## 6. Gaps

1. **The host in the top line only once the popup can act.** In the connecting, no-app, unpaired, pairing and locked states the popup has not read the page (§ For future agent), so the top line is the mark alone.
2. **Usernames read in capitals.** The mono line is the overline role so it can be smaller than 13 px; unicron has no mono line at 13 px or more in a row (the meta role is 12 px).
3. **Badges after the title, not at the row's end.** `ListRow`'s `badge` sits in the trailing words beside `FILL`; in 336 px it left the title two lines and the username cut, so the popup passes the badge as a mark after the title.

## 7. Unicron host-contract gaps

1. **`CodeDisplay`'s copy control is named "Copy code" while it shows the digits** (axe `label-content-name-mismatch`, serious, WCAG 2.5.3). Screen: `extension-popup-code`; every desktop screen with a `CodeDisplay` shares it.
2. **`CodeDisplay` takes the clipboard's seconds before the copy** (`clipboardTimer`, captured when the press starts); a host whose copy answers with the seconds cannot pass them. Ask: let `onCopy` resolve with `{ seconds }`. Screen: `extension-popup-code`.
3. **`SectionHeader`'s heading does not shrink or cut** (`shrink-0`), so a long title pushes its action out of a narrow column. Screen: `extension-popup-results` (with the host in the title).
4. **`SectionHeader`'s count is the 12 px meta role,** under a 13 px floor. Screen: `extension-popup-results`.
5. **No row control slot for a select:** `ListRow`'s `action` is sized for a small button. Screen: `extension-popup-fix`.
6. **No centred state column** (`EmptyState` is left-aligned with an overline): the five states where the popup cannot act are a plain centred layout. Screens: `extension-popup-connecting`, `-no-app`, `-unpaired`, `-pairing`, `-locked`.

## 8. Proposed decisions (D950 to D959)

| # | Decision | Answers |
|---|---|---|
| D950 | The popup's frame: 360 wide, up to 600 tall then scrolling, the ground surface, 12 px in; a top line with the mark alone and the page's host in mono capitals cut in the middle, over a hairline | B1 frame |
| D951 | Where the popup cannot act it draws one centred column: an h1 that says the state, one sentence, one primary and at most one text button | B1, R16 |
| D952 | The pairing code is `DigitGroups` at the large size; the status line says the popup is waiting for the app's yes | B1 pairing |
| D953 | The page's logins are compact rows: title, the username (or Protected) on the mono line, a trailing FILL that is primary on the first row only, and Change password in the row's ⋯ menu | B1 2, R16 |
| D954 | The logins' section reads "For this page" with its count in the overline; the host is in the top line | B1 2 (departs: the brief's `FOR GITHUB.COM 02`) |
| D955 | The From quick access and Close match badges sit after the row's title | B1 2 and 4 |
| D956 | A row's mono line is the overline role (capitals), so the popup's only text under 13 px stays the overline | R16, B1 frame |
| D957 | Results take one text action; a shown code is a compact `CodeDisplay` under the row's words for its seconds, copied through the app, its clear said in the status line; Copy anyway after the first press on a look-alike page | B1 4, D166 |
| D958 | Fix this page is a second view: Back, the title (focused), one sentence, a Never fill switch, a native select under each field's words, Save fix; Back or Escape returns focus to Fix this page | B1 Fix this page |
| D959 | Down moves from the search into the results and between row actions; the status line is the popup's one polite live region | B1 keys |

Each is a change a person sees (D38): flagged for Sean. None answers a §6.1 question: the extension has no Figma frame. §2.12 rows 123 to 127 are what this builds; §3.7 question 13 says it is designed.
