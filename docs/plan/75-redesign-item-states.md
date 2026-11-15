# 75: An item's states, large type and the Wi-Fi guest card

_Slice: `vault-7-item-states` of the 2026-11 redesign, on `wt/lockdown-vault` (for `claude/keen-ritchie-q2hgb2`), built 2026-11-15. Briefs I8 (item states) and I9 (large type and the Wi-Fi guest card); no drawn frame. The slice text named this doc 74 with D820 to D829. vault-6 took both (`74-redesign-item-details.md`, D820 to D828), so this is doc 75 with D830 to D839. The vault lane runs one doc and one block late; the integrator settles the numbers at the fold._

Status: built. The notice slot holds at most one Banner; a conflict is needs-you row 5; the guest card is a viewer with a Tile and unicron's QrCode; large type's viewer reads once per gate.

## For future agent

- **TL;DR:** `features/vault/detail-pane.tsx` draws the notice slot between the mono line and the Use zone: the Recently deleted Banner, else `ItemSecurity` (`features/security/item-security.tsx`), which picks the one Banner (a look-alike, the item's archived state, then its first finding) and a line `NN more in Health`. The conflict registers with `useNeedsYouSource` (row 5, `shows: { item }`); the shell's `NeedsYouSlot` draws it, and `WaitingOnYou` in the slot draws it as a row when something outranks it. `features/vault/wifi-card.tsx` is the guest card; `large-type.tsx` and `item-field.tsx` open unicron's `LargeTypeView`.
- **Read with:** briefs I8, I9, X3 (needs-you), X5 (questions, the gate), cross-app §1, §10, §13; D58, D98, D101, D106, D130, D239, D242; docs 21 (vault depth), 38 §3 (printing), 68 (needs-you), 69 (the frame).
- **Do not:** draw a second yellow shape for a conflict; run a conflict's answer without its Question; let `getSecret` of a gated large type change identity while the gate asks (it reads again with no proof, and asks again: §3); hold the guest card's grid after it closes.

## 1. The notice slot (brief I8)

At most one Banner, in this order. What else the security center says about the item is one line under it, `02 more in Health`, a link to Health.

| State | What shows | Action |
|---|---|---|
| The site looks like another (D130) | critical Banner: the finding's title, its detail and "Don't sign in there unless you're sure it's the real site." | `WHAT TO DO`: the finding's playbook, where its fix is |
| In Recently deleted | info Banner: "In Recently deleted. Deleted for good after {n} days." (`n` from the vault's `trash_days`) | `RESTORE` on a desktop; none on a phone, where the bottom row restores. Delete for good is the ⋯ menu's last row, a destructive Question that starts on Cancel |
| Archived | info Banner: "Archived. It still syncs and exports." | `UNARCHIVE` |
| Other findings | critical Banner for an urgent finding, warning otherwise: the finding's title and detail | `WHAT TO DO` |

- An item in Recently deleted loads no findings (as before), and shows no conflict.
- A site that added a passkey keeps its sentence on the passkey's row (vault-1's `PasskeysBlock`), not in the slot.
- Today's per-finding row with its fix button (`FindingFix`) is replaced by the Banner; the fix is in the playbook dialog that WHAT TO DO opens, and in Health. Nothing becomes unreachable.

## 2. A conflict (brief I8, X3 row 5, D58)

- **Desktop and medium:** the needs-you block at the top of the detail (found-6 places it above the item's overline): `CONFLICT // 2 COPIES`, "Two devices changed this item at the same time. The other copy: edited 2026.10.26.", `KEEP THIS COPY` solid, `KEEP OTHER COPY` text.
- **Phone:** the strip; `RESOLVE` opens the sheet with the same two answers. Nothing in the slot.
- **Outranked** (a sign-in request, say): the block shows the request with `// +1`; the conflict is a row under WAITING ON YOU 01 in the slot, its two answers as buttons.
- Each answer opens a Question (today's words; focus on Cancel). The loser goes to Recently deleted. KEEP OTHER COPY's yes says "Keep other copy" (was "Keep the other").
- Only for someone who may change the item, and never on a copy in Recently deleted.

## 3. Large type (brief I9, D242)

- **Entry:** the visible `SHOW LARGE TYPE` on a Wi-Fi password and on a one-time code item; on every other hidden field the first row of its ⋯ (vault-1, D773), by tap, click or keyboard.
- **Layout:** unicron's `LargeTypeView`: viewer 960 on a desktop, full screen on a phone, `DONE` primary, the title names the thing ("Code · Driftwood · sam_r"), never the value. Hides after two minutes, on blur and on lock; capture protection stays on while it shows.
- **A crown jewel's asks first.** Fixed here: the viewer's `getSecret` was rebuilt every time the gate's state changed, so once the gate closed the viewer read again with no proof left and the gate asked a second time (seen in a test as five reads for one reveal). `item-field.tsx` and `large-type.tsx` now hold the gate's `run`, which does not change while it asks: one read without proof, one with.

## 4. The Wi-Fi guest card (brief I9, D239)

- `GUEST CARD` on a Wi-Fi item; not on a fill-only share or in Recently deleted (unchanged).
- An `AdaptiveDialog` at `viewer` size, full height on a phone. Overline `WI-FI // GUEST CARD`, title "Join {network}". A `Tile` (its primary top edge) holds the network's name and unicron's `QrCode` (240, from the core's grid: no JavaScript encoder), drawn dark on light in both themes. Caption "Scan with a phone camera to join." The live line `HIDES IN 59 S` (`MM:SS` from a minute). `PRINT CARD` secondary, `DONE` primary and focused.
- Kept: hides after a minute, on blur and on lock; the minute stops while a print panel is up; the grid is wiped when it closes; printing reveals the password once more through the gate and draws it only until printing is over.
- **The print** keeps the existing `[data-print-card]` rules in `app-extension.css` (the card alone, black on white, whatever the theme), not `[data-print-page]` as the slice text says: `[data-print-page]` prints the whole dialog it sits in, and AdaptiveDialog takes no attribute for its popup. The paper shows "Wi-Fi: {network}", the password in `DigitGroups` and the code.

## 5. Crown jewels, tiers, fill-only

Unchanged and checked: the Use zone in `CrownJewelSurface` (`ItemCrownJewel`), the gate through `useGate` (`requestProof` with "To use/show {title}"), `RiskTierBadge` beside the title, no SHARE on a crown jewel. `desktop-item-wallet-revealed` now waits for the gate to arm before it presses Confirm (it had pictured the gate still asking); its picture is not sent, see Gaps 6. Fill-only shares draw "Fills in the browser only; never shown or copied." and no large type, guest card or Files. These states now carry `built`.

## 6. Pictures

`pnpm shots:built` (undrawn): `item--lookalike`, `item--conflict`, `item--conflict-confirm`, `item--trashed`, `item--purge`, `item--fill-only`, `item--crown-jewel`, `item--tier`, `item--large-type`, `item--large-type-code`, `item--wifi-card`, `item--web-crown-jewel`; phone and desktop, graphite and stone. `desktop-item-wallet-gate` is found-6's.

## 7. Tests, and what is not proven

- `features/security/item-security.test.tsx` (rewritten, 3): one critical Banner for a look-alike with the rest as one line that goes to Health, WHAT TO DO opens the playbook; urgent critical and others a warning; the state outranks a finding but not a look-alike.
- `features/vault/item-states.test.tsx` (new, 7): a conflict is one needs-you block (row 5) on its item, each answer a Question starting on Cancel, keep-this calls `resolveConflict(this, other)`; a request outranks it and it becomes a WAITING ON YOU row, never a second block; nothing on a trashed copy; a purge asks first and Cancel destroys nothing; Show large type is the first row of each hidden field's ⋯ by tap and by keyboard alone (D242); the visible action on a Wi-Fi password.
- `features/vault/crown-jewel.test.tsx` (+1): a crown jewel's large type asks first, then shows; read exactly twice (without and with the proof), no second ask.
- `features/vault/vault-depth-detail.test.tsx`: the guest card's controls renamed (Print card, the QR's name, `Hides in 01:00`); the words-gone checks read the whole page's text, since DigitGroups splits the password into groups.
- `apps/web/e2e/steps/items.ts`: `hideGuestCard` presses the dialog's Done; `guestCode` reads "QR code to join …".
- Harness: `states/vault.tsx` (`built` on 13 states; notes; the revealed state waits for the gate to arm), `states/steps/vault.ts` (`keepConflictCopy`: the block on a desktop, the strip's sheet on a phone).
- **Not proven:** the paper itself (no picture of a print; the CSS is unchanged); the Touch ID prompt over the viewer; a screen reader through the needs-you sheet.

## 8. Gaps

- `qrPath` in `depth-model.ts` is now used only by its test; the integrator may archive both with `features/depth/qr-code.tsx` (edges), which `lookalikes.cjs` reports as a look-alike of unicron's QrCode.
- A trashed item's fields keep COPY on their rows (the brief's "no copy on its row" read as the list row).
- found-6's block sits flush on the item's overline at every width (no gap): the slot's spacing is master-detail's.

### unicron host-contract gaps

1. `NeedsYouEntry` (foundation) and `NeedsYou` have no mono meta line: "The other copy: edited NOV 02" joins the body sentence. Screen: a conflict.
2. No segmented countdown part for a hiding layer; the guest card's `HIDES IN 59 S` is a plain overline line. Screen: the guest card.
3. `LargeTypeView` aligns groups at their foot, so a group with no caption sits lower than its captioned neighbors ("719 / 649 / 45"). Screen: large type of a code.
4. `AdaptiveDialog` takes no attribute for its popup, so a viewer cannot be a `[data-print-page]`. Screen: the guest card's print.
5. `QrCode` draws on the stone ground and its ink, not pure white and black, on screen and on paper.
6. A revealed `SecretField` inside `CrownJewelSurface` is unreadable: its reveal wash and its ink take the same remapped color (light on light in graphite, dark on dark in stone). Screen: a crown jewel, revealed (`desktop-item-wallet-revealed`). Not sent to Sean until unicron fixes it.

## 9. Proposed decisions

- **D830.** The notice slot holds one Banner (look-alike, Recently deleted, archived, then other findings) and a line `NN more in Health`; a finding's fix is behind WHAT TO DO. Answers I8. Flag for Sean (D38).
- **D831.** A conflict is needs-you row 5 with KEEP THIS COPY solid and KEEP OTHER COPY text, a WAITING ON YOU row when outranked; both answers ask. Changes D58's look. Flag for Sean.
- **D832.** The look-alike Banner's title is the finding's own ("Website looks like another site"), replacing "Careful: this website looks like another site". Flag for Sean.
- **D833.** The guest card is a viewer with the code on a Tile, HIDES IN as a live line, PRINT CARD and DONE (focused). Answers I9. Flag for Sean.
- **D834.** The printed card shows the password in DigitGroups (groups of three, with gaps a guest does not type). Question for Sean: groups or one run.
- **D835.** A gated large type reads once with its proof (the viewer's read is held stable while the gate asks). A fix, not a change a person sees except that the gate asks once.
