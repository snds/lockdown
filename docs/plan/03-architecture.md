# System architecture — v0.1 (draft)

_Status: draft, 2026-10-15. Implements D2, D4, D7, D9, D10, D12, D14 (proposed). Builds on `01-identity.md` (identity crate, consumed as-is) and `02-model.md` (vaults, items, manifests). Evidence: research brief §4.9, §5; `reports/04-foundation.md` §1, §3, §4.7, §5._

## For future agent

- **TL;DR:** One Rust workspace holds every rule that matters: keys, items, sync, matching, codes, policy, approval. Thin shells sit on top: SwiftUI, Jetpack Compose, Tauri 2, an MV3 extension and a web vault. One Rust server binary serves cloud and self-host alike. Sync is server-authoritative per item with a signed manifest chain per vault. Every local channel checks who is calling. Secrets never leave Rust memory as strings. Web UI comes from the shared Unicron design system.
- **Read with:** `02-model.md` §8 and §12 (what sync carries), `01-identity.md` §5–§6 (on-disk layout and the handshake), brief §5.3 (the desktop shell decision).
- **Do not:** fork the identity crate (D12), put vault logic in a shell, or add a second server code path for self-host (D7).

## 1. The shape

```
                 ┌──────────────────────────── shells ─────────────────────────────┐
                 │ SwiftUI (iOS/macOS)   Compose (Android)   Tauri 2 (Mac/Win/Linux)│
                 │  + AutoFill ext        + AutofillService    + macOS AutoFill ext │
                 │  + conditional passkey + CredMan provider   + Windows COM plug-in│
                 │ MV3 extension (Chrome/Edge/Firefox) · Safari Web Extension       │
                 │ Web vault · CLI                                                  │
                 └───┬────────────────┬──────────────────┬───────────────┬──────────┘
                     │ UniFFI         │ UniFFI           │ wasm-bindgen  │ native
                 ┌───▼────────────────▼──────────────────▼───────────────▼──────────┐
                 │                     Rust core workspace                          │
                 │ crypto · identity* · vault · sync · matcher · codes · cxf        │
                 │ policy · approval · bridge-host · store · ffi · wasm             │
                 │                       (*shared crate, D12)                       │
                 └───────────────┬──────────────────────────────┬───────────────────┘
          local IPC (peer-authed)│                              │ HTTPS (hybrid TLS)
   ┌─────────────────────────────▼──────┐              ┌────────▼──────────────────────┐
   │ Mailbird · Synapse · CLI · ext host│              │ Lockdown server (one binary)  │
   │ 01-identity §6 handshake           │              │ axum + sqlx · SQLite/Postgres │
   └────────────────────────────────────┘              │ S3 blobs · KT log · checkpoints│
                                                       │ push relay · OPAQUE           │
                                                       └───────────────────────────────┘
```

Three rules shape everything below. The core decides, shells render. The server stores and orders, never reads. Any peer that touches a secret proves who it is first (D4).

## 2. The Rust core workspace

| Crate | One-line job | Depends on |
|---|---|---|
| `crypto` | AEAD (XChaCha20-Poly1305), Argon2id with the signed floor, HPKE with hybrid X25519 + ML-KEM-768, Ed25519 with an ML-DSA slot, OPAQUE client via `opaque-ke`, BLAKE3, a CSPRNG-only generator with statistical tests in CI | none |
| `identity` | **The shared ecosystem crate** (`01-identity.md`). Profile, personas, devices, key envelopes, recovery kit, detection and attach. Built in Mailbird, extracted, consumed here as a dependency, never forked (D12). | `crypto` |
| `vault` | Items, fields, attachments, versions, trash, the manifest chain, crown-jewel rules (`02-model.md`) | `identity`, `crypto` |
| `store` | Local encrypted SQLite (via `sqlx` or `rusqlite` **[uncertain]**) holding ciphertext, the search index, usage stats; keystore-backed unlock slots | `vault` |
| `sync` | Revisions, optimistic concurrency, conflict copies, manifest verification, key-transparency checks, offline queue (§6) | `vault`, `store` |
| `matcher` | URL match rules, Public Suffix List, curated equivalent domains, lookalike detection, related origins | none |
| `codes` | TOTP, HOTP, 8-digit, Steam 5-character; clock-skew correction; `otpauth://` and `steam://` parsing | `crypto` |
| `cxf` | FIDO Credential Exchange Format read and write; the Lockdown archive; CSV; the other-manager importers, each fuzzed and size-capped | `vault` |
| `policy` | Organization and personal policies (25+), evaluated the same way on every client; export, Send, sharing and agent rules | `vault` |
| `approval` | The one approval service (brief §2.9 point 4): proximity gate, number matching, context, rate limits, "this wasn't me" | `policy` |
| `bridge-host` | Loads signed first-party bridge manifests, enforces tiers and consent gates, holds the kill switch; empty in v1 except device-code (D8) | `approval`, `policy` |
| `ffi` | The UniFFI facade: opaque handles, redacted views, one-shot reveals | all of the above |
| `wasm` | The `wasm-bindgen` facade with the same surface as `ffi` | all of the above |
| `cli` | Binary. The command-line tool and SSH agent | `ffi` surface, direct |
| `server` | Binary. §5 | `crypto`, `identity` (records only), `vault` (manifest verification only) |

Guardrails: `cargo-vet` and `cargo-deny` on every dependency, fuzzing on every parser, reproducible builds, SBOM and SLSA provenance, signed releases (brief §4.9, foundation §4.8).

## 3. Bindings

| Binding | Consumers | Shape of the surface |
|---|---|---|
| **UniFFI** → Swift, Kotlin | iOS, macOS, Android shells and their extensions | Async functions, opaque `SecretHandle` objects, callback interfaces for keystore and biometrics. No secret ever crosses as a `String`. |
| **wasm-bindgen** → TypeScript | MV3 extension, Safari Web Extension, web vault, Tauri's web view | Same surface. Secrets stay in WASM linear memory. The JS side gets `Uint8Array` views that are zeroed on release. |
| **Native Rust** | CLI, server, Tauri's Rust side | Direct crate use. Tauri commands call the core in-process, so the web view never holds a key. |

The three facades are generated from one interface definition, so a feature exists on every platform or on none (D9).

## 4. Shells

| Shell | Stack | Native pieces it must own | Gets the core through |
|---|---|---|---|
| **iOS / iPadOS / macOS app** | SwiftUI | AutoFill credential-provider extension (passwords, passkeys, one-time codes, identity store so suggestions appear without launching); **conditional passkey registration** (iOS 18, macOS 15); `ASCredentialExportManager` / `ASCredentialImportManager` for CXF (OS 26); Secure Enclave slots; App Group for `01-identity` §5; Safari Web Extension container; the macOS Accessibility-based universal fill | UniFFI, in-process in both app and extension (extension memory is tight, so the core streams) |
| **Android app** | Jetpack Compose | `AutofillService` with inline keyboard suggestions; Credential Manager provider for passkeys and passwords; `providerevents` import and export (alpha **[uncertain]**); StrongBox slots; Chrome 135 "Autofill using another service" onboarding; `ContentProvider` with signature permission for `01-identity` §5 | UniFFI |
| **Desktop app** (Mac, Windows, Linux) | Tauri 2, web UI from Unicron (§9) | macOS AutoFill extension (Swift, shared with iOS); **Windows passkey COM plug-in** (`IPluginAuthenticator`, Windows Hello user verification) packaged as **MSIX**; Windows auto-type with signed-process check; Linux Secret Service or TPM2-sealed slot; the local IPC broker (§7); global quick-access palette | native Rust in the Tauri process; the web view talks to it over Tauri commands |
| **Browser extension** | MV3 TypeScript on Chrome, Edge, Firefox; Safari Web Extension on Apple | Content scripts in the isolated world; fill menu in an extension-origin frame in the top layer; visibility checks before any fill (brief §4.9); Signal API handling with soft-delete; WebAuthn override as a fallback on Linux and ChromeOS only | wasm core, in two modes (below) |
| **Web vault** | Same web UI, served by the server | Nothing native | wasm core in the page; Argon2id runs in a worker |
| **CLI** | Rust | SSH agent socket, `age` plug-in later | native |

**Extension modes.** The MV3 service worker holds no long-lived secrets (foundation §3.1). When the desktop app is installed, the extension is a thin client: it asks the app over native messaging, the app does the matching and decryption, and the extension receives only the fields to fill. When no desktop app is present, the extension runs the wasm core in an offscreen document, unlocks with the password or a PRF passkey, keeps keys in WASM memory, and wipes them on lock or idle. The user sees the same UI either way.

**Passkey provider path by platform.** OS provider first: the AutoFill extension on Apple, Credential Manager on Android, the COM plug-in on Windows. The extension override is the fallback for Linux and ChromeOS, because Chrome and Firefox have no provider API and overrides from different managers collide (foundation §1.5).

## 5. The server

One Rust binary, one container image, the same code for cloud and self-host (D7). Features differ by configuration, never by fork, so a security fix ships to both on the same day.

| Component | Job | Storage |
|---|---|---|
| **API** (axum) | Items, manifests, vault membership, devices, personas, Sends, policies | `sqlx` against **SQLite** (default) or **Postgres** |
| **Auth** | OPAQUE server (`opaque-ke`), device sessions, rate limits, new-device approval relay | database |
| **Blobs** | Attachment chunks and Send files, named by ciphertext hash | S3-compatible (MinIO for self-host, any S3 in cloud) or local disk for a single-user install |
| **Key-transparency log** | Append-only log of every public key: identity signing keys, hybrid recipient keys, device keys, Argon2id parameter records; signed checkpoints published for outside auditors | database plus a published checkpoint feed |
| **Anti-rollback checkpoints** | Stores each vault's manifest head hash in the same log, so a client can prove the server showed everyone the same head | database |
| **Push relay** | Content-free "sync now" pings to APNs and FCM, signed with Lockdown's publisher keys. Self-hosted servers connect outbound to the cloud relay with a per-install token, or turn push off and poll. | none |
| **Icon proxy** | Domain in, icon out, no identity, no logs (`02-model.md` §9); self-hosters can point clients at their own | cache only |

What the server never has: a vault key, an item key, a password, a plain-text title or URL, or a device signing key. It orders writes and keeps receipts. The ETH threat model (brief §4.1) assumes it is hostile, and every check in `sync` is written for that case.

## 6. Sync design

- **Unit of sync is the item.** Each item carries a server-assigned `rev`. A write sends the `rev` it last saw. If the server's `rev` moved, the write is refused with the current item (optimistic concurrency, foundation §5).
- **Merge, then conflict copy.** On refusal the client decrypts both versions and merges field by field. If the two edits touched different fields, the merge is automatic and signed as a new version. If they touched the same field, the client keeps both as a "conflict copy" and asks the user. Nothing is overwritten silently.
- **Manifest chain is the vault's truth.** Every push that changes a vault ends with a new manifest head (`02-model.md` §8) signed by the device. The server accepts a head only if `prev_hash` matches its current head and `seq` is one higher. Two offline devices that both extend `seq` 100 race; the loser re-applies its item writes on top of `seq` 101 and signs `seq` 102. The server cannot forge a head because it holds no device key.
- **Rollback check on every pull.** The client verifies the chain from its last accepted head to the new one, checks the checkpoint inclusion proof, and refuses anything older than what it has seen. The message is loud: "Rollback attempt blocked."
- **Key transparency before every wrap.** No vault key is wrapped to a public key the client has not seen in the log, with a consistent checkpoint, since the last sync.
- **Membership is server-ordered, never merged.** Adding or removing a member is a manifest change; the manifest cannot be merged, only extended. Removal rotates the vault key on the client and the server enforces that `vault_key_ref` only moves forward.
- **CRDTs only inside rich-note fields.** A shared Markdown note merges with an end-to-end encrypted CRDT (Automerge-style) so two people can edit offline without conflict copies. Membership, keys and every other field stay revision-based, because CRDTs make revocation and auditing hard (foundation §5). This ships later; the field kind is reserved now.
- **Offline first.** Every shell works fully offline against `store`. The queue replays in order on reconnect.

## 7. Local IPC and peer authentication

Every local channel verifies its peer before any secret moves (D4, brief §4.9). Three kinds of peer share one broker in the desktop app:

| Peer | Channel | How the peer is verified |
|---|---|---|
| Browser extension | Native messaging host (a small Rust binary the browser launches) → the app over the local socket | The app checks the host binary's code signature and the browser that launched it against the manifest's allowed extension ids |
| CLI and SSH agent | The same local socket | Process identity, then a per-session pairing code on first use |
| Ecosystem apps (Mailbird, Synapse) | The `01-identity` §6 handshake: XPC on Apple, bound service on Android, named pipe with ACL on Windows, Unix socket on Linux | Code signature or process identity, both ways, before the IRK is wrapped to the new app |

Per platform, the check is:

| Platform | Mechanism |
|---|---|
| macOS / iOS | audit token of the connecting process → `SecCode` → verify the signing identity and team against an allow-list |
| Windows | `GetNamedPipeClientProcessId` → open the process → verify the Authenticode signature and, where packaged, the MSIX package family |
| Linux | `SO_PEERCRED` → `/proc/<pid>/exe` → compare path and binary hash against the installed manifest. Weakest of the four; the pairing code carries more weight here **[uncertain]** whether to require it on every connect |
| Android | Binder calling UID → package signature must match the ecosystem signing key |

Shared grants are scoped capabilities ("fill login for mail account Y"), signed, time-boxed and revocable. Master keys never cross a local channel.

## 8. Memory hygiene

| Rule | How |
|---|---|
| Secrets live in Rust | Shells hold `SecretHandle`s, not values. The UI asks for a redacted view, or a one-shot reveal that the core zeroes afterwards. |
| Zero on drop | `zeroize` and `secrecy` on every key and plain-text field type. |
| Stay out of swap | `mlock` on key pages where the OS allows; core dumps disabled; the process opts out of crash reporters for the secret heap. |
| No strings | No secret passes through a JavaScript, Swift `String`, or JVM `String`. Bindings move `Uint8Array` or `ByteArray` buffers that the core wipes. |
| WASM | Keys sit in linear memory; lock wipes it; the offscreen document is torn down, not kept warm. |
| Lock on sleep and idle | Configurable timeouts, enforced by the core, not the shell. The PIN is a real key slot, never a screen lock. |
| Clipboard and screen | Clipboard timer with a clear; mask fields during agent sessions; screenshot protection where the OS offers it. |

## 9. Design system: Unicron

Every web-rendered surface (Tauri desktop, extension, Safari Web Extension, web vault, Send pages) is built on `@unicron/ui` and `@unicron/tokens` from `github.com/snds/unicron`. Unicron is the shared design system of D14 (React 19, Vite, Tailwind v4, shadcn/ui on Base UI, Material Symbols). D14 is **proposed** and conflicts with Synapse D87, so until Sean confirms it, Lockdown treats Unicron as a dependency it consumes and does not fork **[uncertain]**.

Tokens flow one way: Figma variables → DTCG JSON in `@unicron/tokens` → generated CSS, Swift and Kotlin (brief §5.7). The native shells consume the generated Swift and Kotlin; nobody hand-edits platform files. Behaviour follows each platform; brand lives in the tokens.

The six security components from brief §5.7 are **Unicron deliverables**, built once in Unicron with one written spec each, then mirrored in SwiftUI and Compose:

| Component | Job |
|---|---|
| Approval sheet | Number matching, context (service, account, device, place), "this wasn't me" |
| Risk-tier badge | Standards / official / tolerated, drawn from the bridge manifest |
| Secret field | Masked, reveal on tap, copy with a clipboard timer |
| Code display | Countdown, colour and haptic cue, next code early; 6-digit, 8-digit and Steam 5-character |
| TV large-type view | Groups characters in threes; digits and letters in different colours |
| Crown-jewel styling | A treatment web content cannot imitate, used only inside the app or OS surfaces |

Clickjacking is a design constraint: in-page menus are always opaque, have a minimum size, and sit in the top layer. Approvals never happen inside a web page.

## 10. Tradeoffs taken

| Choice | Took | Over | Why, and what it costs |
|---|---|---|---|
| Desktop shell | **Tauri 2** | Electron | Smaller download, smaller attack surface, Rust-native calls into the core, a tiny npm tree. Cost: three web engines to test (WKWebView, WebView2, WebKitGTK), and WebKitGTK is the weakest. Mitigation: screenshot tests per engine, Unicron components that avoid engine-specific CSS, and an early MSIX spike because whether Tauri produces MSIX cleanly is **[uncertain]** (brief §5.3). |
| Server database default | **SQLite** | Postgres | One file, trivial backups, matches the single-binary self-host story (D7). Postgres stays a first-class option through `sqlx` for the cloud and for large organizations. Cost: two SQL dialects in tests, and no dialect-specific features. |
| Mobile sharing | **UniFFI** from the Rust core | Kotlin Multiplatform | The core already shares the logic, and crypto must not be reimplemented (brief §5.4). Cost: UniFFI's generated API is plainer than hand-written Swift and Kotlin, so each shell keeps a thin idiomatic layer on top. |
| Extension passkeys | **OS provider first, override as fallback** | Override everywhere | Overrides collide between managers and break under CSP. Cost: Linux and ChromeOS users get the fallback and must be told plainly. |
| Sync | **Revisions plus a manifest chain** | CRDT for everything | Revocation and audit need a single order of events. Cost: conflict copies exist; CRDTs are limited to note bodies. |

## 11. Open questions

1. `store` implementation: SQLCipher-style whole-file encryption, or plain SQLite holding only ciphertext with the index built in memory? The second leaks less and is the current lean **[uncertain]**.
2. Whether Tauri's bundler produces a signed MSIX that the Windows passkey plug-in model accepts, or whether a separate packaging step is needed.
3. Android: a third-party provider in the cross-device (hybrid) passkey flow is not clearly documented. Test on Android 15 and 16 before promising it (foundation §1.3).
4. Push relay trust for self-hosters: a per-install token on Lockdown's relay, or an open relay spec others can run.
5. Linux peer authentication: require the pairing code on every connect, or trust path plus hash after the first pairing.
6. One server binary also hosting the icon proxy, or a separate small binary so a self-hoster can leave it out.
7. Unicron's own release cadence against Lockdown's, once D14 is confirmed or reversed.
8. Team size and timeline for this shape under D9; `06-roadmap.md` must re-estimate.
