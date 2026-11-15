# 86: The extension's in-page frames on unicron's plain-CSS sheet

_Slice: `edges-3-frames` of the 2026-11 redesign, lane `edges` (branch `wt/lockdown-edges`), built 2026-11-15. Briefs B2 (fill menu) and B3 (save prompt and passkey sheet) of the undrawn-areas brief; inventory rows 128 to 131 (§2.12). No drawn frame: the extension has no Figma screen. Decisions D940 to D949. Answers design review R3 and R17 again, in the new look._

## For future agent

- **TL;DR:** the three in-page frames (fill menu, save prompt, passkey sheet) keep `frame.ts`'s logic line for line and change their look: `static/tokens.css` is now unicron's plain sheet (`@unicron/tokens/plain.css`, contract #54) cut down by `scripts/tokens-css.mjs`, with Geist and Geist Mono as files in the extension; `static/ui.css` is rewritten on those roles only (surface-raised, a 1 px `text-label` hairline, 2 px corners, mono caps buttons, 32 px header with the L mark, 48 px rows).
- **Do:**
  - Regenerate the sheet after a tokens update: `node scripts/tokens-css.mjs` (the `--check` form runs in `tokens.spec.ts`). It reads `plain.css`'s blocks by their comment heads and stops on a missing one; it never guesses a value.
  - Name a new role or type role in `ROLES` / `TYPE` of `tokens-css.mjs` before `ui.css` uses it.
  - Keep the fill menu at two heights (64 and 232) and the prompt and sheet at 150 / 260: `clickjacking.spec.ts` now proves the menu's two heights and that nothing is cut at them.
- **Do not:**
  - Hand-edit `static/tokens.css`, or add a colour, a corner or a font that is not in it to `ui.css`.
  - Load anything else into a frame: `frame.html` links `tokens.css`, `ui.css` and `frame.js`, and `tokens.css` names only `fonts/*.woff2` beside it.
  - Move the 400 ms (`SETTLE_MS`) into a motion token: it is the clickjacking rule, not an animation choice.
- **Read with:** briefs B2 and B3 (workspace `07-projects/29-unicron/impl-2026-11/app-lockdown-undrawn.md`), doc 16 §6 (the frame's visibility rules), doc 19 §3 (the passkey sheet), doc 32 (frame a11y), doc 37 §2 (R17's arming cue), doc 62 §6 (the earlier token chain).

## 1. The token sheet (#54)

`scripts/tokens-css.mjs` now reads `@unicron/tokens/dist/plain.css` (pre.2 ships it), not `dist/tokens.css`:

| Block of plain.css | What the sheet keeps | Where it goes |
|---|---|---|
| Tier 1 · App theme: lockdown | the slots the kept roles name (stone and graphite) | `[data-theme="lockdown"]` (set on `<html>` in `frame.html`) |
| Tier 1 · Global primitives | the shared primitives the roles name (critical colours, radius, strokes, the two font stacks) | `:root` |
| Type roles, Font roles | family, weight, size and tracking of ui-emphasis, caption, meta, overline, code-display, brand-wordmark; `--font-sans`, `--font-mono` | `:root` |
| Tier 2 · Semantic roles · light (stone) | surface-raised, line-rule, text-body, text-label, primary-fill, primary-on-fill, critical-text, critical-soft, state-hover, state-pressed, ring, radius, stroke-rule, stroke-emphasis | `:root` |
| Tier 2 · Semantic roles · dark (graphite) | the same roles | `@media (prefers-color-scheme: dark) { :root { … } }` |

- plain.css picks graphite by a class on the root (`.dark`, or `.system` for the browser's scheme, which it writes under the media query only for `:root.system`). A frame takes no class from anyone, and the frames need about 70 of its 900 declarations, so the script writes the graphite roles under the media query itself (gap 1).
- The fonts: `@font-face` rules for the Latin and Latin Extended subsets of Geist and Geist Mono (upright, variable weight), copied from the font packages' own `wght.css` (unicode ranges included) and pointed at `fonts/<file>`. `scripts/build.mjs` copies those four files and both SIL OFL licences into `dist/<family>/fonts/` from the packages beside `@unicron/ui` (no new dependency; the lockfiles do not change). Other scripts fall back to the system sans and mono, as before.
- The fonts need no `web_accessible_resources` entry: the frame (extension origin) loads them itself. Checked in Chromium with the built extension embedded in a web page (both faces load), and in `a11y.spec.ts` (§6). `manifest.base.json` and the manifest test are unchanged.

## 2. The look (ui.css, brief 1.3)

| Part | Built |
|---|---|
| Frame | `surface-raised`, a 1 px hairline in `text-label` so the edge holds on any page, 2 px corners (`--radius`; was 10). The host keeps its shadow; its iframe's corner is 2 px too (`content/host.ts`) |
| Header, 32 px | the mark (16 px `primary-fill` square, "L" in `primary-on-fill`), LOCKDOWN in the brand wordmark role (the `h1`), the host in mono (`type-meta`, `text-label`) cut in the middle (the last 12 characters, where the site's name sits, always show), a close glyph (inline SVG, `aria-label="Close"`) |
| Row, 48 px | a button: the title in `type-ui-emphasis`, one mono caps word after it (`type-overline`, `text-label`): PASSKEY, RELATED SITE, SAME SITE; under it one mono line (`type-meta`): the username, PROTECTED, `One-time code // sam`, the card's or identity's subtitle |
| Buttons | the label role of unicron's Button: `type-overline`, upper case (the DOM keeps sentence case, so names and tests read "Save"); 32 px; primary `primary-fill` / `primary-on-fill`; secondary a 1 px inset ink edge; text buttons boxless; hover and press `state-hover` / `state-pressed`; the focus ring 2 px in `--ring`, inside a full-width row |
| Message | one sentence in `type-caption` |
| Question | the core's sentence, or the prompt's, in `type-ui-emphasis` (`.msg.ask`) |
| Stop | `critical-soft` with `critical-text`, led by a bold "Stop:" (`frame.ts` adds it when the sentence lacks it; the look-alike stop already has it) |
| Pairing | the sentence, then the code in `type-code-display` |
| Arming (R17) | while a control waits out its 400 ms it keeps its full colours and a 2 px rule (`--stroke-emphasis`, current colour) under its label fills from the left; armed, it is plainly enabled; covered, 50% (was 60%); under reduced motion the rule is drawn whole and does not move |

Copy changes (B2): "Follows {host}'s rules" (was "password rules"); "Made by Lockdown // N bits, strength" (was "·"); a menu passkey row's line is the user name alone, with PASSKEY after the title (was "Passkey · {user}" and the word); "Use another device or security key" in the menu too (was "Use a passkey from another device"; the sheet already said it); PROTECTED in caps.

## 3. What did not change

- `frame.ts`'s logic: every message to the background, `heightFor`, `size`, `deliberate`, `arm`, `cue`, `SETTLE_MS`, re-arming on cover, the nonce check, Escape closes, untrusted events ignored. The edits are look hooks only: the mark, the split host, the close glyph, `stopWords`, the `title-text` span, the `ask` class, and the copy above.
- The frame sizes: menu 320 × 64 / 232; prompt 320 × 150; sheet 360 × 150 / 260 (B3 keeps the off-grid 150 as an accepted miss: these are the clickjacking-tested sizes).
- Opaque extension-origin frames in the top layer, a closed shadow root, at least 240 × 44; the extension CSP (`script-src 'self' 'wasm-unsafe-eval'; object-src 'self'`); `prefers-color-scheme` switching; nothing a page can influence is loaded.
- The popup (B1 is another slice) and its ratchet (0).

## 4. States (design/capture)

All 23 frame states keep their ids and gain `built: { area: "extension", screen: <state>, kind: "undrawn" }`: `extension-menu-logins`, `-protected`, `-passkeys`, `-code`, `-card`, `-card-confirm`, `-identity`, `-suggest`, `-lookalike`, `-locked`, `-unpaired`, `-pairing`, `-no-app`, `-covered`, `-arming`, `extension-prompt-save`, `-prompt-update`, `-prompt-arming`, `extension-sheet-get`, `-sheet-confirm`, `-sheet-create`, `-sheet-held`, `-sheet-locked`. `states/extension-frame.ts` takes the four font files from the harness's own install of the same packages (5.3.0 in both lockfiles) and points `tokens.css`'s `fonts/` names at them, and gives its iframe the host's 2 px corner while `harness/test-page.tsx` (foundation) still draws 10.

## 5. Tests and what is not proven

- `tokens.spec.ts`: unchanged; the sheet is current with `@unicron/tokens` (now from plain.css), dark by media query, no dark class.
- `clickjacking.spec.ts`: new, six cases: the built `frame.html` over a stand-in background in each menu state (logins, locked, no app, look-alike, unpaired, a card's second press) posts only 64 or 232, ends on the right one, and draws nothing past it (R3: the stop is never cut).
- `a11y.spec.ts`: the menu and the save prompt also prove both Geist faces loaded in the real extension frame.
- `extension.spec.ts`: the covered opacity reads 0.5 (was 0.6) and the suggestion's line "Follows 1800flowers.com's rules". `passkeys.spec.ts`: the menu's passkey row is found by "passkey sean@example.com". Same meaning, the control's new words.
- `manifest.spec.ts`, `package.spec.ts`, `firefox.spec.ts`, `menu-pairing.spec.ts`: unchanged. The reproducible zip: two clean builds packed with `SOURCE_DATE_EPOCH=1700000000` give the same sha256 (30 files per package).
- Not proven: see the hand-off (`lockdown-edges-3-frames.md`): this session's shared Rust target lacks `examples/browser-endpoint`, and lanes never run cargo, so the specs that start the desktop endpoint (extension, a11y, passkeys, menu-pairing, firefox, bridges, open-app) did not run here. Firefox's loading of the fonts is not observed. The arming rule mid-fill is not pictured: the capture runs under reduced motion, where the rule is whole by design.

## 6. Gaps

1. **plain.css has no root form that follows the browser without a class.** Its graphite blocks are under `.dark` and `:root.system`. The script writes them under `@media (prefers-color-scheme: dark) { :root }` itself. Screens: every frame state in graphite.
2. **plain.css carries no spacing.** The frame's 32 px header, 48 px rows, 12 px gutter and 8 px gaps are written in `ui.css` as the frame's layout (brief B2's numbers). Screens: every frame state.
3. **plain.css has no line heights for the type roles** (only the icon glyph has one). `ui.css` sets 1.3, 20 px for a row title and 16 px for its line. Screens: every frame state.
4. **The capture's test page draws the iframe with a 10 px corner** (`harness/test-page.tsx`, foundation). The frame state corrects it from inside (§4); the integrator should set 2 px there and drop `hostCorner()`.

## 7. Unicron host-contract gaps

- #54 (`plain.css`) exists in pre.2. Gaps 1 to 3 above are asks for it: a prefers-color-scheme block at `:root`, a spacing scale, and line heights for the type roles.

## 8. Proposed decisions (D940 to D949)

| # | Decision | Answers |
|---|---|---|
| D940 | The frames' `tokens.css` is generated from `@unicron/tokens/plain.css` (#54), filtered to the roles and type roles `ui.css` names, graphite under `prefers-color-scheme: dark`; never hand-written | B2 look; brief 1.3 |
| D941 | Geist and Geist Mono (Latin and Latin Extended, upright) ship in the extension as files under `fonts/`, with their OFL licences, copied from the packages beside `@unicron/ui`; no web-accessible listing | B2 "Keeps / changes" |
| D942 | The frame's corner is 2 px (`--radius`), in the frame and in the host's iframe (was 10) | B2 look |
| D943 | The frame's edge is a 1 px hairline in `text-label`, so it holds on a page of any colour; the host's shadow stays | B2 look |
| D944 | Stops read "Stop: …" on `critical-soft` in `critical-text`, and always take the taller height (R3, kept) | B2 stop, R3 |
| D945 | The arming cue is a 2 px rule under the label that fills over the 400 ms; covered is 50%; reduced motion draws the rule whole (R17, kept in meaning) | R17 |
| D946 | Frame buttons draw unicron Button's label role (mono caps, 32 px); the DOM keeps sentence case, so names and tests are unchanged | §5 copy rule |
| D947 | The fill menu's rows are 48 px: title, one mono caps word (PASSKEY, RELATED SITE, SAME SITE), one mono line; a passkey row's line is the user name alone | B2 rows |
| D948 | The menu's passkey escape reads "Use another device or security key", as the sheet's | B2 after the list |
| D949 | The host is cut in the middle, keeping its last 12 characters, where the site's own name sits | B2 header |

Each is a change a person sees (D38): flagged for Sean. None answers a §6.1 question: the extension has no Figma frame, so no row of §6.3 changes; §2.12 rows 128 to 131 are what this builds.
