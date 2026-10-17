# Lockdown — big picture

_Status: plan set v0.1, 2026-10-15. Start here. Then read the rest of `docs/plan/` in number order._

## For future agent

- **TL;DR:** Lockdown is a cross-platform password manager, passkey provider, and authenticator that meets the incumbents on every feature and beats them on trust. It is one of three apps in an ecosystem (with Mailbird and Synapse) that each work alone and work better together. The decisions are in `decisions.md`; the evidence is in `../research/2026-10-15/`.
- **Where we are:** research done, one hundred and fourteen decisions recorded. The identity crate lives at `snds/ecosystem-identity` (D34). The Rust workspace (`07-workspace.md`) now has a SQLite vault store, the identity adoption path, a server with OPAQUE login and verifiable sync, the extension matcher boundary with signed data and a browser red team, and a recovery kit with the Secret Key in the account slot, identity records synced and device revocation enforced on both sides, attachments, a key-transparency log the clients audit and gossip, the extension matcher boundary with a browser red team, a desktop shell on unicron 0.6.0 with its six security components bound to the core, every item kind made and changed from the core's per-kind templates, a password generator and the in-app quick-access palette (`10-desktop-shell.md`), and a CLI that drives all of it; 216 Rust tests and 46 web UI tests. The design brief (`11-desktop-design-brief.md`) is decided through D99 (D100–D114 are the building sessions' rows); the six security component specs live in unicron (`docs/specs/security/`, as built). Security-model details are decided by the building session under D38 and batched as audit items; the rows flagged for Sean are the user-visible ones.

## What it is

One app for every way a person signs in: passwords, passkeys, one-time codes, and TV or console activation codes, on iOS, Android, Mac, Windows, Linux, every browser, and a home server if they want one.

## What it promises

1. **Every way you sign in, in one place** (positioning A). The breadth a person feels in week one.
2. **It can't betray you** (B). Designed so that a hostile server cannot read, swap, or roll back a vault, and the user can check. This is the proof behind A.
3. **Open, fair, yours** (C). Open-source apps, a self-hosted server that runs the same code as the cloud, free export forever. This is policy, not a slogan.

Tagline: **Every way you sign in. Locked down, even from us.**

## What makes it different

| Lockdown does | The incumbents |
|---|---|
| Assumes its own server is hostile and proves it (threat model from the 2026 ETH Zurich study) | Trusted the server's word for public keys; 12 attacks found on Bitwarden, 7 on LastPass, 6 on Dashlane, 2 on 1Password |
| Wraps shared keys with hybrid post-quantum encryption from day one | Post-quantum only on the network connection, if at all |
| Keeps a signed, verifiable history of every vault | No verifiable history anywhere |
| Is an OS passkey provider, so "scan the QR on the laptop, approve on the phone" works for every passkey site | Same, where they ship it |
| Approves TV and console sign-ins by scanning the screen, with a hard anti-phishing rule | Nobody |
| Shares one identity with Mailbird and Synapse without either needing the other | Standalone |

## What it is not

- Not an imitation of vendors' private phone apps. Battle.net, Adobe, Discord, and all push channels are declined in favour of passkeys and codes. Steam is parked in the "maybe" backlog (D8).
- Not a thin wedge. v1 aims for parity across the whole feature matrix (D9), which makes v1 larger and longer than a typical first release. The roadmap says what that costs.

## The plan set

| File | What it settles |
|---|---|
| `00-big-picture.md` | this page |
| `00-ecosystem.md` | the Lockdown × Mailbird × Synapse contract |
| `01-identity.md` | the shared identity crate, format v1 |
| `02-model.md` | vaults, items, the crown-jewel class, manifests |
| `03-architecture.md` | Rust core, bindings, shells, extension, server, sync |
| `04-security.md` | threat model, key hierarchy, login, recovery, as a spec |
| `05-self-host.md` | the single binary and the private-cloud bundle |
| `06-roadmap.md` | phases, team, exit criteria under D9 |
| `07-workspace.md` | the Rust workspace as built: crate map, what is real, the next five steps |
| `decisions.md` | the decision log |

## Walls

- No employer names, tokens, paths, data, or code in this repo.
- Nothing here is legal advice. The Steam bridge, if it ever ships, needs US and EU counsel first.
