# Self-host server, second pass: Postgres, S3, the admin page, drills, email and alerts

_Status: built, 2026-10-18, under D122 ("desktop and web first"). Continues [`20-self-host-server.md`](20-self-host-server.md). Implements the Postgres and S3 rows of `05-self-host.md` §1 and §2, the admin page of §10, the restore drill and the Backup Key on the kit of §4, the operator settings (SMTP) of §2, and the rest of the `self-host-push`, `account-alerts` and `email-breach-monitoring` rows. Operator guide: [`docs/self-host.md`](../self-host.md) §3 and §8–§11._

## For future agent

- **TL;DR:** The server's store runs on **SQLite or Postgres** through one SQL layer (`bins/server/src/db.rs`): the same statements on both, and the whole server test suite runs against Postgres in CI. Attachment bodies can live in an **S3-compatible bucket** (`s3.rs`, a SigV4 client of our own), with rows and the quota unchanged. The operator gets an **admin page** at `/admin` behind an **operator key** (Argon2id hash in the database), with accounts, storage and quota use, devices, sessions, **invites** and backups. The server **backs itself up on a schedule and runs restore drills** (`drills.rs`) into a scratch database, with the result in the log and on the page. `backup keygen --kit` writes the **operator's recovery kit** with the Backup Key. With **SMTP**, a device new to an account **waits for the account's email** to approve it, and new devices and security-key changes are **mailed and alerted**; the desktop shows them as **OS notifications** through Tauri's notification plugin. Items shared on their own now **ping their recipients** on the live channel. **Email breach monitoring** works over an operator's own **k-anonymity range file or service**, no paid key.
- **Read with:** `bins/server/src/db.rs` (the dialect layer), `storage.rs` (`SqlStorage`, `BlobBodies`), `s3.rs`, `admin.rs`, `drills.rs`, `kit.rs`, `mail.rs`, `pages.rs`, `second_factor.rs` (email approval beside keys), `breach_range.rs`, `live.rs` (`item_audience`), `main.rs` (every flag), `crates/app/src/live.rs` (`notices_for`), `apps/desktop/src-tauri/src/live.rs`, the tests `bins/server/tests/{operator,storage_backends}.rs`.
- **Do not:** write SQL that only one database reads (no `INSERT OR …`, no two-argument `MAX`, no `PRAGMA` outside `db.rs`; a `SUM` is cast to `BIGINT`); take a link's host from a request (`--public-url` only); send a mail that carries anything from a session, a key or a vault; let the admin page show or release a secret; answer an email approval on `GET` (mail scanners open links); add a table without adding it to `storage::TABLES` (a test fails).

## 1. What was built, per row

| Row | State | What is real | What is missing |
|---|---|---|---|
| `single-container` | done (not published, by the brief) | **Postgres** behind the existing `Storage` trait (`--database-url`; SQLite stays the default), the same statements and checks on both (§2), the schema made on first start, one server per database (advisory lock), TLS 1.3 to any database away from this machine. The **full server test suite** runs against Postgres (`LOCKDOWN_TEST_DATABASE_URL`, a scratch schema per store) beside the SQLite run, and so do the CLI, app and web tests that start a server in process: CI's `storage-backends` job, a `postgres:16` service container. Backups of a Postgres store are the same snapshot file; a backup restores into either database. **S3-compatible attachment storage** (§3) behind the quota: rows, sizes and references stay in the database; tested against **MinIO** in the same CI job (built from source at a pinned commit, since no MinIO image is pulled). The Dockerfile and its smoke test are unchanged (pure Rust; SQLite by default). | A published, signed, reproducible image and arm64 (the release builder's); a connection pool (one connection, as SQLite has, §2); Helm. |
| `official-self-host` | done | The **admin page** (§4): operator sign-in with a key the operator sets (`admin set-key`), separate from accounts; overview (release, database and whether its link is TLS, attachments, quota, sign-ups, TLS, email, breach feed, backups, last successful restore, setup checks), accounts (login, registered, devices, live sessions, last used, vaults, attachment room against the quota, security keys), one account's devices and sessions with **End every session**, **invites** (closed sign-ups let one invited login name register once), backups on disk and the drill history with **Back up now** and **Run a restore drill now**. **Scheduled backups and restore drills** (§5): the newest backup restored into a scratch database (SQLite, or a scratch Postgres schema) and checked against the live server; pass or fail in the log and on the page, a red banner while the newest drill failed. The **operator's recovery kit** (§6): a printable page with the Backup Key in captioned large type and the restore steps. | "Last successful restore" in the clients (Aegis lesson); a passkey on the admin page (05-self-host §10); cross-publishing checkpoints; update availability; the two-media check of 05-self-host §4. |
| `account-alerts` | done | An **SMTP setting** (§7); **email approval of new devices**: a device the account has never seen waits after the password until the account's address approves it through a one-time link (Approve or "This wasn't me" on the server's page); **new-device and security-key-change alerts** by email (templated, plain, no secrets), linking the session list (`/vault/#/devices`, which the web vault now opens on Devices; the app's Devices → Signed in); security-key changes are new account alerts every device shows in Security. **OS notifications** on the desktop for the same events through `tauri-plugin-notification`, from Rust only (§8): it needed nothing beyond the plugin. | macOS shows notifications only from a bundled, signed app (the plugin's rule; `tauri dev` may show none); the web vault shows the alerts in Security, no browser notifications. |
| `self-host-push` | done (desktop and web half; mobile relay deferred by D122) | Recipients of items shared on their own (wave 2) are pinged on the live channel when an item is shared with them, withdrawn, or changed by its owner (§9); the desktop syncs on the ping and pulls the share. | The mobile push relay (D229, mobile deferred); the web vault opening the live channel itself (it syncs every minute while in view). |
| `email-breach-monitoring` | done | A **pluggable breach feed with a self-hosted option** (§10): an operator-supplied **range file** (read on the server, nothing leaves it) or **range service** (sent the first six hex digits of SHA-1(address) only), with an optional breach catalog in HIBP's free breach-list shape; tested with a committed fixture. The paid HIBP feed stays as it was, optional. | Building range files from public breach corpora is the operator's (the format is documented). |
| `family-plan` | not done | Out of this pass, as the brief says: it waits on pricing. | |

## 2. One SQL layer, two databases (`db.rs`)

The store's queries were written against `rusqlite` across six modules (storage, alerts, sessions, sharing, security keys, legacy). They now go through a small layer whose API mirrors the calls they made (`execute`, `query_row`, `query_opt`, `query_map`, `execute_batch`, `transaction`), over four value kinds (null, `i64`, text, bytes):

| | SQLite | Postgres |
|---|---|---|
| Driver | `rusqlite` (bundled), one connection behind a lock | `tokio-postgres` 0.7, one connection behind a lock, prepared statements cached |
| Placeholders | `?1` | rewritten to `$1` (outside quotes) |
| Types | `INTEGER`, `BLOB`, `TEXT` | the same definitions mapped: `BIGINT`, `BYTEA`, `TEXT COLLATE "C"` (byte order, as SQLite sorts) |
| Upserts | `INSERT … ON CONFLICT … DO NOTHING / DO UPDATE SET c = excluded.c` | the same text |
| Transactions | `BEGIN` … `COMMIT`, `ROLLBACK` on drop | the same, on the held connection |
| Snapshot | `VACUUM INTO` | every row into a fresh SQLite file inside one `REPEATABLE READ` transaction |

**Why not `sqlx`** (03-architecture names it): its compile-time query checks need a database at build time or a checked-in cache for every query, and it would replace the SQLite driver the desktop's own store shares; the server's queries are few and plain, and a thin layer kept every existing statement and test. **Why this way into Postgres:** the store is synchronous and is called from async handlers, as SQLite always was; `tokio-postgres` runs its connection on a two-thread runtime the store owns, and each call waits for its answer on the caller's thread (`futures-executor`), exactly as a caller waited on the SQLite lock. One connection keeps every read-then-write inside a transaction as serialised as it was on SQLite, which the head chain and the item revisions rely on; an **advisory lock** per schema makes a second server on the same tables refuse to start, rather than race. A pool and row locks are the scale step, not needed for a household or a small organisation (05-self-host §8).

**TLS to Postgres.** Through `lockdown-tls`'s rustls client (TLS 1.3, the hybrid group first; the same provider as everything else), with the system's roots or `--database-ca`. Away from this machine TLS is required whatever the URL says; `sslmode=disable` is accepted for a socket or loopback only. Checked by hand against a Postgres 16 with TLS on (TLSv1.3 negotiated, another CA refused: `storage_backends.rs`, `LOCKDOWN_TEST_DATABASE_TLS_URL`); CI's service has no certificate, so that test runs locally only. SCRAM channel binding is not offered (the password exchange is still SCRAM inside TLS 1.3).

**Both ways in tests.** `SqlStorage::open_scratch()` (what `AppState::in_memory` uses) is SQLite in memory, or a new schema `lockdown_t_<random>` in the Postgres database `LOCKDOWN_TEST_DATABASE_URL` names, dropped with the store. So every server test, and every CLI, app and web test that drives a server in process, runs on Postgres in CI's `storage-backends` job: 258 tests, measured locally against Postgres 16 and MinIO, all passing, no schema left behind. `storage::TABLES` lists every table in copy order; a test fails if the schema makes one it doesn't name.

**Backups across databases.** A Postgres store's snapshot is the same `LDSRVBK1` file with the same SQLite copy inside, so `verify` and the drills don't care which database made it, and `restore --database-url` copies the rows into an empty Postgres database: SQLite → Postgres is a backup and a restore.

## 3. Attachments in a bucket (`s3.rs`)

`BlobBodies` is the one seam: `put`, `get`, `delete` of a body named `<vault>/<BLAKE3 hex>`, beside `BlobDir` (the files of D225). `S3Bodies` puts `<prefix><vault>/<hash>` with one signed `PUT` before the row is written, so a row always has its body; collection deletes the row, then the object. The quota (D234) counts rows, so it is unchanged. The client: path-style URLs (or virtual-hosted), Signature Version 4 over the payload's SHA-256 (HMAC-SHA-256 from the RustCrypto crates already in the tree), `reqwest` on the store's runtime with TLS from `lockdown-tls`'s provider, no redirects, plain `http://` only to this machine; the bucket is checked with a `HEAD` at start, and wrong keys, a wrong region and a missing bucket are told apart. The signer reproduces AWS's published "GET Object" example; the whole path is tested against MinIO (store, serve, count, collect, back up, restore into files, a wrong key refused). **Why not an SDK:** `aws-sdk-s3` brings a large tree and its own TLS and runtime choices for four calls.

## 4. The admin page (`admin.rs`, `pages.rs`)

| | |
|---|---|
| Key | `lockdown-server admin set-key` makes `LDOP-` + 20 random bytes in Base32 (or takes one typed with `--stdin`, 16 characters or more), prints it once, and keeps an Argon2id hash (the platform floor, its own salt) in `operator_settings`; `admin clear-key` closes the page. Not an account; opens no vault. |
| Sign-in | A form; wrong keys are slowed by the login brake (per address and on the operator name) and recorded. A right key sets `__Host-lockdown-admin` (random 32 bytes; the server keeps its BLAKE3 hash), `Secure; HttpOnly; SameSite=Strict`, eight hours at most, one unused. Every form carries the session's own anti-forgery token. |
| Pages | Written on the server: no script, one inline style sheet allowed by its hash in the CSP (`default-src 'none'; form-action 'self'; frame-ancestors 'none'`), `no-store`, `no-referrer`, `DENY` framing. Every value from a person, a device or the network is escaped. |
| Shows | What the server already has (docs/self-host.md "What the operator can see"): counts, login names, device ids and join dates (device names are encrypted), sessions' app, address and times, attachment bytes against the quota, security keys counted, invites, backups, drills. |
| Changes | Invites, ending an account's sessions (the devices ask for the password at their next sync; nothing is revoked), a backup or a drill now. Devices, keys and passwords stay with the account's own devices. |

**Invites** are login names, not codes: while sign-ups are closed, `register/start` and `register/finish` take a login name with an unused invite that hasn't run out (7 days unless the operator says otherwise, 1–90), and the invite is used up when the account is made. Nothing new reaches the clients; the operator tells the person.

## 5. Scheduled backups and restore drills (`drills.rs`)

With `--backup-dir` and the Backup Key (`LOCKDOWN_BACKUP_KEY` or `--backup-key-file`, for a container secret), a thread backs up every `--backup-every-hours` (24) into `lockdown-<UTC time>.ldbk`, keeps the newest `--backup-keep` (14), and every `--drill-every-hours` (168) drills the newest one: `verify` (hashes, the database copy opens), `restore` into a scratch database (a temporary SQLite file beside the backups, or a scratch schema in the live Postgres, dropped after), and checks against the live server: the server key file is the live one; the public log is the start of the live log, leaf for leaf; every vault's head chain is whole and not further on than the live one (a live server behind its own backup was rolled back); every attachment body came back at its size under its own hash. Each result is a row in `admin_events` and a log line (file name, counts, the failed step; never a key). Measured here on a fresh server: a backup and a passing drill within a second of start.

The schedule needs the Backup Key **in the server's memory** for as long as it runs: D226 kept it out of the server entirely, and that stays the case without `--backup-dir` (then `backup create` from the operator's own scheduler, and `verify` as their drill). Flagged (D297).

## 6. The operator's recovery kit (`kit.rs`)

`backup keygen --kit FILE [--public-url URL] [--backup-dir WHERE]` writes, beside the key it prints, a self-contained page (user-only, never over an existing file): the server's address, where backups are kept, the Backup Key in groups of four, every character in its own box captioned **letter** or **digit**, the key once more to type, and the three restore steps. `backup kit --kit FILE` makes the page for a key you already have. It is meant to be printed and the file deleted; the server keeps neither. Like the person's kit (D11), it is the one thing that brings a dead server back; the admin page never shows the key.

## 7. Email (`mail.rs`)

`--smtp-url smtps://host[:465]` (TLS from the first byte) or `smtp://host[:587]` (STARTTLS, which must be offered unless the relay is this machine), `--smtp-from`, `LOCKDOWN_SMTP_USER`/`PASSWORD`, `--smtp-ca`, and `--public-url` (links in mail; never a request's `Host`, which an attacker could set). The client is ours (`EHLO`, `STARTTLS`, `AUTH PLAIN` only inside TLS or to this machine, `MAIL`, `RCPT`, `DATA`): every header value is refused if it holds a line break, the address must be a plain one, and the body travels base64-encoded so no line of it can end the message. TLS is `lockdown-tls`'s client config: TLS 1.3 only, which every current relay offers; a relay that speaks only TLS 1.2 is refused (flagged with D299). An outbox thread sends (three tries, five and ten seconds apart), so a slow relay never holds a request; at most 1,000 wait; the log says which kind of message went, never to whom. **Why not `lettre`:** a large tree (its own TLS and runtime choices) for one plain-text message type; the hand-written client is about 250 lines and is tested against a relay in the test process, plain and with STARTTLS (the password sent only after it).

## 8. New devices, alerts and notifications

**Email approval** (`second_factor.rs`). A sign-in from a device the account has never seen, when the server has SMTP with approval on (`--email-approval`, on by default with SMTP), the login is an email address, the account has a device already and has no security keys (keys take precedence, D250), gets the same waiting sign-in a key would: the reply carries `second_factor` with `email_to: "s•••@example.com"` and no keys, and the device polls with the poll key only it holds. The mail's link (`/approve/<pending>?code=<32 random bytes>`; the server keeps BLAKE3 of the code) opens a page that shows the account, the device's own name, the address it came from and the time, and changes nothing: a mail scanner that fetches it approves nothing. **Approve** (a `POST` with the code) lets the device collect its session; **This wasn't me** drops the sign-in, and the page tells the person to change the password. Fifteen minutes, once; five approval mails per account an hour, then `429`. Clients: the desktop's Join waits with the server's words and opens no browser page; the web vault's sign-in waits and polls (`web_sign_in_continue`).

**Alerts.** A new device's sign-in (D133) is now mailed too. Adding or removing a security key records an account alert of the new kinds `security_key_added` / `security_key_removed` with the key's name, which Security shows on every device, and mails the account. Each mail links where sessions are ended. `AlertKind` gained `other` for kinds a build doesn't know (D62).

**OS notifications** (`crates/app/src/live.rs`, `apps/desktop/src-tauri/src/live.rs`). On the live channel's `alerts` event the core fetches the alerts since the newest it has notified (set when the channel first opens, so only what happens from then on notifies) and raises a `notify` notice for a new device or a key change; the shell shows it with `tauri-plugin-notification` from Rust, so the webview gets no notification permission and the capability file is unchanged. Plain words, no value. No OS-level integration was needed beyond the plugin (D-row D302); on macOS the system shows notifications only for a bundled app.

## 9. Pings to people an item is shared with (`live.rs`)

Sharing an item (`POST /v1/shares/items`) pings its recipient's devices with a content-free `sync`; withdrawing it pings the other person; any write to `/v1/vaults/{vault}/items/{item}` also pings everyone that item is shared with on its own (`item_audience`), beside a shared vault's members as before. The desktop's sync already pulls received items, so a share shows up within a second. Nothing in the ping says what changed or for whom.

## 10. Breach monitoring over the operator's own data (`breach_range.rs`)

`BreachFeed` was already injectable (D134). `RangeBreachFeed` looks addresses up by **k-anonymity**: SHA-1 of the trimmed, lower-cased address, upper-case hex. A **range file** (`--breach-range FILE`) holds `HASH:Breach;Breach` lines (`#` comments), read on the server and read again when it changes: nothing leaves the machine. A **range service** (`--breach-range https://…`) is asked `GET <base>/<first 6 hex digits>` and answers `<other 34 digits>:Breach;…` lines (`404` for none), so only a prefix many addresses share leaves the server; `https://` or this machine only, answers capped. `--breach-catalog FILE` (HIBP's free `/api/v3/breaches` list, saved) gives each breach its site, date and data classes. Each breach is still reported once per address (D134, D269). Tested with a committed fixture (`bins/server/tests/fixtures/breach-range.txt`, `breaches.json`) and a local range service that checks only the prefix arrives. **Why SHA-1:** it is what Pwned Passwords' range API and the published corpora use; the hash only buckets addresses, it protects nothing by itself.

## 11. Running and testing it

```sh
# Postgres and a bucket (a MinIO on this machine)
lockdown-server --data /data --database-url 'postgres://lockdown:pw@db.example/lockdown' --database-ca db-ca.pem \
    --s3-endpoint https://minio.example:9000 --s3-bucket lockdown-attachments   # keys in LOCKDOWN_S3_ACCESS_KEY / _SECRET_KEY

# The admin page, invites, the kit, the schedule
lockdown-server admin set-key --data /data
lockdown-server admin invite --data /data --login new@example.com --days 7
lockdown-server backup keygen --kit operator-kit.html --public-url https://vault.example
LOCKDOWN_BACKUP_KEY_FILE=/run/secrets/backup lockdown-server --data /data --backup-dir /data/backups

# Email
lockdown-server --data /data --public-url https://vault.example --smtp-url smtps://mail.example:465 \
    --smtp-from 'Lockdown <lockdown@example.com>'                              # LOCKDOWN_SMTP_USER / _PASSWORD

# Breach monitoring over your own data
lockdown-server --data /data --breach-range /data/breach-range.txt --breach-catalog /data/breaches.json

# Tests
cargo test -p lockdown-server                                   # SQLite
LOCKDOWN_TEST_DATABASE_URL='postgres://…/lockdown_test?sslmode=disable' cargo test -p lockdown-server -p lockdown-cli -p lockdown-app -p lockdown-web
LOCKDOWN_TEST_S3_ENDPOINT=http://127.0.0.1:9000 LOCKDOWN_TEST_S3_ACCESS_KEY=… LOCKDOWN_TEST_S3_SECRET_KEY=… \
    cargo test -p lockdown-server --test storage_backends
cargo test -p lockdown-server --test operator                   # email approval, admin page, drills, SMTP
cd apps/desktop && pnpm vitest run src/features/depth           # the email wait on the desktop
cd apps/web && pnpm vitest run src/features                     # the email wait in the web vault
```

## 12. Gaps

1. One database connection per server (as SQLite); a pool and row-level locking are the step for many concurrent users. One server per database is enforced, not supported.
2. CI's Postgres has no TLS; the TLS path is tested by hand (§2).
3. MinIO is built from source in CI (about three minutes), since its images are no longer published on Docker Hub; any S3-compatible store can replace it.
4. "Last successful restore" isn't shown in the clients yet, only on the admin page and in the log; the drill doesn't start a client against the scratch copy (05-self-host §4 asks for a test identity opening a test vault).
5. The admin page has no passkey yet (05-self-host §10), no update check, no cross-publish switch.
6. The SMTP client speaks TLS 1.3 only; DKIM and SPF are the relay's.
7. The web vault doesn't open the live channel; it syncs every minute while in view and on focus.
8. The Backup Key sits in the server's memory while scheduled backups run (D297).
9. The image wasn't rebuilt in this sandbox (the release build is the container job's); the Dockerfile is unchanged and the server stays pure Rust.

**unicron host-contract gaps:** none hit. The desktop's email wait reuses the security-key wait (`Button`, `Spinner`, `FieldError` from `@unicron/ui`); the server's pages are plain HTML outside the design system, as the share and request pages are.

## 13. Proposed decisions

For the coordinator to record. **Flagged** rows are user-visible and need Sean.

| ID | Decision | Owner |
|---|---|---|
| D290 | **Postgres through one SQL layer, not `sqlx`**: the store's statements are written once in the subset SQLite and Postgres both read (numbered placeholders, `ON CONFLICT` upserts, `COALESCE`/`CASE`, a cast `SUM`), table definitions mapped (`BIGINT`, `BYTEA`, `TEXT COLLATE "C"`); `tokio-postgres` on a two-thread runtime the store owns, one connection, an advisory lock so one server uses one database; TLS 1.3 through `lockdown-tls` to any database away from this machine. Plain version: a bigger install can keep its data in Postgres, with the same rules as the single file. | Accepted by default under D38; audit item (the layer) |
| D291 | **The server's tests run on both databases**: a fresh store in tests is SQLite in memory, or a scratch Postgres schema when `LOCKDOWN_TEST_DATABASE_URL` is set; CI's `storage-backends` job runs the server, CLI, app and web tests against a Postgres service and MinIO. Plain version: Postgres gets every check SQLite gets. | Accepted by default under D38 |
| D292 | **One backup format for both databases**: a Postgres store's snapshot is the same `LDSRVBK1` file with a SQLite copy inside (rows copied in one repeatable-read transaction); restore takes `--database-url` for an empty Postgres database. Plain version: a backup brings a server back on either database, and moves it between them. | Accepted by default under D38 |
| D293 | **Attachments in an S3-compatible bucket**: bodies as `<prefix><vault>/<BLAKE3>` objects written before their rows, rows and quota in the database, our own SigV4 client (path or virtual-hosted style, TLS from `lockdown-tls`, plain HTTP only to this machine), the bucket checked at start; MinIO built from a pinned commit in CI. Plain version: big attachment stores can live in any S3-style storage, still encrypted and still counted. | Accepted by default under D38 |
| D294 | **The admin page and its key**: `/admin`, behind an operator key that is not an account (`admin set-key`: shown once, Argon2id hash kept), a `__Host-` cookie (8 h, 1 h idle, `Strict`) and a per-session form token; shows what the server already knows, never a secret; no passkey yet. Plain version: the person who runs the server gets a page to see accounts, storage, invites and backups, with its own key. | **Flagged for Sean** (operator-facing; no passkey yet) |
| D295 | **Invites are login names**: while sign-ups are closed, a login name the operator invited (7 days by default, 1–90) may register once; nothing changes for the apps. Plain version: invite a household member by their email address instead of opening sign-ups. | **Flagged for Sean** |
| D296 | **The operator can end an account's sessions** from the admin page (no device revoked, no password changed; the devices ask for the password at their next sync). Plain version: if an account looks taken over, the operator can sign it out everywhere. | **Flagged for Sean** (a new operator power) |
| D297 | **Scheduled backups and restore drills**: with `--backup-dir` and the Backup Key the server backs up daily, keeps 14, and drills weekly (restore into a scratch database, then hashes, key file, log prefix, head chains, attachments against the live server); results on the admin page and in the log. The key is then held in the server's memory (amends D226 for this mode only); weekly where 05-self-host §4 said monthly. Plain version: the server checks by itself that its backups would really bring it back. | **Flagged for Sean** (the key in memory; the defaults) |
| D298 | **The operator's recovery kit**: `backup keygen --kit` (or `backup kit`) writes a printable page with the Backup Key in captioned large type and the restore steps; neither page nor key is kept by the server. Plain version: one sheet of paper brings a dead server back. | **Flagged for Sean** |
| D299 | **SMTP**: `smtps://` or `smtp://` with STARTTLS required away from this machine, TLS 1.3 through `lockdown-tls`, `AUTH PLAIN` only inside TLS, headers checked for line breaks, bodies base64; our own client and an outbox thread (three tries); links only to `--public-url`. Plain version: the server can send email safely through your mail provider. | Accepted by default under D38; **flagged for Sean** (TLS-1.2-only relays are refused) |
| D300 | **Email approval of new devices**: with SMTP, a device new to an account with an address login, a device already and no security keys waits after the password for the account's email: a one-time link (15 min, at most five an hour), a page that changes nothing until Approve, "This wasn't me" stops the sign-in; on by default with SMTP (`--email-approval off`). Wire additions: `SecondFactorRequest.email_to`, the error detail `email`. Plain version: a stolen password alone can't add a device to your account; you approve it from your inbox. | **Flagged for Sean** (user-visible; on by default) |
| D301 | **Alert emails and key-change alerts**: new-device sign-ins and security keys added or removed are mailed (plain, no secrets, a link to `/vault/#/devices` and the app's Signed in) and the key changes are new account alerts (`security_key_added`, `security_key_removed`, with `key_name`) shown in Security on every device; `AlertKind::Other` for kinds a build doesn't know. Plain version: you hear about changes to how your account is signed in to, by email and on your devices. | **Flagged for Sean** (user-visible) |
| D302 | **Desktop OS notifications** for a new device and a key change through `tauri-plugin-notification`, raised by the core (`notify` notice) and shown from Rust; only events from when the live channel first opened; the webview gets no notification permission; no OS integration beyond the plugin was needed. Plain version: the desktop app tells you even when its window is in the background. | **Flagged for Sean** (user-visible) |
| D303 | **Pings to item-share recipients**: sharing, withdrawing and editing an item shared on its own pings the other person's devices on the live channel (content-free `sync`). Plain version: something shared with you shows up within a second. | Accepted by default under D38 |
| D304 | **Breach monitoring over the operator's own data by k-anonymity**: SHA-1 of the lower-cased address; a range file (`HASH:Breach;…`, read locally) or a range service sent the first six hex digits only; an optional catalog in HIBP's free breach-list shape; the paid feed stays optional. Plain version: a self-hosted server can watch for your address in breaches without a paid key, and without telling anyone your address. | **Flagged for Sean** (the file format is ours) |
