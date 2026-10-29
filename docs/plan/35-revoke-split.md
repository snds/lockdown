# 35: A revoked device stays out, and nobody new during a split

_Slice: `slice/revoke-split`, built 2026-10-29. Implements the two security follow-ups doc 34 §7 left open (gaps 4 and 5), decided under D38 and D375: a revoked device can no longer sign back in with the password, and a shared vault's sync meets nobody new while the server's public log disagrees with a mirror. Builds on D73 (what revocation means), D196 (sharing gates), D246 (joining from an enrolled device), D250 and D300 (a key or the account's email for a new device), D327 and D328 (the mirror comparison and the split), D400–D404 (renewal and its words), D413 and D414 (sharing paused on a split, a split that sticks), D416 (revoking ends the device's sessions). Proposed decisions D420–D427 (§7)._

## For future agent

- **TL;DR:** (1) When the identity revokes a device, every way into a session refuses that device id with the new wire code `revoked_sign_in` (403) and the words "This device was revoked from your account. Join it again from a device you still use.": the OPAQUE login's finish (after the password and the device proof check out, before any key or email is asked), the end of every sign-in (`issue_session`, so a second factor's poll is refused if the device was revoked while it waited) and session renewal. The old token still reads `GET /v1/identity`, so the device verifies its own revocation and shuts its copy (D73, D416). The same machine comes back only as a new device id, through the join flow (D246) or the kit. (2) While the last mirror check for the account server found a split, a shared vault's sync takes membership changes only up to the first that would make this device meet someone (an account it hasn't pinned, or a pinned member's device it doesn't know); that change and every later one wait, the vault says "New members wait until the server's log matches its mirrors again.", and a head signed by a device the vault doesn't trust stops only that vault. When the split clears, the same sync applies what waited: new accounts are looked up through the log, known members are looked up again and must continue their pin, and a vault stopped at an unknown device refreshes its writers and syncs once more.
- **Read with:** `bins/server/src/revoked_sign_in.rs` (the check and the words), `bins/server/src/api.rs` (`login_finish`, `issue_session`), `bins/server/src/renewal.rs` (`standing`, `listed_device`), `crates/sync/src/renew.rs` (`SignInReason::Revoked`), `crates/sync/src/transport.rs` (`auth_error`, `refused`), `crates/app/src/session/split_hold.rs` (what waits), `crates/app/src/session/sharing.rs` (`pull_membership`, `sync_sharing`, `refresh_writers`), `crates/app/src/session.rs` (`sync`, the vault loop in `sync_with`).
- **Do not:** check revocation before the password and the device proof (nobody learns a device's standing without the password); ask a revoked device for a key or send an approval email for it; take the server's words for a revoked sign-in (the client says its own); skip a waiting membership change and take a later one (the chain is a chain); pin anyone in a shared vault's sync while the log is split; refresh a member's pin without `check_continues`.

## 1. A revoked device can't sign back in

**Before.** Revoking a device ended its sessions (D416) and its renewal answered "revoked", but the device could type the account password, get a fresh session and read (D73 refused only what it signed). The renewal's words even told it to: "Log in again with your account password."

**Now.** `revoked_sign_in::refuse_revoked(state, account, device)` asks the identity whether the device id is revoked: a revocation in the manifest the server holds (D73's marker row in `revoked_devices`), or the device's own record marked revoked in the newest verified record set (`renewal::listed_device`, now `pub(crate)`). A record can't be marked revoked without a manifest revocation (the record set refuses it), so the second check is belt and braces. It runs in three places:

| Path | Where | When it refuses |
|---|---|---|
| The password sign-in | `api::login_finish` | After OPAQUE's finish and the device's proof of its key check out, before the security-key or email decision: a revoked device is never asked for a key and never gets an approval email sent |
| Every sign-in's end | `api::issue_session` | The password path again, and the second factor's poll (`second_factor::poll`) after a key answered or the email's link was approved: a device revoked while it waited gets no session |
| Session renewal | `renewal::standing` (challenge and renew) | Before the ended-row check, as before; it now answers `revoked_sign_in` instead of `revoked_device` |

Every refusal is `403`, `ErrorCode::RevokedSignIn`, and `revoked_sign_in::WORDS`. The rate brake (D57) runs before any of it, as for every sign-in.

**What stays open to the revoked device.** Exactly what D416 left: `GET /v1/identity` with the token it already holds (`api::check_session`). The device's next sync reads the signed record set, finds itself revoked (`merge_records`' `this_device_revoked`) and stops with "this device was revoked from the identity by another device…". Nothing else answers it.

**What the person sees.**

| Where | Words |
|---|---|
| Desktop and web, signing in or renewing | "This device was revoked from your account. Join it again from a device you still use." (`SignInReason::Revoked`, replacing D404's revoked sentence, which said to log in again). The core returns it as `sign_in_needed` with `detail: "revoked"`, as before |
| Desktop Status | The same words in the account section's attention line; **Log in again** is not offered for this reason, and a sync that hits it doesn't open the Log in dialog (the password can't help) |
| Command line | The same words, then " (it comes back only as a new device: `lockdown recover` with your recovery kit into an empty `--root`)", from both `account login` and a refused renewal |

The client never shows the server's text for this code: `HttpTransport` replaces the message of a `revoked_sign_in` refusal with its own words, and the sign-in's `auth_error` maps the code to them.

**Coming back.** A new device id is not revoked, so the machine joins like any new device: approved from an enrolled device with the six digits (D246), or set up with the kit (`recover`), or through the account with the Secret Key. The identity then lists it as a new device beside the revoked one.

## 2. Nobody new during a split

**Before.** D413 paused every new look-up, pin and share while the account server's log disagrees with a mirror, but doc 34 §7 gap 4 left one door: a shared vault's sync pinned every account its membership chain named, so a server showing two histories could introduce a member, or a new device for a known member, during the split.

**Now** (`crates/app/src/session/split_hold.rs`). While `Session::split_view()` (the last mirror check for the account server found a split, D414) holds:

1. **Membership changes wait from the first one that meets someone.** `pull_membership` decodes the new events and `membership_take` walks them in order: an event that names an account this device hasn't pinned (the maker, the owner of a new vault, the member added, set or removed), or that a pinned member signed with a device this device doesn't know (their keys changed), stops the walk. The events before it are taken as ever; it and every event after it are left on the server (the next sync fetches them again from the same point). The vault is marked as waiting.
2. **A waiting vault doesn't move its key or get joined.** `sync_sharing` skips `take_new_key` for it (a removal's new generation may be in the waiting part), and a vault shared with this person whose chain waits is not joined yet.
3. **Heads from devices the vault doesn't trust stop only that vault.** When a shared vault's sync fails and the server's heads past what this device synced name a signer the vault doesn't trust (`unmet_head_signer`), the vault waits with the same sentence; every other vault, the person's own included, goes on syncing. Items from members already pinned keep arriving up to the first such head.
4. **The words.** `VaultSharingView.held` carries "New members wait until the server's log matches its mirrors again." (the desktop's vault sharing dialog, as a `role="note"` line under the crown-jewel line; the web vault has no vault sharing dialog, and its People screen keeps D415's pause banner), and the sync adds "A shared vault has changes from people this device hasn't met yet. New members wait until the server's log matches its mirrors again. Items from members you already have keep syncing."

**When the split clears** (the mirror agrees again, or the person removes a mirror they added; D414), the waiting state is worked out again by the next sync, and `Session::sync` runs that next round at once when the sync it just finished saw the split clear with something waiting:

- the waiting membership changes are taken: each new account is looked up through the server's directory and checked against the log (`verify_directory_entry`), exactly as a sync always pinned; a pinned member who signed with an unknown device is looked up again and must continue the pin (`Contact::check_continues`: same signing and receiving keys, an identity manifest no older);
- a shared vault stopped at an unknown signer (a member's new device that signed no membership change) has every writer looked up again the same way (`refresh_writers`), its roster re-applied, and syncs once more; a signer still unknown after that fails the sync as it always did.

**Outside a split** these two refreshes are new behaviour too: before, a known member's new device made a shared vault's sync fail ("signed by a device this vault doesn't know"); now it is looked up and checked, then trusted.

**What is not held.** Reading, writing and syncing the person's own vaults; items already shared; a waiting vault's items from members already pinned (until a head by someone unmet); every step D413 left open (changing a right, removing someone, stopping a share), though a manager's change to a vault whose chain waits builds on the part this device took, so the server refuses it as behind until the split clears.

## 3. Wire and shape additions (D62)

All additive; no field renamed, removed or retyped, no route, command, wasm boundary or extension schema changed.

- **`lockdown_sync::wire::ErrorCode::RevokedSignIn`** (`revoked_sign_in`), appended. The renewal endpoints answer it where they answered `revoked_device` for a revoked device; this client maps both to `SignInReason::Revoked`.
- **`SignInReason::Revoked`'s words** changed (D404's sentence for this one reason).
- **Server:** the `revoked_sign_in` module (`WORDS`, `is_revoked`, `refuse_revoked`); `renewal::listed_device` made `pub(crate)`.
- **`lockdown-app`:** `VaultSharingView.held` (`null` when nothing waits); the `split_hold` module; `Session::refresh_writers`; `device_table` visible to the session's modules.
- **Desktop types:** `VaultSharingView.held?`.
- **CLI:** `HOW_REVOKED` after the revoked words.

## 4. Running and testing it

```sh
cargo test -p lockdown-server --test revoked_sign_in       # password, renewal, key wait, email wait refused; the old token reads only the identity; others untouched
cargo test -p lockdown-server --test session_renewal       # renewal now answers revoked_sign_in for a revoked device
cargo test -p lockdown-sync renew                          # revoked_device and revoked_sign_in both read "revoked"
cargo test -p lockdown-app --test session_renewal a_revoked_device_cant   # two devices: B refused in the words, its sync still learns it's out, A goes on, B's machine rejoins from A as a new device
cargo test -p lockdown-app --test sharing_pause during     # two people and a real split mirror: Carol added during Bob's split waits, Alice's items still sync, heads by Carol and by Alice's new device stop only Family, all of it applies when the split clears
cargo test -p lockdown-app --lib split_hold                # what an event names; the words
cargo test -p lockdown-cli --test sync_end_to_end          # the CLI end to end: the revoked phone's `account login` refused with the words and how it comes back; set up afresh, it recovers as a third device
cd apps/desktop && pnpm vitest run src/features/status src/features/sharing
cd design/capture && pnpm capture --only desktop-status-revoked --only desktop-sharing-vault-held
```

The split test reuses `sharing_pause.rs`'s setup: the real server and two real mirrors in process, one honest and one whose copy names another log key, so Bob sees a split exactly as he would in the wild; Alice's view isn't split, so she adds Carol and writes while Bob's sync holds.

**Design capture.** Two new states, both with zero axe violations at 1280, 900 and 390, light and dark: `desktop-status-revoked` (Status for a revoked device: the words, no **Log in again**) and `desktop-sharing-vault-held` (the Family vault's sharing dialog with the waiting line). No existing state's words changed: the fixture's Status never showed the revoked reason, and the dialog draws the line only when `held` is present.

## 5. Gaps

1. **A waiting head blocks the whole vault behind it.** Heads are a chain: once a member who waits (or a member's new device) writes, this device takes none of that vault's later heads until the split clears, including heads by members it already has. The rest of the account syncs.
2. **A vault shared by someone not met during a split waits unseen.** Its chain starts with a person this device hasn't pinned, so it isn't joined and has no view to carry the sentence; the sync line says something waits.
3. **Items shared on their own still pin their owner.** `sync_received` pins an unknown owner of an item shared with this person even during a split; holding those too is a small follow-up.
4. **The waiting state lives in the session.** Every sync works it out again, so nothing is lost, but after a restart the sentence shows only from the first sync.
5. **The old token's read of `GET /v1/identity` lasts while the token runs** (8 hours, D52): a revoked device that stays offline longer can't verify its revocation from the server's records, and its local copy opens offline until its next successful contact. Unchanged from D416.
6. **Clients before this slice** don't know `revoked_sign_in`: they read the refusal as a server failure, with the server's words in its text, and their renewal treats it as passing rather than "revoked".
7. **An owner's new device enrolling through the account** fails to pull a shared vault whose chain has heads by another member it hasn't met yet ("signed by a device this vault doesn't know"); found while writing the split test, outside this slice (the test enrols Alice's second device before Carol writes).
8. **No operator revocation** (doc 34 §7 item 5) and **the admin page's session counts** (doc 33 §7 item 2) are unchanged.
9. **The web vault's Playwright suite** has no revoked or split scenario, and the web vault shows no per-vault waiting line (it has no vault sharing dialog); the core paths are covered natively and the desktop views by vitest.

**unicron host-contract gaps:** none hit. The waiting line reuses the sharing dialog's `role="note"` paragraph in the attention ink (as the crown-jewel line), and Status only leaves out its existing outline `Button` for the revoked reason.

## 6. Audit notes (D38)

Refusing a revoked device id at every sign-in path, the order of the check (after the password and the device proof, before any second factor), the second check at `issue_session`, renewal's new code, the client owning the words, the rule for what waits during a split (the first event naming an unpinned account or signed by an unknown device of a pinned one, and everything after it), stopping only the vault whose heads name an untrusted signer, and the refresh of known members' pins through `check_continues` are security-model choices made under D38. They are marked audit items in §7 so the outside review (D377) takes them with D413, D414 and D416.

## 7. Proposed decisions

For the coordinator to record. **Flagged** rows are user-visible and need Sean.

| ID | Decision | Owner |
|---|---|---|
| D420 | **A revoked device can't sign back in.** The password sign-in's finish, the end of every sign-in (a security key's or the email's included) and session renewal refuse a device id the identity revokes (by its manifest or its record) with `revoked_sign_in` (403). The check runs after the password and the device proof, before any second factor, so no key is asked and no email sent for it, and again where the session is issued, so a device revoked while it waited is refused. Its old token keeps reading `GET /v1/identity` only (D416). It comes back only as a new device id (D246, or the kit). Closes doc 34 §7 gap 5. Plain version: a device you removed can't get back in with your password; it has to be added again like a new one. | Accepted under D38; **flagged for Sean** (a removed device must rejoin as new); audit item |
| D421 | **The revoked words.** "This device was revoked from your account. Join it again from a device you still use." replaces D404's revoked sentence everywhere (sign-in, renewal, Status, the web vault's lock); the client says them in its own voice whatever the server wrote; desktop Status offers no **Log in again** for this reason and a sync doesn't open Log in; the command line adds how it comes back (`lockdown recover` into an empty `--root`). Plain version: Lockdown tells a removed device plainly that it was removed and how to add it again, instead of asking for a password that won't work. | **Flagged for Sean** (the words) |
| D422 | **Renewal answers `revoked_sign_in`** for a revoked device, where it answered `revoked_device`; clients read both as "revoked". `revoked_device` stays what a revoked device's writes get (D73). Plain version: one answer for "this device can't get a session back", one for "this device can't write". | Accepted by default under D38 (structure); audit item |
| D423 | **Nobody new during a split.** While the last mirror check found a split (D413, D414), a shared vault's sync takes membership changes in order up to the first that names an account this device hasn't pinned or is signed by a pinned member's device it doesn't know; that change and all after it wait unapplied; the vault's key doesn't move and a vault shared by someone unmet isn't joined; the vault says "New members wait until the server's log matches its mirrors again." and the sync says what waits. Amends D413 (closes doc 34 §7 gap 4). Plain version: while your server might be showing you a different history, nobody new joins your shared vaults on your device; they're added once that's sorted out. | **Flagged for Sean** (what waits, and the words); audit item |
| D424 | **Untrusted heads stop one vault, not the account.** A shared vault whose new heads name a signer it doesn't trust waits during a split with the same sentence while every other vault syncs; items from members already pinned arrive until the first such head. Plain version: one shared vault can pause; the rest of your vaults keep syncing. | Accepted under D38; audit item |
| D425 | **What waited applies when the split clears, checked as ever.** The sync that sees the split clear runs one more round at once: new accounts are looked up and checked against the log, known members whose devices changed are looked up again and must continue their pin (`check_continues`), and a vault stopped at an unknown signer refreshes its writers' pins and syncs once more. Outside a split this refresh now also lets a known member's new device into a shared vault, where the sync used to fail. Plain version: once the warning clears, the people who were waiting appear by themselves, after the same checks as always. | Accepted under D38; audit item |
| D426 | **The waiting state is per session**, worked out again by every sync (never stored); the waiting changes stay on the server and are fetched again from the same point. Plain version: nothing about the wait is saved that could go stale. | Accepted by default under D38 |
| D427 | **Shape additions** (D62): `ErrorCode::RevokedSignIn` (appended); `SignInReason::Revoked`'s words; the server's `revoked_sign_in` module; `VaultSharingView.held` and its desktop type; the app's `split_hold` module and `refresh_writers`. No route, command, wasm boundary or extension schema changed. Plain version: everything new was added beside what was there. | Accepted by default under D38 (structure) |
