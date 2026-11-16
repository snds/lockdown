# 92: unicron 1.1: Lockdown's own theme layer, the 1.1 pre-release, the contrast check

_Slice of the 2026-11 decided changes, on `claude/keen-ritchie-q2hgb2`, built 2026-11-16. Builds on doc 62 (unicron 1.0) and on the decisions Sean delegated on 2026-11-16 (workspace `07-projects/29-unicron/impl-2026-11/rebuild-questions-decided.md`). Decisions D1000 to D1003; the decided Lockdown items are doc 93._

## For future agent

- **TL;DR:** unicron 1.1 ships no app's theme (decision Unicron 1). Lockdown declares its own 28 theme slots in `apps/desktop/src/styles/lockdown-theme.css`, with exactly the values 1.0 shipped, so nothing looks different (§1). The extension keeps an identical copy for its own build. The vendored tarballs move from 1.0.0-pre.2 to 1.1.0-pre.1 (§2), and unicron 1.1's contrast tool measures Lockdown's theme in the desktop's `pnpm contrast` and in CI (§3).
- **Swap line:** Swap to the `@unicron/ui` and `@unicron/tokens` release URLs when Unicron 1.1 is released, and delete `vendor/unicron/`.
- **Do not:**
  - Change a slot in one copy of the theme only. `extension/test/tokens.spec.ts` fails when `extension/src/popup/lockdown-theme.css` and `apps/desktop/src/styles/lockdown-theme.css` differ.
  - Point anything at `--g-neutral-*`, `--g-white-alpha-*` or an accent ramp. They are deprecated in 1.1 and leave in 2.0.
  - Name an exception in `contrast-exceptions.json` without a reason. The tool fails on a listed pair that now passes, so the list cannot go stale.

## 1. The theme layer (D1000)

**What.** Unicron 1.1 took the four app themes out of `@unicron/tokens`. Without its own block, `data-theme="lockdown"` would draw unicron's slate default. Lockdown now declares the 28 slots itself, 14 per ground, under `[data-theme="lockdown"]`, the way unicron's `docs/05-consuming.md` §3a says an app declares its theme.

| File | Read by |
|---|---|
| `apps/desktop/src/styles/lockdown-theme.css` | `app-extension.css` imports it first, so the desktop app, the web vault and the review harness all get it |
| `extension/src/popup/lockdown-theme.css` (identical copy) | the popup's `popup.css`; and `extension/scripts/tokens-css.mjs`, which copies the slots the in-page frames use into `static/tokens.css` |

The extension builds and is reviewed on its own (its source archive carries `extension/`, not `apps/`), so it keeps a copy. A test keeps the two equal, value for value, and checks there are 28 slots.

**The values** are the ones `@unicron/tokens` 1.0.0-pre.2 shipped under `[data-theme="lockdown"]` (Figma's `lockdown / graphite` and `lockdown / stone` modes), copied unchanged. The playground copy in unicron (`apps/playground/src/app-themes/lockdown.css`) has the same values.

**Nothing looks different.** The layer was added under 1.0 first, where it repeats the core's own block, and every state of the review harness was taken before and after: 1,376 pictures (every state at 1440x900 and 390x844, stone and graphite, the popup and frames at their natural size). 1,357 were identical to the pixel. Of the 19 that differed, 16 differ by 1 to 23 pixels of anti-aliasing (a known noise source, doc 62's handover), and 3 (`desktop-item-insurance`, `desktop-item-file-remove`) were taken again and matched (0 and 1 pixel). The frames' sheet `static/tokens.css` changed only in its header comment.

## 2. The packages (D1001)

Written with the move to 1.1.0-pre.1, the commit after this one.

## 3. The contrast check (D1002)

Written with the move to 1.1.0-pre.1.
