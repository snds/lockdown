# Vault and item model — v0.1 (draft)

_Status: draft, 2026-10-15. Implements D1, D3, D5, D8, D9, D11, D13. Builds on `01-identity.md` (the key hierarchy and records live there, not here). Evidence: research brief §2.9, §4.2, §4.7; `reports/04-foundation.md` §2, §3.1, §4.4; `feature-matrix.json` (categories `vault-item-types`, `organization-search`, `sharing-collaboration`, `import-export-portability`)._

## For future agent

- **TL;DR:** A vault is a keyed box of items. Each persona owns its own vaults. Shared, family, organization and legacy vaults are the same box with the key wrapped to more people. Every item is typed, every field is encrypted on its own, and every item has its own key. A signed, hash-chained manifest per vault is the table of contents that stops a hostile server from rolling anything back. High-blast-radius items go in the crown-jewel class under the Crown-Jewel Key from `01-identity.md` §3. Nothing in the server is readable, including titles and URLs.
- **Read with:** `01-identity.md` §3–§4 (keys and envelopes), `decisions.md`, brief §4.2 (the tree of locked boxes), brief §2.9 point 5 (crown-jewel rules).
- **Do not:** add a new key layer, redefine `KeyEnvelope`, or store any plain-text metadata. If a feature seems to need one of those, write an open question instead.

## 1. Vaults

A vault is one random 256-bit Vault Key plus the items it locks. The kinds differ only in who holds the key and what the policy engine allows.

| Kind | One per | Who holds the Vault Key | Notes |
|---|---|---|---|
| **Personal** | persona (D13) | the persona's Lockdown App Data Key (`01-identity` §3) wraps it | The default vault. A persona may have more than one. A `work` persona cannot open a `personal` vault because it never holds that Persona Key. |
| **Shared** | ad-hoc group | wrapped to each member's hybrid public key through HPKE (brief §4.5) | Members hold view, edit, fill-only, manage or re-share rights. Removal rotates the Vault Key and re-wraps items. |
| **Family** | household | as shared, plus an organizer role | Holds Wi-Fi, streaming and door codes. The organizer can run emergency access and organizer recovery (brief §4.8), never alone and never silently. |
| **Organization** | business unit or collection | as shared, plus groups and policy | Policies come from the policy engine. Admin recovery needs M-of-N admins, member consent and a log entry. |
| **Legacy** | heir set | Vault Key split with Shamir k-of-n, each share wrapped to one heir | States: armed, check-in, grace, release-pending, released. The server only runs the clock. Crown-jewel items stay out unless the owner opts them in. |

Rules for every kind: the Vault Key travels in a `KeyEnvelope` (`01-identity` §4), `purpose: "app-data"` for the owner and `purpose: "share"` for others (**[uncertain]** whether to add `purpose: "vault"`; proposed as additive). Before wrapping a key to anyone, the app checks that person's key in the key-transparency log (brief §4.6); high-value shares add an in-person QR or short-code check. Free tier: three vaults per persona and two-person sharing.

## 2. Keys, in one line each

The full tree is in `01-identity.md` §3. This doc only adds the leaves below the App Data Key.

```
Persona Key ─wraps─▶ Lockdown App Data Key ─wraps─▶ Vault Key ─wraps─▶ Item Key ─wraps─▶ Version Key ─encrypts─▶ fields
                                                                       └─wraps─▶ Attachment Key (one per file)
Persona Key ─wraps─▶ Crown-Jewel Key ─wraps─▶ Item Key of every crown-jewel item
```

- **Item Key.** One per item. Sharing or deleting one item never re-encrypts the vault.
- **Version Key.** One per saved version of an item. Deleting a version destroys its key, so the ciphertext is dead ("crypto-shredding", brief §4.7).
- **Attachment Key.** One per file, so a 2 GB file is never fully decrypted in memory.
- **Associated data.** Every field is encrypted with XChaCha20-Poly1305 and associated data that names `{profile_id, persona_id, vault_id, item_id, version_n, field_id, kind, schema_v}` (the same binding `04-security.md` §5 states). A server that moves a ciphertext between fields or items gets a decryption failure, not a swapped value (brief §4.2, foundation §4.1 anti-pattern 2).

## 3. The crown-jewel class

The Crown-Jewel Key (CJK, `01-identity` §3) is a second branch under the Persona Key. An item in this class has its Item Key wrapped by the CJK instead of the Vault Key. The rules come from brief §2.9 point 5 and `01-identity` §3, and they are not configurable downward.

| Rule | What it means for the user |
|---|---|
| Fresh biometric plus a deliberate tap to use | Signing a confirmation or copying a value needs Face ID, Touch ID, Hello or StrongBox right then, not a cached unlock |
| Full account password to reveal or export | A fingerprint is not enough to show the secret on screen |
| Never autofilled | The extension and OS providers do not see these items |
| Never in search previews | Search shows the title only, never a field |
| Never given to agents | Deny by default; override per item with the full password |
| Out of bulk export and legacy release | Unless the user opts each item in |
| "This device only" option | The item never syncs; the manifest holds a tombstone that says so (`04-security.md` §10) |

Which types land there:

| Default into crown-jewel | Suggested (prompt once, user decides) | Never by default |
|---|---|---|
| Crypto wallet seed and recovery phrases (BIP-39, SLIP-39 detected on paste) | The master login of the mailbox used for recovery | Everyday logins |
| Steam `identity_secret` and revocation code (D8; if the bridge ever ships) | Identity-provider master logins that appear in the sign-in-with map (§9) | Codes, Wi-Fi, cards |
| Recovery codes and backup codes for any account | Banking and brokerage logins | Notes, documents |
| Another manager's recovery kit stored as a document | | |

A crown-jewel login cannot be autofilled. That is on purpose, and it costs a copy with a password check each time. **[uncertain]** whether a middle tier for mailbox logins is worth it; see open questions.

## 4. Item types

Every item has one `type` and a `schema_v`. Types are schema-versioned so an old client refuses an item it cannot read rather than guessing. Twenty-five or more typed templates ship in v1 (matrix `typed-templates`, target beat). The table lists the types with special behaviour; the rest are plain templates.

| Type | Anatomy (typed fields beyond the common ones) | Special behaviour |
|---|---|---|
| **Login** | username, password, `urls[]` each with a match rule (§10), `sign_in_method` (password / passkey / federated / sso), `provider_ref` (§9), embedded `passkeys[]` and `codes[]`, change-password link, last fill | The hub type. One record holds every way into one account. |
| **Passkey** | `rp_id`, `credential_id`, `user_handle`, `user_name`, private key (COSE), `aaguid`, backup flags, `related_origins[]`, created, last used, `signal_state` (visible / hidden-by-signal with undo) | No signature counter, so it transfers under CXF. Standalone when imported without a login; otherwise embedded in a Login. |
| **Code** | `secret`, `algorithm` (SHA-1/256/512), `digits` (6 / 8), `period`, `issuer`, `account`, `variant` (`totp` / `hotp` / `steam`) | `steam` is HMAC-SHA1, 30 s, five characters from the alphabet `23456789BCDFGHJKMNPQRTVWXY`; accepts `steam://` keys and converts base64 maFile secrets to base32. 8-digit variant covers Battle.net-style codes. Warns when the issuer equals the sync login (brief §4.9). |
| **Wi-Fi** | SSID, password, security (WPA2 / WPA3 / enterprise EAP with identity and CA), hidden flag, band notes | Full-screen guest QR card that hides itself; printable card; exported through the CXF Wi-Fi type. |
| **SSH key** | private key (OpenSSH or PEM), public key, fingerprint, key type, passphrase, allowed hosts, signing use | Served by the SSH agent with per-use approval (matrix `ssh-agent`, v1.x). Never autofilled. |
| **API credential** | key or token, secret, type (bearer / basic / HMAC), hostname, scopes, expiry, rotation note | Expiry reminders. Agent access only through a scoped token (v2). |
| **Document** | one or more attachments, document kind (passport / licence / contract / other), issue and expiry dates, number, issuing body | Expiry reminders. Camera scan to typed fields in v2 (matrix `document-scan`). |
| **Identity** | subtype (person / address / business / vehicle / passport / driver licence), subtype fields | Used for form fill. Kept apart from any contacts list, which is never used to fill. |
| **Card** | holder, number, expiry, CVC, issuer (detected), billing address ref | Second deliberate action before any fill; optional CVC re-prompt. |
| **Bank account** | bank, account and routing or IBAN and BIC, SWIFT, PIN, branch | Never autofilled. |
| **Note** | Markdown body | The one type where CRDT merging is allowed inside the body (see `03-architecture.md` §6). |
| **Crypto wallet** | address, seed or recovery phrase, wallet name, network | Seed field is crown-jewel by default (§3). |
| **Plain templates** | server, database, software licence, membership, and the rest of the 25+ | Typed fields, no special behaviour. |
| **Custom** | a user-defined template: name, icon, ordered field definitions | Free tier. Each custom field maps to a CXF extension field so export never drops it. |

Common to every type: `title`, `notes` (Markdown), `tags[]`, `folder_ref`, `favourite`, `archived`, `custom_fields[]`, `attachments[]`, `expires_at`, `created_at`, `updated_at`, `crown_jewel` flag, `device_only` flag.

## 5. Fields and custom fields

A field is the unit of encryption. Each has a stable `field_id` (so history can diff it), a `kind`, a `label`, an optional `section` and a `value`.

| Kind | Value | Fill and display rules |
|---|---|---|
| `text` | string | plain |
| `hidden` | string | masked; reveal on tap; copy with clipboard timer |
| `boolean` | true / false | |
| `date` | date, month-year or year | drives expiry reminders when named `expires_at` |
| `url` | string plus match rule | see §10 |
| `email`, `phone`, `address` | typed strings | fill into matching form fields |
| `totp` | a Code sub-record | second deliberate action before fill |
| `linked` | `item_ref` | shows the linked item inline; the link itself is an encrypted field |
| `file` | `attachment_ref` | see §6 |

Custom fields use the same kinds. A custom field is any field the type template did not define. Clients preserve unknown kinds on rewrite and never drop them (same rule as `01-identity` §9).

## 6. Attachments

- Each file gets its own Attachment Key, wrapped by the Item Key.
- The file is encrypted in 1 MiB chunks with XChaCha20-Poly1305. The chunk index and the file id are in each chunk's associated data, so chunks cannot be reordered or swapped.
- The server stores ciphertext blobs named by the hash of the ciphertext. It never sees a file name, type or size beyond the padded blob size.
- Limits: 5 GB per user on paid plans, with per-file caps set by policy. **[uncertain]** the free-tier limit.
- Attachments stay with their item through version history, trash, export and the legacy vault.

## 7. Versions, trash, archive, crypto-shredding

| State | What happens | Default |
|---|---|---|
| **New version** | Every save makes a version with its own Version Key, holding field-level diffs signed by the editing device. The manifest (§8) records its hash. | kept one year |
| **Restore** | One tap copies an old version's fields into a new version. History is never rewritten. | |
| **Archive** | `archived: true`. Out of autofill, agents and default search. Kept in exports. | free, every item |
| **Trash** | Soft delete with a grace period. Nothing is purged while a sync conflict on the item is open. | 30 days, per vault |
| **Purge** | The Item Key, every Version Key and every Attachment Key are destroyed. A tombstone with the last version hash keeps the chain valid. | after the grace period, or now on request |
| **Vault time machine** | Any past manifest head is a full snapshot, readable as far as its Version Keys still exist. | v1.x |

Crypto-shredding means ciphertext can sit in backups forever and stay unreadable. Deletion is honest without trusting the server to delete.

## 8. The vault manifest

Each vault has a signed, hash-chained table of contents (brief §4.7, foundation §4.4). It is the anti-rollback and anti-deletion control.

- One manifest head per vault. Each head names every live item id, its revision and the hash of its latest version, plus tombstones.
- Each head carries `prev_hash` of the one before it and a sequence number. This is the receipt roll.
- A head is signed by the device that made it. The server holds no device keys, so it cannot forge or reorder heads.
- Every device remembers the newest sequence it has seen. If the server serves an older one, the client refuses and says "Rollback attempt blocked."
- The server anchors each head's hash in a public checkpoint feed next to the key-transparency log, so outsiders can audit it.
- The same chain gives consumer history ("Dad changed the Netflix password on 3 October") and the share activity log, which every member sees equally.

Consent records for tolerated bridges (D8, brief §2.9 point 3) are items of a hidden `consent` type in the persona's vault, so they live in the same chain. The `01-identity` `Consent` record stays the source of truth; the vault copy is a pointer with the disclosure version.

## 9. Encrypted metadata and the sign-in-with map

**The rule:** the server sees ciphertext, item ids, revision numbers, sizes and the manifest. Nothing else. Titles, URLs, usernames, tags, folder names, icons and the sign-in-with map are all encrypted fields. This is the LastPass lesson (brief §4.1).

- **Icons.** A site icon is a function of the URL, which is secret. Clients ship a signed icon pack for the most common few thousand sites and look up locally. For the long tail they ask a privacy proxy that takes a domain, returns an icon, holds no account identity and keeps no logs. The user can turn the proxy off per persona. **[uncertain]** whether the proxy should use a private-information-retrieval scheme instead; parked.
- **Search index.** Built on the device from decrypted fields. Never synced in plain text.
- **Sign-in-with map.** A Login's `sign_in_method` and `provider_ref` record that, for example, `airbnb.com` is entered with a Google account, and which Login item that Google account is. The extension captures identity-provider button clicks and redirect chains. The map drives the blast-radius view ("if this Google account is lost, these 31 sites go with it"), the missing-2FA report and the "Sign in" button that picks the right method per site. It is encrypted like everything else.

## 10. Tags, folders, favourites, URL matching

- **Folders** are per vault and nested. A folder inherits its vault's sharing. Imports keep folder trees.
- **Tags** cross vaults and nest with `/`. They are encrypted fields on the item, not a server-side list.
- **Favourites** and **recency** rank suggestions. Fill counts and last-fill times stay on the device and never sync.
- **Smart views** (Passkeys, Codes, Wi-Fi, Expiring, Needs attention, Deleted) are computed from the local index.

URL matching is per URL, synced end to end, enforced by the matcher crate:

| Rule | Matches | Default |
|---|---|---|
| `exact_host` | scheme ignored, host must equal, port must equal if given | **yes** |
| `base_domain` | registrable domain by the Public Suffix List | opt-in per URL or per persona |
| `equivalent` | a curated, signed group such as `amazon.com` / `amazon.co.uk`, plus user-defined groups | the curated list is on by default; user groups are explicit |
| `starts_with` | the saved URL is a prefix | opt-in |
| `never` | the item is not offered on this URL | |

Cross-origin frames never get a fill without a confirmation. Lookalike hosts (homoglyphs, typo-squats) get a hard stop. Related origins from `/.well-known/webauthn` count for passkeys only, capped at five labels as browsers do.

## 11. Import and export

| Format | Direction | Carries | Does not carry |
|---|---|---|---|
| **CXF** (FIDO Credential Exchange Format, PS 2025-08 with 2026-03 errata) | both; through the iOS 26 and Android Play Services OS exchanges, and as a file on desktop | passwords, passkeys, codes, Wi-Fi, identities, cards, notes, folders; custom fields as CXF extension fields | version history, consent records, per-URL match rules **[uncertain]**: the errata may define a field for these |
| **Lockdown archive** (`.lockdown`): one JSON document plus attachment blobs, encrypted with Argon2id-derived or age-recipient keys and XChaCha20-Poly1305 streaming | both | everything: every item, field, attachment, folder, tag, favourite, archive state, version history, manifests, URL rules, the sign-in-with map, crown-jewel flags | nothing; this is the published backup format (reference decoder, open licence, v1.x) |
| **CSV** | both; plain-text export only behind an extra confirm; output is CSV-injection safe | logins, notes, cards, identities, codes as `otpauth://` | passkeys, attachments, history, structure |
| **Other managers' formats** | import only: 1PUX, Bitwarden JSON, Proton, KDBX, Dashlane, Keeper, LastPass, Enpass, browser CSVs, authenticator transfer QR and backups, Steam maFiles | whatever the source has | |

Every importer parses in isolation, caps sizes, validates paths and treats the file as hostile (brief §2.9 point 8). After import, the duplicate finder runs and a per-item report lists anything that did not map.

**Never lost in a round trip** through the Lockdown archive, and reported when lost through any other format:

1. Every field value, custom or typed, with its kind and label.
2. Passkey private keys, `rp_id`, `credential_id` and `user_handle`.
3. Code secrets with algorithm, digits, period and variant (a Steam code comes back as Steam).
4. Attachments, byte for byte.
5. Folders, tags, favourites, archive state and stable item ids (so a re-import merges, not duplicates).
6. Per-URL match rules and the sign-in-with map.
7. Crown-jewel and device-only flags (crown-jewel items only when the user opts in per item).

Export is free forever, even after a subscription lapses (D1, layer C).

## 12. Sketches

Item envelope (JSON-ish; on the wire it is CBOR):

```jsonc
{
  "v": 1,
  "item_id": "uuid",
  "vault_id": "uuid",
  "rev": 42,                       // server-authoritative revision, see 03-architecture §6
  "type": "login",
  "schema_v": 3,
  "class": "standard" | "crown_jewel",
  "device_only": false,
  "item_key": KeyEnvelope,         // wrapped_by: {kind: "vault", id} or {kind: "crown-jewel", id}
  "version": {
    "n": 7,
    "version_key": KeyEnvelope,    // wrapped by the item key
    "parent_hash": "blake3",
    "edited_by": "device_id",
    "edited_at": "time",
    "fields": [
      { "field_id": "title",   "kind": "text",   "ct": "…", "nonce": "…" },
      { "field_id": "url.0",   "kind": "url",    "ct": "…", "nonce": "…" },   // ct holds {value, match_rule}
      { "field_id": "pw",      "kind": "hidden", "ct": "…", "nonce": "…" },
      { "field_id": "passkey.0", "kind": "passkey", "ct": "…", "nonce": "…" },
      { "field_id": "code.0",  "kind": "totp",   "ct": "…", "nonce": "…" },
      { "field_id": "custom.3","kind": "date",   "ct": "…", "nonce": "…" }
    ],
    "attachments": [
      { "att_id": "uuid", "att_key": KeyEnvelope, "chunks": 1930, "blob": "blake3-of-ciphertext", "meta_ct": "…" }
    ],
    "sig": "ed25519 by the editing device over everything above"
  },
  "state": "live" | "archived" | "trashed" | "tombstone",
  "trashed_at": null
}
```

Associated data for each field is the canonical CBOR named in §2.

Vault manifest head:

```jsonc
{
  "v": 1,
  "vault_id": "uuid",
  "seq": 1187,
  "prev_hash": "blake3 of head 1186",
  "vault_key_ref": "key_id",       // which Vault Key generation this head is under
  "members": [ { "persona_id": "…", "rights": ["view","edit"], "key_env": "key_id" } ],   // encrypted as a field under the vault key
  "entries": [
    { "item_id": "uuid", "rev": 42, "version_hash": "blake3", "state": "live" },
    { "item_id": "uuid", "rev": 9,  "version_hash": "blake3", "state": "tombstone", "shredded": true }
  ],
  "signer": "device_id",
  "signed_at": "time",
  "sig": "ed25519 over the canonical head",
  "checkpoint": { "log_index": 88121, "inclusion_proof": "…" }   // added by the server, verified by clients
}
```

## 13. Open questions

1. Where the hybrid recipient key pair for shares lives. `04-security.md` §6 logs one per profile; `01-identity` §2 lists none. **[uncertain]**; proposed as an additive `Persona.kem_pub` field so the wall holds per persona (D13).
2. `purpose: "vault"` on `KeyEnvelope`, or reuse `app-data`?
3. A middle protection tier for mailbox and identity-provider logins that still allows a fill with a second action.
4. Whether the icon proxy is honest enough for self-hosters (D7), or whether the bundled pack plus a user-hosted proxy is the default there.
5. One Version Key per version, or per changed field? Per version is simpler; per field shreds more precisely.
6. CXF coverage of per-URL match rules and the sign-in-with map after the 2026-03 errata; if absent, propose an extension field and document the loss.
7. Device-only items are, by design, out of reach of the recovery kit (D11). The UI must say so when the flag is set.
