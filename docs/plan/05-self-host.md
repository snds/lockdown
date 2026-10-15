# Self-host offer

_Status: draft v0.1, 2026-10-15. Implements D7 and the "open, fair, yours" layer of D1. Evidence: research brief §5.6 and §7.2, `reports/02-bitwarden-proton-others.md` §2 (Vaultwarden), `verification.json` cluster `bitwarden-proton-vaultwarden`. Security rules are in `04-security.md` and are not repeated._

## For future agent

- **TL;DR:** Self-hosting is a value-add, not the default (D7). The self-host server is the cloud server: one Rust binary, one container image, features switched by flags. It stores only ciphertext and signed metadata. A private-cloud bundle hosts Lockdown and Mailbird together with a mail server, DNS helpers and a backup job. Lockdown owns identity, secrets and the backup-encryption primitives; Mailbird owns the mail server, DNS tooling and the bundle's 3-2-1 backup plumbing. Two things a home server cannot do alone, push to phones and public key-transparency checkpoints, go through small content-free relays that Lockdown runs and the self-hoster can opt out of.
- **Why this shape:** Vaultwarden proves the demand for a single Rust binary and shows the trap of a parallel, unaudited implementation (§11).
- **Read with:** `04-security.md` §6–§7 (KT and manifests), `00-ecosystem.md` §5, `decisions.md` D7.

## 1. One binary, two deployments

| Property | Rule |
|---|---|
| Code | The cloud build and the self-host build are the same commit, the same binary, the same image. No fork, no "community edition". |
| Stack | Rust, `axum`, `sqlx`; SQLite for one household, Postgres for more; S3-compatible storage for attachments (brief §5.6) |
| Configuration | Flags and environment variables select: database, storage, push relay on or off, KT cross-publish on or off, signups open or invite-only, organization features, telemetry (off by default in self-host) |
| What differs in cloud | Only operational pieces around the binary: billing, the public KT mirror, the push relay itself, horizontal scaling. None of these live in the binary's trust boundary. |
| Clients | Every Lockdown app accepts a custom server URL at sign-in and pins its KT log key after the first contact. |
| Reproducible | The image is built reproducibly and signed (04-security §15), so a self-hoster can confirm the image matches the tagged source. |

## 2. What the server stores

Everything below is either ciphertext the server cannot open, or signed metadata the server cannot forge. There is no plaintext field anywhere.

| Store | Contents | Readable by server? |
|---|---|---|
| OPAQUE registration records | Per account; lets the server run the login protocol | No password, no salt, no key material usable offline (04-security §3) |
| Key envelopes | Account-slot, kit-slot and device-wrapped IRK envelopes; persona, ADK, vault and item key wraps | No; every envelope is AEAD with bound AAD |
| Item and field ciphertext | Every field, every metadata field, attachments | No |
| Vault manifests | Signed, hash-chained tables of item IDs and versions | Metadata only: counts and timing. Cannot forge or reorder. |
| Signed profile records | Profile, personas, device records with attested public keys, the signed Argon2id parameters | Public keys and labels encrypted; signatures verifiable |
| Key-transparency log | Append-only Merkle tree of blinded key entries; signed checkpoints | Blinded entries; the server cannot read who is who without the profile identifier **[uncertain]** (04-security §6) |
| Sync cursors and push tokens | Per device; opaque | Yes, but content-free |
| Agent audit logs | Encrypted under the ADK, hash-chained | No |
| Operator settings | Admin token hash, SMTP settings, feature flags | Yes; these are the operator's own |

What the server never stores: passwords, Secret Keys, recovery codes, unwrapped keys, plaintext titles or URLs, or any telemetry about item contents.

## 3. The private-cloud bundle

One compose file and one Helm chart. The pieces:

| Component | Job | Whose scope (D7) |
|---|---|---|
| Lockdown server | Vaults, sync, KT log, manifests, agent audit storage | Lockdown |
| Mailbird server | Mail sync relay, note storage, Mailbird's own records | Mailbird |
| Mail server | SMTP in and out, IMAP/JMAP, spam filtering | Mailbird |
| DNS helpers | Publishes MX, SPF, DKIM, DMARC, and TLS records; certificate renewal | Mailbird |
| Identity crate services | Shared identity records, device enrolment, recovery kit handling for both apps (01-identity) | Lockdown |
| Backup job | Scheduled snapshot, encryption, copy to two media and one off-site target, verification, restore drill | Mailbird runs the job; Lockdown supplies the encryption primitives and the Lockdown-side restore checks |
| Push relay client | Sends content-free pings through Lockdown's relay (§5) | Lockdown |
| Reverse proxy | TLS termination with hybrid post-quantum TLS where the proxy supports it | Bundle |

Each app works alone (D2). The compose file has profiles: `lockdown`, `mailbird`, `both`. Running only `lockdown` gives a password-manager server with no mail parts at all.

The bundle needs HTTPS. Browsers block WebCrypto on plain HTTP except on localhost, which is why Vaultwarden has the same requirement.

## 4. 3-2-1 backup

Three copies, two media, one off-site. The backup is useless if the user cannot restore it, so restore is a feature, not a document.

| Element | Design |
|---|---|
| What is backed up | Database, attachments, the KT log, the signed profile directory, and operator settings. All of it is already ciphertext or signed metadata (§2). |
| Encryption at rest | The whole snapshot is encrypted again with a **Backup Key** held by the user, never by the server. The Backup Key is wrapped to the IRK (01-identity §4) and printed in the recovery kit as an extra line, so the kit alone can restore a dead server. |
| Primitives | XChaCha20-Poly1305 with AAD binding the snapshot ID, server ID and schema version; hybrid HPKE wrap of the Backup Key to each enrolled device; Argon2id over a backup passphrase as an optional second slot. Supplied by Lockdown as a small crate the backup job calls. |
| Three copies | Live database; local snapshot on a second disk or NAS; off-site snapshot (S3-compatible bucket, a friend's server, or a cloud drive). |
| Two media | The job refuses to mark a backup healthy if both snapshot targets resolve to the same physical device. |
| Verification | Every snapshot carries a signed manifest of its own file hashes. The job re-reads each copy, checks the hashes, and checks that the newest vault manifest head inside the snapshot is not older than the last one the live server published (anti-rollback for backups). |
| Restore drill | Monthly by default: restore the newest snapshot into a throwaway container, run the integrity checks, start a Lockdown client in test mode against it, open one test vault with a test identity, tear it down. The result shows in the admin page as "last successful restore: <date>". A failed drill is a red banner on every admin login. |
| Retention | Daily for 14 days, weekly for 8 weeks, monthly for 12 months, configurable. Crypto-shredded versions stay shredded: the per-version keys are not in the backup. |
| The Aegis lesson | A "last successful backup" indicator is visible in the client, not just on the server. |

## 5. The push relay for self-hosters

Apple and Google accept push messages only when they are signed with the app publisher's keys. A home server does not hold those keys. Without a relay, phones learn about changes only when the app opens. Vaultwarden routes this through Bitwarden's relay, which requires an installation ID from Bitwarden.

| Rule | Detail |
|---|---|
| Content | A ping is `{device_token, "sync now"}`. No account identifier, no vault identifier, no counts. |
| Registration | The self-host server registers once with Lockdown's relay and gets an opaque server ID. No email, no payment. |
| Opt-out | Push can be off. Clients then fall back to WebSocket on desktop and background fetch on mobile, which is slower but relay-free. |
| What the relay learns | That some server pinged some device at some time. The relay keeps no history beyond delivery retries. |
| Abuse | Per-server rate limits; a server that floods is throttled, never blocked silently. |
| Trust | The relay cannot forge a sync result. A client that receives a ping still verifies manifest and KT exactly as if it had polled. |

## 6. Key transparency and anti-rollback in self-host

The self-host server runs the same KT log and publishes the same signed checkpoints as the cloud (04-security §6–§7). The difference is who watches it.

| Question | Cloud | Self-host |
|---|---|---|
| Who audits the log | Lockdown's public mirror, outside auditors, every client's self-audit | The user's own clients (self-audit on every sync); a second device, which catches a server that shows different histories to different devices |
| Weak spot | A dishonest operator with many users | An operator who is also the only user, and a server that can rewrite its own history when no second device is watching |
| Cross-publish (optional, default on) | — | The server posts each signed checkpoint (log size, root hash, manifest heads: all hashes) to Lockdown's public checkpoint log. A rewritten home server then disagrees with a public record it cannot change. |
| Privacy cost of cross-publish | — | Timing of activity is visible to the public log. Content is not. The admin page states this in one sentence next to the switch. |
| Offline self-host | — | With cross-publish off, protection rests on the clients' stored checkpoints. The client shows "no outside witness" in its security page so the user knows. |

Rollback behaviour and the user-facing wording are identical to the cloud (04-security §7).

## 7. Upgrade path and parity promise

| Promise | Detail |
|---|---|
| Same release, same day | Security fixes ship to cloud and self-host in one release. There is no "cloud first" window. |
| Client compatibility | A client supports the current server release and the previous two. The server reports its version; a client that is too new says so in plain language and names the server version needed, instead of failing like Vaultwarden's lockstep breaks. |
| Migrations | Forward-only, run by the binary on start, with a pre-migration snapshot taken by the backup primitives. |
| Downgrade | Not supported across a migration; the snapshot is the way back. |
| Deprecation | A feature flag is announced one release before removal and lives in the changelog. |
| Verification | Each release's SBOM, SLSA provenance and signature apply to the self-host image too (04-security §15). |
| Timing | The brief's cut list (§6) allows the self-host image to ship a quarter after the cloud launch if v1 slips. The binary is the same from day one; only the bundle and docs may lag. |

## 8. Minimum hardware

Argon2id runs on the client, OPAQUE's server side is cheap, and the data is small, so the server itself is light. The mail server is the heavy part.

| Deployment | CPU | RAM | Disk | Notes |
|---|---|---|---|---|
| Lockdown only, one household | 1 vCPU | 512 MiB (1 GiB recommended) | 10 GB + attachments | SQLite. A Raspberry Pi class device is enough **[uncertain]** until measured. |
| Lockdown only, small organization | 2 vCPU | 2 GiB | 20 GB + attachments | Postgres; the KT log grows with members and devices. |
| Full bundle (both apps + mail) | 2–4 vCPU | 4 GiB | 40 GB + mail | Mail filtering dominates. Figures are Mailbird's to confirm **[uncertain]**. |

Supported: any Linux host with Docker or a Kubernetes cluster; x86-64 and arm64 images.

## 9. Threat differences between cloud and self-host

The client-side guarantees are identical: the server is hostile in both cases, and the proofs do not depend on where it runs. What changes is who holds the operational risk.

| Threat | Cloud | Self-host |
|---|---|---|
| Hostile server | Lockdown is the suspect; public KT mirror and audits watch it | The user's own hardware is the suspect; self-audit, a second device and cross-publish watch it |
| Stolen database | Attacker needs the Secret Key per account; Lockdown handles notification | Same cryptography; the user handles notification and rotation |
| Availability | Lockdown's uptime and backups | The user's uptime and 3-2-1 discipline (§4) |
| Lost server, no backup | Cannot happen to the user's data | Total loss unless the kit holds the Backup Key and an off-site copy exists; the drill exists for this |
| Push and KT witnessing | Built in | Through content-free relays, or degraded with a visible warning |
| Operator mistakes | Lockdown's on-call | Exposed admin page, expired TLS, open signups. The bundle ships closed by default and the admin page scores the setup. |
| Legal requests | Lockdown receives them and reports them | The user receives them; there is nothing readable to hand over |

## 10. What the operator sees

An admin page at `/admin`, behind an Argon2id-hashed token and, when the operator enrols one, a passkey. It shows: version and update availability, the setup score, last backup and last restore drill, KT checkpoint status and whether cross-publish is on, push relay status, users and devices, and feature flags. It never shows vault contents, because it cannot.

## 11. Vaultwarden: the cautionary comparison

Vaultwarden is an unofficial Rust server for Bitwarden's clients. It is the clearest evidence that self-hosters want a single small binary, and the clearest warning about how not to deliver one.

| Vaultwarden got right | Lockdown keeps |
|---|---|
| One Rust binary, about 50 MB of RAM, one container, SQLite or Postgres | The same shape, as the official server |
| Every premium feature free when self-hosted (attachments, TOTP, emergency access, SSO) | No feature is paywalled in self-host |
| Argon2-hashed admin token; trusted-proxy support; S3 storage since 1.37 | Same, plus a passkey on the admin page |
| Immutable releases with attestation since 1.35.0 | Reproducible, signed, attested images from the first release |
| A large, loyal community proving the demand | The reason D7 exists |

| Vaultwarden got wrong, or could not help | Lockdown's answer |
|---|---|
| A parallel implementation of someone else's API; new client versions force server upgrades (1.37.0 required for clients 2026.7+, 1.37.2 for 2026.8+) | One codebase; a documented client-server compatibility window (§7) |
| Not audited by a third party | Annual audit covers the server; the self-host image is the audited binary (04-security §16) |
| Rising advisory volume in 2026: 13 advisories in three months before 1.36, nine in 1.37.0, seven in 1.37.4, CVEs including an 8.1 refresh-token flaw and WebAuthn flag tampering | Same binary gets the same fixes the same day; formal models for the protocol parts (04-security §17) |
| Still lacks passkey login and new-device protection as of 1.37.4 (verified) | Passkey and PRF slots, device enrolment and KT are in the core, not add-ons |
| Push depends on Bitwarden's relay and an installation ID from Bitwarden | A first-party, documented, content-free relay with a clean opt-out (§5) |
| Small maintainer team; licence or policy risk if the client API changes | Lockdown owns the client and the server; the licence question is open (§12) |
| Backups are "copy the database file and the attachments folder" | Encrypted 3-2-1 with verification and a restore drill (§4) |

## 12. Open questions

1. **One name or two products.** Does the bundle ship under one name, or as Lockdown and Mailbird with a documented pairing? One name is simpler to market and harder to keep honest about D2 ("each works alone"). Recommendation: two products, one bundle repository, one install command **[uncertain]** until the ecosystem has a name (01-identity §11).
2. **Licence for the server that keeps self-hosting free.** The apps take an OSI-approved licence (D1 layer C). For the server, AGPL-3.0 keeps every hosted fork open; a source-available licence protects revenue but weakens the trust claim, as 2FAS's BUSL did (brief §7.2). Needs Sean's call before the first public commit of server code.
3. Whether cross-publish of checkpoints (§6) is on or off by default. This doc says on, with the privacy sentence shown.
4. Who owns the restore-drill schedule and alerts when the bundle runs only Lockdown: the Lockdown server must then carry the backup job itself.
5. Hardware figures (§8) need measurement on a small ARM board and on a 2 GiB VPS.
6. Whether the self-host image can ship at launch or a quarter later (§7), given D9.
