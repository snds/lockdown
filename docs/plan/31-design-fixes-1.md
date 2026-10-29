# 31: Design fixes 1, the defects

_Slice: `design/fixes-1`, built 2026-10-29. The first build from the design review's fix list ([`docs/design/2026-10-29-review.md`](../design/2026-10-29-review.md), "Fix list, in build order", item 1): rows R1, R2, R3, R5, R6, R19 and R20. Nothing here changes a wire shape, a gate or what leaves the core. Proposed decisions D380–D389 (§9)._

## For future agent

- **TL;DR:** Approve and deny keep their words on a phone (R1): Lockdown's dialogs use a local `LabelledActions` / `LabelledAction` in place of unicron's `DialogActions` / `DialogAction`, and a small unlayered stylesheet gives the two unicron components that draw their own actions (the crown-jewel gate and the approval sheet) their words back. Below `sm` every action row stacks full width with the primary last. The item's Security box draws once (R2: two panels shared a React key). The extension's lookalike stop takes the menu's larger height and wraps inside it, so its sentence is never cut (R3). Five pieces of copy now follow their decisions (R5). The TV sign-in's camera error stays on the Camera tab (R6). Status has one primary button (R19). The first run's "Learn more" looks like a link and the Create door's button says "Create my vault" (R20).
- **Read with:** `apps/desktop/src/components/labelled-actions.tsx` and its test, `apps/desktop/src/styles/dialog-actions.css`, `extension/src/pages/frame.ts` (`heightFor`, `message`), the review's rows.
- **Do not:** import `DialogAction` / `DialogActions` from `@unicron/ui` in Lockdown's own dialogs (use `LabelledActions`); give two siblings in the detail pane the same key; give the in-page menu a third height (D159); put a camera, picture or paste error anywhere but on its own tab.

## 1. What was built

| Row | Piece | What it does | Where |
|---|---|---|---|
| R1 | `LabelledActions`, `LabelledAction` | A dialog's actions as text buttons at every width. The same footer (`DialogFooter`) and the same button looks (`confirm` → `default`, `cancel` → `outline`, `destructive` → `ghost-destructive`) as unicron's; from `sm` up one row aligned right, as before, with a destructive action leading only when it is written first (unicron's `mr-auto` on every destructive action pulled the whole row left in Lockdown's dialogs, where Delete or Revoke is the yes and comes last); below `sm` a full-width stack in source order, so the primary, written last, sits last. Each button keeps `data-kind`, so unicron's dialog still hides its corner × on phones when a Cancel is in the row. It takes `kind`, `type`, `form`, `disabled`, `onClick` and children, by name: no class overrides and no spread props on unicron components, so the design-system ratchet stays at 4 | `apps/desktop/src/components/labelled-actions.tsx`; used by all 36 files that had `DialogActions` |
| R1 | `dialog-actions.css` | For unicron's own `DialogActions` row (a `dialog-footer`, or the approval sheet's `approval-sheet-actions`): hides the phone icon, takes the words out of `sr-only`, and below `sm` stacks the row full width at the button's own height (the approval sheet reversed, so Approve is last). Unlayered, so it beats Tailwind's utilities whatever the order. It reaches only a button that is a direct child of that row; `LabelledActions` nests its buttons a level deeper | `apps/desktop/src/styles/dialog-actions.css`, imported by the desktop's, the web vault's and the capture harness's sheets |
| R2 | Detail pane keys | `ItemSecurity` is keyed `security:<id>:<version>` and `ItemProtection` `protection:<id>:<version>`; they shared `<id>:<version>`, and React's keyed diff drew the Security box again on each redraw | `apps/desktop/src/features/vault/detail-pane.tsx` |
| R3 | The menu's message height | `heightFor(list)` holds the two heights per mode (menu 64 / 232, prompt 150 / 232, passkey sheet 150 / 260). `message()` asks for the larger one for every stop, and for a plain message only when its words overflow the smaller one. `.msg` wraps long words (`overflow-wrap: anywhere`), so a long host breaks instead of running out of the frame | `extension/src/pages/frame.ts`, `extension/static/ui.css` |
| R5 | Copy | See §2 | five files |
| R6 | TV sign-in errors | The camera's error is its own state, drawn inside the Camera tab. Changing tabs clears both it and the general error, so a picture's or a paste's error stays with the tab it came from, and the camera tries again when its tab comes back | `apps/desktop/src/features/scan/scan-dialog.tsx` |
| R19 | Status | "Sync now" is `outline`; "Check now" is the screen's one filled button | `apps/desktop/src/features/status/status-view.tsx` |
| R20 | First run | "Learn more" is the link button at `size="inline"`, on the sentence's baseline in its size; the sentence's paragraph underlines it and draws it in the foreground ink, since unicron's link variant underlines only on hover in the same ink as the sentence. The Create door's button says "Create my vault"; the tab still says "Create" | `apps/desktop/src/features/unlock/{first-run,unlock-screen}.tsx` |

### Where R1's collapse came from

From unicron, not Lockdown. `@unicron/ui` 0.12.0's `DialogAction` renders an icon (`sm:hidden`) and its words in a `max-sm:sr-only` span, and sizes itself `max-sm:size-10 max-sm:rounded-full max-sm:p-0`; its `DialogActions` keeps one row (`flex-row`) at every width. Neither has a prop or a variant that keeps the words (`icon` only changes which icon). Lockdown had no collapse of its own. So the fix is local, as the brief asks: the smallest replacement for Lockdown's own dialogs, and a stylesheet for the two unicron components that draw `DialogActions` inside themselves, where no prop reaches (`CrownJewelGate`, every gated action's password step, and `ApprovalSheet`). Both go when unicron ships the request in §6.

## 2. The copy (R5)

| Where | Was | Now | Follows |
|---|---|---|---|
| Item editor, the crown-jewel switch | "… This can't be undone later." | "… To undo it later, choose Stop keeping as a crown jewel in the item's menu; it asks for your account password." | D148 |
| Item menu, "Keep as a crown jewel?" | ends "… sealed again under the crown-jewel key." | adds "You can stop later from this menu, with your account password." | D148 |
| Codes, empty | "Add one by pasting its otpauth:// link. Scanning a QR code comes with the camera later." | "Add one from the set-up link the site gives you (otpauth:// or steam://). The camera here is for signing in a TV, not for adding codes." | D310, D311 |
| Recovery, trusted contacts | a "later" badge; "Comes with the social-recovery slice." | no badge; "Someone you trust can ask for your vaults if something happens to you, and you get a waiting period to stop it. Set it up in Sharing → Emergency access." and an **Open Emergency access** button, which opens Sharing | emergency access (doc 18 §6) |
| Signed in elsewhere, the dialog's title | "Signing in on someone else's screen?" | "Signing in on a TV?" (the button already said it) | D237 |
| Kit test, skipping | a checkbox, "I understand an untested kit can fail on the day I need it.", then "Skip for now" | one button, **I understand an untested kit can fail**; the description adds "Skip it, and Lockdown asks again in a week." | D251 |

The kit test's button sends `kit_test_skip(understood: true)`; the core's refusal without it is unchanged. The brief suggested "add a code from its set-up link or key": Lockdown takes a link only today (`otpauth://` or `steam://`, `lockdown_codes::parse_uri`), so the words say a link.

## 3. How to run and test

```sh
cd apps/desktop
pnpm vitest run src/components src/features/vault/detail-pane.test.tsx \
  src/features/{scan,status,unlock,codes,recovery,depth} src/features/vault/{item-editor,vault-depth-detail}.test.tsx
pnpm ds:ratchet                  # stays at 4

cd ../../extension               # R3 runs in the real menu
bash build-wasm.sh debug && pnpm build
LOCKDOWN_TEST_BINS=<a folder with lockdown-native-host and browser-endpoint> pnpm test

cd ../design/capture             # the screens, re-taken
pnpm build && pnpm capture --only desktop-approval- --only desktop-first-run-doors --widths 1280,390
```

Vitest's `css` option now processes `dialog-actions.css` and nothing else, so the R1 test can read the sheet as text (`?raw`); every other stylesheet stays off in tests, as before.

## 4. Tests added

Desktop (Vitest): 11 new, 4 changed. Extension (Playwright): 1 extended.

| Test | What it covers |
|---|---|
| `labelled-actions.test.tsx` (6) | At 390 px, each action found by its accessible name shows the same words, with no `sr-only` part, no round icon look and no icon, and the primary is the row's last child in a stacking row: browser pairing (The codes differ, Pair), passkey approval (Decline, Approve), revoking a device (Cancel, Revoke), the window's plain confirm (Cancel, Confirm). With the sheet applied over the rules unicron's classes resolve to below `sm`: the crown-jewel gate's Cancel and Confirm and the approval sheet's Approve, Deny and This wasn't me have their words drawn (`position: static`, no clip) and their icons hidden |
| `detail-pane.test.tsx`, one Security box | The detail redrawn three times, as the lock poll does: one Security box and one Protection box (four Security boxes before the fix) |
| `scan.test.tsx`, the camera's error | The Camera tab says jsdom has no camera; Picture and Paste show no alert; back on Camera it says it again |
| `status-view.test.tsx`, one primary | "Check now" is the only filled button; "Sync now" is outline |
| `codes-view.test.tsx`, empty | The new words, and not the old |
| `recovery-view.test.tsx` | The row points to Sharing → Emergency access, has no "later", and its button opens Sharing |
| `first-run.test.tsx` (changed) | "Learn more" sits in the sentence, underlined; the tab says "Create" and no button does; the door's button is "Create my vault" |
| `depth.test.tsx` (changed) | The kit test skips with one press on "I understand an untested kit can fail", which sends `true`; there is no "Skip for now" |
| `item-editor.test.tsx` (changed) | The crown-jewel switch says how to undo it and not that it can't be |
| `vault-depth-detail.test.tsx` | The log dialog's heading is "Signing in on a TV?" |
| `extension.spec.ts`, the lookalike stop (extended) | The whole sentence, matched exactly, and its box inside the frame with nothing cut (no overflow, bottom within the frame). It fails on the 64 px frame |

The capture harness's first-run states press "Create my vault" now; their notes for the kit test and the sign-in log say the new words.

## 5. Gaps

- **jsdom lays nothing out.** The R1 test checks the markup and the classes that make the stack; the stack itself, at 390 px, is checked by the re-taken images. The test of the two unicron components writes unicron's below-`sm` `sr-only` rule out by hand; if unicron renames that class, the test still checks the sheet's selectors against the components' real markup.
- **The approval sheet and the gate depend on unicron's markup**: `data-slot="dialog-footer"`, `data-slot="approval-sheet-actions"`, `data-kind` on each action, a `span` around its words and an `svg[data-slot=icon]`. A unicron update that changes these breaks the sheet's reach; the test above fails when it does.
- **A stop longer than the larger height** (a very long host and several lookalike names) would still be cut at 232 px; long words now wrap, and the sentences the core writes fit with room to spare. The frame can't grow past its second height without breaking D159.
- **Codes take a link only.** A set-up key typed by hand has no field (the core's `add_code` reads a URI). Adding one is a feature, not a copy fix.
- **Trusted contacts that vouch for you** (social recovery) aren't built; the row now names what is (emergency access), under the same title.
- **The web vault** has no Recovery screen, so the new button exists on the desktop only (`onEmergency` is optional).

## 6. unicron requests

| Missing in unicron | Lockdown's workaround | Row |
|---|---|---|
| `DialogAction` that keeps its words below `sm` (a prop or a variant), with `DialogActions` stacking full width, primary last; `ApprovalSheet` and `CrownJewelGate` using it | `LabelledActions` / `LabelledAction`; `styles/dialog-actions.css` for the two composites | R1 |
| A `link` Button that reads as a link at rest (underlined, or in a link ink), for "Learn more" in a sentence | the sentence underlines its link button | R20 |

These join brief §11.1. **unicron host-contract gaps** beyond these: none. No class or style overrides were added on unicron components; the ratchet stays at its baseline of 4.

## 6.1 Security notes

- No new path releases a secret, and no gate changed. The crown-jewel gate and the approval sheet only change how their buttons are drawn; arming, the match, the timeout and `origin` are unicron's, untouched.
- Approve and deny now carry words on every width, which is what brief §6 rule 5 asks of a security yes.
- The kit test's skip still needs the explicit acknowledgement the core checks (`understood: true`); it now rides on the button that says it.
- The lookalike stop is the same message, drawn whole; the frame keeps D159's two heights, so a page still learns only "something to offer" or "a message".

## 7. Counts

- Desktop Vitest: 202 before, 213 after (40 files, all passing).
- Web Vitest: 45 before and after. Web end to end: 10 of 10 pass (the views it draws come from the desktop).
- Extension Playwright: 54 pass and 1 skip (Firefox not installed) before and after; the lookalike test is extended, not added.
- Rust: no Rust changed. `cargo fmt --check`, `cargo clippy --workspace --all-targets`, `cargo clippy -p lockdown-web --target wasm32-wasip1`, `cargo clippy -p lockdown-desktop --all-targets --features shell` and `cargo deny check` pass. `cargo test --workspace` and `cargo test -p lockdown-desktop --features shell` were left to CI with the coordinator's agreement, as the shared build disk was nearly full.
- Design-system ratchet: 4 class overrides, unchanged.

## 8. Re-taken captures

Re-taken with `pnpm capture --only <prefix> --widths 1280,390`, light and dark, into `/home/user/design-capture/2026-10-29/` (98 images, plus `desktop-devices-revoke` once more after the destructive-action alignment change): `desktop-approval-{browser-pairing,passkey,sheet-seam}`, `desktop-devices-revoke`, `desktop-item-{setup-guide,login-full,jewel-on,away-dialog}`, `extension-menu-lookalike` (natural size), `desktop-codes-{empty,scan,scan-paste}`, `desktop-recovery` and its four dialogs, `desktop-reminder-kit-test`, `desktop-status` and its five states, `desktop-first-run-doors`. The review build (`pnpm review:build`) is 16 files, 6.7 MB.

## 9. Proposed decisions

For the coordinator to record. **Flagged** rows are the ones Sean should see.

| ID | Decision | Status |
|---|---|---|
| D380 | **Dialog actions keep their words at every width.** Below `sm` a dialog's actions stack full width as text buttons, primary last, never as × and ✓ circles. Lockdown's dialogs use `LabelledActions` / `LabelledAction` instead of unicron's `DialogActions` / `DialogAction` until unicron keeps the words itself. Plain version: on a phone, Approve says "Approve" and Deny says "Deny". | Proposed; **flagged for Sean** (every dialog's phone layout) |
| D381 | **The crown-jewel gate and the approval sheet get their words back from a stylesheet** (`styles/dialog-actions.css`), unlayered, reaching only unicron's own action rows, until unicron ships the request in doc 31 §6; then it is deleted. Plain version: the two dialogs Lockdown can't rebuild are fixed with a few CSS rules. | Accepted by default under D38 (structure) |
| D382 | **A stop in the in-page menu takes its larger height** (232 px) and wraps inside it; a plain message takes it only when its words don't fit the small one. Still two heights (D159). Plain version: the lookalike warning is never cut off. | Proposed; **flagged for Sean** (the menu is taller for a stop) |
| D383 | **The kit test's skip is one button that says "I understand an untested kit can fail"** (D251's words), in place of a checkbox and "Skip for now"; the description says it asks again in a week. Plain version: skipping the test is one press on a button that says what you accept. | Proposed; **flagged for Sean** (words; one control fewer) |
| D384 | **Copy that followed a decision now says it:** the crown-jewel switch and "Keep as a crown jewel?" say how to undo it (D148); the Codes empty state says a set-up link adds a code and the camera is for TVs; Recovery's trusted contacts point to Sharing → Emergency access with a button; the sign-in log's dialog says "Signing in on a TV?" (D237). Plain version: no screen says something is impossible or coming later when it isn't. | Proposed; **flagged for Sean** (words) |
| D385 | **In "Sign in a TV or console", each tab's error stays on its tab**; changing tabs clears it, and the camera tries again on its own tab. Plain version: the camera's complaint doesn't follow you to Paste. | Accepted by default under D38 |
| D386 | **Status has one primary button, "Check now"**; "Sync now" is a secondary (outline) button. Plain version: one obvious button per screen. | Accepted by default under D38 (design review R19) |
| D387 | **First run: "Learn more" is drawn as a link** (underlined, in the foreground ink, on the sentence's line) and **the Create door's button says "Create my vault"**. Plain version: the link looks like one, and the button says what it makes. | Proposed; **flagged for Sean** (words) |
| D388 | **Siblings in the item detail have distinct keys** (`security:…`, `protection:…`); a test redraws the detail and counts one of each box. Plain version: the Security box no longer appears twice. | Accepted by default under D38 |
| D389 | **Vitest processes one stylesheet, `dialog-actions.css`**, so its test can read it; every other sheet stays off in tests. Plain version: a test checks the CSS that keeps the buttons' words. | Accepted by default under D38 (tests) |
