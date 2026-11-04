# 47: Shared vaults and items shared on their own, post-quantum

_Slice: `slice/shared-vault-pq`, built 2026-11-02. Closes D343 (doc 28 §8): shared vaults and items shared on their own were left Ed25519. Builds on D248 (the identity's ML-DSA-65 key, pinned by contacts on first sight), D335–D342 (post-quantum heads, the level-2 rollout, the chain rules, the server's check), D185–D199 (sharing), D430–D435 (shared vaults on every device) and D365–D374 (the status line). Proposed decisions D550–D559 (§10)._

## For future agent

- **TL;DR:** Each person in a shared vault now follows doc 28's rules for what they sign themselves, under their own key. This identity's key is derived from the root key and checked against the profile (D339). Another member's key is the one their pinned contact holds (`Contact::pq_isk`), pinned on first sight and never allowed to change or drop. Heads, membership changes, vault key grants, items shared on their own and emergency grants carry ML-DSA-65 beside Ed25519 once their maker's identity has rolled out. A verifier holds each person to their own signatures. Once one of a member's heads in the chain carries a verified signature, every later head by that member must. The same holds for each person's membership changes. Each device remembers each member's first server-confirmed post-quantum head per vault (`member_pq_since`) and refuses a stripped chain after it. Nobody waits for anybody else, and nobody is held to a signature they can't make yet. The server refuses a bad `pq_sig` on every kind and requires none.
- **Read with:**
  - `crates/vault/src/pq.rs`: `MemberPq`, `check_member_heads`, `check_events`, the contexts.
  - `crates/vault/src/vault.rs`: `set_member_post_quantum`, `member_post_quantum_since`, `signs_post_quantum`.
  - `crates/vault/src/sharing.rs`: `{MembershipEvent, ShareEnvelope, EmergencyGrant}::pq_sig` and their sign and check methods.
  - `crates/vault/src/contact.rs`: `ContactDevice.pq_level`, `Contact::{pq_key, signs_heads_post_quantum}`.
  - `crates/app/src/session/sharing/pq.rs`: the session's rules.
  - `crates/store/src/remembered.rs`: `member_pq_since`.
  - `bins/server/src/sharing_api.rs`: `check_pq_sig`.
  - `crates/web/src/host.rs`: `Carried.member_pq_since`.
- **Do not:**
  - Let a member's device stand for one of this identity's, or the reverse (D431). Each person's heads are checked only under their own key.
  - Start this identity's own post-quantum signing because another member's heads carry it. "The chain already carries one" means this identity's own heads (§3).
  - Remember `member_pq_since` from a head the server hasn't confirmed.
  - Accept a `pq_sig` you can't check as if it were checked.
  - Put `pq_sig` inside a record's signed bytes or a membership event's hash.

## 1. What was built

| Piece | What it does | Where |
|---|---|---|
| Members' keys | `ContactDevice.pq_level` (each device's `lockdown.pq` mark, from the verified record); `Contact::pq_key`, `devices_without_head_support`, `signs_heads_post_quantum` | `crates/vault/src/contact.rs`, `pq::level_of` |
| Members' heads | `MemberPq {devices, key, since}` per other account; `check_member_heads` runs doc 28's rule per member; `Vault::set_member_post_quantum` checks the chain held and keeps it for every later `rebase` and `verify`; `member_post_quantum_since(through)` | `crates/vault/src/{pq,vault}.rs` |
| Sharing records | `pq_sig` on `MembershipEvent` (context `lockdown membership event`), `ShareEnvelope` (`lockdown share envelope`) and `EmergencyGrant` (`lockdown emergency grant`), over the same bytes as Ed25519. It sits outside `signable()` and outside a membership event's `hash()`. `pq::check_events` holds a chain to each maker's own signatures. `ShareEnvelope::check_post_quantum(key, required)`, `EmergencyGrant::check_post_quantum(key)` | `crates/vault/src/{sharing,pq}.rs` |
| Memory | `HeadMemory::{member_pq_since, remember_member_pq_since}`: per vault, per member account, it only moves down. The web vault carries it between visits (`Carried.member_pq_since`) | `crates/store/src/remembered.rs`, `crates/web/src/host.rs` |
| The session | Signs what it writes with both keys when its identity has rolled out. Holds shared vaults' other writers to their rule (`apply_roster` → `hold_members_to_pq`). Checks membership chains (`replay` → `check_membership_pq`), vault key grants (`open_vault_envelope`), item shares (`sync_received`) and emergency grants (`take_release`). Remembers each member's first confirmed head after each vault's sync. Looks up again each sync anyone in a shared vault who doesn't sign post-quantum yet (`refresh_unready`) | `crates/app/src/session/sharing/pq.rs`, `sharing.rs`, `session.rs` |
| Own heads in someone else's vault | A vault another person owns now opens with this identity's own post-quantum setting and memory, as its own vaults do. Before, it got them only at the first sync | `Session::open_shared_vault` |
| The server | `check_pq_sig`: a membership change's, a share envelope's (vault, item, emergency) or an emergency grant's `pq_sig`, when present, must verify under the key the sending account's profile publishes. A record that carries one from an account that publishes none is refused. Heads already had this check (D341), and a member's head is checked under that member's own account | `bins/server/src/sharing_api.rs` |
| A fix on the way | A membership chain that fails its check is no longer kept in the session's memory. Before, the next sync fetched nothing new and saw the refused change as taken | `Session::pull_membership` |

## 2. Whose keys

- **This identity:** derived from the root key; the profile must publish exactly that key or the vault doesn't open (D339, unchanged).
- **Another member:** the key their contact pinned (`Contact.pq_isk`), which comes from their profile through the key-transparency log, as their Ed25519 Identity Signing Key does. `check_continues` already refuses a later lookup that drops or changes a pinned post-quantum key (D248). Their devices are their contact's devices, and never this identity's own.
- **A member whose profile publishes no key yet:** nothing is pinned. Their heads, changes and grants carry nothing this device can check; a `pq_sig` from them counts for nothing (it neither starts their requirement nor meets it). The vault opens and syncs as before. Each sync looks them up again through the log (`refresh_unready`), so their key is pinned at the first sync after they publish it. During a split nobody is looked up (D423), and the lookup waits.
- **A key that differs from the pin:** the lookup is refused (`check_continues`) and the pin stays. `refresh_unready` treats that like a failed lookup: nothing new is trusted, and the sync goes on with what was pinned. Their post-quantum heads then fail under the old key, and the vault is refused as for any bad signature. The person forgets the contact and compares fingerprints again, as for a changed Ed25519 key.

## 3. The rollout across a shared vault: per person

**The rule.** A device signs a head, a membership change or a grant post-quantum when its own identity has rolled out: the key is published and every device of the identity not revoked marks level 2 (D337). It also signs when the chain it extends already carries a verified post-quantum signature by its own identity. Another member's signatures don't count toward that.

**Why not wait for every member.** The brief proposed one gate for the whole vault: sign once every member publishes a key and every member device marks level 2, and from the first post-quantum head on, require it of every head. That can't hold with what doc 28 already built. An owner's vault carries post-quantum heads from before it was shared (D338). Under a vault-wide rule, a member who joins without a key would have every head refused. Alternatively the owner would have to stop signing, which D338 forbids. Gating by members also isn't needed:

- A `pq_sig` sits outside every signed byte and every hash, so an older build ignores it. One person signing doesn't break anyone else's reading.
- What a stripped or forged signature lets an attacker do is per signer. Holding each person to their own signatures closes exactly the hole that person's rollout can close.

So the smallest sound design is doc 28 per person:

- The vault is "post-quantum for Bob" once Bob's side rolled out.
- It is post-quantum throughout once everyone's has.
- One member who never updates holds back only their own heads.

The two-identity test shows the vault getting there only when both sides roll out (§6).

**A fix to doc 28.** `Vault::signs_post_quantum` used to start signing when any head in the chain carried a `pq_sig`. In a shared vault that could be another member's head. It then started this identity's signing while one of its own devices still couldn't sign, and that device's next head would have been refused. It now counts only this identity's own heads (D556).

## 4. Chain rules for shared vaults

For each head, the signer decides whose rule applies:

| Signer | Key | Rule |
|---|---|---|
| One of this identity's devices | derived (D339) | doc 28 unchanged: every `pq_sig` must verify; once one carries it every later own head must; from `pq_since` on every own head must |
| A device of member M | M's pinned key | the same, for M's heads alone: every `pq_sig` must verify under M's key; once one of M's heads carries a verified one, every later head by M must; from `member_pq_since[M]` on every head by M must |
| Anyone else | none | not held (Ed25519 still checks it, and an unknown signer still stops the vault as before) |

- **Where it runs.** On open (`apply_roster` at unlock), on every pull (`rebase`), on `verify`, and whenever the members' keys or memory change (`set_member_post_quantum`, re-checked only when they did).
- **The memory.** After each vault's sync, each member's first verified post-quantum head at or below the `seq` the server confirmed is remembered (`member_post_quantum_since`), never from a provisional head. A device that remembers one for M but knows no key for M now refuses the vault.
- **History holds.** A member's heads from before their rollout stay fine. The requirement starts at their own first signed head, not at anyone else's.
- **Membership changes.** The device keeps every event it took, and that stored chain is its memory. `check_events` runs on every replay: each event's `pq_sig` must verify under its maker's key (this identity's own published key, or the pinned contact's). Once one of an account's events carries a verified signature, every later event by that account must. A first-sight device (a new member's first pull) is open to a fully stripped chain, as doc 28 §7 says for heads.
- **Vault key grants.** A grant is the envelope that carries a Vault Key generation to a member. It is checked when its sender's key is known, and required when the membership change that handed it over carries one. That change is the recipient's latest addition under that generation, or the removal that made the generation, made by the same sender. Both are signed by the same device in the same push, so they carry a signature together or not at all.

## 5. Items shared on their own, and emergency grants

An item shared on its own has no chain: one envelope per share, which a re-key replaces. Nothing records that an earlier envelope from the same owner carried a signature. The emergency grant is in the same position. So both are **signed with both keys and checked when present, never required**:

- **Covered:** a bent or forged ML-DSA signature on an item share, an emergency grant or an emergency envelope is refused. A bad item share doesn't arrive, and a bad emergency release fails. The share's content is unchanged: the item's versions are Ed25519, and the item key is under X-Wing to the recipient. A forged envelope can only carry a key the attacker made, so it can't open the owner's item. The real risk is a fake item "from Alice" wrapped to the recipient's public key.
- **Not covered:** an attacker who can break Ed25519 can strip the ML-DSA signature and forge the envelope's Ed25519 one. A recipient then takes a forged item share, or a forged grant, as genuine. Closing it would need a per-owner memory ("Alice's shares carry it from now on") on every recipient. That is left for later (§8).
- **Emergency steps** (accept, decline, request, stop, revoke) stay Ed25519 only. The server orders them, and what they release is keys sealed to the contact's hybrid receiving key. A forged step can at worst start or stop a wait the owner sees.

## 6. Tests

| Test | What it covers |
|---|---|
| `vault::tests::each_members_heads_follow_their_own_post_quantum_rule_under_their_pinned_key` | Alice signing while Bob can't; Bob's own chain starting his signing (and Alice's not); his memory; his head stripped, bent, under another key, under Alice's key; all his signatures stripped with and without memory; a memory naming an unsigned head; a remembered member with no key; his pre-rollout heads staying fine |
| `sharing::tests::membership_changes_follow_each_makers_post_quantum_rule_and_older_builds_ignore_it` | Per-maker chain rule, an older build's replay (same hash, same signed bytes), bent, swapped key, stripped, unknown key |
| `sharing::tests::envelopes_and_grants_carry_the_senders_post_quantum_signature` | Envelopes and grants: Ed25519 unchanged, required and not, bent, another key |
| `remembered::tests::a_members_first_post_quantum_head_is_remembered_per_vault_and_never_forgotten` | The memory |
| `bins/server/tests/api.rs::a_shared_vaults_heads_membership_changes_and_grants_must_carry_a_good_post_quantum_signature_if_any` | The server on each kind: a membership change (no key published, bent, another person's key, good), a vault key grant, a member's head (bent, the owner's key on it, good, then none at all), an item share's envelope, an emergency grant and its envelope |
| `crates/app/tests/shared_vault_pq.rs::a_shared_vault_goes_post_quantum_side_by_side_and_a_server_that_bends_or_strips_it_is_refused` | Two identities against the real server. Bob has an older device and publishes no key, so his heads are Ed25519 and nothing breaks. Alice's side signs from the start. Bob's device updates, his heads carry both, and Alice pins his key at her next sync and remembers his first one. A server that bends, forges or strips his head is refused, and so is one that bends or strips a membership change before Bob pulls it. A stripped or bent vault key grant stops Carol's join until it is put back. A bent item share doesn't arrive |
| `crates/web/tests/shared_vault_pq.rs::a_tab_opens_a_post_quantum_shared_vault_and_carries_its_members_rule` | The web vault (same core) opens a post-quantum shared vault from a tab, writes to it with both signatures, remembers the owner's first post-quantum head and carries it to a second visit, which opens the vault again |
| `crates/vault/tests/shared_pq_cost.rs` | §7 |

No screen changed, so there is no Vitest or Playwright change (§9).

## 7. What it costs

Measured on this machine (`cargo test -p lockdown-vault --test shared_pq_cost -- --nocapture`), on the default dev build. In that build ML-DSA, SHAKE and Keccak are optimised (D344) and the encoding is not. The head lists 10,000 items.

| | |
|---|---|
| A membership event | +3,319 B (505 B before) |
| A share envelope (vault key, item, emergency) | +3,319 B (2,017 B before) |
| An emergency grant | +3,319 B (548 B before) |
| A member's head | +3,319 B, as an own head (doc 28 §6) |
| Checking a 10,000-item shared vault's chain, 3 own heads | 209 ms |
| The other member's 3 heads too | +230 ms (77 ms a head: encoding plus one ML-DSA check, the same work as an own head) |
| 50 post-quantum membership events checked | 170 ms (3.4 ms an event) |
| The server, per pushed record that carries one | one ML-DSA check (about 0.5 ms near-release, doc 28 §6) |
| A sync with a member who doesn't sign post-quantum yet | one directory lookup and inclusion proof per such member, until they do |

**In words:** a member's heads cost exactly what this identity's own heads cost (doc 28 §6), so a shared vault costs as much as a personal one with the same number of post-quantum heads. Each sharing record grows by one signature, about 3.3 KB. It is paid once per change or share, not per item.

## 8. Gaps

- **Item shares and emergency grants aren't required to carry the signature** (§5). Closing it needs a per-owner memory on every recipient. _Closed by doc 49 §5 (a per-sender memory on each device)._
- **A first-sight device** (a new member, a fresh install) is open to a chain stripped of every signature by a person it never saw sign, for heads and membership changes alike (doc 28 §7, unchanged in kind).
- **The command line holds shared vaults' other members to nothing post-quantum.** `bins/cli` opens shared vaults with their pinned writers (`vault_open::pinned_writers`) but has no sharing step, so it never sets `MemberPq`. Its own heads follow doc 28 as before. Moving the CLI onto the session's sharing step (doc 36 gap 3) closes it.
- **Each pull checks every post-quantum head again.** It does so for members as for this identity (doc 28 §9's gap, now per member). Checking only from the newest post-quantum head per person back would cut it.
- **Looking members up every sync** while they don't sign post-quantum costs one lookup each (§7). A member who never updates costs that forever. A back-off isn't built.
- **A member's refused lookup is quiet:** `refresh_unready` keeps the old pin and says nothing. The changed key shows in the contact's next deliberate lookup.
- **Emergency steps** stay Ed25519 (§5).
- **Status line:** dropped by Sean (D558, 2026-11-04); nothing shows other members' rollout (§9).

**unicron host-contract gaps:** none hit; no screen changed.

## 9. The status line for other members: dropped

Dropped by Sean on 2026-11-04 (D558: "drop D558"). Whether another member has updated isn't something the person can act on, so nothing anywhere shows it, and the Security line speaks only for this identity's own devices (D365). Nothing of it was built; the words this section once proposed are withdrawn.

## 10. Wire and file additions (D62, additive)

None of these is renamed, removed or retyped. Every one is absent when empty, and a build that doesn't know it drops it on decode and still verifies everything it verified before.

- **Sync wire (CBOR):** `MembershipEvent.pq_sig`, `ShareEnvelope.pq_sig`, `EmergencyGrant.pq_sig` (all outside the Ed25519-signed bytes; the first also outside the event hash that chains events).
- **Files:**
  - `Contact.devices[].pq_level` (the sharing record),
  - `MemoryFile.member_pq_since` (`remembered.cbor`),
  - the web vault's carried `Carried.member_pq_since` (opaque bytes to the page).
- **API:**
  - `lockdown_vault::pq::{MemberPq, check_member_heads, check_events, level_of, MEMBERSHIP_CONTEXT, SHARE_CONTEXT, EMERGENCY_CONTEXT}`,
  - `Vault::{set_member_post_quantum, member_post_quantum, member_post_quantum_since}`,
  - `{MembershipEvent, ShareEnvelope, EmergencyGrant}::sign_post_quantum`, `MembershipEvent::verify_post_quantum`, `ShareEnvelope::{verify_post_quantum, check_post_quantum}`, `EmergencyGrant::check_post_quantum`,
  - `Contact::{pq_key, devices_without_head_support, signs_heads_post_quantum}`,
  - `HeadMemory::{member_pq_since, remember_member_pq_since}`.
- **No change** to the wasm boundary, the extension's schemas, the desktop's IPC, the web command table, any route or any view, so no schema file moved.

## 11. How to run and test

```sh
export CARGO_TARGET_DIR=… CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=line-tables-only
cargo test -p lockdown-vault --lib pq sharing vault::tests           # the rules
cargo test -p lockdown-vault --test shared_pq_cost -- --nocapture    # §7
cargo test -p lockdown-store --lib remembered                        # the memory
cargo test -p lockdown-server --test api a_shared_vaults             # the server on each kind
cargo test -p lockdown-app --test shared_vault_pq                    # two identities, a hostile server
cargo test -p lockdown-web --test shared_vault_pq                    # the web vault
```

To try it by hand: register two people on two desktops, share a vault, and write from both. Each head the server holds carries `pq_sig` once its maker's side has rolled out (`sqlite3 lockdown-server.sqlite "SELECT seq, length(body) FROM heads"`). So does each membership change (`… FROM membership_events`).

## 12. Proposed decisions

For the coordinator to record. Every row is a security-model choice, so every row is an audit item (D38). **Flagged** rows are the ones Sean should see.

| ID | Decision | Status |
|---|---|---|
| D550 | **Shared vaults go post-quantum per person.** Each identity signs what it writes in a shared vault (heads, membership changes, grants) with ML-DSA-65 beside Ed25519 once its own rollout is done (D337), or once the chain carries a verified signature of its own. No member waits for another. Each person's records are held to their own signatures under their own key. This replaces the brief's vault-wide gate, which would refuse every head of a member without a key in a vault already signed post-quantum (D338). Closes D343. Plain version: everyone's changes to a shared vault get the stronger signature as soon as their own devices are ready, and someone who hasn't updated yet doesn't hold anyone else back or get locked out. | Proposed; audit item; **flagged for the coordinator** (differs from the brief's rule, §3) |
| D551 | **A member's post-quantum key is their pinned contact's** (`Contact.pq_isk`, first sight through the log, never dropped or changed, D248). This identity's own stays derived and checked against the profile (D339). A member who publishes no key has nothing checked and nothing required, and is looked up again at each sync outside a split until they publish. A changed key is refused and the old pin stays. Plain version: Lockdown checks each person's stronger signature against the key it first saw for them. | Proposed; audit item |
| D552 | **Each member's first confirmed post-quantum head is remembered per shared vault** (`member_pq_since`, from heads the server confirmed, never provisional ones). From there on every head by that member must carry one, and a device that remembers one but knows no key for them refuses the vault. The web vault carries it between visits. Plain version: once a person's changes to a shared vault are signed the new way, a server can't quietly take the new signatures off. | Proposed; audit item |
| D553 | **Membership changes and vault key grants carry ML-DSA-65** (contexts `lockdown membership event`, `lockdown share envelope`), outside the Ed25519-signed bytes and outside the event hash. A chain holds each maker to their own signatures. A grant must carry one when the change that handed it over does. Plain version: who's in a shared vault, and the keys handed to them, are signed the new way too. | Proposed; audit item |
| D554 | **Items shared on their own and emergency grants are signed with both keys, checked when the signature is there, and not required.** No chain records that an owner's earlier shares carried one. A bent one is refused (the item doesn't arrive), but a stripped one can't be told from one made before. Emergency steps stay Ed25519. Plain version: things shared one at a time get the stronger signature, but for now Lockdown can't insist on it. | Proposed; audit item |
| D555 | **The server checks every sharing record's post-quantum signature** (membership change, share envelope of each purpose, emergency grant) against the sending account's published key, as it does heads (D341). It refuses a bad one, or one from an account that publishes none, and requires none. A member's pushed head is checked under the member's own account. Plain version: the server turns away a shared vault change whose new signature is wrong. | Proposed; audit item |
| D556 | **"The chain already carries one" counts only this identity's own heads** when a device decides to start signing (amends D337's second clause). Another member's post-quantum head no longer starts this identity's signing while one of its own devices can't sign. Plain version: your devices start the stronger signatures when your devices are ready, not because someone you share with is. | Proposed; audit item |
| D557 | **A membership chain that fails its check isn't kept**, not even in the session's memory. The next pull fetches the same changes and checks them again. Before, a refused change stayed in memory, so the next sync saw nothing new and went on. Plain version: a shared vault change Lockdown refused stays refused. | Accepted by default under D38 (a bug fix); audit item |
| D558 | **Status line words for a shared vault waiting on someone else** (proposed, not built): after "Quantum-safe signatures: on", add "In Family, changes Bob makes are still signed the older way until Bob's devices update." It names the vault and the person, never their devices. Building it adds a field to `PqStatusView` and its schema, the desktop and web views, `lockdown security`, and a Playwright spec. Plain version: Security could say which shared vault is still waiting, and on whom. | Proposed; **flagged for Sean** (new words on a screen) · **dropped by Sean 2026-11-04** (§9) |
| D559 | **Wire and file additions (D62, additive):** §10's fields. No wasm, IPC, route, view or extension schema changed. Plain version: older and newer apps keep understanding each other. | Accepted by default under D38 |
