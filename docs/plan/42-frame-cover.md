# 42: The stuck Save button, and frames that are really covered

_Slice: `slice/frame-cover`, built 2026-10-30. A bug fix in the browser extension. The save prompt's Save button sometimes stayed disabled through a 5 s wait in `extension/test/a11y.spec.ts` ("the save prompt has no serious or critical axe violation"). It failed on PR #33 and PR #34, and locally about 1 run in 30. Builds on D38 (who decides security-model details), D62 (wire shapes only grow), D67 (the visibility rule), D159 (the in-page menu and its arming), D206 (presence for passkeys), D446 (arming you can see) and the coordinator's calls under D375. Proposed decisions D500–D503 (§7)._

## For future agent

- **TL;DR:** The suspected cause was wrong. IntersectionObserver v2 (IO v2, the browser's own "is anything covering this frame" signal) never went stale. The welcome page really held **two** save prompts in the same place. The second one covered the first. The first one correctly stayed disabled, and the test found the first one. The two prompts came from a race in the content script: two requests for the prompt both passed the "no prompt yet" check before either one finished setting up. The menu had the same race. The fix is in `extension/src/content/index.ts`: only one prompt can be set up at a time, and a newer menu open makes an older one that is still setting up draw nothing. The frame's guard (`extension/src/pages/frame.ts`) is unchanged.
- **Read with:** `showPrompt`, `openMenu`, `menuTurn` and `promptPending` in `extension/src/content/index.ts`; `arm`, `deliberate` and the observer at the top of `extension/src/pages/frame.ts`; `makeHost` in `extension/src/content/host.ts`; `offerSave` and `top_ready` in `extension/src/background/index.ts`; the tests after "one frame at a time, and covers that come and go" at the end of `extension/test/extension.spec.ts`.
- **Do not:**
  - Add a timer, a re-observe or a "settled" message to the frame to "unstick" IO v2. A fresh observer reports the same thing as the old one (§2). Those additions would only loosen the guard.
  - Put an `await` between a frame's "is one already shown?" check and the moment the content script records the frame, unless a guard taken before the `await` covers that gap (`promptPending`, `menuTurn`).
  - Treat a frame IO v2 calls covered as a test problem. Look for something really on top of it first, including another Lockdown frame.

## 1. What happened

```text
  sign-up submit ──► background: offer_save ──► app (slow: native host, core)
                                    │
  page navigates to welcome.html    │
  content script loads ──► top_ready ──► "an offer is waiting" ──► showPrompt()  ─┐  both pass
                                    └──► show_prompt to the tab  ──► showPrompt()  ─┘  `!prompt`
                                                                           │
                       register() ── await ──► host 1 (prompt = 1)         │
                       register() ── await ──► host 2 (prompt = 2), on top of host 1
```

- Each prompt host is a `popover="manual"` element in the top layer at `top: 12px; right: 12px`. A later popover paints above an earlier one, so host 2 covers host 1 exactly.
- Frame 1's IO v2 reported `isVisible: false` and kept saying so, which was true. Its buttons stayed disabled with no arming cue, which is right for a covered frame.
- `lockdownFrame()` in the tests returns the first frame whose URL matches, and that was frame 1.
- **What a person would have seen:** two prompts, one exactly on top of the other. Only the top one works. After the person answers it, only host 2 is removed, because `prompt` points to it. Host 1 is no longer covered, so it arms. But its offer is gone, so its Save or Not now does nothing, and its × sends a close that no longer matches `prompt`. It stays on the page until the person leaves the page.
- **The menu had the same race.** Two opens in flight for one field (it is focused twice quickly, or a focus and the background's `open_menu` arrive together) both drew a menu. The older one was forgotten under the newer one. It could never arm and was never removed.

### 1.1 The evidence

The failing run, captured from inside the stuck frame on the welcome page:

```text
IO v2 reports: [{t: 67 ms, isVisible: false, isIntersecting: true, ratio: 1, 320×150}]   (one report, never another)
a fresh IO v2 observer made at 5.1 s: {isVisible: false}
frames on the page: welcome.html, frame.html#prompt:09d5…, frame.html#prompt:95a2…
top-level children: HEAD, BODY, DIV:popover-open, DIV:popover-open
```

Probes of the suspected cause, each on the fill menu, with the page acting on the host as it appears. In every case IO v2 sent a second report when the effect ended, and the choice armed:

| The page does this to the host | IO v2 reports | Arms? |
|---|---|---|
| Fades it in from `opacity: 0` over 300 ms | visible at once, or covered and then visible about 110 ms later | Yes |
| Slides it in from `translateY(12px) scale(.96)` | covered, then visible about 110 ms later | Yes |
| Plays a 300 ms Web Animations entrance | covered, then visible at about 300 ms | Yes |
| Holds it at `opacity: .99`, clears that at 600 ms | covered, then visible at about 610 ms | Yes |
| Covers it with its own popover, removes it at 600 ms | covered, then visible at about 610 ms | Yes |
| Covers it, then hides the cover with `visibility: hidden` | covered, then visible at about 610 ms | Yes |
| Covers it, then makes the cover `opacity: 0` and leaves it there | covered, and a fresh observer agrees | No, and rightly so (D502) |

## 2. The fix

| Piece | Change |
|---|---|
| `showPrompt` | Takes `promptPending` before it awaits the registration, and clears it after. A second ask that arrives meanwhile returns at once. One prompt answers both asks. |
| `openMenu` | Takes the next `menuTurn` before it awaits the registration. When the registration comes back, an open whose turn is no longer the latest one forgets its registration and draws nothing. This sits beside the existing checks for the person moving on or typing. |
| `pages/frame.ts` | Unchanged. |

**Why the guard is just as strict:** nothing in the frame changed. A frame is still armed only when IO v2 says it is visible (or, on Firefox, when the host's own hit test says so) and 400 ms have passed since then. A click still counts only when it is trusted, the frame is not covered and the settle time has passed. The arming cue is unchanged. The fix removes the one thing that legitimately covered Lockdown's frame on a well-behaved page, which was Lockdown's own second copy.

The passkey sheet (`content/passkeys.ts`, `showSheet`) has no `await` between its check and its draw, so it can't race this way.

## 3. How it is tested

New tests at the end of `extension/test/extension.spec.ts` (Chromium, with the real native host and desktop endpoint):

| Test | What it shows | Before the fix |
|---|---|---|
| two requests for the save prompt at once draw one prompt, and it leaves no copy behind | The service worker sends `show_prompt` twice together. The page sees one host, and none is left behind. | **Fails every time:** one host stays on the page. |
| a field focused twice in a row gets one menu, and Escape leaves none | `focus(); blur(); focus()` in one task. One host, and it arms. | **Fails every time:** the frame found is the covered copy, and it never arms. |
| after a sign-up the welcome page holds one save prompt, and its Save arms | The real path the a11y test takes. Exactly one host and one prompt frame after 1 s. | Fails about 1 run in 30 (the race). |
| a menu whose host fades in / slides in / is held see-through / is covered, then uncovered in place as it appears arms once it settles (4 tests) | A covered report from the moment it appears recovers once the effect ends. Nothing about the host's place or size changes. | Pass: IO v2 was never stale. |
| red team: a save prompt the page keeps covered never arms, and a click through the cover saves nothing | A see-through decoy that lets clicks through. Save stays disabled with no arming cue for 3 s. A real mouse click on Save saves nothing. | Pass (the guard held). |
| a save prompt uncovered in place re-arms after the full 400 ms, then saves | The page removes its decoy at 1.5 s and notes the time. The frame arms 400 ms or more after that, and the save goes through. | Pass. |

The existing red-team test ("a page that covers Lockdown's menu cannot click through it") and the arming-cue tests (D446) still pass unchanged.

Run them:

```sh
cd extension
pnpm build                                   # needs pkg/ from `pnpm run build:wasm`
pnpm run build:native                        # the native host and the endpoint fixture
LOCKDOWN_TEST_BINS=<dir with private copies> pnpm test
npx playwright test test/a11y.spec.ts --repeat-each 15
npx playwright test test/extension.spec.ts --repeat-each 5
```

### 3.1 The flake is gone

With fresh private copies of the native host and the endpoint fixture (`LOCKDOWN_TEST_BINS`), after the fix:

| Run | Result |
|---|---|
| `a11y.spec.ts --repeat-each 15` | 105 passed, 0 failed (each save-prompt test ran 15 times in light and 15 in dark) |
| `extension.spec.ts --repeat-each 5` | 185 passed, 0 failed (37 tests × 5) |
| The whole extension suite, once | 79 passed, 1 skipped, 0 failed |

Before the fix, the whole suite failed on its first local run at the light save-prompt test (64 passed, 1 failed, 1 skipped, 5 not run). A 30-times run of `a11y.spec.ts` failed once (204 passed, 1 failed), and that failure gave the evidence in §1.1.

The extension suite grew from 71 tests to 80. The design-system ratchet passes, and so do the axe checks (all in `a11y.spec.ts` above).

### 3.2 A desktop race fixed on the way

CI on this branch's first commit failed the desktop vitest "keeps the guest card and its words while a native print panel is up, then drops the words" (from slice 37, doc 38). In `apps/desktop/src/features/vault/wifi-card.tsx`, the `printingNow` ref was set from `printing` in a plain effect, which runs after paint. So a `blur` landing just after the print panel closed still read "printing", and the card stayed up. The ref is now set in a layout effect. A layout effect runs in the same task as the render that drops the words, before any event can be handled. The test is unchanged. It is a separate commit because it is unrelated to the frames.

## 4. Gaps

1. **A see-through cover keeps the frame disabled, with no message.** Chromium's IO v2 counts an element on top as cover even at `opacity: 0`. A page can leave Lockdown's menu disabled for good this way. That is the safe failure (nothing fills), but the person sees a menu that never arms and isn't told why. A later slice could show a message when the frame has been covered for a few seconds. That message would have to come from the frame itself, never from the page.
2. **Before IO v2's first report, the frame counts as not covered.** `visible` starts as `null`, so the arming cue runs and a click could count after 400 ms if Chromium never sent a first report. Chromium always sends one within about 100 ms (it did in every probe). A frame that gets no report would be a browser bug, not a page attack. A stricter rule (no click counts until the first report on a browser that has IO v2) is left for the audit (D503).
3. **Firefox has no IO v2.** It uses the host's own hit test (`hostVisible`), which the background checks at pick time. This slice didn't change it. The duplicate-prompt race existed there too and is fixed by the same change.

## 5. unicron requests (host-contract gaps)

None. No view or token changed. The extension's visuals are unchanged, so the states weren't taken again.

## 6. Audit notes

- **No secret moves differently.** The change is in when frames are drawn, not in what they hold or send. Every pick still goes through the background's `fill_check` and the app's gates.
- **The guard is unchanged.** IO v2's `trackVisibility` with a 100 ms delay, the 400 ms settle, trusted events only, and covered means disabled. A page can still make Lockdown's frames refuse, as before. It cannot make them accept anything more easily.
- **The race gave a page nothing to use.** A page could set off a second open (by focusing a field twice), but the copy it got was an extension-origin frame like the first. It was covered by the newer one, so it stayed disabled. Now the second copy isn't drawn at all.

## 7. Proposed decisions

For the coordinator to record. **Flagged** rows are user-visible and need Sean. "Audit item" rows go to the external audit under D38.

| ID | Decision | Owner |
|---|---|---|
| D500 | **One frame of each kind per page, enforced before the first `await`.** The content script sets up the save prompt once while one is being set up (`promptPending`), and each menu open takes a turn (`menuTurn`). An open that comes back after a newer one began draws nothing and forgets its registration. Plain version: Lockdown never stacks two copies of its save prompt or its menu on a page. | Under D375; audit item |
| D501 | **IO v2's report stands; no re-observe or timer to "unstick" it.** The stuck Save was a real cover (Lockdown's own second prompt). A fresh observer agrees with the old one, and IO v2 recovered from every fade, slide, animation and brief cover probed. Plain version: Lockdown didn't loosen its "is anything on top of me" check to fix this. | Under D375; audit item |
| D502 | **A see-through element over a Lockdown frame counts as cover.** Chromium reports a frame under an `opacity: 0` element as not visible, and Lockdown keeps the frame disabled for as long as the element stays. Showing the person why is gap 1. Plain version: if a page hides something invisible over Lockdown's menu, the menu won't work there, and for now it doesn't say why. | Under D375; **flagged for Sean** (a menu that never arms, without a message) |
| D503 | **The first-report gap is written down, not changed.** Before IO v2's first report a frame counts as not covered, as in D159. Whether to refuse every click until that report, on browsers that have IO v2, goes to the audit (gap 2). Plain version: one corner of the click check is noted for the security review. | Under D375; audit item |
