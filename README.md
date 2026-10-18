# Lockdown

**Every way you sign in. Locked down, even from us.**

Lockdown is a cross-platform password manager, passkey provider and authenticator: passwords, passkeys, one-time codes, and TV or console activation codes, on iOS, Android, Mac, Windows, Linux, every browser, and a home server if you want one. It aims to meet 1Password, Bitwarden, Proton Pass, Vaultwarden and Apple Passwords on every feature and beat them on trust.

Three promises, from [the big picture](docs/plan/00-big-picture.md):

1. **Every way you sign in, in one place.** The breadth you feel in week one.
2. **It can't betray you.** Designed so a hostile server cannot read, swap or roll back a vault, and you can check. Hybrid post-quantum key wrapping from day one; a signed, verifiable history of every vault.
3. **Open, fair, yours.** Open-source apps, a self-hosted server that runs the same code as the cloud, free export forever.

Lockdown is one of three apps in an ecosystem (with Mailbird and Synapse) that each work alone and work better together. They share one identity through the [`ecosystem-identity`](https://github.com/snds/ecosystem-identity) crate.

## Status

**157 decisions, fourteen build slices, no release yet (2026-10-18).** Of the 122 features committed to v1, 30 are done, 44 are partial, 38 are not started, and 10 are not code (audits, a licence call, a bug bounty). Every feature, with what is left and the evidence for its status, is in the **[v1 feature checklist](docs/plan/12-feature-status.md)**. The core, the desktop app, the CLI, the self-host server and a first browser extension (Chromium and Firefox, through the desktop app) exist; the web vault and the mobile apps do not yet.

| Slice | What it delivered |
|---|---|
| **0 · Plan** ✅ | Research across 9 managers (201 features), the plan set, the ecosystem identity design ([big picture](docs/plan/00-big-picture.md), [roadmap](docs/plan/06-roadmap.md)) |
| **1 · Crypto core** ✅ | Keys, sealed items with a signed history, the URL matcher, one-time codes, CXF import and export ([workspace](docs/plan/07-workspace.md), D35) |
| **2 · Local vault** ✅ | A SQLite file that holds ciphertext only, opened from the shared identity and the device keystore (D39–D48) |
| **3 · Account and sync** ✅ | OPAQUE sign-in, the self-host server, verifiable sync that blocks rollback and forks (D49–D61) |
| **4 · Extension boundary** ✅ | The matcher in the browser over wasm, signed site data, a clickjacking red team; not the extension itself ([boundary](docs/plan/09-extension-boundary.md), D62–D67) |
| **5 · Recovery and devices** ✅ | Recovery kit with the Secret Key, the identity record set, device revocation (D68–D74) |
| **6 · Attachments and key transparency** ✅ | Chunked encrypted files, a public append-only key log every sync checks (D75–D82) |
| **7 · Desktop shell** ✅ | Tauri app on unicron: unlock, list, detail, Login editor, sync ([shell](docs/plan/10-desktop-shell.md), D83–D91) |
| **8 · Desktop depth** ✅ | Settings, first run, Touch ID and Windows Hello approvals, Codes, Devices, Recovery, import and gated export ([brief](docs/plan/11-desktop-design-brief.md), D92–D104) |
| **9 · Security components** ✅ | Secret fields, code display, crown jewels, large type, approval sheet, risk badge; idle lock, trash restore, conflict resolve (D105–D109) |
| **10 · Every item kind** ✅ | Templates and one editor for every kind, the password generator, the ⌘K palette, lossless CXF export (D110–D115) |
| **11 · Strength and codes** ✅ | Password strength meter, HOTP next code, copy by reference, unicron 0.9 (D116–D119) |
| **12 · Security center** ✅ | Breached passwords by k-anonymity, weak and reused report, missing two-step sign-in, lookalike warnings, change-password links, playbooks, new-device alerts ([doc](docs/plan/13-security-center.md), D125–D134) |
| **13 · Organization** ✅ | Folders, tags, favourites, several vaults with moves (and into or out of crown jewels), archive, self-purging trash, duplicates, expiry reminders, several websites per login, attachments, Markdown notes, version history ([doc](docs/plan/15-organization.md), D145–D154) |
| **14 · Browser extension** ✅ | MV3 extension for Chromium and Firefox through the desktop app: native-messaging host, pairing with a code, inline menu in a closed shadow root, three checks per fill, save and update prompts, fill shortcut, code fill, Basic Auth, card and address fill, lookalike stop ([doc](docs/plan/16-browser-extension.md), D155–D169) |
| **Next** | Browser extension (fill, save, passkeys), mobile apps, sharing and family, breach monitoring, packaged and signed releases |

| Real (with tests) | Skeleton (types, traits, one real rule each) |
|---|---|
| `lockdown-crypto` · `lockdown-matcher` · `lockdown-codes` · `lockdown-vault` · `lockdown-cxf` · `lockdown-store` · `lockdown-sync` · `lockdown-wasm` · `lockdown-cli` · `lockdown-server` | `lockdown-policy` · `lockdown-approval` · `lockdown-bridge-host` · `lockdown-ffi` |

## How the workspace maps to the plan

[`docs/plan/03-architecture.md`](docs/plan/03-architecture.md) §2 names the crates; this repository has one directory per row.

```
crates/
  crypto        AEAD, Argon2id floors, HKDF, Ed25519, X-Wing, BLAKE3, CSPRNG,
                OPAQUE login as a pure state machine, the Merkle tree and proofs (04-security §3–§6)
  vault         items, fields, versions, attachments, crown-jewel class, manifest
                chain, load/save through a store, the identity adoption path,
                the log leaf, tree head, citation and checkpoint shapes         (02-model, 01-identity §6–§8)
  store         SQLite + in-memory stores (ciphertext only, blobs by hash),
                rollback memory, the synced identity records, the pinned log
                key and last tree head per server                               (03-architecture §2, §11)
  sync          the wire protocol, the HTTPS transport, pull/merge/push/head,
                blobs riding with items, conflict copies, rollback and fork
                checks, the key-transparency session and self-audit            (03-architecture §6, 04-security §6)
  matcher       URL rules, Public Suffix List, equivalent domains, lookalikes   (02-model §10)
  codes         TOTP, HOTP, 8-digit, Steam 5-character, otpauth:// / steam://   (02-model §4)
  cxf           FIDO CXF v1.0 read/write, CSV                                   (02-model §11)
  policy        policy evaluation order, crown-jewel hard rules                 (02-model §3)
  approval      proximity gate, number matching, rate limit                     (brief §2.7, §2.9)
  bridge-host   bridge manifests, tiers, consent gate, kill switch              (brief §2.9, D6, D8)
  ffi           secret handles, redacted views, one-shot reveals                (03-architecture §3, §8)
  wasm          the matcher boundary and one-time codes over wasm-bindgen       (03-architecture §4, 09-extension-boundary)
bins/
  cli           `lockdown init | add login | edit | list | show | status | attach | attachments | fetch | account register | account login | sync | audit | checkpoint | kit create|verify | recover | device list|revoke | code | match`
  server        axum: OPAQUE login, sessions, the identity record set, vault heads and items,
                content-addressed blobs, the key-transparency log, SQLite       (03-architecture §5, 05-self-host)
extension/      the wasm build script, the JSON schema, the Playwright red team  (09-extension-boundary)
```

The identity crate is **consumed, not copied** (D12, D34): `[workspace.dependencies]` in `Cargo.toml` points at `snds/ecosystem-identity`, pinned by commit. `lockdown-crypto` re-exports its KDF floors, KEM and canonical CBOR rather than redefining them, so the two cannot drift.

## Build and test

Stable Rust (`rust-toolchain.toml`; MSRV 1.90) and network access to crates.io and GitHub for the identity crate.

```sh
cargo build                         # the default members: every pure-Rust crate
cargo test --workspace              # 165 tests across the workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo deny check                    # cargo install cargo-deny --locked
cargo run -p lockdown-cli -- code "otpauth://totp/Example:me?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ"
cargo run -p lockdown-cli -- --root /tmp/snds/identity init            # creates the identity and a vault
echo 'hunter2' | cargo run -p lockdown-cli -- --root /tmp/snds/identity add login --site https://example.com --username me --password-stdin
cargo run -p lockdown-cli -- --root /tmp/snds/identity list
cargo run -p lockdown-server -- --data /tmp/lockdown-server   # listens on 127.0.0.1:8787 (--listen, LOCKDOWN_LISTEN)
echo 'a long account password' | cargo run -p lockdown-cli -- --root /tmp/snds/identity account register --server http://127.0.0.1:8787 --login me@example.com --password-stdin   # prints the recovery kit: a Secret Key and a recovery code; keep the page
cargo run -p lockdown-cli -- --root /tmp/snds/identity sync                  # checks the server's public log before and after
cargo run -p lockdown-cli -- --root /tmp/snds/identity device list
cargo run -p lockdown-cli -- --root /tmp/snds/identity attach <item-id> ./passport.pdf   # a file, sealed in chunks under its own key
cargo run -p lockdown-cli -- --root /tmp/snds/identity attachments <item-id>
cargo run -p lockdown-cli -- --root /tmp/snds/identity fetch <item-id> passport.pdf --out ./passport-copy.pdf
cargo run -p lockdown-cli -- --root /tmp/snds/identity audit                 # every log entry about this account, verified
cargo run -p lockdown-cli -- --root /tmp/snds/identity checkpoint            # a signed page to keep or verify on another device
```

A second machine with nothing on it comes back with the recovery kit: `recover --server … --login … --password-stdin --kit-stdin`, giving the password, the Secret Key and the recovery code one per line; it enrols and pulls the vault. A machine that already holds the identity uses `account login` (with `--secret-key-stdin` on an account made before the kit existed). The Secret Key from the kit is needed beside the password to open the account on any new device. A lost device is cut off with `device revoke <id>`. `checkpoint --verify FILE` on a second device says whether the two saw the same server. The server is plain HTTP only on localhost; put TLS in front of it anywhere else (05-self-host §3).

CI (`.github/workflows/ci.yml`) runs the same four checks on Ubuntu and macOS, plus `cargo-deny`, plus the wasm build and the browser red team.

## Desktop shell

`apps/desktop` is the Tauri 2 window around the same core: React 19 and `@unicron/ui` on the web side, `lockdown-app` (`crates/app`) on the Rust side, one Tauri command per operation. It shares the CLI's data directory and device key, so the window and `lockdown` on one machine are one device. The vault opens through the device keystore (D11); the account password appears only to join or recover. Secrets reach the webview as bytes and only by field; **Copy** goes from the core to the clipboard without passing through the webview and clears itself after 30 seconds; the idle, hidden-window and sleep timers that lock the vault run in `lockdown-app`, not in the UI. The dangerous parts of the UI (reveal, copy, a code, a crown jewel, large type, approvals, the risk tier) are unicron's six security components, bound to the core through one adapter module each; layout and colour are unicron defaults under the design brief ([`docs/plan/11-desktop-design-brief.md`](docs/plan/11-desktop-design-brief.md)). Every kind a person makes by hand (logins, codes, cards, notes, Wi-Fi, identities, documents, bank accounts, crypto wallets, SSH keys, API keys, servers, databases, licences, memberships, custom items) has one editor drawn from the core's per-kind templates, with a password generator in the core; ⌘K or Ctrl+K opens the quick-access palette.

```sh
cd apps/desktop && pnpm install
pnpm tauri dev                      # Linux needs WebKitGTK: libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libayatana-appindicator3-dev libdbus-1-dev
pnpm typecheck && pnpm lint && pnpm test && pnpm build && pnpm ds:ratchet
cargo clippy -p lockdown-desktop --features shell --all-targets -- -D warnings
```

The Tauri half sits behind the crate's `shell` feature, so the root `cargo … --workspace` gates stay pure Rust; `tauri dev`/`tauri build` turn it on through `tauri.conf.json`. Details, the command surface and the decisions proposed: [`docs/plan/10-desktop-shell.md`](docs/plan/10-desktop-shell.md).

## Plan and research

Start with [docs/plan/00-big-picture.md](docs/plan/00-big-picture.md). The plan set (00–07 plus [`decisions.md`](docs/plan/decisions.md)) is the source of truth; the research in [`docs/research/2026-10-15/`](docs/research/2026-10-15/) is its evidence, beginning with [the brief](docs/research/2026-10-15/brief.md). Where the fact-check (`verification.json`) disagrees with a research report, the fact-check wins.

## Licence

Apache-2.0. See [LICENSE](LICENSE) and [NOTICE](NOTICE).
