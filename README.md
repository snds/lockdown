# Lockdown

**Every way you sign in. Locked down, even from us.**

Lockdown is a cross-platform password manager, passkey provider and authenticator: passwords, passkeys, one-time codes, and TV or console activation codes, on iOS, Android, Mac, Windows, Linux, every browser, and a home server if you want one. It aims to meet 1Password, Bitwarden, Proton Pass, Vaultwarden and Apple Passwords on every feature and beat them on trust.

Three promises, from [the big picture](docs/plan/00-big-picture.md):

1. **Every way you sign in, in one place.** The breadth you feel in week one.
2. **It can't betray you.** Designed so a hostile server cannot read, swap or roll back a vault, and you can check. Hybrid post-quantum key wrapping from day one; a signed, verifiable history of every vault.
3. **Open, fair, yours.** Open-source apps, a self-hosted server that runs the same code as the cloud, free export forever.

Lockdown is one of three apps in an ecosystem (with Mailbird and Synapse) that each work alone and work better together. They share one identity through the [`ecosystem-identity`](https://github.com/snds/ecosystem-identity) crate.

## Status

**Plan set v0.1, 61 decisions, third Rust slice built (2026-10-17).** The workspace builds, lints clean and passes its tests on stable Rust. A vault round-trips through a plain SQLite file that holds ciphertext only, opens from the shared ecosystem identity and this device's keystore with no kit and no master password, and syncs: the `lockdown` command line registers an account with OPAQUE, a second device enrols through it, and the two push, pull and merge against the one server binary, with conflict copies where a field-level merge cannot decide and "Rollback attempt blocked" when the server falls behind. The matcher crosses into the browser over wasm with signed data and a Chromium-driven red team. Nothing ships yet. What is real and what is a skeleton is listed in [`docs/plan/07-workspace.md`](docs/plan/07-workspace.md).

| Real (with tests) | Skeleton (types, traits, one real rule each) |
|---|---|
| `lockdown-crypto` · `lockdown-matcher` · `lockdown-codes` · `lockdown-vault` · `lockdown-cxf` · `lockdown-store` · `lockdown-sync` · `lockdown-wasm` · `lockdown-cli` · `lockdown-server` | `lockdown-policy` · `lockdown-approval` · `lockdown-bridge-host` · `lockdown-ffi` |

## How the workspace maps to the plan

[`docs/plan/03-architecture.md`](docs/plan/03-architecture.md) §2 names the crates; this repository has one directory per row.

```
crates/
  crypto        AEAD, Argon2id floors, HKDF, Ed25519, X-Wing, BLAKE3, CSPRNG,
                OPAQUE login as a pure state machine                            (04-security §3–§5)
  vault         items, fields, versions, crown-jewel class, manifest chain,
                load/save through a store, the identity adoption path          (02-model, 01-identity §6–§8)
  store         SQLite + in-memory stores (ciphertext only), rollback memory,
                devices learned from the identity's signed records             (03-architecture §2, §11)
  sync          the wire protocol, the HTTPS transport, pull/merge/push/head,
                conflict copies, rollback and fork checks                       (03-architecture §6)
  matcher       URL rules, Public Suffix List, equivalent domains, lookalikes   (02-model §10)
  codes         TOTP, HOTP, 8-digit, Steam 5-character, otpauth:// / steam://   (02-model §4)
  cxf           FIDO CXF v1.0 read/write, CSV                                   (02-model §11)
  policy        policy evaluation order, crown-jewel hard rules                 (02-model §3)
  approval      proximity gate, number matching, rate limit                     (brief §2.7, §2.9)
  bridge-host   bridge manifests, tiers, consent gate, kill switch              (brief §2.9, D6, D8)
  ffi           secret handles, redacted views, one-shot reveals                (03-architecture §3, §8)
  wasm          the matcher boundary and one-time codes over wasm-bindgen       (03-architecture §4, 09-extension-boundary)
bins/
  cli           `lockdown init | add login | edit | list | show | status | account register | account login | sync | kit create|verify | recover | device list|revoke | code | match`
  server        axum: OPAQUE login, sessions, the identity record set, vault heads and items, SQLite (03-architecture §5, 05-self-host)
extension/      the wasm build script, the JSON schema, the Playwright red team  (09-extension-boundary)
```

The identity crate is **consumed, not copied** (D12, D34): `[workspace.dependencies]` in `Cargo.toml` points at `snds/ecosystem-identity`, pinned by commit. `lockdown-crypto` re-exports its KDF floors, KEM and canonical CBOR rather than redefining them, so the two cannot drift.

## Build and test

Stable Rust (`rust-toolchain.toml`; MSRV 1.90) and network access to crates.io and GitHub for the identity crate.

```sh
cargo build                         # the default members: every pure-Rust crate
cargo test --workspace              # 132 tests across 14 crates
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo deny check                    # cargo install cargo-deny --locked
cargo run -p lockdown-cli -- code "otpauth://totp/Example:me?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ"
cargo run -p lockdown-cli -- --root /tmp/snds/identity init            # creates the identity and a vault
echo 'hunter2' | cargo run -p lockdown-cli -- --root /tmp/snds/identity add login --site https://example.com --username me --password-stdin
cargo run -p lockdown-cli -- --root /tmp/snds/identity list
cargo run -p lockdown-server -- --data /tmp/lockdown-server   # listens on 127.0.0.1:8787 (--listen, LOCKDOWN_LISTEN)
echo 'a long account password' | cargo run -p lockdown-cli -- --root /tmp/snds/identity account register --server http://127.0.0.1:8787 --login me@example.com --password-stdin   # prints the recovery kit: a Secret Key and a recovery code; keep the page
cargo run -p lockdown-cli -- --root /tmp/snds/identity sync
cargo run -p lockdown-cli -- --root /tmp/snds/identity device list
```

A second machine with nothing on it comes back with the recovery kit: `recover --server … --login … --password-stdin --kit-stdin`, giving the password, the Secret Key and the recovery code one per line; it enrols and pulls the vault. A machine that already holds the identity uses `account login` (with `--secret-key-stdin` on an account made before the kit existed). The Secret Key from the kit is needed beside the password to open the account on any new device. A lost device is cut off with `device revoke <id>`. The server is plain HTTP only on localhost; put TLS in front of it anywhere else (05-self-host §3).

CI (`.github/workflows/ci.yml`) runs the same four checks on Ubuntu and macOS, plus `cargo-deny`, plus the wasm build and the browser red team.

## Desktop shell

`apps/desktop` is the Tauri 2 window around the same core: React 19 and `@unicron/ui` on the web side, `lockdown-app` (`crates/app`) on the Rust side, one Tauri command per operation. It shares the CLI's data directory and device key, so the window and `lockdown` on one machine are one device. The vault opens through the device keystore (D11); the account password appears only to join or recover. Secrets reach the webview as bytes and only by field; **Copy** goes from the core to the clipboard without passing through the webview and clears itself after 30 seconds; the idle, hidden-window and sleep timers that lock the vault run in `lockdown-app`, not in the UI. Layout and colour are unicron defaults until the design brief ([`docs/plan/11-desktop-design-brief.md`](docs/plan/11-desktop-design-brief.md)) lands.

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
