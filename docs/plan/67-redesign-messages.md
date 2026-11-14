# 67: messages in five channels, and the scrolled phone chrome

_Slice: `found-5` of the 2026-11 redesign, on `claude/keen-ritchie-q2hgb2`, built 2026-11-14. Brief X2 of the undrawn-areas brief (the status line, the status strip, toasts and Banners), the cross-app decisions §1 and §3, and the drawn frame States / 02 (Scrolled, dock minimized; Figma `51:8443`, stone `53:11805`). The slice text named this doc 66 and its decisions D740 to D749; both were taken by `66-shared-feed.md` (D740 to D746), so this is doc 67 with D750 to D759._

## For future agent

- **TL;DR:** what the app says goes to one of five channels, each with one home. Live status: `useStatusItem()` and the shell's status line and strip (`app/status.tsx`). Finished work: `notify()` (`app/notify.ts`). Waits on you: the needs-you (X3, not this slice). Broken and fixable: one critical `Banner` through `<BannerSlot place=…/>` (`app/banners.tsx`). News that can wait: one info `Banner` on All items. An update is a mark on the way into Settings, nothing else.
- **Do:**
  - Raise a toast only for work that finished, with `notify()` and the words of §2: no full stop, the thing named, one action at most (UNDO).
  - Say a screen's own fact at the end of the status line with `useStatusItem({ text })` (Codes' "Refresh every 30 s", Health's "Checked 09:40", Search's "03 results", the version on Settings).
  - Put `<BannerSlot place="status" />` and `<BannerSlot place="backups" />` at the top of Settings → Status and Settings → Backups (the screens lane; §4).
  - Wrap Health's own CHECK NOW in the shell's live work so the line reads CHECKING (§1; the security view lane).
- **Do not:**
  - Raise a toast for an error, a sync, an update or the daily news. An error that blocks is a critical Banner; one that belongs to a control is a line beside it.
  - Make anything in a status line yellow. Needs-you has its own block and strip.
  - Add a status-line button or a strip for an update. RESTART TO UPDATE is in About.
- **Read with:** brief X2, X3 and §3.3 (workspace, `07-projects/29-unicron/impl-2026-11/app-lockdown-undrawn.md`), cross-app decisions §1, §3, §8 and §12, doc 64 (the places and the shell), doc 65 (keys, pushed chrome).

## 1. Live: the status line and the strip

`app/status.tsx`. One shows at a time, the first that is true (`statusRow`):

| # | True when | Desktop status line (its end) | Phone strip |
|---|---|---|---|
| 1 | The idle lock is 10 s away (`LockState.idle_secs` against `idle_timeout_secs`; `useIdleCountdown` in `app/lock-watch.ts`) | `StatusItem` live: LOCKING IN // 08 S, ten segments, STAY UNLOCKED | `Accessory` live: LOCKING IN // 08 S over "No activity", STAY, the ten-segment bar |
| 2 | Opening (unlock to the first `list()`), a sync the app runs, the check a pull on Health runs | OPENING YOUR VAULT, SYNCING, CHECKING; neutral | `Accessory` neutral: UNLOCKING over "Opening your vault" (States / 04); SYNCING over "Checking for changes"; CHECKING over "Checking for breaches" |
| 3 | Nothing above | The screen's own line (`useStatusItem`), else the host's: HEALTH // 03 TO FIX or HEALTH // ALL CLEAR on Items and Sharing (a button to Health, once the cached report is read); SYNCED 14:31 in the web vault | none |

- The countdown is drawn every second between the lock watch's 3 s polls and is `aria-hidden`; the sentence "Locking in 10 seconds. Stay unlocked, button." goes once to the frame's one polite region (contract 5.1c C7). STAY is activity (`touch`), so the core starts the idle timer over; the strip hides at once and the next poll confirms.
- The strip's place is unicron's `AppFrameAccessory`: above the bottom row, inside it once the page has scrolled, at the foot on a short screen, nothing on a desktop. A needs-you (X3) outranks it: `StatusStrip` takes `outranked`.
- `useLiveWork(opening)` gives the app `track(kind, promise)`. The desktop tracks its own `client.sync()` (a pull, "Check again", TRY AGAIN, the first sync of a waiting vault) and the Health pull's `securityCheck()`. The web vault tracks `syncThenList` (after a change, a pull, Check again, the minute's sync while the live channel is down). Health's own CHECK NOW button is not tracked yet: it is in the security view (another lane).

## 2. Finished: toasts

`app/notify.ts`. `notify(text, { action, takeFocus, id })` over unicron's `toast` and the `Toaster` the shell now holds inside the `AppFrame`, so the frame places it: the bottom left of the content on a desktop (three at most), the strip's slot on a phone (one at a time), under the top row while a needs-you holds the strip. Times are unicron's: 6 s with an action, 4 s without. `toastWords` takes the full stop off.

| After | Words | Action | Built here |
|---|---|---|---|
| A copy from a swipe, a menu or Quick access | `copiedWords(label, title, clears_in_secs)`: "Copied the password for Steam, clears in 30 s" | none | the words; `useQuickCopy` (vault lane) still says today's sentence |
| A save | `savedWords(title, generated)`: "Saved Steam", "Saved Steam with a new password" | none | yes, both apps (the web vault says "Saved" for a new item, whose title arrives with the list) |
| Moved to Recently deleted | `deletedWords(title)` | UNDO | the words; the row menu is the vault lane's |
| Archived | `archivedWords(title)` | UNDO | the words; as above |
| A typed password cleared (D315) | `wipeWords(n, "window" \| "tab")` (`features/hygiene/secret-wipe.ts`) | none | yes, both apps |
| A gate backed out of | `cancelledWords("copied" \| "shown")` | none | the words |

Every `onNotice` the shell hands a screen now goes through `notify`, so today's sentences lose their last full stop wherever they come from. A control with its own copy (`SecretField`, `CodeDisplay`, a Field Row's COPY) raises no toast; nothing here changes that.

## 3. Updates: a mark, and About

`features/updates/restart-row.tsx` is now `updateMark(status)` and `useUpdateMark()`: while `UpdateStatus.state === "ready"`, a square mark on the Settings nav item, its rail item and the phone's profile button, with "update 1.9 ready" joined to each one's name (unicron's `mark`, contract 5.1b #7, 5.1c C1). RESTART TO UPDATE is About's `UpdateStatusRow`, unchanged. The sidebar's restart row is gone (it left the sidebar in doc 64 and is not drawn anywhere now).

`useUpdateNotices(show, onAsked)` no longer raises a toast. A check asked from the macOS menu while About is not showing opens Settings → About as it starts, once per check, where the rows say how it went. The automatic check opens nothing.

## 4. Broken and fixable: one critical Banner

`app/banners.tsx`. `firstCritical(sources)` picks the first true one; `criticalBanner` gives its words and its one action. Not dismissable; it leaves when its source clears. Never a toast.

| # | True when | Title | Body | Action | Also on |
|---|---|---|---|---|---|
| 1 | `Status.account.sign_in`, or a live `session_ended` | Lockdown isn't syncing | the core's `sign_in.message`, else "This device's session was ended from another of your devices." | SIGN IN: today's `AccountDialog`, as Status opens it | Settings → Status |
| 2 | A sync line holds "Split view" (live `synced`, or the app's own sync) | Split view | "Your server may be showing you a different history from everyone else. New sharing is paused until it clears." | SEE STATUS (the web vault: This server, where the mirrors are) | Sharing; Settings → Status with no action |
| 3 | A live `sync_failed`, or the app's `client.sync()` rejected | Couldn't sync | the core's words | TRY AGAIN: `client.sync()` | — |
| 4 | Backups are on and `BackupStatus.last_error` is set (`useBackupTrouble`, read at unlock and every minute) | The last backup didn't work | the core's words | OPEN BACKUPS | Settings → Backups |

- **Where:** at the top of Items, under the page head: on a desktop, in the list pane under its head; on a phone, under the head's chip row. On Sharing, under its head. The shell lays it in through the page head's `below` (`PageHeadFromShell`), so a screen draws nothing for it. Settings → Status and → Backups take `<BannerSlot place="status" />` / `"backups"` from the screens lane.
- A split view clears at the next sync without the line; a failed sync at the next sync that works; the session at a sign-in through the Banner's dialog (or Status's, which refreshes the status).
- The web vault: rows 2 and 3. A session the server won't renew locks the tab, so its words stay a toast over the unlock screen (a Banner would never be seen), and the same for a session ended elsewhere. The web has no backups.

## 5. News that can wait: one info Banner

`useNews()` holds the news in arrival order and shows the first, at the top of All items (no scope), with SHOW and a close button. A dismissed one stays away for the rest of the day (`lockdown.news-dismissed` in local storage: an id and the day, nothing from the vault). Locking clears the queue; the next unlock reads the sources again.

| News | Source | Title (body) | SHOW |
|---|---|---|---|
| Expiry (once a day after unlock, as before) | `useExpiryReminder` (D153) | "Passport expires in 12 days", or "3 items expire soon" ("Passport expires in 12 days.") | Health at Expiring |
| A sign-in to sign out of | `useAwayReminder` (D237) | "Sign out of Netflix on the hotel TV", or "2 sign-ins elsewhere to sign out of" ("First …") | the item; Needs attention when there are more |
| A site added a passkey by itself | `usePasskeyUpgrades` (D312) | "github.com added a passkey to this login" (the login's title) | the item |

## 6. The scrolled phone chrome (States / 02) and pull to refresh

- **Scrolled:** unicron's frame does it from the content's scroll (`useScrollMinimize`): the page head collapses to the small centred title, which stays the switcher; the wordmark hides and the L mark stays; the tab bar shrinks to the active tab; the strip joins the bottom row; a hairline appears under the top row; `AppFrameScrollEdge` fades the content into the ground with no blur. Lockdown writes nothing for it but the strip it hands the frame. A needs-you strip keeps its yellow in the row (Q6; X3's slice draws it).
- **Pull to refresh** (contract 5.1c C9): the shell puts the place's refresh in `ShellRefreshProvider`; `MasterDetail`'s list and `PageColumn` pass it to the frame's column as `onRefresh`. Items, Codes and Sharing sync (SYNCING in the strip while it runs); Health runs CHECK NOW and reads the report again. Elsewhere a column takes no pull.
- **Check again:** a `renew` icon button named "Check again" at the end of the head of Items, Codes and Sharing, through `PageHeadFromShell.actions` (cross-app §8). Health keeps its own CHECK NOW.

## 7. Tests and what is not proven

- `app/messages.test.tsx` (new, 18): the order of the live rows; the idle countdown on a desktop and a phone, in the live tone with ten segments and its action, said once while it ticks; opening and syncing in words; a screen's own line over the host's; toasts with no full stop, three at most, one action at most; each of the four critical Banners alone, with its one action, no close button, gone when it clears, no toast; only the first true one; the places each shows in; the news one at a time, dismissed for the day, never a toast, only on All items; on a desktop no status-line button and no toast for a Banner; on a phone the Banner after the chips; the scrolled phone (top and bottom rows minimized, the tab bar minimized, the strip inside the row, the small title as the switcher, the wordmark for a screen reader only); pull to refresh on Items, Codes and Health and not on Settings; no Check again on Health.
- Changed with their controls: `updates/restart-row.test.tsx` (the mark, never a restart button, nothing on the web), `updates/use-update-notices.test.tsx` (About opens once per asked check; no toast), `bridges/backup-indicator.test.tsx` (the failed backup in the core's words; off is not broken; a refused call says nothing; nothing asked while locked; the `backupLine` words, kept for Settings), `hygiene/hygiene.test.tsx` (the wipe words, no full stop), `passkeys/use-passkey-upgrades.test.tsx` (the news words).
- `apps/web/e2e/live.spec.ts`: a change from another device shows with no toast.
- `src/test/setup.ts` gains unicron's `matchMedia` stub (never matches): the shell now mounts the `Toaster`, whose sonner reads the colour scheme through it. Every unicron reader already treats a missing one as no match.
- **Not proven:** the desktop `App` wiring itself (no test mounts the Tauri `App`); the capture states cover it by eye. A real screen reader hearing the countdown once. WebKit's pull (unicron reads the bounce there). The tone of a needs-you strip in the scrolled row (X3).

## 8. Gaps

- The copy, delete, archive and favourite toasts are raised in the vault lane's files (`quick-copy.ts`, `row-menu.tsx`, `vault-view.tsx`); they keep today's sentences (less the full stop) until that lane takes `copiedWords`, `deletedWords` and `archivedWords` and UNDO.
- Health's own CHECK NOW does not show CHECKING in the status line, and Codes, Health, Search and Settings do not yet say their own line (`useStatusItem`).
- `BannerSlot` on Settings → Status and → Backups is the screens lane's.
- A web lint warning (`react(refs)`) on the web vault's Banner actions: the linter reads passing the sync function, which guards with a ref inside its own body, as reading the ref. It is a warning; `pnpm lint` passes.

### unicron host-contract gaps

1. **`StatusItem` draws a countdown, a bar and an action together**, while its doc says "at most one of". The idle lock needs all three (brief X2). Either the doc or the part should change; Lockdown uses all three.
2. **No way to raise a Banner from the frame.** Each app carries its own context to put a section's Banner under its page head; the head's `below` slot is the only way in. Ask: an `AppFramePageHead` `banner` slot, or a frame-level Banner place per column.
3. **The page head's small form holds no actions.** On a short screen (844 × 390) the head draws collapsed, so "Check again" is not on screen there and only the pull reaches a sync. Ask: the collapsed head keeps its `actions` (or the top bar takes them).
4. **`Toaster` needs `matchMedia` in tests** (sonner's theme); unicron's own setup stubs it, and an app finds out only when its tests fail. The migration guide could say so.

## 9. Proposed decisions

| Id | Decision | Status |
|---|---|---|
| D750 | **The idle-lock warning is live status, not a toast** (changes D107's warning). Ten seconds before the idle lock, the status line reads LOCKING IN // 08 S in the live tone with ten segments and STAY UNLOCKED; on a phone the live strip reads the same over "No activity" with STAY. It is said once, not every second. Plain version: the countdown before Lockdown locks itself sits where the app's live status always is, and a screen reader hears it once. | Proposed; **flagged for Sean** |
| D751 | **A device that must sign in again is a critical Banner on Items and Status,** "Lockdown isn't syncing", with the core's words and SIGN IN opening the account dialog. It replaces the error toast for a session ended elsewhere. Plain version: when this device stops syncing, it says so at the top of the list until you sign in. | Proposed; **flagged for Sean** |
| D752 | **A split view is a critical Banner on Items, Sharing and Status, until it clears** (changes D328's sticky toast). SEE STATUS opens Status. Plain version: the warning that your server may show you a different history can't be dismissed and lost. | Proposed; **flagged for Sean** |
| D753 | **The daily notices are news: one info Banner at the top of All items** (changes D153, D237, D312 from toasts). One at a time, SHOW and a close button; dismissed, it stays away for the day. The sign-out reminder's SHOW opens the item when there is one. Plain version: the "something expires" and "sign out there" notices wait at the top of your list instead of flashing by. | Proposed; **flagged for Sean** |
| D754 | **A failed backup is a critical Banner on Items and Backups** (answers Q13; changes D274's sidebar line). OPEN BACKUPS opens Settings → Backups; "Backed up 3 hours ago" lives in Settings. Plain version: a backup that stopped working is still seen, at the top of the list. | Proposed; **flagged for Sean** (Q13) |
| D755 | **An update is a mark on the way into Settings; RESTART TO UPDATE is in About** (answers Q11, cross-app §3; changes doc 60's sidebar restart row). No toast, strip or status-line button for an update. A check asked from the macOS menu opens About. Plain version: a ready update shows as a small mark on Settings, and you restart from About. | Proposed; **flagged for Sean** (Q11, cross-app question 3) |
| D756 | **A failed sync is a critical Banner on Items with TRY AGAIN,** until a sync works. The toasts for a failed sync are gone, on the desktop and in the web vault (where a session the server won't renew still locks the tab with its words). Plain version: when syncing fails, the list says so and offers to try again. | Proposed |
| D757 | **Toasts are for finished work only, in one line with no full stop, naming the thing,** three at most at the content's bottom left on a desktop and one at a time in the strip's place on a phone. "Saved Steam" for a save. Plain version: a toast says what just happened to what, and nothing else uses one. | Proposed |
| D758 | **Pull to refresh on Items, Codes, Health and Sharing, and "Check again" in the head of Items, Codes and Sharing.** A pull syncs (Health: CHECK NOW) and the strip reads SYNCING meanwhile. Plain version: pull the list down, or press Check again, to fetch what changed. | Proposed |
| D759 | **The scrolled phone chrome is unicron's, as States / 02 draws it,** except that a needs-you strip keeps its yellow in the row (Q6, the X3 slice). The status line's end on Items and Sharing reads HEALTH // NN TO FIX or HEALTH // ALL CLEAR; in the web vault, SYNCED 14:31. Plain version: scrolled, the phone's bars shrink out of the way; the desktop's top line says what Health has to fix. | Proposed |
