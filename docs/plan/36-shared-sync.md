# 36: Shared vaults in step on every device

_Slice: `slice/shared-sync`, built 2026-10-29. Closes the three shared-vault sync gaps doc 35 §5 recorded (gaps 7, 3 and 1), decided under D38: an owner's new device couldn't pull a shared vault other members had written to, items shared on their own still pinned an unknown owner during a split, and a waiting member's write stopped the whole vault. Builds on D73 (revocation boundaries), D185–D191 (sharing, the membership chain, pins and `check_continues`), D246, D250 and D300 (a new device of the owner), D327 and D328 (the mirror comparison), D335–D346 (post-quantum heads), D413 and D414 (the split and its pause), D423–D426 (nobody new during a split). Proposed decisions D430–D435 (§8)._

## For future agent

- **TL;DR:** (1) A vault of this persona opens trusting this identity's devices and, apart from them, the devices of every writer its signed membership chain names, as this device pinned them through the log (`Session::vault_trust`, `lockdown_vault::identity::open_shared_vault_trusting`). A vault whose stored copy names a signer nobody vouches for yet waits unopened (`shared_sync::unknown_signers`) instead of failing the unlock or the sync; the sync's sharing step pins its writers first, then opens it (`Session::open_pulled_vaults`). Outside a split a writer whose device is still unknown is looked up again and must continue the pin; during one, the vault waits as D423 says. This also fixed the owner's own device, which couldn't unlock once a member had written to its shared vault. (2) During a split, `sync_received` takes an item shared on its own only when its owner is pinned and every device that signed it is known; the rest wait unapplied, the sync says "Items shared with you by people this device hasn't met yet wait until the server's log matches its mirrors again. Items from people you already have keep syncing.", and they apply in the sync that sees the split clear. Outside a split, a pinned owner's new device is looked up again through `check_continues` (it used to be dropped silently). (3) The stop at a waiting member's head stays: heads chain hash to hash and every head lists every item, so a known member's later head carries the waiting member's changes inside it. The vault now says "New members wait until the server's log matches its mirrors again. Changes made after a new member's or a new device's wait too, even from members you already have." and the sync says so too.
- **Read with:** `crates/app/src/session/shared_sync.rs` (the rules, the words, `vault_trust`, `open_pulled_vaults`, `received_take`, `unknown_signers`, `Hold`), `crates/app/src/session.rs` (`Session::open`'s vault loop, `sync`, `sync_with`), `crates/app/src/session/organize.rs` (`open_new_vaults`), `crates/app/src/session/sharing.rs` (`sync_received`, `refresh_writers`), `crates/app/src/session/split_hold.rs` (D423's membership walk), `crates/vault/src/identity.rs` (`open_shared_vault_trusting`), `crates/app/tests/sharing_pause.rs`.
- **Do not:** count a member's device as the identity's own (the post-quantum rule would then demand the identity's key on their heads, or worse, accept a head the identity never signed as its own); open a waiting vault on anything but pins made through the log; take a known member's head that sits on top of a waiting one (it carries the waiting member's changes); skip a waiting membership change or item and take a later one from the same signer; pin anyone from `sync_received` during a split.

## 1. An owner's new device and a shared vault others wrote to

**The bug** (doc 35 §5 gap 7). A shared vault the person owns is one of their own vaults: a new device pulls it with the rest (`pull_new_vaults`, at the first unlock after joining and in every sync), then opens it like any vault of the persona. That open trusted only this identity's devices. A head or a version signed by Bob's or Carol's device failed the load with "the vault's history didn't check out at manifest 2: signed by a device this vault doesn't know", and because the first unlock after joining opens every vault, the new device couldn't even unlock. The failing test, before the fix:

```
cargo test -p lockdown-app --test sharing_pause an_owners_new_device
  panicked at crates/app/tests/sharing_pause.rs:487:30 (open_or_create on the tablet):
  AppError { code: Refused, message: "the vault's history didn't check out at manifest 2: signed by a device this vault doesn't know" }
```

The same cause hit the owner's own laptop: after a member wrote to its shared vault, locking and unlocking it failed with the same sentence (a probe of the old code, `alice.lock(); alice.unlock()`, panicked on the unlock). Doc 35 didn't list this one; the test below covers it.

**Now.**

| Step | What happens |
|---|---|
| Unlock (`Session::open`) | Every vault of the persona is tried with this identity's devices. One that fails is checked with `unknown_signers`: if it names a head or version signer the identity doesn't know, it waits; any other refusal stops the unlock as before. At least one vault must open (the person's first vault is never shared by someone else). After the sharing record loads, `open_new_vaults` tries the waiting ones again, trusting each vault's writers as pinned; a vault whose writers this device already pinned (the owner's laptop) opens here, offline included. |
| Sync (`sync_with`) | New vaults are pulled into the store, then the sharing step runs first: each of this persona's shared vaults' membership chain is fetched, its new accounts looked up and checked against the log and pinned (`pin`, `verify_directory_entry`), exactly as any sync pins. Then `open_pulled_vaults` opens every vault of the persona not open yet, trusting its writers. |
| A signer still unknown | Outside a split: the vault's writers are looked up again and each must continue its pin (`refresh_writers`, `check_continues`), then it is opened once more; a signer still unknown then fails the sync with the load's own words, as it always did. During a split: the vault waits unopened (D423) and the sync says the vault-stop line (§3). |

**Whose devices count as whose.** `open_shared_vault_trusting` takes the members' devices apart from the identity's (`DeviceTrust`). Both verify heads and versions; only the identity's count as "own" for the post-quantum rule (doc 28), so a member's head is never held to this identity's post-quantum key and a member's device can never stand in for one of the identity's. The first version of this fix passed the members in `DeviceTrust.keys` and the open failed with "the head's post-quantum signature didn't check out" on Bob's head, which is what made the split necessary.

**During a split.** A new device has pinned nobody. If the owner shares a vault and a member writes to it while this device's view is split, the first sync after that pulls the vault into the store, takes its membership chain only up to the first event naming someone unmet (D423), and leaves the vault unopened: it isn't listed, its items don't show, nobody is pinned, and the sync says the vault-stop line. Locking and unlocking during the split works (the vault simply waits). The sync that sees the split clear runs once more (D425), pins the members through the log and opens the vault with everything in it.

## 2. Items shared on their own during a split

**Before** (doc 35 §5 gap 3). `sync_received` pinned the owner of every item shared with this person, during a split too, and an item signed by a pinned owner's device this device didn't know was dropped without a word.

**Now** (`shared_sync::received_take`). For each item the server lists, the owner and every device that signed it (the share envelope's and the item's versions') are checked:

| | Outside a split | During a split |
|---|---|---|
| Owner pinned, every device known | taken | taken ("known owners' shared items keep syncing") |
| Owner not pinned | looked up through the log, checked and pinned, then taken | waits unapplied; nobody is pinned |
| Owner pinned, a device unknown (their new device) | looked up again and must continue the pin (`check_continues`), then taken | waits unapplied |

An item that waits keeps whatever this device already held of it (normally nothing: an unpinned owner had no items here). The sync adds **"Items shared with you by people this device hasn't met yet wait until the server's log matches its mirrors again. Items from people you already have keep syncing."** (`HELD_ITEMS_LINE`), ending in D423's own clause. The waiting count lives in the session (`held_items`) and is worked out again by every sync, like D426's vault state; `Session::sync` runs its extra round when the split clears with items waiting, as it does for vaults.

## 3. A waiting member's write still stops the vault, and says so

**The question** (doc 35 §5 gap 1). Once a member who waits writes, this device takes none of the vault's later heads, including known members'. Could heads from known members be applied on top of the last trusted head, leaving only the waiting member's changes out?

**No, the chain's design makes it unsafe.** A manifest head (`crates/vault/src/manifest.rs`) carries `prev_hash`, the hash of the head before it, and `entries`, every item's newest version and its hash. When Alice writes after Carol, Alice's head chains to Carol's and lists Carol's items at the versions Carol signed. So:

- **Taking Alice's head takes Carol's changes.** Its entries pin Carol's versions; applying it while leaving Carol's items out breaks the "every referenced version exists" check (04-security §7), and applying it with them means accepting versions signed by a device this device hasn't verified, which is exactly what D423 holds back.
- **There is no head "on top of the last trusted head".** Alice's head's `prev_hash` is Carol's head; a chain from the last trusted head straight to Alice's doesn't exist, and a client can't make one (it would need Alice's signature over a different predecessor).
- **Alice's signature vouches for Carol only as far as Alice's view goes.** During a split the risk is that the server shows different keys to different people; Alice's device trusted "Carol" through the same server. Taking Alice's word for Carol's key is the trust D413 pauses.

So the stop stays, per vault (D424): the person's other vaults keep syncing. What changes is what it says:

| Where | Before | Now |
|---|---|---|
| The vault's sharing dialog (`VaultSharingView.held`) | "New members wait until the server's log matches its mirrors again." for both kinds of wait | The same sentence while only membership changes wait (`Hold::Members`; heads from known members still arrive); once a head from someone unmet stopped the vault (`Hold::Heads`): **"New members wait until the server's log matches its mirrors again. Changes made after a new member's or a new device's wait too, even from members you already have."** |
| The sync's line | "A shared vault has changes from people this device hasn't met yet. New members wait until the server's log matches its mirrors again. Items from members you already have keep syncing." for both | The same while only membership waits; once stopped at a head: **"A shared vault has changes from people this device hasn't met yet. They and every change made after them wait until the server's log matches its mirrors again, even from members you already have. Your other vaults keep syncing."** |

The old line promised "Items from members you already have keep syncing", which wasn't true once a head stopped the vault. `Session::hold` keeps the stronger reason per vault, and the sync shows the strongest one line for all vaults.

## 4. Wire and shape additions (D62)

All additive; no field renamed, removed or retyped; no route, desktop command, IPC message, wasm boundary or extension schema changed, so no schema file moved.

- **`lockdown-vault`:** `identity::open_shared_vault_trusting` (a new public function; `open_vault_trusting` calls it with no members, unchanged for every caller).
- **`lockdown-app`:** the `session::shared_sync` module (`Hold`, `HELD_AFTER`, `HELD_AFTER_LINE`, `HELD_ITEMS_LINE`, `unknown_signers`, `received_take`, `ReceivedTake`, `Waiting`, `Session::{hold, held_lines, anything_held, vault_trust, open_pulled_vaults}`); `Session.held_vaults` now records why (`BTreeMap<Id, Hold>`, was a set) and `Session.held_items` is new (both per session, never stored); `open_new_vaults` returns the vaults left waiting; `apply_roster` and `writer_keys` visible to the session's modules.
- **Words:** `VaultSharingView.held` may now carry the two-sentence `HELD_AFTER` (same field, same type); two new sync lines.
- **Design capture:** the state `desktop-sharing-vault-held-after`.

## 5. Running and testing it

```sh
export CARGO_TARGET_DIR=… CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=line-tables-only
cargo test -p lockdown-app --lib shared_sync      # the hold's order and words; received_take in and out of a split; unknown_signers over a real vault in a memory store
cargo test -p lockdown-app --test sharing_pause   # the five end-to-end tests below, against the real server and real mirrors in process
cd design/capture && pnpm build && pnpm capture --only desktop-sharing-vault-held
```

| Test (`crates/app/tests/sharing_pause.rs`) | People | What it proves |
|---|---|---|
| `an_owners_new_device_pulls_a_shared_vault_others_already_wrote_to` | Alice, her tablet, Bob, Carol | The failing test of §1: the tablet joins through the account after Bob and Carol wrote to Family, pulls all of it, has pinned both through the log, and sees them as members; it writes and Bob takes its head (Alice's new device continuing her pin); it takes Bob's next head; both of Alice's devices lock and unlock with Family open |
| `during_a_split_an_owners_new_device_holds_a_vault_whose_writers_it_hasnt_met` | Alice, her tablet, Bob | The tablet's view splits before Alice shares Trip with Bob and Bob writes: the tablet's sync succeeds, its own writes sync, Trip waits unopened with the vault-stop line, Bob isn't pinned; lock and unlock work during the split; when it clears, Bob is pinned and Trip opens with both items |
| `during_a_split_items_shared_by_someone_unmet_wait_and_known_owners_items_sync` | Alice, her tablet, Bob, Carol | During Bob's split, Alice's items from her known laptop arrive; Carol's item and the one Alice's new tablet shared wait with `HELD_ITEMS_LINE`; Carol isn't pinned; when it clears, Carol is pinned, Alice continues her pin with the tablet, and both items arrive in the same sync |
| `during_a_split_new_members_wait_and_known_members_items_still_sync` (doc 35's, extended) | Alice, her tablet, Bob, Carol | As before, plus: Alice writes again after Carol; Bob takes neither, and Family says `HELD_AFTER` and the sync `HELD_AFTER_LINE`; after the split clears all of it arrives |
| `a_split_view_pauses_each_new_share_and_the_vault_keeps_syncing` (doc 34's) | Alice, Bob | Unchanged |

The split is real in every test: one mirror copies the server's log honestly, a second is a copy of it whose pinned log key differs, so the device that checks against the second sees a split exactly as it would in the wild. No test reaches the internet.

**Design capture.** `desktop-sharing-vault-held-after`: Family's sharing dialog with the two-sentence line, zero axe violations at 1280, 900 and 390, light and dark (beside `desktop-sharing-vault-held`, re-taken unchanged).

## 6. Gaps

1. **A vault that waits at unlock shows nothing until the next sync.** A new device before its first sync, or any device during a split with a vault whose writers it hasn't met, opens without that vault; the sync's line is the first word about it. The Sharing screen has no row for a vault that isn't open.
2. **A vault shared by someone not met during a split still waits unseen** (doc 35 gap 2, unchanged), and so does an own vault pulled during a split (§1).
3. **The command line opens only the persona's first vault by id.** `bins/cli/src/app.rs` keeps its own open, which trusts only the identity's devices and opens `persona_vaults(...).first()`; ids are random, so if a shared vault others wrote to sorts first, the CLI's unlock fails as the app's did. The CLI has no sharing step to pin writers; moving it onto `lockdown-app`'s session (or opening its first non-waiting vault) is the follow-up.
4. **A vault re-keyed by a manager who isn't the owner** stays unreadable on the owner's brand-new device until `take_new_key` runs: the pulled record holds the owner's last generation, so the open is refused for the key, not a signer, and fails the sync. Not new in this slice; noted because §1 makes owners' new devices reach these vaults at all.
5. **Waiting items have no row.** The Sharing screen lists items received; one that waits isn't opened, so only the sync's line says something waits.
6. **The waiting state is per session** (D426): after a restart the line shows from the first sync.
7. **The web vault** compiles the same session (wasm), so every rule here holds there; its Playwright suite has no split scenario (doc 35 gap 9).

**unicron host-contract gaps:** none hit. The longer line reuses the sharing dialog's `role="note"` paragraph; it wraps at 390 without clipping.

## 7. Audit notes (D38)

Trusting a shared vault's writers' pinned devices when an owner's device opens it, keeping them apart from the identity's own devices for the post-quantum rule, deferring a vault with unknown signers instead of failing (and when that deferral turns back into the old failure), holding items shared on their own during a split by owner and by signing device, refreshing a pinned owner's devices through `check_continues`, and keeping the stop at a held head because heads chain hash to hash are security-model choices made under D38. They are audit items in §8, for the outside review (D377) with D423–D425.

## 8. Proposed decisions

For the coordinator to record. **Flagged** rows are user-visible and need Sean.

| ID | Decision | Owner |
|---|---|---|
| D430 | **An owner's devices open a shared vault trusting its writers, as pinned.** At unlock and after each sync's sharing step, a vault of the persona opens trusting this identity's devices and the devices of every account its signed membership chain says ever held edit, as this device pinned them through the log. A vault whose copy names a signer nobody vouches for yet waits unopened instead of failing the unlock; the sync pins its writers first (checked against the log, as every pin), then opens it; outside a split a still-unknown device is looked up again and must continue the pin, then the sync fails as before if it still can't; during a split the vault waits (D423). Closes doc 35 §5 gap 7 and the owner's own device failing to unlock once a member wrote. Plain version: your new phone can open a vault you share with family even after they've added things, and your laptop still opens it after a restart. | Accepted under D38; audit item |
| D431 | **A member's device never counts as one of yours.** The members' devices verify their heads and versions but are kept apart from the identity's own (`open_shared_vault_trusting`), so the post-quantum rule holds only your devices' heads to your key. Plain version: Lockdown never mistakes someone you share with for one of your own devices. | Accepted by default under D38; audit item |
| D432 | **Items shared on their own meet nobody new during a split.** While the last mirror check found a split, an item shared with this person waits unapplied when its owner isn't pinned or a device of the owner this device doesn't know signed it; nobody is pinned; the sync says "Items shared with you by people this device hasn't met yet wait until the server's log matches its mirrors again. Items from people you already have keep syncing."; they apply in the sync that sees the split clear. Outside a split a pinned owner's new device is looked up again and must continue the pin (it used to be dropped silently). Amends D423; closes doc 35 §5 gap 3. Plain version: while your server might be showing you a different history, something shared with you by someone new waits until that's sorted out. | **Flagged for Sean** (the words); audit item |
| D433 | **A held head still stops its vault, and says so.** Heads chain hash to hash and each lists every item's newest version, so a known member's head written after a waiting member's carries the waiting member's changes; it is not taken during the split. The vault says "New members wait until the server's log matches its mirrors again. Changes made after a new member's or a new device's wait too, even from members you already have." and the sync says "A shared vault has changes from people this device hasn't met yet. They and every change made after them wait until the server's log matches its mirrors again, even from members you already have. Your other vaults keep syncing." Keeps D424's stop; answers doc 35 §5 gap 1. Plain version: once someone new has changed a shared vault, later changes to it wait too, and Lockdown tells you that instead of suggesting they still arrive. | **Flagged for Sean** (the words); audit item |
| D434 | **The sync that sees the split clear also runs for waiting items**, as D425 does for vaults; the waiting count is per session and worked out again by every sync (D426). Plain version: items that waited appear by themselves once the warning clears. | Accepted by default under D38 |
| D435 | **Shape additions** (D62): `lockdown_vault::identity::open_shared_vault_trusting`; the app's `shared_sync` module; `Session.held_vaults` keeps why and `Session.held_items`; `open_new_vaults` returns what waits; `VaultSharingView.held` may carry the two-sentence line; two sync lines; the capture state `desktop-sharing-vault-held-after`. No wire type, route, command, IPC message, wasm boundary or extension schema changed. Plain version: everything new was added beside what was there. | Accepted by default under D38 (structure) |
