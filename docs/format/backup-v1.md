# The Lockdown backup format, version 1

_Status: published with Lockdown 0.1, 2026-10-18. This page is the whole format: anyone may write a reader or a writer from it without asking. The reference reader and writer are `crates/cxf/src/archive.rs` (Apache-2.0); the test vectors are `crates/cxf/tests/fixtures/backup-v1.json` and `backup-v1.lockdown`, and `crates/cxf/tests/backup_format.rs` opens them with a decoder written from this page alone._

A Lockdown backup is one file. It is either **plain** (the document of §2, as JSON text) or **sealed** (the document encrypted under a password, §3). Lockdown's exports, its automatic backups and its restores all use this one format. The key words MUST, MUST NOT, SHOULD and MAY are as in RFC 2119.

## 1. In one paragraph

The document lists every item of a vault with every field (its id, kind, label, section and value), the item's earlier versions, and its attached files inline, plus a list of items that were deliberately left out and why. The sealed file is a short JSON header naming an Argon2id key derivation and an XChaCha20-Poly1305 cipher, then the document in 1 MiB chunks, each sealed in its place, so a reader detects a changed header, a wrong password, and a chunk changed, dropped, added or reordered.

## 2. The document

UTF-8 JSON, one object. Unknown members MUST be ignored by a reader of the same `v`; a writer MUST NOT add members with meaning without raising `v`.

| Member | Type | Meaning |
|---|---|---|
| `format` | string | always `"lockdown-backup"` |
| `v` | integer | `1`. A reader MUST refuse a higher value it doesn't know |
| `exported_at` | integer | seconds since the Unix epoch, UTC |
| `exporter` | string | the app and version that wrote it, in words |
| `items` | array of item | every item carried |
| `left_out` | array of left-out | items deliberately kept out (may be empty or absent) |
| `vaults` | array of vault, optional | the vaults the items came from, with their folders (§2.6) |

### 2.1 Item

| Member | Type | Meaning |
|---|---|---|
| `id` | string | the item's UUID (lowercase, hyphenated). Stable: restoring the same backup twice merges by it |
| `kind` | string | one of `login`, `passkey`, `code`, `wi-fi`, `ssh-key`, `api-credential`, `document`, `identity`, `card`, `bank-account`, `note`, `crypto-wallet`, `server`, `database`, `software-licence`, `membership`, `custom` |
| `class` | string | `standard` or `crown_jewel` (the stronger protection of `docs/plan/02-model.md` §3) |
| `state` | string | `live`, `archived` or `trashed` |
| `created_at`, `modified_at` | integer, optional | seconds since the epoch: the first and the newest version |
| `fields` | array of field | the item as it is now; the first is `title` |
| `history` | array of version | earlier versions, oldest first (may be empty) |
| `attachments` | array of attachment | the newest version's files (may be empty) |
| `vault` | string, optional | the id of the vault (in `vaults`) it came from |

A reader MUST refuse an item whose `kind`, `class` or `state` it doesn't know, and SHOULD carry on with the other items, saying which one it left out.

### 2.2 Field

| Member | Type | Meaning |
|---|---|---|
| `id` | string | the field's stable id. Template fields have fixed ids (`user`, `pw`, `url.0`, `code.0`, `card.number`, `wifi.ssid`, …; `crates/vault/src/template.rs`); fields a person added are `custom.<n>`; ids starting `lockdown.` are the app's own (below) |
| `kind` | string | `text`, `hidden`, `boolean`, `date`, `url`, `email`, `phone`, `address`, `totp`, `passkey`, `linked` or `file` |
| `label` | string | the label shown |
| `section` | string, optional | a heading the field is grouped under |
| `value` | value | below |

**Value.** An object with exactly one of these members (a reader MUST refuse a value with none or more than one):

| Member | Holds |
|---|---|
| `text` | a string: plain and hidden text, dates (`YYYY-MM-DD`, or `YYYY-MM` for a card's expiry), email, phone, a one-time code's set-up as an `otpauth://` or `steam://` link |
| `flag` | `true` or `false` |
| `url` (with `matchRule`) | a web address and its match rule: `exact_host` (the default when absent), `base_domain`, `equivalent`, `starts_with` or `never` |
| `bytes` | base64url without padding: passkey material (`passkey.<n>.credential_id`, `.user_handle`, and `.key`, the PKCS#8 private key) |
| `ref` | another item's UUID |

**Filing.** `org.folder` (`text`: the id of one of the vault's folders, §2.6, or empty), `org.tags` (`text`: one tag per line) and `org.favourite` (`flag`) say where the item is filed. A reader that restores into a vault without the named folder SHOULD file the item nowhere and say so.

**The app's own fields.** `lockdown.export` (`flag`): a crown jewel the person chose to include in exports and backups. `lockdown.security` (`text`, JSON): the security center's dismissals. A reader that is not Lockdown MAY ignore them; a writer MUST NOT set `lockdown.export` on an item the person didn't opt in.

### 2.3 Version

`{ "n": integer, "edited_at": integer, "fields": [field, …] }`: an earlier version's number in the item's history, when it was saved, and its fields. Versions whose keys were destroyed on purpose (crypto-shredding) are not in the file; the export says so.

### 2.4 Attachment

| Member | Type | Meaning |
|---|---|---|
| `name` | string | the file's name |
| `mime` | string | its media type |
| `size` | integer | its length in bytes |
| `blake3` | string | BLAKE3 of the bytes, 64 lowercase hex characters |
| `data` | string | the bytes, base64url without padding |

A reader MUST check `size` and `blake3` and refuse the item when either is wrong.

### 2.5 Left out

`{ "id", "title", "kind", "reason" }`. `reason` is `crown-jewel` for a crown jewel the person didn't include (decision D121: crown jewels leave a bulk export only when opted in, per item); other reasons are words. A reader SHOULD tell the person what was left out.

### 2.6 Vault

`{ "id", "name" (optional), "folders": [ { "id", "name", "parent" (optional) } ] }`: a vault's id, the name the person gave it, and its folders by id, each under its `parent` folder or at the top. A restore SHOULD bring the folders back with their ids so the items' `org.folder` still names them.

## 3. The sealed file

```
offset  size  content
0       8     "LDBACKUP" (4C 44 42 41 43 4B 55 50)
8       1     sealed-file version: 1
9       2     header length N, big-endian
11      N     header: UTF-8 JSON (below)
11+N    …     chunks
```

### 3.1 Header

```json
{
  "v": 1,
  "kdf": { "alg": "argon2id", "m_kib": 65536, "t": 3, "p": 4, "salt": "<base64url, 16–64 bytes>" },
  "aead": { "alg": "xchacha20poly1305", "chunk": 1048576, "nonce_prefix": "<base64url, 19 bytes>" },
  "payload": "lockdown-backup+json",
  "created_at": 1760000000
}
```

N MUST be at most 4096. A reader MUST refuse any `v`, `alg`, `chunk` or `payload` other than these. The header is not secret; it is bound to every chunk (§3.3).

### 3.2 The key

1. `stretched` = Argon2id (version 0x13) of the password's UTF-8 bytes with `salt`, `m_kib` KiB of memory, `t` passes, `p` lanes, 32 bytes out.
2. `key` = HKDF-SHA-512 with no salt, input `stretched`, info `lockdown/backup/v1/file-key`, 32 bytes out.

Writers MUST use at least 64 MiB, 3 passes and 4 lanes (Lockdown's desktop floor) and a fresh random salt of at least 16 bytes. Readers MUST refuse less than 32 MiB, 3 passes or 1 lane, and SHOULD refuse more than 1 GiB, 16 passes or 16 lanes, so a hostile file can't stall them. (The test vectors use 256 KiB, 1 pass, 1 lane so the suite stays fast; a release build of Lockdown refuses them, as this rule says.)

An automatic backup keeps `key` sealed on the device that made it, so a scheduled run needs no password; any backup still opens anywhere with the password alone.

### 3.3 Chunks

The document's bytes are cut into chunks of `chunk` bytes; the last chunk holds the rest (an empty document is one empty chunk; a document of exactly k chunks has no extra empty chunk). Chunk `i` (from 0) is sealed with XChaCha20-Poly1305 under `key`, with:

- nonce = `nonce_prefix` (19 bytes) ‖ `i` as 4 bytes big-endian ‖ `01` for the last chunk, else `00` (24 bytes);
- associated data = every byte of the file before the first chunk (magic, version, length and header).

Each sealed chunk is its ciphertext followed by the 16-byte tag, so every frame but the last is `chunk + 16` bytes. A reader opens frames in order; a frame that fails is a wrong password when it is the first, else a changed file; a file that ends before a frame flagged last, or goes on after one, MUST be refused. Nothing from a file that failed MAY be used.

## 4. Test vectors

- `crates/cxf/tests/fixtures/backup-v1.json`: a plain document with one vault and one folder; a login filed in that folder with two tags and a star (password, code, passkey key, a base-domain website, one earlier version, one attachment); an opted-in crown-jewel wallet; an archived Wi-Fi network; a trashed note; and one left-out crown jewel.
- `crates/cxf/tests/fixtures/backup-v1.lockdown`: the same document sealed with the password `correct horse battery staple` at the test cost.

## 5. Changes

Version 1 is the first. A change a version-1 reader can't ignore raises `v` (the document) or the sealed-file version byte. Recipient (public-key) sealing, as `age` does it, is a planned addition and will take a new `kdf.alg` and a new version.
