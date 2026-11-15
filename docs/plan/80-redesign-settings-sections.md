# 80: Settings sections: Browsers, Quick access, Appearance, Codes, Email aliases, Security checks

_Slice: `screens-5` of the 2026-11 redesign, on `wt/lockdown-screens` (from `claude/keen-ritchie-q2hgb2`, after doc 79), built 2026-11-15. No drawn frame; briefs T6 and T9 of the undrawn-areas brief (workspace `07-projects/29-unicron/impl-2026-11/app-lockdown-undrawn.md`); cross-app decisions §5, §12, §13; contract 5.1b #37. Decisions D880 to D889. Status: built in the lane; not merged._

## For future agent

- **TL;DR:** six short Settings sections, each inside doc 79's `SettingsFrame`, built from Field Rows, Toggle Rows, Section Headers, one critical `Banner` and two forms. Browsers (`features/browsers/browsers-section.tsx`): EXTENSION, BROWSERS ON THIS COMPUTER NN, PAIRED NN with REMOVE behind a Question. Quick access (`features/quick/quick-settings.tsx`): the shortcut switch with the key as a `Kbd`, the in-app key caption, DETAILS with OUT OF SWAP. Appearance (in `features/settings/settings-view.tsx`): found-1's THEME chips and DENSITY chips. Codes: edges-5's `ClockSetting`, as it is. Email aliases (`features/bridges/alias-settings.tsx`): SERVICES NN, ADD SERVICE (a form), the watch switch disabled with its reason without an account. Security checks (`features/security/security-settings.tsx`): the breach check, BREACH LIST with CHANGE (a form), the change-password check.
- **Do:**
  - Save a switch silently; a refusal keeps the old state and shows one critical line under the row (`SaveError`, doc 79). Toast only an action that has no state of its own (connect, remove, set up), per X2: no full stop, names the thing.
  - Keep `BrowsersSection`, `QuickSettings`, `AliasSettings` and `SecuritySettings` exports and props: Settings mounts them, and the vault lane's Health still mounts `SecuritySettings` until vault-8 takes it out (R13).
  - Add a row's name to `SETTINGS_INDEX` (`features/settings/settings-index.ts`) when a row is added.
- **Do not:**
  - Put the passkey offer back in Browsers: it is on This Mac (T1, doc 79 §2), and only there.
  - Toast a switch.
  - Edit `features/bridges/clock.tsx` from this lane (edges-5 owns it).
- **Read with:** doc 79 (the frame, `setting-rows.tsx`), doc 62 §3 (the theme choice), doc 67 (toasts, X2), docs 23 (bridges), 26 (quick access, memory), 29 (the browser host check).

## 1. Browsers (brief T6)

| Part | What shows | Data |
|---|---|---|
| Critical `Banner` (only when the core refused the browser host) | "Lockdown refused the browser host", then the core's `problem` and `peer_check.detail` | `EndpointStatus.peer_check.by === "refused"` |
| Field Row `EXTENSION` | "Can reach this app" with how the host is checked as the line (`peer_check.detail`); or "Can't reach this app" with the core's `problem` as the line; "Checking…" first | `EndpointStatus` |
| `BROWSERS ON THIS COMPUTER NN` | One Field Row per browser: a neutral `Badge` CONNECTED, or the action CONNECT. Header action CONNECT ALL when more than one is not connected | `EndpointStatus.browsers` |
| `PAIRED NN` | One Field Row per paired browser: its name, "Paired 2026.10.02" (`fullDate`), REMOVE. None: "None yet. A browser pairs once, with a code you compare here." | `PairedBrowser` |
| REMOVE | A Question (`ConfirmDialog`, destructive): "Remove {name}?" / "It can't fill or save logins until it pairs again, with a new code." Then a toast "{name} removed" | `unpair` |

CONNECT and CONNECT ALL toast "{name} connected" and "N browsers connected". The passkey switch that sat here is on This Mac only (doc 79).

## 2. Quick access (brief T6)

- Toggle Row "Open quick access from anywhere" with `HotkeyStatus.label` as a `Kbd` in the label (it is part of the switch's name). When the core reports a `problem` (a Wayland session, a shortcut taken), it is the row's description in critical text. A refused change keeps the switch as it was, with the core's words under it (before: a toast).
- Caption: "⌘K inside Lockdown opens it too." (`Kbd`; "Ctrl+K" off a Mac).
- `DETAILS` (a collapsible Section Header, shut; its summary is the value): one read-only Field Row OUT OF SWAP: "48 pages of keys", with "This computer refused N." and "Crash dumps are off." as its line; "Not on this build" with "This build can't keep memory out of swap." where the build has no locking.
- A shell with neither call (the web vault has no such section) draws nothing, as before.

## 3. Appearance (brief T6, Q21)

Kept from found-1 and doc 79: THEME with SYSTEM · STONE · GRAPHITE (light and dark only in the tips), kept on the device by next-themes (desktop) and `apps/web/src/theme.ts` (web, no inline script); DENSITY with COMFORTABLE · COMPACT (D92). This slice sets Density's line to the brief's "Compact fits more on the screen." Compact lands as `data-density="compact"` on the root (app/settings.tsx, foundation) and as `ListRow size="compact"` in the vault's rows (vault-2's `item-row.tsx` reads `settings.density`).

## 4. Codes (brief T6)

`ClockSetting` (edges-5's `features/bridges/clock.tsx`) as it is: the switch "Correct for a wrong clock" with today's sentence. It still draws its own heading "One-time codes" under the frame's "Codes" head and a horizontal `Field`; edges-5 restyles it (gap 1).

## 5. Email aliases (brief T9)

- Caption: "Give each site its own forwarding address from an alias service you use."
- `SERVICES NN`: one Field Row per service set up: its name, "14 aliases made", its domain (and own server) as the line, REMOVE. REMOVE asks: "Remove Relaymail?" / "Its key goes to Recently deleted. Lockdown makes no new aliases through it." Then `trash(item)` and a toast "Relaymail's key moved to Recently deleted". None: one line saying where aliases are made once a service is set up.
- ADD SERVICE (secondary): a form (`AdaptiveDialog`, form 640; a sheet on a phone): Service (`NativeSelect`, unicron's default for a form), API key (`SecretField mode="edit"`, with the service's hint), "Your domain" only where the service takes one, "Your own server (optional)" only where it can be self-hosted. Primary SET IT UP. The key leaves as bytes, which are wiped after the call; closing the form empties the field. Toast: "{Service}'s key is kept as a protected item".
- Toggle Row "Watch my aliases for data breaches", with today's sentence. Without an account it is disabled with its reason under it, "This needs an account." (#37), stays in the tab order and does not turn; the description is left out then, because the row dims it below the contrast floor (unicron gap 4). Turning it is silent (before: a toast each way).

## 6. Security checks (brief T9, R13)

- Caption: "Weak and reused passwords, two-step sign-in and look-alike websites are worked out on this computer. Nothing about them leaves it."
- Toggle Row "Check my passwords against known data breaches", with today's five-characters sentence.
- Field Row BREACH LIST: "The public Pwned Passwords list", or the address set. CHANGE opens a form (form 640): one Text Field "Breach list address" with today's hint, and SAVE LIST (it holds typed text, T0). Empty means the public list. While the breach check is off, the row is disabled with "The breach check is off." (#37; before: the field was greyed with no reason).
- Toggle Row "Find each site's change-password page before opening it", with today's sentence.
- The old heading "How these checks work" goes: the frame's section head says "Security checks".

## 7. Tests and what is not proven

- `browsers-section.test.tsx`: CONNECT installs that browser and toasts; CONNECT ALL only with two or more to connect; REMOVE asks first, Cancel unpairs nothing, the yes unpairs; "Can't reach this app" with the core's problem; the host check's sentence; a refused host is a critical Banner.
- `quick-settings.test.tsx` (new): the shortcut is a `kbd` inside the switch's name; turning it is silent; the core's problem describes the switch; a refused change keeps the switch with the core's words; DETAILS starts shut and holds OUT OF SWAP; a shell with neither draws nothing.
- `alias-settings.test.tsx`: the key goes as bytes and is wiped, from the ADD SERVICE form, which closes; fields a service doesn't take are not asked; an empty key is refused under the field; REMOVE asks first and trashes the key's item; watching is disabled with its reason without an account and does not turn; turning it on is silent.
- `security-settings.test.tsx` (new): both switches save at once with nothing said; BREACH LIST changes in its form with its own Save; the row is disabled with its reason while the check is off; a refused save keeps the switch with the core's words.
- `settings-view.test.tsx`: Compact switches a `ListRow` that reads the density to its compact size, and the root's `data-density`. The Theme chips' whole-app switch, reload and storage are found-1's tests (`theme-setting.test.tsx`, `apps/web/src/theme.test.ts`, which also checks no script or style element is written) and the web e2e (`web-vault.spec.ts`, under the page's CSP); they pass unchanged.
- Capture states (strict, `--sizes 1440x900,390x844,768x1024`): `desktop-settings-browsers`, `-browsers-refused`, `-browsers-remove`, `-quick-access`, `-quick-access-taken`, `-appearance`, `-codes`, `-aliases`, `-aliases-add`, `-aliases-no-account`, `-checks`, `-checks-list`. The theme states of doc 79 and `desktop-settings-compact` are unchanged.
- Not proven: a real browser connecting through a written manifest; a real Wayland refusal; a real alias service's key; a screen reader on the Kbd inside a switch's name.

## 8. Gaps

1. Codes: `ClockSetting` still draws its own h2 and a horizontal `Field` (edges-5's file). The section reads "Codes", then "One-time codes".
2. Browsers: the core does not say when a browser was last used, so PAIRED rows show only the pairing date.
3. Email aliases: the core does not report a service's last failure, so a service whose key stopped working looks like one that works until an alias is made.

## 9. Unicron host-contract gaps

1. `SecretField mode="edit"` is controlled only (`value`, `onChange`), so a typed API key lives in React state while the form is open. The old form read an uncontrolled input by ref. Screen: Email aliases → Add a service. Ask: an uncontrolled edit mode (`inputRef` read on submit, no `value`).
2. `ToggleRow` has no critical tone for its description; the core's shortcut problem is a span with the critical text role inside the description. Screen: Quick access when the shortcut is refused.
3. `FieldRow` `badge` with no value leaves the value column empty; fine on desktop, but on a phone (stacked) the badge sits alone under the label. Screen: Browsers on this computer at 390.
4. A `ToggleRow` disabled with a reason dims its description to half strength, which fails AA contrast (axe `color-contrast` on the no-account state). Screen: Email aliases without an account. The description is left out there until the row keeps it at full strength.

## 10. Proposed decisions

- **D880.** Browsers is three blocks of rows: EXTENSION; BROWSERS ON THIS COMPUTER NN (CONNECTED or CONNECT, CONNECT ALL); PAIRED NN (REMOVE). A refused host is a critical Banner at the top in the core's words. Seen by people (D38). Answers T6.
- **D881.** REMOVE on a paired browser asks first ("It can't fill or save logins until it pairs again, with a new code."), destructive, no gate. Before, it unpaired at once. Seen by people (D38). T6, X5.
- **D882.** The passkey offer is on This Mac only; Browsers no longer repeats it (T6 "moves to This Mac"). Seen by people (D38).
- **D883.** Quick access: the shortcut is a `Kbd` in the switch's name, the in-app key is ⌘K on a Mac and Ctrl+K elsewhere, and what is kept out of swap sits in a shut DETAILS as one row. A refused change is a critical line, not a toast. Seen by people (D38). T6.
- **D884.** Appearance keeps D700's chips; Density's line is "Compact fits more on the screen." (T6, D92). The words System, Stone, Graphite stand until Sean answers Q21. Seen by people (D38).
- **D885.** Email aliases: SERVICES NN rows with REMOVE behind a Question (the key goes to Recently deleted), and ADD SERVICE as a form with a `NativeSelect` and a `SecretField` (unicron's default select for a form). Seen by people (D38). T9.
- **D886.** The watch switch is disabled with "This needs an account." under it, not hidden or greyed with a sentence (#37). Seen by people (D38). T9.
- **D887.** Security checks: BREACH LIST is a row with CHANGE and a one-field form with SAVE LIST; disabled with "The breach check is off." while the check is off. The three settings stay here (R13). Seen by people (D38). T9.
- **D888.** No switch in these sections toasts; a refused save is one critical line under the row. Actions with no state of their own toast, without a full stop: "Google Chrome connected", "Firefox on Sam's MacBook Pro removed", "Relaymail's key is kept as a protected item", "Relaymail's key moved to Recently deleted". Seen by people (D38). T0, X2.
- **D889.** The API key is typed into unicron's `SecretField` (rule: secrets only through SecretField) at the cost of the value living in React state while the form is open (unicron gap 1); the bytes sent to the core are wiped after the call and the field empties on close. Not seen by people.
