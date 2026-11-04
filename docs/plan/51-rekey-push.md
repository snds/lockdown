# 51: Re-keyed items reach the server before their new keys go out

_Slice: `slice/rekey-push`, built 2026-11-04. Fixes D588 (doc 50 §6 and §8): removing a member from a shared vault, or withdrawing one share of an item, broke everyone else's copies. Builds on doc 18 (rotation on removal, items shared on their own, emergency access), doc 36 (shared vaults on every device), doc 49 (slice 48's quiet rules: a record that doesn't check out is held back, never shown as an attack), doc 50 (holders of items shared on their own), doc 28 (post-quantum heads) and D36 (key envelopes sit outside every signature and hash). No wire, IPC, wasm, view or extension schema changes. Two additive fields in the device's own sealed sharing record. Proposed decisions D590–D599 (§9)._

## For future agent

- **TL;DR:** A re-key used to move an item to a fresh Item Key without a new version. `Vault::changed_since` compares the versions a head signed, so it never saw the move, and the item never left the device. The owner then sealed the *new* key into every share that stayed, and recipients got a key for a copy the server didn't serve. Their whole `list()` failed. Now every re-key also writes one **re-key version**: the newest fields sealed again under a fresh Version Key wrapped by the fresh Item Key, signed by the re-keying device, with the newest version's own `edited_at`. Sync pushes it like any edit. The new keys go out (item shares, and after a new Vault Key the emergency grants) only once the vault's re-keyed items are on the server. A recipient whose new share doesn't open the served copy yet keeps the copy that did, quietly. One item that doesn't open is left out of the list instead of failing it. Each device compares its vaults with the server once and repairs items an earlier build re-keyed and never pushed.
- **Read with:**
  - `crates/vault/src/vault/share.rs`: `rotate_vault_key`, `rekey_item`, `write_rekey_version`, `reseal_item`, `reseal_unpushed`, `Vault::received`, `received_key_opens`.
  - `crates/vault/src/vault.rs`: `changed_since` (unchanged, documented).
  - `crates/app/src/session/sharing/rekey.rs`: `repair_rekeyed`, `after_rekey_push`, `all_pushed`, `reseal_shares` (was `after_rotation`), `received_opens`.
  - `crates/app/src/session/sharing.rs`: `remove_member`, `unshare_item`, `take_new_key`, `sync_received`.
  - `crates/app/src/session.rs`: `sync_round` (the two calls around the vault loop), `list`.
  - `crates/app/src/sharing.rs`: `Reseal`, `SharingState::mark_reseal` / `resealed` / `rekey_checked` / `mark_rekey_checked`.
  - Tests: `crates/app/tests/rekey_push.rs`, `crates/web/tests/rekey_push.rs`, the `share` unit tests in `lockdown-vault`.
- **Do not:**
  - Move an Item Key without writing the re-key version. `move_item_to_fresh_key` is private for that reason, and only `rotate_vault_key` and `rekey_item` call it.
  - Seal a new item key or Vault Key into a share before the vault's re-keyed items are on the server. Mark the vault (`mark_reseal`), and let the end of the sync seal it.
  - Treat a share whose key doesn't open the served copy as an attack. It is held quietly, as a copy on its way (doc 49's rule).
  - Let one item that doesn't open fail a whole list or walk. Leave it out.

## 1. The fix, and why this one

Two fixes were possible: carry key-only changes in `changed_since` and the push, or write a new version. This slice writes a new version (D590). The reasons:

- **The server would refuse a key-only push.** `push_item` checks the newest version's signer against the vault's current writers (`writer_key`). An item last edited by the member being removed would be refused with `UnknownDevice`, the very item that most needs its new key. The re-key version is signed by the re-keying device, which is a writer.
- **Every other device takes a pushed item only if the head lists it** at the revision and version hash it carries. A key-only change keeps the version hash, so its own tracking would need a new field in the manifest entry (a wire change, D62) or a local list of unpushed keys that an older build can't keep. The re-key version changes the hash. The head, the server's checks, the pull and the merge all work unchanged.
- **D36 stays as it is.** Earlier versions keep every signed byte. Their Version and Attachment Keys are re-wrapped under the fresh Item Key as before. The removed member's old keys still read only what they could already read. The re-key version and everything after it are under keys they never held.

**What changes in version history.** Each re-keyed item gets one more version (`n + 1`), with the same fields and attachments as the newest one. It carries the newest version's `edited_at`, so the list's "newest edit first" order and the item's edited time don't move. `edited_by` is the re-keying device. History shows one more entry for each item in a vault someone was removed from, and for an item whose share was withdrawn (D591, flagged). Nothing is written for an item with nothing left to read: purged, newest version shredded, or one this device can't open.

**Post-quantum heads (doc 28).** Nothing new. The re-key version is an item version, and versions carry only Ed25519 (as every edit does). The head committed after it is signed by `commit_head` under the device's own post-quantum rule, as any head is.

**The server's checks.** All unchanged and all met. `remove_member` posts the membership change (which records the new key generation) before its sync, so the pushed items are under the generation in force (`check_item_generation`) and the head names it (`check_head_generation`). The newest version's signer is the re-keying device, a current writer (`writer_key`). `rekey_item` keeps the generation.

## 2. Order: the server first, then the keys

| Step | Remove a member | Withdraw one share |
|---|---|---|
| 1 | `rotate_vault_key`: fresh Vault Key, fresh Item Keys, a re-key version each, one head | The server drops the share; `rekey_item`: a fresh Item Key and a re-key version |
| 2 | The vault is marked in the sharing record (`mark_reseal(vault, rotated)`) and the record saved | The item is marked (`mark_reseal(vault, items)`) and the record saved |
| 3 | The membership change goes up with the new Vault Key for everyone who stays (they keep the old one, so nothing they hold stops opening) | |
| 4 | Sync: the re-key versions and the head are pushed | Sync: the re-key version and the head are pushed |
| 5 | At the end of that sync (`after_rekey_push`), once nothing of the vault is unpushed (`all_pushed`): every item shared on its own from the vault, and every emergency grant covering it, gets an envelope for the new keys | Same, for the shares of that item still standing |

If the sync fails, or the device restarts between steps, the mark stays and the next sync that leaves the vault pushed does step 5. A share whose recipient isn't pinned here, or whose item is gone, is passed over. A share whose item is in the trash can't be sealed (`seal_item_key` takes only a live item), so it waits, marked by item, and is sealed at the first sync after the item is restored. A refusal from the server leaves the mark for the next sync.

When the owner's device takes a newer Vault Key it didn't make (`take_new_key`: another device of the owner, or a manager, rotated), it marks the vault too. Its shares follow the new key after its own sync.

**What a recipient sees in between.** A person holding an item shared on its own can sync at any moment:

- Before step 4 they get the old copy and the old key. It opens.
- Between step 4 and step 5 the server serves the re-keyed copy while their share still carries the old key. `sync_received` takes the share as before (every check of doc 49 applies), then asks `received_opens` of the new pair. It doesn't open, so the device keeps the copy it already had, which opens. Nothing is held back and no line shows.
- After step 5 the new pair opens and replaces the old one.
- A person who never had a copy (their first sync falls between step 4 and step 5) keeps the new pair in the record. The holder leaves that item out (`Vault::received` checks `received_key_opens`). Every other item still lists. The item comes in at the sync after step 5.

## 3. One item never fails a list

- **Holders.** `Vault::received` used to fail as a whole when one item's newest version wasn't signed by a trusted device. `rebuild_received` then dropped the whole holder, so every item from that vault vanished. Now an item is left out on its own when its signer isn't trusted, its signature doesn't check, or its key doesn't open its newest version. A malformed record (an item naming another vault) is still refused.
- **`list()`** leaves out an item whose `open_item` fails, and the rest of the vault lists. So do the other item walks: codes, the security center's facts, duplicates, passkeys, the away reminders and their pick list, and the bridges' logins, code items and alias items. The CLI's `list` and `search` do the same.
- **What the person sees.** Nothing for the left-out item: it isn't in the list, search or quick access until a copy that opens arrives. Sharing's "Received" list still shows the share itself, because it is drawn from the sharing record. No line is shown (slice 48's quiet rules; D594, flagged).

## 4. Repair: items an earlier build re-keyed and never pushed

Once per vault on each device (`repair_rekeyed`, before the vault loop of a sync), for every open vault this persona may change, the device asks the server for every item (`items_since(0)`) and hands the copies to `Vault::reseal_unpushed`. An item served at the same revision and newest version as this device's, but under a different Item Key envelope, was re-keyed here and never pushed. It gets its re-key version under the key this device holds, which is the key the earlier build sealed into the shares. The vault is marked for step 5 (rotated when the envelope moved to another generation). The vault is recorded as compared (`rekey_checked`) at the end of the first sync that leaves it pushed. A vault whose items the server doesn't send right now is compared at a later sync.

A recipient who already holds a non-matching share (the earlier build's key, the old copy) keeps it in the record. The holder leaves the item out, and every other item lists. Once the owner's device repairs and pushes, the next sync brings the matching copy and the item opens.

## 5. The pieces

- `Vault::write_rekey_version` (private): the re-key version, not committed.
- `Vault::rotate_vault_key` / `rekey_item`: move keys, write the version, commit one head.
- `Vault::reseal_item`: a re-key version for an item already under its key, plus a head.
- `Vault::reseal_unpushed(served)`: the repair's comparison.
- `received_key_opens(ctx, item, key)`: whether an Item Key unwraps the newest version's Version Key in that vault's place.
- `Session::repair_rekeyed`, `after_rekey_push`, `reseal_shares`, `received_opens` (`session/sharing/rekey.rs`).
- `SharingState`: `reseal` (vault to `Reseal { rotated, items }`) and `rekey_checked` (vault ids). Both are in the device's own sealed record, `serde(default)`, left out when empty.

## 6. How to run and test

```
export CARGO_PROFILE_DEV_DEBUG=line-tables-only
cargo test -p lockdown-vault --lib share
cargo test -p lockdown-app --test rekey_push
cargo test -p lockdown-web --test rekey_push
```

- `removing_a_member_keeps_everyone_else_opening_the_vault_and_its_shares`: Alice shares "Trip" with Bob and Carol and one login of it with Dave on its own, then removes Carol. The login has one more version and keeps its edited time. Bob opens both items, Dave opens his login, and Carol has neither. Erin, added afterwards with only the new Vault Key, opens everything. Later syncs, and Dave's lock and unlock, change nothing.
- `withdrawing_one_share_keeps_the_others_opening`: Mail goes to Bob and Carol, and Shop to Bob. Carol's share of Mail is withdrawn. Only Mail gets a version. Bob opens both, and Carol no longer lists Mail.
- `a_recipient_who_syncs_while_a_new_key_waits_still_opens_and_the_rest_still_lists`: both Trip items go to Dave and Erin on their own. Rail is in the trash when Bob is removed, so its shares wait. Dave, who synced before, keeps the copy he had, and it opens. Erin, who never synced, holds a share that doesn't open the served copy: Rail is left out, Hotel lists and opens, and her sync shows no line about it. Alice restores Rail. After her next sync both open it, live.
- `a_member_removed_from_a_vault_keeps_an_item_still_shared_on_its_own`: slice 49's "loses the vault" case, now through a real removal. Bob holds Rail on its own and the whole vault. Removed from the vault, his next sync closes it, and Rail is back as received, under its new key, alone. It still opens after a lock and unlock.
- Web: `a_tab_keeps_opening_its_shares_after_a_removal_and_a_withdrawal`: Dave signs in as a tab, holding one Trip login and one of Alice's own logins on their own. Alice removes Carol from Trip and withdraws Carol's share of the other login. The tab lists and reveals both, Bob still opens Trip, and Carol has neither.
- Vault unit tests: rotation writes a version that keeps the time and files and is seen by `changed_since`. A re-keyed share waits beside the copy from before the re-key, and the rest is held. An earlier build's key-only move is found by `reseal_unpushed` (one generation, and a rotation). `reseal_item` writes nothing for a shredded or purged item.

All four app tests and the web test fail on main. The "loses the vault" test and the web test fail with "this data didn't open" (the web test at `list`). The removal and withdrawal tests fail at the version count. The trash test fails at the removal itself: on main, `after_rotation` refused the item in the trash ("only a live item can be shared"), so removing a member failed whenever an item shared from the vault was in the trash.

The CLI doesn't list items shared on their own, so it has no share test here. Its `list` and `search` leave out an item that doesn't open, as the app's do.

## 7. Unicron host-contract gaps

None hit. The slice is core-only, and no host surface changed.

## 8. Gaps

- **An item shared on its own whose newest version another member signed** doesn't open for its recipient. The recipient trusts only the owner's devices (doc 18). This was already so for a member's edit. Before this slice it failed the recipient's whole holder; now the recipient keeps the last copy that opened. With this slice it also covers a rotation a *manager* runs: the re-key versions are the manager's, so recipients of the owner's shares keep their last copy until the owner edits the item (D596).
- **The repair needs the device that re-keyed.** Only that device holds the fresh keys an earlier build sealed into the shares. If it is gone, the owner's other devices can't repair. Withdrawing and sharing the item again does.
- **The CLI's own sync doesn't run the repair or the re-seal.** The CLI never removes a member or withdraws a share, so it never marks one. A device that syncs only through the CLI repairs at its next app or web sync.
- **The repair downloads every item of each writable vault once per device.** It is about as much as one bootstrap of the vault, with no attachments.

## 9. Proposed decisions

| # | Decision | Status |
|---|---|---|
| D590 | **A re-key writes a version.** Removing a member or withdrawing one share gives each item a fresh Item Key and one re-key version: the newest fields sealed again under keys wrapped by it, signed by the re-keying device, synced like any edit. Earlier versions keep every signed byte (D36). Not chosen: a key-only push, which the server's writer check refuses for an item a removed member edited last, and which would need a new manifest field. Plain version: when you remove someone or stop sharing an item, Lockdown saves the item again under its new key so everyone who still has access gets it. | Proposed; audit item |
| D591 | **The re-key version shows in history and keeps the time.** It has the same fields and files, the newest version's edit time and the re-keying device as its editor. The list's order and edited times don't move; history has one more entry. Plain version: an item's history shows one extra save when its keys changed; nothing in it changed. | Proposed; **flagged for Sean** (history shows one more entry) |
| D592 | **New keys go out only after the re-keyed items are on the server.** The vault is marked in the device's sharing record. Item shares, and after a new Vault Key the emergency grants, are sealed at the end of the first sync that leaves the vault pushed; a failure or restart only delays it. An item in the trash waits until it is restored. Plain version: the people you still share with get the new key only once it can open what the server holds. | Proposed; audit item |
| D593 | **A share whose key doesn't open the served copy is a copy on its way, not an attack.** The device keeps the copy it had while that opens, takes the new one at a later sync, and shows nothing. With no earlier copy, the item waits out of the list. Plain version: while an owner's change is on its way, you keep seeing the item as it was. | Proposed; audit item |
| D594 | **One item that doesn't open is left out; the rest lists.** It applies to `list()`, the other item walks, the holder of items shared on their own, and the CLI's `list` and `search`. No line is shown, and Sharing's "Received" still lists the share. Plain version: one unreadable item no longer hides your whole vault. | Proposed; **flagged for Sean** (nothing says an item is missing) |
| D595 | **A one-time repair per vault and device.** Each device compares its writable vaults with the server once. It writes the re-key version of any item an earlier build re-keyed and never pushed, then seals that vault's shares again. It is recorded in the device's sealed sharing record. Plain version: shares an earlier version broke fix themselves the next time the owner's device syncs. | Proposed; audit item |
| D596 | **Recorded gap: a manager's rotation, or a member's edit, isn't readable to recipients of the owner's single-item shares.** They trust only the owner's devices (doc 18), and now keep the last copy that opened. Plain version: if someone other than the owner changes a shared vault's item, a person who has only that item keeps the older copy. | Proposed; recorded gap; **flagged for Sean** |
| D597 | **The owner's device follows a rotation it didn't make.** When it takes a newer Vault Key (another device of its own, or a manager), it seals its shares and grants for that vault again after its next sync. Plain version: your shares keep working when someone else removes a member from your vault. | Proposed |
| D598 | **Local record additions only.** `Reseal` and the `reseal` / `rekey_checked` fields are in the device's sealed sharing record, `serde(default)`. No wire, IPC, wasm, view or extension schema change. Plain version: older and newer apps keep understanding each other. | Accepted by default under D38 |
