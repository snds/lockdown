# 49: Shared vaults hold back what doesn't verify, quietly

_Slice: `slice/shared-vault-quiet`, built 2026-11-04. Sean's rule for it: "From a user's perspective, they should only have to log in with their master password and the vault unlocks. The idea for anything here is ease of use, and having the user handle anything like this manually is a non-starter." Builds on doc 47 (D550–D559: shared vaults post-quantum per person), doc 36 (D430–D435: shared vaults on every device, the split's holds), doc 28 (post-quantum heads), D62 (wire shapes: add fields only) and D248. Closes doc 47's D554 gap, and the unlock of a person whose only vault is shared and others wrote to it. D558 is dropped (Sean, 2026-11-04): nothing shows other members' rollout, and doc 47 §9 now says so. Proposed decisions D570–D579 (§10)._

## For future agent

- **TL;DR:** A record another person made that fails its check no longer stops the sync or the vault. It is **held back**: never taken, shown, opened or acted on. The vault opens at the newest state that checks out, and one calm line names the person ("Some recent changes from bob@example.com couldn't be verified, so Lockdown left them out. You don't need to do anything."). Every sync fetches the same records again; once they verify they are taken and the line goes by itself. While anything of a shared vault is held back, this device sends nothing to it (its own changes wait, as offline ones do) and who is in it can't be changed here. This identity's own records are still refused, as before. Items shared on their own and emergency grants that fail keep what this device had, with their own line. Once a sender's item share or emergency grant carries a verified ML-DSA signature, this device holds that sender to one from the next sync on (D554 closed, per device). A persona whose only vault is a shared one other people wrote to now unlocks: through the writers this device pinned, or, on a device that has met nobody yet, on an empty stand-in until the first sync opens the vault.
- **Read with:**
  - `crates/vault/src/vault.rs`: `Vault::checked_prefix`, `check_rebase`, `failing_head`.
  - `crates/sync/src/engine.rs`: `sync_vault_holding`, `Holding`, `HeldHeads`, `servable_prefix`.
  - `crates/app/src/session/quiet.rs`: the state (`Unverified`, `Held`, `Kind`), every line, the stand-in, `held_back_details`.
  - `crates/app/src/session.rs`: `Session::open_with` (step 3 before giving up, the stand-in), `sync_with` / `sync_round`, the vault loop's `Holding`.
  - `crates/app/src/session/sharing.rs`: `replay_held_back`, `vault_key_or_hold`, `KeyFor`, `take_received`, `check_grant`, `see_grants`, `take_release`, the refusals in `share_vault`, `set_member_right`, `remove_member`.
  - `crates/app/src/sharing.rs`: `ReceivedItem.envelope_hash`, `pq_senders`, `grants_seen`.
  - `crates/app/src/vault_open.rs`: `open_with_pins`, `waiting_keys`.
  - `bins/cli/src/app.rs`: the same open, `NOTHING_VOUCHES`, the sync's held-back lines.
  - Tests: `crates/app/tests/shared_vault_quiet.rs`, `crates/app/tests/shared_vault_pq.rs`, `crates/web/tests/shared_vault_quiet.rs`, `bins/cli/tests/shared_vault.rs`, `apps/web/e2e/shared-quiet.spec.ts`.
- **Do not:**
  - Take, show, decrypt or act on a record that didn't verify. A held record's own fields only ever name who it claims to be from, and only a person this device pinned is named.
  - Push anything to a shared vault while something of it is held back. Any head this device wrote would follow the one that failed.
  - Hold back this identity's own records. They are refused, as doc 28 says.
  - Mark a sender as signing post-quantum from a record this device made, or from one it didn't verify.
  - Save, sync, list or write to the stand-in. It holds nothing and gives way to the first vault that opens.

## 1. What "held back" means

| Record | What is taken | What is held back | Where the vault stands |
|---|---|---|---|
| A shared vault's heads | The longest run of the pulled heads, from the first, that checks out (`Vault::checked_prefix`), as far as the server still serves the items the newest of them lists (`servable_prefix`) | The first head that fails and every head after it: each names the one before by hash, so nothing after it can be taken without it | At that head, or where it was; this device's own changes stay on top under a head that is never sent |
| A rollback or a fork (the server shows less than this device took, or another chain) | Nothing | The heads the server dropped, when the newest one this device holds is another person's | Where it was |
| Membership changes | The longest run of the pulled changes that replays and checks out (`replay_held_back`) | The first change that fails and every later one | At the last good roster |
| A vault key grant | Nothing | The grant | Not joined yet, or at the key generation it had |
| An item shared on its own | Nothing new | The share | The copy this device had (none if it had none) |
| An emergency grant or a key it releases | Nothing | The release | What an earlier release opened stays |

"Fails its check" is any refusal of the existing checks: a bad Ed25519 or ML-DSA signature, a stripped signature after the maker's first verified one (or after `member_pq_since`), a key that differs from the pin, a chain rule (sequence, `prev_hash`, key generation), a revoked device's head past its boundary, a rollback or a fork. A head signed by a device nobody vouches for still goes the doc 36 way (looked up again outside a split, waits during one): it isn't held back here, because looking the writer up is how it resolves.

**Whose records.** A head is another person's when its signer is a device of a pinned contact (`device_table` minus this identity's). A membership change or a grant names its maker's account. Records by this identity, or by nobody this device knows, fail as before, except that a membership change or grant by someone unpinned is held back without a name ("Some recent changes couldn't be verified …").

**Why the heads before the failing one, and only as far as the server serves.** The server keeps only each item's newest copy. A head that is checked can still be taken only when every item it lists is either what this device took at `synced_seq` or the server's current copy at exactly that revision and version. When a later, held-back head changed an item again, its earlier copy is gone, so the heads that listed it wait too. While anything is held back, `synced_rev` stays where it was, so the next pull fetches every item those heads changed again.

**Choice for the person's own vaults: refused, as today.** A failure in a vault only this identity writes is its own chain under attack, and doc 28 makes that a refusal that names the check. Holding it back quietly would hide the one case where the person's own devices disagree with the server. In a shared vault, a head by one of this identity's own devices is refused the same way (the two-device test in §8).

## 2. When the vault can't be written to, and how it clears

**This device sends nothing to a shared vault while anything of it is held back.** For heads that is forced: any head it pushed would have to follow the one that failed. For a membership change or a grant it is the safe reading: the change might be a removal, and nothing written here should reach the server under a key it may have replaced. The person's own changes still save: they wait on this device under a provisional head, exactly as offline changes do (`Holding::push`, the engine commits locally and returns). When the vault's records verify, the normal sync takes the chain, takes the membership and key changes (and re-keys items under a new generation), merges and pushes. Nothing is lost; the two-identity test locks and unlocks in the middle to prove it.

**Who is in the vault can't be changed on this device meanwhile.** Sharing it, changing a right or removing someone each post a membership change that would have to follow the one held back (the server orders the chain), so each is refused with the read-only words, and the sharing panel hides its controls (`VaultSharingView.can_manage` false, its `held` line set). This is the "read-only" of the brief, narrowed to the one thing that can't be done safely: item edits stay open, because they are safe and keep the person's flow (D572, flagged).

**It clears by itself.** The state is per session (`Session.unverified`, like the split's holds, D426) and every sync works it out again from what it fetches; a sync that fails keeps what the last one held back. Once the server serves the good records, they are taken and the line is gone from the vault, the sync's lines and Status. After a restart the vault opens at the last good state it stored (nothing unverified was ever stored), and the line comes back from the first sync if the server still serves the bad record.

## 3. What the person sees

All new words are flagged for Sean (D573). None of them names a device or a key; each ends in "You don't need to do anything."; none comes with a button, a modal or a dialog.

| Where | Words |
|---|---|
| The vault (desktop and web), a `role="note"` line under the search row when the vault is open | **Some recent changes from bob@example.com couldn't be verified, so Lockdown left them out. You don't need to do anything.** (several people: "from bob@example.com and carol@example.com"; nobody this device knows: "Some recent changes couldn't be verified, …") |
| The vault's sharing panel, for someone who could manage it, and the refusal of a membership change (the read-only variant) | **Some recent changes from bob@example.com couldn't be verified, so Lockdown left them out. Until they can be, people can't be added or removed here. You don't need to do anything.** |
| The sync's lines (desktop's Sync now notice, the web vault's sync, `lockdown sync`) | **In Family: Some recent changes from bob@example.com couldn't be verified, …** (the vault's name in front, so a line among several vaults says which) |
| A shared vault this device couldn't join yet (its key grant held back) | **A shared vault from alice@example.com couldn't be verified, so Lockdown left it out for now. You don't need to do anything.** |
| Items shared on their own | **A shared item from eve@example.com couldn't be verified, so Lockdown left it out. You don't need to do anything.** (several: "Some shared items from … left them out.") |
| Emergency access (the sync's line, and the Open step's refusal) | **Emergency access from alice@example.com couldn't be verified, so Lockdown left it out for now. You don't need to do anything.** |
| A shared only vault before the first sync (§6) | **This shared vault opens by itself once Lockdown has checked who changed it, at the next sync. You don't need to do anything.** (the vault's name shows as "Shared vault" until it opens: its name is sealed inside it) |
| The command line when nothing vouches for the only vault (§6) | **Your vault is shared, and this device hasn't checked the people who changed it yet, so it can't open here. Open the Lockdown app and let it sync once; the command line opens it after that.** |

The names are login names, as everywhere else in sharing (`name_of`); the brief's "Bob" is `bob@example.com` on screen.

**The technical detail, for support,** is on Status (desktop and web), in the "Account and sync" section, under **Held back until they verify** (a list, in the mono style of the audit's lines): one line per record, `Family: head manifest 7 by bob@example.com held back: the vault's history didn't check out at manifest 7: the device signature didn't check out`. It comes from `Status.held_back` (D62, absent when empty). The command line's sync prints the same after the line: `held back: manifest 7 and every one after it (…)`. The design system's ratchet stays at 3 overrides (baseline 4); no new component.

## 4. The pieces

| Piece | What it does | Where |
|---|---|---|
| How far a chain checks out | `Vault::checked_prefix(keep_through, remote)`: how many pulled heads `rebase` would take and the first that failed, with why. `rebase` now checks everything first (`check_rebase`, the members list sealed in the newest head included) and changes nothing on a refusal | `crates/vault/src/vault.rs` |
| The holding sync | `sync_vault_holding(…, Holding { holdable, push })`: takes the servable checked prefix, holds back a failing head whose signer `holdable` accepts (and a rollback or fork whose newest held head it accepts), pushes nothing while held or when `push` is false, commits this device's changes under a head that is never sent. `SyncReport.held` says what. `sync_vault_with` is it with `Holding::NONE`, unchanged for every caller | `crates/sync/src/engine.rs` |
| The session's state and words | `quiet::Unverified` (per session), the lines, `held_back_details` | `crates/app/src/session/quiet.rs` |
| Heads | The vault loop syncs each shared vault holding back other people's devices' heads, and with `push` false while a membership change or grant of it is held | `Session::sync_round` |
| Membership changes | `replay_held_back`: keeps the longest run of new changes that replays, holds back the rest; a join or a new key waits behind it | `Session::pull_membership`, `sync_sharing` |
| Vault key grants | `vault_key_or_hold` at the join and at a new generation; `open_vault_envelope` takes `KeyFor { purpose, grant, require_pq }` | `crates/app/src/session/sharing.rs` |
| Refusals while held | `refuse_while_unverified` in `share_vault`, `set_member_right`, `remove_member`; the panel's `can_manage` and `held` | the same |
| Items shared on their own | `take_received`: every check of the old loop, now refused with words; a refused one keeps this device's copy. A rollback (an older version than taken) is held back too, instead of failing the sync | the same |
| Emergency grants | `check_grant`, `see_grants`, `take_release` | the same |
| Views | `VaultView.unverified`, `Status.held_back`, `Status.waiting` (D62) | `crates/app/src/types.rs` |
| Desktop and web | The line on the vault (`vault-view.tsx`), the detail on Status (`status-view.tsx`), one sync at once while `status.waiting` (`App.tsx`, `web-app.tsx`) | `apps/desktop/src`, `apps/web/src` |
| Command line | The same open as the app (§6); its sync holds back a pinned writer's head and prints the app's line (`lockdown_app::held_back_line`) | `bins/cli/src/app.rs` |

## 5. D554 closed: items shared on their own and emergency grants

**The memory.** When an item share or an emergency grant this device fetched from the server carries an ML-DSA signature that verifies under the sender's pinned key, the sender's account goes into the persona's sharing record (`sharing-<persona>.cbor`, `pq_senders`, with when): **per device, not synced**, beside the pins it belongs with. It is never forgotten, not even with the contact. It applies **from the next sync on**, so a share the sender made before their update, seen in the same first sync, still arrives. Records this device made never mark anyone.

**The rule.** From then on, a share or grant from that sender without a valid signature is held back with the quiet line, unless it is the very record this device took before the mark:

- **Item shares:** each `ReceivedItem` keeps the BLAKE3 hash of the envelope it was taken from (`envelope_hash`). The same envelope keeps opening after the mark; a different one (a stripped copy, a forged one) must carry the signature. A re-key after a rotation makes a new envelope, which the sender signs, being rolled out.
- **Emergency grants** live for years before a release, so a grant made before the owner's update must still release. Each sync now lists the grants naming this persona (one more request per sync) and notes each one the first time it is seen while its owner isn't marked (`grants_seen`, its hash; dropped once it is no longer listed). At release, an unsigned grant from a marked owner releases only if it is that noted grant. The keys a grant releases must carry the owner's signature when the grant does (`KeyFor.require_pq`), as a vault key grant must when its membership change does (D553).
- **Emergency steps** stay Ed25519, as doc 47 §5 says.

**The first-sight gap stays, recorded (D576).** A device that has never seen a sender sign (a new install, a new contact) can't tell a stripped share or grant from an old one. Covering it from the sender's own signed profile (say `lockdown.pq_shares`, set once every outstanding share and grant carries the signature, pinned so it can't be dropped) isn't small or purely additive: the sender's device would first have to re-sign every outstanding item share and emergency grant and envelope (the server has no path to replace a grant today), and the flag would need its own "never drops" rule in `check_continues`. It is left as a gap, with that design.

## 6. A person whose only vault is shared

**The bug.** `Session::open` tries every vault of the persona with this identity's devices; a shared vault other people wrote to waits, and doc 36's step 3 opens it trusting its writers' pinned devices. But the sharing record that holds those pins opens with the App Data Key of a vault that opened, so when **no** vault opened, the unlock returned the load's refusal before step 3 could run. A person who shared their only vault, after a member wrote to it, couldn't unlock; nor could their new device.

**Now.**

1. When every vault waits, step 3 runs first: the persona's keys come from the identity directly (`vault_open::waiting_keys`: `keys_for` with the ids the waiting vault's record names), the sharing record opens with them, and each waiting vault is tried again trusting its pinned writers (`vault_open::open_with_pins`, which the command line shares). The first that opens is the session's first vault, and the unlock goes on as always.
2. When nothing vouches yet (a device that has met nobody: a new device before its first sync), **the unlock still succeeds**, on a **stand-in** (`quiet::stand_in`): an empty vault in memory under the persona's own keys and a fresh id, holding nothing. It is never saved, synced, listed, exported or written to (`require_edit` refuses it with the waiting words; `save`, `remember`, the sync loop and the audit's expectations skip it). The waiting vaults show in the vault list instead, each with the waiting line, and `Status.waiting` is set. The desktop and the web vault sync once at once when they see it; that sync pins the writers through the log as any sync does, opens the vault, and the vault takes the stand-in's place (`open_new_vaults`). No vault is created in the waiting one's place, and nothing unverified is accepted.
3. **The command line** does step 3 the same way. It has no sharing step to meet anyone, so when nothing vouches it says so in words (§3) and exits, instead of the raw refusal.

Zero open vaults on screen: the sidebar lists the waiting vault ("Shared vault", with its line when it is the one in view), "All items" is empty, and adding an item says the waiting line. The vault's options menu is still drawn for it (§9).

## 7. Wire and file additions (D62, additive)

None renamed, removed or retyped; each is absent when empty.

- **Views (JSON to the desktop, the web vault and the CLI's tests):** `VaultView.unverified`, `Status.held_back`, `Status.waiting`. No schema file covers these views, so none moved. No wasm boundary, IPC message, route or extension schema changed.
- **Files:** `ReceivedItem.envelope_hash`, `Body.pq_senders`, `Body.grants_seen` in `sharing-<persona>.cbor` (an older build drops them on its next write, and with them the D554 memory: §9).
- **API:** `lockdown_vault::Vault::checked_prefix`; `lockdown_sync::{sync_vault_holding, Holding, HeldHeads}` and `SyncReport.held`; `lockdown_app::held_back_line`; `lockdown_app::vault_open::{open_with_pins, waiting_keys}`; `SharingState::{pq_senders, mark_pq_sender, grants_seen, see_grant, keep_grants}`.
- **Developer tool:** `lockdown-devtools`' `shared-fixture` feature and `lockdown-shared-fixture` binary (never in a release), built in the web vault's CI job; `apps/web/e2e/serve.mjs` leaves a note of its data directory for the spec.

## 8. How to run and test

```sh
export CARGO_TARGET_DIR=… CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=line-tables-only
cargo test -p lockdown-vault --lib each_members_heads        # checked_prefix
cargo test -p lockdown-app --lib quiet                       # the words, the state
cargo test -p lockdown-app --test shared_vault_quiet         # two identities, a hostile server
cargo test -p lockdown-app --test shared_vault_pq            # doc 47's, now holding back
cargo test -p lockdown-cli --test shared_vault               # the command line
cargo test -p lockdown-web --test shared_vault_quiet         # the web vault
cargo build -p lockdown-devtools --no-default-features --features shared-fixture --bin lockdown-shared-fixture
cd apps/web && pnpm build:wasm && pnpm build && pnpm e2e e2e/shared-quiet.spec.ts
cd apps/desktop && pnpm test                                 # the line and the Status detail
```

| Test | What it proves |
|---|---|
| `shared_vault_quiet::a_shared_vault_holds_back_what_doesnt_verify_keeps_its_own_changes_and_clears_by_itself` | Bob writes twice; the server bends his newer head's device signature. Alice's sync takes his first head and not the second, keeps her own unsynced note, shows the line on the vault and in the sync ("In Family: …"), sends nothing (the server's head count doesn't move), has the detail on Status, refuses to add Carol with the read-only words and hides the panel's controls; lock and unlock keep it all; a second sync is held back again. Once the server serves the good head, it is taken, the line clears, and Alice's head goes out chained on Bob's good one; Bob gets her note. A rolled-back head of Bob's is held back and clears once shown again. A membership change Bob made, bent, is held back (Carol isn't in), Alice's new note waits unsent, and both go through once it is put back. Alice's tablet still refuses a bent head of her own laptop's, as before |
| `…::a_share_or_grant_from_someone_who_signed_before_must_carry_it_and_older_ones_still_arrive` | Eve, who never signed, shares an item: it arrives. Eve updates and shares again, signed; from the next sync Dan holds her to it, and her older unsigned share still opens. Her signed share, stripped by the server: held back with the item line, Dan's copy stays, it clears once put back. Alice's emergency grant, stripped after Dan saw her shares signed: the release opens nothing and says the emergency line; put back, it releases |
| `…::a_person_whose_only_vault_is_shared_unlocks_and_a_new_device_opens_it_after_a_sync` | Alice's only vault is shared and Bob wrote last: her unlock opens it through his pins, offline. Her new tablet, which has met nobody, unlocks all the same, lists the vault with the waiting line, holds nothing and refuses a new item with the waiting words; its sync opens the vault in place, and it writes, locks, unlocks, and Bob gets its note |
| `shared_vault_pq::a_shared_vault_goes_post_quantum_side_by_side_and_what_a_server_bends_or_strips_is_held_back` | Doc 47's test with its refusals turned into holds: a bent, forged or stripped post-quantum signature on Bob's head, a bent or stripped membership change, a stripped or bent vault key grant (the join line) and a bent item share (the item line); each clears once put back |
| `bins/cli/tests/shared_vault.rs::the_cli_opens_a_persona_whose_only_vault_is_shared_and_holds_back_what_doesnt_verify` | The command line opens the only vault through the app's pins; without them it says the sentence of §3; its sync holds back Bob's bent head with the app's line and takes it once put back |
| `crates/web/tests/shared_vault_quiet.rs` | The web vault (same core): a fresh tab of Alice's shows the waiting line, its sync opens the vault; a bent head of Bob's is held back with the line and the Status detail, and clears |
| `apps/web/e2e/shared-quiet.spec.ts` | In Chromium against the real server: the tab's first sync opens the shared only vault; Bob's change arrives by the live channel; the server rolls that head back and the vault shows the line (no button, no dialog), keeps what it had and the tab's new note; put back, the line goes |
| `vault-view.test.tsx`, `status-view.test.tsx` (Vitest) | The line on that vault only, as a note with nothing to click; the Status list and its absence |

## 9. Gaps

- **The first sight of a sender** (§5, D576): a device that never saw a sender sign takes a stripped share or grant from them.
- **An older build drops the D554 memory** when it next writes the sharing record (`pq_senders`, `grants_seen`, `envelope_hash` are unknown to it). The pins themselves survive; the memory starts again at the next signed share.
- **A changed key never clears by itself.** A member whose post-quantum or device key no longer matches the pin (a real reset, not an attack) has every later head held back forever; the line stays, and only the existing "forget the contact and compare fingerprints" resolves it (doc 47 §2). Sean's rule says that is a non-starter; resolving a key change without the person needs a decision of its own.
- **The line appears only after a sync.** The state is per session; at unlock the vault opens quietly at its last good state.
- **The heads after the failing one wait whole**, even when they are another member's and change nothing the held head touched (they chain on it; doc 36 §3).
- **Only the newest item copies are served**, so a checked prefix can be shorter than it could be (§1).
- **One more request per sync** (the emergency list, §5) for every persona, released grants or not.
- **The stand-in's vault menu:** while no vault opened, the sidebar still draws the waiting vault's options menu; its actions refuse (the vault isn't open). The Status screen's vault line names the stand-in's id meanwhile.
- **The command line** still has no sharing step: it holds a pinned writer's head back but can't meet anyone, check members' post-quantum rules or pull membership (doc 47 §8).
- **An emergency release over a vault an item was shared from on its own** fails with "vault … isn't open here" (`take_release` treats the received in-memory vault as open). Found while building this; not caused by it; the test shares its item from another vault. Proposed as its own task.

**unicron host-contract gaps:** none hit. The line reuses the `role="note"` paragraph style the sharing dialog already has (`text-sm text-muted-foreground`); the Status list reuses the audit lines' mono style.

## 10. Proposed decisions

For the coordinator to record. Every row but D578 is a security-model or user-visible choice, so they are audit items (D38). **Flagged** rows are the ones Sean should see.

| ID | Decision | Status |
|---|---|---|
| D570 | **Hold back, don't refuse, what other people make in a shared vault.** A head, membership change or vault key grant another person made that fails its check (bad Ed25519 or ML-DSA signature, a stripped one after their first, a key unlike the pin, a chain rule, a revoked device past its boundary, a rollback or fork of their heads) is held back instead of stopping the sync: never taken, shown, opened or acted on, fetched again and checked afresh every sync, taken once it verifies. This identity's own records keep today's refusal, in its own vaults and in shared ones. Amends doc 47 §3 ("the vault is refused as for any bad signature"). Plain version: if something someone else changed in a shared vault can't be checked, Lockdown leaves it out and carries on, and takes it in once it checks out. | Proposed; audit item |
| D571 | **The newest state that checks out.** Heads: the longest run of pulled heads that checks out and whose items the server still serves; the failing head and every later one wait (they chain on it), and `synced_rev` stays so their items are fetched again. Membership: the longest run of changes that replays; the failing one and every later one wait, and a join or a new key waits behind them. A vault key grant that fails leaves the vault unjoined or at its key. Plain version: the vault shows everything up to the first change that can't be checked. | Proposed; audit item |
| D572 | **While anything of a shared vault is held back, this device sends nothing to it and who is in it can't change here.** The person's own changes save and wait on this device under a head that is never sent, as offline changes do, and go out once the vault's records verify (after any re-key). Sharing the vault, changing a right or removing someone is refused with the read-only words, and the panel hides those controls. Narrower than the brief's read-only vault: item edits stay open because they are safe. Plain version: you can keep using the vault; your changes wait on this device until the rest checks out, and adding or removing people waits too. | Proposed; audit item; **flagged for Sean** (what can't be done meanwhile) |
| D573 | **The words** (§3): the vault line, its read-only variant, the sync's "In <vault>: …", the join line, the item line, the emergency line. Each names the person by login, never a device or a key, ends "You don't need to do anything.", and comes with nothing to click. Plain version: one calm sentence on the vault says some changes were left out, and by whom. | Proposed; **flagged for Sean** (new words on a screen) |
| D574 | **The technical detail is on Status, for support**: "Held back until they verify", one line per record with which record and which check, and the same after the command line's sync line. Plain version: the details for someone helping you are on the Status screen, not in the message. | Proposed; **flagged for Sean** (a new label on Status) |
| D575 | **D554 closed, per device.** A sender whose item share or emergency grant this device fetched and verified with an ML-DSA signature under their pinned key is held to one from the next sync on (`pq_senders` in the sharing record, not synced, never forgotten). A share or grant without one is then held back, unless it is the very envelope or grant this device took or first saw before. Each sync lists the emergency grants naming this persona to note them at first sight. A released key must carry the owner's signature when its grant does. Emergency steps stay Ed25519. Plain version: once someone's shares to you are signed the stronger way, a server can't quietly take that signature off. | Proposed; audit item |
| D576 | **The first-sight gap for shares and grants stays, recorded** (§5): covering it from the sender's signed profile needs the sender to re-sign every outstanding share and grant first and a pin that can't drop, which isn't small or additive. Plain version: a brand-new device can't yet tell an old unsigned share from a stripped one. | Proposed; audit item |
| D577 | **A persona whose only vault is shared unlocks.** When every vault waits, the doc 36 step 3 runs first with the persona's keys from the identity, trusting each vault's pinned writers. When nothing vouches yet, the session opens on an empty stand-in that is never saved, synced, listed or written to; the waiting vault is listed with the waiting line, `Status.waiting` makes the desktop and the web vault sync once at once, and that sync opens it in the stand-in's place. No vault is made in its place; nothing unverified is accepted. The command line does the same and, when nothing vouches, says so in words. Plain version: you can always unlock, even when your only vault is shared; on a new device it opens by itself after the first sync. | Proposed; audit item; **flagged for Sean** (the waiting line and the command line's sentence) |
| D578 | **Wire and file additions (D62, additive)**: §7's fields and API, the developer fixture tool. No wasm, IPC, route, view schema or extension schema changed. Plain version: older and newer apps keep understanding each other. | Accepted by default under D38 |
| D579 | **The command line's sync holds back a pinned writer's head too**, with the app's line and a "held back:" detail line, for the vault it opens. It still has no sharing step (doc 47 §8). Plain version: the command line says the same thing the app does when it leaves a change out. | Proposed; audit item |
