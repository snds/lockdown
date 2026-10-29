# 38: Mac fixes: icons in WebKit, printing, and the kit as a file

_Slice: `slice/mac-fixes`, built 2026-10-29. Three defects from Sean's first run of the real macOS build (Tauri 2, so the UI runs in WKWebView). Until this slice, every check of the UI ran in Chromium, which is why none of them was caught. Builds on D38 (who decides security-model details), D62 (wire shapes only grow), D69 (the recovery kit's page), D94 (first run stays succinct), D95 (proof moments), D151 (what counts as a use), D239 (the Wi-Fi guest card), D274 (the emergency sheet), D375 (the flag review). Proposed decisions D451–D459 (§8)._

## For future agent

- **TL;DR:** (1) **Icons.** Apple's WebKit does not shape ligatures inside SVG `<text>`, and unicron's `Icon` draws each icon as its ligature (the name as text in an `<svg>`), so the Mac app drew every icon as blank letters. A new CI job, `engines`, proved it: on `macos-latest`, Playwright's WebKit measured all 619 icons as wide as their names; on Linux, WebKit and Chromium drew them as one glyph each. The fix is a pnpm patch of `@unicron/ui` (`patches/@unicron__ui@0.12.0.patch`): `Icon` draws the glyph by its private-use **codepoint** when the app has handed it one (`setIconCodepoints`), and as its ligature otherwise. A Vite plugin (`apps/desktop/scripts/icon-codepoints.ts`) works the codepoints out at build time from the font that ships, for the names the sources use. The `<svg>` wrapper and the variable axes are unchanged. (2) **Print.** `window.print()` does nothing in WKWebView under Tauri. Printing goes through a paper client. The desktop's shell opens the webview's own print panel (`print_page`, `Webview::print`), and the web vault keeps `window.print()`. This covers the kit page, the emergency sheet and the Wi-Fi card. (3) **Save as PDF…** sits beside Print on the kit page. The core lays out the page it was shown as one A4 PDF (`lockdown_app::kit_pdf`, pure Rust through `pdf-writer`, JetBrains Mono embedded). On the desktop the core writes it only where the person picks in the native save dialog (`kit_save_pdf`). The web vault downloads the same bytes (`kit_pdf`).
- **Read with:** `design/capture/scripts/engine-check.ts` and the `engines` job in `.github/workflows/ci.yml`; `patches/@unicron__ui@0.12.0.patch`, `apps/desktop/scripts/icon-codepoints.ts`, `apps/desktop/src/app/icon-codepoints.ts`; `apps/desktop/src/features/paper/paper-client.ts` (the seam), `apps/desktop/src/app/tauri-paper.ts`, `apps/web/src/core/web-paper-client.ts`, `apps/desktop/src-tauri/src/paper.rs`; `crates/app/src/kit_pdf.rs`, `App::kit_pdf` and `App::kit_pdf_save` in `crates/app/src/lib.rs`, `crates/web/src/commands/paper.rs`; `apps/desktop/src/features/status/once-page.tsx`, `features/vault/wifi-card.tsx`, `features/bridges/emergency-sheet.tsx`, and the print rule at the end of `styles/app-extension.css`.
- **Do not:**
  - Import `@tauri-apps/*` in a feature; printing and the kit's file go through `usePaper` / `usePrinting`.
  - Call `window.print()` from desktop code.
  - Let the kit's PDF reach the desktop webview, or write it anywhere but the place the person picked: no temp file, no copy in app data.
  - Make `kit_pdf` lay out anything but a kit page.
  - Drop the patch before unicron ships the change in §2.4.
  - Add an icon name built by string concatenation. The build finds names as string literals; the engine check fails on any icon still drawn by name.

## 1. The shape

| Piece | Where | What it does |
|---|---|---|
| Engine check | `design/capture/scripts/engine-check.ts`, CI job `engines` | Opens the review harness's build of the real desktop screens in each engine. It fails when any icon is not one glyph or is drawn by name, or when a page that prints alone would print the app behind it. It keeps a screenshot of each main screen. It runs twice: `ubuntu-24.04` (WebKit, Chromium, Firefox) and `macos-latest` (WebKit, Chromium). |
| Icon patch | `patches/@unicron__ui@0.12.0.patch`, `patchedDependencies` in the `pnpm-workspace.yaml` of `apps/desktop`, `apps/web`, `design/capture` and `extension` | Adds `setIconCodepoints` to `@unicron/ui/icon`. `Icon` draws `String.fromCodePoint(cp)` when it knows the name's codepoint, and the name otherwise. The `<text>` carries `data-icon-name`. |
| Codepoints | `apps/desktop/scripts/icon-codepoints.ts` (Vite plugin, used by the desktop, web vault and harness configs), `apps/desktop/src/app/icon-codepoints.ts` (imported once by each entry) | Serves `virtual:icon-codepoints`: `{ name: codepoint }` for every string literal in the sources that names an icon in the shipped font. That is 153 names today, out of the font's 4,301. |
| Paper seam | `features/paper/paper-client.ts` | `print(): "done" \| "open"`, and `saveKitPdf?(lines, page): "saved" \| "cancelled"`. `usePrinting` keeps `printing` true until a native panel has given the window back. With no provider: `window.print()` and no file. |
| Desktop paper | `app/tauri-paper.ts`, `src-tauri/src/paper.rs` | `print_page` calls `WebviewWindow::print`. `kit_save_pdf` takes the page as raw bytes, the core checks it is a kit page, the native save dialog picks the place, and the core lays it out and writes it there. |
| Web paper | `apps/web/src/core/web-paper-client.ts`, `crates/web/src/commands/paper.rs` | `window.print()`. `kit_pdf` returns the same bytes, which the page hands to the browser as a download and then wipes. |
| Kit PDF | `crates/app/src/kit_pdf.rs` | Parses `render_kit`'s page (anything else is refused) and lays it out on one A4 page. Every buffer that holds the kit is wiped when it drops. |

## 2. Icons in WebKit

### 2.1 The evidence

The brief's hypothesis was that WebKit does not shape ligatures in SVG `<text>`. It was tested before anything was fixed. The `engines` job measures each icon's `<text>` with `getComputedTextLength()`, in the SVG's own units (an icon is set at 24 of them, so one glyph is about 24 wide). It also checks that the Material Symbols face reports `loaded`.

| Run | Engine | Result |
|---|---|---|
| Job 123618204291 (`ubuntu-24.04`, commit `cefb375`) | WebKit 26.0 (Playwright's Linux build, HarfBuzz shaping) | 619 icons over 8 screens, **all one glyph**, 24.0 wide. The job failed only on a false alarm: on a screen with no icons the font had not loaded yet, because engines load a face only when something uses it. The check now ignores such screens. |
| Same job | Chromium 141 | 619 icons, all one glyph. |
| Job 123625950471 (`macos-latest`, commit `c2a3cc8`) | WebKit 26.0 (Apple's port, Core Text shaping, as WKWebView) | **619 of 619 icons not one glyph**, each exactly 24 units per letter of its name: `inventory_2` 264 wide, `star` 96, `passkey` 168, `close` 120. The font reports `loaded`. The screenshots show every icon blank: the font's letter glyphs are empty and one em wide. |
| Same job | Chromium 141 on macOS | 619 icons, all one glyph. |

So the cause is not the CSP, the font's MIME type or font loading. The font loads under `font-src 'self'` and its letters are measured from it. The cause is that Apple's WebKit text path leaves `liga` unapplied inside SVG `<text>`. WebKit on Linux, which shapes with HarfBuzz, does apply it, so the Linux app (WebKitGTK) should not show the defect.

### 2.2 The fix

`Icon` keeps its `<svg viewBox="0 0 24 24">` and its `<text>`, so every shadcn `svg` selector and size rule still applies. When it knows the icon's private-use codepoint it draws that one character instead of the name. A single codepoint needs no shaping: every engine draws the glyph straight from the font's `cmap`. A name without a codepoint is still drawn as its ligature, so nothing renders worse than before in any engine.

The codepoints come from the font itself, at build time. The plugin:

1. Finds the font as installed (`@unicron/ui` → `material-symbols/material-symbols-rounded.woff2`).
2. Unpacks the WOFF2 with Node's own Brotli. Only `cmap` and `GSUB` are read; WOFF2 never transforms them.
3. Takes every ligature from GSUB (lookup type 4, and through type 7 extensions).
4. Maps each ligature's component glyphs back to lower-case letters and its result glyph to a private-use codepoint through `cmap`. When several codepoints map to one glyph (`close` and `clear`, say), it takes the lowest, and it draws the same glyph either way.
5. Keeps the names that appear as string literals in the sources given: the desktop's, the web vault's, the harness's, the extension popup's, and unicron's own `dist`, so names used inside unicron's components, such as a dialog's `close`, are kept too.

The desktop's, the web vault's and the harness's entries import `@/app/icon-codepoints`, which hands the map to `setIconCodepoints` once. Tests do not import an entry, so the component tests still see icon names, as before.

### 2.3 What is lost

Nothing visible. The codepoint names the same glyph the ligature produced, so `FILL`, `wght`, `GRAD` and `opsz` still reach it through `font-variation-settings`, and the animated fill on active nav items still runs. Two small costs:

- An icon name that the sources build at run time from pieces is not found by the scan, and draws as a ligature, which means blank on the Mac. The engine check fails on any such icon on the screens it opens; screens it doesn't open are covered only by keeping names as literals.
- `textContent` of an icon is now a private-use character, not its name. `data-icon-name` carries the name for anything that needs it.

### 2.4 The request to unicron

So that the patch can be dropped:

1. In `packages/ui/src/components/icon.tsx`, draw the glyph by codepoint: `children: codepoint ?? name`, with `data-icon-name={name}` on the `<text>`.
2. Ship the codepoint table with the package. The best option is a generated `icon-codepoints.ts`, built from the same `material-symbols` font at unicron's build. `Icon` would then always have the codepoint, and no consumer would need a plugin or `setIconCodepoints`. If the table's size matters (about 4,300 names, roughly 90 KB before compression), export `setIconCodepoints` as this patch does and document the plugin.
3. Add a WebKit check to unicron's own CI on `macos-latest`, since Linux WebKit does not show the defect. `design/capture/scripts/engine-check.ts` is a starting point.

This is a host-contract gap: unicron's `Icon` assumes the engine shapes ligatures inside SVG, and Apple's WebKit, which every Mac and iOS shell uses, does not.

### 2.5 Anything else WebKit draws wrong

The `engines` job keeps a screenshot of each main screen per engine: first run, unlock, the vault list, a login, settings, status, the kit page (screen and print) and the emergency sheet (screen and print).

- **Print layout.** Before this slice, a page that prints alone (`[data-print-page]`) printed as the dialog it is: fixed, centred by a translate, scrolled inside itself. On paper it came out shifted and cut off, in Chromium as much as WebKit. The print rule now lives in `app-extension.css`, which the desktop, the web vault and the harness all import (before, the harness had none of it). It sets the page static, unscrolled and untranslated, hides the overlay, the close button and the footer, and prints black on white.
- **The build's CSS minifier** folds a plain `translate: none` into `transform`. Tailwind's centring is reset through `--tw-translate-x/y` instead, which is noted in the rule.
- **A focus ring on the paper.** When the kit page opens, macOS WebKit focuses its first stop (the page-as-text block, `tabIndex=0`) and draws its focus ring, and the print emulation showed the ring on the printed page. The print rule now drops focus rings inside a page that prints alone. Chromium had put no visible ring there.
- **Dates read slightly differently.** WebKit's `Intl` writes "Oct 17, 2026 at 4:20 PM" where Chromium writes "Oct 17, 2026, 4:20 PM". This is the engines' own formatting of the same `en-US` options. It is harmless and left as it is.
- Apart from those and the icons, the WebKit screenshots on both runners (the first run, then the macOS run after the fix, artifact 12922071840) matched Chromium's layout on the screens checked: no clipped text and no other blank glyphs. The Inter text face loads and shapes in both.

## 3. Printing

`window.print()` returns at once and prints nothing in WKWebView under Tauri. Printing now asks the host:

| Host | `print()` | Returns |
|---|---|---|
| Desktop (`tauriPaperClient`) | `invoke("print_page")`: the shell calls `WebviewWindow::print`. On macOS that is `WKWebView.printOperation` run as a sheet on the window; on Linux, WebKitGTK's print dialog; on Windows, WebView2's own `window.print()`. | `"open"`: the panel runs on its own. |
| Web vault (`webPaperClient`), and the default with no provider | `window.print()` | `"done"`: the browser's dialog has closed. |

`usePrinting()` wraps that. After `"open"` it waits until the person is back in the window (a `focus` event, or a pointer going down in the page, which the panel's sheet would have stopped) before it says printing is over.

- **The kit page** (`once-page.tsx`) and **the emergency sheet** (`emergency-sheet.tsx`) call it. While a page with `[data-print-page]` is open, the print stylesheet hides `#root`, so the vault behind it is never printed. This is checked in both engines by the engine check (the app hidden, the page shown, under print media).
- **The Wi-Fi card** (`wifi-card.tsx`) reveals the password for the card's words (the same gate as before, D239) and draws the print-only card. It keeps the words until printing is over, then drops them. While a native panel is up, the card's minute stands still and losing the window to the panel does not hide the card, so the panel prints the card rather than an empty page. Once the person is back, blur hides it as before.

## 4. The recovery kit as a PDF

### 4.1 What the person sees

Beside **Print**, the kit page has **Save as PDF…**. It is shown only where the host can keep a file: the desktop, and the web vault if it ever shows a kit. The web vault makes no kits today (D176), so its path is wired and tested but cannot be reached. Above the buttons, before anyone makes a file:

> A saved file holds everything needed to get into your account with your password. Keep it off this computer: on a USB drive, as a printed copy, or in a safe. Lockdown keeps no copy.

After a save: "Saved where you chose. Lockdown kept no copy." A cancelled dialog says nothing. A refusal shows its words.

Other changed words:

- The dialog's description: "Shown once and never kept. Print it, save it as a file or write it down, and put it somewhere safe, away from your devices."
- First run's third screen: the sentence "Print the page, save it as a file or write it down, away from your devices; the drill checks it works before you need it."
- First run's "Learn more" gains: "A saved file holds everything needed to get into your account with your password, so keep it off this computer: on a USB drive, as a printed copy, or in a safe."
- The checkbox reads **"I saved, printed or wrote down the kit"**. Saving counts as printing does: ticking the box is still the person's own act.

### 4.2 The page

One A4 page, laid out as the dialog prints it, in JetBrains Mono:

1. The title **Lockdown recovery kit** and "Made <date> on <device>".
2. The notes the dialog shows above the page.
3. **Secret Key** and **Recovery code**, each under a heading with a rule and the date on the right (as unicron's LargeTypeView draws them), in 26 pt with extra letter spacing. Each value is shown as typed, without the dashes, in groups of four, four groups to a row: the same grouping as the dialog.
4. The account lines (Login name, Server, Profile, Made).
5. The page's own advice ("Keep this page somewhere safe…", "To get back in on a new device…", "Lockdown never asks for this page…").
6. **Where to keep this file**, with: "This file holds everything needed to get into your account with your password. Keep it off this computer: on a USB drive you put away, as a printed copy, or in a safe. Lockdown keeps no copy."

Each value appears once. The dashed forms are not repeated, unlike the printed dialog's text block. The dialog's per-character captions ("capital A three…") are not on the PDF. The font is chosen so they are not needed: JetBrains Mono's `0` is dotted, its `l` has a tail, its `1` a flag and foot, and its `I` bars.

### 4.3 The crate

**`pdf-writer` 0.15.0** (typst; MIT OR Apache-2.0). It is a step-by-step writer with no layout engine and no font parser, and it is pure Rust, so it builds unchanged for `wasm32-wasip1`. Its four dependencies (`bitflags`, `itoa`, `memchr`, `ryu`) were already in the lock file at the same versions, so the only new crate is `pdf-writer` itself.

- **cargo-deny:** its licence is on the allow list, it has no advisories and no duplicate versions, and `cargo deny check` passes.
- **cargo-vet:** none of the imported audit sets covers it, so it has one exemption at `safe-to-deploy` in `supply-chain/config.toml`. This is what `cargo vet regenerate exemptions` would add; cargo-vet was not installable in the build container, so the entry was added by hand in sorted place, and CI's `cargo vet --locked` passed on it.

The alternatives were heavier: `printpdf` pulls in `lopdf` and its own font and image handling, and `krilla` a full text stack. Neither is needed for one page of monospaced text.

The font is JetBrains Mono Regular and Bold (SIL OFL 1.1, no reserved font name), cut down with fontTools to the Windows-1252 characters (about 18 KB each) and embedded as simple TrueType fonts with WinAnsi encoding. The licence is in `crates/app/assets/fonts/OFL.txt` and the attribution in `NOTICE`. A character outside Windows-1252, such as a non-Latin device name, prints as `?`.

### 4.4 Secrets, gates and wiping

- **The gate is the one that shows the kit.** The kit page reaches a shell only from `kit_create`, `account_register` or the opted-out registration. Those calls need the vault open and, with an account, the account password. Saving takes back exactly the page the shell holds while the dialog is open. The core refuses anything that isn't a kit page in `render_kit`'s shape, and refuses while the vault is locked (`Locked`). No new way to get the kit exists. It does not count as a use under D151, because the kit is not an item.
- **Desktop:** the page travels as raw bytes, the same shape `kit_create` sent it in (a NUL-terminated JSON list of notes, then the page). The webview wipes its copy once sent. The shell parses it into `Zeroizing` buffers and checks it is a kit page before it opens any dialog. After the dialog, the core lays out the PDF and writes it to that path only: create or truncate, owner-only (`0600`) on Unix, `sync_all`, with no temporary file or rename, and the file is removed again if the write fails. Then it drops the bytes (`Zeroizing`). The PDF never reaches the webview, and the webview gets no dialog or file permission.
- **Web vault:** `kit_pdf` takes the page as call arguments, which `call` scrubs. It returns the bytes as `Reply::Bytes` (wiped on drop). The page hands them to the browser as a `Blob` download, then zero-fills its array and revokes the object URL a second later. The browser's own copy in the Blob cannot be wiped from the page.
- **Inside the core:** every buffer holding a value is `Zeroizing`: the typed groups, each encoded string, the content stream, and the finished file. The writer is given room for the whole file up front, so it never grows by reallocating, which would leave an unwiped copy behind. Nothing here logs.

## 5. Running and testing

```sh
# The engine check, locally (Chromium only here; CI installs WebKit and Firefox on its runners)
cd design/capture && pnpm build && pnpm engine:check --engines chromium --out /tmp/engine-check

# The kit PDF
cargo test -p lockdown-app --lib kit_pdf      # parses its own output: xref, page tree, both fonts embedded, the text shown
cargo test -p lockdown-app --test kit_pdf     # the page kit_create returns; written there only, 0600; refused when locked
cargo test -p lockdown-web --test kit_pdf     # a signed-in tab gets the desktop's bytes; a locked tab gets nothing
cargo test -p lockdown-desktop --features shell paper   # the body split

# The seams
cd apps/desktop && pnpm vitest run src/app/tauri-paper.test.tsx src/app/icon-codepoints.test.tsx src/features/status/once-page.test.tsx src/features/vault/vault-depth-detail.test.tsx
cd apps/web && pnpm vitest run src/core/web-paper-client.test.ts
```

The PDF tests parse the file with a reader written for the test, so the parse does not rely on the writer's own bookkeeping. They check that the header, every cross-reference offset, the trailer, the page tree and the content stream's length are consistent, and that both fonts are embedded. They extract every string shown and check that each value appears exactly once, as its groups of four in order, and that the date, the notes, the advice and the file's words are there. A test also checks the constants written into the PDF against the fonts' own tables (`head`, `hhea`, `hmtx`, `OS/2`, and `cmap` coverage of every Windows-1252 character). `pdftotext` and `pdffonts` (poppler) read the file as expected, and a rendered copy was checked by eye.

## 6. Gaps

1. **The web vault never shows a kit.** It makes none (D176), so its Save path is reachable only if that changes. It is tested at the client seam and in `crates/web/tests/kit_pdf.rs`.
2. **The print panel's end is inferred.** Neither wry nor Tauri reports when a native print panel closes. `usePrinting` takes "the window is back" to mean the panel has closed. If a platform's panel never takes focus, the Wi-Fi card's words stay drawn until the card closes (its minute or a lock still closes it).
3. **Linux and Windows printing changed too.** `print_page` uses `Webview::print` on every platform: WebKitGTK's print dialog on Linux, WebView2's `window.print()` on Windows. Neither was run by hand here. CI builds and lints the shell on both.
4. **The extension popup** compiles the patched `Icon` but does not register codepoints. It runs only in Chromium and Firefox, which shape the ligature. A Safari Web Extension will need the plugin's map in its build (esbuild there).
5. **No PDF/UA tags or outline.** The PDF is a plain page with real text (selectable and searchable). Tagging for screen readers is a later step.
6. **Paper size.** A4 only. The content sits inside 792 pt, so it also fits US Letter at 100%.
7. **cargo-vet** was not run in the container (no binary). CI's `cargo-vet` job passed with the hand-added exemption.

## 7. unicron host-contract gaps

- `Icon` relies on the engine shaping ligatures inside SVG `<text>`. Apple's WebKit does not (§2.4).
- `LabelledActions` (Lockdown's own) and `DialogFooter` take no hook to keep a footer off the paper. The print rule hides `[data-slot="dialog-footer"]` inside a `[data-print-page]` instead. A `print:hidden` default on `DialogFooter`, or a documented print story for dialogs (static, unscrolled, no overlay), would make that unicron's.

## 8. Proposed decisions

For the coordinator to record. **Flagged** rows are user-visible and need Sean.

| ID | Decision | Owner |
|---|---|---|
| D451 | **The UI is checked in the engines it ships on.** A CI job opens the review harness's build of the real desktop screens in WebKit, Chromium and Firefox on Linux, and WebKit and Chromium on macOS (Apple's WebKit, as WKWebView). It fails when an icon is not one glyph or is drawn by name, or when a page that prints alone would print the app behind it, and it keeps the screenshots. Plain version: the app is now checked in the Mac's own browser engine, not only in Chrome's. | Accepted under D38 (structure, not security) |
| D452 | **Icons are drawn by codepoint.** unicron's `Icon` is patched here (until unicron ships §2.4) to draw an icon's private-use codepoint, worked out at build time from the shipped Material Symbols font for the names the sources use. Names it lacks still draw as ligatures. The `<svg>` wrapper and the variable axes are unchanged. Plain version: icons show on the Mac. | Accepted under D38 (structure, not security) |
| D453 | **Printing goes through the host.** Desktop features never call `window.print()`: the desktop's shell opens the webview's own print panel on every platform (`Webview::print`), and the web vault uses the browser's. A page that prints alone prints whole, without its buttons, and never with the vault behind it. Plain version: Print works on the Mac, and prints only the page. | Accepted under D38 (structure, not security) |
| D454 | **The Wi-Fi card holds still while a print panel is up.** Its minute stops and losing the window to the panel doesn't hide it. The revealed words stay drawn only until the person is back in the window. Plain version: printing the guest card prints the card, not a blank page. | Accepted under D38 (amends D239's timing only while printing) |
| D455 | **Save the recovery kit as a PDF.** Beside Print, **Save as PDF…** saves the page being shown as one A4 page laid out like the printed one. The suggested name is `Lockdown Recovery Kit.pdf`. On the desktop it goes only where the person picks in the system's save dialog: no temporary file, no copy in Lockdown's data, owner-only. In the web vault it is a browser download of the same bytes. Plain version: you can keep the kit as a file as well as on paper. | **Flagged for Sean** (a new way to keep the kit) |
| D456 | **The words around the file.** Before saving: "A saved file holds everything needed to get into your account with your password. Keep it off this computer: on a USB drive, as a printed copy, or in a safe. Lockdown keeps no copy." After: "Saved where you chose. Lockdown kept no copy." On the PDF: "Where to keep this file" with the same advice. The first-run checkbox reads "I saved, printed or wrote down the kit", and saving counts as printing does (the person ticks it). Plain version: the app tells you plainly what the file is and where it should live. | **Flagged for Sean** (the words) |
| D457 | **Saving the kit is gated as showing it is.** Only the page a shell was shown can be saved: the core refuses any other text, and refuses while the vault is locked. It adds no new release of the kit and is not a use under D151. The page and the PDF are wiped after use (`Zeroizing`; the writer never reallocates), never logged, and the PDF never reaches the desktop's webview. Plain version: saving the kit needs nothing less than seeing it did, and leaves nothing behind. | Accepted under D38; audit item |
| D458 | **The kit's PDF is built in the core with `pdf-writer` and JetBrains Mono.** It is pure Rust and the same bytes on the desktop and in the web vault's wasm. The only new crate is `pdf-writer` (MIT OR Apache-2.0, one cargo-vet exemption). The font is embedded and cut to Windows-1252 (OFL 1.1), chosen for a dotted 0 and a distinct 1, l and I; other characters print as `?`. Plain version: the file is made inside Lockdown, with a typeface where similar characters can't be confused. | Accepted under D38 (supply chain: audit item) |
| D459 | **The kit's PDF is A4, values in groups of four.** One page; each value once, as typed, in groups of four (the dialog's grouping), with no per-character captions. The content fits US Letter too. Plain version: the saved page matches the printed one, on either paper size. | **Flagged for Sean** (A4, and no captions on the file) |
