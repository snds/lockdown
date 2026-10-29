# 37: The command line opens shared vaults, and the popup's polish

_Slice: `slice/cli-popup`, built 2026-10-29. Two small pieces built together: the command line's vault open (doc 36 §6 gap 3, a bug) and the extension's polish rows from the design review (R16, R17 in `docs/design/2026-10-29-review.md`). Builds on D38 (who decides security-model details), D62 (wire shapes only grow), D73 (revocation boundaries), D151 (what counts as a use), D155–D159 (the extension's channel, pairing and menu), D196 (sharing gates), D206 (presence for passkeys), D430–D435 (shared vaults in step). Proposed decisions D440–D447 (§7)._

## For future agent

- **TL;DR:** (1) `bins/cli/src/app.rs` opened `persona_vaults(...).first()` (ids are random, so a shared vault could come first) and trusted only the identity's devices, so a persona whose shared vault a member had written to could fail every command with "the vault's history didn't check out at manifest 2: signed by a device this vault doesn't know". The open is now one code path, `lockdown_app::vault_open`, which `Session::open` and `open_new_vaults` run too: every vault of the persona is tried with the identity's devices; the persona's first vault is the first that opened in the app's order (the person's order, the oldest first head, then the id); a vault naming a signer nobody vouches for is tried again trusting its writers as this device pinned them in the sharing record (read only, `open_shared_vault_trusting`), and otherwise waits instead of failing; `status` says how many more opened and how many wait. (2) The popup draws every result as one row (the kind's glyph, the title, one line under it, one action at the trailing edge that never wraps under the words), its text is 13 px or more, and where it can't act (locked, no app, not paired) it offers **Open Lockdown**: a running app brings its window forward (`show`, answered before pairing), and with no app the native host starts the app installed beside it (`launch`), or the popup says how. (3) An in-page control waiting out its 400 ms keeps its colours and draws an underline that fills (static under reduced motion); covered it looks disabled, armed it looks plainly enabled; D159's two heights are unchanged.
- **Read with:** `crates/app/src/vault_open.rs` (the rules), `crates/app/src/session.rs` (`Session::open`'s vault loop), `crates/app/src/session/organize.rs` (`open_new_vaults`), `bins/cli/src/app.rs` (`App::open`, `open_with_pinned_writers`, `other_vault_lines`), `bins/cli/tests/shared_vault.rs`; `extension/src/popup/popup.tsx` (`Link`, `OpenLockdown`, `Row`, `ROWS`), `extension/src/background/link.ts` (`openApp`, `launchApp`), `bins/native-host/src/launch.rs`, `apps/desktop/src-tauri/src/browser/{mod,handler}.rs` (`show`, `on_show`), `extension/src/pages/frame.ts` (`arm`, `cue`), `extension/static/ui.css` (the arming rule).
- **Do not:** open a persona vault by id order, or trust a writer's device anywhere but `open_shared_vault_trusting`'s member set; let the command line pin anyone or write the sharing record (it has no sharing step; the app's sync meets writers through the log); launch anything a request names (the host picks the app itself, by the endpoint file's hash or its own folder); answer anything but `show` (and the four pairing ops) before a connection proves its pairing; let a click count before the 400 ms are up (the cue is a picture, `deliberate()` still decides); pass `className` to a unicron component in the popup (the design-system ratchet's baseline is zero overrides).

## 1. The command line opens shared vaults the way the app does

**The bug** (doc 36 §6 gap 3). The CLI kept its own open: `persona_vaults(&identity, persona, &store).first()`, which the SQLite store lists by vault id, opened with `open_vault_trusting` and only this identity's devices. Ids are random. When a vault the person shares sorts in front of their first vault and a member has written to it, its heads name the member's device, and every command that opens the vault (`list`, `show`, `status`, `init`, `sync`) failed. The failing test, before the fix:

```
cargo test -p lockdown-cli --test shared_vault
  panicked in the test's `ok` helper, on the first `lockdown list` on Alice's root:
  lockdown ["list"] failed:
  lockdown: the vault's history didn't check out at manifest 2: signed by a device this vault doesn't know
```

**Now: one code path** (`lockdown_app::vault_open`). The CLI depends on `lockdown-app` and calls the functions `Session` itself runs, rather than a copy of them:

| Step | Function | What happens |
|---|---|---|
| 1 | `open_own_vaults` | Every vault of the persona is opened with this identity's devices (`own_device_keys`: its records, then the synced set) and the revocation boundaries for that vault; each is held to post-quantum heads from where this device first saw one (doc 28). A vault that names a head or version signer the identity doesn't know waits (`Waiting`, the load's own words kept); any other refusal stops the open, as in the app. |
| 2 | `order_vaults` | The vaults that opened are put in the app's order: the place the person gave each one, then its first head's time, then its id. The first is the persona's first vault, the one the CLI's commands work in. The id only breaks ties, so a vault that sorts first by id can't take its place. |
| 3 | `pinned_writers`, `vault_trust`, `open_one` | Only when something waits: the persona's sharing record (`sharing-<persona>.cbor`, sealed under the persona's App Data Key, written by the app) is opened read only; each waiting vault's signed membership chain is replayed against the devices this device pinned, and the vault is tried again trusting its writers' devices apart from the identity's own (`open_shared_vault_trusting`, D431). One that still names a signer nobody vouches for keeps waiting. |

`Session::open` runs steps 1 and 2 (`open_own_vaults`) and step 3 through `open_new_vaults` (`open_one` with `vault_trust` over its rosters); the session's sharing helpers (`device_table`, `device_keys_of`, the roster replay, `writer_keys`) are now thin calls into `vault_open` as well, so the CLI and the app read pins the same way.

**What the CLI does with the others.** Its commands still work in one vault, the persona's first (§6 gap 1). The others that opened are counted, and the ones that wait are said, in `lockdown status`:

| Line | Words |
|---|---|
| More vaults opened | `other vaults   2 more open; the command line works in the vault above` |
| One waits | `waiting        1 shared vault has changes from people this device hasn't met yet. It opens once the Lockdown app syncs and checks them against the server's log.` |
| Several wait | `waiting        2 shared vaults have changes from people this device hasn't met yet. They open once the Lockdown app syncs and checks them against the server's log.` |

**Whom it trusts, and from where.** The CLI never pins anyone and never writes the sharing record: it has no sharing step, so it trusts a member's device only when the app on this device already pinned it through the log (`pin`, `verify_directory_entry`, `check_continues`, as every pin). On a machine where only the CLI runs, a vault others wrote to waits until the app syncs there; nothing is trusted from the server on the CLI's word.

## 2. The popup (review R16)

**One row anatomy** (`Row` in `popup.tsx`). Every result in the popup, the page's logins, quick access's results and the alias services alike, is unicron's `Item` with: `ItemMedia` holding the kind's glyph (the app list's own glyphs; decorative, the words say the kind), `ItemTitle` (the title, and a badge such as "From quick access" or "Close match"), `ItemDescription` (one line: username and site, or the kind and "protected" for a crown jewel), and one `ItemActions` button at the trailing edge:

| Row | Its one action |
|---|---|
| A login for the page | **Fill** (its **Change password** link sits under the username, inside the words) |
| A result with something to copy | **Copy *label*** (**Copy anyway** on a lookalike page, D166) |
| A result whose copy is a one-time code | **Show code**; while the code shows in the row, the same place says **Copy code** |
| An alias service | **New alias** |

unicron's `Item` wraps its actions under its words when they run long (`flex-wrap`), which is why the copy button sat beside the title in one row and under the subtitle in the next. The popup's lists keep the action at the trailing edge and let the words truncate, one line each (`ROWS`: arbitrary variants on the list's wrapper `div`, set on the slots, not by `className` on the components, so `pnpm ds:ratchet` stays at its zero baseline; §5).

**Text 13 px or more.** The popup's small type was unicron's `Button size="sm"` (`text-[0.8rem]`, 12.8 px): every button is now the default size (14 px). Titles, subtitles, the status line and the fields were already 14 px. Badges stay unicron's 12 px label (`text-xs`); they are labels, not body text.

**Open Lockdown** in the three states where the popup can't act:

| State | What the popup shows | What the button does |
|---|---|---|
| Locked | "Lockdown is locked. Unlock it in the app to fill and search." and **Open Lockdown** (primary) | `show`: the app's main window comes forward (restored, shown, focused). The status line: "Lockdown is open on this computer. Unlock it there to fill and search." |
| Not paired | the pairing sentence, **Pair with Lockdown** (primary) and **Open Lockdown** (outline) | `show`, before any pairing; the status line: "Lockdown is open on this computer." |
| No app (or the host can't reach one) | the host's sentence, **Open Lockdown** (primary) and **Try again** (outline) | the native host starts the app (below); the status line says "Opening Lockdown…" and the popup asks each second for up to 20 s, then shows where things stand ("Lockdown is open on this computer."). Where nothing could be started, or it never answered: "Lockdown couldn't open the app from the browser. Open Lockdown from your applications, then unlock it." |

**How the app is opened** (`link.ts` `openApp`). With the app reachable, the background sends `show` on its link; the endpoint answers it before pairing (`handler.rs`), at most once a second (`SHOW_EVERY`), through the shell's hook (`Endpoint::on_show`; `browser_commands::start` gives it one that unminimizes, shows and focuses the `main` window). Nothing else is answered before pairing, and nothing secret moves: the person unlocks or approves in the window as always. With no app, the background opens a port of its own (`launchApp`, so the link's own state isn't touched): the host says `no_app` as before and then reads one request (`bins/native-host/src/launch.rs`). Only `launch` is answered, only for an extension Lockdown answers (`Caller::is_allowed`), and the app started is never one the request names:

1. the executable the endpoint file names, while its SHA-256 still matches what the app wrote there (an app that ended without removing the file); else
2. `lockdown-desktop` beside the host's own executable (every package installs the two together; `EndpointConfig::for_app` assumes the same).

The app starts detached: no stdin, stdout or stderr from the browser's pipe, its own process group (`process_group(0)`; on Windows `DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP`), the browser's environment (so a profile set by `LOCKDOWN_DATA` or `LOCKDOWN_ROOT` reaches it, as it reaches the host). The answer is `{"launched": true}`, or the refusal `not_launched` with the sentence above, or `no_app` for any other request. A host whose app came up meanwhile says `ready`, and `show` is asked instead.

## 3. Arming you can see (review R17)

`frame.ts`'s `arm()` already kept every in-page control disabled until the frame had been uncovered for 400 ms (D159, D206), and again whenever it was covered. While waiting, a control looked exactly like a disabled one (`opacity: 0.6`), so the person saw no sign it was about to work. Now:

| Moment | How a control looks |
|---|---|
| Waiting out the 400 ms (uncovered) | Its full colours (`opacity: 1`, `cursor: progress`) and an underline in its ink that fills left to right over what is left of the wait (`data-arming`, a `::after` scaled from 0 to 1 with a negative delay for the time already waited, so a control drawn late doesn't start from empty). Under `prefers-reduced-motion: reduce` the underline is drawn whole, at 60 %, and doesn't move. |
| Covered | Disabled, as before (`opacity: 0.6`, no underline). Uncovered again, the wait starts over and so does the underline. |
| Armed | Plainly enabled: no underline, full colours, the pointer. |

The cue is drawn only: `deliberate()` still refuses any click before the 400 ms or while covered, untrusted events are still ignored, and the frame's two heights (D159) don't change (the underline sits inside the button). It applies to every armed control in the menu, the save prompt, the passkey sheet and the menu's pairing offer.

## 4. Running and testing it

```sh
export CARGO_TARGET_DIR=… CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=line-tables-only
cargo test -p lockdown-cli --test shared_vault            # the failing test of §1, now passing
cargo test -p lockdown-app --test sharing_pause --test sharing --test organization   # the session on the shared path
cargo test -p lockdown-native-host                         # launch: beside the host, the endpoint file's hash, refusals
cargo test -p lockdown-desktop --lib browser               # `show` before pairing, once a second, nothing else answered
cd extension && pnpm build:wasm && pnpm run build:native && pnpm build
LOCKDOWN_TEST_BINS=<private copies of lockdown-native-host and browser-endpoint> pnpm test
cd ../design/capture && pnpm build && pnpm capture --only extension- && pnpm review:build
```

| Test | What it proves |
|---|---|
| `bins/cli/tests/shared_vault.rs` `the_cli_opens_the_personas_own_vault_when_a_shared_vault_others_wrote_to_sorts_first` | Alice (an `App` on the real server in process) makes vaults until one sorts in front of her first by id, shares it with Bob, Bob writes to it; the real `lockdown` binary on Alice's root then lists her first vault, `status` names it and says `N more open`, `init` keeps it; with the sharing record left out (the same device before it met Bob) the shared vault waits, `list` still works and `status` says the one-vault waiting line |
| `bins/native-host/src/launch.rs` (3) and `lib.rs` (1) | `launch` starts the app beside the host and never a path the request names; the endpoint file's app only while its hash matches; with nothing to start it answers `not_launched` with the sentence; other requests get `no_app`; an extension Lockdown doesn't answer gets `refused`; through `run` with no endpoint file, `no_app` then the launch answer |
| `apps/desktop/src-tauri/src/browser/tests.rs` `open_lockdown_brings_the_window_forward_before_pairing_and_answers_nothing_else` | `show` before pairing: `unsupported` without a window hook, then shown once for two presses within a second, again after; `search` still `not_paired` |
| `extension/test/extension.spec.ts` | An unpaired popup's Open Lockdown brings the fixture's window forward (`shown`); the locked popup says so and opens it with the locked words; a code row shows its code, then copies it from the same place; a long-titled result keeps glyph, words and its one action in one line with the subtitle cut short, and every drawn word but the badges is 13 px or more; a menu choice covered then uncovered shows the arming underline at full colour and then looks plainly enabled, with the animation under `no-preference` and none under `reduce` |
| `extension/test/open-app.spec.ts` (new) | With no app and a private copy of the host, Open Lockdown says how when nothing is beside the host; with a stand-in `lockdown-desktop` beside it, the host starts it (with no arguments), the popup says "Opening Lockdown…", and since the stand-in never answers, says how after its wait |
| `extension/test/a11y.spec.ts` | slice 31's frame checks unchanged and green; the popup's rows and its locked state with Open Lockdown have no serious or critical axe violation, light and dark |

**Design capture.** All 37 extension states re-taken (`pnpm capture --only extension-`, 74 images), four of them new: `extension-popup-locked-opened`, `extension-popup-no-app-how`, `extension-menu-arming` and `extension-prompt-arming`. The two arming states are taken with the clock held (`hold_clock`, a new `StateDef` field the capture honours), so the frame stays inside its 400 ms; the capture runs with reduced motion, so they show the static underline. No extension state has an axe violation. The review build: 17 files, 6.7 MB.

## 5. Gaps

1. **The CLI works in one vault.** `list`, `search`, `show`, `add`, `edit`, attachments and `sync` act on the persona's first vault; the others are opened (so a broken one still stops the command, as in the app) and counted in `status`, not listed or synced. Moving the CLI onto `lockdown-app`'s session (doc 36 §6's follow-up) would give it every vault, the sharing step and the app's sync; this slice moved only the open.
2. **On a machine with only the CLI, a vault others wrote to waits** until the app syncs there (the CLI has no sharing step to meet its writers through the log). `status` says so.
3. **Opening the app depends on the install layout.** The host starts `lockdown-desktop` beside itself. That holds for the Linux packages, the MSIX layout and the reproducible build's stage; on macOS it starts the bundle's binary directly (not through Launch Services) and is untested here; inside a Flatpak the host runs in the sandbox, and an app it starts may close with it. In each case the popup's 20-second wait ends in the how-to sentence rather than a silent nothing.
4. **The menu's own locked and no-app states** (in the page) don't offer Open Lockdown: R16 asked for the popup; a page-side button that starts a program is a larger question (it would be reachable by a page's focus on any sign-in field).
5. **Firefox** runs the same code; Playwright can't load the add-on, so Open Lockdown and the arming cue are driven only on Chromium (doc 16 §7).
6. **Showing then copying a code is two presses** where it was one press of Copy beside a separate Show: the one-trailing-action rule made the code row's place do both in turn.

**unicron host-contract gaps** (for `snds/unicron`, with brief §11.1):

| Missing in unicron | Lockdown's workaround |
|---|---|
| An `Item` whose `ItemActions` stay at the trailing edge (no `flex-wrap`), with one-line truncating `ItemTitle` and `ItemDescription` (`line-clamp-2` today) | arbitrary variants on the popup list's wrapper `div` (`ROWS`) |
| A small `Button` at 13 px or more (`size="sm"` is `text-[0.8rem]`, 12.8 px) | the default size in the popup |
| A `Badge` size at 13 px | none; badges stay 12 px labels |
| An arming / pending state on `Button` (a progress underline, static under reduced motion) | the in-page frames are plain CSS (`static/ui.css`), by design (brief §3.5) |

## 6. Audit notes (D38)

Which devices the CLI trusts when it opens a vault (the identity's own, then a shared vault's writers only as the app pinned them, kept apart as members), that it reads the sharing record without writing it, which program the native host may start and for whom, and that `show` is answered before pairing are security-model choices made under D38. They are audit items in §7, for the outside review (D377) with D430–D435.

## 7. Proposed decisions

For the coordinator to record. **Flagged** rows are user-visible and need Sean.

| ID | Decision | Owner |
|---|---|---|
| D440 | **The command line opens a persona's vaults through the app's code path** (`lockdown_app::vault_open`): each with the identity's devices; the persona's first vault is the first that opened in the app's order (the person's order, the oldest first head, then the id), never the first by id; a vault naming a signer nobody vouches for is tried again trusting its writers as pinned, apart from the identity's own devices (D431), and otherwise waits instead of failing the command; any other refusal stops it, as in the app. `Session::open` and `open_new_vaults` run the same functions. Closes doc 36 §6 gap 3. Plain version: the command line opens your own vault every time, even when a vault you share with family was changed by them. | Accepted under D38; audit item |
| D441 | **The command line trusts only pins the app made, and makes none.** It reads the persona's sharing record (sealed under the App Data Key) to replay a waiting vault's membership chain against the devices this device pinned through the log; it never pins, looks anyone up or writes the record; a vault whose writers the app hasn't met waits until the app syncs. Plain version: the command line believes only who the Lockdown app already checked. | Accepted under D38; audit item |
| D442 | **`lockdown status` says what else opened and what waits**: "other vaults   *n* more open; the command line works in the vault above" and "waiting        1 shared vault has changes from people this device hasn't met yet. It opens once the Lockdown app syncs and checks them against the server's log." (the plural for more). Plain version: the command line tells you when it can't open one of your shared vaults yet, and why. | **Flagged for Sean** (the words) |
| D443 | **One popup row**: the kind's glyph, the title (and its badge), one line under it, one action at the trailing edge that never wraps; a code row's action is **Show code**, then **Copy code** in the same place while the code shows; buttons at the default size so text is 13 px or more (badges stay 12 px labels). Amends doc 26 §9's popup. Plain version: every result in the browser popup looks the same, with its button always at the right, in bigger type. | **Flagged for Sean** (copying a code from the popup now takes Show, then Copy) |
| D444 | **Open Lockdown where the popup can't act**: locked, not paired and no app; a running app brings its window forward (`show`, answered before pairing, at most once a second, nothing else unlocked by it); otherwise the native host starts the app and the popup waits up to 20 s; the words "Lockdown is open on this computer.", "… Unlock it there to fill and search.", "Opening Lockdown…" and "Lockdown couldn't open the app from the browser. Open Lockdown from your applications, then unlock it." Plain version: the browser popup has a button that takes you to Lockdown when it needs you there. | **Flagged for Sean** (the words; an unpaired browser can bring the window forward) |
| D445 | **What the native host may start**: with no app on the socket, after `no_app`, one `launch` request from an extension Lockdown answers; it starts the executable the endpoint file names while its SHA-256 matches, else `lockdown-desktop` beside the host; never a path from the request; detached, in its own process group, with none of the browser's pipes; any other request gets `no_app`. Plain version: the browser can ask its helper to open Lockdown, and the helper only ever opens Lockdown itself. | Accepted under D38; audit item |
| D446 | **Arming you can see** (review R17): an in-page control waiting out its 400 ms keeps its colours and draws an underline that fills over the time left (whole and still under reduced motion); covered it looks disabled; armed, plainly enabled; the click still counts only once armed, and D159's two heights stand. Plain version: Lockdown's buttons on a web page show they are about to work instead of looking switched off. | Accepted by default under D38 (review R17's fix as written) |
| D447 | **Shape additions** (D62): the public module `lockdown_app::vault_open` (`PersonaVaults`, `Waiting`, `open_own_vaults`, `open_one`, `order_vaults`, `own_device_keys`, `vault_trust`, `pinned_writers`); `lockdown-cli` depends on `lockdown-app`; the extension-to-app op `show` (answered before pairing; `{shown: true}`, or `unsupported`); the host's `launch` after `no_app` (`{launched: true}`, or `not_launched`), `lockdown_native_host::launch`; `Endpoint::on_show`, `Shared::show_window`, `ShowWindow`, `SHOW_EVERY`; the extension's `open_app` message, `OpenedApp` and `OPEN_HOW`; the fixture's `shown` event; the capture's `hold_clock` and four states. No existing op, field, route, command, wasm boundary or extension schema changed. Plain version: everything new was added beside what was there. | Accepted by default under D38 (structure) |
