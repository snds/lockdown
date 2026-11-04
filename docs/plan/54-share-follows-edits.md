# 54: An item shared on its own follows other people's edits

_Slice: `slice/share-follows-edits`, built 2026-11-04. Closes D596 (doc 51 §8): a person holding one item shared on its own from a shared vault trusts only the owner's devices for it (doc 18), so when a member edited the item, or a manager re-keyed the vault, their copy went stale. Builds on doc 18 (items shared on their own), doc 47 (members' pinned keys), doc 49 (slice 48's quiet rules: what doesn't verify is held back, never accepted), doc 50 (holders) and doc 51 (re-key versions, D590; the owner's device following a rotation it didn't make, D597). No wire, IPC, wasm, view or extension schema changes. No new screen. Proposed decisions D620–D629 (§9)._

## For future agent

- **TL;DR:** The owner's device signs other people's edits again. Each sync, after a vault's round, the owner's device looks at its items shared on their own from that vault. Any whose newest version none of the owner's devices signed (a member's edit, or a manager's re-key versions) gets a **follow version**: the same fields and files, under the item's current Item Key, signed by the owner's device, with the newest version's `edited_at`. The vault then syncs once more in the same sync, so the follow version is on the server before the sync ends. The person holding the item takes it at their next sync, as any edit of the owner's. They still trust only the owner's devices. Nothing new is shown anywhere.
- **Read with:**
  - `crates/vault/src/vault/share.rs`: `follow_edits` (and `write_rekey_version`, which it reuses).
  - `crates/app/src/session/sharing/follow.rs`: `shared_on_their_own`, `follow_edits`.
  - `crates/app/src/session.rs`: `sync_round` (the vault loop runs over an `order` list; a vault that wrote follow versions is queued once more).
  - `crates/app/src/session/sharing.rs`: `sync_received` (the devices it checks: the envelope's and the newest version's).
  - Tests: `crates/app/tests/share_follows_edits.rs`, `crates/web/tests/share_follows_edits.rs`, the `share` unit tests in `lockdown-vault`.
- **Do not:**
  - Follow anything in a vault with something held back, or whose pushes are blocked. Only versions the vault took are followed, and taking one means it verified under a writer's pinned key.
  - Hand a person holding an item a way to trust a member's device directly. Their trust stays rooted in the owner (§1).
  - Follow an item whose newest version one of this persona's own devices signed. That is what keeps two of the owner's devices from writing follow versions in turn.

## 1. The design, and why this one

Two designs were weighed (the brief's options):

| | (a) The owner's device signs again | (b) The share carries the vault's writer list |
|---|---|---|
| Who the person trusts | The owner's devices only, as now | The owner, plus every writer the owner's signed list names |
| New wire | None | A signed writer list in each item share, kept current on every roster change (D62 addition, schema and server change) |
| Roster changes | Nothing to do: the owner's device sees every change through its own roster | The list must be sealed again into every item share whenever a writer joins, leaves, changes right or adds a device |
| A manager's re-key | Followed like an edit (§3) | Still needs the owner's device to seal the new Item Key (D597) |
| History | One more version after each edit by someone else (D621) | Unchanged |
| Offline owner | The copy waits until the owner's device syncs | Follows without the owner |

This slice takes (a) (D620). It keeps the person's trust rooted in the owner and adds no screen and no wire field. It reuses the re-key version of doc 51 (D590): a follow version is a re-key version written under the item's *current* key, which the vault already writes, pushes and checks. (b) would let the person pin people they never chose to trust, through a list the server serves and every roster change must re-seal. Its one advantage, following while the owner is offline, matters less: the owner's devices sync often, and doc 51 already needs the owner's device for a re-key's new keys.

## 2. The pieces

```text
 owner's device, sync_round
 ──────────────────────────────────────────────────────────────────────────────
 sync_sharing             take_new_key (a manager's rotation) marks the vault (D597)
 shared_on_their_own      GET /v1/shares/items/sent, once, only if this persona owns
                          an open shared vault → { vault → [item] }
 vault loop (order list)  round for vault V ─┐
                                             ├─ nothing held back, pushes allowed,
                                             │  V has items shared on their own:
                                             │  Vault::follow_edits(items, own devices)
                                             │  wrote any? save V, queue V once more
                          round for V again ─┘  (pushes the follow versions and a head)
 after_rekey_push         seals the new keys of a marked vault, now all pushed (doc 51)

 person holding the item, sync_received
 ──────────────────────────────────────────────────────────────────────────────
 take_received            the share's checks as before (doc 49)
 received_opens           newest version signed by one of the owner's devices, and
                          the key opens it → take it; else keep the copy that opened
```

- **`Vault::follow_edits(items, own)`** (`lockdown-vault`). For each item named, present, a standard item, whose newest version `own` doesn't claim: writes the re-key version (`write_rekey_version`), which seals the newest fields and files again under a fresh Version Key wrapped by the current Item Key, signed by this device, with the newest version's `edited_at`. An item with nothing left to read (purged, newest version shredded, or one this vault can't open) is passed over, as for any re-key version. Commits one head when anything was written.
- **`Session::shared_on_their_own`**. The vaults it covers are this persona's own (`vault_right` is `None`), open here, not a stand-in, and shared (they have a roster): the only vaults where someone else can write. With none, it asks nothing. Otherwise one `sent_items` call lists the shares. A failed call answers nothing, and the items are followed at a later sync.
- **`Session::follow_edits`**. Calls the vault's, with this persona's own devices (`own_device_keys`) as `own`, and saves the vault when it wrote.
- **`sync_round`**. The vault loop walks an `order` list instead of a range. After a vault's round, when that round held nothing back (`one.held` is `None`), the vault's pushes aren't blocked by something else held back, the vault has items shared on their own and it wasn't followed yet in this sync, `follow_edits` runs. If it wrote, the vault's index is queued once more, and its second round pushes the follow versions. The report adds both rounds.
- **`sync_received`**. The devices it hands to the split rule (`received_take`, D423) are now the envelope's signer and the newest version's signer. Before, they were every version's. Only those two are checked here (`take_received`, `received_opens`, `Vault::received`), and an earlier version a member signed names a device that isn't the owner's. With every version counted, such an item would wait during a split, and outside one the owner would be looked up again at every sync (D624).

## 3. What the person sees, and when

| Moment | The person holding the item |
|---|---|
| A member edits and syncs; the owner's device hasn't synced | Their sync takes the share, sees the newest version isn't the owner's (`received_opens`), and keeps the copy they had. Nothing is held back and no line shows (doc 51 §2). |
| The owner's device syncs | It takes the edit (verified under the member's pinned key), writes the follow version and pushes it in the same sync. |
| The person's next sync | The new copy is the owner's: it opens and replaces the old one. List, search and quick access show the new values. |
| The owner's device is offline for a while | The copy stays as it was. It catches up at the first sync after the owner's device syncs. No line shows (D625). |
| A person who never had a copy syncs in the window | The item is left out until the follow version arrives (doc 51, D594). |

**A manager's re-key** (D626). A manager removes a member: a new Vault Key, fresh Item Keys, and a re-key version of the manager's on every item. The owner's device, at its next sync, takes the new key (`take_new_key`, which marks the vault, D597), pulls the re-key versions, writes the follow versions under the fresh Item Keys and pushes them. Only then, at the end of that sync, `after_rekey_push` seals the fresh Item Keys into the shares. So the person's next sync gets a key and a copy that match, signed by the owner. Before this slice, the key went out while the newest version was the manager's, and the person kept their old copy for good (D596).

**A forged edit.** A version the owner's vault didn't take is never followed. A head signed by someone who may not write (a view-only member, a stranger, a bent signature) is held back at the owner's device (doc 49). The owner's vault stays where it was, nothing is signed again, and nothing goes out on top. The person is served the forged copy, but its newest version isn't the owner's, so they keep theirs, quietly. When the server serves the real records again, the owner's device takes them and follows, and the person's copy follows too.

**Two devices of the owner.** If both sync after the same edit, the second one to sync sees the first one's follow version, signed by one of its own devices, and writes nothing. If they race, the second push is refused, the engine merges, and the two follow versions carry the same fields as their base. The server's history wins and no conflict copy is made (`merge_item`: `merged == their_fields`).

**History.** The owner and the members see one more version after each edit by someone else, with nothing changed and the owner's device as its editor (D621). The edit's own version keeps the member as its editor. The person holding the item can't read the member's version in history (its signer isn't one they trust), which was already so.

## 4. How to run and test

```
export CARGO_PROFILE_DEV_DEBUG=line-tables-only
cargo test -p lockdown-vault --lib share
cargo test -p lockdown-app --test share_follows_edits
cargo test -p lockdown-web --test share_follows_edits
```

- `follow_edits_signs_again_only_what_another_device_signed_last` (vault unit test): an item last signed by an owner's device is left as it is. One signed by another device gets a follow version: the same fields, files and time, this device as its editor, the old newest as its parent, seen by `changed_since`, opened by its Item Key alone. One head for the lot. Asking again writes nothing. A missing item, a purged one and a crown jewel are passed over.
- `a_members_edit_reaches_the_person_holding_the_item_once_the_owners_device_syncs`: Alice shares "Trip" with Bob to edit, and Rail with Dave on its own. Bob changes Rail's password. Dave's sync before Alice's keeps the old password, with no line. Alice's sync takes Bob's edit and adds the follow version (history: Bob's edit, then a version with nothing changed). Dave's next sync shows Bob's password, with no line, and keeps it after a lock and unlock. Later syncs on every side write nothing more, and Bob gets no conflict copy. An edit of Alice's own needs no follow.
- `a_managers_re_key_still_opens_for_the_person_holding_the_item`: Bob, a manager, removes Carol. Dave keeps opening Rail before and after Alice's sync, and after it he holds the newest copy (his history matches Alice's). Carol no longer lists it. An edit Bob makes under the new keys follows too, and survives Dave's lock and unlock.
- `a_forged_edit_by_someone_who_may_not_write_is_held_back_and_never_followed`: the server turns Bob's edit into one by Carol, who may only view, signed with her own device key, version and head. Alice's sync holds it back with one calm line. Rail stays as it was, nothing is signed again and no head goes up. Dave, served the forged copy, keeps his own with no line. When the server serves Bob's real edit again, Alice follows it, and Dave's copy follows.
- Web: `a_tab_follows_a_members_edit_and_a_tab_holding_the_item_sees_it`: Alice's syncing device and Dave's are both tabs. Bob changes Rail. Dave's tab keeps the old password until Alice's tab syncs. Alice's tab writes the follow version, and Dave's tab then reveals Bob's password. Alice's desktop takes the tab's version as its own and writes nothing more.

All four app and web tests fail without the sync step: Dave keeps the old password (the member and web tests), keeps the pre-re-key copy (the manager test), or never gets Bob's real edit after the forgery is undone.

## 5. Gaps

- **The owner's device must sync.** The person's copy follows only after one of the owner's devices has seen the edit. While none syncs, the copy stays as it was, and nothing says so (D625).
- **The CLI's own sync doesn't follow.** It doesn't run the app's `sync_round` (doc 51 §8 has the same gap for the re-seal). An owner who syncs only through the CLI follows at their next app or web sync.
- **History grows by one version per edit someone else makes** to an item the owner shared on its own (D621).
- **One more request per sync** (`sent_items`) for a persona that owns an open shared vault. Personas without one ask nothing more.
- **A member's trash, archive or restore** changes the item's state, not its newest version, so there is nothing to sign again. Items shared on their own carried the owner's state before this slice too, and that is unchanged.

## 6. Wire and file changes

None. No IPC, wasm, view, extension or server schema changed, and no screen changed. A follow version is an ordinary item version and head, checked by the server as any edit: the owner's device is a writer (`writer_key`), and the item stays under the generation in force.

## 7. Unicron host-contract gaps

None hit. The slice is core-only, and no host surface changed.

## 8. Security notes

- The owner's device signs only what its own vault took, after the vault verified it under the writer's pinned key (doc 47) and nothing in the vault was held back (doc 49). Nothing from the network is signed unverified.
- The follow version is sealed under keys the owner already holds. No secret leaves the core, and nothing is logged.
- The person's trust is unchanged: one of the owner's devices must sign the newest version, and the share's key must open it (`received_opens`, `Vault::received`).
- What changes for the person: an edit by a member the owner gave edit right now reaches them, signed by the owner's device (D629). That is the owner's choice of writers, applied to what the owner shared.

## 9. Proposed decisions

| # | Decision | Status |
|---|---|---|
| D620 | **The owner's device signs other people's edits again.** An owner's item shared on its own whose newest version none of the owner's devices signed (a member's edit, a manager's re-key) gets a follow version: the newest fields and files under the item's current Item Key, signed by the owner's device. The person holding it keeps trusting only the owner. Not chosen: a writer list carried in each share, which needs a wire field and lets the person trust devices they never chose. Plain version: when someone else changes an item you shared on its own, your device passes the change on. | Proposed; audit item |
| D621 | **The follow version shows in history and keeps the time.** Same fields and files, the newest version's edit time, the owner's device as editor. The list's order and edited time don't move; history has one more entry after each edit by someone else. Plain version: an item's history shows one extra save after someone else edits it; nothing in it changed. | Proposed; **flagged for Sean** (history shows one more entry) |
| D622 | **The follow version goes up in the same sync.** The vault syncs once more after its follow versions are written, before the sync seals any new keys. The list of items shared on their own is asked once per sync, and only by a persona that owns an open shared vault. Plain version: the person you shared with gets the change at their next sync after yours. | Proposed |
| D623 | **Only what verified is followed.** A vault with anything held back, or whose pushes are blocked, isn't followed; only versions the vault took, verified under a writer's pinned key, are signed again. A forged edit by someone who may not write is held back as before and never reaches the person. Plain version: a change Lockdown couldn't check is never passed on. | Proposed; audit item |
| D624 | **The holder checks the envelope's and the newest version's devices.** `sync_received` hands only those two to the split rule (D423) and the owner's refresh, the only ones it verifies. An earlier version a member signed no longer makes the item wait during a split or the owner be looked up again each sync. Plain version: an item someone else once edited still arrives normally. | Proposed; audit item |
| D625 | **While the owner's device is offline, the copy waits, and nothing says so.** The person keeps the last copy that opened and catches up after the owner's device syncs (doc 51, D593). Plain version: if the owner hasn't opened Lockdown since the change, you keep seeing the item as it was. | Proposed; **flagged for Sean** (nothing says a change is on its way) |
| D626 | **A manager's re-key is followed before the new keys go out.** The owner's device writes the follow versions under the fresh Item Keys, pushes them, then seals the keys into the shares (doc 51, D597). D596 is closed. Plain version: when a manager removes someone, the items you shared on their own keep working and keep updating. | Proposed; audit item |
| D627 | **No wire, IPC, wasm, view or extension schema change.** A follow version is an ordinary version and head. Plain version: older and newer apps keep understanding each other. | Accepted by default under D38 |
| D628 | **Recorded gap: the CLI's own sync doesn't follow.** An owner syncing only through the CLI follows at their next app or web sync. Plain version: the command-line tool doesn't pass on other people's changes; the app does. | Proposed; recorded gap |
| D629 | **A writer's edits reach the person holding the item.** Edits by anyone the owner gave edit (or manage) to now reach the person the owner shared the item with, signed by the owner's device. Before, they never did. Plain version: if someone you share a vault with edits an item you also shared on its own, the person you shared it with sees the edit. | Proposed; **flagged for Sean** (what the person sees changes) |
