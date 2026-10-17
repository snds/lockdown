# Proposal to `ecosystem-identity`: envelope slots below the App Data Key

_Status: proposal, 2026-10-17, from the Lockdown session that built the store and the adoption path (`07-workspace.md` §4 steps 1 and 3). Carries out D37's promise. Nothing here is implemented in the crate; Lockdown runs on its own `vault::keys::WrappedKey` until the crate owner accepts, amends or declines this. Companion to `01-identity.md` §4 and `02-model.md` §2, §13 q.2._

## For future agent

- **TL;DR:** Lockdown wraps four kinds of key the identity crate has no words for (Vault, Item, Version, Attachment) under three kinds of wrapper it also has no words for (a Vault Key, an Item Key, the Crown-Jewel Key). It does so with an envelope shaped like the crate's `KeyEnvelope` and an associated-data record shaped like the crate's `Aad`, plus one field the crate lacks (`vault_id`). The ask is additive: new enum variants, one optional AAD field, and a rule that these envelopes may go unsigned by the ISK. When it lands, `lockdown-vault/src/keys.rs` becomes re-exports and there is one envelope code path in the ecosystem.
- **Read with:** `ecosystem-identity/src/records.rs` (`Purpose`, `KeyRefKind`, `Aad`), `src/keys.rs` (`wrap`, `unwrap`, `app_aad`), Lockdown `crates/vault/src/keys.rs` (what is proposed, as running code with tests).
- **Do not:** change the encoding of any existing envelope. Every addition below is `skip_serializing_if` or a new enum variant, so a `v: 1` envelope written today reads back byte-identical.

## 1. What Lockdown needed, as built

The key tree below the App Data Key (02-model §2):

```
App Data Key ─wraps─▶ Vault Key ─wraps─▶ Item Key ─wraps─▶ Version Key ─encrypts─▶ fields
                                                  └─wraps─▶ Attachment Key (one per file)
Crown-Jewel Key ─wraps─▶ Item Key of every crown-jewel item
```

Each arrow is one envelope. Lockdown's envelope today (`WrappedKey`):

| Field | Lockdown `WrappedKey` | Crate `KeyEnvelope` | Same? |
|---|---|---|---|
| `v` | `u16`, 1 | `u16`, 1 | yes |
| `key_id` | `Id` | `Id` | yes |
| `wrapped_by` | `{kind: WrapperKind, id}` | `{kind: KeyRefKind, id}` | shape yes; kinds differ (§2) |
| `alg` | `"xchacha20poly1305"` | `WrapAlg::XChaCha20Poly1305` | yes |
| `aad` | canonical CBOR of `KeyPlace` | canonical CBOR of `Aad` | shape differs by one field (§3) |
| `nonce` + `ciphertext` | `Sealed {nonce, ct}` | `nonce`, `ciphertext` | same bytes, nested differently |
| `purpose` | `KeyPurpose` | `Purpose` | values differ (§2) |
| `app` | — | `String` | Lockdown would set `"lockdown"` |
| `created_at` | `Timestamp` | `Timestamp` | yes |
| `enc` | — | `Option<Vec<u8>>` | not used below the App Data Key |
| `sig` | — | ISK signature | see §4 |
| `extra` | — | `Extra` | fine |

The reader's rule is the same in both: rebuild the associated data from where the envelope was found, refuse by name before decrypting if the stored `aad` disagrees, then let the tag decide.

## 2. New variants (additive)

```rust
pub enum Purpose {
    Irk, Persona, AppData, CrownJewel, Share,   // as today
    /// A Vault Key, wrapped by an App Data Key (owner) or a receiving key (share).
    Vault,
    /// An Item Key, wrapped by a Vault Key or by the Crown-Jewel Key.
    Item,
    /// A Version Key, wrapped by its Item Key.
    Version,
    /// An Attachment Key, wrapped by its Item Key.
    Attachment,
}

pub enum KeyRefKind {
    Account, Device, Persona, Irk, Kit, Peer,  // as today
    /// An App Data Key. `id` is that envelope's `key_id`.
    AppData,
    /// A Crown-Jewel Key. `id` is that envelope's `key_id`.
    CrownJewel,
    /// A Vault Key. `id` is that envelope's `key_id`.
    Vault,
    /// An Item Key. `id` is the item id (an Item Key's `key_id` is its item id, the way a Persona Key's is its persona id, D20).
    Item,
}
```

`as_str` values, for file names and messages: `vault`, `item`, `version`, `attachment`; `app-data`, `crown-jewel`, `vault`, `item`. Serde names follow the crate's `kebab-case` rule.

Note that `KeyRefKind::AppData` and `KeyRefKind::CrownJewel` are also new. Today nothing in the crate is wrapped *by* an App Data Key, so no kind names it as a wrapper; a Vault Key is the first thing that is.

## 3. The associated data

Lockdown's `KeyPlace`, which is the AAD of every envelope in the table above, and how each field maps onto the crate's `Aad`:

| `KeyPlace` field | Crate `Aad` field | Note |
|---|---|---|
| `profile_id: Id` | `profile_id: Id` | same |
| `persona_id: Id` | `persona_id: Option<Id>` | always `Some` below the App Data Key |
| `vault_id: Id` | **new:** `vault_id: Option<Id>` | `#[serde(default, skip_serializing_if = "Option::is_none")]`, so every existing envelope encodes exactly as before (same trick as `app`, D28) |
| `key_id: Id` | `key_id: Id` | same |
| `purpose: KeyPurpose` | `purpose: Purpose` | §2 |
| `wrapped_by: {kind, id}` | — | see below |
| — | `app: String` | Lockdown sets `"lockdown"`; the crate already binds it for `app-data` and `crown-jewel` and the same rule extends to the four new purposes |
| `v: u16` | `v: u16` | same |

**`wrapped_by` in the AAD.** Lockdown binds the wrapper's kind and id into the tag today. It is redundant: a wrong wrapper is a wrong key, and a wrong key fails the tag. The one case it would catch, the same bytes serving as two kinds of wrapper, does not arise because every key is random and used for one purpose. Lockdown will drop it on adoption rather than ask the crate to add it. If the crate owner prefers belt and braces, `wrapped_by: Option<KeyRef>` with the same skip rule is harmless.

**`vault_id`** is the field that earns its place. Without it an Item Key envelope moved from one vault to another of the same persona (same App Data Key tree) would be refused only by the Vault Key being different; with it the refusal is by name, before decryption, and the message says which vault the key belongs to. The same reasoning the crate applied to `persona_id`.

Worked AAD for the four new purposes, with `app = "lockdown"` in every one:

| Envelope | `persona_id` | `vault_id` | `key_id` | `purpose` | opened by |
|---|---|---|---|---|---|
| Vault Key under the App Data Key | the persona | the vault | the Vault Key's id (= the manifest head's `vault_key_ref`) | `vault` | `unwrap_app_key` result |
| Item Key under the Vault Key | the persona | the vault | the item id | `item` | the Vault Key |
| Item Key under the Crown-Jewel Key | the persona | the vault | the item id | `item` | `unwrap_crown_jewel_key` result |
| Version Key under the Item Key | the persona | the vault | the Version Key's id | `version` | the Item Key |
| Attachment Key under the Item Key | the persona | the vault | the attachment id | `attachment` | the Item Key |

These are the exact fields Lockdown's `Vault` computes today in `vault.rs` (`wrap_vault_key`, `item_key_place`, `version_key_place`); attachments follow the same shape when 07-workspace step 2 lands.

## 4. Signing

The crate signs every `KeyEnvelope` with the ISK. Lockdown's four new purposes are written by the device during ordinary edits (every save makes a Version Key), when the open `Identity` and its ISK may not be at hand, and in volume. Lockdown does not sign them, and D36 keeps them out of the version signature on purpose, so that shredding a key never breaks history. Their integrity rests on the AEAD binding: an envelope cannot be forged without the wrapping key, and one moved to another place opens nothing.

Proposed rule: **`sig` may be empty for purposes `vault`, `item`, `version` and `attachment`.** `verify` on such an envelope checks nothing and passes; `Record::verify` callers that iterate a store skip them. Everything above the App Data Key stays signed exactly as today.

If the owner would rather every envelope carry a signature, the device key (`DeviceKeystore::sign`, which is what signs Lockdown's manifest heads) is the right signer for these four, not the ISK; the proposal then becomes "signed by the ISK **or** by an enrolled device's key, named in a new optional `signer: Option<Id>` field".

## 5. API surface Lockdown would call

```rust
// unchanged
Identity::wrap_app_key(persona, "lockdown", &adk) -> Id
Identity::app_keys(persona, "lockdown") -> Vec<Id>
Identity::unwrap_app_key(persona, "lockdown", key_id) -> AppDataKey
Identity::wrap_crown_jewel_key / unwrap_crown_jewel_key

// wanted: the lister the crate has for app keys, for crown-jewel keys too
Identity::crown_jewel_keys(persona, app) -> Vec<Id>      // Lockdown filters `envelopes()` by hand today

// wanted: the free functions, with the new Aad field
keys::wrap(secret, &with, KeyRef::new(KeyRefKind::Vault, vault_key_id), &aad)
keys::unwrap_key::<ItemKey, _>(&envelope, &vault_key, &aad)
keys::vault_aad(profile_id, persona_id, vault_id, key_id, purpose, app) -> Aad   // the new constructor
```

Lockdown's own typed keys (`VaultKey`, `ItemKey`, `VersionKey`, `AttachmentKey`) already implement the crate's `SymmetricKey`, so `wrap`/`unwrap_key` work with them unchanged.

## 6. Migration

None needed. No envelope with the new purposes exists in any identity store yet; Lockdown's envelopes live inside its own item records and store, not in the identity root. When the crate ships the variants, Lockdown re-encodes nothing: `WrappedKey` becomes a type alias for `KeyEnvelope`, `KeyPlace::to_bytes` becomes `Aad::to_bytes` with `wrapped_by` gone and `app` added, and the `v` of every Lockdown item envelope goes from 1 to 2 so a reader knows which AAD shape to rebuild. The identity store's `FORMAT` does not change.

## 7. Open

1. Whether `KeyRefKind::Item` should carry the item id (as proposed, mirroring D20) or a separate Item Key id. Lockdown uses the item id; it keeps the item's own envelope self-describing.
2. Whether `Purpose::Share` should be the Vault Key's purpose when wrapped to a member's receiving key, or `Purpose::Vault` with `wrapped_by: peer`. Lockdown leans to the second: the purpose says what the key is, `wrapped_by` says who opens it.

## 8. A second proposal: the record set, revocation, and the ISK signer (from slice 3)

_Added 2026-10-17 by the session that built the recovery kit and the cross-machine identity sync (`07-workspace.md` §4 steps 6 and 7; D70–D74). As in §1–§7, nothing here is implemented in the crate; Lockdown runs on its own types until the owner accepts, amends or declines. These are additive and independent of §2–§6._

### 8.1 A signed, server-orderable record set

Lockdown needs the identity's records to travel between machines as one signed, append-only unit the server can order but never rewrite (D71). The crate's `Backup` is a flat snapshot with no sequence, no `prev_hash` and no signature over the set as a whole, so two machines' backups can't be told apart by age and a hostile server could mix them. Lockdown built `lockdown_vault::identity_set` on top of the crate's records: an `IdentityManifest { v, profile_id, seq, prev_hash, entries: [{path, hash}], revocations, written_by, written_at, sig }`, signed by the ISK and hash-chained, listing each record by its store path and BLAKE3 hash.

**Ask:** the crate owns this, so every ecosystem app syncs records the same way. The natural home is a `manifest`-shaped record the crate already signs with the ISK, extended with the per-device revocation boundary below, plus a builder (`Identity::record_set(prev, revocations)`) and a verifier (`RecordSet::verify(expected_isk)`) that reuse the crate's own record verification. Until then Lockdown's `identity_set` is the reference.

### 8.2 Device revocation with a per-vault boundary

The crate's `Device` already carries `revoked_at`, but nothing says *where* a revoked device's writes stop, and `device_keys` trusts every device's key revoked or not (07-workspace §5 q.6, now closed locally). Lockdown added, outside the crate, a `Revocation { device_id, vaults: [{vault_id, seq}] }` carried by the identity manifest: the newest head `seq` that device may have signed in each vault. The rule is monotonic — a revocation never comes back, a boundary never moves (D73).

**Ask:** a `Device` method or a manifest field for the boundary, and a crate rule that a revocation is append-only, so `device_keys` can answer "trusted, and up to which head" rather than a bare key. Lockdown's `check_revocations` and `Vault::revoke_device` are the reference.

### 8.3 A signer the app can borrow without re-deriving the ISK

Revoking, renaming and signing the manifest need the ISK, which the crate derives from the IRK (`signing_key`) but exposes only as a free function over an `Irk` the app has to unwrap itself (D72). Lockdown does exactly that — keystore → device wrap → `signing_key` → check against `isk_pub` — but it means the app touches the root key for an ordinary record edit.

**Ask:** record-editing calls on `Identity` that sign with the ISK internally and never hand it out: `Identity::revoke_device(device_id)`, `Identity::rename_device(device_id, name)`, and the record-set builder of §8.1. Then the app never re-derives a signing key, and §4's "these envelopes may be ISK- **or** device-signed" question stays the only place the app signs at all.

### 8.4 A structured machine label on `Device`

D68/D74 name a device `Lockdown on <hostname>` by convention in the `name` field. A first-class optional field (`Device.machine: Option<String>`, the hostname alone) would let every app show "<app> on <machine>" without parsing a display string, and let a future UI group an identity's devices by machine.

**Ask:** an additive `Device.machine` with the usual `skip_serializing_if`, set at enrolment. Until then `name` carries both.
