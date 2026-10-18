# Sharing, households and emergency access: first slice

_Status: built, 2026-10-18. Implements the "Shared" and "Family" vault kinds of `02-model.md` §1, "membership is server-ordered, never merged" from `03-architecture.md` §6, the sharing rows of `04-security.md` §6 and §18 (authenticated sharing keys), and the matrix rows `shared-vaults`, `granular-permissions`, `key-transparency` (the sharing half), `item-sharing`, `share-links`, `request-link`, `fill-only-sharing`, `family-plan` and `emergency-access`. Builds on `01-identity.md` (personas, receiving keys, the key-transparency log) and `15-organization.md` (more than one vault per persona)._

## For future agent

- **TL;DR:** A vault is shared by wrapping its Vault Key to another persona's receiving key, found and checked through the key-transparency log, inside an envelope the sending device signs. Who is in a vault and what each may do is a signed, hash-chained list of membership events that every device replays with the same rules; the server stores and orders it and enforces it on writes, but can't forge it. Removing someone moves the vault to a new Vault Key at once. An item can be shared on its own the same way with its Item Key. Share links and request links are sealed on the device, with the key in the URL fragment. A household is a shared vault with an organizer. Emergency access hands over keys the owner sealed in advance, only after a waiting period the owner can stop. Rights (fill only, view, edit, manage) are decided in `lockdown-vault` and enforced in `lockdown-app`; the server refuses writes from anyone without edit.
- **Read with:** `crates/vault/src/sharing.rs` (rights, membership events, the roster, share envelopes, emergency grants), `crates/vault/src/contact.rs` (contacts and fingerprints), `crates/vault/src/send.rs` (share and request links), `crates/vault/src/vault/share.rs` (wrapping, rotation, received items), `crates/sync/src/sharing.rs` (the wire and the client), `bins/server/src/sharing.rs` and `sharing_api.rs` (storage and handlers), `crates/app/src/session/sharing.rs` (the session logic) and `crates/app/src/sharing.rs` (the `App` surface), `apps/desktop/src/features/sharing/` (the UI).
- **Do not:** wrap a key to a receiving key that didn't come from a verified log entry; accept a membership event, an envelope or a grant step without checking its signature against a device of the account that should have made it; let a removal go through without a new Vault Key generation; let a fill-only value leave the core except into a browser fill; put a crown jewel in a shared vault, an item share or a share link; send the URL fragment of a link to any server.

## 1. What was built, per row

| Row | Status | What |
|---|---|---|
| `shared-vaults` | done | Add and remove members; the Vault Key wrapped to each member's receiving key from the log; rotation to a new Vault Key generation on removal, re-wrapped to everyone who stays; server tables for membership and per-member sealed envelopes; member devices open the vault with the ordinary `Vault::load`. |
| `granular-permissions` | done | Fill only < view < edit < manage, decided in `lockdown-vault::sharing::Right` and enforced in the core; the desktop offers only what the right allows; the server refuses heads and items from members without edit and membership changes from anyone without manage; every change is a signed event shown as history. |
| `key-transparency` (sharing half) | done | Look up another account through `/v1/directory`, verify the entry by inclusion proof against a verified tree head, pin it, show a fingerprint (six groups of five digits, plus a `lockdown:contact:v1:` QR payload), compare it, and refuse to share to a key that doesn't verify or that changed since it was pinned. |
| `item-sharing` | done | One item shared by wrapping its Item Key to the recipient, to view or to fill only; two taps in the desktop (who, then Share); stopping a share moves the item to a fresh Item Key. |
| `share-links` | done | Links for an item, some text or a file; sealed on the device, key in the fragment; the server keeps ciphertext, an expiry, a view limit and an optional password's verifier. The server's `/s/{id}` page is a no-script notice; opening a link in a plain browser is deferred to the web vault (§7). |
| `request-link` | done | A single-use link someone else uses to send you a secret, sealed on their device to your receiving key, which the link pins. |
| `fill-only-sharing` | done | The core refuses reveal, copy, codes, attachments, export and share links for fill-only items and serves only browser fills; the extension's popup gets no copy target for them and its copy goes through the refusing path. A fill-only passkey still signs in. |
| `family-plan` | done (no billing) | A household: a `Family` shared vault with one organizer who manages it. Billing is out of scope for Lockdown and not built. |
| `emergency-access` | done | A trusted contact, looked up through the log and with fingerprints compared; a wait of 1 to 30 days the owner can stop; the server releases the sealed keys only after the wait; every step is signed and listed. |
| Desktop UI | done | A Sharing screen and a Share menu on each item, from unicron components (§8, §11). |

## 2. Keys between people

**The wrap.** `ShareEnvelope` holds a 256-bit key wrapped with the identity crate's peer wrap (`x-wing-xchacha20poly1305`, purpose `share`) to the recipient persona's receiving key, plus a `SharePlace` naming exactly where the key belongs (purpose `vault`, `item` or `emergency`, the vault, the item, the key's id, the recipient, the sender's account and, for emergency, the grant). The sending device signs the whole envelope. A public-key wrap alone proves nothing about who made it (anyone, the server included, can encrypt to a public key), so a recipient opens an envelope only after checking the signature against a device of the account that should have sent it (04-security §18).

**Vault Keys have generations.** Each manifest head carries `vault_key_ref`; items name the generation that wraps their Item Key. `Vault::vault_key_named` finds a generation among the current key and `old_vault_keys`, so a member reads items under the generation that wrapped them.

**Rotation re-keys, it does not re-encrypt.** Key envelopes sit outside every version's signature and hash (D36). Moving a vault to a new Vault Key gives each item a fresh Item Key and re-wraps its Version and Attachment Keys under it, without touching a sealed field or a signed byte (`Vault::rotate_vault_key`, `rekey_item`). A removed member who kept the old keys can still read what they could already read; every version saved afterwards is under keys they never held. The server refuses a head or an item still under an old generation once a newer one is recorded (`KeyRotated`, a new `ErrorCode`), so a stale writer can't put the old key back in use.

**Member vaults open like any other.** `member_record` builds the record a member's device keeps: the owner's public context (whose ids every field is bound to), the member persona's own App Data Key id, and the Vault Key generation wrapped under that key. `Vault::load` then opens the shared vault unchanged. Items shared one by one live in in-memory "received" vaults (`Vault::received`) rebuilt on each sync, so the same item code reads them.

## 3. People and key transparency

- **Lookup.** `/v1/directory?login=` answers a `DirectoryEntry`: the identity manifest, the profile, persona and device records, and the log leaf and index. `verify_directory_entry` checks the inclusion proof against a tree head the device has verified (the same STH path `sync_transparency` uses), then `PublicIdentity::verify` checks every record's signature, that each persona and device the manifest names is present, and that no secret envelope came along.
- **Pinning.** A `Contact` keeps the account, the receiving persona, its KEM key, the device signing keys, the identity sequence and hash, the log index and when it was verified. `check_continues` refuses a later entry whose keys changed under the same account; the person forgets the contact and looks them up again, deliberately.
- **Fingerprint.** BLAKE3 in derive-key mode over the profile id, the identity signing key, the persona id and its receiving (KEM) key, shown as six groups of five digits (about 100 bits). The same bytes go in a QR payload `lockdown:contact:v1:<account>:<hex>`. Comparing means typing what the other person reads out; the core compares, ignoring spaces. A compared contact is marked verified; emergency access requires it.

## 4. Shared vaults and rights

**Membership events.** `MembershipEvent { vault_id, seq, prev, change, key_ref, by_account, by_device, at, sig }`, signed with the prefix `lockdown-membership-v1\0` and chained by hash. Changes: `Create { owner, kind }`, `Add { member, right }`, `SetRight`, `Remove`. `Roster::replay` applies the chain with the rules every device and the server share:

- only an account with manage changes membership; the owner can't be removed or demoted;
- a removal must name a new Vault Key generation, and no event may go back to an earlier one;
- `writers` records every account that ever held edit, so an old version's signer is still recognised after a right is lowered.

Events are ordered by the server (`/v1/vaults/{vault}/members`, push and since) and never merged: a push that isn't the next in sequence is refused and the device replays and tries again.

**Rights.** `Right::FillOnly < View < Edit < Manage`, with `can_reveal`, `can_edit`, `can_manage` and the words the UI shows. In the core, `holding_mut` and `vault_by_id_mut` call `require_edit`, so every write path (edit, trash, restore, purge, move, tags, attachments, passkey changes, duplicate merges, trash sweeps) refuses without edit. `Core::reveal` and `Core::code_now` call `refuse_fill_only`; the browser fills go through `reveal_for_fill` and `code_for_fill`. Fill-only items are masked in `show`, carry no `copy` target, and are left out of codes, exports and attachments. `ItemSummary.access`, `ItemDetail.access` and `VaultView.sharing` say the right to the UI (fields added under D62).

**On the server.** `vault_access` answers Owner, Member(right) or Emergency for each request; `readable` and `writable` gate every vault endpoint. Heads and items from a member without edit are refused, and so is a version signed by a device outside the vault's writers. `/v1/vaults/{vault}/key` serves a member their own envelope for the current generation. A removal must carry envelopes for every member who stays, and for the owner when a manager who isn't the owner removes someone. A crown jewel can't be pushed into a shared vault.

**Crown jewels never share.** A vault holding a crown jewel can't be shared until it is moved out (the UI says why), and a crown jewel can't be added to, or moved into, a shared vault.

## 5. Items shared one by one, and fill only

`share_item` wraps the item's Item Key to the contact, to view or to fill only (an item share never grants edit: the owner's vault keeps one writer per item). The recipient's device fetches `/v1/shares/items`, checks the envelope's signature against the owner's devices as the log records them, and reads the item through a received vault. `unshare_item` deletes the share and gives the item a fresh Item Key at once, so the withdrawn person can't read later versions. Only items in the person's own vaults can be shared on their own.

Fill only is a promise honest clients keep, not cryptography: a member holds the key and a modified client could read what it fills. The core keeps it everywhere a value could leave (reveal, copy, codes, attachments, export, share links, the extension popup's copy), and still lets the browser fill the value into the page and a passkey sign in. A person who must not learn a password should not be given it at all.

## 6. Households and emergency access

**Household.** `create_household` makes a `Family` shared vault with the organizer as owner and manager; `add_to_household` adds people with a right, through the `Approve` gate. It is a shared vault in every other way. Billing and seats are not part of Lockdown.

**Emergency access.** `EmergencyGrant { grant_id, owner, contact, wait_days (1–30, default 7), vaults, by_device, sig }`, signed by the owner's device. The owner seals each chosen vault's current Vault Key to the contact (`SharePurpose::Emergency`, naming the grant) and the server holds those envelopes. Steps are signed `EmergencyStep`s: the contact accepts, declines or asks; the owner stops a request or revokes the grant. States: invited → accepted → requested → ready (the wait ran out) → released, with declined and revoked as ends. The server releases the envelopes only when a request is older than the wait and not stopped; the contact's device checks the grant's and the envelopes' signatures and opens the vaults to read. The owner sees "asked for access" on the Sharing screen with a one-press Stop.

## 7. Share links and request links

**Share links.** `seal_send` makes a random 256-bit link secret; with a password, Argon2id over it at the WASM floor (so the web vault can open the same link) mixes in; HKDF gives the content key and an access proof; the payload (an item's fields, text, or a file) is sealed with XChaCha20-Poly1305. The server keeps the ciphertext, the expiry (1 to 30 days), the view limit (1 to 1,000, or none) and the BLAKE3 of the access proof, and hands out the ciphertext only for the right proof, counting a view and deleting the link at its limit. At most 200 open links per account. The link is `<server>/s/<id>#<b64url>`; the fragment never reaches a server. Putting an item in a link goes through the `Approve` gate and counts as a use (D151).

**Request links.** `request_link` names the request and, in the fragment, the BLAKE3 of the asking persona's receiving key. The person answering fetches `/v1/requests/{id}/key`, checks it against the fragment (so a server that offers another key is caught), seals the answer with the peer wrap, and posts it once. The asker opens the answer on their device.

**The browser pages.** `/s/{id}` and `/r/{id}` are server pages with a strict CSP and no script: they say what the link is and that it opens in Lockdown. Opening a share link or answering a request in a plain browser belongs to the web vault, which can reuse `lockdown-vault::send` compiled to WASM.

## 8. The desktop

- **Sharing screen** (`features/sharing/sharing-view.tsx`): the person's fingerprint; People (look up, compare, forget); Vaults with Manage sharing (members, a right per member, Remove with a key change, Add through the gate, history, and the reason a vault can't be shared); Household; items shared one by one (Stop sharing, Open); Share links (make one for text, list, delete, open one with its password, see what it holds, keep it as a note or save its file); Request links (make, list, open the answer, delete, answer someone else's); Emergency access (add a verified contact with the wait and the vaults, each grant's state in words, its history and the next steps).
- **Item detail:** a Share menu ("With someone…" and "As a share link…") on the person's own items, never on a crown jewel; "Shared with you" with the right on others'. Edit, Trash, Restore, moving, conflict resolution and file changes appear only with edit; a fill-only item's hidden fields say "Fills in the browser only; never shown or copied" and offer no reveal, copy or large type, and its files and codes are not offered.
- **Sidebar:** shared vaults carry a people, home or emergency icon and the owner's name when it isn't the person's.
- **Commands** (`src-tauri/src/sharing.rs`, appended to the handler list): `sharing_overview`, `contact_lookup`, `contact_verify`, `contact_forget`, `vault_sharing`, `vault_share`, `vault_member_right`, `vault_member_remove`, `household_create`, `household_add`, `item_share`, `item_unshare`, `send_item`, `send_text`, `sends`, `send_delete`, `send_open`, `opened_value`, `opened_file`, `opened_save`, `request_create`, `requests`, `request_open`, `request_delete`, `request_label`, `request_answer`, `emergency_add`, `emergency_list`, `emergency_step`, `emergency_open`, `opened_copy`, `link_copy`.
- **What crosses the bridge.** Names, ids, rights, fingerprints and words. A link made here comes back as raw bytes, decoded once and the buffer wiped; it is copied with `link_copy` on the shell's clipboard path and timer (D85), like a password. What a link or an answer holds stays in the core and comes over one value at a time (`opened_value`), shown through unicron's `SecretField` and copied with `opened_copy`. Typed secrets go in as byte arrays the webview zeroes after the call.

## 9. How to run and test

```sh
export CARGO_TARGET_DIR=… CARGO_INCREMENTAL=0
cargo test -p lockdown-vault sharing contact send share   # rights, events, rosters, envelopes, contacts, links, rotation
cargo test -p lockdown-sync sharing                       # the directory check against a fake log
cargo test -p lockdown-server --test sharing              # every endpoint in process
cargo test -p lockdown-app --test sharing                 # two and three people end to end against the real server
cargo test -p lockdown-desktop --features shell           # the commands' option checks
cd apps/desktop && pnpm test                              # the Sharing screen and the rights-aware detail
```

| Layer | Where | Count | What |
|---|---|---|---|
| Model | `crates/vault/src/sharing.rs` | 4 | what each right allows; a roster follows its signed chain and refuses what rights forbid (only manage, a removal needs a new generation, no going back, the owner stays, writers remembered); a shared key opens only for its persona, at its place, from its sender; an emergency grant is signed, bounded and in order |
| Contacts | `crates/vault/src/contact.rs` | 2 | a public identity verifies, pins, and a key swap is refused; the digits are stable |
| Links | `crates/vault/src/send.rs` | 2 | a share link opens with its secret and password only; a request is answered to the key its link pins |
| Vault | `crates/vault/src/vault/share.rs` | 2 | rotation moves every item to fresh keys and keeps history readable; a member record opens the shared vault and a received item stands alone |
| Sync | `crates/sync/src/sharing.rs` | 2 | the sharing bodies round-trip; a directory entry counts only when the log records its manifest (against a fake log) |
| Server | `bins/server/tests/sharing.rs` | 7 | a shared vault read and written by its people by right; a head or item under an old generation refused as `KeyRotated`; item shares reach only their recipient; share links expire, count views and check their password; a request link takes one answer; emergency access waits, can be stopped and releases once (the clock moved); the link pages load nothing |
| App | `crates/app/tests/sharing.rs` | 5 | a vault shared by right with fingerprints, rotation and fill only; an item shared to fill only and withdrawn (no reveal, no copy target, no `reveal_to_use`, fills work); share and request links between two people (a link of an item is one use); a household and emergency access with its wait, stop and release; a fill-only passkey signs in but is never revealed or deleted |
| Desktop shell | `src-tauri/src/sharing.rs` | 1 | link options and passwords checked before anything is sealed |
| Desktop UI | `features/sharing/*.test.tsx` | 21 | fingerprint and people; offline; lookup and compare, a refused compare; a manager's rights, Remove with the key change; a viewer sees no controls; a crown-jewel vault's reason; a text link shown once and copied on the shell's path; opening a link (plain drawn, secrets masked); answering a request (the typed bytes wiped); emergency Stop, the verified-only rule, adding with the wait; the steps by state; the item detail by right (fill only, view, edit, own, crown jewel), two-tap item share, an item's share link |

The extension is unchanged: its popup already offers Copy only for a row's copy target, which fill-only rows don't have, and its copy goes through `reveal_to_use`, which refuses them (asserted in `crates/app/tests/sharing.rs`).

## 10. Security notes and gaps

- **Fill only is policy.** See §5. The UI words say so ("never shown or copied" is what Lockdown does, not what a modified client can't).
- **Gossip.** A member vault's first open trusts the directory entry's inclusion proof against a tree head the device verified, but doesn't yet cross-check tree-head citations with other members (the "gossip" half of key transparency).
- **A stale directory entry.** Inclusion proves an entry is in the log, not that it is the newest for the account; a server could serve an older entry. The pin (`check_continues`) catches a change after first contact, not a stale first answer.
- **A revoked contact device** is still accepted for signatures it made before it was revoked, as history; the server refuses its new writes.
- **Leaving a vault** isn't built: a manager removes the member.
- **Emergency vaults** trust only the owner's devices for their content, so edits by other members of a shared vault after release aren't checked against those members' keys.
- **Passkey changes on a shared item** are refused in the core without edit; the desktop's passkey list (another slice) still shows Delete, and the refusal comes back as the core's words.
- **The web vault page** for opening links in a browser is deferred (§7).
- **Logs** carry no secret: ids, rights and counts only.

## 11. unicron

Built from `Item`, `ItemGroup`, `Badge`, `Button`, `Dialog`, `DropdownMenu`, `Field`, `Input`, `Textarea`, `NativeSelect`, `ToggleGroup`, `Checkbox`, `Separator`, `Spinner` and `SecretField`; no className or style on a unicron component (the ratchet stays at 4). Host-contract gaps found:

- **No QR component.** The fingerprint's QR payload is computed but shown only as digits; a `QrCode` (render-only, no network) would let a phone scan it.
- **No fingerprint display.** Six groups of five digits wants a component that spaces, speaks digit by digit and compares two strings, like `SecretField` spells secrets.
- **`SecretField` has no copy-only host for text the shell holds outside a vault.** The opened values use `copyRef` with a shell command, which works; a link (not a vault field) needed its own `link_copy` command because the field's host contract is per value, not per string the UI already has.
- **No timeline.** Membership history and emergency steps are plain ordered lists; a small `Timeline` (time, actor, words) would fit both and the account's sign-in history.

## 12. Proposed decisions

For the coordinator to record. **Flagged** rows are user-visible and need Sean.

| ID | Decision | Owner |
|---|---|---|
| D185 | **Keys between people** go in a `ShareEnvelope`: the identity crate's peer wrap (X-Wing, purpose `share`) of the key, plus where it belongs (vault, item or emergency grant, key id, recipient, sender), signed by the sending device and checked against that account's pinned devices before it is opened. Plain version: a shared key proves who sent it, so nobody (the server included) can slip in their own. | Accepted by default under D38; audit item |
| D186 | **Membership is a signed, hash-chained event list**, ordered by the server and never merged; every device and the server replay it with the same rules: only manage changes members, the owner stays, a removal names a new Vault Key generation, no return to an old one, and every account that ever had edit is remembered as a writer. Plain version: who is in a shared vault is a tamper-evident history everyone checks the same way. | Accepted by default under D38 |
| D187 | **Four rights**: fill only, view, edit, manage, in that order, worded as "Fill only: Lockdown fills it in the browser; it is never shown or copied", "View: see, copy and fill", "Edit: see, fill and change items", "Manage: change items and who has access". Fill only is enforced by Lockdown, not by cryptography, and the help says so. | **Flagged for Sean** (the four words and the honesty note) |
| D188 | **Removing someone re-keys the vault at once**: fresh Item Keys, Version and Attachment Keys re-wrapped (no re-encryption, D36), the new Vault Key to everyone who stays; the server refuses anything still under the old generation (`KeyRotated`). What the removed person already saw, they may have kept. Plain version: someone you remove can't read anything changed after. | **Flagged for Sean** (the "they may have kept what they saw" wording in the confirm) |
| D189 | **Crown jewels never share**: not in a shared vault or household, not shared on their own, not in a share link, not moved into a shared vault; a vault holding one says why it can't be shared. | **Flagged for Sean** |
| D190 | **Contacts and fingerprints**: a person is looked up through the key-transparency log and pinned; their fingerprint is BLAKE3 derive-key over the profile, its identity signing key, the persona and its receiving key, shown as six groups of five digits and as a `lockdown:contact:v1:` QR payload; comparing is optional for sharing, required for emergency access; a pinned person whose keys change is refused until forgotten and looked up again. | **Flagged for Sean** (digits as the format; compare optional for sharing) |
| D191 | **Items shared on their own** are view or fill only, never edit, only from the person's own vaults; stopping a share gives the item a fresh Item Key. Two taps from the item: who, then Share. | **Flagged for Sean** |
| D192 | **Share links**: sealed on the device, the key in the fragment; lifetimes 1, 3, 7, 14 or 30 days (default 7); opens once, 3, 10 or until expiry (default once; the server allows 1–1,000); an optional password through Argon2id at the WASM floor with a server-side proof hash; at most 200 open links per account; putting an item in one goes through `Approve` and counts as a use. | **Flagged for Sean** (the defaults) |
| D193 | **Request links**: single use, 1 to 30 days, the asker's receiving key pinned by hash in the fragment; the answer is text, sealed with the peer wrap; the asker opens it on their device and can keep it as a note. | Accepted by default under D38 |
| D194 | **A household is a shared vault** of kind `Family` with one organizer who manages it; billing, seats and plans are out of scope for Lockdown. | **Flagged for Sean** (no billing) |
| D195 | **Emergency access**: only a contact whose fingerprint was compared; a wait of 1 to 30 days (default 7) the owner can stop at any point before release; the server releases the keys sealed in advance only after the wait; the contact reads, never edits; every step signed and listed; the owner sees "asked for access" with Stop on the Sharing screen. | **Flagged for Sean** (the default wait, read-only) |
| D196 | **Gates**: sharing a vault, adding to a household, sharing an item, putting an item in a share link and naming an emergency contact go through `Approve`; removing someone and stopping a share don't (they take access away). | Accepted by default under D38 |
| D197 | **Links are copied like passwords**: a link made in the desktop (share or request) and a value opened from one go to the clipboard from the shell with the clipboard timer (`link_copy`, `opened_copy`), never from the webview. | Accepted by default under D38 |
| D198 | **Server link pages** `/s/{id}` and `/r/{id}` are no-script notices under a strict CSP; opening a link and answering a request in a plain browser is the web vault's, from the same `lockdown-vault::send` code. | Accepted by default under D38; the web vault slice picks it up |
| D199 | **Wire additions** (D62, additive): `ErrorCode::KeyRotated`; the `/v1/directory`, `/v1/shared`, `/v1/vaults/{vault}/members`, `/v1/vaults/{vault}/key`, `/v1/shares/items*`, `/v1/sends*`, `/v1/requests*`, `/v1/emergency*` endpoints; `ItemSummary.access`, `ItemDetail.access` and `VaultView.sharing` in the app's views, omitted when empty. | Accepted by default under D38 |
