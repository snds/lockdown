# Lockdown design brief: desktop, web vault and extension

_Status: v1.0, 2026-10-29. Brought up to date with build slices 13–29 at Sean's request: the screens added since v0.5 (§3.3–§3.6), the rules builders added (§6), the copy that decisions fixed (§10), and the gaps they recorded (§11). Questions 1–6 and 8 were decided by Sean on 2026-10-17; question 7 stands under D38. v0.5 (2026-10-17) covered the first 19 screens. Companion to `10-desktop-shell.md` (the Tauri shell as built). This page is the design contract for the desktop app, and for the web vault and the extension popup, which share its views; the six security components it depends on are specified in `snds/unicron` under `docs/specs/security/`. The design review of the built screens is in progress and will be `docs/design/2026-10-29-review.md` (§12)._

## For future agent

- **TL;DR:** The desktop app is a calm master–detail vault built from unicron primitives, with six security components doing the dangerous work (approve, reveal, copy, show a code, show large type, mark a crown jewel). Locked is the default state, and every proof of "locked, even from us" lives on one Status screen. Approvals and crown-jewel UI never render inside a web page. Since v0.5 the app grew a Security screen, a Sharing screen and about forty panels and dialogs (§3.3); the web vault (§3.4) and the extension (§3.5) reuse its views or follow its rules. §11 lists the unicron components the builders had to work around, and §12 points at the design review.
- **Reads before this:** `00-big-picture.md`, `02-model.md` §3–§4, `03-architecture.md` §8–§9, `04-security.md` §10–§12, decisions D14, D17, D47, D58, D61, D69, D70, D74, and unicron `DESIGN.md`, `docs/01-principles.md`, `docs/02-tokens.md`.

## 1. Positioning, carried into the UI

| Promise | What the desktop app shows | Where |
|---|---|---|
| **A. Every way you sign in** | One Login record holds the password, its passkeys, and its codes. The row says which methods exist (three small glyphs), and the detail lays them out as one account. Smart views cut across kinds (Passkeys, Codes, Wi-Fi, Expiring, Needs attention). | Vault list, Login detail |
| **B. Locked down, even from us** | Proof moments, one line each, with a "Check now" action that visibly re-verifies: the password never leaves this device (OPAQUE), the Secret Key the server has never seen, the signed vault history, the key-transparency check, the device list with revoke, the recovery drill result. No marketing panel. | Status screen, kit page, Devices |
| **C. Open, fair, yours** | Export is always there and never behind a plan. It asks only "is it you" (the password, D103), and a plain file needs an explicit "I understand this file won't be protected" (D142). The unlock screen has a server field, so a self-hosted server is a first-class choice, not a hidden setting. | Unlock, Settings → Import and export |

The tagline appears once, on the first-run screen. After that the app proves it instead of saying it.

## 2. Who the first user is

Sean, dogfooding the vault the CLI built (`07-workspace.md`), then the alpha wedge from the brief: gamers and multi-device households. For the desktop app that means a person who already has an account on another device, or who ran `lockdown init` and now wants a window. The first-run path therefore has two doors of equal weight: **create** (new account, kit, drill) and **join** (login name, password, Secret Key, then the other device approves). Neither door assumes the other app in the ecosystem is installed (D2).

## 3. Information architecture

Three columns at desktop width, two below `lg`, one below `sm` (the web UI is shared with the extension popup and web vault, so the narrow layouts are real, not theoretical).

As built (2026-10-29, `apps/desktop/src/app/shell.tsx`):

```
Sidebar                  List (master)              Detail
  Persona switcher         search (focused on        item header: kind glyph, title,
   (read-only for now)      launch)                   favourite, crown-jewel treatment
  All items · Favourites   Sort: title / recently    lookalike warning first (D130)
  Smart views               changed / recently used  fields by kind (§3.1)
    Passkeys · Codes ·      Group: by kind / none    websites with match rules
    Wi-Fi · Contacts ·     rows: kind glyph · title  passkeys · files · notes
    Expiring · Needs        · subtitle · methods ·   where it is (vault, folder,
    attention · Archived    star · crown-jewel mark   tags) · protection · sharing
    · Deleted               · row menu                history (dialog)
  Vaults (folders inside)  Find duplicates
  Tags
  ───
  Security · Sharing · Devices · Recovery · Status · Settings
  backup line · Lock
```

The web vault keeps the same three panes and swaps the bottom group for **Links · People · Devices · Settings** (no Security, Sharing or Recovery yet; §3.4). The extension popup is a single column of the same list rows (§3.5).

Codes get their own smart view rather than their own top-level area, because a code is usually part of a Login. The standalone authenticator feel comes from the Codes view plus the quick-access palette.

### 3.1 Detail layouts by item kind

| Kind | Detail layout | Components | Rules from `02-model.md` |
|---|---|---|---|
| Login | Methods block first (password, passkeys, codes), then URLs, then the rest | SecretField (password), CodeDisplay (each code), Badge (sign-in method) | Hub type; passkeys and codes are embedded |
| Passkey | Relying party, user name, created, last used, signal state | Badge (`hidden by site`, with Undo) | Hidden, never deleted, on a Signal |
| Code | CodeDisplay full width, issuer and account under it | CodeDisplay (6, 8, or Steam 5) | Warns when issuer equals the sync login |
| Wi-Fi | SSID, SecretField, guest QR card, "Show large type" | SecretField, LargeTypeView | QR card hides itself |
| SSH key, API credential | Public parts in plain text; private parts in SecretField; allowed hosts and expiry | SecretField | Never autofilled |
| Card, Bank account | Number masked in groups of four; CVC and PIN in SecretField | SecretField | Second deliberate action before any fill |
| Document, Identity | Attachment list, typed fields, expiry | Item, Badge (expiring) | Expiry reminders |
| Note | Markdown body | none | CRDT merge inside the body only |
| Crypto wallet, recovery codes, anything marked | Whole detail under the crown-jewel treatment; fields in SecretField with the password gate | CrownJewel, SecretField | Fresh biometric to use, full password to reveal |
| Custom | Field list from the template | SecretField for `hidden` kinds | Unknown kinds preserved |

### 3.2 Screen inventory

| Screen | Job | Primary action | unicron primitives | Security components | Empty / error / locked | Status (2026-10-29; what changed since v0.5) |
|---|---|---|---|---|---|---|
| Unlock | Open the vault | Unlock | Field, Input, Button, NativeSelect (server) | SecretField (password; Secret Key on a new device) | Error: wrong password, server unreachable (offline unlock still works). Locked is this screen. | built. Since v0.5: Create / Join / Recover tabs; "Join with another device instead" (code or QR, then six digits, D246); a new device may wait for a security key or an email approval (D250, D300); lock reasons in words ("Locked after a while without you."). The web vault has its own password-only unlock (D174) |
| First run: create | New account, kit, recovery picks | Continue | Dialog with steps (DialogTitle back), Field, Checkbox | SecretField (show the Secret Key once) | Error: weak password against the signed floor | built. Since v0.5: the kit test is a prompt right after the first run, and skipping it needs "I understand" (D251); an account without a Secret Key needs an explicit confirmation (D247) |
| First run: join | Enroll this device | Request approval | Field, Input, Spinner | ApprovalSheet (shown on the other device; this side shows the matching number) | Error: denied, timed out | built. Since v0.5: an enrolled device approves with matching digits and hands over the Secret Key sealed, so no kit is typed (D246, D247); the web vault never creates accounts (D176) |
| Kit page (D69) | Print or save the kit | Print | Dialog, Button | LargeTypeView (Secret Key and recovery code, printable) | Error: file exists, path refused | built (LargeTypeView print variant, both values) |
| Recovery drill | Prove the kit works | Start drill | Dialog, Field, Input | SecretField | Error: kit does not open, kit re-issued | built. Since v0.5: yearly and dated; checks the kit, the Secret Key kept here, each emergency contact's keys and which crown jewels they would get; a reminder prompt after unlock (D253); a password reminder every 14 days where unlock never asks for it (D252) |
| Vault | Browse and find | Open item | Item, ItemGroup, Command (search), ScrollArea, Badge | CrownJewel mark on rows | Empty: import or add. Error: sync failed banner. Locked: list blurred under `--surface-locked`, no titles readable | built. Since v0.5: Favourites, Contacts and Archived views; scoping by vault, folder or tag; sort by recently used; star, Archived badge and a row menu on each row; Find duplicates; Expiring in sections (D149, D151, D153, D241) |
| Item detail | Read and use one item | Copy / fill / reveal | Item, Button, IconButton, Tabs (history) | SecretField, CodeDisplay, CrownJewel, RiskTierBadge (bridge-backed items) | Empty: no fields of this kind. Error: field failed to decrypt (shows the item id, never a guess) | built. Since v0.5 it stacks many panels (§3.3): where it is, websites with match rules, sign-in methods, passkeys, files, sharing, protection, export opt-in, fill fix, signed-in-elsewhere, Wi-Fi guest card, large type on every hidden field, linked fields, conflict resolution. RiskTierBadge is wired but no item carries a tier |
| Item edit | Change an item | Save | Field, Input, Textarea, Switch, DialogActions | SecretField (edit mode) | Error: FieldError inline. Conflict: see Needs attention | built. Since v0.5: New item lists 30 templates in four groups plus your own templates (D231, D232); several websites, sign-in method, organization section, generator options (D230, D236), custom fields add One-time code and Link to another item (D235, D244), Hide your email address (D267) |
| Codes view | Every code, scanning fast | Copy | Item, Command | CodeDisplay (list density) | Empty: add a code or scan a QR. Locked: no codes drawn | built. Since v0.5: "Sign in a TV" opens the scan sheet (D311); a clock banner (D272); codes import from nine authenticator formats. Its empty copy is stale (§11.3) |
| Large type | Read across a room, type on a TV | Done | Sheet (fullscreen) | LargeTypeView | Hides on lock or blur | built. Since v0.5: on every hidden field (D242); the window is capture-protected while it shows (D316) |
| Quick-access palette | Global hotkey, act without the window | Copy or fill | Command, CommandDialog, Kbd | SecretField (copy only), CodeDisplay (compact) | Locked: palette asks to unlock first; crown jewels never listed | built. Since v0.5: one search index in the core with "Close match" (D305); Show code (Alt+Enter) and Fill in the browser (Shift+Enter) (D306); a separate global-hotkey window, Ctrl+Shift+Space / ⌘⇧Space, on by default (D307) |
| Approvals | Approve a device, an agent grant, a device code, or veto a contact request | Approve / Deny | Sheet, DialogActions | ApprovalSheet (number matching, context, "This wasn't me") | Error: timed out, rate limited. Never from a push that carries a code | Real approvals are host dialogs, not ApprovalSheet: browser pairing, passkey approval, add a device, TV sign-in, email approval on the server's page (§3.3). ApprovalSheet needs a `matching: none` mode before it can host them (§11.1) |
| Needs attention | Conflict copies (D58), expiring items, issuer-equals-login warnings, items hidden by Signal | Resolve | Item, Badge, Button | none | Empty: "Nothing needs you" | built. Since v0.5: also passkeys hidden by a site (D211) and due "signed in elsewhere" reminders (D237) |
| Devices | See and revoke devices | Revoke | Item, Badge, Dialog (confirm) | ApprovalSheet (enroll), CrownJewel (revoke needs a fresh check) | Empty: this device only; nudge to add a second in week one | built. Since v0.5: Add a device (D246), Signed in sessions with End and End all other sessions (D221), Security keys (D250) |
| Recovery | Kit status, drill, trusted contacts, Shamir later | Run drill | Item, Badge, Dialog | LargeTypeView (kit) | Error: drill failed | built. Since v0.5: the Emergency sheet (D274) and the Drill section. The "trusted contacts" row still says "later" though emergency access is built under Sharing (§11.3) |
| Status | The proofs, and sync health | Check now | Item, Badge, Spinner, Skeleton | none | Error: a check failed, in plain words, with what to do | built. Since v0.5: mirror lines and the split-view warning (D327, D328); the quantum-safe signatures line is being built (D346) |
| Settings | Lock timing, clipboard timer, extension pairing, appearance, server | Save | Tabs, Field, Switch, NativeSelect | none | Locked: not reachable | built. Since v0.5 the sections are: Lock, Clipboard, Appearance (density), Reveal, Approvals, Server, This server (restore drills, mirrors), Import and export, Backups, Browsers, Quick access, Email aliases, Clock |
| Import / export | Move data in and out | Import / Export | Dialog with steps, Field | CrownJewel (export needs the password) | Error: hostile file refused, with the reason | built. Since v0.5: 20 import formats with a preview of what can't come along; Restore from a backup; four export file kinds plus age (D273); crown jewels left out unless opted in (D137, D138); the plain-export acknowledgement (D142) |
| Agent activity (v2) | What agents did | Revoke grant | Item, Badge | RiskTierBadge | Empty: no agents yet | later |

### 3.3 Screens added since v0.5: desktop

Built by slices 13–29 and shared with the web vault unless §3.4 says otherwise. "States" lists what a designer has to draw, not every message. Copy is in §10.

| Screen | Where | Job | Primary action | States to design | Built from | Source |
|---|---|---|---|---|---|---|
| **Security** | sidebar screen | One report of what to fix, in three groups: Fix now, Fix soon, Worth doing | Check now | checking; all clear; findings by group; dismissed and snoozed (collapsed); facts in words ("The breach check is off."); error line; crown jewels by title only; credits footer | Item as stat tiles, ItemGroup, Empty, Spinner | 13; D125–D134, D261, D301 |
| Playbook ("What to do") | dialog from a finding | Short steps ending in how to confirm the fix | the finding's fix | steps didn't load | Dialog, plain ordered list | 13; D125 |
| Set-up guide | dialog from a missing two-step finding | What a site offers for two-step sign-in, and how | open the site's set-up page | no guide for this site | Dialog | 23; D261 |
| Accounts you sign in with | Security section | Which logins hang on Google, Apple, Microsoft…, and what falls with them | confirm an observation ("Yes, note it") | no account saved; no second step; not protected | Item, NativeSelect | 23; D264 |
| Something happened? | Security section | Seven guided incident flows filled from the vault | start a flow | in progress (this device only); done | Item, plain checklist | 23; D270 |
| Lookalike warning | item detail, first | Says a login's site imitates another | none (dismiss in Security) | draws nothing when fine | bordered `role=alert` section | 13; D130 |
| **Sharing** | sidebar screen | Your fingerprint; People; Vaults; Household; shared items; Share links; Request links; Emergency access | per section | offline (disabled); emergency-access banner; viewer sees no controls; crown-jewel vault explains why it can't be shared | Item, ItemGroup, Dialog, DropdownMenu, ToggleGroup, SecretField | 18; D185–D199 |
| People | Sharing section (web: its own screen) | Look up, compare fingerprints (read out, pasted code, camera), forget | Look someone up | nobody yet; not compared; verified; changed key refused | digits as text (no fingerprint component) | 18, 26; D190, D317 |
| Phrase check | dialog from People | Verify keys with nine digits said aloud | type their phrase | one try; mismatch; relay restarted | large mono digits | 22; D255 |
| Manage sharing | dialog per vault | Members, rights, add, remove, history | Add (Approve gate) | crown-jewel vault refused; viewer; look the person up first | Dialog, ToggleGroup, plain history list | 18; D186–D189, D196 |
| Household | Sharing section | A family vault with one organizer | Start a household | organized by someone else | Item, Button | 18; D194, D376 |
| Item Share menu | item detail | Share with someone, or as a link (right, lifetime, opens, password) | Share | never on a crown jewel; "Shared with you" on others' items; fill-only treatment | DropdownMenu, Dialog, ToggleGroup | 18; D187, D191, D192 |
| Link shown once | dialog | Show a new link one time and copy it | Copy | copied with countdown | Dialog | 18; D197 |
| Share and request links | Sharing sections | Make, list, delete, open a link, answer a request | Share some text / Ask someone for a secret | waiting; answered; gone | Field, Textarea | 18; D192, D193 |
| Opened link | panel | Read a link's values one at a time | Show / Copy | kept in vault | SecretField | 18 |
| Emergency access | Sharing section | Name verified contacts with a wait; follow each grant; stop a request | Add an emergency contact | invited, accepted, requested, ready, released, declined, revoked; "asked for access" with Stop | Item, NativeSelect, Checkbox | 18; D195, D254 |
| Vaults and tags trees | sidebar | Narrow by vault, folder, tag; manage them | New vault | shared vault icons and owner; trash grace | own nav row with depth spacer (no tree component) | 15; D145–D150 |
| Possible duplicates | dialog from Find duplicates | Merge same logins, trash other copies | Merge into "X"? | none found | Dialog, ItemGroup | 15; D152 |
| Folder and tags | dialog from row menu | File one item | Save | vault has no folders yet | Chip, Input with datalist (no tag input) | 15; D145 |
| Where it is, item menu | item detail | Vault, folder, tags, star; History, Archive, Move, crown-jewel on/off | — | leaving the crown-jewel class asks for the password | DropdownMenu, Dialog | 15; D148, D149 |
| Version history | dialog | Every version, which device, which fields | Restore | hidden values stay hidden; deleted for good | Dialog | 15 |
| Files | item detail | Attach, save, remove; storage used | Attach a file | none yet; over 100 MB; quota line; web refuses in words | hidden native input (no file picker) | 15, 21; D154, D234 |
| Your templates | dialog | Design a form of your own | Save template | none yet | Dialog, Field | 21; D232 |
| Generator options | editor | Characters or words, strength without a value | Generate | working out strength; site rules | inline fields (no Popover, Slider, Meter) | 21; D230, D236 |
| Link to another item | editor | Point a field at another item's field | Choose an item | broken link | Dialog | 21; D235 |
| Wi-Fi guest card | dialog "Join *network*" | A QR guests scan | Print | hides after 60 s, on lock and blur; never for fill-only | Dialog, own SVG QR | 21; D239 |
| Signed in elsewhere | item detail | Log a sign-in on a hotel TV and get reminded | Remind me | due (Needs attention and a toast) | Field, Button | 21; D237 |
| Protection | item detail | Fill freely / Ask each time / Keep to the app; emergency opt-in | choose a tier | default and its reason; asking less needs the password | NativeSelect, Switch | 22; D245, D254 |
| Include in exports and backups | crown-jewel detail | Per-item export opt-in | switch | on needs the password | Switch under CrownJewel | 14; D137 |
| Filling on this site | item detail (and popup) | Name a stubborn page's fields, or never fill there | Save | never fill here | Field, Switch | 23; D266 |
| Passkeys block | item detail | Provider, algorithm, synced or this device only, made, last used | Delete | only-way-in warning then Delete anyway; hidden by the site with Undo | Item, Badge, Dialog | 19; D211, D212 |
| Hide your email address | editor field | Make a forwarding alias for this site | New alias | needs a service first; Use gate each time | Field, Button | 23; D267 |
| Browser pairing | dialog, any time unlocked | "Connect *Chrome on Linux* to Lockdown?" with the code | Pair (Approve gate) | The codes differ; waits 2 min | Dialog (not ApprovalSheet) | 16; D156 |
| Passkey approval | dialog | "Sign in to *site* as *user*?" on password-only machines | Approve | Decline; waits 2 min | Dialog | 19; D208 |
| Reminders | dialog after unlock | Kit test, then password reminder, then yearly drill | type it back | Skip (needs "I understand"); Next week; Tomorrow | Dialog, Checkbox | 22; D251–D253 |
| Add a device | Devices | Approve a new device by code or QR and six digits | Approve (Approve gate) | they don't match; ten minutes, once | Dialog, own SVG QR | 22; D246 |
| Join from another device | unlock, web sign-in | Enrol with a code or QR, then compare digits | Join | not a Lockdown link; code shape hint | Field, Spinner | 22; D246, D247 |
| Security-key or email wait | unlock, web sign-in | A new device waits for a key or an email approval | wait | cancel; masked address | Spinner | 22, 25; D250, D300 |
| Signed in | Devices (web too) | Where the account is signed in | End | End all other sessions; this device marked | Item, Badge, Dialog | 20, 27; D221, D321 |
| Security keys | Devices | A second factor for devices the account has never seen | Add (opens the system browser) | Remove (Approve) | Item, Button | 22; D250 |
| Sign in a TV or console | dialog from Codes | Scan a TV's QR and open the real page | Approve with a login picked | Camera / Picture / Paste tabs; picture and paste refused; unknown activation page; passkey and Steam QR stop; waits 5 min, once | Dialog, Tabs (no ApprovalSheet) | 26; D309–D311 |
| Quick-access window | OS window, global shortcut | Find, copy, show a code, fill, without the main window | Enter opens in the main window | locked shows only Unlock; hides on blur; always capture-protected; no hotkey on Wayland | Command, CrownJewelMark | 26; D305–D308 |
| Emergency sheet | Recovery, printable | One page for a future you, with no secret | Print | prints alone | Dialog with a print rule | 23; D274 |
| Backup line | sidebar footer | Last backup at a glance | opens Settings | backed up *n* ago; no backup yet; off; late | Button | 23; D274 |
| Settings → Backups | Settings | Scheduled sealed backups and restore | Turn on backups | off; checking; crown jewels left out counted | Field, NativeSelect | 14; D143, D138 |
| Settings → Browsers | Settings | Connect browsers, paired list, passkey offer, how the peer is checked | Connect | can't reach; every host refused (error colour) | Field, Switch, Item | 16, 19, 29; D168, D205, D357, D361 |
| Settings → Quick access, Email aliases, Clock | Settings | Shortcut on or off; alias services; code clock correction | toggles | Wayland; needs an account; clock far off | Switch, Field | 23, 26; D267, D272, D307 |
| Settings → This server | Settings (web too) | The server's last restore drill; mirrors | Save mirrors | the server's word; no mirror compared yet; split warning | Field, Textarea | 27; D322, D328, D329 |
| OS notifications | the OS | A new device or a key change while in the background | — | macOS shows them only from a signed app | tauri-plugin-notification | 25; D302 |

### 3.4 Web vault

The web vault compiles the desktop's feature views (§3.2, §3.3) and adds these. It refuses import, export, backups, attachments, the Security screen, the recovery kit, passkeys, item and vault sharing, households and emergency access, in words (§11.2).

| Screen | Job | Primary action | States to design | Source |
|---|---|---|---|---|
| Sign in | Sign in to an existing account; each tab is a new device | Sign in | With a device tab; No account yet? tab; Secret Key needed; security-key step; email wait; rate limit | 17, 22; D173, D176, D257 |
| Unlock | A fresh sign-in with the password only | Unlock | locked while out of view; offline can't unlock | 17; D174 |
| Settings | Lock timers (no "Never"), clipboard, density, reveal, This server, the trust note | Sign out of this tab | settings last for the tab only | 17; D177, D180 |
| Links | Make text and request links; list; open answers | Share some text / Ask for a secret | shown once | 17; D182 |
| Link page (`/s/…`, `/r/…`) | Open a share link or answer a request, no account needed | Open (counts a view) / Send | views left; password; may not open again | 17; D182 |
| People | Contacts, as on the desktop | Look up | your code | 26; D317 |
| Key pages | Add a security key; confirm a new sign-in with one | Add a security key / Confirm | looking up; lost keys need the operator | 22; D257 |

### 3.5 Extension

The popup is on unicron. The in-page frames are plain HTML and CSS by design: they live in a closed shadow root in an extension-origin frame, armed only after 400 ms uncovered (§6).

| Surface | Job | Primary action | States to design | Source |
|---|---|---|---|---|
| Popup | Pair; logins for this page; search; copy | Fill / copy | not paired; pairing code; locked; app not reachable; results with Close match and From quick access; lookalike banner with Copy anyway | 16, 26; D156, D157, D166, D305, D306 |
| Popup → Fix this page | Name the page's fields, or never fill here | Save the fix | no fields to name | 23; D266 |
| Popup → Hide my email | Make an alias into the page's email field | New alias from *service* | platform check only, else use the editor | 23; D268 |
| Fill menu (in page) | Offer logins, passkeys, codes, cards; suggest a password; pair | pick a row (armed) | lookalike stop; covered; locked; unpaired; pairing code; protected (use the app); two heights only | 16, 19, 21, 27; D159, D166, D209, D236, D331 |
| Save / update prompt (in page) | Save a new login or update a password | Save / Update | Not now; held 3 minutes | 16; D161 |
| Passkey sheet (in page) | Create or use a passkey | Save passkey / pick then Sign in | already held; locked; refused; covered; checking it's you; Use another device or security key | 19; D205–D207 |

### 3.6 Self-host server pages

Plain server-rendered HTML with one hashed inline script; they don't use unicron. Operator-facing apart from the email approval page and the link notices.

| Page | Job | Primary action | Source |
|---|---|---|---|
| Admin sign-in | Operator key or passkey | Sign in | 25, 27; D294, D324 |
| Admin Overview, Accounts, Invites, Backups, Passkeys | Health and setup checks; accounts and sessions; invites while sign-ups are closed; backups and drills; operator passkeys | End every session of this account; Invite; Back up now; Run a restore drill now; Add a passkey | 25, 27; D294–D297, D323, D324 |
| Email approval | Approve or stop a new device from a mail link | Approve / This wasn't me | 25; D300 |
| Operator recovery kit | The Backup Key in captioned boxes and the restore steps | Print | 25; D298 |
| `/s/`, `/r/` notices | Shown when no web vault is served | — | 18; D198 |

## 4. The lock model as a UX

| Moment | What the person sees | Rule |
|---|---|---|
| Locked | The unlock screen. Behind it, the shell is visible but the list is a wash (`--surface-locked`) with skeleton rows: no titles, no icons, nothing to read. The window title drops the vault name. | The core enforces lock; the shell only draws it (`03-architecture.md` §8) |
| Auto-lock | Defaults: on sleep, on screen lock, after 10 minutes idle (desktop), on quit. Configurable 1 minute to 4 hours. A 10-second toast before an idle lock, with "Stay unlocked" (built, D107). | The PIN is a key slot, never a screen lock |
| Reveal | A SecretField reveal lasts 30 seconds, then masks itself. It also masks on window blur and on lock. One reveal at a time per screen. | Crown jewels need the full password to reveal |
| Copy | Copy shows a countdown on the control itself (ring, then "Cleared"). Default 30 seconds, range 10 to 90. The clear only runs if the clipboard still holds our value, so a later copy of the person's own text is never destroyed. Linux Wayland: say plainly that copy works only while Lockdown has focus. | `03-architecture.md` §8 clipboard row |
| Panic lock (v1.x) | A hotkey locks every window, clears the clipboard, and ends sessions. | Brief §2 novel features |
| Typed secrets | A half-typed password clears when the window hides, or after a minute out of focus, with a toast saying so (D315). | The webview holds typed strings; the core can't wipe them |
| Screen capture | Anything revealed makes the window capture-protected while it shows; the quick-access window always is (D316). | Windows and macOS; Linux has no call for it |

Lock is never animated as a transition the person waits for. Locked content disappears first, then the unlock screen draws (reduced motion: the same, with no fade).

## 5. Handoffs to surfaces we cannot style

| Surface | What it looks like | Our part | What we say beforehand |
|---|---|---|---|
| macOS AutoFill and the system passkey sheet | Apple's | The item list it shows comes from our identity store; our icon and item titles | In Settings → Autofill: "Passwords and passkeys appear in Apple's own prompt" |
| Windows Hello and the passkey plug-in dialogs | Microsoft's | Nothing visual | Same line, for Windows |
| Browser passkey dialogs (Chrome, Edge, Firefox) | The browser's | Nothing; on Linux and ChromeOS the override fallback shows our own sheet instead | Plainly: "On this platform Lockdown uses a fallback" |
| Extension fill menu | Ours, but in an extension-origin frame in the top layer | Full | n/a |
| Native file dialogs (kit save, import) | The OS's | Nothing | n/a |

The rule for every handoff: the last screen we own says what will happen next, in one sentence, and the first screen we own afterward confirms it happened.

## 6. Clickjacking constraints as design rules

1. In-page surfaces (the fill menu, the save prompt) are fully opaque, at least 240 × 44 px, and live in the top layer in an extension-origin frame. No page can draw over them or make them transparent.
2. Approvals, reveals of crown jewels, exports, and device enrollment happen only in the app window or an OS surface. The extension can request them; it cannot host them.
3. Passkeys, codes, and cards need a second deliberate action before any fill, by default. The second action is a visible control the person presses, never a hover or a timer.
4. Device codes are approved only when scanned from a screen in front of the person. The approval sheet has no path to render an approve control for a pasted or messaged code; the component's `origin` prop encodes this, not a setting.
5. Nothing security-relevant rides on color alone or on an icon alone. Every tier, state, and warning carries a word.

### 6.1 Rules the build slices added

These came out of slices 13–29. Each is a rule a new screen has to follow, with the decision that set it.

**Presence and clickjacking**
- In-page controls (menu rows, the save prompt, the passkey sheet, menu pairing) stay disabled until the frame has been visible and uncovered for 400 ms, go disabled again when covered, and ignore untrusted events (D159, D206, D331).
- The fill menu has two heights only, so a page learns nothing but "something to offer"; it closes on Escape, blur, typing, or when it has nothing to offer (D159).
- Signing in with a passkey takes two presses (pick, then Sign in); saving one takes one armed press (D206). Codes and cards always take a second press; an identity fills on one. A card can also ask before its security code (D238).
- A lookalike page gets a hard stop in the menu and a second, explicit "Copy anyway" in the popup; in the app the warning comes first and loudest (D130, D166).

**Gates** (what each one asks is in §4 and D101)
- **Approve:** share a vault or an item, put an item in a link, add to a household, name an emergency contact, pair a browser, approve a device, approve a TV sign-in, remove a security key (D156, D196, D246, D311).
- **No gate:** remove a member, stop a share, end a session, dismiss a finding, even on a crown jewel (D132, D196, D221). Taking access away never asks.
- **Reveal (the password):** let an item ask less, opt a crown jewel into emergency access or export, take an item out of the crown-jewel class, export, show a crown jewel's Wi-Fi card (D137, D142, D148, D239, D245, D254).
- **Use:** every alias made; every value out of an "Ask each time" item (D245, D267).
- The app window's plain confirm is never user verification (D207).

**Screen and clipboard hygiene**
- While a reveal, large type, the Wi-Fi card, a shown code or the scan sheet's code is drawn, the main window is capture-protected, so screenshots come out blank; the quick-access window always is. Linux has no such call, and a browser tab can't protect itself (D316).
- Copies are marked to stay out of clipboard history and cloud clipboard (D316). In the browser a copy goes through the page and may clear a later copy of your own (D178).
- Password fields clear when the window or tab hides, or after a minute out of focus, with a toast (D315).
- The Wi-Fi card hides after 60 seconds, on lock and on blur. The card, the emergency sheet and the kit page print alone (D239, D274).

**Camera and proximity**
- Only the camera leads to a yes. A picture file counts as a paste and is refused with no override (D310).
- The TV sign-in sheet shows the service, the full domain and the TV's code, waits five minutes and works once (D311).

**Focus, keys and timing**
- Search has focus whenever quick access opens: the palette, the quick-access window and the popup (D308).
- Quick access: Enter opens, Ctrl+Enter copies, Alt+Enter shows a code for 30 seconds, Shift+Enter fills in the browser. Ctrl+Shift+Space (⌘⇧Space) opens the window; Ctrl+Shift+L (⌘⇧L) fills in the browser (D163, D306, D307).
- A forgiven match says "Close match", and Ctrl+Enter copies it only when nothing matched exactly; words of three letters or fewer are never matched loosely (D305).
- Syncs started by the live channel are not activity for the idle lock (D222, D320).

**Notices**
- A daily toast after unlock for expiring items, and for due "signed in elsewhere" reminders (D153, D237).
- A split view raises an error toast that stays until dismissed; a session ended elsewhere raises one too, and a web tab locks (D321, D328).
- After acting on a finding: "Dismissed on all your devices.", "Snoozed until …", "It's back on the list." (D132).
- Decided, not built yet: a notice after a site adds a passkey quietly (D210, D312, changed by D375).

**Words over colour** (rule 5 above, carried through)
- Severities, exposure tiers, rights, "synced with your vault" or "on this device only", match rules and the peer check are all words; icons are decorative (D154, D187, D212, D245, D357).
- Fill-only items say "Fills in the browser only; never shown or copied" and offer no reveal, copy or large type (D187).

## 7. Accessibility targets

| Target | How |
|---|---|
| WCAG 2.2 AA across the web UI, checked per engine (WKWebView, WebView2, WebKitGTK) | axe plus manual keyboard pass per screen; target size 24 × 24 CSS px minimum (2.5.8), focus not obscured (2.4.11), no drag-only interactions (2.5.7) |
| Codes read digit by digit | CodeDisplay and LargeTypeView expose `aria-label` with the characters separated ("4, 8, 2, pause, 9, 1, 7"), never the number as a quantity |
| Reduced motion | No countdown animation, no next-code slide, no sheet travel; states still change (color plus text) |
| High contrast | `prefers-contrast: more` and Windows forced colors: borders become solid, the ring thickens, masked dots become a text placeholder, the crown-jewel treatment keeps its border and label when the inverted surface is lost |
| Keyboard everything | Palette, reveal, copy, approve, deny, and "This wasn't me" all reachable without a pointer; Escape never approves |
| Screen reader timing | Countdowns are not announced every second; a polite announcement at "about to expire" and at rollover, only while the control has focus |

## 8. Lockdown app-extension token roles (tier 2 aliases only)

Declared in `apps/desktop/src/styles/app-extension.css`, written as Mailbird's is. Each aliases a `--g-*` or `--*` value, swaps only in `.dark`, and is exposed through the file's own `@theme inline` block. No raw values.

| Role | Light alias | Dark alias | Used for |
|---|---|---|---|
| `--surface-chrome` | `--g-neutral-50` | `--g-neutral-900` | Sidebar and list pane (same name as Mailbird, so the siblings rhyme) |
| `--surface-canvas` | `--g-neutral-0` | `--g-neutral-950` | Detail pane |
| `--surface-selected` | `--g-neutral-100` | `--g-neutral-800` | Selected row |
| `--surface-locked` | `--g-neutral-100` | `--g-neutral-900` | The wash over the vault while locked |
| `--verified` | `--g-green-500` | `--g-green-400` | KT-verified, manifest verified, drill passed; always paired with a word |
| `--attention` | `--warning` (adopted in tokens 0.2.0) | `--warning` (the tokens swap it) | Needs attention, expiring, unverified |
| `--area-codes` | `--g-blue-500` | `--g-blue-400` | The one wayfinding hue, for the Codes view and the palette's code rows |

The six components take no app-extension tokens; their colors come from core tier 2 roles, the three the specs added (`warning`, `info`, `protected`) among them.

## 9. Open questions for Sean

| # | Question | Recommendation | Tradeoff |
|---|---|---|---|
| 1 | **Density.** One density, or a compact/comfortable toggle on desktop? | **Decided (Sean, 2026-10-17): start comfortable, and offer a density setting the person can change.** Earlier recommendation, superseded: one density for v1: unicron's compact default (32 px controls, 14 px body), with OS text scaling respected. Touch builds get bigger targets from their platform, not from a toggle. | A toggle doubles screenshot coverage across three engines. The brief's "density setting" reads as cross-platform, not per-user. |
| 2 | **Vault list.** One list, or grouped by kind? | **Decided (Sean, 2026-10-17): offer sorting and grouping options; default is alphabetical, grouped by type.** A Login still holds its passkeys and codes inside its group. Earlier recommendation, superseded: one list, kind shown as a glyph, with smart views and an optional "group by kind" sort. | A Login holds its passkeys and codes, so kind-grouping would split one account across groups. Grouping suits people with hundreds of Wi-Fi and card items; the sort option serves them. |
| 3 | **First run.** How much to explain? | **Decided (Sean, 2026-10-17): keep it succinct; more detail behind a dialog and progressive disclosure.** Compatible with: three screens at most (what this is and the tagline; the Secret Key and the kit; pick two recovery options), then explain at the moment of first use: first copy explains the timer, first crown jewel explains the class. The drill is scheduled, not forced, on day one. | Longer onboarding raises recovery setup; the brief says most people learn recovery fails at the worst moment. The drill reminder is the hedge. |
| 4 | **Proof moments.** Where does "locked, even from us" show? | **Decided (Sean, 2026-10-17): judicious and minimal; the proofs appear once, as part of first-time account creation, not repeated through the app.** The Status screen keeps "Check now" for re-verification. Earlier recommendation, narrowed: the Status screen owns every proof, one line each with "Check now". Two proofs also appear in place: the kit page says "we have never seen this key", and the unlock screen's help text says the password never leaves this device. | A dedicated "Security" tab reads as marketing. Proofs that re-run on demand read as true. |
| 5 | **Risk-tier words.** unicron's principles list `low / elevated / crown-jewel`; the plan set says `standards / official / tolerated`. | **Decided (Sean, 2026-10-17): the words must be generic and understandable by non-technical, non-security-minded people.** Neither vocabulary qualifies as shown to people; the badge carries plain words (proposed: `Built in` · `Verified` · `Use with care`), while `standard / official / tolerated` stay as the internal tier ids in the bridge manifest. Earlier recommendation, narrowed: the badge uses the bridge tiers (`standard`, `official`, `tolerated`); crown jewel is an item class with its own treatment, not a tier. unicron's principles row should be corrected when the specs land. | Two vocabularies for one badge is the thing the uniqueness rule exists to stop. |
| 6 | **Reveal gesture.** Tap-to-toggle with auto-hide, or press-and-hold? | **Decided (Sean, 2026-10-17): press-and-hold by default, with a setting to change it to toggle.** Release hides at once; the toggle setting keeps the 30-second auto-hide. Earlier recommendation, reversed: toggle with a 30-second auto-hide and hide on blur, as the default. Hold stays as a variant. | Hold shortens exposure but fails people who cannot hold a pointer still; WCAG 2.5.2 wants cancelable pointer actions. |
| 7 | **Crown-jewel "cannot imitate".** Is a visual treatment enough? | **Sean (2026-10-17): unclear what the point is; explained below, and the recommendation stands as a security-model call under D38.** The point: a web page can draw a convincing copy of any in-app panel to trick a person into revealing a crown jewel, so the guarantee is the rule (crown-jewel UI never renders in a page), and the treatment is the cue: an inverted `protected` surface, lock glyph, and the word. An optional per-profile seal (a word the person chose) can be added later. | Site-key images were shown to be ignored in 2007-era studies; a seal helps the careful few and costs a setup step. |
| 8 | **Device-code approval on desktop.** Phone-first, or also from the desktop camera? | **Decided (Sean, 2026-10-17): platform-first.** Approvals are confirmed with the platform's own biometric wherever it exists: Touch ID or Windows Hello on desktop, Face ID or Touch ID on phone, on whichever device the person is using. Desktop approves too; the proximity rule (scanned from a screen in front of the person) still governs device codes. Earlier recommendation, replaced: phone-first for v1. Desktop approves devices and agents, not TVs. | A laptop webcam can scan a TV, but the proximity rule is easier to hold to on a phone. |

## 10. Copy register

The wording these decisions fixed. Change words here and in the code together; the decision row records why.

| Where | Copy | D |
|---|---|---|
| Security groups | "Fix now" ("Someone could already be using these.") · "Fix soon" ("These make an account easier to break into.") · "Worth doing" ("Extra protection for when something goes wrong.") | D125 |
| Security empty | "All clear" | D125 |
| Security actions | "What to do", "Change password", "Turn it on", "Go to Devices", "Open item", "Dismiss, I've dealt with it", "Remind me in a week", "Remind me in a month", "I already use two-step sign-in here", "Set-up guide" | D125, D129, D132, D261 |
| Security sections | "Accounts you sign in with"; "Something happened?" | D264, D270 |
| Export | "Include in exports and backups"; "I understand this file won't be protected" | D137, D142 |
| Match rules | "This address only", "Anywhere on this site", "This site and its sister sites", "Pages starting with this address", "Never offer here" | D154 |
| Sign-in methods | "Password", "Passkey", "Another account", "Single sign-on"; "Signed in with" | D154 |
| Crown jewel class | "Keep as a crown jewel?" / "Stop keeping it as a crown jewel?" / "Stop protecting it" | D148 |
| Rights | "Fill only: Lockdown fills it in the browser; it is never shown or copied" · "View: see, copy and fill" · "Edit: see, fill and change items" · "Manage: change items and who has access" | D187 |
| Exposure tiers | "Who can get its values": "Fill freely" / "Ask each time" / "Keep to the app" | D245 |
| Pairing | "Connect *Chrome on Linux* to Lockdown?"; "Pair" / "The codes differ" | D156 |
| Save prompt | "Save this login for *site* in Lockdown?" / "Update the password for *site* in Lockdown?"; "Save" / "Update" / "Not now" | D161 |
| Lookalike | "Stop: *host* isn't *site*. It looks like a site you have a login for, so Lockdown won't fill here."; "Copy anyway" | D166 |
| Passkeys | "Save passkey"; "Sign in to *host* as *user*?"; "Use another device or security key"; "Use a passkey from another device"; "hidden by the site"; "Delete anyway"; "Offer Lockdown's passkeys in browsers" | D205, D206, D209, D211, D212 |
| Household | "A shared vault you organize. Billing isn't part of Lockdown." | D194, D376 |
| Web vault | "Lockdown web vault · *browser*" (device name); "No account yet?"; the trust note ending "For crown jewels and recovery, prefer the desktop app."; "Sign out of this tab" | D173, D176, D180 |
| New item | "Sign-ins and money", "Identities and documents", "Networks and machines", "Other"; "Your templates" | D231, D232 |
| Item extras | "One-time code", "Link to another item", "Show large type", "Ask before using the security code", "Hide your email address" / popup "Hide my email", "Fix this page", "Never fill on this site" | D235, D238, D242, D244, D266–D268 |
| Recovery prompts | "I understand an untested kit can fail"; "Tomorrow"; "I understand my password alone protects this account." | D247, D251, D252 |
| TV sign-in | "Sign in a TV or console"; "Only say yes if this is the TV or console in front of you, and it asked for this service."; "I'll sign in by hand" | D310, D311 |
| Quick access | "Close match"; "From quick access" | D305, D306 |
| Typed-secret wipe | "Lockdown cleared a password you were typing while the window was away." | D315 |
| Split view | "This server's public log doesn't match a mirror. …" (full sentence in D328) | D328 |
| Backup line | "Backed up 3 hours ago" / "No backup yet" / "Backups are off" | D274 |
| Quantum-safe line | proposed "Post-quantum signatures: on" / "Waiting for: *device*"; the build words it for people who have never heard the term | D346 |

## 11. Gaps the builders recorded

### 11.1 unicron components the shell worked around

Each was built locally in the app instead. They are requests for `snds/unicron`, grouped by component.

| Missing in unicron | Lockdown's workaround | Where it was found |
|---|---|---|
| Alert / Callout, and warning or severity tones on Icon, Badge and Item | bordered `role=alert` sections; words carry the severity | 13 |
| Stat tile | Item styled as tiles | 13 |
| Ordered steps, wizard / stepper | plain `ol`; Dialog with steps | 13, 14 |
| Collapsible | worked around in the view | 13 |
| File input, file picker, drop zone, save-to-path host call | a hidden native input; the OS save dialog | 14, 15 |
| Radio-card choice list | NativeSelect | 14 |
| Tree / nested nav row | the shell's own nav row with a depth spacer | 15 |
| Tag input | Chip + ChipRemove + Input + datalist | 15 |
| Markdown prose style | worked around in the view | 15 |
| Byte-size and relative-time formatting | formatted in the app | 14 |
| QR code, QR scanner | own SVG QR; camera view | 18, 21, 22, 26 |
| Fingerprint display, compare-digits | digits as text; large mono digits | 18, 22 |
| Pairing-code display (CodeDisplay counts down) | plain digits | 16 |
| Timeline | plain history list | 18 |
| Meter, Slider, Popover | inline fields and a strength word | 21 |
| Print region / Dialog print mode | a print rule per dialog | 21, 23 |
| ApprovalSheet `matching: none` | host Dialogs for every real approval | 26 |
| SecretField: copy-only host for strings; edit mode `masked` and `onWipe` | workarounds in the adapters | 18, 26 |
| Command item with several actions | one action per item plus key shortcuts | 26 |
| A plain-CSS token sheet | the extension's frames carry their own CSS | 16, 26 |
| Icon size; IconButton pressed state | worked around in the app | 15, 23 |
| Toaster that works under a strict CSP; DropdownMenuLabel outside a group | worked around in the web vault | 17 |

### 11.2 Not built, or partly built

- **Security:** no count in the sidebar; no Security screen in the web vault or the CLI; the quantum-safe line is being built (D346).
- **Organization:** no drag and drop for folders; no tag colours; no side-by-side duplicate compare; an old version's secret can't be revealed; Markdown has no tables or images; the persona switcher is read-only; leaving a shared vault isn't built.
- **Templates and generator:** your own templates have no sections or per-field generator options; the extension offers no generator options.
- **Passkeys:** the list shows Delete on shared items and the core refuses in words; you can't choose which login a new passkey goes to; approval dialogs don't raise the window.
- **Extension menu:** no keyboard navigation beyond Tab and Enter; no inline "Hide my email" in the menu.
- **Web vault:** refuses import, export, backups, attachments, the Security screen, the kit, passkeys, sharing, households and emergency access; no offline use.
- **Device approval:** no in-window security keys on desktop (the system browser opens); a server restart drops a pending link.
- **TV sign-in:** the desktop camera is untested on WebKitGTK, WKWebView and WebView2.
- **Platform limits:** Linux has no capture protection; Wayland has no global shortcut; Safari isn't built.
- **Never drawn yet:** ApprovalSheet in a real flow; RiskTierBadge on an item; Agent activity (later).
- **Decided on 2026-10-29, not built:** session renewal (D52), the passkey-upgrade notice (D210, D312), the new strength words (D230), pausing sharing on a split view (D328). D138 waits on Sean.

### 11.3 Copy that contradicts a decision

| Where | What it says | What it should follow |
|---|---|---|
| Item editor (`features/vault/item-editor.tsx`) | "Keep as a crown jewel … This can't be undone later." | D148 made the class reversible behind the password |
| Codes view empty state | "Scanning a QR code comes with the camera later." | The camera exists now (TV sign-in), though not for adding codes |
| Recovery, trusted contacts row | "Comes with the social-recovery slice", marked later | Emergency access is built under Sharing |
| Signed in elsewhere | "Signing in on someone else's screen?" | D237 says "Signing in on a TV?"; pick one |
| Kit-test skip | a plain "I understand" button | D251: "I understand an untested kit can fail" |

## 12. Design review

The built screens are being captured on desktop, web vault and extension, in light and dark, at 1280, 900 and 390 px, with an axe scan of each state, by a capture harness being added in `design/capture/`. The review against this brief, with a ranked list of fixes, is in progress and will be `docs/design/2026-10-29-review.md`. Fixes it accepts become rows in the status page's "Claude can build next" list.
