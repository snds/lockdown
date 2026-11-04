# 50: An item shared on its own doesn't make its vault "open"

_Slice: `slice/received-vs-open`, built 2026-11-04. Sean's task: "Fix emergency release over a vault an item came from". Builds on doc 18 (items shared on their own, emergency access), doc 36 (shared vaults on every device), doc 47 §5 and doc 49 (slice 48: held-back records, kept as they were). No wire, IPC, wasm or view schema changes. Proposed decisions D580–D589 (§8)._

## For future agent

- **TL;DR:** An item someone shares on its own is held in memory in a read-only "holder" `Vault`. The holder carries the context of the vault the item came from, so its `vault_id` is that vault's id. `Session::vaults()` lists holders after the real vaults, so `self.vaults().any(|v| v.context().vault_id == id)` answered "open" for a vault that wasn't. An emergency release over that vault skipped the bootstrap and then failed with "vault … isn't open here". Sync skipped joining a shared vault the person had just been added to, and its roster step failed the sync. Now `Session::is_open` and `Session::open_vaults` ask only about real vaults (`opened` and `more`). Every "is this vault open or joined" check uses them. `vaults()` stays for reading items. When a real vault opens, its holder goes, so the item is listed once, from the vault's own copy, under the vault's right. When the vault closes again (a release revoked, a membership ended), the item comes back as received.
- **Read with:**
  - `crates/app/src/session.rs`: `vaults`, `open_vaults`, `is_open`, `holding`, `holding_mut`.
  - `crates/app/src/session/sharing.rs`: `drop_received`, `rebuild_received`, `open_shared_vault`, `close_vault`, `item_right`, `load_sharing`, `sync_sharing`, `pull_membership`, `join_shared`, `take_new_key`, `refresh_writers`, `take_release`, `take_released_items`.
  - `crates/app/src/session/organize.rs`: `vault_ids`, `open_new_vaults`, `create_vault`.
  - `crates/app/src/session/sharing/pq.rs` (`refresh_unready`), `crates/app/src/session/quiet.rs` (`unverified_lines`).
  - Tests: `crates/app/tests/received_vs_open.rs`, `crates/web/tests/received_vs_open.rs`.
- **Do not:**
  - Ask "is this vault open?" with `vaults()`. Use `is_open`, or `open_by_id` / `vault_slot` when you need the vault itself.
  - Write to, re-key or apply a roster to a holder. It has no Vault Key and is never saved.
  - Delete a received share from the sharing record because its vault opened. The record is what brings the item back when the vault closes.

## 1. The bug, check by check

Each check below used `self.vaults().any(…)` or `vault_by_id`, which a holder answers. What it asks now:

| Where | Before | Now |
|---|---|---|
| `take_release` (emergency release) | Skipped `bootstrap_member_vault` when a holder had the id; then `vault_slot` failed, so the release failed | `is_open`: bootstraps the vault, opens it, takes its key and the released crown jewels; when it is really open, compares keys through `open_by_id` |
| `open_shared_vault` (unlock, bootstrap) | Returned early on a holder, so the vault never joined `more` | `is_open`; after it opens, the holder goes (`drop_received`) |
| `sync_sharing` (join) | `join_shared` skipped, so the person never joined | `is_open` |
| `load_sharing`, `pull_membership`, `refresh_writers`, `refresh_unready` (`apply_roster` callers) | Applied the roster to the holder; `vault_slot` failed and stopped the sync (and, at `load_sharing`, the unlock could stop) | `is_open`: the roster waits until the vault really opens |
| `take_new_key` | `vault_by_id` could return the holder; its key never matched, so it fetched an envelope and then failed in `vault_slot` | `open_by_id`: only a real vault moves to a new key |
| `open_new_vaults` (`vault_ids`) | A store vault with a holder for its id was never opened | Real vaults only; the holder goes once the vault opens |
| `unverified_lines` | A vault only received from counted as open, so its held-back line was the vault's line, not the join line | `is_open` |
| `create_vault` | Holders counted toward the vault limit and the next vault's order | Real vaults only |

The other `vaults()` callers read items: the list, search, quick access, codes, passkeys, bridges, depth, duplicates, exposure, recovery, transfer and status counts. They keep `vaults()`. The other `vault_by_id` callers are covered already. Some only read a name (emergency names, households, held-back names, link targets, templates). The rest refuse a holder first: `vault_right` answers `View` for a holder, so `check_shareable`, `share_vault`, `add_emergency_contact` and `move_item` refuse it, and `vault_by_id_mut` refuses through `require_edit`. Some are the owner's own vaults (`after_rotation`, `put_emergency_items`, `unshare_item`).

## 2. What happens to a held item

- **The vault opens for real** (an emergency release, a join at sync, an open at unlock, or `open_new_vaults`): `drop_received` removes the holder for that vault id at once, and `rebuild_received` skips every vault that is open. The item is listed once, from the vault's copy. Edits go to the vault, as its right allows. `item_right` follows the vault's right while the vault is open, and the share's right only while the item is held as received.
- **The share stays in the sharing record** while the vault is open, and each sync refreshes it as before. Sharing's "Received" list still shows it, because it is still a share.
- **The vault closes** (`close_vault`: a release revoked, a membership gone): `close_vault` rebuilds the holders, and the item comes back as received, under its share's right. If the share was withdrawn meanwhile, the item goes.
- **At unlock**, `load_sharing` opens the shared and released vaults first, then builds the holders, which skip the open vaults.

## 3. The pieces

- `Session::open_vaults()`: the real vaults, first one first. `vaults()` is `open_vaults()` followed by the holders.
- `Session::is_open(id)`: the one test for "open or joined".
- `Session::drop_received(id)`: called when `open_shared_vault` or `open_new_vaults` pushes a vault into `more`.
- `rebuild_received`: builds holders only for vaults that aren't open.
- `close_vault`: rebuilds the holders after a vault leaves `more`.
- `item_right`: uses the share's right only when the item sits in a holder (or isn't found).

## 4. How to run and test

```
export CARGO_PROFILE_DEV_DEBUG=line-tables-only
cargo test -p lockdown-app --test received_vs_open
cargo test -p lockdown-web --test received_vs_open
```

- `an_emergency_release_opens_a_vault_an_item_came_from`: Alice shares one login from her vault with Bob on its own, then names Bob her emergency contact for that vault. A crown jewel in it is opted in. When the wait runs out, `emergency_open` opens the vault with every item and the crown jewel, and the shared login is listed once. Bob can't change anything (view). It stays that way after a sync. After a lock and unlock, the vault and its items are there, and the crown jewel is back at the next sync (§6). After Alice revokes, Bob's next sync closes the vault, and the login is back as received, alone.
- `a_vault_an_item_came_from_joins_at_sync_and_opens_at_unlock`: Alice shares one login from "Trip" with Bob to fill only, then adds Bob to Trip to view. Bob's next sync joins Trip, which failed before. Both items are listed once, the login now under the vault's right ("view"). A later sync works, and so does an unlock: it doesn't fail with the roster and the received item there together, and Trip opens.
- The web vault test does the same join through a tab (`web_sign_in`, `sync`, `list`, `vaults`).

Before the fix, both app tests and the web test fail on main. The emergency test fails at `emergency_open` with "vault … isn't open here". The join tests fail at the sync after Bob is added.

## 5. Unicron host-contract gaps

None hit. The slice is core-only, and no host surface changed.

## 6. Gaps

- **Released crown jewels after an unlock** come back only at the next sync. Their keys are never saved (`adopt_released_item`, doc 22), and the sync takes the release again. This was already so, and it is unchanged.
- **Removing a member or withdrawing a share breaks items shared on their own from that vault.** This was already on main and is not fixed here. `rotate_vault_key` and `rekey_item` give items fresh Item Keys without a new version, and `Vault::changed_since` doesn't see that, so the re-keyed items are never pushed. The owner then seals the *fresh* key into each item share (`after_rotation`, `unshare_item`), and the recipient gets the old body with the new key: it doesn't open, and `list()` fails as a whole. So "removed from the vault, but the item is still shared" can't be tested through `remove_member` yet. The emergency revoke covers the return to received instead. A follow-up task was suggested (D588).
- If the vault is open but the item shared on its own is no longer in it (moved or deleted there), the item isn't listed until the vault closes. The vault is the authority while it is open.

## 7. Wire and file changes

None. No IPC, wasm, view or extension schema changed. No screen changed. Two things can read differently: the access label on an item once its vault opens (D582), and the held-back line for a vault that is only received from (D586).

## 8. Proposed decisions

| # | Decision | Status |
|---|---|---|
| D580 | **One test for "open".** `Session::is_open` / `open_vaults` ask only about the persona's real vaults (its own, joined shared ones, emergency-released ones), never the holders of items shared on their own. Every "is this vault open or joined" check uses them; `vaults()` stays for reading items. Plain version: an item someone shared with you no longer makes Lockdown think you have the whole vault. | Proposed; audit item |
| D581 | **The real vault replaces the holder.** When a vault opens for real, the in-memory holder for the same vault goes, so each item is listed once, from the vault's copy, and edits go to the vault. The share stays in the sharing record; when the vault closes, the item comes back as received under its share's right. Plain version: if you get the whole vault, the shared item shows up once, inside it; if you lose the vault, you still have the item. | Proposed; audit item |
| D582 | **An item's right follows its vault while the vault is open.** An item shared to fill only, in a vault the person may view, can be shown and copied like the rest of that vault, and its row says "view". Plain version: if you can see the whole vault, you can see that item too. | Proposed; audit item; **flagged for Sean** (the access label changes) |
| D583 | **An emergency release, a join and an open at unlock over a vault an item came from** bootstrap the vault, take its key (and, for a release, the opted-in crown jewels), apply its roster and show it, as for any other vault. Plain version: emergency access and shared vaults work even if the person was already sent one of the items. | Proposed; audit item |
| D584 | **Only a real vault moves to a new Vault Key or takes a roster.** `take_new_key` and every `apply_roster` caller skip a vault that isn't open; the roster applies when it opens. Plain version: nothing tries to re-key a copy that only holds a shared item. | Proposed |
| D585 | **The vault limit and the new vault's order count real vaults only.** Plain version: an item someone shared with you doesn't use up one of your vaults. | Proposed; **flagged for Sean** (when the limit is reached) |
| D586 | **"This device has it open", in the held-back lines, means really open.** A vault the person only received an item from gets the join line, as intended in doc 49 §3, not the vault's line. Plain version: the message about a change that couldn't be checked fits whether you have that vault. | Proposed; **flagged for Sean** (which line shows) |
| D587 | **Released crown jewels after an unlock wait for the next sync, recorded.** Their keys stay unsaved; the sync takes the release again. Plain version: after unlocking, a released crown jewel appears again once Lockdown syncs. | Proposed; recorded gap |
| D588 | **Re-keyed items aren't pushed, recorded for a follow-up** (§6): after a member is removed or a share withdrawn, other people's items shared on their own from that vault don't open, and their list fails. Fix in the vault's change tracking, with its own tests. Plain version: there is a separate bug where removing someone from a vault can break items you shared from it with others; it gets its own fix. | Proposed; audit item; **flagged for Sean** (a known bug) |
| D589 | **No wire, IPC, wasm, view or extension schema change.** Plain version: older and newer apps keep understanding each other. | Accepted by default under D38 |
