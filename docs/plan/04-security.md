# Security specification

_Status: draft spec v0.1, 2026-10-15. Implements D1 (layer B), D4, D5, D6, D11. Builds on `01-identity.md` §3 and never redefines it. Evidence: research brief §4 and §2.9, `reports/04-foundation.md` §4, `verification.json` cluster `security-research` (where it disagrees with the brief, it wins)._

## For future agent

- **TL;DR:** Lockdown assumes its own server is hostile and proves it cannot read, swap, or roll back a vault. Four mechanisms carry that promise: a key tree rooted in the identity crate, OPAQUE login mixed with a Secret Key, AEAD that glues every ciphertext to its place, and two public records (a key-transparency log and a hash-chained vault manifest) checked on every sync. Everything else here keeps those four honest.
- **Read with:** `01-identity.md` §3–§4 (the keys this doc wraps), `02-model.md` §2 and §8 (the vault, item and manifest detail; §2 here names the same layers), `decisions.md`.
- **Evidence status:** the 2026 ETH Zurich / USI study (Scarlata, Torrisi, Backendal, Paterson; USENIX Security 2026; ePrint 2026/058) is confirmed. The "NDSS 2025 IPC theft paper" is **[uncertain]** and not cited. Bitwarden's "7 fixed, 3 design decisions" is **[uncertain]**.

## 1. Threat model

The rule is the ETH model: the server is fully malicious. It may lie, swap keys, replay, drop, reorder, and collude with insiders. The apps must catch it.

| Adversary | Controls | Lockdown guarantees | Mechanism |
|---|---|---|---|
| Hostile server, live | Every byte sent; timing | Cannot read a vault, swap a key, or roll back, hide, or reorder items unnoticed | §3, §5, §6, §7 |
| Stolen server database | All ciphertext and wrapped keys | No offline password guessing; wraps stay sealed even against a future quantum computer | Secret Key (§3); hybrid wraps; Argon2id floor (§4) |
| Stolen device, unlocked | One device, one persona | Crown jewels still need a fresh biometric; export needs the password; other devices can revoke it | §10; revocation via manifest and log |
| Stolen device, locked | Disk and keystore at rest | Nothing readable without the device unlock | IRK wrapped only to the keystore key |
| Malicious browser page | DOM, hidden frames, overlays | No fill the user did not see | §12 |
| Malicious local process | Same machine, same user | Cannot reach the core, extension, or a sibling app without proving identity | §13, D4 |
| Coerced user | Physical pressure | Recovery paths have a waiting period and veto; crown jewels can be "this device only" | §8, §10 |
| Lost all devices | Only paper and people | Full recovery from the kit, k-of-n shares, or trusted contacts | §8, §9 |
| Malicious insider | Server, build pipeline, release keys | Same as hostile server; a swapped build or key is detectable | §15; §6 |
| Quantum adversary, later | Recorded traffic and stored wraps | Symmetric data already safe; each wrap needs both X25519 and ML-KEM-768 broken | Hybrid HPKE; ML-DSA slot reserved |

Out of scope for v1: a compromised OS kernel or keystore, and a user who shares their own vault on purpose.

## 2. Key hierarchy

The tree is defined once in `01-identity.md` §3 (kit, IRK, ISK, Device Key, Persona Key, App Data Key, Crown-Jewel Key). This doc adds the layers below Lockdown's App Data Key (ADK).

| Layer | Key | Wrapped by | Notes |
|---|---|---|---|
| Vault Key (VK) | random 256-bit, one per vault | ADK for own vaults; hybrid HPKE to a peer for shared vaults | Rotated when a member leaves |
| Item Key (IK) | random 256-bit, one per item | VK | Sharing or deleting one item never re-encrypts the vault |
| Version Key | one per item version | IK | Deleting it crypto-shreds that version (§7) |
| Field ciphertext | — | IK, XChaCha20-Poly1305 with AAD | Every field and every piece of metadata (§5) |

Unlock slots open the IRK, never a lower key: password + Secret Key, platform biometric, passkey or hardware key with PRF, recovery kit. Adding a slot never weakens another. Biometric-only users get a periodic password reminder. A PIN is a real slot, never a screen lock (the Ente Auth lesson).

All metadata is encrypted: titles, URLs, usernames, tags, icons, the "sign in with" map. Icons load through a privacy proxy. LastPass's unencrypted URLs turned a stolen database into a target list.

## 3. Login: OPAQUE plus the Secret Key

OPAQUE (RFC 9807, July 2025, an Informational CFRG document) lets the client prove it knows the password without ever sending the password or its salt, so a stolen database gives no head start on guessing. WhatsApp runs it in production; the Rust crate is `opaque-ke`. Confirmed in `verification.json`.

The Secret Key (SK) is a random 128-bit value made on the first device. It is on by default with a plain-language opt-out. It never reaches the server. It is half of the recovery kit (D11).

| Step | Where | What happens |
|---|---|---|
| 1 | Client | User types the password; client runs the OPAQUE OPRF round with the server. |
| 2 | Client | OPAQUE's stretching step is **Argon2id** (§4) over the OPRF output. The "salt" is the server-held OPRF key, so no salt is exposed. |
| 3 | Both | OPAQUE completes. Client holds `export_key` (64 bytes); the server stores only the registration record. |
| 4 | Client | `AUK = HKDF-SHA-512(ikm = export_key ‖ SK, salt = profile_id, info = "lockdown/auk/v1")`. With SK opted out the field is empty; the format stays one. |
| 5 | Client | AUK opens the account-slot `KeyEnvelope` holding the IRK (01-identity §4). The identity crate takes over. |
| 6 | Client | AUK and `export_key` are zeroized. The IRK stays in locked memory until lock. |

**[uncertain]** 01-identity §4 has no `wrapped_by` kind for a password slot. Proposal for the crate owner: add `account`, additive under 01-identity §9. The OPAQUE caveat (ePrint 2023/843: a corrupt server may get a few extra guesses) is covered by rate limits and the Secret Key.

## 4. Argon2id calibration and the signed floor

| Surface | Floor | Calibration target |
|---|---|---|
| Desktop | 64 MiB, 3 passes, 4 lanes (01-identity §3) | about 1 s at enrolment |
| Mobile | 32 MiB, 3 passes, 2 lanes **[uncertain]** | about 1 s, re-measured per device |
| Extension (WASM) | 32 MiB, 3 passes, 1 lane **[uncertain]** | about 1.5 s; WASM lacks threads by default |

Parameters are chosen on the device and stored in the profile **signed by the ISK**. The server relays and cannot change them. Readers refuse anything below the floor, and anything lower than the last set they accepted; parameters only rise. A device that cannot meet the floor is told so and offered another device or the kit; the floor never drops (Ente Auth needed 1 GB and failed on small phones). OWASP's 19 MiB / 2 passes is a server number and is too low here.

## 5. AEAD and associated-data binding

Every ciphertext is XChaCha20-Poly1305 with associated data (AAD): data that is authenticated but not encrypted. AAD glues a ciphertext to the one place it belongs.

| Rule | Why |
|---|---|
| AAD = canonical CBOR of `{profile_id, persona_id, vault_id, item_id, version_n, field_id, kind, schema_v}` (owned by `02-model.md` §2); envelopes use the crate's own AAD (01-identity §4) | A server cannot move a password into another item or field (ETH anti-pattern 2) |
| Empty AAD is a decode error, never a default | No unbound path exists |
| `schema_v` is in the AAD; readers refuse a lower major than they wrote | Old formats cannot be forced back (anti-pattern 4) |
| AAD never holds secrets; nonces are random 192-bit | AAD is readable; devices do not coordinate counters |
| A failed tag check is shown as "tampered", not "corrupt file" | The user learns what happened |

## 6. Key transparency

Key transparency (KT) is a public, append-only log of public keys. A fake key the server shows you must also appear in the log, where its real owner's app will notice. Proton Pass does this; Bitwarden, LastPass and 1Password sharing trusted the server's word, which is the attack the ETH study confirmed.

**Logged per profile:** ISK public key; hybrid HPKE public key; every `Device.device_pub` and its revocation; persona share keys; the signed Argon2id record; each vault's current manifest head hash (§7). Entries are blinded with a per-profile VRF so only a holder of the profile identifier can look it up **[uncertain]**: the construction is fixed with the formal model (§17).

**Clients verify:**
1. Before wrapping any key to a peer (share, trusted contact, heir, agent, new device): fetch the peer's entry and inclusion proof; refuse if it fails or the key differs from one already pinned.
2. On every sync: fetch the latest signed checkpoint and a consistency proof from the last one seen; store it.
3. Self-audit on every sync: the profile's entries must be exactly the keys this client knows. An unknown key is the loudest alert Lockdown has.
4. Checkpoints are published outside the server and mirrored (`05-self-host.md` §6) so outsiders can audit them.

**In-person check for high-value shares:** when a vault holds crown jewels, or when enrolling a trusted contact or heir, both people see a QR code of the other's ISK fingerprint and a 6-digit short authentication string. Scanning it, or reading it aloud, pins the key. The log then only has to catch a later change.

## 7. The signed manifest and rollback detection

Each vault keeps a manifest: a signed table of every item ID, its version, its Version Key reference, and the hash of the previous manifest. The writer's ISK signs it. Every device remembers the newest head it accepted.

| Check on sync | On failure |
|---|---|
| Signature valid; signer is a current vault member | "Untrusted change blocked" |
| New head chains to the stored head | Rollback detected |
| Head hash matches the KT checkpoint for this profile | Fork detected: devices saw different histories |
| Every referenced version exists and decrypts | Hidden or dropped item detected |

**What the user sees:** a red banner, "Rollback attempt blocked. The server offered an older copy of this vault from <date>. Your data is unchanged." The vault stays readable. New writes queue locally until the user chooses: keep my copy (re-sign as the new head) or inspect the differences. The event enters item history and the audit log. On repeat, the app advises changing server.

The same chain gives "Dad changed the Netflix password on 3 October", one-click restore, and crypto-shredding (delete the Version Key and that version is gone everywhere).

## 8. Recovery menu

Everyone picks at least one at onboarding; Lockdown recommends two. Each rebuilds the IRK, so it opens everything wrapped under it in every ecosystem app (D11). Every recovery wrap is hybrid and KT-verified.

| Option | Spec | Guard | Phase |
|---|---|---|---|
| **Recovery kit** (default) | Secret Key + recovery code, printed or as words; rebuilds the IRK per 01-identity §8 | Drill (§9); yearly reminder | v1 |
| **Another device** | QR of a one-time HPKE key, 6-digit compare, IRK wrapped across | Nudge to enrol a second device in week one | v1 |
| **Trusted contacts** | Contact key verified by KT **and** the in-person check; a request starts a waiting period (default 7 days, 1–30); every owner device is notified and can veto with one tap; on expiry the contact's wrap opens | Closes the Bitwarden emergency-access attack; veto covers coercion | v1 |
| **Shamir k-of-n** | IRK split into n printable cards (default 3-of-5); fewer than k reveal nothing | A share drill | v1.x |
| **Hardware-backed PIN escrow** | Short PIN checked by tamper-resistant servers allowing few guesses (Signal SVR, WhatsApp pattern) | Ships only after outside review | v2 **[uncertain]** |
| **Organization M-of-N** | Member consents at enrolment; M of N admins approve; admin keys KT-verified; all parties notified and logged | Replaces the single-admin back door | v2 |

## 9. The recovery drill

Most people learn their recovery fails at the worst moment. The drill proves it works without touching the live vault.

1. Onboarding ends with the kit; the app asks the user to put it away and come back.
2. The app makes a throwaway envelope wrapped under the kit-derived key.
3. The user types or scans the kit. It opens, or it does not.
4. On success `Profile.recovery` moves from `created` to `drilled`; on failure the kit is re-issued and the drill repeats.
5. A yearly reminder repeats it. Trusted-contact and Shamir setups get their own dry run.

## 10. Crown-jewel class

High-blast-radius items: crypto recovery phrases, the master login of an email account, vendor identity secrets, anything the user marks. They live under the CJK (01-identity §3).

| Rule | Detail |
|---|---|
| Use needs a fresh biometric and a deliberate tap | No cached unlock counts |
| Reveal or export needs the full account password | A fingerprint is not enough |
| Never autofilled, never in search previews, out of bulk export and legacy handover unless opted in per item | Brief §2.9 |
| Never given to agents | No capability can request one (§11) |
| "This device only" is one tap | The item never syncs; a tombstone in the manifest says so |
| Seed-phrase safe mode | A note that looks like a recovery phrase is offered a move here |

## 11. Agent access

Agents and automation (CLI, SSH agent, tool servers, browser agents) get their own door.

| Rule | Detail |
|---|---|
| Actions, not getters | "Sign in to X" or "sign this SSH challenge", never "give me the password for X". The core acts and returns the outcome. |
| Per-task approval | A scoped, time-limited grant (D4) approved on a trusted surface with the full target shown. No standing "allow all". |
| Encrypted, hash-chained audit log | Every request, approval and outcome, encrypted under the ADK, chained like the manifest, shown as "what agents did". |
| Crown jewels denied by default | No capability exists. |
| Same approval service as passkeys and device codes; kill switch | The proximity rule applies (brief §2.7); a tolerated-tier bridge under D6, switched off by signed configuration. |

## 12. Clickjack-hardened autofill

At DEF CON 33, hidden fill controls leaked credentials in 10 of 11 managers with one click. 1Password's "Ask before filling" is off by default; Bitwarden's fix status is disputed **[uncertain]**.

- The fill menu lives in an extension-owned frame in the browser's top layer, never in the page's DOM.
- Before showing it, the extension checks the field and every ancestor: visible, not moved off-screen, not covered, not in a cross-origin frame.
- Exact-origin match by default; base-domain matching opt-in; cross-site frames need confirmation.
- Passkeys, one-time codes and cards need a second deliberate action **by default**. Passwords are one-click only after the checks pass.
- Device codes: only when scanned from a screen in front of the user; pasted or messaged codes are refused with no override.

## 13. Local IPC peer authentication

Desktop apps, the extension, the CLI and sibling ecosystem apps (D4) talk over XPC, a bound service, a named pipe with an ACL, or a Unix socket (01-identity §6). Before any key material moves, both sides verify the peer's **code signature** (Apple, Windows, Android) or **process identity** (Linux: peer credentials plus executable hash). Grants are scoped; the IRK crosses only once, wrapped to a verified new app during attach. Each channel has a session key and a replay counter.

## 14. Memory hygiene

Secrets live only in the Rust core. `zeroize` and `secrecy` wrap every key type; sensitive pages are `mlock`ed; bindings pass handles, never secrets, into JavaScript, JVM or Swift strings. The core locks on sleep, idle, and biometric change (KeePass CVE-2023-32784 is the precedent).

## 15. Supply chain

Reproducible builds for extension, desktop and Android, with recipes published. An SBOM and SLSA provenance (level 3 target) on every artifact. Sigstore-style signed releases that the updater verifies. `cargo-vet` and `cargo-deny` gate every Rust dependency, licence and advisory in CI. Pinned, vendored npm with a small JavaScript tree, because the extension and shell are exposed to npm worms. Every import is parsed in isolation, size-capped and validated (the Aegis icon-pack traversal).

## 16. Audit and disclosure plan

| Item | Commitment |
|---|---|
| Malicious-server analysis | Published before launch against §1; academics invited to attack it |
| Third-party audit | Annual; core, server, extension; published in full |
| Public bug bounty | From beta; includes the server and the KT log |
| Transparency report | Twice a year: legal requests, KT statistics, kill-switch uses, incidents |
| Security white paper | Kept with the code; every claim here maps to a section |

## 17. Formal modelling before launch

In Tamarin or ProVerif, models published: (1) OPAQUE + Secret Key login and the account-slot unwrap under a malicious server; (2) new-device enrolment over QR + short code; (3) trusted-contact recovery with waiting period and veto against key substitution; (4) vault sharing with hybrid HPKE and KT, including member removal; (5) the manifest chain bound to KT checkpoints, for rollback and fork; (6) the IPC attach handshake. The hybrid KEM (X-Wing or HPKE hybrid) needs its standards status confirmed first **[uncertain]**.

## 18. What competitors got wrong

| Finding (ETH study unless noted) | Who | Lockdown's answer |
|---|---|---|
| Public keys fetched unauthenticated in recovery and enterprise flows; escrowed keys captured (confirmed) | Bitwarden, LastPass | KT + in-person check (§6); recovery wraps KT-verified (§8) |
| Sharing did not authenticate public keys or RSA-OAEP ciphertexts (2 attacks); "held up thanks to the Secret Key" is too simple | 1Password | KT; authenticated hybrid HPKE; the Secret Key covers stolen databases, not key swaps |
| Fields swapped between items; old modes forced back (11 vault-encryption attacks across four vendors) | Bitwarden, LastPass, Dashlane, 1Password | AAD binding and version gating (§5) |
| Key wrapping still RSA-OAEP-SHA-1 on 15 Oct 2026 (verified from source) | Bitwarden | Hybrid wraps everywhere |
| Post-quantum only on TLS or app-to-server traffic; NordPass and Dashlane unchecked, so "first" is **[uncertain]** | 1Password, Keeper, Proton Pass | Vault-level hybrid wrapping from day one |
| Not a PAKE; email as salt lets guesses be prepared | Bitwarden | OPAQUE (§3) |
| Unencrypted URLs; low stretching cracked offline | LastPass | Encrypted metadata (§2); signed floor (§4) |
| No verifiable vault history | All | Manifest chain (§7) |
| Emergency access was the attacked surface; or none at all, plus Secret Key lockouts | Bitwarden; 1Password | Recovery menu + drill (§8, §9) |
| Single-admin recovery; screen-lock-only PIN | Passbolt; Ente Auth | M-of-N (§8); PIN as a real slot (§2) |
| One-click leak via hidden fill controls | 10 of 11 managers | §12, on by default |

## 19. Open questions

1. The `account` kind in the crate's `KeyRef` (§3) needs the crate owner's sign-off.
2. Mobile and extension Argon2id floors (§4) need a device lab.
3. KT privacy construction and checkpoint format (§6).
4. Where the kit-wrapped IRK lives: server, backup, or both (01-identity §11). This doc assumes both.
5. Hardware-backed PIN escrow (§8): build, partner, or skip.
6. The hybrid KEM's standards status (§17).
7. One agent audit log per persona, or one per agent (§11).
