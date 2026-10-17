# The Rust workspace — v0.1 (as built)

_Status: as built, 2026-10-17 (first slice 2026-10-16; store, identity adoption and CLI 2026-10-17). Implements D35 over the crate table in `03-architecture.md` §2. Companion to `02-model.md` (what `vault` holds), `04-security.md` §3–§5 (what `crypto` must be) and `01-identity.md` §10a–§10b (the crate we consume)._

## For future agent

- **TL;DR:** One Cargo workspace, fourteen crates, one directory per row of the architecture table. Seven crates are real with tests (`crypto`, `matcher`, `codes`, `vault`, `cxf`, `store`, `cli`); seven are compile-clean skeletons that each carry the public types the plan names and one real rule under test. A vault round-trips through a plain SQLite file holding ciphertext only, opens from the ecosystem identity root and this device's keystore with no kit and no password (D10, D11), and the `lockdown` binary proves it end to end from a shell. The identity crate is a git dependency pinned by commit. CI runs fmt, clippy with warnings denied, tests and cargo-deny on Ubuntu and macOS.
- **Read with:** `03-architecture.md` §2 (the table this follows), `02-model.md` §12 (the sketches `vault` encodes), `decisions.md` D34–D47, `08-identity-proposal.md` (what the identity crate is asked to grow).
- **Do not:** copy anything out of `ecosystem-identity` into `lockdown-crypto` (D12), put a `todo!()` anywhere (use `Error::Unsupported` with a message), or add a dependency that `deny.toml` would refuse.

## 1. The crate map, as built

| Crate | Package | Status | What is real | What is not yet |
|---|---|---|---|---|
| `crypto` | `lockdown-crypto` | **real** | AEAD seal/open with associated data (empty AAD refused); Argon2id through the identity crate's floors and `stretch`; HKDF-SHA-512; Ed25519; X-Wing encapsulate/decapsulate; BLAKE3 (plain, keyed, derive-key); CSPRNG; `Key32`/`SecretBytes` zeroizing types; `key32!` macro; `AccountAuth` re-exported with `OpaqueAccountAuth` answering `Unsupported` | The OPAQUE client (`opaque-ke`), the ML-DSA slot, the CI statistical tests on the generator |
| `vault` | `lockdown-vault` | **real** | `ItemKind` (18 kinds incl. the four named plain templates, `custom` and hidden `consent`), `ItemClass`, `ItemState`; `PlainField`/`SealedField` with the eight-field AAD; `WrappedKey` envelopes for Vault/Item/Version/Attachment keys; version history with per-version keys and device signatures that also cover the item header (`ItemHeader`: kind, schema, class, device-only); `ManifestHead` chain (signed, hash-linked, `verify_chain` with rollback refusal); crypto-shredding on purge and per version; crown-jewel default table and rules; the `Store` trait and `VaultRecord` (`vault::store`); `Vault::load` / `Vault::save` with the chain, rollback and item-versus-manifest checks; `Signer` (a bare key or the device keystore); `vault::identity` — the adoption path (`open_or_create_identity`, `default_persona`, `persona_keys`, `open_or_create_vault`, `open_vault`) | Attachment chunking, trash grace timers, members and sharing beyond the sealed list, the vault time machine, more than one vault per persona in the default path |
| `store` | `lockdown-store` | **real** | `SqliteStore` (rusqlite, bundled; WAL, `synchronous=FULL`, `secure_delete`; `schema_version` table; user-only file and directory; `save_all` in one transaction); `MemoryStore`; `HeadMemory` — the per-device memory of accepted identity and vault heads in its own file; the same test bodies against both backends (round trip with history, trash, purge and a crown-jewel item; a forged head; a tampered blob; a rolled-back file); the identity exit test (one run creates, a fresh run opens through the root) | Keystore-backed unlock slots, the search index, incremental item writes (every save rewrites the vault's items), compaction of purged ciphertext |
| `sync` | `lockdown-sync` | skeleton | `Revision`, `PushOutcome`, `ConflictCopy`; three-way `merge_fields` (different fields merge, same field conflicts); `RollbackGuard` | Transport, offline queue, key-transparency and checkpoint checks (need the server) |
| `matcher` | `lockdown-matcher` | **real** | `MatchRule` (exact_host default, base_domain, equivalent, starts_with, never); `MatchDecision { Exact, BaseDomain, Equivalent, Lookalike, NoMatch }`; Public Suffix List via `psl`; 16 curated equivalent-domain groups in `data/equivalent-domains.json`; lookalike detection (confusables, punycode, edit distance, hyphen decoys, leading-subdomain squats); related-origins cache with the five-label cap; `FillContext`/`fill_check` with the cross-origin-frame confirmation rule | The full Unicode confusables table, the signed icon pack, per-persona overrides |
| `codes` | `lockdown-codes` | **real** | HOTP and TOTP (SHA-1/256/512, 6–8 digits) against the RFC 4226/6238 vectors; Steam 5-character; skew window `verify`; `next_code_in`; `otpauth://` in three Steam spellings and `steam://` with base64→base32 conversion; `to_otpauth` | The "issuer equals sync login" warning (needs the account) |
| `cxf` | `lockdown-cxf` | **real** | CXF v1.0 model with `Credential::Other` passthrough; `parse`/`write` with size caps; CXF ↔ items for basic-auth, passkey, note, credit-card, totp, person-name + address; CSV reader (never panics, capped), injection-safe writer, login importer | Collections/folders mapping, the Lockdown archive, 1PUX/Bitwarden/KDBX importers, the OS exchange hooks |
| `policy` | `lockdown-policy` | skeleton | `Policy`, `Rule`, `Effect`, `PolicyDecision`; the evaluation order (hard rules → org deny → personal deny → step-up → allow) | The 25+ named organization policies |
| `approval` | `lockdown-approval` | skeleton | `ApprovalRequest` with `CodeSource` (the proximity gate), `NumberMatch`, `ApprovalContext`; `ApprovalService::evaluate`; `RateLimit`; `NotMeReport` | Prompt delivery, the activation-page allow-list, the server report |
| `bridge-host` | `lockdown-bridge-host` | skeleton | `BridgeTier { Standards, Official, Tolerated, Closed }`, `BridgeManifest`, `ConsentRecord`; `BridgeHost::load` refusing closed, unconsented and killed bridges; `kill` | Manifest signatures, the remote kill-switch feed, the device-code bridge itself |
| `ffi` | `lockdown-ffi` | skeleton | `SecretHandle`, `SecretRegistry` (redacted view, one-shot reveal, lock wipes) | UniFFI scaffolding and the generated Swift/Kotlin |
| `wasm` | `lockdown-wasm` | skeleton | `WasmSession` over the ffi surface | `wasm-bindgen` exports, the offscreen-document lifecycle |
| `cli` | `lockdown-cli` (binary `lockdown`) | **real** | `init [--root DIR] [--data DIR] [--persona ID\|LABEL]`, `add login --site URL --username U` (password from `--password-stdin` or `LOCKDOWN_PASSWORD`, never an argument), `list`, `show <id> [--reveal]`, `status` (root, profile, device, persona, vault head seq + hash); every run remembers the heads it accepted; `code <uri>`, `match <saved> <current>`; an end-to-end test drives the binary as separate processes on a temp root | Other item kinds, edit and trash, export, the SSH agent, local-socket pairing |
| `server` | `lockdown-server` | skeleton | axum `/healthz`, `/.well-known/lockdown` discovery document with feature flags all false | Everything in 03-architecture §5 |

`identity` from the table is not a directory here; see §2.

What the second slice settled (D39–D47; details in `decisions.md`):

- **The local store is a plain SQLite file holding ciphertext only** (03-architecture §11 q.1 decided). Four tables, every body canonical CBOR; the index lives in memory after unlock. What the file reveals: ids, sequence numbers, timestamps, sizes. What it never holds: a key, a title, a URL, a value.
- **The `Store` trait lives in `lockdown-vault`** (`vault::store`) and `lockdown-store` re-exports it, so `Vault::load(store, …)` and `Vault::save(store, adk)` take any backend without the vault crate depending on one.
- **Rollback memory is a separate file** (`HeadMemory`, `remembered.cbor`), never inside the vault file, so putting back an old copy of the vault file cannot put back the memory with it.
- **Load trusts nothing until it has checked the chain, the key and every item against the newest signed head.** The item header (kind, schema, class, device-only) is now under every version's signature and hash, because the load-time check showed a store could relabel an item otherwise.
- **Heads and versions are signed by the device keystore's key**, the one in the identity's `Device` record, through the `Signer` trait; other devices' heads verify against those records.
- **Lockdown's data lives beside the identity root** (`…/snds/lockdown` next to `…/snds/identity`): the device key file, the vault file, the memory file.

Two things the first slice changed in the model, both recorded in code comments:

- **Version signatures and hashes exclude the key envelopes** (`ItemVersion::without_keys`). Shredding a version destroys its wrapped key; if the signature covered the envelope, every purge would break history. The envelopes need no cover from the signature: each is sealed with associated data naming its place, and a swapped envelope yields a key that opens nothing.
- **The vault's key envelope is its own type** (`vault::keys::WrappedKey`), shaped like the identity crate's `KeyEnvelope` with the same refuse-by-place-before-decrypting rule, because the identity crate's `Purpose` and `KeyRefKind` stop at the App Data Key (02-model §13 q.2). The proposal to the crate owner is additive: `Purpose::{Vault, Item, Version, Attachment}` and `KeyRefKind::{Vault, Item, CrownJewel}`; when it lands, `keys.rs` becomes re-exports.

## 2. How identity is consumed

```toml
# Cargo.toml, [workspace.dependencies]
ecosystem-identity = { git = "https://github.com/snds/ecosystem-identity", rev = "96434d1de190164d7635302ce851fc0817000649", version = "0.1.0" }
```

- The crate was extracted from Mailbird `crates/identity` on 2026-10-16 (D34). It is pinned by **commit** because the `v0.1.0` tag push was being rejected by the git proxy when this was written; the line becomes `tag = "v0.1.0"` once the tag is on the remote. Mailbird's swap PR pins the same commit.
- `lockdown-crypto` depends on it directly and **re-exports** rather than mirrors: `kdf::{Argon2Params, FLOOR_DESKTOP, FLOOR_MOBILE, FLOOR_WASM, stretch, calibrate, hkdf_sha512}`, `kem::*`, `cbor::{to_canonical, from_slice}`, `auth::{AccountAuth, ExportKey, LocalStubAuth}`. The architecture table draws `identity → crypto`; as built it is `crypto → identity` until the crate exposes a primitives-only layer. Either way the constants have one home.
- `lockdown-vault` uses the identity crate's `Id`, `Timestamp`, `SymmetricKey`, `AppDataKey` and `CrownJewelKey` so a vault hangs off a persona's keys without conversion.
- Test speed: the identity crate's `insecure-test-kdf` feature lowers the Argon2id floor for debug tests. `lockdown-crypto` forwards it as its own feature of the same name, enabled only through its self dev-dependency. Never turn it on in an app.

## 3. CI matrix and guardrails

| Job | Where | Steps |
|---|---|---|
| `check` | `ubuntu-latest`, `macos-latest` | `cargo fmt --all --check` · `cargo clippy --workspace --all-targets -- -D warnings` · `cargo test --workspace` |
| `deny` | `ubuntu-latest` | `cargo deny check` (advisories, bans with `wildcards = "deny"`, licence allow-list, sources limited to crates.io and the identity repo) |

`RUSTFLAGS=-D warnings` is set for every step. `default-members` is every pure-Rust crate; `ffi` and `wasm` are built with the shells but their skeletons compile today, so `--workspace` covers all fourteen. Windows is not in the matrix yet: nothing platform-specific exists to test there, and the first Windows work (the COM passkey plug-in) needs an MSIX spike first (03-architecture §10).

Local numbers on 2026-10-17, stable 1.97: 81 tests, all green; clippy and fmt clean; `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok` (`inout` moved off a yanked release; `Zlib` allowed for `foldhash`, which `rusqlite` pulls in through `hashlink`).

## 4. The next implementation steps

1. ~~**`store` on SQLite.** Decide 03-architecture §11 q.1 (lean: plain SQLite holding ciphertext only, index in memory), implement `Store` over `rusqlite`, and add `Vault::load(store, keys)` so a vault round-trips through disk, not just memory. Exit: the vault tests pass against the SQLite store too.~~ **Done 2026-10-17** (commit "Store on SQLite"). Became: `SqliteStore` + `MemoryStore` behind `lockdown_vault::store::Store`; `Vault::load` / `Vault::save` / `VaultRecord`; `HeadMemory`; `Signer`; `ItemHeader` under the version signature; `crates/store/tests/backends.rs` runs one set of test bodies against both backends, including reopen-verifies-chain, tampered-blob and rolled-back-file. D39–D44.
2. **Attachments.** 1 MiB chunking under per-file Attachment Keys with the chunk index in the AAD, blob naming by ciphertext hash, and the padded-size rule (02-model §6). Exit: a 5 MiB file round-trips and a reordered chunk fails its tag.
3. ~~**Identity adoption path.** Create a persona's Lockdown App Data Key through `ecosystem_identity::Identity::wrap_app_key(persona, "lockdown", …)`, wrap the Vault Key under it with `Vault::wrap_vault_key`, and open a vault from an identity store on disk. Propose the additive `Purpose`/`KeyRefKind` variants to the crate owner at the same time. Exit: a vault created by one process opens in another through the identity root.~~ **Done 2026-10-17** (commits "Identity adoption path" and "CLI as the proof"). Became: `lockdown_vault::identity` (attach-or-create that never makes a second identity, the persona rule, App Data Key + Crown-Jewel Key per persona made once, `open_or_create_vault` / `open_vault`); `crates/store/tests/identity.rs` (a fresh run opens the first run's vault from the root, the keystore file and the store file alone); the `lockdown` binary with `init`, `add login`, `list`, `show`, `status` and `bins/cli/tests/end_to_end.rs` driving it as separate processes; `08-identity-proposal.md` for the crate owner. D45–D47.
4. **Server first endpoints.** OPAQUE registration and login with `opaque-ke` behind `OpaqueAccountAuth`, item push with revision checks (`PushOutcome`), and manifest head acceptance (`prev_hash` and `seq` rules). Exit: `sync` has a transport and its merge rule runs end to end against a local server.
5. **The extension's matcher boundary.** Expose `decide` and `fill_check` through `lockdown-wasm` with `wasm-bindgen`, ship the curated groups and the confusables table as signed data, and run the 500-site matching set from `06-roadmap.md` workstream 10 against it. Exit: the clickjacking red-team cases in `matcher::tests` become browser-driven tests.

## 5. Open questions raised by the build

1. Whether `lockdown-crypto` should keep depending on the whole identity crate (it pulls `schemars`, `serde_json`, `uuid`, `libc`) or the identity crate should split a `-primitives` crate the two can share. Harmless now; matters for the WASM binary size.
2. `Credential::Other` passthrough keeps unknown CXF credential types byte-for-byte on a CXF→CXF round trip, but a CXF→vault→CXF trip drops them with a report. Whether to store the raw JSON as a `custom` field so nothing is ever dropped.
3. The lookalike edit-distance rule (labels of six or more characters at distance one) will have false positives between unrelated real sites. Keep as a hard stop, or downgrade to a confirmation when the candidate has a valid certificate chain the extension can see.
4. `Vault::save` rewrites every item of the vault on every save (one transaction in SQLite). Fine for a CLI; a desktop vault with thousands of items wants dirty tracking per item. Also `trashed_at` is the one item field outside every signature; it only times the grace period, but the attachment step should fold it into the header or drop it from the item.
5. The identity crate lists app keys (`app_keys`) but not crown-jewel keys; Lockdown filters `envelopes()` by hand. Asked for in `08-identity-proposal.md` §5.
6. `device_keys` trusts every device in the identity's records, revoked or not, so history a device signed before revocation still verifies. Refusing a revoked device's *new* heads needs an order between devices only the server sees; it belongs to step 4.
7. Whether Lockdown's device keystore file should be the same file another ecosystem app on the device uses (one `Device` record per machine) or one per app, as now (one per app per machine). The identity crate's `FileKeystore` sits in "the app's data directory"; the handshake (§6 step 3) assumes separate devices per app. Worth settling before the Mailbird link.
