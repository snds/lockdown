# Lockdown desktop design brief

_Status: v0.4, 2026-10-17 (questions 1–6 and 8 decided by Sean; 7 explained, recommendation stands under D38). The screen inventory (§3.2) carries a status column for what the shell has built, now on unicron's six security components; D100–D109 record the security-model choices made building it. Companion to `10-desktop-shell.md` (the Tauri scaffold, written by another session). This page is the design contract for the desktop app; the six security components it depends on are specified in `snds/unicron` under `docs/specs/security/`._

## For future agent

- **TL;DR:** The desktop app is a calm master–detail vault built from unicron primitives, with six security components doing the dangerous work (approve, reveal, copy, show a code, show large type, mark a crown jewel). Locked is the default state, and every proof of "locked, even from us" lives on one Status screen. Approvals and crown-jewel UI never render inside a web page. Open questions for Sean are at the end, each with a recommendation.
- **Reads before this:** `00-big-picture.md`, `02-model.md` §3–§4, `03-architecture.md` §8–§9, `04-security.md` §10–§12, decisions D14, D17, D47, D58, D61, D69, D70, D74, and unicron `DESIGN.md`, `docs/01-principles.md`, `docs/02-tokens.md`.

## 1. Positioning, carried into the UI

| Promise | What the desktop app shows | Where |
|---|---|---|
| **A. Every way you sign in** | One Login record holds the password, its passkeys, and its codes. The row says which methods exist (three small glyphs), and the detail lays them out as one account. Smart views cut across kinds (Passkeys, Codes, Wi-Fi, Expiring, Needs attention). | Vault list, Login detail |
| **B. Locked down, even from us** | Proof moments, one line each, with a "Check now" action that visibly re-verifies: the password never leaves this device (OPAQUE), the Secret Key the server has never seen, the signed vault history, the key-transparency check, the device list with revoke, the recovery drill result. No marketing panel. | Status screen, kit page, Devices |
| **C. Open, fair, yours** | Export is always one click and never gated. The unlock screen has a server field, so a self-hosted server is a first-class choice, not a hidden setting. | Unlock, Settings → Export |

The tagline appears once, on the first-run screen. After that the app proves it instead of saying it.

## 2. Who the first user is

Sean, dogfooding the vault the CLI built (`07-workspace.md`), then the alpha wedge from the brief: gamers and multi-device households. For the desktop app that means a person who already has an account on another device, or who ran `lockdown init` and now wants a window. The first-run path therefore has two doors of equal weight: **create** (new account, kit, drill) and **join** (login name, password, Secret Key, then the other device approves). Neither door assumes the other app in the ecosystem is installed (D2).

## 3. Information architecture

Three columns at desktop width, two below `lg`, one below `sm` (the web UI is shared with the extension popup and web vault, so the narrow layouts are real, not theoretical).

```
Sidebar                  List (master)            Detail
  Persona switcher         search (focused on      item header: kind glyph, title,
  All items                 launch, as Ente does)    favorite, crown-jewel treatment
  Smart views              rows: kind glyph ·       fields by kind (§3.1)
    Passkeys · Codes ·      title · subtitle ·       SecretField · CodeDisplay
    Wi-Fi · Expiring ·      method glyphs ·          URLs with match rule
    Needs attention ·       crown-jewel mark         notes, tags, attachments
    Deleted                sort: recent / title     history, "this device only"
  Vaults, folders, tags
  ───
  Devices · Recovery · Status · Settings
```

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

| Screen | Job | Primary action | unicron primitives | Security components | Empty / error / locked | Status (shell, 2026-10-17) |
|---|---|---|---|---|---|---|
| Unlock | Open the vault | Unlock | Field, Input, Button, NativeSelect (server) | SecretField (password; Secret Key on a new device) | Error: wrong password, server unreachable (offline unlock still works). Locked is this screen. | built |
| First run: create | New account, kit, recovery picks | Continue | Dialog with steps (DialogTitle back), Field, Checkbox | SecretField (show the Secret Key once) | Error: weak password against the signed floor | built (3 screens + the proof panel, D94/D95) |
| First run: join | Enroll this device | Request approval | Field, Input, Spinner | ApprovalSheet (shown on the other device; this side shows the matching number) | Error: denied, timed out | built (the form; the other device's approval sheet is unicron's) |
| Kit page (D69) | Print or save the kit | Print | Dialog, Button | LargeTypeView (Secret Key and recovery code, printable) | Error: file exists, path refused | built (LargeTypeView print variant, both values) |
| Recovery drill | Prove the kit works | Start drill | Dialog, Field, Input | SecretField | Error: kit does not open, kit re-issued | built (kit verify + drill record) |
| Vault | Browse and find | Open item | Item, ItemGroup, Command (search), ScrollArea, Badge | CrownJewel mark on rows | Empty: import or add. Error: sync failed banner. Locked: list blurred under `--surface-locked`, no titles readable | built (sort / group per D93; CrownJewelMark; the locked wash) |
| Item detail | Read and use one item | Copy / fill / reveal | Item, Button, IconButton, Tabs (history) | SecretField, CodeDisplay, CrownJewel, RiskTierBadge (bridge-backed items) | Empty: no fields of this kind. Error: field failed to decrypt (shows the item id, never a guess) | built on the unicron components (Login, Code, Wi-Fi, Note, Card layouts; others generic; RiskTierBadge wired, no item carries a tier yet; Restore in Deleted) |
| Item edit | Change an item | Save | Field, Input, Textarea, Switch, DialogActions | SecretField (edit mode) | Error: FieldError inline. Conflict: see Needs attention | login only |
| Codes view | Every code, scanning fast | Copy | Item, Command | CodeDisplay (list density) | Empty: add a code or scan a QR. Locked: no codes drawn | built (CodeDisplayList, next code in the last seconds) |
| Large type | Read across a room, type on a TV | Done | Sheet (fullscreen) | LargeTypeView | Hides on lock or blur | built (Wi-Fi hidden fields, a Code's current code; 120 s) |
| Quick-access palette | Global hotkey, act without the window | Copy or fill | Command, CommandDialog, Kbd | SecretField (copy only), CodeDisplay (compact) | Locked: palette asks to unlock first; crown jewels never listed | later |
| Approvals | Approve a device, an agent grant, a device code, or veto a contact request | Approve / Deny | Sheet, DialogActions | ApprovalSheet (number matching, context, "This wasn't me") | Error: timed out, rate limited. Never from a push that carries a code | gate on CrownJewelGate (D101, D106); ApprovalSheet bound and tested as a seam, drawn once the core carries requests |
| Needs attention | Conflict copies (D58), expiring items, issuer-equals-login warnings, items hidden by Signal | Resolve | Item, Badge, Button | none | Empty: "Nothing needs you" | built (view; conflict copies resolve by keeping one, D108) |
| Devices | See and revoke devices | Revoke | Item, Badge, Dialog (confirm) | ApprovalSheet (enroll), CrownJewel (revoke needs a fresh check) | Empty: this device only; nudge to add a second in week one | built |
| Recovery | Kit status, drill, trusted contacts, Shamir later | Run drill | Item, Badge, Dialog | LargeTypeView (kit) | Error: drill failed | built (contacts later) |
| Status | The proofs, and sync health | Check now | Item, Badge, Spinner, Skeleton | none | Error: a check failed, in plain words, with what to do | built (proofs + Check now) |
| Settings | Lock timing, clipboard timer, extension pairing, appearance, server | Save | Tabs, Field, Switch, NativeSelect | none | Locked: not reachable | built |
| Import / export | Move data in and out | Import / Export | Dialog with steps, Field | CrownJewel (export needs the password) | Error: hostile file refused, with the reason | built (CXF, CSV) |
| Agent activity (v2) | What agents did | Revoke grant | Item, Badge | RiskTierBadge | Empty: no agents yet | later |

## 4. The lock model as a UX

| Moment | What the person sees | Rule |
|---|---|---|
| Locked | The unlock screen. Behind it, the shell is visible but the list is a wash (`--surface-locked`) with skeleton rows: no titles, no icons, nothing to read. The window title drops the vault name. | The core enforces lock; the shell only draws it (`03-architecture.md` §8) |
| Auto-lock | Defaults: on sleep, on screen lock, after 10 minutes idle (desktop), on quit. Configurable 1 minute to 4 hours. A 10-second toast before an idle lock, with "Stay unlocked" (built, D107). | The PIN is a key slot, never a screen lock |
| Reveal | A SecretField reveal lasts 30 seconds, then masks itself. It also masks on window blur and on lock. One reveal at a time per screen. | Crown jewels need the full password to reveal |
| Copy | Copy shows a countdown on the control itself (ring, then "Cleared"). Default 30 seconds, range 10 to 90. The clear only runs if the clipboard still holds our value, so a later copy of the person's own text is never destroyed. Linux Wayland: say plainly that copy works only while Lockdown has focus. | `03-architecture.md` §8 clipboard row |
| Panic lock (v1.x) | A hotkey locks every window, clears the clipboard, and ends sessions. | Brief §2 novel features |

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
