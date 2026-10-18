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
| `--breach-feed-url`, `--breach-feed-key` | `LOCKDOWN_BREACH_FEED_URL`, `LOCKDOWN_BREACH_FEED_KEY` | none | Email breach monitoring (needs a paid Have I Been Pwned key; off unless set). |

**Sign-ups.** The image starts closed. To let your household register, set `LOCKDOWN_SIGNUPS=open`, have everyone register, then set it back to `closed` and restart. Signing in keeps working while sign-ups are closed.

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

**Restore** into an empty data directory (it refuses one that already has a server), then start the server on it:

```sh
LOCKDOWN_BACKUP_KEY='LDBK-…' lockdown-server backup restore --in 2026-10-18.ldbk --data /var/lib/lockdown-new
```

Devices notice nothing: the same accounts, the same signing keys, the same public log. A restore from an older backup looks to devices like a server that went back in time, and they say so ("Rollback attempt blocked"); that is the protection working. Push again from a device that has the newer data.

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
| Account alerts (a new device signed in, a breach, when monitoring is on) | It raises them. |

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
