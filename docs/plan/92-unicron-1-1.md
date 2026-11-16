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

unicron 1.1 is not released until Sean merges its pull request and the Version Packages pull request publishes it. Until then the two packages are the integrator's pre-release tarballs, checked in where 1.0's were (doc 62 §1):

```
vendor/unicron/unicron-ui-1.1.0-pre.1.tgz       sha256 bcbef0574fbb75d901615719bd05d2129c01afe89b1d1679c769c18de3b73755
vendor/unicron/unicron-tokens-1.1.0-pre.1.tgz   sha256 b4f3c846da7a523df3ad0cf89adad728eb42ddaad67e97a4cf85e07783fd5b24
```

The 1.0.0-pre.2 tarballs are gone. All four pnpm roots (`apps/desktop`, `apps/web`, `design/capture`, `extension`) name the new files in their `package.json` and in the `@unicron/tokens` override of their `pnpm-workspace.yaml` (ui pins tokens `1.1.0-pre.1` exactly). **Swap to the `@unicron/ui` and `@unicron/tokens` release URLs when Unicron 1.1 is released, and delete `vendor/unicron/`.** The same line is in each `pnpm-workspace.yaml`.

**Lockfiles**, the way doc 62 §1 says: `apps/web` and `design/capture` with `pnpm install`; `apps/desktop` and `extension` with dsa held out for the install only (this session cannot fetch `@snds/dsa`) and put back, then `pnpm install --frozen-lockfile --lockfile-only` to prove the lockfile matches. Every lockfile carries pnpm's sha512 integrity for both tarballs.

**What arrives inside `@unicron/tokens`:** its contrast tool brings `apca-w3` 0.1.9 (license "Limited W3 License") and, through it, `colorparsley` 0.1.8 (AGPL v3). Neither is bundled into any page or the extension: they run only in `pnpm contrast`. **Flagged for Sean:** an AGPL package in every root's install tree is worth a look; unicron could move the tool's dependency to an optional or separate package.

**The migration notes of 1.1 (unicron `docs/08-migrating-to-1.1.md`), as they touch Lockdown:**

| Note | Done here |
|---|---|
| Declare the theme | §1 (D1000), the commit before this one |
| Contrast in CI | §3 |
| 47 icons left the set | `star--filled` (a favorite) and `data-backup` (Backups) are registered with `registerIcons()` in `apps/desktop/src/app/app-icons.ts`, with 1.0's exact drawing; each entry and both test setups import it |
| `timer` and `snooze` are Carbon's own now | Codes (the sidebar, the tab bar, the place table, a code row's kind and its swipe tile) draws `timer`, no longer the clock `time` that Expiring's neighbour also drew; a finding's snooze rows draw `snooze`. The clock notice keeps `time` |
| A long press from the keyboard is Shift+F10 | the harness's quick-create states and the shell test press Shift+F10; the create button's `aria-keyshortcuts` is unicron's `Shift+F10 ContextMenu` |
| Washes 4% and 6% | arrive with the core; the frames' `static/tokens.css` is regenerated |
| `plain.css` follows the browser with no class | no change: the frames' sheet is Lockdown's own cut (`tokens-css.mjs`), graphite under `prefers-color-scheme: dark` already |
| Counts read three digits | through unicron's parts at once; Lockdown's own `count()` follows in doc 93 (the cross-app settlement) |
| Shared build watch | **not adopted here:** the web vault keeps its own `core/build-watch*`, whose rule is stricter (reload only when nobody is signed in). The swap is optional in the guide ("when you are ready"); left for a later slice |

## 3. The contrast check (D1002)

`pnpm contrast` in `apps/desktop` runs unicron's `unicron-contrast` over `src/styles/lockdown-theme.css` with `src/styles/contrast-exceptions.json`, and CI's desktop job runs it after Lint. It measures 826 gated pairs on both grounds; 50 named exceptions are reported (Figma values with no room, flagged for Sean in unicron's hand-off: Unicron 16, 21 and 35), and 116 measures are reported only. The exceptions file started from unicron's playground copy for Lockdown; the tool fails on any listed pair that starts passing, so the list stays true.

## 4. Decisions

| ID | Decision | Status |
|---|---|---|
| D1000 | **Lockdown declares its own theme**, the 28 slots 1.0 shipped, in `apps/desktop/src/styles/lockdown-theme.css`, with an identical copy in the extension kept equal by a test. Plain version: the app looks exactly as it did, now that the shared kit holds no app's colours. | Built (Sean's ruling on Unicron 1) |
| D1001 | **Vendored 1.1.0-pre.1 tarballs** replace 1.0.0-pre.2 in all four roots, with the swap line kept. Plain version: Lockdown runs on the 1.1 pre-release until 1.1 is published. | Built |
| D1002 | **The theme's contrast is a CI gate** (`pnpm contrast`). Plain version: a colour change that makes text too faint fails the build. | Built |
| D1003 | **The web vault keeps its own build watch** for now (1.1's shared one is optional). Plain version: no change to when the web vault reloads itself. | Proposed |
