# Self-host server hardening and deployment: first slice

_Status: built, 2026-10-18, under D122 ("desktop and web first"). Implements the transport half of `03-architecture.md` §1 ("HTTPS (hybrid TLS)"), the single-binary and container rows of `05-self-host.md` §1, the backup primitive of §4, the "WebSocket on desktop" fallback of §5, and the session list the security center's `account-alerts` row asks for. Operator guide: [`docs/self-host.md`](../self-host.md)._

## For future agent

- **TL;DR:** Lockdown's own client and server now speak **TLS 1.3 with the hybrid X25519MLKEM768 key exchange first** (`crates/tls`, `lockdown-tls`: rustls on `ring`, with the ML-KEM-768 half from RustCrypto's `ml-kem`, already in the tree for the vault's X-Wing wraps). The server serves TLS itself (`--tls-cert`/`--tls-key`, reload on SIGHUP or file change, HSTS, an optional HTTP→HTTPS redirect) and refuses plain HTTP away from loopback unless `--insecure-http` says a proxy does TLS. Clients refuse plain HTTP to anything but this machine and can pin a private CA (`LOCKDOWN_CA_CERT`). A **live channel** (server-sent events at `/v1/events`) tells an account's other devices, and a shared vault's members, that something changed; the desktop syncs on it and redraws. Every token now has a **session row**, so Devices → **Signed in** lists where the account is signed in and ends any session on the server at once. A **Dockerfile** (distroless, non-root, read-only root, one volume) and `compose.yaml` ship; CI builds the image, never pushes it, and smoke-tests it with the real CLI. `lockdown-server backup` makes, verifies and restores an encrypted snapshot under a **Backup Key** the person holds. Attachment bodies can live as files (`--blob-dir`).
- **Read with:** `crates/tls/src/hybrid.rs` (the key exchange), `crates/tls/src/lib.rs` (configurations, reloading certificates), `bins/server/src/tls.rs`, `bins/server/src/live.rs`, `bins/server/src/sessions.rs`, `bins/server/src/backup.rs`, `bins/server/src/client_addr.rs`, `bins/server/src/main.rs` (every flag), `crates/sync/src/transport.rs` (the client side, the SSE reader), `crates/app/src/live.rs` (the desktop's channel and sessions), `apps/desktop/src/features/sessions/`, `Dockerfile`, `compose.yaml`, `bins/server/container/smoke.sh`.
- **Do not:** let anything but `lockdown-tls` build a rustls configuration for Lockdown's own server (the hybrid group and TLS 1.3 only live there); put content into a live event (a kind and a version, nothing else); treat an event as a fact (it only starts a sync that checks everything); drop a session row before its token has run out (the token would come back as one from before the table, §6); log a token, a key or a Backup Key; publish the image to a registry from CI.

## 1. What was built, per row

| Row | State | What is real | What is missing |
|---|---|---|---|
| `pq-transport` | done (Lockdown's client ↔ Lockdown's server) | X25519MLKEM768 (draft-ietf-tls-ecdhe-mlkem, code point `0x11EC`) as a rustls `SupportedKxGroup` (§2), first in every Lockdown client and in the server; TLS 1.3 only. The client also offers X25519 alone (its "hybrid component"), so a reverse proxy without the group still connects (the fallback 05-self-host §3 allows); the server prefers the hybrid and takes X25519 or P-256 from older clients, or nothing but the hybrid with `--tls-pq-only`. The breach-range check, the change-password probe and the server's breach feed get the same provider as the process default. Tests: both ends negotiate the hybrid in memory and over sockets; classical peers fall back; a hybrid-only server refuses a classical client; **OpenSSL 3.5 (through Node) as the server and as the client negotiates X25519MLKEM768 with Lockdown's ends** (required in CI's `tls-interop` job); the CLI end-to-end test runs register → sync → recover → sync against a hybrid-only server and the server counts only hybrid handshakes. | Browsers and other servers are reached on whatever they offer. Hybrid ML-DSA certificates are not a thing yet. |
| `single-container` | done (image built and smoke-tested in CI, not published) | `Dockerfile`: multi-stage, `--locked` release build, `gcr.io/distroless/cc-debian12:nonroot` (uid 65532, no shell), one volume `/data`, `tmpfs /tmp`, read-only root, `HEALTHCHECK` through `lockdown-server healthcheck`, sign-ups closed by default. `compose.yaml`: read-only, no capabilities, `no-new-privileges`, 256 MiB limit, certificate from `./tls`, attachments as files. CI's `container` job builds it (`push: false`) and runs `bins/server/container/smoke.sh`: health, HSTS, discovery, register, add, sync, a second device's recovery and sync over hybrid TLS with the real CLI, 40 more syncs, a backup made and verified inside the container, the root file system refusing a write. **Memory: 5.5 MiB idle, 6.2 MiB after the load** (§7). Attachments can be files (`--blob-dir`, §9). | Postgres and S3-compatible storage (§9), a published, signed, reproducible image with SBOM (04-security §15), arm64 builds, the Helm chart and the bundle's profiles (05-self-host §3). |
| `self-host-push` (desktop and web half) | partial | The live channel (§5): `GET /v1/events`, server-sent events, content-free `{v, kind}`. Every successful write under `/v1/vaults` and `/v1/identity` reaches the writer's other sessions and, for a shared vault, its owner and every active member; alerts and session changes too. `lockdown-sync` reads it (`HttpTransport::events`), `lockdown-app` keeps it open while unlocked and syncs on it (bursts folded, never counted as activity), the desktop redraws on `synced`. Tests: a write reaches the other device's stream and not the writer's; an ended session hears so and its stream closes; two apps, a change on one shows on the other with no Sync pressed. | The mobile push relay (D122; its shape is D229). The web vault subscribes when it exists (its page needs `fetch` streaming, since `EventSource` can't send the bearer header). Items shared on their own (`/v1/shares/items`) don't ping their recipient yet. |
| `account-alerts` | partial | Sessions (§6): one row per token, checked on every request, so ending one stops it at once. Devices → **Signed in** lists every live session with its device name, client, address, start and last use, this device marked; **End** and **End all other sessions** behind a confirm; the ended device is told on the live channel and forgets its token. The list refreshes itself on the channel. | OS notifications for alerts; email verification for new devices (no SMTP setting exists; adding one is the operator-settings work of 05-self-host §10). |
| `official-self-host` | partial | `docs/self-host.md`: install (container and binary), every setting, TLS (on the server or behind a proxy, private CAs), backup and restore, upgrades and the compatibility window, what the operator can see, health, measured memory. `lockdown-server backup keygen | create | verify | restore` (§8). The discovery document names the server's release (`version`) for the compatibility window. | The admin page (05-self-host §10), the restore drill on a schedule with "last successful restore" shown in the clients, cross-publishing checkpoints, the pre-migration snapshot (05-self-host §7), Postgres. |

## 2. Hybrid post-quantum TLS (`lockdown-tls`)

**The group.** `hybrid::X25519MLKEM768` follows the draft exactly: the client's key share is the ML-KEM-768 encapsulation key (1184 bytes) followed by an X25519 public key (32); the server's is the ML-KEM ciphertext (1088) followed by its X25519 key; the shared secret is the ML-KEM key followed by the X25519 output (64 bytes), which TLS 1.3's key schedule extracts from. The server runs FIPS 203's encapsulation-key check (the modulus check in `EncapsulationKey::new`) and refuses a share of the wrong length; decapsulation's implicit rejection turns a forged ciphertext into an unrelated key, which the Finished message then fails. The X25519 half is rustls's own `ring` group, so only the ML-KEM half is new code. Randomness is `ring`'s system source; every secret is zeroed after use.

**Why this path** (D215). Three ways were weighed:

| Path | For | Against |
|---|---|---|
| **rustls `ring` + this group over RustCrypto `ml-kem`** (taken) | `ring` stays the only C/assembly in the TLS stack; `ml-kem` 0.3.2 is already linked for the vault's X-Wing wraps (`x-wing` → `ml-kem`), so no new cryptography and no new duplicate crate joins the tree (`cargo deny check` passes, duplicates 54 before and after); pure Rust everywhere the apps build, including Android and wasm later | ~200 lines of our own glue in the handshake (an audit item); RustCrypto's ML-KEM is not FIPS-validated |
| rustls `aws-lc-rs` provider (`X25519MLKEM768` built in) | the group is rustls's own and AWS-LC's ML-KEM is FIPS-validated and formally verified in parts | a second C library beside `ring` (which stays: the passkey authenticator and tokens use it), its C/assembly build on every target, a larger binary; licences pass `deny.toml` (`ISC AND Apache-2.0 AND MIT AND BSD-3-Clause …` for `aws-lc-sys` 0.46) |
| `rustls-post-quantum` | one line to turn on | on `aws-lc-rs` too, with the same costs, and at `0.3.0-dev` |

Moving to `aws-lc-rs` later is a change inside `lockdown-tls::provider` and nothing else; the interop tests stay as they are.

**Configurations.** `client_config(trust)`: TLS 1.3 only, groups `[X25519MLKEM768, X25519, P-256]`, the platform verifier (`rustls-platform-verifier`, as reqwest uses) or, under `Trust::Only`, exactly the given certificates. `server_config(cert, groups)`: TLS 1.3 only, ALPN `http/1.1`, `PreferHybrid` or `HybridOnly`, the certificate from a `ReloadingCert`. A network attacker can't push two hybrid-capable ends down to X25519: TLS 1.3 signs the transcript, the group choice and any HelloRetryRequest included (D216).

## 3. TLS on the server

| Piece | Behaviour |
|---|---|
| Certificates | `--tls-cert` (chain, PEM) and `--tls-key` (PKCS#8, SEC1 or PKCS#1 PEM). The pair is checked at start (a key that doesn't match the certificate is refused). |
| Reload | On SIGHUP, and when either file's time or size changes (checked every `--tls-reload-secs`, default 60, 0 = SIGHUP only). A bad renewal is logged and the pair in use keeps serving. New handshakes get the new pair; open connections keep theirs. |
| Handshakes | Each on its own task with a 10 s limit and at most 512 at once, so a slow client holds up nobody; the group each used is counted (`TlsStats`, read by the tests). |
| HSTS | `Strict-Transport-Security: max-age=31536000` on every answer over TLS (`--hsts-max-age`, 0 = none); no `includeSubDomains` or `preload`, which are the domain owner's call. |
| Redirect | `--redirect-http ADDR` answers plain HTTP with `308` to `https://<host>:<tls port><path>`, taking only a plain host name from `Host` (anything else is a 400) and answering `/healthz` in place. |
| Plain HTTP | Without TLS the server listens on loopback only; on any other address it refuses to start unless `--insecure-http` (a TLS proxy in front). The start line says "PLAIN HTTP" in capitals. |
| Behind a proxy | `--trusted-proxy CIDR` (repeatable, `LOCKDOWN_TRUSTED_PROXIES` comma-separated): only a connection from such an address has its `X-Forwarded-For` read, from the right, skipping trusted hops, so the login brake (D57) and the session list see real clients and a client can't forge its own. |
| Health | `GET /healthz`; `lockdown-server healthcheck` for containers (liveness on this machine only, so it doesn't check the certificate's name). |
| Shutdown | Ctrl-C closes every live stream first, so a graceful shutdown doesn't wait on them for ever. |

ACME is not built in (not trivial: account keys, challenges, storage, rate limits); `certbot` or the proxy renews, and the reload picks it up.

## 4. Clients: trust and plain HTTP

`HttpTransport::new` (CLI, desktop, every app on the core) builds its TLS from `lockdown-tls`. Two rules (D217, D218):

- **`LOCKDOWN_CA_CERT=/path/ca.pem`** makes the client trust the certificates in that file **and nothing else**: a pin for a server on a private CA. A set variable naming a missing or empty file is an error, never a quiet fall back to the system store.
- **Plain `http://` only to this machine** (`localhost`, `127.0.0.0/8`, `::1`). A session token is a bearer credential; on a network it would be readable. Everything else must be `https://`.

`HttpTransport::with_trust` takes the trust explicitly (tests). The client's `User-Agent` is now `lockdown/<version> (<os>)`, which the session list shows.

## 5. Live sync

```
device A ──PUT /v1/vaults/…/heads──▶ server ──(layer: notify_writes)──▶ Notifier (broadcast)
                                                             │
          device B ◀── event: lockdown / data: {"v":1,"kind":"sync"} ──┘   (A's own stream: nothing)
          device B: sync (pull, verify manifests and the log, merge) ──▶ window redraws
```

- **Server** (`live.rs`). `GET /v1/events` with a session: `text/event-stream`, `event: lockdown`, JSON data `{v: 1, kind}` with `kind` one of `sync`, `alerts`, `sessions`, `session_ended`; a `: keep-alive` comment every 20 s; `X-Accel-Buffering: no` for nginx. A layer over the router announces every successful `PUT`/`POST`/`DELETE` under `/v1/vaults` and `/v1/identity` (and `alerts` under `/v1/account/alerts`) to the writer's account, minus the writing session, and for a vault named in the path that is shared, to its owner and every active member (`vault_audience`, from the sharing slice's tables). New-device alerts and session starts and ends are announced by their handlers. A stream re-checks its session every minute and ends when the session is ended (with `session_ended` first) or the token runs out. 16 streams per account, 10,000 in all (`429` beyond); a stream that falls behind gets one `sync`.
- **Client** (`lockdown-sync`). `HttpTransport::events(token)` opens it with a per-read timeout of 65 s (three keep-alives missed means a dead link); `EventStream::next_event`/`next_item` parse SSE with a 16 KiB cap per event, skipping other event names and data that doesn't parse; an unknown `kind` reads as `other` (forward compatible, D62).
- **App** (`lockdown-app::live`). `App::start_live(sink)` runs two threads: one keeps the channel open while the vault is unlocked and the account has a live token (looks every 3 s while locked; closes within a keep-alive of a lock; reconnects after 2 s, doubling to a minute); one waits for `sync` events, gathers a burst for 250 ms and syncs once through the worker **without touching the idle clock**, so a busy second device never keeps this one unlocked. `session_ended` makes the app forget that token (the next sync asks for a login) and not knock with it again.
- **Desktop.** The shell turns each notice into the window event `lockdown://live`; the webview refreshes its status (and so the list) on `synced`, says so on `session_ended` and `sync_failed`, and the Signed in section re-reads on `sessions`.

Nothing in an event is trusted. The worst a lying server or a forged event can do is cause a sync, which checks everything as **Sync now** does.

## 6. Sessions

- Every `lk1` token (D52) has a row keyed by its random nonce, which is the session's id on the wire: account, device, created, last used, expires, the client address (behind trusted proxies, the real one) and the client's `User-Agent` cut to 80 printable characters. The row is written at login; a token from before this table gets one the first time it is seen.
- Every authenticated request reads the row: an **ended** session is refused with `unauthenticated` ("this session was ended from one of your devices; log in again"). "Last used" and the address are written at most once a minute (or when the address changes). This amends D52's "checked with no database lookup": one primary-key read per request is what makes ending a session immediate (D220).
- `GET /v1/account/sessions` lists the account's live sessions (oldest first, the asking one marked); `POST /v1/account/sessions/{id}/end` ends one of the account's own (another account's id is "not found"); `POST /v1/account/sessions/end-others` ends all but the asking one. Ending is announced on the channel (`session_ended` to that stream, `sessions` to the account).
- Rows go a week after their token runs out, never before: a missing row would let the token back in as a pre-table one.
- Ending a session is not revoking the device: the device stays enrolled and can log in again with the password. The Devices screen keeps **Revoke** (behind the `Approve` gate, D101) for that, and the end dialog says so.

## 7. The container, and what it uses

| | |
|---|---|
| Image | `gcr.io/distroless/cc-debian12:nonroot` + the release binary: **53 MB** on disk, **14 MB** compressed |
| User | 65532:65532 |
| Writes | `/data` only (database, key file, attachment files, backups); `/tmp` as tmpfs for SQLite's temporary files |
| Defaults | `LOCKDOWN_LISTEN=0.0.0.0:8443`, `LOCKDOWN_DATA=/data`, `LOCKDOWN_SIGNUPS=closed` (05-self-host §9: "ships closed by default") |
| Memory, idle after start | **5.5 MiB** (`docker stats`, x86-64, release, TLS on) |
| Memory, after load | **6.2 MiB**: two devices, register, recover, 21 logins, 41 syncs each with its key-transparency checks |

Measured in this sandbox on 2026-10-18; the CI smoke test prints both numbers on every run. The 512 MiB floor of 05-self-host §8 is very generous for the Lockdown server alone; a Raspberry Pi class board is plenty. Load with many open live streams was not measured.

## 8. The server backup

`lockdown-server backup keygen` prints a **Backup Key** (`LDBK-` and 32 random bytes in Base32 groups) once; the server keeps nothing. `create --out FILE` (with the key on standard input or in `LOCKDOWN_BACKUP_KEY`) takes `VACUUM INTO` of the live database, the server key file and every attachment file, and writes one file:

```
"LDSRVBK1" ‖ u32 header length ‖ header (canonical CBOR {v, snapshot_id, created, server_version, schema})
chunks: u32 length ‖ 24-byte nonce ‖ XChaCha20-Poly1305 ciphertext
plaintext stream: u32 manifest length ‖ manifest {files: [{name, size, BLAKE3}]} ‖ each file's bytes
key = HKDF-SHA-512(Backup Key, salt = snapshot_id, info = "lockdown/server-backup/v1")
AAD per chunk = BLAKE3(header) ‖ chunk index ‖ last-chunk flag
```

So the header can't be swapped, chunks can't be reordered or dropped, a cut-off file is refused, and file names and sizes are inside the encryption. `verify` opens everything, checks each hash, opens the database copy and the key file (a restore drill that changes nothing); `restore --in FILE --data DIR` writes through a staging folder into an empty directory only, accepting only the database, the key file and `blobs/<vault>/<hash>` names. Tests: a live server's backup, restored, starts a server with the same accounts, the same key-transparency log key and log, where the account still signs in; a wrong key, a changed byte and a cut-off file are refused; a restore over an existing server is refused (D226).

Not yet (05-self-host §4): the Backup Key wrapped to the IRK and printed on the recovery kit, the hybrid wrap to each device, an Argon2id passphrase slot, schedules, retention, two-media checks, the anti-rollback check against the live server's heads, the drill's result shown in the clients.

## 9. Storage

**Attachment files** (`--blob-dir DIR`, D225): a blob's body is written to `DIR/<vault>/<BLAKE3 hex>` (whole, then renamed into place, user-only) before its row; the row keeps the size and references, so every blob rule (D76: content addressing, `unknown_blob`, the grace period) is unchanged; collection deletes the row and then the file; bodies already in the database are still served. Backups carry the files.

**Postgres and S3** are not built. Both fit behind `Storage` (the blob body is now one seam: an S3 put/get/delete in place of the file calls), but Postgres means a second SQL dialect for every query the sharing and server slices wrote against SQLite, and S3 means SigV4 signing and a test double; neither fits a first slice that also had to carry TLS, the live channel and sessions. Recorded as the next storage step.

## 10. Running and testing it

```sh
# A server with TLS on this machine (any PEM pair; tests make theirs with rcgen)
cargo run -p lockdown-server -- --data /tmp/ld --listen 127.0.0.1:8443 \
    --tls-cert cert.pem --tls-key key.pem --redirect-http 127.0.0.1:8080
LOCKDOWN_CA_CERT=ca.pem cargo run -p lockdown-cli -- --root /tmp/me account register --server https://localhost:8443 --login me@example.com --password-stdin

# The tests of this slice
cargo test -p lockdown-tls                       # handshakes; LOCKDOWN_TLS_INTEROP=require to insist on OpenSSL interop
cargo test -p lockdown-server --test selfhost    # hybrid TLS, sessions, live channel, file blobs
cargo test -p lockdown-server --test binary      # the binary: plain-HTTP rule, TLS, healthcheck, redirect, SIGHUP, backups
cargo test -p lockdown-cli --test tls_end_to_end # two CLI devices over a hybrid-only server
cargo test -p lockdown-app --test live_sync      # two apps: a change shows without Sync; sessions end
cd apps/desktop && pnpm vitest run src/features/sessions

# The image
docker build -t lockdown-server:local .
bins/server/container/smoke.sh lockdown-server:local target/debug/lockdown
```

## 11. Gaps

1. Postgres and S3-compatible attachments (§9).
2. The mobile push relay (D122; D229 records its shape).
3. A published image: reproducible build, signature, SBOM and provenance (04-security §15); arm64.
4. The admin page, the operator settings (SMTP, so email verification of new devices can exist), scheduled backups and drills shown in the clients (05-self-host §4, §10).
5. Item shares to someone else don't ping the recipient's live channel; the recipient sees them on its next look.
6. A device revoked through the identity manifest keeps its sessions until they run out (its writes are refused by D73 already); ending them with the revocation is a small next step.
7. Many open live streams were not load-tested; the per-account cap is a guess (16).
8. The web vault must subscribe with `fetch` streaming and the bearer header, not `EventSource`.

**unicron host-contract gaps:** none hit. The Signed in section is `Item`, `Badge`, `Button` and `Dialog` from `@unicron/ui` as the Devices screen already used them; the design-system ratchet stays at its baseline (4).

## 12. Proposed decisions

For the coordinator to record. **Flagged** rows are user-visible and need Sean.

| ID | Decision | Owner |
|---|---|---|
| D215 | **The post-quantum transport's provider**: rustls on `ring` with Lockdown's own `X25519MLKEM768` group over RustCrypto `ml-kem` 0.3 (already linked for X-Wing), in `lockdown-tls`; not `aws-lc-rs` (a second C library beside `ring`, which the passkey authenticator and tokens keep; licences would pass) nor `rustls-post-quantum` (`0.3.0-dev`, on `aws-lc-rs`). Interop with OpenSSL 3.5 is a required CI job. Plain version: the connection to your server resists a future quantum computer, built from code already in the app. | Accepted by default under D38; **audit item** (our handshake glue) |
| D216 | **TLS policy to Lockdown's server**: TLS 1.3 only on both ends; X25519MLKEM768 first; the client also offers X25519 (for a proxy without the hybrid group) and the server takes X25519 or P-256 from older clients, or only the hybrid with `--tls-pq-only`; downgrade between two hybrid-capable ends is blocked by the signed transcript. Plain version: post-quantum whenever both sides can, ordinary modern TLS otherwise. | Accepted by default under D38; audit item |
| D217 | **Pinning a private CA**: `LOCKDOWN_CA_CERT` makes a client trust exactly the certificates in that file instead of the system's store, for every server; a missing or empty file is an error. Plain version: a self-hosted server on its own certificate authority is trusted by name, and nothing else is. | **Flagged for Sean** (how self-hosters on a private CA connect) |
| D218 | **Plain HTTP only on this machine**: clients refuse `http://` to anything but loopback; the server refuses to listen in plain HTTP away from loopback unless `--insecure-http` (a TLS proxy in front), and says "PLAIN HTTP" when it does. Plain version: your session never crosses a network unencrypted. | **Flagged for Sean** (a LAN server on plain HTTP stops working) |
| D219 | **TLS on the server**: PEM certificate and key; reload on SIGHUP and when the files change (every 60 s), a bad renewal keeps the old pair; HSTS one year without `includeSubDomains` or `preload`; optional HTTP→HTTPS `308` from the `Host` name; handshakes on their own tasks (10 s, 512 at once); no built-in ACME. Plain version: the server does its own HTTPS and picks up renewed certificates by itself. | Accepted by default under D38 |
| D220 | **Sessions are rows**: a row per token keyed by its nonce (the session id), read on every authenticated request so an ended session stops at once (amends D52's "no database lookup"); last use and address written at most once a minute; the client's own name kept (80 printable characters); a pre-table token gets a row on first sight; rows dropped a week after the token runs out, never earlier. Plain version: the server remembers where you're signed in so you can sign a place out. | **Flagged for Sean** (the server now keeps each session's address and app version) |
| D221 | **Ending a session**: Devices → Signed in lists every live session (device, app, address, signed in, last used; this device marked) with **End** and **End all other sessions** behind a confirm, no approval gate (it takes access away, as D196's removals do); it doesn't revoke the device; the ended device is told at once and asks for the password at its next sync. Plain version: see where you're signed in and sign any of them out. | **Flagged for Sean** (no fresh check to end a session) |
| D222 | **The live channel**: server-sent events at `/v1/events`, content-free `{v, kind}` (`sync`, `alerts`, `sessions`, `session_ended`); every write under `/v1/vaults` and `/v1/identity` pings the writer's other sessions and a shared vault's owner and active members; 20 s keep-alives, 65 s client read timeout, 16 streams per account; the desktop keeps it open only while unlocked, folds bursts into one sync, and a sync it starts is not activity for the idle lock. Plain version: a change on one device shows on your others within a second, and your server learns nothing new from it. | Accepted by default under D38; **flagged for Sean** (devices now stay connected while unlocked) |
| D223 | **Server-sent events over WebSocket** for the desktop and web half: no new dependency on either side, plain HTTP/1.1 through every proxy, the bearer header as everywhere else (the web vault reads it with `fetch` streaming). Plain version: the simplest live connection that works everywhere. | Accepted by default under D38 |
| D224 | **Trusted proxies**: `--trusted-proxy` IPs or CIDR blocks; only their `X-Forwarded-For` is read, from the right, skipping trusted hops; the login brake and the session list use the result. Plain version: behind a proxy the server still sees who is really knocking. | Accepted by default under D38 |
| D225 | **Attachments as files**: `--blob-dir` keeps blob bodies as `<vault>/<hash>` files written before their row, the rows and references staying in SQLite; old bodies in the database are still served; Postgres and S3 remain to build. Plain version: big attachments sit beside the database instead of inside it. | Accepted by default under D38 |
| D226 | **The server backup**: format `LDSRVBK1` (§8); a **Backup Key** (`LDBK-…`) made by `backup keygen`, shown once, held by the person; database, server key file and attachment files in one sealed file; `verify` as the restore drill; restore only into an empty directory. The kit line and the device wraps of 05-self-host §4 come later. Plain version: one encrypted file brings a dead server back, and only you can open it. | **Flagged for Sean** (a new key to keep; losing it loses the backups) |
| D227 | **The container**: distroless `cc` nonroot (uid 65532), read-only root, one volume `/data`, `/tmp` in memory, port 8443, `LOCKDOWN_SIGNUPS=closed` by default, `HEALTHCHECK` by `lockdown-server healthcheck`; built and smoke-tested in CI, never pushed; compose with no capabilities and a 256 MiB limit. Measured 5.5 MiB idle, 6.2 MiB after load. Plain version: one small, locked-down container, closed to strangers until you open it. | **Flagged for Sean** (sign-ups start closed; the port) |
| D228 | **Wire additions** (D62, additive): `version`, `features.live` and `features.sessions` in the discovery document; `SessionInfo`, `SessionList`, `SessionsEnded`, `LiveEvent`, `LiveKind` (unknown kinds read as `other`); `/v1/account/sessions*` and `/v1/events`; the client `User-Agent` names the OS; the desktop commands `session_list`, `session_end`, `session_end_others` and the window event `lockdown://live`. Plain version: older and newer parts keep understanding each other. | Accepted by default under D38 |
| D229 | **The mobile push relay's shape** (recorded, not built): the self-host server registers once with Lockdown's relay for an opaque server id; on a `sync` change it posts `{server_id, device_token}` per subscribed phone; the relay signs for APNs and FCM and keeps nothing beyond retries; per-server rate limits; off by a flag, phones then fall back to background fetch. The live channel's kinds are the ping's only content. Plain version: phones will be woken the same content-free way, through a relay that learns only that some server woke some phone. | Proposed; **flagged for Sean** (a Lockdown-run relay for self-hosters) |
