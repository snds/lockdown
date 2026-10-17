# Lockdown

**Every way you sign in. Locked down, even from us.**

Lockdown is a cross-platform password manager, passkey provider and authenticator: passwords, passkeys, one-time codes, and TV or console activation codes, on iOS, Android, Mac, Windows, Linux, every browser, and a home server if you want one. It aims to meet 1Password, Bitwarden, Proton Pass, Vaultwarden and Apple Passwords on every feature and beat them on trust.

Three promises, from [the big picture](docs/plan/00-big-picture.md):

1. **Every way you sign in, in one place.** The breadth you feel in week one.
2. **It can't betray you.** Designed so a hostile server cannot read, swap or roll back a vault, and you can check. Hybrid post-quantum key wrapping from day one; a signed, verifiable history of every vault.
3. **Open, fair, yours.** Open-source apps, a self-hosted server that runs the same code as the cloud, free export forever.

Lockdown is one of three apps in an ecosystem (with Mailbird and Synapse) that each work alone and work better together. They share one identity through the [`ecosystem-identity`](https://github.com/snds/ecosystem-identity) crate.

## Status

**Plan set v0.1, 47 decisions, second Rust slice built (2026-10-17).** The workspace builds, lints clean and passes its tests on stable Rust. A vault round-trips through a plain SQLite file that holds ciphertext only, opens from the shared ecosystem identity and this device's keystore with no kit and no master password, and the `lockdown` command line proves it end to end. Nothing ships yet. What is real and what is a skeleton is listed in [`docs/plan/07-workspace.md`](docs/plan/07-workspace.md).

| Real (with tests) | Skeleton (types, traits, one real rule each) |
|---|---|
| `lockdown-crypto` · `lockdown-matcher` · `lockdown-codes` · `lockdown-vault` · `lockdown-cxf` · `lockdown-store` · `lockdown-cli` | `lockdown-policy` · `lockdown-approval` · `lockdown-bridge-host` · `lockdown-sync` · `lockdown-ffi` · `lockdown-wasm` · `lockdown-server` |

## How the workspace maps to the plan

[`docs/plan/03-architecture.md`](docs/plan/03-architecture.md) §2 names the crates; this repository has one directory per row.

```
crates/
  crypto        AEAD, Argon2id floors, HKDF, Ed25519, X-Wing, BLAKE3, CSPRNG   (04-security §3–§5)
  vault         items, fields, versions, crown-jewel class, manifest chain,
                load/save through a store, the identity adoption path          (02-model, 01-identity §6–§8)
  store         SQLite + in-memory stores (ciphertext only), rollback memory    (03-architecture §2, §11)
  sync          revisions, field merge, conflict copies, rollback guard         (03-architecture §6)
  matcher       URL rules, Public Suffix List, equivalent domains, lookalikes   (02-model §10)
  codes         TOTP, HOTP, 8-digit, Steam 5-character, otpauth:// / steam://   (02-model §4)
  cxf           FIDO CXF v1.0 read/write, CSV                                   (02-model §11)
  policy        policy evaluation order, crown-jewel hard rules                 (02-model §3)
  approval      proximity gate, number matching, rate limit                     (brief §2.7, §2.9)
  bridge-host   bridge manifests, tiers, consent gate, kill switch              (brief §2.9, D6, D8)
  ffi           secret handles, redacted views, one-shot reveals                (03-architecture §3, §8)
  wasm          the same surface for the extension and web vault               (03-architecture §4)
bins/
  cli           `lockdown init | add login | list | show | status | code | match`
  server        axum: /healthz and /.well-known/lockdown                        (03-architecture §5)
```

The identity crate is **consumed, not copied** (D12, D34): `[workspace.dependencies]` in `Cargo.toml` points at `snds/ecosystem-identity`, pinned by commit. `lockdown-crypto` re-exports its KDF floors, KEM and canonical CBOR rather than redefining them, so the two cannot drift.

## Build and test

Stable Rust (`rust-toolchain.toml`; MSRV 1.90) and network access to crates.io and GitHub for the identity crate.

```sh
cargo build                         # the default members: every pure-Rust crate
cargo test --workspace              # 81 tests across 14 crates
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo deny check                    # cargo install cargo-deny --locked
cargo run -p lockdown-cli -- code "otpauth://totp/Example:me?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ"
cargo run -p lockdown-cli -- --root /tmp/snds/identity init            # creates the identity and a vault
echo 'hunter2' | cargo run -p lockdown-cli -- --root /tmp/snds/identity add login --site https://example.com --username me --password-stdin
cargo run -p lockdown-cli -- --root /tmp/snds/identity list
cargo run -p lockdown-server        # listens on 127.0.0.1:8787 (LOCKDOWN_LISTEN to change)
```

CI (`.github/workflows/ci.yml`) runs the same four checks on Ubuntu and macOS, plus `cargo-deny`.

## Plan and research

Start with [docs/plan/00-big-picture.md](docs/plan/00-big-picture.md). The plan set (00–07 plus [`decisions.md`](docs/plan/decisions.md)) is the source of truth; the research in [`docs/research/2026-10-15/`](docs/research/2026-10-15/) is its evidence, beginning with [the brief](docs/research/2026-10-15/brief.md). Where the fact-check (`verification.json`) disagrees with a research report, the fact-check wins.

## Licence

Apache-2.0. See [LICENSE](LICENSE) and [NOTICE](NOTICE).
