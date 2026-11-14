# 69: the item detail: the frame, every kind, and websites, passkeys and codes on an item

_Slice: `vault-1` of the 2026-11 redesign, on `wt/lockdown-vault` (for `claude/keen-ritchie-q2hgb2`), built 2026-11-14. Briefs I1 (the detail frame), I2 (layouts for every kind) and I3 (websites, passkeys and codes on an item) of the undrawn-areas brief; the drawn frames Mobile / 02 to 05 and Desktop / 01 to 04 (their detail halves). The slice text named this doc 68 with D760 to D769; found-6 took both (`68-redesign-needs-you.md`, which keeps D760 to D769 for itself), so this is doc 69 with D770 to D779. The vault lane's later slices move one doc and one block on; the integrator settles the numbers at the fold._

## For future agent

- **TL;DR:** `features/vault/detail-pane.tsx` draws one item as the brief's I1 frame: the overline (kind // what tells it apart // state), the title in the X6 role with `CrownJewelMark` and `RiskTierBadge` beside it, a mono line (first site's host // edited date), one notice slot, the Use zone, then a shut `Details` SectionHeader. Every field is a unicron Field Row by its template control (`item-field.tsx`); hidden values are `SecretField`; codes are `CodeDisplay layout="field"`; passkeys are rows with a details dialog (`passkeys-block.tsx`); a card's art is a `Tile`; a note's body is `Prose`; SSH and PGP public keys are `CodeBlock`s.
- **Read with:** the undrawn brief §4 I1 to I3, I8 (the notice slot), I4 (what Details will hold); doc 65 (the phone's pushed chrome hooks); doc 67 (`notify`); doc 68 (the Question rule).
- **Do not:** compose a secret from `FieldRow` and `Button` (always `SecretField` through the host in `item-field.tsx`); copy a code outside `ItemCodeDisplay` or the row's quick copy; add a row to the ⋯ menu that the person's access does not allow; edit `item-share.tsx`, `away-block.tsx`, `large-type.tsx` or `place-panel.tsx` from here (other slices own them).

## 1. The frame (brief I1)

| Part | Built |
|---|---|
| Overline | `overlineOf()` in `detail-model.ts`: the kind's word; the template's variant; a login's methods (`passkey + code`, `no password` when it has none); a card's brand, a Wi-Fi network's security, an SSH key's type and the other kinds' tell; a code's `Steam`, `counter` or `6 digits`; a note's `edited NOV 08`. Then the state words: `crown jewel`, `shared with you // view`, `archived`, `recently deleted`, `conflict copy`. |
| Title | `h2` (the list pane's head is the `h1`; on a phone the frame's hidden `h1` stays, see Gaps). A card uses `type-title`. Otherwise `type-display` on two lines on a desktop, `type-display-compact` on one line on a phone, falling back to `type-title` on two lines when it does not fit (measured). The heading holds the title alone; the marks sit beside it, so a step that waits for the heading by name still finds it. |
| Mono line | The first website's host, then `// edited 2026.11.08`. No "added" (W3). A note's line holds only a site, because its overline already says when it was edited. |
| Notice slot | Recently deleted and Archived as info Banners (RESTORE on a desktop; on a phone the bottom row restores), the conflict as a warning Banner with today's two choices, and today's `ItemSecurity`. vault-7 makes the slot the I8 table. |
| Use zone | Section 2. |
| Details | `SectionHeader collapsible`, shut by default, remembered on this device (`localStorage`, key `lockdown.detail.details-open`, never a secret). Shut, it shows one line (vault // folder). Open, it holds VAULT, FOLDER, TAGS and VERSION (with HISTORY) as Field Rows, then today's panels: who shared it, Signed in elsewhere, Protection, Exports and backups, Filling on this site, and on a phone the Share entry. vault-6 turns them into the I4 rows. |

**Actions on a desktop** sit at the end of the overline's line, all quiet: the star (`aria-pressed`, named "Add to favorites" or "Remove from favorites"), today's SHARE (until edges-8 lets the ⋯ open it), EDIT, and ⋯ ("More for this item").

**The ⋯ menu** (an `AdaptiveMenu`, a sheet on a phone) lists only what the access allows. Own item or a share with edit: Add to / Remove from favorites, Folder and tags…, Move to vault (one row per other vault), Archive or Unarchive, History, Signing in on a TV? (logins), Keep as a crown jewel or Stop keeping it as a crown jewel (not on a fixed one), then Move to Recently deleted in critical words, last. In Recently deleted: Restore, History, Delete for good. A view-only or fill-only share has no menu and no EDIT.

**Move to Recently deleted** asks nothing (Q14): the item goes, and a toast says "Moved {title} to Recently deleted" with UNDO, which restores it and opens it again. Delete for good, the crown-jewel change and the conflict's two choices are `ConfirmDialog`s (focus on Cancel, the yes repeats the verb).

**On a phone** the top bar holds back (the frame's), edit and ⋯ (`useDetailTopBar`). The bottom row (`useDetailBottomRow`) follows the row's quick copy (`bottomRowOf()`): a secret gets the eye at the start (`SecretFieldRemoteButton action="reveal"`, so it follows the Reveal setting: "Hold to show password" or "Show password"; a crown jewel's is a gated 20 s tap, as the field's own) and its copy as the primary at the end; a code gets its copy only (the row's quick copy, as Quick access copies); nothing to copy means no row (the fingerprint button of Mobile / 03 is left out, Q17); Recently deleted gets Restore. A fill-only share has none.

**States.** Loading: five skeleton rows. Could not open: `EmptyState tone="critical"`, `ITEM // NOT OPENED`, "Couldn't open this item.", the core's sentence and the id in mono, TRY AGAIN (it reads the item again). Nothing chosen: the frame's "Nothing open." with "Pick an item to see it here." No fields: "Nothing filled in yet." (a note: "An empty note.") with EDIT.

## 2. One rule for every kind (brief I2)

The Use zone draws the template's fields in the template's order (`sectionsOf`). The first section has no header; later ones sit under a `SectionHeader`. A field with no value is not drawn. A template's catch-all section named "Details" is shown as "More details", so it never shares a name with the item's own Details (R18).

| Control | Row |
|---|---|
| text, email, phone, number, select | Field Row: label, value in mono (cut in the middle), COPY. A row's own COPY says "Copied" for two seconds and raises no toast (cross-app §1). |
| url | OPEN and COPY; the address without its scheme. |
| date, month | `2029.08.14`, `08/29`. |
| boolean | Yes or No. |
| multiline | A paragraph under its label (the frames' "Protected" note). A note's body is `Prose` from `markdown.ts`. |
| secret, secret-multiline | `SecretField` through `item-field.tsx`'s host; grouped fields in fours; its ⋯ holds "Show large type" first (D242, LA5). A Wi-Fi password keeps SHOW LARGE TYPE as a visible text action (I9). |
| totp | `CodeDisplay layout="field"` through `ItemCodeDisplay` (section 3). |
| passkey | The passkey rows of section 3. |
| linked | Section 3. |
| `ssh.public_key`, `pgp.public_key` | `CodeBlock` with copy. |

What each kind adds: a login's Sign-in rows (Username, Password, passkeys, codes, sign-in method) then Websites, with "No password; this login uses other methods." when it has none; a card's art on a `Tile` (`Visa // personal`, the number as four masked groups, the expiry, `CVC •••`) from its real fields only (no last four: W1) and "Filling a card into a page always takes a second press."; a Wi-Fi network's GUEST CARD under its fields; a passkey item's "A passkey is made by a site and used from the browser."; a crown jewel's whole Use zone in `CrownJewelSurface`. A kind with no template falls back to its fields under OTHER FIELDS.

## 3. Websites, passkeys and codes (brief I3)

- **Websites:** one Field Row each with OPEN and COPY; the match rule's words (`MATCH_RULES`, D154) as a caption only when it is not the default (`exact_host`); past three, three rows and SHOW ALL n.
- **Passkeys:** one Field Row each, `Lockdown // 2026.10.02`, the account under it, HIDDEN BY THE SITE with UNDO or ADDED BY THE SITE with today's line. DETAILS opens a review dialog (a sheet on a phone): User name, Display name, Made by, Provider ID, Syncs ("With your vault" or "This device only", from `backed_up`; no device names and no count, W9), Made, Last used ("// on this computer"), Algorithm, and DELETE PASSKEY in critical words, which asks (today's words; "Delete anyway" when it is the only way in) and runs the gate. A share without edit gets no UNDO and no delete.
- **Codes:** `ItemCodeDisplay` gained `layout`, `label`, `rule` and `menu` (props only added; `density` still works for edges-5's Codes). The label is the issuer, or "One-time code"; the 30-segment countdown, the critical tone near the end, `NN S`, the counter kind's NEXT CODE, Steam's five characters and the issuer warning are unicron's. Its ⋯ holds Show large type. While the core has not answered, the row is `CodeDisplay loading`; if it fails, `CodeDisplay error` with the core's sentence. A fill-only share says "Filled in the browser only."
- **Linked fields:** a Field Row with the link glyph and "Password from Router admin"; a secret target is a `SecretField` through this item; a code target has COPY CODE (it says "Copied" in its row); a gone target says "Points at an item or field that is gone." with BROKEN.

## 4. Tests, and what is not proven

- `detail-pane.test.tsx` (22 tests): the frame's overline, title and mono line; the overline's words; loading and could-not-open with TRY AGAIN; nothing filled in; Details shut by default and remembered; the ⋯ menu's rows for an own item, an edit share, view and fill-only shares, and Recently deleted; Move to Recently deleted asks nothing, toasts UNDO, and UNDO restores; the star; the phone's bottom row by quick copy (`bottomRowOf`); one test per kind's essentials (Wi-Fi, card with its art, passkey, note in Prose, SSH key in a CodeBlock, a variant); empty fields not drawn; websites with OPEN, COPY, the rule only when not the default and SHOW ALL; the Use zone as real lists (R7); one Security box (R2).
- `item-field.test.tsx` (12): the host contract as before, plus the phone's eye following the Reveal setting (hold and tap) through a remote, Show large type as the ⋯'s first row, and a plain COPY that says "Copied" and raises no toast.
- `passkeys-block.test.tsx` (8), `vault-depth-detail.test.tsx`, `detail-share-entry.test.tsx`, `crown-jewel.test.tsx`: the same facts at their new places (the details dialog, the ⋯, Details).
- Not proven by a test: the title's fallback measurement (jsdom has no layout); the bottom row's elements in the shell (the plan is tested; the hand-up is foundation's and tested there). The capture states show both.

## 5. Gaps

- **Share… is not in the ⋯ yet.** `ItemShare` draws its own trigger and takes no open state, and it belongs to edges-8. Until it gains `open`/`onOpenChange` (props only added), SHARE stays on the overline's line on a desktop and in Details on a phone.
- **The title is an `h2` on a phone too.** The frame's list + detail draws a hidden `h1` for the pushed detail; two `h1`s would follow if the title were one.
- **No EXPIRES IN n DAYS badge on a date.** The reminder's lead is not read here; the H1 lists keep expiry.
- **The notice slot is today's content restyled**, not the I8 table (vault-7), and Details holds today's panels, not the I4 rows (vault-6).

### unicron host-contract gaps

1. `FieldRow`'s middle cut splits a value into two text nodes, so a test (or anything that searches the DOM by text) cannot find "home-net" as one string. Not a fault in the page; noted for the migration guide.
2. No part for a paragraph under a label (the frames' "Protected" note). Built as a plain label line and a `type-body` paragraph inside `ItemField`.
3. `CodeDisplay` has no host slot for a failed copy's sentence; a failed copy still raises a critical toast (today's), where 4.0 wants a line beside the control.
4. `Tile` has no width knob; the card art is held at 420 by a plain wrapper (`max-w-105`), as Desktop / 03 draws it.
5. `SectionHeader collapsible` has no way to say the summary is the vault and folder in the summary's own role words; it draws the line as given.

## 6. Proposed decisions

| # | Decision | Status |
|---|---|---|
| D770 | **The item detail is one frame at every width** (I1): overline, title in the X6 role, mono line, one notice, the Use zone, then Details shut by default and remembered on the device. "version 7" leaves the header for Details. Answers review R8. Plain version: an item shows what you came for first; what is about the item waits behind one line. | Proposed; **flagged for Sean** (R8) |
| D771 | **One rule draws every kind** (I2): Field Rows by the template's control, empty fields left out, sections under Section Headers, a card's art on a Tile from real fields only, a note's body in Prose, public keys as code blocks. Plain version: every kind of item looks like one family, and a new template needs no new design. | Proposed; **flagged for Sean** |
| D772 | **Move to Recently deleted asks nothing and offers UNDO** (Q14). Delete for good still asks. Plain version: deleting an item is one press and can be undone for a few seconds; it stays in Recently deleted for the vault's days. | Proposed; **flagged for Sean** (Q14) |
| D773 | **Large type is the first row of a hidden field's ⋯** (changes the place of D242's action; one action on any hidden field is kept). A Wi-Fi password and a one-time code item keep the visible text action. Plain version: Show large type is in the field's own menu. | Proposed; **flagged for Sean** (D242, 6.2) |
| D774 | **The phone's bottom row follows the item's quick copy**: a secret's eye (following the Reveal setting) and copy; a code's copy; nothing for an item with nothing to copy; Restore in Recently deleted. The fingerprint button of Mobile / 03 is left out (Q17). Plain version: the two big buttons on a phone always do something real for that item. | Proposed; **flagged for Sean** (Q17, 6.3 Mobile / 02, 03, 04) |
| D775 | **Passkeys are rows with a details dialog**: who made it and when on the row; the account under it; DETAILS for the rest; "Syncs" yes or no from the backup flag, with no device names and no count (W9); DELETE PASSKEY asks first. "ES256" leaves the row. Plain version: a passkey reads like any other field, and its technical facts wait one press away. | Proposed; **flagged for Sean** (R8) |
| D776 | **A row's own COPY says "Copied" for two seconds and raises no toast** (cross-app §1); a website row has OPEN and COPY; the match rule shows only when it is not the default. Plain version: copying a plain value says so where you pressed. | Proposed |
| D777 | **The ⋯ menu offers only what the person's access allows**: none on a view-only or fill-only share; Restore and Delete for good in Recently deleted. Plain version: the menu never shows a row that would be refused. | Proposed |

D778 and D779 are free.
