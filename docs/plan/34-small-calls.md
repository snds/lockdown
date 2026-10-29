# 34: Four small calls

_Slice: `slice/small-calls`, built 2026-10-29. Implements the four follow-ups D375 handed to the building session (`flag-review.json`): the notice after a quiet passkey upgrade (D210 and D312 as changed by D375), strength words that match the default passphrase (D230 as changed by D375), sharing paused on a split view (D328 as changed by D375), and revoking a device ends its sessions (the D405 follow-up from doc 33 §7). Builds on D38 (security-model calls are the building session's, marked audit items), D62 (additive shapes), D73 (what revocation means), D101 and D151 (gates and uses), D196 (sharing gates), D221 and D296 (ending sessions), D327 (the mirror comparison). Proposed decisions D410–D417 (§8)._

## For future agent

- **TL;DR:** (1) A passkey a site adds by itself after a password sign-in Lockdown saw still happens silently, but now the passkey carries `upgraded` (the site and when) on the item version, the desktop app toasts "*site* added a passkey to this login" with **Show**, and the item says "Added by *site* on *date*, after a password sign-in" until it is opened once on that device. (2) `lockdown_app::strength::bits_word` is the one rule for the generator's word: weak under 50 bits, fair 50 to under 75, strong from 75; the six-word passphrase (77.5 bits) reads strong. (3) While the account server's public log disagrees with a mirror, the core refuses new look-ups and pins, phrase checks, vault and household shares, item shares, share and request links, naming an emergency contact and opening emergency access, each with a sentence that says what is paused and why; the vault keeps reading, writing and syncing. (4) When the server accepts an identity manifest that revokes a device, it ends every session of that device in the same request, so its token stops reading at once; renewing says "revoked".
- **Read with:** `crates/app/src/passkeys.rs` (the upgrade and its notice), `crates/app/src/session/passkeys.rs` (the `upgraded` part), `crates/app/src/recency.rs` (what this device has seen), `crates/app/src/strength.rs` (`bits_word`), `crates/app/src/session/split_pause.rs` (the pause), `crates/app/src/session/mirrors.rs` (the split that sticks), `bins/server/src/api.rs` (`put_identity`, `end_revoked_sessions`, `check_session`), `bins/server/src/sessions.rs` (`end_device_sessions`), `bins/server/src/renewal.rs` (revoked before ended).
- **Do not:** say anything to the page about the upgrade's notice (the page gets a passkey, never a hint of the notice); decide a strength word anywhere but `bits_word`; check the split only in a shell (the core refuses, the shells only say so first); run an approval gate before the pause check (a paused step never asks for the password); end a revoked device's sessions anywhere but after the identity push is accepted.

## 1. The notice after a quiet passkey upgrade

**What it does.** The automatic upgrade (conditional create, D210, D312) is unchanged at the moment it happens: no sheet, no prompt, UP and UV clear, refused unless this core filled that login's password on that site in the last five minutes, or saw its saved password typed there. What is new is afterwards:

| Where | What the person sees | How |
|---|---|---|
| The item version | Nothing directly: the passkey's group gains `passkey.<n>.upgraded = "<site> <secs>"`, a text part, so every device that syncs the item knows the site added it | `NewPasskey.upgraded`, written by `store::append`; read back as `Stored.upgraded` |
| The desktop app, at once | A toast: **"github.com added a passkey to this login"**, the login's title under it, **Show** opening the login | The core keeps each upgrade it makes in memory (`Core.passkey_upgrades`, at most 16, dropped on lock); `App::passkey_upgrades()` hands them out once; the window asks every two seconds while the vault is open (`usePasskeyUpgrades`) |
| The item, until opened once | A badge **added by the site** and the line **"Added by github.com on 29 Oct 2026, after a password sign-in."** | `PasskeyView.upgrade {site, at}` is present while this device hasn't seen it; opening the item calls `passkey_upgrade_seen`, which records the upgrade's time in the sealed recency file (`upgrades_seen`, added within v1); the line stays while the item stays open |

The site is the passkey's RP ID, which the core already checked against the browser's record of the tab. "Seen" is per device, like the recency record it lives in (D151's file; it is not a use and ranks nothing): a second device that syncs the item shows the line until it is opened there too.

**Files.** `crates/app/src/passkeys.rs` (`UpgradeNotice`, `UpgradeView`, `PasskeyView.upgrade`, the two `App` calls), `crates/app/src/session/passkeys.rs` (`Upgraded`, `note_upgrades_seen`), `crates/app/src/recency.rs`, `apps/desktop/src-tauri/src/passkey_commands.rs` (`passkey_upgrades`, `passkey_upgrade_seen`, appended to the handler list), `apps/desktop/src/features/passkeys/use-passkey-upgrades.ts`, `passkeys-block.tsx`, `App.tsx`.

## 2. Strength words that match the default passphrase

`lockdown_app::strength::bits_word(bits)` decides the generator's word: **weak** under 50 bits, **fair** from 50 to under 75, **strong** from 75 (`FAIR_FROM_BITS`, `STRONG_FROM_BITS`). `depth::strength_word` keeps its name and calls it. The generator's preview (`GeneratorPreview.strength`) carries the word to every shell: the desktop's and the web vault's editors ("6 words joined by "-": about 78 bits, strong."), and now the extension's suggestion row ("Made by Lockdown · 129 bits, strong"). Six words from EFF's long list are 77.5 bits and read strong; five words (64.6 bits) read fair; twenty characters with digits and symbols stay strong. The editor's meter for a typed password (zxcvbn's 0–4 score, D116) is a different rule and is unchanged. The design capture's fixture uses the same thresholds.

## 3. Sharing paused on a split view

**When.** The last mirror check after a sync, kept per account server in `kt-mirrors.cbor` (doc 27 §4), holds a split (`MirrorVerdict::Split`). It clears only when a later check no longer finds one: the mirror agrees again, or the person removes a mirror they added. New: a mirror that showed a split and can't be reached at the next check keeps its split (`mirrors::check`), so blocking a mirror doesn't lift the pause.

**What pauses, in the core.** Each step below is refused by the session (`Session::refuse_while_split`) and, where it has a gate, by the `App` call before the gate runs, so the person is never asked for a password for something that will be refused:

| Step | Refusal starts |
|---|---|
| Looking someone up (`lookup_contact`, which pins them) | "Looking someone up is paused: …" |
| A phrase check (`phrase_invite`, `phrase_incoming`, `phrase_answer`, which pin) | "Checking someone's keys with a phrase is paused: …" |
| Sharing a vault, adding someone to a household (`share_vault`, `add_to_household`) | "Sharing a vault is paused: …" |
| Sharing an item (`share_item`) | "Sharing an item is paused: …" |
| A share link of an item, text or a file (`send_item`, `send_text`, `send_file`) | "Making a share link is paused: …" |
| A request link (`create_request`) | "Making a request link is paused: …" |
| Naming an emergency contact (`add_emergency_contact`) | "Naming an emergency contact is paused: …" |
| Opening emergency access after its wait (`emergency_open`, the step where the released keys reach this device) | "Opening emergency access is paused: …" |

Each sentence ends: "… is paused: this server's public log doesn't match a mirror, and a server showing two histories could swap someone's keys. Your vault still opens and syncs; this works again once the split clears."

**What does not pause.** Reading, editing, syncing (D328 stands), comparing a fingerprint already pinned, changing a member's right, removing someone, stopping a share or deleting a link (they take access away, as D196's removals), answering someone's request link (the link pins its own key), the emergency steps accept, decline, request, stop and revoke, and the members a shared vault's sync pins on its own (`pin_unknown`, part of syncing).

**What the shells say.** `SharingOverview.paused` (added under D62) carries the banner the Sharing screen and the web vault's People screen draw: "Sharing is paused: this server's public log doesn't match a mirror, and a server showing two histories could swap someone's keys. Until it clears, Lockdown won't look anyone up or check their keys with a phrase, share a vault or an item, make share or request links, name an emergency contact or open emergency access. Your vault still opens and syncs." The split warning on Status and Settings now names what is paused, and the sticky toast adds "New sharing is paused until it clears." The share-text and request-link dialogs now show a refusal inside the dialog, where the person is looking, instead of only behind it.

## 4. Revoking a device ends its sessions

The server learns of a revocation one way: an identity manifest pushed by another device (`PUT /v1/identity`; the desktop's Devices, `lockdown device revoke`, the web vault). After `put_identity` accepts one, every device it newly revokes (a new manifest revocation, or a device record marked revoked) has each of its sessions not yet ended ended at once (`SessionStore::end_device_sessions`): sessions whose token still runs and sessions only renewable alike. The live channel is told (`session_ended` to each, `sessions` to the account), the same as D221's End.

- The revoked device's next call gets 401 with "this device was revoked from the identity, and its sessions ended with it"; its renewal now checks revocation before an ended row, so it answers `revoked_device`, and the core says the revoked words (D404) rather than the ended ones.
- One read stays open to it: `GET /v1/identity`, the signed identity record set. That is how a revoked device learns, from records it can verify rather than from the server's word, that it is out and shuts its own copy (D73); the command line's `sync` does exactly that, as before. Every other call is refused.
- The core's live handler, told its session ended, asks the server why with a renewal challenge (which changes nothing) before forgetting the token, so a device revoked while open shows the revoked sentence on Status; the desktop's toast uses that sentence too.
- **The operator.** The admin page has no device revocation (revoking stays with the account's own devices, D296 and `admin.rs`); its **End sessions** already ends every session at once. So "the operator's revoke" needed nothing new; this is noted as a gap rather than invented.

## 5. Wire and shape additions (D62)

All additive; no field renamed, removed or retyped, no wasm boundary or extension schema changed.

- **Item data:** the passkey part `passkey.<n>.upgraded` (text, `"<site> <secs>"`); readers that don't know it ignore it, and a value that doesn't read is no notice.
- **`lockdown-app`:** `PasskeyView.upgrade`, `UpgradeView`, `UpgradeNotice`, `App::passkey_upgrades`, `App::passkey_upgrade_seen`; `SharingOverview.paused`; `strength::{bits_word, FAIR_FROM_BITS, STRONG_FROM_BITS}`; the recency file's `upgrades_seen` (serde default within v1).
- **Desktop commands** (appended): `passkey_upgrades`, `passkey_upgrade_seen`. **Desktop types:** `PasskeyView.upgrade?`, `PasskeyUpgrade`, `PasskeyClient.upgrades/upgradeSeen`, `SharingOverview.paused?`.
- **Server:** `SessionStore::end_device_sessions`. No route, no wire type and no error code was added; the 401 for a revoked device's ended session keeps `unauthenticated` with new words.
- **Extension test fixture:** `browser-endpoint`'s `upgrades` line; `webauthn.html`'s `#upgrade` button.

## 6. Running and testing it

```sh
cargo test -p lockdown-app strength                               # the thresholds, the default passphrase strong
cargo test -p lockdown-app --lib depth                            # the preview: six words strong, five fair
cargo test -p lockdown-app --lib session::passkeys               # the upgraded part read back, hostile values
cargo test -p lockdown-app --lib recency                          # seen is sealed with the uses
cargo test -p lockdown-app --test passkeys an_automatic_upgrade   # silent, then the toast once and the item until opened; a lock
cargo test -p lockdown-app --lib split_pause                      # every step's sentence
cargo test -p lockdown-app --test sharing_pause                   # two people, a real split mirror: each step refused, the vault syncs, an unreachable mirror keeps it, then each step allowed
cargo test -p lockdown-app --test session_renewal revoking        # two devices: revoked, B's token reads nothing at once, "revoked"
cargo test -p lockdown-server --lib sessions                      # end_device_sessions: every session of one device, no other
cargo test -p lockdown-server --test session_renewal a_revocation # over HTTP: both of B's tokens refused at once, C untouched, a later push ends nothing
cargo test -p lockdown-desktop --features shell browser::tests    # the upgrade over the extension's protocol; the suggestion's word
cd apps/desktop && pnpm vitest run src/features/passkeys src/features/sharing src/features/server
cd extension && pnpm exec playwright test test/passkeys.spec.ts   # the upgrade after a real fill in Chromium, verified by the RP
cd design/capture && pnpm capture --only desktop-passkey-upgrade-toast --only desktop-item-passkey-upgraded --only desktop-editor-generator --only desktop-sharing-paused --only desktop-status-split --only desktop-live-split --widths 1280,390
```

## 7. Gaps

1. **The toast is for upgrades this app made.** An upgrade made through another device's app reaches this one with the item (its line shows until opened), but not as a toast; the web vault has no passkey path and no toast.
2. **"Seen" is per device.** Opening the item on one device doesn't close the line on another; that is a choice (the recency file never syncs, D151), recorded as D411.
3. **A mirror the person removes clears the split.** Removing their own mirror is a person's choice in Settings, so it lifts the pause; a mirror the server named is remembered (doc 27) and can't be removed that way.
4. **The pause covers new keys only.** A shared vault's sync still pins members it meets, and items already shared keep syncing to the people they're shared with; both are existing access, and stopping them would stop syncing (D328).
5. **No operator revocation** (§4); and a revoked device can still sign in again with the password (D404's revoked words say so) and read until something revokes it again: the identity can't revoke a device twice. Refusing a revoked device's password sign-in is a possible next step.
6. **The admin page's session counts** (doc 33 §7 item 2) are unchanged.
7. **The extension's suggestion row** is the only extension text that changed; the in-page frames' other captures were not re-taken.

**unicron host-contract gaps:** none hit. The toast is `sonner`'s action toast with a description, the item's mark is `Badge`'s `outline` variant, and the pause banner reuses the split warning's bordered `role="alert"` block with the design tokens' destructive colour.

## 8. Proposed decisions

For the coordinator to record. **Flagged** rows are user-visible and need Sean.

| ID | Decision | Owner |
|---|---|---|
| D410 | **The upgrade is recorded on the passkey and told afterwards.** The automatic upgrade (D210, D312) stays silent at the moment; the passkey's group gains `upgraded` (the RP ID and the time) on the item version; the desktop toasts "*site* added a passkey to this login" with the login's title and **Show**, once per upgrade this app made; the item shows "Added by *site* on *date*, after a password sign-in" and a badge until it is opened once. Amends D210 and D312 as D375 asked. Plain version: a site can still add a passkey right after you sign in, and Lockdown now tells you it did. | **Flagged for Sean** (the words of the toast and the line) |
| D411 | **"Opened once" is per device.** Which notices were seen is kept in the device's sealed recency file (D151's), not synced; seeing one is not a use and ranks nothing. Plain version: each of your devices shows the note until you open that login there. | Accepted by default under D38; **flagged for Sean** (the note can show on each device) |
| D412 | **One strength rule:** `strength::bits_word`, weak under 50 bits, fair 50 to under 75, strong from 75; the six-word default (77.5 bits) reads strong, five words fair. Every shell takes the word from the core's preview; the extension's suggestion now shows it. Amends D230 as D375 asked. Plain version: the passphrase Lockdown suggests is called strong, because it is. | **Flagged for Sean** (the thresholds are user-visible), as D375 set them |
| D413 | **A split view pauses new trust in the server's keys, in the core:** look-ups and pins, phrase checks, vault and household shares, item shares, share and request links of any kind, naming an emergency contact and opening released emergency access are refused with "*Step* is paused: this server's public log doesn't match a mirror, and a server showing two histories could swap someone's keys. Your vault still opens and syncs; this works again once the split clears." A gated step is refused before its gate asks. Reading, writing, syncing and every step that takes access away go on. Amends D328 as D375 asked. Plain version: when your server might be showing you a different history, Lockdown stops new sharing through it until that's sorted out, and keeps your vault working. | **Flagged for Sean** (what pauses, and the words); audit item |
| D414 | **A split clears only when the mirror is heard again.** A mirror that showed a split and can't be reached at the next check keeps the split; it clears when that mirror agrees, or when the person removes a mirror they added. Plain version: cutting Lockdown off from the mirror doesn't make the warning go away. | Accepted under D38; audit item |
| D415 | **The Sharing and People screens say the pause** with the core's banner (`SharingOverview.paused`); Status's split warning names what is paused; the split toast adds "New sharing is paused until it clears."; a refused link shows its sentence inside its dialog. Plain version: the screens where you'd share tell you it's paused and why before you try. | **Flagged for Sean** (the words) |
| D416 | **Revoking a device ends its sessions at once.** When the server accepts an identity manifest that newly revokes a device (by revocation or record), every session of that device not yet ended ends in the same request, renewable ones included, and the live channel is told; the device's renewal answers `revoked_device` before anything else; its tokens' 401 says it was revoked; the one call still answered is `GET /v1/identity`, so the device can verify its own revocation and shut its copy (D73). The core asks the server why with a renewal challenge when told its session ended, so a revoked device shows the revoked words. Replaces D405's "revoking still doesn't end its current session". Plain version: a device you remove loses access the moment you remove it, not hours later. | Accepted under D38; audit item |
| D417 | **Shape additions** (D62): the passkey part `upgraded`; `PasskeyView.upgrade`, `UpgradeView`, `UpgradeNotice`, `App::passkey_upgrades`, `App::passkey_upgrade_seen`; the commands `passkey_upgrades` and `passkey_upgrade_seen` (appended); `SharingOverview.paused`; `strength::bits_word` and its two constants; the recency file's `upgrades_seen`; `SessionStore::end_device_sessions`; the desktop types. No route, error code, wasm boundary or extension schema changed. Plain version: everything new was added beside what was there. | Accepted by default under D38 (structure) |
