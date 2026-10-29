# 32: Accessibility structure

_Slice: `design/a11y-structure`, built 2026-10-29. The second build from the design review's fix list ([`docs/design/2026-10-29-review.md`](../design/2026-10-29-review.md), "Fix list, in build order", item 2): rows R7, R4 and R18. Nothing here changes a wire shape, a gate or what leaves the core. Proposed decisions D390–D399 (§9)._

## For future agent

- **TL;DR:** Lists are lists (R7): every child of a `role="list"` is a list item. The item detail's blocks wrap each child in a list item (`ListItems`), the codes are Lockdown's own `ul` inside unicron's `CodeDisplayList` (whose live region is a sibling, not an item), five more `ItemGroup`s mark their `Item`s, and the extension's menu and passkey sheet put their rows in list items with "Use another device" after the list. The in-page frames read unicron's colours (R4): `extension/static/tokens.css` is generated from `@unicron/tokens` 0.6.0 as plain CSS, and `ui.css` aliases its roles under `--ld-*` names; the "close to invisible" text the review saw came from the capture harness leaking the apps' stylesheet into the frame, which is fixed too. The crown-jewel field's dimmed "Show large type" is disabled while the gate asks. Headings and landmarks (R18): one `h1` per screen at every width, unique names for the list's groups, a `main` in the quick-access window and in the frames, a status for the loading skeleton, and focusable scrolling text.
- **Read with:** `apps/desktop/src/components/list-items.tsx`, `apps/desktop/src/test/a11y.ts`, `extension/scripts/tokens-css.mjs`, `extension/static/{tokens,ui}.css`, `extension/src/pages/frame.ts` (`choices`, `header`), `extension/test/a11y.spec.ts`, `design/capture/{frame.html,src/frame-main.ts}`.
- **Do not:** put anything but list items directly in an `ItemGroup` or another `role="list"` (wrap with `ListItems`, or give an `Item` `role="listitem"`); hand-edit `extension/static/tokens.css` (run the script); use unicron's `--muted` or `--accent` as an ink in the frames (they are surfaces; use `--ld-ink-muted`); give a frame or a pane a second `h1`; load the apps' bundle into the harness's frame page.

## 1. What was built

| Row | Piece | What it does | Where |
|---|---|---|---|
| R7 | `ListItems` | Each child of a list container in its own `role="listitem"` block; fragments are opened (`<>{a}{b}</>` is two items) and empty children dropped, so a block never announces an empty item. A plain block, so a flex column lays out as before | `apps/desktop/src/components/list-items.tsx` |
| R7 | Detail blocks | `Block` (Sign-in methods, Websites, Network, One-time code, Wallet, Card, every template section, "Your fields") holds `<ItemGroup><ListItems>…</ListItems></ItemGroup>` | `apps/desktop/src/features/vault/detail-pane.tsx` |
| R7 | Codes | `CodeDisplayList` keeps its job (one polite live region for all the codes) and no longer carries `role="list"`; inside it, `<ul role="list" aria-label="One-time codes">` with one `li` per `CodeDisplay`. Its live region was the list's stray child | `apps/desktop/src/features/codes/codes-view.tsx` |
| R7 | Other lists | `role="listitem"` on the `Item`s of "Signed in elsewhere", Files, the template designer's own kinds and Recovery's three rows (unicron's `Item` takes `role` as a prop); the opened share's values go through `ListItems` | `features/vault/{away-block,template-designer}.tsx`, `features/organization/files-panel.tsx`, `features/recovery/recovery-view.tsx`, `features/sharing/opened-panel.tsx` |
| R7 | The frames' lists | `choices(label, rows, after)`: a scroller holding a named `role="list"` (one `role="listitem"` per row button) and, after it, anything that isn't a choice ("Use a passkey from another device", "Use another device or security key") | `extension/src/pages/frame.ts` |
| R4 | `tokens.css` | unicron's colour roles as plain CSS for the frames: the thirteen roles `ui.css` aliases, the nine neutrals, two reds and two white alphas they name, values copied as `@unicron/tokens` declares them, dark under `prefers-color-scheme: dark`. Written by `scripts/tokens-css.mjs` from the installed package's `dist/tokens.css`; `--check` fails when it is stale | `extension/static/tokens.css`, `extension/scripts/tokens-css.mjs` |
| R4 | `ui.css` | Every colour is an `--ld-*` alias of a unicron role: surface `--popover`, ink `--popover-foreground`, secondary text `--muted-foreground`, dividers `--border`, control edges `--input`, hover `--accent`, primary `--primary` / `--primary-foreground`, focus `--ring`, links the ink underlined, a stop `--destructive` on a 12 % tint of it. `frame.html` links `tokens.css` first; the manifest exposes it beside `ui.css` | `extension/static/{ui.css,frame.html}`, `extension/manifest.base.json` |
| R4 | The crown-jewel field | While the gate asks (`GateProvider`'s new `asking`), `ShowLargeType` is disabled, as the secret field's own reveal and copy are | `apps/desktop/src/app/gate.tsx`, `features/vault/large-type.tsx` |
| R4 | The harness's frame | The in-page frames load their own page (`design/capture/frame.html`, entry `src/frame-main.ts`) with the extension's two sheets, instead of the review page's bundle | `design/capture/{frame.html,vite.config.ts,src/frame-main.ts,src/main.tsx,src/states/extension-frame.ts,src/harness/test-page.tsx}` |
| R18 | One `h1` at every width | `MasterDetail` takes the screen's `title`; with an item open, the phone bar (hidden from `md` up) carries it as an `sr-only` `h1`, while the list pane and its visible `h1` are hidden below `md`. The item's own title stays an `h2` | `apps/desktop/src/components/master-detail.tsx`, `features/vault/vault-view.tsx` |
| R18 | Unique section names | The list's kind groups are named with their counts ("Notes, 2 items"), so the open login's own "Notes" block is the only region called "Notes" | `features/vault/vault-view.tsx` |
| R18 | The quick-access window | A `main` around it and an `sr-only` `h1`, "Lockdown quick access", the OS window's own title (`quick.rs`) | `apps/desktop/src/app/quick-window.tsx` |
| R18 | Loading | The vault list's skeleton is `role="status"` with "Loading items" inside it, not an `aria-label` on a plain `div` (axe's `aria-prohibited-attr`) | `features/vault/vault-view.tsx` |
| R18 | Scrolling text | The kit page's text (`OncePage`) is a focusable region, "The page as text"; a Markdown code block is focusable too, since both scroll sideways on a phone | `features/status/once-page.tsx`, `features/organization/markdown-view.tsx` |
| R18 | The frames | The brand in each frame's header is its `h1` ("Lockdown", drawn as before); the frame's `main` is named "Lockdown", so it stays apart from the host page's own `main` | `extension/src/pages/frame.ts`, `extension/static/{frame.html,ui.css}` |
| R18 | The popup, the web vault's start | An `sr-only` `h1` "Lockdown" where the popup has no visible title (connecting, unlocked); "Lockdown web vault" while the web vault starts | `extension/src/popup/popup.tsx`, `apps/web/src/app/web-app.tsx` |
| R18 | The harness's test page | The plain page around a frame has its own `h1` and `main`, so axe's page rules judge Lockdown's frame and not the stand-in (the review's `region` hits on `div[data-capture-frame]`) | `design/capture/src/harness/test-page.tsx` |

## 2. What caused each rule's hits

Grouped from `axe.json` by rule and by source, before any change (the review's totals: critical 420, serious 41, moderate 293 image × rule pairs; the file as slice 30 left it: 404, 41, 291).

| Rule (impact) | Cause | Fix |
|---|---|---|
| `aria-required-children` (critical) | unicron's `ItemGroup` is `role="list"`; the detail's `Block` filled it with secret fields, codes, passkey lists and sentences (every login, card, Wi-Fi, document, wallet…). The codes view put `role="list"` on `CodeDisplayList`, whose own live-region `span` is a child. Five more `ItemGroup`s held bare `Item`s. The frames' `.list` held buttons and a link | §1, R7 rows |
| `aria-required-children` (critical), left | unicron's `ApprovalSheet` draws its context ("Device", "Where", …) as an `ItemGroup` of bare `Item`s (`data-slot="approval-sheet-context"`); no prop reaches it. Seen only in the seam state (`desktop-approval-sheet-seam`), since every real approval uses a host dialog (brief §11.1) | unicron request, §6 |
| `color-contrast` (serious), frames | Not the frames' own values: in the real extension `ui.css` met AA. The capture harness's frame shared the review page's entry, and the bundler preloaded the apps' stylesheet into the frame with it; that sheet's `--muted` (a surface, neutral-100) and `--accent` (neutral-100) replaced the frame's inks of the same names, so hosts, usernames, "Protected", badges, links and the Save button drew at 1.16:1. Fixed at both ends: the harness's frame is its own page, and the frame's names can't collide again (`--ld-*`), on unicron's values | §1, R4 rows |
| `color-contrast` (serious), crown jewel | While the gate asks, unicron's `SecretField` disables its reveal and copy, and its `InputGroup` dims the whole row (`has-disabled:opacity-50`), Lockdown's "Show large type" in it too, which stayed enabled: a pressable button at 2.86:1 | Disabled while the gate asks; unicron request, §6 |
| `aria-prohibited-attr` (serious) | `aria-label="Loading"` on the skeleton's plain `div` | A status with its words |
| `scrollable-region-focusable` (serious) | The kit page's `pre` scrolls sideways at 390 px and couldn't take focus | `tabIndex={0}`, named region |
| `page-has-heading-one` (moderate) | At 390 px an open item hides the list pane, the only `h1`. The quick-access window, the popup's unlocked and connecting states, the web vault's start and the frames had none. The harness's test page had none | §1, R18 rows |
| `landmark-unique` (moderate) | The list's "Notes" group and the open login's "Notes" block (and, before slice 30, the Security box drawn twice) | Groups named with counts |
| `landmark-one-main`, `region` (moderate), quick window | No `main` | `main` |
| `region` (moderate), harness | `div[data-capture-frame]`'s test page outside any landmark | The test page is a `main` |
| `region` (moderate), left | unicron's `DropdownMenuContent` and `TooltipContent` portal to `<body>`, outside every landmark, with no `container` prop (axe exempts dialogs, not menus or tooltips): the sort, new-item, row, item, share, vault and dismiss menus, and a tooltip open on focus in two states | unicron request, §6 |

## 3. axe, before and after

`pnpm capture` (default folder, light and dark, every width), image × rule pairs and elements:

| Impact | Review (2026-10-29) | File before this slice | After | Elements before → after | What is left |
|---|---|---|---|---|---|
| Critical | 420 | 404 | **6** | 658 → 6 | `aria-required-children` in unicron's `ApprovalSheet` context list, `desktop-approval-sheet-seam` only (§6) |
| Serious | 41 | 41 | **0** | 70 → 0 | none |
| Moderate | 293 | 291 | **52** | 375 → 52 | `region` only: unicron's menus (sort, new item, row, item, share, vault, dismiss) and two focus-opened tooltips (`desktop-item-revealed`, `desktop-editor-templates`) portal outside every landmark (§6) |

By rule, before → after (pairs): `aria-required-children` 404 → 6, `color-contrast` 33 → 0, `aria-prohibited-attr` 6 → 0, `scrollable-region-focusable` 2 → 0, `page-has-heading-one` 154 → 0, `region` 106 → 52, `landmark-unique` 19 → 0, `landmark-one-main` 12 → 0. The run took 1,298 images (the file before held 1,250, after slice 30's partial re-takes); every state reported ready and no step failed. One hit seen only in a probe, never in the capture: Base UI's focus guards around the new-item menu at 390 px (`aria-hidden-focus`, §6).

## 4. How to run and test

```sh
cd apps/desktop
pnpm vitest run src/components src/app/quick-window.test.tsx \
  src/features/vault/{detail-pane,vault-view,crown-jewel}.test.tsx \
  src/features/{codes,recovery} src/features/status/once-page.test.tsx
pnpm ds:ratchet                    # stays at 4

cd ../../extension
node scripts/tokens-css.mjs --check # after a @unicron/tokens update: run it without --check
bash build-wasm.sh debug && pnpm build
LOCKDOWN_TEST_BINS=<a folder with lockdown-native-host and browser-endpoint> \
  pnpm exec playwright test test/a11y.spec.ts test/tokens.spec.ts test/manifest.spec.ts

cd ../design/capture
pnpm build && pnpm capture        # axe.json beside the images
```

`apps/desktop/src/test/a11y.ts` holds two DOM walks for jsdom, where axe's layout-bound rules can't run: `malformedLists(root)` (every list whose element children aren't all list items) and `repeatedLandmarks(root)` (landmarks sharing a role and name). The extension's suite runs axe-core itself (pinned at 4.14.0, the capture's version) inside each frame's document, because the frame sits in a closed shadow root that an axe run from the page can't enter.

## 5. Tests added

Desktop (Vitest): 13 new in 9 files (4 new files). Extension (Playwright): 7 new in 2 new files.

| Test | What it covers |
|---|---|
| `components/list-items.test.tsx` (2) | `ListItems` inside unicron's `ItemGroup`: five items from plain children, a fragment, an array, with `null` and `false` left out, and no malformed list; the check itself catches an `ItemGroup`'s bare child |
| `features/vault/detail-pane.test.tsx` (2) | A login's Sign-in methods and Websites are lists of items; a card's and a Wi-Fi network's blocks hold nothing else. Both fail with `ListItems` taken out |
| `features/codes/codes-view.test.tsx` (1) | One list item per code, each a `CodeDisplay`; the live region is outside the list |
| `features/recovery/recovery-view.test.tsx` (1) | One `h1`, "Recovery"; the three rows are list items |
| `components/master-detail.test.tsx` (2) | Nothing open: one `h1`. An item open: the phone bar's `sr-only` `h1` with the screen's title, hidden from `md` up, while the list pane's is hidden below `md`; the item's title stays an `h2` |
| `app/quick-window.test.tsx` (1) | One `main`, one `h1` "Lockdown quick access", the locked state inside it |
| `features/vault/vault-view.test.tsx` (2) | Loading is a status with "Loading items" and no `aria-label`; the groups are "Notes, 2 items" and "Logins, 1 item", no region is called plain "Notes", and no landmark repeats |
| `features/status/once-page.test.tsx` (1) | The kit page's text is a focusable `pre`, named "The page as text" |
| `features/vault/crown-jewel.test.tsx` (1) | While the gate asks, "Show large type" in the field's row is disabled with the field's own buttons, and enabled again after Cancel; fails without the change |
| `extension/test/a11y.spec.ts` (5) | The real build in Chromium over the real native host and endpoint: the fill menu (light, dark), the save prompt (light, dark) and the passkey sheet (save and sign-in, light and dark) have no serious or critical axe violation; each frame is in the scheme asked, has one `h1` "Lockdown" and a `main` named "Lockdown"; the menu's rows and the sheet's passkeys are list items, with "Use another device" outside the list. It fails on a muted ink of `#d8d8d8` |
| `extension/test/tokens.spec.ts` (2) | `static/tokens.css` is what the script writes from the installed `@unicron/tokens`; `frame.html` links it before `ui.css`; the built sheet names its source and puts dark under `prefers-color-scheme`, with no `.dark` |
| `extension/test/manifest.spec.ts` (changed) | `tokens.css` is among the frame's web-accessible resources |

## 6. unicron requests and host-contract gaps

| Missing in unicron | Lockdown's workaround | Row |
|---|---|---|
| A plain-CSS token sheet (custom properties only, dark under `prefers-color-scheme` as well as `.dark`, no `@theme`) | `extension/static/tokens.css`, generated from `@unicron/tokens` 0.6.0's `dist/tokens.css` by `extension/scripts/tokens-css.mjs` | R4 (already in brief §11.1; now with a stand-in) |
| `CodeDisplayList` list semantics: a list with each code an item, and its live region outside the list | Lockdown's own `ul` inside `CodeDisplayList` | R7 |
| `ItemGroup` either not a list by default, or `Item` a list item when inside one | `ListItems`; `role="listitem"` on `Item` | R7 |
| `ApprovalSheet`'s context rows as list items | none (seam state only; real approvals use host dialogs) | R7 |
| `SecretField`: dim only its own disabled controls while it gates, not the host's `more` slot (or pass the slot a `disabled` flag) | `GateProvider.asking` disables `ShowLargeType` | R4 |
| A `container` (portal) prop on `DropdownMenuContent` and `TooltipContent`, so a host can keep popups inside a landmark | none | R18 |
| Base UI's focus guards around a nested menu at phone width (`aria-hidden` spans with `tabindex="0"`, axe `aria-hidden-focus`) | none | R18 |

**unicron host-contract gaps** beyond these: none. No class or style overrides were added on unicron components (`role` on `Item` is a prop); the ratchet stays at its baseline of 4.

### 6.1 Security notes

- No new path releases a secret, and no gate changed. `GateProvider.asking` only reports whether a gate dialog is open; `ShowLargeType` still goes through `gate.run` for its value.
- The frames' look changed (unicron's neutral primary for the old blue, the ink for links), their behaviour didn't: arming, the visibility check, D159's two heights and every message are as before. A page still learns nothing from the frame but its height.
- The list's accessible names in the frames carry the host the frame already shows; nothing from the page reaches them but `data.host`, set as text.

## 7. Gaps

- **jsdom lays nothing out.** The `h1` at each width is checked by its classes (`md:hidden`, `hidden md:flex`), and contrast only by axe in a browser (the extension's suite, the capture).
- **Menus and tooltips stay outside landmarks** (§6); axe's `region` is a best-practice rule and the content is reachable.
- **The approval sheet's context list** (§6) keeps one critical hit per seam image.
- **The token sheet is unicron 0.6.0's**, the version the override pins; when the override moves, rerun the script (the suite fails until then).
- **The frames now use unicron's neutral primary** where they had a blue of their own (D392).

## 8. Counts

- Desktop Vitest: 213 before, 226 after (44 files, all passing).
- Web Vitest: 45 before and after (40 pass, 5 skip). Web end to end: 10 of 10 pass (the views it draws come from the desktop).
- Extension Playwright: 54 pass and 1 skip (Firefox not installed) before; 61 pass and 1 skip after.
- Design capture harness: `pnpm typecheck` and `pnpm lint` pass.
- Rust: no Rust changed. `cargo fmt --check`, `cargo clippy --workspace --all-targets`, `cargo clippy -p lockdown-web --target wasm32-wasip1`, `cargo clippy -p lockdown-desktop --all-targets --features shell` and `cargo deny check` pass. `cargo test --workspace` and `cargo test -p lockdown-desktop --features shell` are left to CI, as no Rust changed.
- Design-system ratchet: 4 class overrides on the desktop, unchanged; the popup's, unchanged.
- Re-taken captures: all of them, `pnpm capture` into `/home/user/design-capture/2026-10-29/` (1,298 images). The review build (`pnpm review:build`) is 17 files (one more than slice 30's: the frames' `frame.html`), 6.7 MB.

## 9. Proposed decisions

For the coordinator to record. **Flagged** rows are the ones Sean should see.

| ID | Decision | Status |
|---|---|---|
| D390 | **A list's children are its items.** Anything in an `ItemGroup` or another `role="list"` is a list item: an `Item` takes `role="listitem"`, anything else goes through `ListItems`; what isn't a choice (a "use another device" link) follows the list instead of sitting in it. Plain version: a screen reader hears "list, 3 items" and then three items. | Accepted by default under D38 (structure) |
| D391 | **The in-page frames draw with unicron's roles from a generated plain-CSS sheet** (`extension/static/tokens.css`, from `@unicron/tokens` by `scripts/tokens-css.mjs`, checked in the suite), aliased in `ui.css` as `--ld-*`; it goes when unicron ships its own plain sheet. Plain version: the menu, the save prompt and the passkey sheet use the same colours as the app, and their grey text is readable in light and dark. | Accepted by default under D38 (structure) |
| D392 | **The frames' primary button is unicron's neutral primary** (near-black in light, near-white in dark) instead of the frames' own blue, and links are the ink, underlined. Plain version: "Save", "Fill" and "Sign in" in the page look like the app's buttons. | Proposed; **flagged for Sean** (the in-page look) |
| D393 | **A dimmed control is a disabled one.** While the gate asks, an action Lockdown adds to a secret field's row ("Show large type") is disabled, as the field's own reveal and copy are. Plain version: nothing greyed out can still be pressed. | Accepted by default under D38 |
| D394 | **One `h1` per screen at every width**, the screen's existing title: on a phone with an item open, the back bar carries the list's title as a heading for a screen reader only; the quick-access window, the popup and the web vault's start get one the same way; a frame's brand is its `h1`. Plain version: a screen reader always finds the page's title first. | Accepted by default under D38 |
| D395 | **The list's groups are named with their counts** ("Notes, 2 items"), so no two sections of a screen share a name. Plain version: "Notes" means the item's notes, and the list's notes say how many. | Proposed; **flagged for Sean** (what a screen reader says) |
| D396 | **Text that scrolls sideways takes keyboard focus** (the kit page, Markdown code blocks). Plain version: you can scroll the recovery kit's text with the keyboard on a phone. | Accepted by default under D38 |
| D397 | **The capture harness draws each in-page frame from its own page** (`design/capture/frame.html`) with only the extension's sheets, and its plain test page has an `h1` and a `main`. Plain version: the review's screenshots of the in-page menu show what the browser shows. | Accepted by default under D38 (tooling) |
| D398 | **The extension's suite runs axe in the menu, the save prompt and the passkey sheet, light and dark, and fails on serious or critical violations**, with `axe-core` pinned to the capture's version. Plain version: a contrast or list mistake in the page's frames fails the build. | Accepted by default under D38 (tests) |
