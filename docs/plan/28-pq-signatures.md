# 28: Post-quantum signatures

_Slice: `slice/pq-signatures`, built 2026-10-18 under D122 ("desktop and web first"). Row: pq-key-wrapping (the signature part: device signatures, vault heads, the key-transparency log's tree heads). Builds on D248 (ML-DSA-65 on identity manifests) and D318 (the rollout rule), and follows the plan in doc 26 §10. Proposed decisions D335–D346 (§13)._

## For future agent

- **TL;DR:** Every vault head now carries an ML-DSA-65 signature beside the device's Ed25519 one, by the identity's post-quantum key (the one D248 derives from the root key and publishes on the profile). The log's signed tree heads carry one too, by a new ML-DSA-65 log key in the server's key file, which clients pin on first sight. Checkpoints that `lockdown checkpoint` exports carry one once every device reads them. Item versions carry none of their own: the head pins each item's latest version by its BLAKE3 hash, and each version pins the one before, so the head's signature covers them. Hybrid means both: a verifier with the post-quantum key checks both signatures and refuses either failing. ML-DSA alone is never accepted, and Ed25519 is never dropped.
- **Older clients keep working.** The new fields sit outside the Ed25519-signed bytes and outside the hashes that chain heads together, so a build that drops them on decode still verifies everything it verified before. A device signs heads post-quantum only once every device not revoked marks level 2 on its own `Device` record (D318's mark, one level up), or once the chain it extends already carries one.
- **Read with:** `crates/vault/src/pq.rs` (the rules), `manifest.rs` (`ManifestHead::{sign,verify}_post_quantum`), `vault.rs` (`set_post_quantum`, `require_post_quantum_from`, `post_quantum_since`, `signs_post_quantum`), `transparency.rs` (`SignedTreeHead::{sign,verify}_hybrid`, `Checkpoint::{sign_post_quantum,verify_hybrid}`), `crates/sync/src/kt.rs` (`KtSession::begin`, the pin), `crates/store/src/{remembered,transparency}.rs`, `bins/server/src/{keys,api}.rs`.
- **Do not:** accept a `pq_sig` you can't check as if it were checked; put `pq_sig` inside `signable()` or `hash()` (an older build would then break its chain); sign heads post-quantum before the rollout rule says so; remember `pq_since` from a head the server hasn't confirmed (a provisional head would then poison a later, legitimate one).

## 1. What was built

| Piece | What it does | Where |
|---|---|---|
| `ManifestHead.pq_sig` | ML-DSA-65 over `signable()` (the head without `sig`, `checkpoint`, `pq_sig`), context `lockdown vault head`, by the identity's post-quantum key | `crates/vault/src/manifest.rs` |
| The head rules | Sign when ready; check every `pq_sig` by the identity's devices; once one head carries it, every later one must; from the remembered `seq` on, every one must | `crates/vault/src/pq.rs` (`HeadPq`, `head_pq`, `heads_ready`, `check_heads`) |
| The vault | Holds the setting, signs at commit, checks on open, on `rebase` (every pull) and on `verify` | `crates/vault/src/vault.rs`, `identity.rs` (`vault_pq`, wired into `open_vault_trusting` and `open_or_create_vault`) |
| Support levels | Level 1 reads manifests' `pq_sig` (D318); level 2 also heads, tree heads and checkpoints. This build marks 2 | `pq::{MANIFEST_LEVEL, HEAD_LEVEL, supports_level, devices_without_head_support}` |
| The memory | `HeadMemory::pq_since(vault)`: the first post-quantum head the server confirmed. It only moves down, never away | `crates/store/src/remembered.rs`; the app's and the CLI's sync record it; the web vault carries it between visits |
| Tree heads | `SignedTreeHead.pq_sig` by the server's ML-DSA-65 log key, context `lockdown kt tree head`; `SthWire.pq_log_key` | `crates/vault/src/transparency.rs`, `bins/server/src/{keys,api}.rs` |
| The pin | The client pins the post-quantum log key the first time the server offers one; after that a tree head without its signature, a dropped key or a changed key is refused | `crates/sync/src/kt.rs`, `ServerView.pq_log_key` |
| Checkpoints | `Checkpoint.pq_log_key` (under both signatures) and `pq_sig` by the identity's key, context `lockdown kt checkpoint`, only once every device reads them | `transparency.rs`, `bins/cli/src/app.rs` |
| The server's check | A pushed head that carries a `pq_sig` must verify under the key the account's profile publishes; the server doesn't require one | `bins/server/src/api.rs` (`put_head`, `account_pq_key`) |
| The CLI | Runs the rollout and signs identity manifests with both keys, as the app does. Before, a CLI device never marked itself and kept signing manifests with Ed25519 alone after the key was published | `bins/cli/src/app.rs` |

## 2. Vault heads

**Whose key.** The identity's ML-DSA-65 key (`pq::identity_pq_key`, HKDF from the root key under `lockdown/pq-isk/ml-dsa-65/v1`), not a key per device. Every device of the identity holds the root key (that is how it opens anything), and revocation does not rotate the root key. So a per-device key derived from the root key would be no stronger, and a per-device random key would need a new sealed file, a publication on every `Device` record and its own rollout. The device is still named by the Ed25519 signature and the `signer` field, and revocation (D73) still works through them.

**Where it sits.** `pq_sig` is outside the bytes both signatures cover and outside `hash()`, as doc 26 §10 planned. A build from before this slice decodes the field away. Its hash of the head is the same, so its chain check passes and its `prev_hash` matches. That is what lets old and new devices share one chain during the rollout.

**When a device signs.** `Vault::signs_post_quantum`: it holds the signing half (the profile publishes the key and this device's derivation matches it), and either:

- every device not revoked marks level 2 (`pq::heads_ready`), or
- the chain already carries a post-quantum head. Another device saw the rollout finish first, and every head after that one must carry the signature.

The setting comes from `identity::vault_pq` when a vault opens. After every identity sync it is refreshed (`Session::refresh_trust`), so a device starts signing in the same sync that tells it the last device has caught up.

**When a device checks.** `pq::check_heads`, on open, on every pull (`rebase`) and on `verify`. For each head by one of the identity's devices:

1. A `pq_sig` must verify under the published key.
2. A head without one is refused if any earlier head in the chain carries one, or if its `seq` is at or after the `seq` this device remembers (`pq_since`).

Heads by other people's devices (a shared vault's members) are not covered (§8). A `pq_sig` that can't be checked, because this device knows no key yet, counts for nothing. It doesn't satisfy the rules, and it isn't trusted. A device that remembers a `pq_since` but now sees no published key refuses the vault outright.

**The memory.** After a sync, `pq_since` takes the first post-quantum head at or below the `seq` the server confirmed (`post_quantum_since(head_seq)`). It is never taken from a provisional, offline head: that head may be replaced by another device's head at the same `seq`, made before that device was ready. The value only ever moves to a lower `seq`. Without it, a server could strip every signature from the whole chain and a device would see a chain that simply predates the rollout. With it, that server is refused.

**A changed profile key.** When a device derives the identity's key and it isn't the key the profile publishes, the vault doesn't open (`head_pq`). Someone replaced the profile's key under this device. Only a holder of the root key can make the published key, so this check is what stops a quantum attacker who has forged the Ed25519 profile signature from slipping their own key in (§7).

## 3. Item versions ("device signatures")

Each item version is still signed by the editing device with Ed25519, and nothing else. It gets no ML-DSA signature of its own, and that is deliberate:

- The head lists every item with the BLAKE3 hash of its latest version (`ManifestEntry.version_hash`). That hash covers the version's Ed25519 signature, its fields, its attachments' blob hashes and its `parent_hash`. `parent_hash` is the hash of the version before it, so the latest version pins the whole history.
- `check_items_against_head` refuses any item whose latest hash isn't what the newest head lists. Sync ignores a pulled item no head lists yet.
- So forging an item version means forging the head that lists it, which now needs the ML-DSA key. A quantum attacker can forge the version's Ed25519 signature, but every reader that loads the item checks it against the head.

What a signature per version would cost is in §6: 3,309 bytes per version, 158% of a typical login's envelope, 33 MB for 10,000 single-version items. A head's signature costs 0.3% of that head.

The other signatures a device's key makes stay Ed25519, each for its own reason:

- **The device's sign-in proof** (`device_pop_message`) is answered live. Forging it needs the quantum computer during the sign-in, not afterwards (§7).
- **Checkpoints** gain the identity's signature (§5).
- **Sharing records** (the roster chain, grants, item shares) are §8.

## 4. The log's tree heads

The server's key file gains `pq_log_seed` (FIPS 204 `ξ`). A file from before gains it once, on the next start, and is rewritten, as `log_seed` was. `current_sth` signs with both keys. One ML-DSA signature per request over 61 bytes takes about 1.2 ms (§6). `GET /v1/kt/sth` adds `pq_log_key`.

`KtSession::begin` decides as follows:

| This device pinned | The server offers | Result |
|---|---|---|
| nothing | no post-quantum key | Ed25519 alone, as before |
| nothing | a post-quantum key | pin it; the tree head must carry a valid ML-DSA signature now |
| a key | the same key | both signatures must verify |
| a key | another key | refused: "changed since this device pinned it" |
| a key | no key | refused: "dropped the post-quantum log key" |

`TransparencyMemory::remember` refuses to drop or change a pinned post-quantum key as well, so a caller that forgets the check still can't. The web vault carries the pinned key between visits (`LogPin.pq_log_key`, D181).

## 5. Checkpoints

`lockdown checkpoint` adds three things once `pq::heads_ready`:

- the tree head's ML-DSA signature,
- the pinned post-quantum log key (`pq_log_key`, under both signatures),
- the identity's ML-DSA signature (`pq_sig`).

Before then it strips them all, so an older build that reads the file re-encodes exactly what was signed. `--verify` does three things:

- checks the tree head with both log keys,
- refuses a post-quantum log key other than the one pinned here,
- runs `Checkpoint::verify_hybrid`: a checkpoint with any post-quantum part must carry the identity's signature and pass it.

## 6. Sizes and timings

Measured on this machine (Intel Xeon @ 2.10 GHz, 4 cores) by `crates/vault/tests/pq_cost.rs` (`cargo test -p lockdown-vault --test pq_cost -- --nocapture`). The head lists 10,000 items. Times are means of 20 runs.

**Sizes** (asserted by the test):

| | Bytes |
|---|---|
| ML-DSA-65 signature | 3,309 (Ed25519: 64) |
| ML-DSA-65 public key (profile, log key) | 1,952 |
| A head's `pq_sig` field, encoded | +3,319 per head |
| A 10,000-item head | 1,120,189; with `pq_sig` 1,123,508 (+0.30%) |
| One sample login item envelope | 2,099 (a signature per version would add 158%) |
| 10,000 items, a signature per version instead | +33.09 MB, and again for every later version |
| One tree head | +3,319 per `GET /v1/kt/sth` answer, plus 1,952 for the key |

**Timings.** The second column is a near-release build: `--config profile.dev.package.<p>.opt-level=3` for the crypto and vault crates, the curve, CBOR and serde. The default dev build is in brackets: ML-DSA, SHAKE and Keccak are optimised there (D344) and the rest is not.

| | Near-release | (dev build) |
|---|---|---|
| ML-DSA-65 sign, one-entry head | 1.17 ms | (30 ms) |
| ML-DSA-65 verify, one-entry head | 0.46 ms | (1.9 ms) |
| Ed25519 sign / verify, one-entry head | 0.05 / 0.06 ms | (0.22 / 10.6 ms) |
| ML-DSA-65 sign, 10,000-entry head | 10.8 ms | (16.1 ms) |
| ML-DSA-65 verify, 10,000-entry head | 7.5 ms | (10.7 ms) |
| Ed25519 sign / verify, 10,000-entry head | 5.0 / 2.6 ms | (5.6 / 12.3 ms) |
| Canonical encoding of that head (both schemes pay it) | 37 ms | (55 ms) |
| Check one head: Ed25519 chain step / ML-DSA step with its encoding | 34 / 45 ms | (62 / 67 ms) |

With ML-DSA, SHAKE and Keccak unoptimised (main before this slice), one ML-DSA signature over the 10,000-entry head took 422 ms and one check took 439 ms.

**What this costs a 10,000-item vault:**

- **Storage and sync.** +3.3 KB for every head, about 0.3% of a head that size. Items add nothing.
- **A commit.** About +11 ms of signing, beside the 37 ms of encoding every commit already pays.
- **An open or a pull.** About 45 ms more per post-quantum head checked, on top of today's 34 ms per head. Most of that is encoding the head a second time, which `check_heads` does separately from `verify_chain` (§9).
- **The server.** About +1.2 ms of signing per tree-head request, and one ML-DSA check per pushed head that carries one.

## 7. What an attacker with a quantum computer can still do

The attacker below can break Ed25519, X25519 and the ristretto255 group (OPAQUE) at will, and controls the server. They cannot break ML-DSA-65, ML-KEM-768, BLAKE3 or 256-bit symmetric keys.

- **Every Ed25519-only signature is theirs to forge.** That covers:
  - a head's device signature (they still need the ML-DSA one),
  - every item version's signature (they still need the head),
  - the Identity Signing Key's records (the profile, devices, personas, envelopes; they still need the manifest's ML-DSA signature once the key is published),
  - the server's session tokens (the token key is Ed25519, so they can act as any account's session),
  - every device's sign-in proof: with the account password, they can sign in as a known device and skip the security-key second factor (D250, D300), live,
  - the matcher's signed site data and the activation-page list (D309).
- **Before the rollout finishes, nothing in this slice applies.** Heads stay Ed25519 until every device marks level 2. One device that never updates holds the whole account back (`pq::devices_without_head_support` names it; no screen shows it yet, §9).
- **A device with no memory of a vault can be shown a chain with every post-quantum signature stripped**, which looks like a chain from before the rollout. Examples: a fresh install, a tab's first visit, or a device that was offline since before the first post-quantum head. Such a device would accept a forged chain: a rolled-back or hidden item, a trash or restore that never happened. It still can't read or plant item contents without the keys, which are hybrid-wrapped or symmetric. The identity's other devices remember `pq_since`, and their key-transparency audit refuses a vault head at a `seq` that differs from theirs, so a split view is caught once one of them syncs.
- **A brand-new contact, or one who first looks after the attack,** can be shown a forged profile with the attacker's ML-DSA key (D248's first-sight pin). The identity's own devices refuse such a profile, because they derive the key themselves (`head_pq`). A contact who pinned the key earlier refuses it too.
- **A revoked device that kept its root key** can still make the identity's ML-DSA signature. The root key doesn't rotate on revocation. With a quantum computer, it could forge heads in a device's name past its revocation boundary (D73), since that boundary is enforced through the Ed25519 `signer` alone.
- **Shared vaults and items shared on their own** (§8) stay Ed25519: the roster chain, grants, member heads and single-item shares. The attacker can roll back, reorder or hide what another person's device wrote there. They can't read it or forge its contents.
- **TOFU on the post-quantum log key.** The client pins the first ML-DSA key the server offers. A server that is the attacker picks that key, as it picks its Ed25519 one. Equivocation is still caught by consistency proofs and gossip, which rest on hashes.
- **TLS.** The handshake's key exchange is hybrid (X25519MLKEM768, D215), but certificates are classical. So a live attacker can impersonate the server on the wire, as the hostile server already may. Recorded traffic stays sealed.
- **The password file.** With the server's database, they can guess passwords offline, as a classical attacker with the database can. The Secret Key (128 random bits, D11) still stands in the way. Accounts that opted out of it (D247) have only the password.
- **Passkeys Lockdown holds are ES256** (the site chose the algorithm). A quantum attacker who knows a passkey's public key, which every site holds, can forge its assertions. That is the web's problem, not this protocol's, but people should know.

What it can no longer do, once the rollout finishes:

- Forge a vault head for any of the identity's vaults (and so roll back, hide or reorder items) on a device that has seen the vault before.
- Forge a tree head from a server whose post-quantum log key a device pinned.
- Forge a checkpoint.

## 8. Shared vaults and items shared on their own

These stay Ed25519, deferred as doc 26 §10 proposed. Their heads, roster events and grants are signed by other people's devices. Checking them post-quantum needs each member's ML-DSA key from their pinned contact (`Contact.pq_isk` exists), plus a rollout across everyone in the vault. `check_heads` skips heads whose signer isn't one of this identity's devices. A head this identity's devices sign in a shared vault still carries `pq_sig`, and their own devices check it. Other members never see the field: they open shared vaults through `Vault::load` without a post-quantum setting.

## 9. Gaps

- **No screen shows the rollout.** `devices_without_head_support` names the devices holding heads back, but Settings and Security don't show it. A line in Security ("Post-quantum signatures: on" or "waiting for …") is proposed (D346), not built.
- **Opening re-encodes each post-quantum head.** `check_heads` verifies every `pq_sig` in the chain, apart from `verify_chain`'s Ed25519 pass, so each head is encoded twice (§6). Only the newest needs it, since the hash chain pins the rest. Folding both checks into one pass, or checking only from the newest post-quantum head back, would halve the cost for long chains of big vaults.
- **A first-sight device is open to a stripped chain** (§7). Closing it needs a per-vault marker the identity signs ("post-quantum since seq N"), written once when each vault's first post-quantum head is confirmed. That is an identity-record write per vault, left for later.
- **A restored server key file from before this slice** gains a new post-quantum log key on its next start. Clients that pinned the old one then refuse the server, as they would after a lost Ed25519 log key. Back up the key file after upgrading (`docs/self-host.md` already says to keep it with the database).
- **The identity crate's own records** (profile, devices) are still signed by the ISK with Ed25519 only. The manifest's ML-DSA signature covers them by hash. ML-DSA on each record would be an identity-crate change.
- **Secret pages.** The ML-DSA signing key (an expanded key of about 4 KB on the heap, zeroized on drop by `ml-dsa`) isn't under D314's `mlock`. The server's post-quantum log key isn't either.
- **No browser e2e.** The web vault runs the same core, so the rules hold there. The carried pin and memory are covered by the Rust web tests' existing visits, but no Playwright spec looks at a post-quantum head.

**unicron host-contract gaps:** none. This slice adds no UI.

## 10. Wire and file additions (all additive, D62)

None of these is renamed, removed or retyped. Every one is absent when empty, and a build that doesn't know it ignores it.

- **Sync wire (CBOR):**
  - `ManifestHead.pq_sig`,
  - `SignedTreeHead.pq_sig`,
  - `SthWire.pq_log_key`.
- **Files:**
  - `Checkpoint.{pq_log_key, pq_sig}` (the `lockdown checkpoint` page),
  - `ServerView.pq_log_key` (`transparency.cbor`),
  - `MemoryFile.pq_since` (`remembered.cbor`),
  - the web vault's carried `Carried.pq_since` and `LogPin.pq_log_key` (opaque bytes to the page),
  - the server key file's `pq_log_seed`,
  - the `Device` record's `lockdown.pq` mark, which can now be 2.
- **API:**
  - `lockdown_vault::pq::{HeadPq, head_pq, heads_ready, check_heads, supports_level, devices_without_head_support, published_key_in, MANIFEST_LEVEL, HEAD_LEVEL}`,
  - `Vault::{set_post_quantum, post_quantum, require_post_quantum_from, post_quantum_since, signs_post_quantum}`,
  - `ManifestHead::{sign,verify}_post_quantum`,
  - `SignedTreeHead::{sign,verify}_hybrid`,
  - `Checkpoint::{sign_post_quantum, verify_hybrid}`,
  - `Signer::identity_pq_key` (a default method),
  - `identity::vault_pq`,
  - `HeadMemory::{pq_since, remember_pq_since}`,
  - `ServerKeys.pq_log_key`, `AppState.pq_log_key`.
- **No change** to the wasm boundary, the extension's JSON schemas (`extension/schema/`), the desktop's IPC or the web commands.
- **No new dependency:** `ml-dsa` 0.1.1 was already in the tree (D259). `Cargo.toml` adds `opt-level = 3` in dev builds for `ml-dsa`, `module-lattice`, `shake` and `keccak`, as for `sha2`.

## 11. How to run and test

```sh
export CARGO_TARGET_DIR=… CARGO_INCREMENTAL=0
cargo test -p lockdown-vault --lib pq vault::tests transparency   # rules, heads, tree heads, checkpoints
cargo test -p lockdown-vault --test pq_cost -- --nocapture       # §6's sizes (asserted) and timings (printed)
cargo test -p lockdown-store --test pq_heads                      # a rolled-out identity, a real store, tampering
cargo test -p lockdown-store --lib                                # pq_since memory, the pinned log key
cargo test -p lockdown-sync --lib kt                              # the pin, each tree-head signature bent
cargo test -p lockdown-server --lib keys                          # key file upgrades
cargo test -p lockdown-server --test api a_heads_post_quantum     # the server's check on push, the hybrid STH
cargo test -p lockdown-app --test pq_signatures                   # two devices, a hostile server
cargo test -p lockdown-cli --test transparency                    # checkpoints with post-quantum parts
```

To try it by hand, run `lockdown-server`, register on the desktop, add an item and sync. The head the server holds then carries `pq_sig`: read it with `sqlite3 lockdown-server.sqlite "SELECT length(body) FROM heads"`. It is 3,319 bytes longer than a head from before.

## 12. Tests added

Rust: 10 new tests and 3 extended. Wherever a signature is checked, a test tampers with each one on its own.

| Test | What it covers |
|---|---|
| `vault::tests::heads_carry_ml_dsa_beside_ed25519_once_every_device_reads_them` | The rollout; an older build's view of the same chain |
| `vault::tests::each_head_signature_is_checked_on_its_own_and_stripping_is_refused` | Ed25519 bent, ML-DSA bent, another key, one head stripped, every head stripped with and without memory, a refused rebase changes nothing |
| `pq::tests::the_key_is_published_only_once_every_device_says_it_keeps_the_signature` (extended) | Level 1 against level 2; a derivation that isn't the published key |
| `transparency::tests::a_hybrid_tree_head_needs_both_signatures_once_the_post_quantum_key_is_pinned` | Each tree-head signature on its own |
| `transparency::tests::a_post_quantum_checkpoint_needs_both_signatures` | Each checkpoint signature on its own |
| `kt::tests::a_post_quantum_log_key_is_pinned_once_and_each_signature_is_checked` | Five ways a server bends a tree head |
| `remembered::tests::the_first_post_quantum_head_is_remembered_and_never_forgotten` | The `pq_since` memory |
| `transparency` memory test (extended) | A pinned post-quantum log key isn't dropped or swapped |
| `keys::tests` (extended) | Old key files gain the post-quantum log key and keep it |
| `api::a_heads_post_quantum_signature_must_check_out_under_the_accounts_published_key` | The server's check on push |
| `pq_heads::a_rolled_out_identity_signs_heads_with_both_keys_and_checks_both_on_open` | A real identity and a real store |
| `pq_signatures::both_devices_sign_heads_post_quantum_and_a_server_that_bends_or_strips_one_is_refused` | Two devices against a hostile server |
| `pq_cost` | §6 |

No desktop, web or extension tests: no view changed.

## 13. Proposed decisions

For the coordinator to record. Every row is a security-model choice, so every row is an audit item (D38). **Flagged** rows are the ones Sean should see.

| ID | Decision | Status |
|---|---|---|
| D335 | **Vault heads carry ML-DSA-65 beside Ed25519.** `ManifestHead.pq_sig`, by the identity's post-quantum key (D248), context `lockdown vault head`, over the same bytes as the device's Ed25519 signature. It sits outside both signatures and outside the head's hash, so an older build drops it without breaking its chain. A verifier holding the key checks both; ML-DSA alone is never enough. Plain version: every change to your vault is now signed in a way a quantum computer can't forge, as well as the old way. | Proposed; audit item |
| D336 | **Item versions carry no post-quantum signature of their own.** The head pins each item's latest version by BLAKE3, and each version pins the one before, so the head's signature covers all of them. Measured: per-version signatures would add 3,309 bytes per version (158% of a login, 33 MB for 10,000 items); the head's adds 0.3% of a 10,000-item head. Plain version: one strong signature on the vault's table of contents protects every item in it, at almost no cost. | Proposed; audit item |
| D337 | **The heads' rollout: level 2.** Each device marks `lockdown.pq = 2` on its own record. A device signs heads post-quantum once the key is published and every device not revoked marks 2, or once the chain it extends already carries one. Extends D318. Plain version: as with the identity's key, the stronger signature on your vault switches on when every device can read it. | Proposed; audit item |
| D338 | **The heads' rules.** Every `pq_sig` on a head by the identity's devices must verify. Once one head of a chain carries it, every later one must. Each device remembers the first post-quantum head the server confirmed (`pq_since`, never from a provisional head) and refuses any head at or after it without one. A device that remembers one but knows no key refuses the vault. Checked on open, on every pull and on `verify`. Plain version: once your vault is signed the new way, a server can't quietly take the new signatures off. | Proposed; audit item |
| D339 | **Own devices check the profile's post-quantum key against their own derivation**, and refuse to open a vault when they differ. Plain version: your devices notice if someone swaps the key your account publishes. | Proposed; audit item |
| D340 | **The log's tree heads carry ML-DSA-65 by a post-quantum log key.** `pq_log_seed` is added once to the server key file; `SignedTreeHead.pq_sig` uses context `lockdown kt tree head`; `SthWire.pq_log_key`. Clients pin the key the first time it is offered; then the signature is required, and a dropped or changed key is refused. Restoring a key file from before the upgrade changes the key, which pinned clients refuse. Plain version: the public log your apps check is signed the new way too. Operators should back up the key file again after upgrading. | Proposed; audit item; **flagged for Sean** (operator-facing: back up the key file after the upgrade) |
| D341 | **The server checks a pushed head's post-quantum signature** against the key the account's profile publishes, and refuses a bad one, or one from an account that publishes none. It doesn't require the signature (the clients' rule, D338). Plain version: the server turns away a head whose new signature is wrong. | Proposed; audit item |
| D342 | **Checkpoints carry post-quantum parts only after the rollout**: the tree head's ML-DSA signature, the pinned post-quantum log key (under both signatures) and the identity's ML-DSA signature (context `lockdown kt checkpoint`). A checkpoint with any of them must carry and pass the last. Plain version: the page you keep to compare devices is signed the new way once all your devices read it. | Proposed; audit item |
| D343 | **Shared vaults and items shared on their own stay Ed25519.** Heads, roster events and grants by other people's devices are not checked post-quantum (their keys would come from pinned contacts, with a rollout across everyone in the vault). This identity's own heads in a shared vault are signed and checked as in a personal one. Plain version: what other people write in a vault you share is protected the old way for now. | Proposed; audit item |
| D344 | **ML-DSA runs optimised in dev and test builds** (`opt-level = 3` for `ml-dsa`, `module-lattice`, `shake`, `keccak`), as `sha2` does: unoptimised, one signature over a 10,000-item head took 422 ms. | Accepted by default under D38 (build, not security) |
| D345 | **Wire and file additions (D62, additive):** §10's fields; the `lockdown.pq` mark's level 2; the server key file's `pq_log_seed`. The CLI now runs the rollout and signs identity manifests with both keys, as the app does. Plain version: older and newer parts keep understanding each other. | Accepted by default under D38 |
| D346 | **Show the post-quantum state in Security** ("Post-quantum signatures: on", or "Waiting for: Lockdown on the old laptop"), from `devices_without_support` and `devices_without_head_support`. Not built. Plain version: you can see which device is holding the stronger signatures back. | Proposed; **flagged for Sean** (a new line on a screen) |
