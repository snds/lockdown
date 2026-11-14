# 62: unicron 1.0: Carbon icons, Geist, Lockdown's theme and roles, the Theme choice

_Slice: `found-1` of the 2026-11 redesign, on `claude/keen-ritchie-q2hgb2`, built 2026-11-14. The first slice of the redesign: the desktop app, the web vault, the review harness and the browser extension all move onto the unicron 1.0 pre-release in one step. Every screen keeps its layout and takes the new look; no screen is redesigned here. The redesign's own slices build on this one. No wire, stored-record or IPC shape changes; no Rust change; no new setting in `settings.cbor`. Builds on D38 (who decides what a person sees), D92 (density), D115–D119 (how a unicron bump is recorded), D451 (the engine check), D452 (icons by codepoint, superseded here). Proposed decisions D700–D707 (§9); D708 and D709 are unused numbers of this slice's block._

## For future agent

- **TL;DR:** `@unicron/ui` 0.24 → 1.0.0-pre.2 and `@unicron/tokens` 0.16 → 1.0.0-pre.2 in `apps/desktop`, `apps/web` and `design/capture`, and from 0.12 / 0.6 in `extension`. unicron 1.0 is not released, so both packages are **vendored tarballs** in `vendor/unicron/` (§1). Icons are IBM Carbon, drawn as inline SVG: the codepoint patch, its Vite plugin and their test are gone (§2). Every page sets `data-theme="lockdown"`; the `dark` class picks graphite, no class is stone; Settings → Appearance has a **Theme** row, System / Stone / Graphite, kept on the device (§3). `app-extension.css` points at 1.0's roles only (§4). Geist and Geist Mono ship as files; the web vault's policy still refuses nothing (§5). The extension's popup and its frames' token sheet moved too (§6).
- **Swap line:** Swap to the `@unicron/ui@1.0.0` and `@unicron/tokens@1.0.0` release URLs when Unicron 1.0 is released, and delete `vendor/unicron/`.
- **Read with:** unicron's `docs/07-migrating-to-1.0.md` and `docs/05-consuming.md`; `apps/desktop/src/styles/app-extension.css`; `apps/web/src/theme.ts`; `apps/desktop/src/features/settings/theme-setting.tsx`; `apps/desktop/src/app/template-model.ts` (`templateIcon`); `design/capture/scripts/engine-check.ts`; `extension/scripts/tokens-css.mjs`; doc 10 §4 (the role table), doc 38 §1 and §2 (what this replaces).
- **Do not:**
  - Import `sonner` in app code. Toasts come from `@unicron/ui`'s `toast`; its `Toaster` does not hear sonner's own, and sonner's module adds a `<style>` tag the web vault's policy refuses (§5).
  - Cast a name that arrives as data to `IconName`. Translate it with `templateIcon` (or `iconFromMaterial`, Material first) where it is read (§2).
  - Build a surface or an ink on `--g-neutral-*` or an accent ramp. Those ramps are not themed. Alias a role.
  - Give `--secure`, `--area-codes` or `--attention-text` a new use. They are kept only so that no class silently stops drawing (§4).
  - Put `className` on a unicron component to restyle it back. The ratchet's counts are 4 (desktop) and 0 (popup), unchanged.
  - Wrap a unicron `FieldRow`, `SecretField` or field-layout `CodeDisplay` in something that shrinks to its content (`items-start`, `w-fit`). They measure their own box (a container query) and collapse to nothing (§4.4).
  - Use `text-primary` or `bg-primary` for ink. `primary` is the app's green now: the one primary action, identity text, and selection as `primary-soft`.
  - Run `unicron-codemod icons` again on these trees. A second pass is not safe.
  - Store the theme in `settings.cbor`, or set the ground from an inline script in the web vault.

## 1. Packages

unicron 1.0 is not released until Sean merges its pull request and the release workflow tags it. Until then the two packages are the pre-release tarballs, checked in:

```
vendor/unicron/unicron-ui-1.0.0-pre.2.tgz        sha256 466790467bb267d56775e1c989ef03fc619eb18736b40b4ba54454f219ba61d4
vendor/unicron/unicron-tokens-1.0.0-pre.2.tgz    sha256 7f6591f59e541956c60dd3bbef364df83c234605667f6c2976718a7f4efcda1f
```

**One folder at the repo root, a departure from the redesign's app contract.** That contract (workspace `impl-2026-11/apps-contract.md` §7) says to vendor "in the UI package's folder when the repo has several packages that install separately". Lockdown has four pnpm roots that install separately and no single UI package: the desktop's views are compiled by the web vault and the harness, and the extension's popup by the harness. So the two files sit once at the root, as `patches/` did, and each root names them by a relative path:

| Root | `@unicron/ui`, `@unicron/tokens` and the `@unicron/tokens` override |
|---|---|
| `apps/desktop`, `apps/web`, `design/capture` | `file:../../vendor/unicron/unicron-…-1.0.0-pre.2.tgz` |
| `extension` | `file:../vendor/unicron/unicron-…-1.0.0-pre.2.tgz` |

The override stays for the same reason as before: `@unicron/ui` pins `@unicron/tokens` by exact version (`1.0.0-pre.2`), which is on no registry.

**Swap to the `@unicron/ui@1.0.0` and `@unicron/tokens@1.0.0` release URLs when Unicron 1.0 is released, and delete `vendor/unicron/`.** The same line is in each root's `pnpm-workspace.yaml`. The swap is the two lines in each `package.json`, the override in each `pnpm-workspace.yaml`, four installs, and `node scripts/tokens-css.mjs` in `extension` (the sheet's header names the version).

**Lockfiles.** pnpm writes a `file:` tarball's integrity itself, so all four lockfiles carry `integrity: sha512-…` for both tarballs (the extension's lockfile had none for its unicron tarballs before). The two hashes are the sha512 of the vendored files, checked by hand. `pnpm install --frozen-lockfile --lockfile-only` passes in `apps/desktop` and `extension`.

**Patches.** `patches/@unicron__ui@0.24.0.patch` and `patches/@unicron__ui@0.12.0.patch` are deleted, with the `patchedDependencies` entry in each of the four `pnpm-workspace.yaml` files. unicron 1.0 ships what doc 38 §2.4 asked for in a stronger form: no icon font at all (§2).

**What arrives and what leaves, inside `@unicron/ui`.** No package gained a direct dependency.

| Root | Arrives | Leaves |
|---|---|---|
| all four | `@fontsource-variable/geist` 5.3.0, `@fontsource-variable/geist-mono` 5.3.0 (the fonts, SIL OFL 1.1), `qrcode-generator` 2.0.4 (MIT, for unicron's `QrCode`) | `@fontsource-variable/inter` 5.3.0, `material-symbols` 0.48 |
| `extension` only | `react-resizable-panels` 4.14.8 (MIT; unicron has depended on it since 0.17, which the extension skipped) | |

`cmdk`, `next-themes`, `sonner`, `tailwind-merge` and `tw-animate-css` were already in every lockfile. `@unicron/ui` also carries a copy of sonner 2.0.8's module (`vendor/sonner.js`, MIT) with one step taken out (§5). `NOTICE` names IBM Carbon's icons (Apache-2.0) and Geist and Geist Mono (SIL OFL 1.1) as bundled.

**Supply-chain checks with a `file:` dependency.**

| Check | Result |
|---|---|
| `pnpm install --frozen-lockfile` (CI, every root) | Accepts the `file:` tarballs: the lockfile names them by the same relative path. Run here for `apps/web` and `design/capture`; for `apps/desktop` and `extension` as `--lockfile-only`, since this container cannot fetch `@snds/dsa` (as before). |
| `onlyBuiltDependencies` | Unchanged. Neither tarball has an install script; `apps/web` and `design/capture` still allow none. |
| The extension's reproducible zip (`reproducible.yml`) | Built twice here with `SOURCE_DATE_EPOCH` set: the same sha256 both times (§7). |
| The web vault's `dist` (`reproducible.yml`) | Built twice here: the same hash for every file (§7). |
| `cargo deny`, `cargo vet` | Untouched: no Rust change. |

No check refused the `file:` dependency, and none was changed.

## 2. Icons

**What went.** Until 1.0 unicron drew an icon as a ligature in the Material Symbols font, which Apple's WebKit did not shape inside SVG text, so Lockdown patched `Icon` to draw by codepoint and worked the codepoints out at build time (doc 38 §2, D452). unicron 1.0 draws IBM Carbon's own art as inline SVG and ships no icon font, so all of that is deleted:

- the `iconCodepoints` plugin in the three Vite configs, and `apps/desktop/scripts/icon-codepoints.ts` with its entry in `apps/desktop/tsconfig.node.json`;
- `apps/desktop/src/app/icon-codepoints.ts` and its import in the three entries (`apps/desktop/src/main.tsx`, `apps/web/src/main.tsx`, `design/capture/src/app-main.tsx`);
- `virtual-icon-codepoints.d.ts` and its `include` in `apps/web/tsconfig.app.json` and `design/capture/tsconfig.app.json`;
- `icon-codepoints.test.tsx`: its 3 tests go with the code they tested (the font's codepoints, the patched `Icon`). `template-model.test.tsx` (4 tests) and the rewritten engine check hold the new path instead.

**The codemod.** `unicron-codemod icons` ran once over `apps/desktop/src` (153 names in 58 files), `extension/src/popup` (16 in 1), `design/capture/src` (1) and `apps/web/src` (0). `tsc` then found one name the codemod could not see, in a tuple typed in another file (`apps/web/src/app/web-app.tsx`, `group` → `user--multiple`). The names it wrote for the sidebar are the ones the design's frames show for those places: All items `folder`, logins `password`, Passkeys `locked`, Codes `time`, cards `wallet`, notes `document`, Settings `settings`. One name was changed by hand: Expiring is `event--schedule`, because the map gave it `time`, the same glyph as Codes one row above. Lock and Passkeys both draw `locked` until the shell is rebuilt; each has its word.

Three stars took the filled glyph where `fill` marked a state (a favourite): `star--filled`.

**Names that arrive as data.** A template's icon is a name the core sends (`crates/vault/src/template.rs`), or one a person's own template stored, and both name Material Symbols. No Rust changed. `templateIcon(name, fallback)` in `apps/desktop/src/app/template-model.ts` translates the name once, where the "New item" menu draws it: Material first (`iconFromMaterial`), because a few words are names in both sets with different drawings (`category`, `badge`); then a Carbon name as it is; then the fallback. Every one of the 31 names the Rust file uses today has a Carbon drawing. The fixtures (`src/test/templates-fixture.ts`, the harness's `fixture/commands.ts`) keep the core's Material names, so the tests and the harness run the real translation. The palette's and the list's kind icons are a local table keyed by kind, not names as data; the codemod rewrote it.

**The engine check** (`design/capture/scripts/engine-check.ts`, CI job `engines`) is rewritten, not deleted: it is the WebKit guard. In each engine, on each of its screens:

- every `svg[data-slot=icon]` has the view box `0 0 32 32` and at least one shape in it;
- none is the empty box unicron draws for a name it does not know (`data-icon-missing`): a name that arrived as data and was not translated fails here;
- every icon the engine lays out has a box on the page and a drawing that are not empty;
- Geist and Geist Mono each have a face the engine loaded;
- the two pages that print alone still print alone (unchanged).

Its screens gained the "New item" menu, whose icons are the core's templates read from the Rust source. Here: 849 icons over 9 screens in Chromium and in Firefox, none wrong. WebKit runs in CI.

## 3. The theme, and the Theme choice

**The app's theme is set once, in HTML.** `data-theme="lockdown"` is on `<html>` in `apps/desktop/index.html`, `apps/web/index.html`, the harness's `index.html` and `frame.html`, the popup's `popup.html` and the extension's `static/frame.html`. Nothing sets it from script. It picks Lockdown's identity green, its live blue and its tinted neutrals.

**The ground** is what it was: the `dark` class is graphite, no class is stone.

**The choice** (cross-app decisions §5; the undrawn brief's T6). Settings → Appearance, on the desktop and in the web vault, has one row before Density:

| Row | Chips | Tip (also the chip's description, so a screen reader hears it) |
|---|---|---|
| Theme | System | "Follows your device's light or dark setting." |
| | Stone | "Light" |
| | Graphite | "Dark" |

The words "light" and "dark" appear nowhere else. Picking a chip changes the whole app at once and says nothing more: there is nothing to save and nothing to fail, so no toast and no error line. The row is unicron's `FieldRow` with a single-choice `FilterChipGroup` (one of many: the arrow keys move and choose), in `features/settings/theme-setting.tsx`, shared by both settings screens.

**Where it is kept: on the device, not in the vault's settings** (proposed D700). It says nothing about the vault, and the unlock, sign-in and first-run screens must already be on the right ground before any vault is open.

| Surface | Kept in | Applied by |
|---|---|---|
| Desktop app, and its quick-access window | next-themes' own store (local storage key `theme`: `system`, `light`, `dark`; Stone is its light, Graphite its dark) | next-themes' provider, as before |
| Web vault | this browser's local storage, key `lockdown.theme` (`stone` or `graphite`; System stores nothing) | `apps/web/src/theme.ts`, called by `main.tsx` before the first render, from the page's own bundle: no inline script, no `<style>` element. A change in another tab arrives through the `storage` event. With storage off it follows the browser, and a choice holds until the page reloads. |
| Extension popup and in-page frames | nowhere | the browser's `prefers-color-scheme`, as before: a frame cannot be given a class |

The web vault's settings "hold for this tab; a new sign-in starts from the defaults". The theme is the one exception: it outlives a sign-out, because it is the browser's, not the account's.

## 4. Roles

### 4.1 The app-extension layer

`apps/desktop/src/styles/app-extension.css` is rewritten onto 1.0's roles. Aliases only: every role points at a core role, never at a raw value and never at `--g-neutral-*` or an accent ramp, which are not themed. A role that only points at core roles is the same on both grounds, so each is declared once, on `:root, .light, .dark, [data-theme]` (the core's own example). One role differs by ground, `--attention-text`, and has the file's only `.dark` rule. The table is in doc 10 §4.

| Role | Was | Is | |
|---|---|---|---|
| `--surface-chrome`, `--surface-canvas`, `--surface-locked` | neutral 50 / 0 / 100 (900 / 950 / 900 dark) | `--surface-ground` | The panes sit on the ground, set apart by space and a hairline, as 1.0's sidebar does. |
| `--surface-selected` | neutral 100 / 800 | `--primary-soft` | The selected row, never a solid. |
| `--danger` | `--destructive` | `--critical-text` | |
| `--secure`, `--secure-foreground`, `--secure-text` | green 500 / white / green 600 (300 dark) | `--text-body`, `--surface-ground`, `--text-body` | **Legacy, no new use.** The primary is green now; the word carries "verified". |
| `--attention-text` | the core's `--attention-foreground` | `--attention-foreground` on stone, `--warning` on graphite | **Legacy, no new use.** The same two inks as before: amber 600 on stone, amber 300 on graphite. On graphite the core's attention ink is now the needs-you yellow, which only says "this waits on you", so graphite takes the core's warning ink. On stone the core's `--warning` is a fill (amber 550): as words on the ground it is 4.0:1, under the 4.5:1 floor for text, and axe-core reported it on every stone picture with the sidebar. So stone keeps the attention ink, 6.9:1 (gap U7 in §8). |
| `--area-codes`, `--area-codes-foreground`, `--area-codes-text` | blue 500 / white / blue 600 (300 dark) | `--secondary-live`, `--surface-ground`, `--secondary-live` | **Legacy, no new use.** The live colour is for a code counting down, not a nav entry. |
| `--sidebar` | re-pointed at `--surface-chrome` | not declared | The core's `--sidebar` is `--surface-ground` already. |

The density knobs (`--c-row-height` and the rest, D92) and both print rules are unchanged.

The three retiring roles keep their names so that no class silently stops drawing (a Tailwind class for a colour that no longer exists draws nothing and says nothing). What still names them: `text-attention-text` in 15 places (expiring, unverified, a conflict copy), `text-secure-text` once (Status's facts), `text-area-codes-text` once (the Codes sidebar entry). Each goes when its screen is rebuilt (proposed D701).

### 4.2 `primary` is the app's green now

Before 1.0 `primary` was the strongest ink. Five places used it that way and now name the ink:

| Where | Was | Is |
|---|---|---|
| `features/server/server-picker.tsx`, `features/unlock/join.tsx` (a native radio's tick) | `accent-primary` | `accent-foreground` |
| `features/bridges/incidents.tsx` (the first open step's edge) | `border-primary` | `border-foreground` |
| `devtools/demo-bar.tsx` (the demo bar) | `bg-primary text-primary-foreground` | `bg-foreground text-background` |

The master-password strength meter (`features/unlock/master-password-fields.tsx`) drew weak, fair and strong in `bg-destructive`, `bg-warning` and `bg-secure`. It now reads the three fills of unicron's own meter (`--c-secret-field-meter-weak`, `-fair`, `-strong`), so the two meters a person sees read alike, and `--secure` has no fill left to draw.

### 4.3 Dialog actions: unicron keeps the words now

`LabelledActions` and `styles/dialog-actions.css` exist because unicron's `DialogAction` drew a round icon button below `sm` (review R1; brief §6 rule 5). Checked against 1.0:

- `DialogAction` shows its words at every width ("The words always show"), with an optional 16 px glyph before them.
- The crown-jewel gate and the approval sheet no longer draw `DialogAction`s. Each draws its own buttons, with words, at every width.
- So `dialog-actions.css` matches nothing any more: its selectors look for a `[data-kind]` button directly inside a dialog footer or the approval sheet's row, and no unicron component draws that. It does nothing and is left in place.
- `LabelledActions` still differs from unicron's footer in one way: below `sm` it stacks the actions full width, the primary last. unicron's footer is one row at every width.

Both files stay until the fold slice, which decides between the stack and unicron's row and then deletes them. `labelled-actions.test.tsx` now holds unicron to R1 directly (§7).

### 4.4 Layout that 1.0 broke, and the fix

- **A code on an item collapsed.** 1.0's field-layout `CodeDisplay` measures its own box (a container query), and the detail pane wrapped it in a box that shrank to its content (`items-start`), so it shrank to nothing and its seconds drew over the digits. The wrapper now lets the code take the block's width (`features/vault/detail-pane.tsx`).
- **A list item inside a list item.** From 1.0 an `Item` inside an `ItemGroup` takes the list-item role itself. A detail block and the "opened" panel already wrap each child in one (`ListItems`, review R7), so a row would have been an item inside an item. Those two lists are plain `role="list"` elements now, not `ItemGroup`s; the structure a screen reader hears is what it was.

## 5. Fonts, and the web vault's policy

- Geist and Geist Mono come with `@unicron/ui/styles`, from `@fontsource-variable/geist` and `-geist-mono`. Inter and the Material Symbols font are gone.
- The web build keeps `assetsInlineLimit: 0`, so the 11 font files are files under `/vault/assets/` and pass `font-src 'self'`. The built sheet has no `data:` URL. Nothing loads from another origin.
- **Subresource integrity** covers what it covered: the script and the stylesheet `index.html` names. A font has no integrity attribute to carry (CSS has none); the stylesheet, which is checked, names each font by a file name that carries its content hash. That is how Inter shipped too.
- **Toasts.** sonner's module writes its rules into a `<style>` element as it loads, which `style-src 'self'` refuses. Until now the web build cut that step out of sonner and `main.tsx` imported sonner's sheet. From 1.0 unicron does both itself: its `Toaster` loads its own copy of sonner 2.0.8 with the step taken out, and `@unicron/ui/styles` carries sonner's rules as a file. Its `Toaster` no longer hears sonner's own `toast`. So the 12 files that raised a toast import `toast` from `@unicron/ui` (the calls are unchanged: `toast()`, `toast.error()`, `toast.dismiss()`), `main.tsx` no longer imports sonner's sheet, and no source imports `sonner`.
- **`sonnerWithoutInlineStyle()`** in `apps/web/vite.config.ts` still matches sonner 2.0.8 (the version did not move) and now has nothing to do. It stays as the guard: if a source imports `sonner` again the build still cuts the step out, and a sonner whose code no longer matches fails the build rather than the page.
- **The proof** is the web vault's end-to-end run against the real server and its real policy: 13 of 13. Its first test watches the console for anything the policy or an integrity check refuses and every origin a request goes to, through sign-in, the vault, Settings and the Theme choice, a sign-out and a reload.
- The desktop's policy allows inline styles, so next-themes and its `disableTransitionOnChange` stay as they are there.

## 6. The extension

The extension was on `@unicron/ui` 0.12 and tokens 0.6, twelve releases behind the app, because moving it changes what people see. That change is the point of the redesign, so it moved in this slice (proposed D705). The harness compiles `extension/src/popup` with its own unicron, so the popup had to compile on 1.0 the moment the harness did.

- **Popup.** `popup.css` still imports `@unicron/ui/styles`; `popup.html` sets `data-theme="lockdown"`; `popup.tsx` changed only where the codemod renamed icons. No `className` was added: the popup's ratchet counts stay 0. It follows the browser's scheme. The fonts are files in the extension's own package (`popup/*.woff2`), which its policy allows.
- **The frames' token sheet.** The fill menu, the save prompt and the passkey sheet are plain HTML and CSS and load `static/tokens.css`, which `scripts/tokens-css.mjs` generates from the installed `@unicron/tokens`. From 1.0 a role is a chain, and the generator follows it:

  ```
  --popover              what static/ui.css aliases (the shadcn contract)
  → --surface-raised     the canonical role
  → --g-theme-stone-raised / --g-theme-graphite-raised    the theme's slot for each ground
  → a hex value          under [data-theme="lockdown"]
  ```

  The sheet carries the lockdown theme's slots under `[data-theme="lockdown"]`, the shared critical colours, the roles on stone, and the same roles on graphite under `prefers-color-scheme: dark`. Every declaration is unicron's, copied as written; the script throws if a role or a slot it needs is missing. `static/frame.html` sets `data-theme="lockdown"`.
- **`frame.ts` and `static/ui.css` are untouched.** Every in-page rule holds as it was: opaque frames, the 400 ms arming, untrusted events ignored, two menu heights, the extension's policy. The frames keep the system font and their 10 px corners until they are redesigned.
- **unicron ships a plain sheet of its own now** (`@unicron/tokens/plain.css`, asked for in brief §11.1). The frames move to it when they are redesigned; this slice keeps the generated sheet, so only the values moved.

**What a person sees change, before the redesign reaches these surfaces:**

| Surface | Change |
|---|---|
| Popup | The whole 1.0 look: Geist, Carbon icons, square corners, mono caps on buttons, the green primary. Button labels are wider, so a long title is cut sooner (the first result's title, beside "From quick access", most of all). |
| Frames | Lockdown's tinted surfaces and inks, and a green primary button where it was ink. The secondary button's edge and the search field's edge are a hairline (`--input` is `line-rule` in 1.0) where they were a 3:1 grey. The words on those controls are unchanged, and the frame's own edge and shadow are what they were. |

## 7. Tests and what is not proven

**Updated for 1.0, none weaker.**

| Test | Was | Is |
|---|---|---|
| `features/security/pq-status-line.test.tsx` (3) | the icon's ligature in the line's text (`verified_user`, `hourglass_top`, `help`) | the icon's `data-icon-name` (`security`, `hourglass`, `help`); a device link's text is its name alone |
| `features/codes/codes-view.test.tsx` (1) | the HOTP counter reads `#3`, then `#4` | unicron's words: `Counter 3`, then `Counter 4` |
| `features/status/status-view.test.tsx` (1) | "Sync now" carries `border-border` | it is the outline variant (`data-variant`) with 1.0's inset ink edge; "Check now" is still the one `bg-primary` button |
| `components/labelled-actions.test.tsx` (2) | unicron's own actions get their words back from Lockdown's sheet | unicron's own actions show their words by themselves: the words are text in the button, nothing between them and the button hides them (by class and by computed style), and the button is not the old round one |
| `features/vault/detail-pane.test.tsx` (1) | unchanged | unchanged: it failed until the list fix in §4.4 |
| `apps/web/e2e/device-link.spec.ts`, `finish.spec.ts` (1 line each) | the gate's field found by the label "Account password" | by that label exactly: unicron 1.0's gate field has a "Show Account password" button beside it, which the loose match also found |
| `extension/test/extension.spec.ts` (review R16, 1) | every word the popup draws, but the badges, is 13 px or more | every word is 13 px or more unless it is in unicron's label role (its overline: Geist Mono, capitals, 11 px), which a button's label is set in from 1.0, as a badge always was; nothing is under 11 px; a result's title and line are 13 px or more, checked by themselves |

**New.**

| Test | Holds |
|---|---|
| `app/template-model.test.tsx` (4) | every icon name the core's templates use translates to a Carbon name unicron draws, none by the fallback; names in both sets read as Material; a Carbon name is kept; an empty or unknown name falls back |
| `features/settings/theme-setting.test.tsx` (4) | one row, three chips, one on; "light" and "dark" only in the descriptions; a press and the arrow keys choose; on the desktop the choice changes the root's class at once, is kept by next-themes, and a new start reads it |
| `apps/web/src/theme.test.ts` (8) | System follows the browser and stores nothing; Stone and Graphite hold; a reload reads the choice at the start; an unknown value reads as System; another tab's change arrives; storage off; no script and no style element is written |
| `apps/web/src/features/web-settings-view.test.tsx` (2) | the web vault's row: a chip changes the page at once, keeps the choice in this browser, and raises no alert |
| `apps/web/e2e/web-vault.spec.ts` (added to its first test) | against the real server and its policy: the page starts on stone with System on; Graphite changes the root at once; the choice outlives the sign-out and a reload, applied at the start; the policy refuses nothing and nothing leaves the origin |

**Gone:** `icon-codepoints.test.tsx` (3), with the code it tested.

**Counts.** Desktop: 285 − 3 + 8 = 290. Web vault: 50 + 10 = 60, and 13 of 13 end to end. Extension: 80 of 80 (tokens, a11y, clickjacking, red team, package). Engine check: Chromium and Firefox.

**Reproducible.** The extension's two store packages built twice from a clean `dist` with `SOURCE_DATE_EPOCH=1700000000`: the same sha256 each time. The web vault's `dist` built twice: the same hash for every file.

**Pictures.** `pnpm shots`, strict: all 1,050 pictures of the 283 states, 0 failures. Every picture changed, as it should; no state was lost. Light is stone and dark is graphite.

**axe-core over the pictures** (`axe.json` beside them; nothing gates on it). Before, on 0.24: 64 findings. After: 140.

| Rule | Before | After | |
|---|---|---|---|
| `color-contrast` | 24 | 54 | 24 are the same line as before (an item's description in the recovery-codes and wallet-gate dialogs). 26 are the label ink on a checked radio card's soft fill, stone only (gap U8 in §8). 4 are single places. The first run of this slice had 267: the warning fill as words on stone, fixed in §4.1. |
| `label-content-name-mismatch` | 0 | 52 | unicron's `CodeDisplay` copy button (gap U9) |
| `region` | 34 | 30 | |
| `aria-hidden-focus` | 0 | 2 | unicron's menu (gap U10) |
| `scrollable-region-focusable` | 2 | 2 | |
| `aria-required-children` | 4 | 0 | |

**Not proven here.**

- `pnpm ds:ratchet` (snds/dsa cannot be fetched in the build container; CI runs it). Counted by hand with a script of the same rule: `className` on a unicron component is 4 on the desktop (the same four) and 0 in the popup; look-alikes 0 and 0.
- WebKit. The engine check ran in Chromium and Firefox; CI's `engines` job runs WebKit on Linux and macOS. Inline SVG needs no shaping, so the defect doc 38 fixed has no way back, but that is an argument, not a run.
- The Tauri shell (no WebKitGTK here) and the Rust workspace's clippy, tests, deny and vet. No Rust changed.
- The real desktop app on a Mac, on both grounds, and the extension in a real browser profile.
- A frozen install of the `file:` tarballs on a clean runner. The lockfiles' hashes match the files; CI's first run is the proof.

## 8. Gaps

1. **Nothing is redesigned.** Every screen has its old layout in the new look. Some of it reads rough until its slice lands: page titles are larger and can wrap in the list's header; buttons are taller, in mono caps; rows of bordered boxes sit edge to edge; the popup cuts long titles sooner. Nothing is unreachable and nothing overlaps.
2. **Lock and Passkeys share a glyph** (`locked`) in the sidebar, each with its word. The shell slice rebuilds the sidebar.
3. **The current sidebar entry is an ink outline**, 1.0's secondary button, not the soft primary fill with its edge. It is a `Button` today; the shell slice makes it a `NavItem`.
4. **The Codes sidebar entry is in the live blue** and "Needs attention" in amber, through the legacy roles (§4.1).
5. **`LabelledActions` and `dialog-actions.css`** wait for the fold slice (§4.3).
6. **`sonner` is still a direct dependency** of the three app roots, though no source imports it. It goes with the fold slice, with its `dedupe` and `paths` entries.
7. **The extension's source archive for store review** (`release.yml`, "Source archive for review") lists what it packs by name: `extension crates Cargo.toml Cargo.lock rust-toolchain.toml LICENSE NOTICE`. It has never carried `patches/`, which the extension's install needed, and it does not carry `vendor/`, which it needs now. Not changed here (the path would have to come out again at the swap). If an extension release is cut before the swap, add `vendor` to that list.
8. **The frames' secondary edge** is a hairline until the frames are redesigned (§6).
9. **The review harness's own chrome** (`design/capture/src/styles.css`, the start list and the bar) still names Inter, which no longer ships; it falls back to the system font. It is not product UI.
10. **Stale comments** in `apps/desktop/src/core/types.ts` still say a template's icon is "a Material Symbols icon name". It is: that is what the core sends. `templateIcon` is where it stops being one.

**unicron host-contract gaps** (U1 to U10):

1. **A `FilterChip` cannot be a tooltip's own trigger.** As the `render` of `TooltipTrigger` it takes the trigger's `data-slot`, and `FilterChipGroup` finds its chips by `data-slot="filter-chip"`, so the arrow keys stop choosing. Worked around: the tip hangs on a plain box around the chip, and the chip carries the tip as `aria-description`. Asked of unicron: a `tooltip` prop on `FilterChip`, or a group that finds its chips by registration (it already keeps that list), or a trigger that does not overwrite the slot of a unicron part it renders.
2. **A part that measures its own box collapses in a shrink-to-fit parent** (§4.4). `FieldRow`, `SecretField` and the field-layout `CodeDisplay` are size containers. unicron's migration guide does not say so. Asked: a line in the guide, or a minimum inline size on those parts.
3. **`ItemGroup` and `Item` took list semantics** (what review R7 asked for), with no line in the migration guide. An app that already wrapped rows in list items gets an item inside an item (§4.4). Asked: a line in the guide.
4. **The migration guide's short version does not say that the `Toaster` stopped hearing sonner's `toast`** until its section 9. An app that only bumps and compiles loses every toast silently: nothing fails to type-check. Asked: say it in step 1, and have the `Toaster` warn when a toast is raised that it will never show (it warns today only when it finds sonner's `<style>` tag).
5. **The codemod does not see an icon name inside a tuple whose type is declared in another file** (`ShellScreen`). `tsc` caught it; recorded so the mapping's notes can say it.
6. **No role for a mark that means "verified / passed".** The brief settles it with a neutral Badge and the word (§4.1); recorded as the answer, not as an ask.
7. **The core's warning as words is under the text floor on stone.** `--warning` on stone is amber 550, a fill with a white label. unicron draws a warning Badge's word and its `text-warning` in it: on Lockdown's stone ground (`#EDF1EE`) that is 4.0:1, where text needs 4.5:1. Lockdown's legacy `--attention-text` therefore stays on the attention ink on stone (§4.1). Asked: a warning ink for words on stone (amber 600 is 6.9:1), before the rebuilt screens draw warning Badges.
8. **The label ink on `primary-soft` is under the text floor on stone.** Lockdown's stone label ink (`#656A66`) on its soft primary (`#CCE8D2`) is 4.2:1, and its primary text (`#007839`) on the same fill is 4.3:1. The selected row is `primary-soft` (§4.1), so a second line in label ink on a selected surface fails. axe-core reports it on the two radio cards that fill when checked (the server picker, the ways to join): 25 pictures. Not changed here: those two screens are rebuilt. Asked: a darker label ink on the soft fill, or a lighter soft fill, in the lockdown theme.
9. **`CodeDisplay`'s copy button has a name that does not hold its visible text** (axe-core's `label-content-name-mismatch`, 52 pictures, wherever a code is drawn). Inside unicron; nothing in Lockdown names that button.
10. **A menu's focus guards are focusable inside an `aria-hidden` tree** (axe-core's `aria-hidden-focus`, the "New item" menu at the phone width, 2 pictures). Inside unicron's menu (Base UI's guards).

## 9. Proposed decisions

| # | Decision | Status |
|---|---|---|
| D700 | **The app is drawn in Lockdown's theme, and the ground is the person's choice, kept on the device.** Every page sets `data-theme="lockdown"` in its HTML. Settings → Appearance → Theme offers System, Stone and Graphite; "light" and "dark" appear only in the chips' tips. System follows the device. The choice is not a setting of the vault: the desktop keeps it in next-themes' store, the web vault in this browser's local storage (`lockdown.theme`), applied before the first render with no inline script, and it outlives a sign-out. The extension follows the browser. Plain version: you can pick the light or the dark look, and the app remembers it on this device. | Proposed; **flagged for Sean** (a new setting a person sees; the chip words are cross-app question 8) |
| D701 | **Three app roles retire: `--secure`, `--area-codes`, `--attention-text`.** The primary is green, so "verified" has no hue of its own: the word carries it, in body ink, and a new one is a neutral Badge with the word. Blue is the app's live colour, for a code counting down, not for the Codes entry. A needs-attention word is never the needs-you yellow: the legacy role is amber 600 on stone and amber 300 on graphite, as before, and a new one is a warning Badge. The three names stay as aliases, marked "legacy, no new use", until each screen that names them is rebuilt. The surfaces (`chrome`, `canvas`, `locked`) are the ground; a selected row is `primary-soft`. Plain version: green now means Lockdown, not "verified"; the word says verified. | Proposed; **flagged for Sean** (the app spec's questions 3 and 4: what green and blue mean) |
| D702 | **Icons are IBM Carbon, drawn as inline SVG; D452 is superseded.** The codepoint patch, its Vite plugin, the virtual module and their test are deleted. A name that arrives as data (a template's icon, from the core or a person's own template) is translated where it is read, Material first; the core's names do not change. The engine check (D451 stands) now fails when an icon has no drawing, the wrong view box or an empty box, or when Geist or Geist Mono did not load. Plain version: the icons are a new set, and they no longer depend on a font the Mac's browser engine drew wrong. | Proposed; accepted under D38 (structure, not security) |
| D703 | **Until unicron 1.0 is released, its two packages are vendored tarballs at `vendor/unicron/`,** one copy for the four pnpm roots, each named by a relative `file:` path, with their sha512 in every lockfile. This departs from pinned release URLs (and from the app contract's "the UI package's folder", which Lockdown does not have). Swap to the `@unicron/ui@1.0.0` and `@unicron/tokens@1.0.0` release URLs when Unicron 1.0 is released, and delete `vendor/unicron/`. Plain version: the design system's files are checked in for now, because the version this needs is not published yet. | Proposed; audit item (a dependency's source) |
| D704 | **Toasts come from unicron; no source imports sonner.** unicron's `Toaster` loads its own copy of sonner without the `<style>` step and carries sonner's rules as a file, so the web vault's policy holds with no special step. The web build's own step stays as a guard against a new import. Plain version: nothing a person sees; the notices keep working under the strict policy. | Proposed; accepted under D38 (structure, not security) |
| D705 | **The extension moves from unicron 0.12 to the 1.0 pre-release with the app.** The popup takes the new look as it stands; the in-page frames take Lockdown's tinted colours through the generated token sheet, which now follows 1.0's chain of roles to the lockdown theme's values. `frame.ts`, `ui.css` and every in-page rule are untouched. The frames' secondary edge is a hairline until they are redesigned. Plain version: the browser extension changes its look along with the app. | Proposed; **flagged for Sean** (what a person sees in the popup and in pages, D38) |
| D706 | **A spinner says "Loading" to a screen reader** (unicron's change at 0.23, kept at 1.0). One inside a status that already says what is happening is hidden, so the status is heard once. A busy button keeps its words. Plain version: a screen reader hears that something is loading. | Proposed; recorded (the groundwork's change, given a number here) |
| D707 | **Recorded gaps:**<br>• `LabelledActions` and `dialog-actions.css` wait for the fold;<br>• `sonner` as a direct dependency;<br>• the review source archive does not carry `vendor/`;<br>• the frames' hairline secondary edge;<br>• every screen keeps its old layout until its slice.<br>Plain version: what this slice leaves for later, written down. | Proposed; recorded gap |
