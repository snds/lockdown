# 75: Health, a finding of every kind, accounts, Something happened? and duplicates

_Slice: `vault-8-health` of the 2026-11 redesign, on `wt/lockdown-vault` (for `claude/keen-ritchie-q2hgb2`), built 2026-11-15. Frames Mobile / 09 and 10, Desktop / 07 and 08; briefs H1 to H4 and V3. The slice text named this doc 75 with D830 to D839. vault-7 took doc 75 (`75-redesign-item-states.md`) and D830 to D835, so this doc shares the number and uses the block's last four, D836 to D839; three more proposals are lettered (H-a to H-c) for the integrator to number at the fold._

Status: built. Health is a page with the count to fix as its hero; a finding, an item, Duplicates, an account or a guided flow opens beside the lists at the expanded width and as a pushed screen narrower; the settings moved to Settings → Security checks.

## For future agent

- **TL;DR:** `features/security/security-view.tsx` is Health on the desktop and, with `surface="web"`, in the web vault (`apps/web/src/features/web-security-view.tsx`). It holds what is open (`Open`: a finding, an item, Duplicates, Accounts, Something happened?) and lays it out itself: two plain columns at the expanded width (the frame's list width knob and `rule-l`), a pushed screen narrower (`usePushedInPage` tells the frame `detailOpen` and the way back). A finding's detail is `finding-detail.tsx`; its fix is `useFix` in `finding-actions.tsx`; the steps are `PlaybookSteps` in `playbook-dialog.tsx` (the item notice's dialog keeps `WhatToDo`). Duplicates is `features/organization/duplicates-review.tsx`, Accounts `features/bridges/federation.tsx`, Something happened? `features/bridges/incidents.tsx`, the set-up guide `features/bridges/setup-guide.tsx`, all read in place.
- **Read with:** briefs H1 to H4, V3, X2, X3, W1; cross-app §1, §8, §9, §10, §13; D125, D129, D132, D149, D152, D153, D261, D346; docs 23 (bridges), 30 (the quantum-safe line), 64 (places), 67 (messages), 68 (needs-you).
- **Do not:** draw a score, an "Old" count or a breach corpus size (the core has none); run `securityCheck()` anywhere but on open when the report is a day old, on CHECK NOW, or on a pull; merge without the Question; show a crown jewel's site.

## 1. The page (brief H1; Mobile / 09, Desktop / 07)

| Frame | Built as | Data |
|---|---|---|
| Overline `HEALTH // CHECKED 09:40` (phone), `HEALTH // PERSONAL + FAMILY` (desktop) | The hero Metric's label: when it was checked on a phone; the vaults on a desktop, where the status line says `CHECKED 09:40`. `HEALTH // CHECKING` while a check runs | `breach.checked_at`, else `generated_at`; the vaults |
| Hero `92 /100` | `Metric size="hero"`, `valueTone="primary"` at every value (Q9): the count to fix, `TO FIX` beside it. Read as "3 things to fix" | `toFixCount`: urgent + important findings + Needs attention rows (unchanged; the shell's nav marks read the same) |
| Weak, Reused, Old, Exposed | Weak (`OF 17 PASSWORDS`), Reused (`PASSWORDS`), Exposed (`IN BREACHES`, or `—` and `CHECK IS OFF`), No second step (`SITES`) | findings by kind |
| Fix first | `FIX NOW`, `FIX SOON`, `WORTH DOING` (D125's names), each with its sentence; more than five in Worth doing: `SHOW ALL 12` | findings by severity |
| — | `NEEDS ATTENTION` (CONFLICT, PASSKEY HIDDEN, SIGN OUT DUE, EXPIRES SOON, NEEDS ATTENTION) and `EXPIRING` (the kind and the date; IN 9 DAYS, IN 5 WEEKS, EXPIRED), soonest first (Q7) | `rowsIn(rows, "attention" / "expiring")` |
| — | `DISMISSED`, a collapsible Section Header, closed: when, or until when, and BRING BACK | `report.dismissed` |
| Good news | `PROTECTIONS`, Field Rows, each stating its state once: Breach check (`No breaches // checked 09:40`, or `01 breached`, with how many passwords were checked against which list; `Off` with TURN ON), Sign-in alerts, Passkeys, Backups (OPEN), Devices (OPEN), Quantum-safe signatures (the core's headline and explanation, each waiting device a link), Duplicates (`Not checked`, `02 groups`, `None`; FIND) | report, rows, `backupStatus()`, `status()`, `PqStatus`, `duplicates()` once opened |
| — | `MORE HELP`: Nav Items Accounts you sign in with, Something happened? (with how many flows are under way) | bridges client |
| — | `WHOLE LISTS`: Needs attention, in full and Expiring, in full: today's views with sort and group (found-3's entry rows, so nothing reachable before is lost) | places `attention`, `expiring` |
| — | The credits the code shows today (2fa.directory, the passkey directory) | report, `guideSources()` |

- **CHECK NOW** is a text button at the end of the hero's overline line. It runs `securityCheck()`; the status line and the overline read CHECKING meanwhile; the button keeps its words (busy). A pull on a phone runs the shell's check (CHECKING in the strip) and Health reads the report again.
- **States:** first load: skeleton rows. Nothing to fix: the hero `00`, still green, and an Empty State `HEALTH // 00`, "All clear.", "None of your 17 saved passwords is weak, reused or known from a breach."; Protections still show. A check failed over a report: the report stays, under a critical Banner "The check didn't finish." with the core's words and TRY AGAIN. The report could not be read: a critical Empty State with TRY AGAIN in place of the hero and the findings, never an all-clear; the lists and Protections that do not hang on it still show. Breach check off: Exposed `—`, `CHECK IS OFF`; the Breach check row `Off` with TURN ON (Settings → Security checks).
- **The settings leave Health (R13):** `SecuritySettings` is in Settings → Security checks since found-3 (screens-5 restyles it). Health no longer mounts it.
- **The status line** reads `CHECKED 09:40` on the page, `FIX 6 OF 9` on a finding, `CHECKING` while a check runs.

## 2. A finding of every kind (brief H2; Mobile / 10, Desktop / 08)

- **Layout:** beside the lists (Fix now, Fix soon, Worth doing, Needs attention, Expiring) at the expanded width, the row selected; a pushed screen narrower, with the frame's back. Crumb `TOOLS // HEALTH // {item}`; Health stays current.
- **Top to bottom:** `HEALTH // WEAK PASSWORD` (a phone: `HEALTH // FIX 1 OF 3`, as drawn); the item's title in the display role and its site in mono; a Banner with the finding's own title and whole sentence (critical when urgent or a look-alike, warning otherwise); Field Rows: the field as unicron's `SecretField` through vault-1's `ItemField` (reveal, copy, large type, capture protection as everywhere), LAST CHANGED from history only (the newest version that changed that field, else the first version; left out when history does not say), ALSO USED ON (each related item with OPEN, or "None"; LOOKS LIKE for a look-alike); WHAT TO DO, the playbook's steps in place; SET-UP GUIDE for a missing second step (in place; text links ACCOUNT RECOVERY PAGE and ABOUT PASSKEYS THERE; the sources).
- **Actions:** one primary by kind (OPEN {SITE} for weak, reused and breached, and an exposed email whose site has a page; TURN IT ON; OPEN ITEM for a look-alike; GO TO DEVICES for a new sign-in), SKIP FOR NOW (a menu on a desktop, a sheet on a phone: a week, a month, Dismiss, I've dealt with it / I already use two-step sign-in here; an account alert can only be dismissed), OPEN ITEM as text. On a phone the primary is the bottom row's float button (the drawn launch glyph, named), and SKIP FOR NOW's rows and OPEN ITEM sit behind the top bar's ⋯.
- **After SKIP FOR NOW:** a toast with D132's words and no full stop ("Snoozed until 2026.11.22", "Dismissed on all your devices") with UNDO (the core's restore) for an item's finding; the next finding opens.
- **A dismissed finding** (opened from Dismissed): an info Banner "Dismissed 2026.10.30." with BRING BACK, and no other action.
- **OPEN ITEM, an item in Needs attention or Expiring, ALSO USED ON:** the item opens inside Health with vault-1's `DetailPane` (its notice says what to do, brief I8); its Edit opens the shell's editor.
- The frame's sentence "Lockdown fills a new 24-character one" is not drawn: the steps are the core's, which say only what Lockdown does.

## 3. Accounts you sign in with (brief H3)

The list: the caption, `DID YOU SIGN IN? 01` (each sign-in the extension saw: YES, NOTE IT or YES, SAVE LOGIN, NO, and "Which login" when there are several), `ACCOUNTS 01` (a provider, how many sites, a warning word: NO LOGIN SAVED, NO SECOND STEP). The detail: `SIGNED IN WITH`, the provider, a warning Banner per warning from the core, YOUR {PROVIDER} LOGIN with OPEN (or a choice of candidates with USE THIS ONE), `SITES` each opening its item. Empty: "No accounts yet." Desktop only (the web has no bridges client).

## 4. Something happened? (brief H4)

The list: the flows with "Use this when…" and START, `2 OF 6` or DONE. The detail: `SOMETHING HAPPENED // 2 OF 6 DONE`, the flow, a segmented Progress (neutral, one segment per step), "Which email address" with START AGAIN WITH THIS, then unicron's `Steps`: a Checkbox per step ("Done: …"), a NEXT badge on the first open one, the step's sentence, its items as checkbox rows with OPEN, and one text action where it leads (GO TO DEVICES, GO TO RECOVERY, CHECK NOW, GO TO SIGNED IN). START OVER and "Progress is kept on this computer only." at the foot; "Every step is done." as an info Banner.

## 5. Duplicates (brief V3)

From the Duplicates row's FIND, on the desktop and in the web vault (`TOOLS // HEALTH // DUPLICATES`). The list: `POSSIBLE DUPLICATES 02`, a row per group (its words, how many items, SAME LOGIN / SAME PASSWORD / SAME CODE). A group that can merge: `Keep which?`, a RadioGroup of rows (the vault when there are several, `changed 2026.06.01`, OPEN), the newest chosen first; "Websites, codes, passkeys, notes and tags from the others join it. The others go to Recently deleted."; one MERGE (secondary; on a phone the bottom row's end), which asks "Merge into "Home Wi-Fi"?" with today's words, focus on Cancel, then runs through the gate (a crown jewel asks, D152). Toast "Merged into {site}" with no UNDO; a failed merge keeps the Question open with one critical line. A group that cannot merge: each copy with OPEN, and DELETE on every copy but the first (to Recently deleted, toast with UNDO). The web vault's list loses its Find duplicates button (it was there only because the web's Health had no way in, vault-2).

## 6. The web vault (brief H1, W1)

No hero, no Metrics, no Fix groups; one info Banner "Checks for weak, reused and breached passwords are in the desktop app."; Needs attention and Expiring; Protections with Passkeys, Devices, Quantum-safe signatures and Duplicates. The web reads its own rows and status (`web-app.tsx` is frozen and passes only `onDevice`).

## 7. Tests, and what is not proven

- `security-view.test.tsx` (rewritten, 9): the hero equals urgent + important + Needs attention rows and no score or `/100` shows; Metrics with No second step and no Old; a fresh report runs no network check; a finding in place (Banner, SecretField, LAST CHANGED from history, ALSO USED ON, the steps, OPEN {SITE} through the shell, SKIP FOR NOW); a week's snooze opens the next finding; TURN IT ON; the stale re-check; a failed check keeps the report under a critical Banner; an unread report never says all clear; all clear and the breach check off (TURN ON; the settings' switch is gone); BRING BACK.
- `pq-status-line.test.tsx` (unchanged, 8): the row keeps `role="status"`, its name, `data-state`, the icon and the device links.
- `duplicates-review.test.tsx` (rewritten, 3): Keep which? with the newest chosen, MERGE asks first with focus on Cancel and merges into the copy kept; a group that cannot merge (OPEN, DELETE but on the first); the web reaches Duplicates from Health.
- `federation.test.tsx` (2), `incidents.test.tsx` (2), `setup-guide.test.tsx` (3): the split views, as Health lays them out.
- e2e: `pq-status.spec.ts` (foundation's) through `steps/health.ts`, whose locators still hold.
- Harness: `states/vault.tsx`: `built` on desktop-security-filled (health--overview, drawn), desktop-security-playbook (health--weak-password, drawn), -empty, -checking, -error, -dismissed, -incident, desktop-vault-duplicates, and the new desktop-security-lists and desktop-security-federation. New: desktop-security-unread (the unread report), web-security-duplicates. The error state is now a failed check over a report.
- Not proven: a real breach check, a real extension sighting, a screen reader through the pushed screens, WebKit.

## 8. Gaps

- **Recovery kit row** (brief H1's table) is left out: Health would need a `Status.recovery` reading and the drill reminder in words; the Recovery section says it.
- **A place for Duplicates.** `places.ts` is frozen: FIND DUPLICATES after an import (screens-8) still lands on Health, one press from Duplicates.
- **The pushed screen inside a page.** Health is mounted in `PageColumn` (App.tsx, frozen), one frame column. Its two columns at the expanded width are plain layout with the frame's knobs; narrower, `usePushedInPage` sets the frame's `detailOpen` and back itself. If the integrator mounts Health outside `PageColumn`, it can move onto `AppFrameList` and `AppFrameDetail`.
- The phone's scrolled small title: Health draws no page head (the frames draw the hero under the overline), so the collapsed top row shows no title.

### unicron host-contract gaps

1. A pushed screen inside an `AppFrameColumn` page (no part; Health uses `setDetailOpen`/`setBack`). Screen: a finding, Duplicates, Accounts, Something happened? on a phone.
2. `AppFramePageHead` with a hero Metric in place of a title (so the phone's collapsed title still names the page). Screen: Health.
3. `FieldRow` has no icon slot for a state glyph; the quantum-safe row puts it in the value. Screen: Protections.
4. `Banner` warning in graphite reads close to the critical one at a glance (amber edge on a dark wash). Screen: a finding that is not urgent.

## 9. Proposed decisions

- **D836.** Health's hero is the count to fix (urgent + important findings + Needs attention rows), with TO FIX beside it, in the identity green at every value; the Metrics are Weak, Reused, Exposed and No second step. No score, no Old (Q8 a, Q9). Flag for Sean (D38): the frame draws 92 /100.
- **D837.** Needs attention and Expiring are lists on Health under the findings, and Duplicates is a Protections row that opens list + detail inside Health on the desktop and in the web vault (Q7). Changes where D149, D152 and D153 put them; the views stay reachable from places and the expiry news. Flag for Sean.
- **D838.** A finding opens beside the lists (pushed on a phone) with one primary by kind, SKIP FOR NOW as a menu with a toast and UNDO, OPEN ITEM as text; the playbook and the set-up guide are read in place (R13). The solid fix beside the sidebar's NEW ITEM follows Desktop / 08 (Q16). Flag for Sean.
- **D839.** Health's three settings live in Settings → Security checks only; Health shows Protections, each stating its state once, and the quantum-safe line is one of them (R13). Flag for Sean.
- **H-a.** Accounts you sign in with and Something happened? are list + detail places under Health's MORE HELP, crumbs `TOOLS // HEALTH // ACCOUNTS` and `// SOMETHING HAPPENED`; a flow's steps are unicron's Steps with a NEXT badge.
- **H-b.** A merge of duplicates is chosen with Keep which? (the newest first) and one MERGE that asks; a copy that cannot merge is deleted one at a time with UNDO (cross-app §10).
- **H-c.** The web vault's Health shows Needs attention, Expiring and Protections (Passkeys, Devices, Quantum-safe signatures, Duplicates) under an info Banner, and its item list loses Find duplicates.
