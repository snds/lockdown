# 55: A member's key change, resolved without them

_Slice: `slice/member-key-change`, built 2026-11-04. Sean's decision of 2026-11-04 ("key change: go"): a planned key change is taken by every device with nothing shown; a reset after a lost recovery kit is let back in only by the vault's owner, with one calm question, and every other member's device follows the owner's signed change; a key that only the server or the log shows is never taken. Closes doc 49 §9's gap "A changed key never clears by itself". Builds on D248 and D551 (contacts pinned on first sight), doc 18 (contacts, the membership chain, re-adding a member), doc 47 (members' post-quantum keys), doc 49 (what doesn't verify is held back quietly, D570–D579) and doc 54. Proposed decisions D630–D639 (§10)._

## For future agent

- **TL;DR:** Every lookup of someone this device pinned now goes through `Contact::take_lookup`. The same keys continue the pin. New keys move the pin only when (a) the new identity's signed profile lists a **hand-over** the pinned keys signed (`lockdown.handovers`; a chain of them works), or (b) the owner of a shared vault this persona is in signed a membership change that names exactly the new keys (`Change::Add.keys`). Anything else leaves the pin where it was: nothing is wrapped to the new keys, what they sign is held back with doc 49's quiet line, and only the owner of something the person is in is asked "bob@example.com reset their account. Let them back in?". The owner's yes is one signed membership change per vault naming the new keys, with the vault key wrapped to them, plus a new share of each item shared with them on its own. Old keys are kept as former keys, so what they signed still verifies, and a lookup that shows them again is refused.
- **Read with:**
  - `crates/vault/src/key_change.rs`: `KeyHandover`, `HandoverKeys`, `handovers_of`, `publish_handover`, `walk`.
  - `crates/vault/src/contact.rs`: `Contact::take_lookup`, `Lookup`, `moved_to`, `FormerKeys`, `KeyMove`, `former_of`, `is_named_by`.
  - `crates/vault/src/sharing.rs`: `MemberKeys`, `Change::Add.keys`, `Roster::readmitted`, the owner-only rule in `check_next`.
  - `crates/vault/src/pq.rs`: `check_events_by`.
  - `crates/sync/src/sharing.rs`: `verify_directory_entry_unpinned`, `MembershipSince.identity_heads`.
  - `crates/app/src/session/sharing/key_change.rs`: `look_up`, `vouched_for`, `move_pin`, `note_key_change`, `follow_keys`, `let_back_in`, `key_resets`.
  - `crates/app/src/session/sharing.rs`: `pin` (deliberate steps: refuses a change) and `pin_quiet` (a sync's lookups).
  - `crates/app/src/session/sharing/pq.rs`: `pq_key_by`, `member_pq` (each identity under its own key).
  - `bins/server/src/api.rs`: `put_identity` (another profile or signing key only along a hand-over). `bins/server/src/sharing_api.rs`: `members_since` (the hint).
  - Desktop and web: `apps/desktop/src/features/sharing/key-resets.tsx`, `key-resets-words.ts`; `apps/web/src/features/people-view.tsx`.
  - CLI: `bins/cli/src/main.rs`, `sharing_command`.
- **Do not:**
  - Move a pin because the server's directory or log shows new keys. Only a hand-over the pinned keys signed, or the owner's signed membership change naming exactly those keys, moves it.
  - Let a manager who isn't the owner vouch for new keys. The roster refuses it, on every device and on the server.
  - Wrap any key (vault, item, emergency) to keys a pin hasn't moved to. `pin` refuses deliberate steps with the calm words; `pin_quiet` answers the old pin.
  - Drop a contact's former keys or their devices. What they signed must keep verifying, and their keys must never be taken again.
  - Check a former identity's records under the current post-quantum key, or the reverse (`pq_key_by`, `member_pq`, `check_events_by`).

## 1. Which real operations change a person's pinned keys

A contact pins a profile, its Identity Signing Key (ISK), its ML-DSA-65 key when published, one persona and that persona's receiving (KEM) key, and the identity's devices (doc 18 §3, D248).

| Operation | What it does to the pinned keys | How it is handled now |
|---|---|---|
| **A new device** (approval, QR, kit enrolment) | None. A new `Device` record signed by the same ISK | Unchanged: `check_continues` lets devices move forward. The members-list hint (§4) now has a member looked up as soon as their identity moves, not only when an unknown device writes |
| **A revoked device** | None. The device is marked revoked | Unchanged: revoked stays revoked |
| **Recovery with the kit** (new computer, forgotten password) | None. The kit rebuilds the same Identity Root Key, and the ISK, the post-quantum key and each persona's receiving key are derived from it (D19, D248) | Nothing to do |
| **A post-quantum key published for the first time** | Adds a key; nothing pinned changes | Unchanged: a pin without one takes it (doc 47) |
| **A persona added** | None for the pin: the lookup keeps the pinned persona | Unchanged |
| **A planned rotation** of the ISK, the post-quantum key or a receiving key | Would change them | **No such operation exists today.** The identity crate derives the ISK from the root key with a fixed label (D19: "rotation needs a counter field later"), the post-quantum key likewise (D248), and persona re-keying is open in the crate (doc 01 §10b). The hand-over record (§2) is built as the mechanism any future rotation uses, and the server already accepts it (§5) |
| **A reset after a lost kit** (a new identity under the same account) | Changes all of them | **Not a product path today.** The server pins each account's profile id and ISK and refuses an identity push that changes either (`put_identity`); nothing in the server, the app or the CLI starts over under an existing account. Today, then, a key change a contact sees can only come from the server's own hand: an operator editing its database, or an attack. The members' side is now ready for the operator reset when it is built (gap, §8) |

So the one key change a device can meet today is one the server made. That is exactly what Sean's third rule is about: the server's word alone never moves a pin.

## 2. The hand-over

**The record** (`lockdown_vault::key_change::KeyHandover`, format 1, canonical CBOR):

| Field | What |
|---|---|
| `from`, `to` | `HandoverKeys`: profile id, ISK, post-quantum key when published, every persona's receiving key |
| `personas` | Where each persona of `from` went in `to` |
| `at` | When |
| `from_sig`, `to_sig` | Ed25519 by the old and the new ISK over `lockdown-key-handover-v1\0` and the record with all four signatures cleared |
| `from_pq_sig`, `to_pq_sig` | ML-DSA-65 (context `lockdown key handover`) by each side's post-quantum key, **required** when that side names one, and refused when it doesn't |

**Where it lives:** on the new identity's own profile, signed with the rest of the profile by the new ISK, as `lockdown.handovers` (an array of the records' bytes, oldest first, at most 64). The profile travels in every directory entry, which the log records, so no new endpoint and no new record kind are needed (D62: an unknown profile field is kept as read; doc 01 §9). `publish_handover` adds one after checking it verifies and hands over to this identity's keys.

**Taking it:** `Contact::take_lookup` walks the list from the pinned keys (`key_change::walk`). Each step must start at the keys reached so far (profile, ISK, the pinned persona's receiving key, and the post-quantum key when one was pinned), verify in full, and is used once. When the walk reaches the keys the lookup shows, the pin moves (`KeyMove::HandedOver`). A hand-over that doesn't verify is passed over, never followed. The compared fingerprint carries over, because the compared keys signed the step.

**Once and never backwards:** the keys moved on from become `Contact.former` (with their devices, which stay in `devices` marked revoked, so what they signed still verifies). A lookup that shows a former ISK or receiving key is refused, vouched for or not. For the same profile, an identity manifest no newer than the pinned one is refused as before.

**What then clears:** the sync that moves the pin trusts the new devices in every open shared vault (`apply_roster`). A head the new keys signed, held back until then, verifies and is taken; doc 49's line goes by itself. The test shows it (§7).

## 3. A reset, and the owner's question

**Noticing.** A lookup that shows keys nothing vouches for (`Lookup::Changed`) leaves the pin as it was and records the fresh lookup for this session only (`Session.changed_keys`). Their devices' heads in a shared vault are then held back and named, instead of stopping the sync at an unknown signer. When this persona owns something the person is in (a shared vault it owns, or an item it shared with them on its own), the question is kept in the sharing record (`key_resets`, per device, beside the pins) until it is answered.

**Who is asked.** Only the owner. A manager who isn't the owner is not asked, because only the owner's signature roots the other members' trust: everyone's device pins the owner from the vault's first membership change, and the rule that the owner can't be removed or demoted is the one fixed point every device and the server share (doc 18 §4). A manager vouching for new keys would let any manager hand the vault to a key of their choosing.

**What the owner sees** (new words, flagged, D634):

| Where | Words |
|---|---|
| Desktop, Sharing screen, a row at the top (unicron `Item`, no dialog) | **bob@example.com reset their account. Let them back in?** with the line **Say yes if you know they did. Until then, what you share with them stays closed to them.** and two buttons, **Let them back in** and **Not now** |
| Web vault, People, the same row (the desktop's component) | The same |
| `lockdown sharing` | `bob@example.com reset their account. Let them back in?` and `  to let them back in: lockdown sharing let-back-in bob@example.com` (or `nobody is waiting on you to let them back in`) |
| After yes | Desktop and web notice: **bob@example.com can open what you share with them again.** CLI: `bob@example.com can open what you share with them again` |
| A deliberate step with them meanwhile (sharing a vault or an item, an explicit lookup) | **bob@example.com reset their account. Let them back in from Sharing first; nothing is shared with their new keys until then.** (someone not waiting on this persona: **bob@example.com's keys changed and nothing vouches for the new ones yet, so nothing is shared with them.**) |

The person is named by login only, never a device or a key. **Not now** hides the row until the screen is opened again; nothing is recorded, and the question stays until it is answered (it survives a lock and a restart). If the owner never answers, the person stays out, and the others see doc 49's quiet line at most.

**The yes** (`Session::let_back_in`, through the `Approve` gate as sharing a vault is, paused during a split):

1. The log is asked again; the keys must still be the ones asked about, or the question is refreshed and the yes refused ("…changed again since you were asked…").
2. For each vault this persona owns that they are in, from the server's newest membership: one membership change, signed by this device (and post-quantum when the owner's side signs), `Change::Add { member: <their new persona>, right: <the right they have>, keys: Some(MemberKeys { isk_pub, pq_isk, kem_pub }) }`, with the vault's current key wrapped to the new receiving key. The roster accepts it only from the owner, only for someone in the vault now, only with the right they have, and only under the same key generation; the server replays the same rule. Records held back in the vault stop it (D572), unless they are only heads the person's new keys signed.
3. Each item shared with them on its own is shared again to the new keys (a new share), and the share to the old keys is withdrawn. The server keeps one recipient per share.
4. The owner's pin moves (`KeyMove::LetBackIn`; the fingerprint shows as not compared).

## 4. Everyone else follows

After each vault's membership pull (`follow_keys`), a member is looked up again when:

- the owner's newest re-admission names keys for them this device hasn't moved to (`Roster::readmitted`), or
- the server's hint says their identity moved past the pin (`MembershipSince.identity_heads`, each current member's newest identity manifest seq and hash; absent from an older server). The hint only triggers a lookup checked against the log like any other. A server that hides it delays the question, and one that fakes it costs a lookup.

The lookup moves the pin when the owner's change names exactly the keys the log shows (`vouched_for`), and only then. Every chain in `self.rosters` checked out, so a vouch can't be forged by the server. Nobody is looked up during a split (D423).

**Items shared on their own and emergency grants.** The rule is the same: the share's owner decides. The owner of an item shared with someone who reset is asked, and their yes shares it again. Items and grants *from* a person who reset are re-shared by that person: their old shares keep opening (signed by their former devices, under their former post-quantum key), and new ones from the new keys are held back with the item or emergency line, until a pin moves by a hand-over or an owner's yes. Emergency grants *to* someone who reset are **not** re-sealed by the yes: emergency access needs a compared fingerprint (D195), and a reset changes the fingerprint. The owner names them again after comparing (D637, flagged).

**Post-quantum rules per identity.** A person's former identity keeps its own rule under its own key: their heads (`member_pq` gives each former identity its own `MemberPq`, under its profile id, with what this device remembered for them moved there, `HeadMemory::retire_member_pq_since`), their membership changes (`check_events_by`), and their grants, envelopes and item shares (`pq_key_by`, by the signing device).

## 5. The pieces

| Piece | What it does | Where |
|---|---|---|
| Hand-over | The record, its signatures, the profile field, publishing, the walk | `crates/vault/src/key_change.rs` |
| Pins | `take_lookup`, `moved_to`, `former`, `former_of`, `is_named_by`, `member_keys`, `pinned_keys`; `check_continues` keeps `former` | `crates/vault/src/contact.rs` |
| Re-admission | `MemberKeys`, `Change::Add.keys`, owner-only rule, `Roster::readmitted`, the history line "alice@example.com let bob@example.com back in after they reset their account" | `crates/vault/src/sharing.rs` |
| Post-quantum per identity | `check_events_by` | `crates/vault/src/pq.rs` |
| Directory check | `verify_directory_entry_unpinned` (the entry, its records, the profile's hand-overs; falls back to the receiving persona when the pinned one isn't there); `verify_directory_entry` is it plus `check_continues` | `crates/sync/src/sharing.rs` |
| The hint | `MembershipSince.identity_heads` | `crates/sync/src/sharing.rs`, `bins/server/src/sharing_api.rs` |
| Server | `put_identity` takes another profile or ISK for an account only when the new profile lists a hand-over from the account's pinned key to the new one; the account then moves (`IdentityAccept.handed_over_to`). A forged hand-over, or none, is refused as before | `bins/server/src/api.rs`, `storage.rs` |
| Memory | `HeadMemory::retire_member_pq_since` | `crates/store/src/remembered.rs` |
| Session | `look_up`, `pin` / `pin_quiet`, `changed_keys`, `changed_devices` (holdable and named in the vault loop), `follow_keys`, `let_back_in`, `key_resets`, `retrust` | `crates/app/src/session/sharing/key_change.rs`, `sharing.rs`, `session.rs` |
| Record | `SharingState::{key_resets, put_key_reset, clear_key_reset}`, `KeyReset` | `crates/app/src/sharing.rs` |
| App | `App::key_resets`, `App::let_back_in` (the `Approve` gate); `SharingOverview.resets` (`KeyResetView`), `reset_question` | `crates/app/src/sharing.rs` |
| Desktop | `sharing_let_back_in` command (end of the handler list); `KeyResets` row; `SharingClient.letBackIn` | `apps/desktop/src-tauri/src/sharing.rs`, `apps/desktop/src/features/sharing/` |
| Web vault | `sharing_let_back_in` command; the row in People | `crates/web/src/commands/finish.rs`, `apps/web/src/core/web-finish-client.ts`, `apps/web/src/features/people-view.tsx` |
| CLI | `lockdown sharing` (lists, the default) and `lockdown sharing let-back-in <login>` (the account password, or the master password on a sealed vault, as the gate) | `bins/cli/src/main.rs` |

## 6. Wire and file additions (D62, additive)

None renamed, removed or retyped; each is absent when empty.

- **Sync wire (CBOR):** `Change::Add.keys` (`MemberKeys`). It is inside the signed event, as it must be: an older build decodes the change, drops the field, and its signature no longer checks, so a build from slice 48 on holds that change back quietly and an older one refuses the chain until it updates (§8). `MembershipSince.identity_heads`.
- **Identity profile:** `lockdown.handovers`.
- **Files:** `Contact.former` (the sharing record's pins); `Body.key_resets` in `sharing-<persona>.cbor`. An older build drops them on its next write (§8).
- **Views:** `SharingOverview.resets` (`KeyResetView`). No schema file covers the sharing views, so none moved.
- **Commands:** desktop IPC `sharing_let_back_in`, appended; web command `sharing_let_back_in`, added; CLI `lockdown sharing`. No wasm boundary or extension schema changed.
- **API:** `lockdown_vault::key_change::*`; `Contact::{take_lookup, moved_to, former_of, is_named_by, member_keys, pinned_keys}`, `contact::{Lookup, KeyMove, FormerKeys}`; `sharing::MemberKeys`, `Roster::readmitted`; `pq::check_events_by`; `lockdown_sync::sharing::verify_directory_entry_unpinned`; `HeadMemory::retire_member_pq_since`; the server's `IdentityAccept.handed_over_to`.

## 7. How to run and test

```sh
export CARGO_TARGET_DIR=… CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=line-tables-only
cargo test -p lockdown-vault --lib key_change contact:: only_the_owner   # the record, the walk, the pin, the roster
cargo test -p lockdown-store --lib remembered                            # the memory moves to the former keys
cargo test -p lockdown-server --test api an_account_moves                # the server moves an account only along a hand-over
cargo test -p lockdown-app --test member_key_change                      # three identities, a hostile server
cargo test -p lockdown-web --test member_key_change                      # the web vault
cargo test -p lockdown-cli --test member_key_change                      # lockdown sharing
cd apps/desktop && pnpm test                                             # the row (Vitest)
cd apps/web && pnpm test                                                 # the web client's command (Vitest)
```

| Test | What it proves |
|---|---|
| `key_change::tests::a_hand_over_is_signed_by_both_sides_and_anything_less_is_refused` | Both Ed25519 and both post-quantum signatures; a hand-over the new key alone signed, a stripped or bent post-quantum one, a changed field, a stray persona, a post-quantum signature with no key: refused |
| `key_change::tests::a_chain_of_hand_overs_walks_forward_once_and_never_back` | One step, two in any order, never backwards, a gap stops it, a forged link is passed over, a pin from before the post-quantum key still follows, another receiving key doesn't |
| `key_change::tests::the_new_identity_lists_its_hand_overs_on_its_signed_profile` | Published once, only by the identity it hands over to, never a forged one |
| `contact::tests::a_new_key_moves_the_pin_only_along_a_hand_over_or_a_vouch_and_never_back` | Same keys continue; new keys with nothing: `Changed`; the owner's vouch for exactly those keys moves the pin (not compared); a vouch for other keys doesn't; a hand-over moves it silently and carries the comparison; old devices stay, revoked; old keys again refused; a chain of two; another account refused |
| `sharing::tests::only_the_owner_lets_a_member_back_in_under_new_keys_and_the_roster_names_them` | A manager can't; not a change of right or key; not for a stranger; adding again without keys still refused; the owner's names the keys in the roster; the server's replay agrees; an older build can't verify it |
| `remembered::tests::a_members_memory_moves_to_their_former_keys_when_their_keys_change` | The memory moves |
| `bins/server/tests/api.rs::an_account_moves_to_new_keys_only_along_a_hand_over_its_pinned_key_signed` | No hand-over or a forged one: refused; the real one: accepted, the account moves, the old key can't write after |
| `crates/app/tests/member_key_change.rs::a_hand_over_moves_every_pin_silently_and_what_was_held_back_clears` | New keys appear before their hand-over: Carol holds back their head with the line and keeps her pin, Alice (the owner) is asked, Carol isn't. The hand-over arrives: both pins move with nothing shown, the head is taken, the line and the question go. Bob's older writes still open. The server showing his old keys again: refused, even to a deliberate lookup |
| `…::a_reset_is_let_back_in_by_the_owner_alone_and_everyone_else_follows` | Alice is asked in the exact words; Carol, a manager, isn't; the question survives a lock; sharing with Bob meanwhile is refused; the server's envelope doesn't open for the new keys. Carol holds back their head. Alice's yes: one change she signed names the new keys, the envelope opens for them. Carol's pin follows, the held head is taken, the line goes |
| `…::a_server_that_fakes_a_reset_or_forges_a_hand_over_never_gets_the_vault_key` | A forged hand-over (the server's key on both sides) moves nobody's pin; Alice is asked and doesn't answer; she removes Carol and the vault moves to a new key: Bob's new envelope opens for his pinned keys, never the server's; no change names the server's keys; sharing an item with Bob is refused in words |
| `crates/web/tests/member_key_change.rs` | The web vault: the question in the overview, absent when none; the yes refused without the gate and done with it; the vault key opens for the new keys only after |
| `bins/cli/tests/member_key_change.rs` | `lockdown sharing` lists nothing, then the question; `let-back-in` for someone not waiting and with a wrong password refused; yes done; the app's pin moved |
| `sharing-view.test.tsx` (Vitest) | The row: the question and its line, no dialog, Not now hides it, the screen opened again asks again, yes through the gate, the notice; no row when nobody waits |
| `web-finish-client.test.ts` (Vitest) | The web client sends `sharing_let_back_in` with and without the gate |

## 8. Gaps

- **The reset itself isn't built** (§1). An operator's "let this person start over under the same account" needs the server to accept a new profile and password for an existing account and to keep the account's identity chain going (the clients' own-account audit would otherwise flag the old manifests). The members' side here is ready for it; the tests stand in for it with the server's own hand on its database.
- **No identity can rotate yet** (§1). The hand-over is ready, and the server accepts it, but the identity crate needs a rotation counter (D19) before a device can make one. That repo isn't changed here.
- **An older build can't verify a re-admission** (§6): from slice 48 on it holds the owner's change back quietly; before that it refuses the chain until it updates. On the owner's own older device, the change is its own identity's and is refused, as doc 28 says, until that device updates.
- **An older build drops `former` and `key_resets`** when it next writes the sharing record. The question comes back at the next sync if still unanswered; a dropped `former` loses the never-backwards memory on that device (the server's hint and the owner's change still can't move a pin to keys nothing vouches for).
- **Former devices still verify new writes.** A former identity's devices are kept, marked revoked, so their old heads and versions verify; like a revoked contact device (doc 18 §10), nothing tells a later head they sign from an old one. The server refuses writes from devices that no longer hold a session.
- **Emergency grants to someone who reset aren't re-sealed** (§4, D637): the owner names them again after comparing fingerprints.
- **A server can stall the yes** by holding back other records of the vault (D572 stops membership changes while anything but the person's own heads is held back), or by hiding the hint. Neither lets it in.
- **The owner who resets** has no one to vouch for them: their members keep the vault at the old pin, and a new owner can't take over. Moving ownership isn't built.
- **One lookup per sync** for a member whose change is unresolved, while the server's hint differs from the pin.

**unicron host-contract gaps:** none hit. The row is `ItemGroup` / `Item` / `ItemContent` / `ItemTitle` / `ItemDescription` / `ItemActions` / `Button`, as the Sharing screen's other lists. The ratchet stays under baseline (3 overrides, baseline 4).

## 9. Security notes

- The only things that move a pin: a signature by the pinned keys (the hand-over), or a signature by the owner's device, whose account this device pinned from the vault's first change (the re-admission). The server's directory, its log and its hint only ever start a lookup.
- The owner's yes is the one decision a hostile server can try to provoke. The question's line asks the owner to say yes only if they know the person reset, and the yes goes through the `Approve` gate.
- Nothing is wrapped to unvouched keys: `pin` refuses every deliberate step, and every sync path wraps only to the pin (`pin_quiet` answers the old pin). A re-key after a removal wraps to the pinned keys (the hostile-server test).
- Logs carry no keys: the question names a login, the Status detail names the record and the check.

## 10. Proposed decisions

For the coordinator to record. Every row is a security-model or user-visible choice, so all are audit items (D38). **Flagged** rows are the ones Sean should see.

| ID | Decision | Status |
|---|---|---|
| D630 | **A planned key change is a signed hand-over.** `KeyHandover` names the old and new keys (profile, signing key, post-quantum key, every persona's receiving key, where each persona went), signed by both signing keys and both post-quantum keys where they exist, listed oldest first on the new identity's signed profile (`lockdown.handovers`). A lookup that walks a valid chain from the pin to the keys the log shows moves the pin with nothing shown; once, never backwards; the fingerprint comparison carries over. Plain version: when someone's app changes their keys on purpose, the old keys vouch for the new ones and everyone's Lockdown takes them silently. | Proposed; audit item |
| D631 | **Old keys are kept, never taken again.** A moved pin keeps the keys it left (`Contact.former`) and their devices, marked revoked, so what they signed still verifies under their own keys and post-quantum rules; a lookup that shows them again is refused. Plain version: what someone wrote before their keys changed still opens, and their old keys can't come back. | Proposed; audit item |
| D632 | **A key change nothing vouches for leaves the pin.** The new keys are only named, never trusted: nothing is wrapped to them, what they sign is held back with doc 49's quiet line, and deliberate steps with that person are refused in calm words. Replaces "forget the contact and compare fingerprints" as the only way out (doc 47 §2, doc 49 §9). Plain version: if someone's keys change and nobody vouches for the new ones, Lockdown keeps using the old ones and waits. | Proposed; audit item |
| D633 | **Only the vault's owner lets someone back in.** The re-admission is `Change::Add` with `keys` (the new signing, post-quantum and receiving keys), allowed only by the owner, only for a current member, with the right they have and the same key generation, and the vault key wrapped to the new receiving key. A manager who isn't the owner can't, because only the owner's signature roots every member's trust in the vault. Every other member's device moves its pin when the owner's change names exactly the keys the log shows, and only then. Plain version: when someone resets their account, the vault's owner decides whether they come back, and everyone else's Lockdown follows the owner. | Proposed; audit item; **flagged for Sean** (only the owner, not managers) |
| D634 | **The owner's question, word for word**: "bob@example.com reset their account. Let them back in?" with "Say yes if you know they did. Until then, what you share with them stays closed to them.", buttons "Let them back in" and "Not now"; a row at the top of Sharing (desktop) and People (web vault), never a dialog; after yes, "bob@example.com can open what you share with them again."; the CLI's `lockdown sharing` and `lockdown sharing let-back-in <login>`; the refusal "bob@example.com reset their account. Let them back in from Sharing first; nothing is shared with their new keys until then." Not now hides the row until the screen opens again; the question stays until answered, across locks and restarts. Plain version: the owner sees one calm question with yes and not now. | Proposed; **flagged for Sean** (new words, the second line, where it sits) |
| D635 | **The yes goes through the `Approve` gate**, as sharing a vault does, checks the log still shows the keys asked about, and is paused during a split. Records held back in the vault stop it (D572) unless they are only the person's own new heads. Plain version: saying yes asks for your password, like sharing a vault. | Proposed; audit item; **flagged for Sean** (a password at the yes) |
| D636 | **Items shared on their own follow the same rule**: the share's owner is asked, and their yes shares each item again to the new keys and withdraws the old share. Shares from a person who reset are theirs to make again; their old ones keep opening. Plain version: things you shared one at a time come back with the same yes. | Proposed; audit item |
| D637 | **Emergency access isn't re-sealed by the yes.** A grant to someone who reset stays as it was (it can't open for the new keys); the owner names them again after comparing fingerprints, as D195 requires for emergency access. Plain version: emergency access needs you to check their new numbers once. | Proposed; audit item; **flagged for Sean** (the one manual step left) |
| D638 | **The server moves an account to new keys only along a hand-over** its pinned key signed (`put_identity`), and tells each member's devices where every member's identity stands (`MembershipSince.identity_heads`) so they look someone up again at once. Both are hints or gates for the server; every device still decides against its own pin. Plain version: the server won't accept new keys for your account unless your old keys vouched for them. | Proposed; audit item |
| D639 | **Wire and file additions (D62, additive)**: §6's fields, profile field, commands and API. `Change::Add.keys` is signed, so an older build holds the re-admission back (slice 48 on) or refuses the chain (before). Plain version: older and newer apps keep understanding each other, except that an old app can't take a "let back in" until it updates. | Proposed; audit item |
