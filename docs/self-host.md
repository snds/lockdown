# Running your own Lockdown server

_For the person who runs the server. The design behind it is in [`plan/05-self-host.md`](plan/05-self-host.md) and [`plan/20-self-host-server.md`](plan/20-self-host-server.md)._

The Lockdown server is one program, `lockdown-server`, and the same program Lockdown would run as a cloud. It keeps only locked boxes: every item, name, address and note is encrypted on your devices before it reaches the server, and the server can't open any of it. What it can see is listed in [What the operator can see](#what-the-operator-can-see).

It is small. One household fits on a Raspberry Pi class machine; see [Memory and disk](#memory-and-disk) for measured numbers.

## 1. Install

### With a container (recommended)

The repository's `Dockerfile` builds a small image: the server and the C runtime it needs, nothing else (no shell), running as an unprivileged user (uid 65532), made for a read-only root file system with one volume, `/data`.

```sh
git clone https://github.com/snds/lockdown && cd lockdown
mkdir -p tls && cp /path/to/fullchain.pem /path/to/privkey.pem tls/
chgrp 65532 tls/privkey.pem && chmod 0640 tls/privkey.pem   # the container's user reads the key
docker compose up -d --build
```

`compose.yaml` runs it read-only with `/tmp` in memory, no Linux capabilities, a 256 MiB memory limit, the certificate from `./tls` and the data in the `lockdown-data` volume. It listens on port 8443.

To have the image carry the web vault, build the page first (`cd apps/web && pnpm install && pnpm build:wasm && pnpm build`), then `docker compose build`, and add `LOCKDOWN_WEB_VAULT: /usr/share/lockdown/web` to the environment; it is then at `https://<your server>/vault/`.

No image is published to a registry yet; build it from the source you checked out. That way the image is the code you can read.

### As a plain program

```sh
cargo build --release -p lockdown-server
./target/release/lockdown-server --data /var/lib/lockdown \
    --listen 0.0.0.0:443 --tls-cert /etc/lockdown/fullchain.pem --tls-key /etc/lockdown/privkey.pem
```

Run it as its own user. It makes its data directory readable only by that user.

### Every setting

Every flag has an environment variable, so a container needs no command line. `lockdown-server --help` lists them all.

| Flag | Variable | Default | What it does |
|---|---|---|---|
| `--listen` | `LOCKDOWN_LISTEN` | `127.0.0.1:8787` (image: `0.0.0.0:8443`) | Address and port. |
| `--data` | `LOCKDOWN_DATA` | `~/.local/share/snds/lockdown-server` (image: `/data`) | The database and the server key file. |
| `--tls-cert`, `--tls-key` | `LOCKDOWN_TLS_CERT`, `LOCKDOWN_TLS_KEY` | none | Serve TLS on the server itself (§2). |
| `--tls-pq-only` | `LOCKDOWN_TLS_PQ_ONLY` | off | Accept only the post-quantum key exchange (§2). |
| `--tls-reload-secs` | `LOCKDOWN_TLS_RELOAD_SECS` | 60 | How often to look for a renewed certificate on disk; 0 = only on SIGHUP. |
| `--hsts-max-age` | `LOCKDOWN_HSTS_MAX_AGE` | 31536000 (a year) | `Strict-Transport-Security` over TLS; 0 sends none. |
| `--redirect-http` | `LOCKDOWN_REDIRECT_HTTP` | none | Also listen for plain HTTP here and redirect it to HTTPS. |
| `--insecure-http` | `LOCKDOWN_INSECURE_HTTP` | off | Allow plain HTTP away from this machine: only behind a TLS proxy (§2). |
| `--trusted-proxy` | `LOCKDOWN_TRUSTED_PROXIES` (comma-separated) | none | Proxies (IPs or CIDR blocks) whose `X-Forwarded-For` is believed. |
| `--blob-dir` | `LOCKDOWN_BLOB_DIR` | none (in the database) | Keep attachment files here instead of inside the database. |
| `--signups` | `LOCKDOWN_SIGNUPS` | `open` (image: `closed`) | Whether new accounts may register. |
| `--session-ttl` | `LOCKDOWN_SESSION_TTL` | 28800 (8 h) | How long a sign-in lasts before the device asks for the password again. |
| `--max-blob` | `LOCKDOWN_MAX_BLOB` | 64 MiB | The largest attachment taken. |
| `--address-burst` | `LOCKDOWN_ADDRESS_BURST` | 30 | Sign-in steps one client address may take in a burst before the brake slows it to one every 2 s. Raise it when many people share one address. |
| `--login-burst` | `LOCKDOWN_LOGIN_BURST` | 10 | Sign-in steps one login name may take in a burst before the brake slows it to one every 6 s. |
| `--attachment-quota` | `LOCKDOWN_ATTACHMENT_QUOTA` | 5 GB (5000000000) | The attachment room per account, counted over the vaults it owns (a shared vault's files count against its owner); 0 is no limit. Clients read it at `GET /v1/account/storage`. |
| `--breach-feed-url`, `--breach-feed-key` | `LOCKDOWN_BREACH_FEED_URL`, `LOCKDOWN_BREACH_FEED_KEY` | none | Email breach monitoring through Have I Been Pwned (needs a paid key; off unless set). |
| `--breach-range`, `--breach-catalog` | `LOCKDOWN_BREACH_RANGE`, `LOCKDOWN_BREACH_CATALOG` | none | Email breach monitoring over your own data, no key needed (§8). |
| `--database-url` | `LOCKDOWN_DATABASE_URL` | none (SQLite in `--data`) | Keep everything in Postgres instead (§9). |
| `--database-ca` | `LOCKDOWN_DATABASE_CA` | the system's | The CA to trust for Postgres's TLS. |
| `--s3-endpoint`, `--s3-bucket`, `--s3-region`, `--s3-access-key`, `--s3-secret-key`, `--s3-prefix`, `--s3-virtual-hosted` | `LOCKDOWN_S3_…` | none | Keep attachment files in an S3-compatible bucket (§9). |
| `--public-url` | `LOCKDOWN_PUBLIC_URL` | none | The server's address as people reach it, for links in email (§10). |
| `--smtp-url`, `--smtp-user`, `--smtp-password`, `--smtp-from`, `--smtp-ca` | `LOCKDOWN_SMTP_…` | none | Send email (§10). |
| `--email-approval` | `LOCKDOWN_EMAIL_APPROVAL` | `on` (with SMTP) | Whether a device new to an account waits for the account's email to approve it (§10). |
| `--backup-dir`, `--backup-key-file` | `LOCKDOWN_BACKUP_DIR`, `LOCKDOWN_BACKUP_KEY_FILE` (or `LOCKDOWN_BACKUP_KEY`) | none | Back up on a schedule and drill the restores (§3). |
| `--backup-every-hours`, `--backup-keep`, `--drill-every-hours` | `LOCKDOWN_BACKUP_EVERY_HOURS`, `LOCKDOWN_BACKUP_KEEP`, `LOCKDOWN_DRILL_EVERY_HOURS` | 24, 14, 168 | How often, how many kept, how often a restore drill runs. |

**Sign-ups.** The image starts closed. To let someone in, invite their login name on the admin page (Invites) or with `lockdown-server admin invite --login them@example.com`: that one name may register, once, within a week, while sign-ups stay closed. Or set `LOCKDOWN_SIGNUPS=open`, have everyone register, then set it back to `closed` and restart. Signing in keeps working while sign-ups are closed.

## 2. TLS

Lockdown's apps only talk to a server on another machine over HTTPS. Plain HTTP is accepted for `localhost` and `127.0.0.1` only (development and tests); the server itself refuses to listen anywhere else without TLS unless you say a proxy does it.

**On the server itself (recommended).** Give it a certificate chain and key in PEM files (`--tls-cert`, `--tls-key`), for example from Let's Encrypt with `certbot` or your own CA. It then:

- speaks **TLS 1.3 only**;
- prefers the **hybrid post-quantum key exchange X25519MLKEM768** (X25519 and ML-KEM-768 together, so a recording made today can't be opened by a quantum computer later) and falls back to X25519 for older clients such as an older browser. `--tls-pq-only` refuses every client without the hybrid exchange; Lockdown's own apps always have it;
- sends `Strict-Transport-Security` so a browser that reached it once never tries plain HTTP again;
- **reloads the certificate** when the files change (checked every minute) or on `SIGHUP` (`docker compose kill -s HUP lockdown`), with no restart and no dropped connections. A renewal that doesn't load (a key that doesn't match, a half-written file) is reported in the log and the old certificate keeps serving;
- with `--redirect-http 0.0.0.0:80`, answers plain HTTP with a redirect to the same address over HTTPS (and `/healthz` in place, for load balancers).

Certificate renewal (ACME) is not built in: run `certbot` or your proxy's ACME next to it and point `--tls-cert`/`--tls-key` at the files it renews.

**Behind a reverse proxy.** If a proxy (Caddy, nginx, Traefik) does TLS, run the server with `--insecure-http` on a private network and tell it which proxy to believe for client addresses with `--trusted-proxy 172.16.0.0/12` (the login brake and the session list then see real addresses). Configure the proxy for the hybrid exchange where it can (recent OpenSSL 3.5, BoringSSL and Go builds have X25519MLKEM768). The live channel (`/v1/events`) is a long-lived response: turn response buffering off for it (nginx: `proxy_buffering off;`, or rely on the `X-Accel-Buffering: no` the server sends) and allow reads of at least a minute.

**Your own CA.** If the certificate is from a private CA, the apps must be told to trust it: set `LOCKDOWN_CA_CERT=/path/to/ca.pem` for the CLI and the desktop app. Lockdown then trusts the certificates in that file **and nothing else** for every server, a pin rather than an addition to the system's store.

## 3. Backup and restore

The server's data is all ciphertext and signed records, plus one secret file: `server-key.cbor`, which signs sessions and runs the password protocol. Lose it and every account's password stops working. So a backup is the database **and** that file, and the backup is encrypted again under a **Backup Key** only you hold.

```sh
# Once: make a Backup Key and write it down with your recovery kit.
docker compose exec lockdown lockdown-server backup keygen
#   LDBK-ABCD-EFGH-…  (shown once; the server doesn't keep it)

# Each time (safe while the server runs):
docker compose exec -e LOCKDOWN_BACKUP_KEY='LDBK-…' lockdown \
    lockdown-server backup create --out /data/backups/2026-10-18.ldbk
docker compose exec -e LOCKDOWN_BACKUP_KEY='LDBK-…' lockdown \
    lockdown-server backup verify --in /data/backups/2026-10-18.ldbk
```

`create` takes a consistent copy of the database while the server keeps running, the key file and every attachment file, and writes one sealed file (never over an existing one). `verify` opens it, checks every file's hash and opens the database copy, changing nothing: run it on a schedule as a restore drill. The key can also come on standard input instead of `LOCKDOWN_BACKUP_KEY`.

Copy the backups somewhere else: three copies, two kinds of storage, one away from home (05-self-host §4). Without the Backup Key a backup is noise; with it, it is the whole server.

**The operator's recovery kit.** `lockdown-server backup keygen --kit operator-kit.html --public-url https://vault.example --backup-dir "the NAS, folder lockdown"` also writes a printable page with the Backup Key in large type, every character captioned, and the restore steps. Print it, put it with your own recovery kit, and delete the file. For a key you already have: `lockdown-server backup kit --kit operator-kit.html` with the key on standard input.

**On a schedule, with restore drills.** Give the server a folder and the key and it does both by itself:

```sh
LOCKDOWN_BACKUP_KEY_FILE=/run/secrets/lockdown-backup-key \
  lockdown-server --data /data --backup-dir /data/backups   # a backup a day, 14 kept, a drill a week
```

A **restore drill** opens the newest backup with the Backup Key, restores it into a scratch database (a temporary SQLite file, or a scratch schema in your Postgres), and checks it against the running server: every file's hash, the server key file, the public log (the backup's must be the start of the live one), every vault's chain of heads, every attachment. The result is in the log and on the admin page (Backups; "last successful restore" on the overview, a red banner while the newest drill failed). The server then holds the Backup Key in memory while it runs; leave `--backup-dir` off and run `backup create` from your own scheduler if you'd rather it never did.

**Restore** into an empty data directory (it refuses one that already has a server), then start the server on it:

```sh
LOCKDOWN_BACKUP_KEY='LDBK-…' lockdown-server backup restore --in 2026-10-18.ldbk --data /var/lib/lockdown-new
```

Devices notice nothing: the same accounts, the same signing keys, the same public log. A restore from an older backup looks to devices like a server that went back in time, and they say so ("Rollback attempt blocked"); that is the protection working. Push again from a device that has the newer data.

Backups of a Postgres server are the same file: `backup create --database-url …` copies every row into the snapshot, and `backup restore --database-url …` restores into an empty Postgres database, so a backup also moves a server from SQLite to Postgres or back.

## 4. Upgrades and the compatibility window

- **Same release, same day.** The self-host server is the cloud server; fixes reach both in one release.
- **Upgrading.** Stop the server, take a backup, start the new version. Database changes are forward-only and run on start; a newer database refuses to open in an older server ("the database is from a newer server"). The way back from a bad upgrade is the backup.
- **Compatibility.** A Lockdown app supports the current server release and the two before it. The server says its release in `/.well-known/lockdown` (`version`), so an app that is too new can say which server release it needs instead of failing.
- **Certificates and settings** change without an upgrade: replace the files, or change a variable and restart.

## 5. What the operator can see

Running the server does not let you read anyone's vault: not passwords, titles, addresses, usernames, notes, attachments or the names of folders. That is enforced by the apps, not by policy.

What the server does see, and so what its operator can see:

| What | Why the server has it |
|---|---|
| Each account's **login name** (often an email address) | It is what you sign in with. |
| **Devices** of each account: an id, a public key, when it first signed in | So only your own devices can write. Device names are encrypted. |
| **Sessions**: device, address, client version, when started and last used | So you can see where you're signed in and end a session (Devices → Signed in). Kept until a week after each session runs out. |
| **When** things change, and how many encrypted items and versions a vault has, and their sizes | It stores and orders them. It can't tell what they are. |
| The **public log** of keys and vault checkpoints | Anyone may read it; that is how devices catch a lying server. |
| Who **shares** a vault with whom (accounts, not contents) | It delivers the shares. |
| Account alerts (a new device signed in, a security key added or removed, a breach, when monitoring is on) | It raises them, and with email set up, mails them. |
| Invites, and the operator's log (sign-ins to `/admin` and from where, backups, restore drills) | You keep them, on the admin page. |

It never sees your account password (the sign-in protocol, OPAQUE, proves it without sending it), your Secret Key, your recovery code, or any key that opens a vault. A stolen copy of the server's database doesn't give anyone a head start on guessing passwords: they would also need each account's Secret Key, which never leaves the devices.

**Live sync.** Signed-in apps keep one connection open (`/v1/events`) so a change on one device shows on the others within a second. The messages say only "something changed", never what; the app then syncs and checks everything as usual.

**Mobile push** (phones learning about changes while the app is closed) needs a relay signed with the app store keys and is not part of the self-host server yet.

## 6. Health and monitoring

- `GET /healthz` answers `ok`.
- `lockdown-server healthcheck` exits 0 when the local server answers (the image's `HEALTHCHECK` uses it).
- The log (standard error) says where it listens, whether TLS is on and with which key exchange, certificate reloads, and failures. It never contains a token, a key or anything from a vault.

## 7. Memory and disk

Measured on the container image (x86-64 Linux, release build, read-only root, TLS on, 2026-10-18) with `docker stats`, one household:

| | Memory (cgroup) |
|---|---|
| Idle, just started | 5.5 MiB |
| After two devices registered, recovered and made and synced 21 logins (41 syncs, each with its key-transparency checks) | 6.2 MiB |

The image is 53 MB on disk (14 MB compressed). `compose.yaml` caps it at 256 MiB, far above what it uses; the 512 MiB floor in 05-self-host §8 is generous for the server alone.

Disk: the database grows by a few kilobytes per item version; attachments add their own size (with `--blob-dir`, as files beside the database). Give it 10 GB plus your attachments.

## 8. Email breach monitoring without a paid key

The server can warn people when their email address (and the aliases they ask it to watch) shows up in a breach. Have I Been Pwned's address lookups need a paid key (`--breach-feed-url`). Instead, point it at breach data of your own:

- **A range file** (`--breach-range /data/breach-range.txt`): one line per address, `SHA1(lower-cased address in hex):Breach;Breach`. It is read on this machine and read again when it changes; nothing leaves the server.
- **A range service** (`--breach-range https://breaches.example/range`): the server asks `GET <url>/<the first six hex digits of the hash>` and gets back `<the other 34 digits>:Breach;Breach` lines (404 for none). Only those six digits leave the server, which many addresses share (k-anonymity, as Pwned Passwords works).

`--breach-catalog breaches.json` (Have I Been Pwned's free breach list, `GET https://haveibeenpwned.com/api/v3/breaches`, saved to a file) adds each breach's site, date and what was taken to the alerts.

## 9. Postgres and an S3-compatible bucket

**Postgres.** `--database-url postgres://lockdown:PASSWORD@db.example/lockdown` keeps everything in Postgres instead of the SQLite file (the server key file still lives in `--data`). The tables are made on first start. Away from this machine the connection must be TLS (TLS 1.3; `sslmode=require` is implied; `--database-ca` names a private CA); `sslmode=disable` is accepted only for a database on this machine. One server per database: a second one refuses to start (an advisory lock). The admin page says whether the connection is encrypted.

**Attachments in a bucket.** `--s3-endpoint https://s3.example --s3-bucket lockdown-attachments` with `LOCKDOWN_S3_ACCESS_KEY` and `LOCKDOWN_S3_SECRET_KEY` keeps attachment files in any S3-compatible store (MinIO, Garage, Ceph, AWS). The bucket must exist; the server checks it can use it at start. Only the encrypted bodies go there, named by their own hash under the vault's id; their sizes and who uses them stay in the database, so the attachment room per account works the same. `--s3-prefix lockdown/` keeps them under a folder; `--s3-virtual-hosted` for stores that want `bucket.host` addresses. Plain `http://` only to a store on this machine.

## 10. Email: approving new devices, and alerts

With `--smtp-url smtps://mail.example:465` (TLS from the start) or `smtp://mail.example:587` (STARTTLS, required unless the relay is this machine), `--smtp-from "Lockdown <lockdown@example.com>"`, the relay account in `LOCKDOWN_SMTP_USER` and `LOCKDOWN_SMTP_PASSWORD`, and `--public-url https://vault.example` (the address links in email point at; never taken from a request):

- **A new device waits for the account's email.** When a device the account has never seen signs in with the right password, the server mails the account's address a link. The page behind it shows the device and its address and asks Approve or "This wasn't me"; only Approve lets the device in. The link works once, for fifteen minutes; at most five go to one account an hour. Accounts with security keys use those instead; the account's first device isn't asked; login names that aren't addresses aren't mailed. `--email-approval off` keeps the alerts and drops the wait.
- **Alerts by email.** A new device signing in, and a security key added to or removed from the account, are mailed to the account, with a link to where its sessions can be ended (`/vault/#/devices`). The apps show the same alerts, and the desktop app as a notification.

Messages are plain text: the device's own name, the address it came from, the time and links to this server. Never a password, a key, a token or anything from a vault (the server has none of it). The log says which kind of message went, never to whom.

## 11. The admin page

`https://<your server>/admin`, behind an **operator key** that is not an account:

```sh
lockdown-server admin set-key --data /data        # prints a new key once (or --stdin to type one)
lockdown-server admin clear-key --data /data      # closes the page
```

Only the key's hash is kept. Signing in gives the browser a cookie for eight hours (an hour unused); every form carries its own anti-forgery token. It shows the release, where the data and attachments live, the setup checks, each account (login name, devices, live sessions, vaults, attachment room used, security keys), invites, backups and drills. It can invite a login name, end an account's sessions, and run a backup or a drill now. It never shows a vault, a key or a token: the server has none. Put it behind the same TLS as everything else; if only you use it, your proxy can also limit `/admin` to your own network.

