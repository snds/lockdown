# Lockdown × Mailbird ecosystem — contract sketch

_Status: sketch, 2026-10-15. Decisions D2–D7. Nothing here is built._

## Rule zero
Each app works alone. Every shared feature below has a "not linked" behaviour that is complete on its own.

## 1. Identity contract (D3)
- **Profile**: one person, one recovery story.
- **Persona**: a named wall (personal, work, side project). Owns accounts, devices, mailboxes, vaults.
- **Device**: enrolled per persona, with an attested key.
- Storage: each app keeps its own copy; linking reconciles by stable IDs, never by display name.
- **Creation (D10):** any app can create the identity alone. On first run an app looks for an existing ecosystem install (platform app registry, known data paths, then a local handshake with attestation). If one exists, the new app piggybacks on that identity and asks nothing of the user. If none exists, it creates one with the shared crate.
- **Adoption (D10):** when Lockdown is installed after another app, it reads the existing identity and becomes its manager (recovery kit, device enrolment, sharing, key transparency). It never moves the data, changes its storage location, or re-encrypts it. If the existing identity has no recovery kit yet, Lockdown offers to make one; that is the only new ask.
- **One format from day one:** identity records and key envelopes come from one shared Rust crate (`ecosystem-identity`, name TBD) with a versioned on-disk format. Every app embeds the same crate. This is what makes adoption free of migration.
- Owner of the schema: the shared crate. Lockdown is the reference implementation; Synapse and Mailbird embed it. (Resolves the earlier open question.)

## 2. Enclave-to-enclave trust (D4)
- Each app holds its keys in the platform keystore (Secure Enclave, StrongBox, TPM).
- Linking = a one-time pairing: both apps show a short code, the user confirms, each stores the other's attested public key.
- After pairing, apps exchange **scoped grants** (for example "open locked note X", "fill login for mail account Y"). Never master keys.
- Every local channel authenticates its peer by code signature or process identity.

## 3. Locked notes with a Lockdown recovery path (D5)
- Not linked: Mailbird locks notes with its own key (Phase 33: PBKDF2 + AES-256-GCM). Unchanged.
- Linked: Mailbird's notes key is additionally wrapped to the Lockdown identity key (hybrid classic + post-quantum). Opening a note then works with either the Mailbird lock or the Lockdown master password.
- Recovery: the Lockdown recovery kit (Secret Key + recovery code) can rebuild the identity key, so the wrapped notes key is recoverable even if the Mailbird lock password is lost. This is the 1Password Emergency Kit pattern, extended across apps.
- Unlinking re-keys the notes so the old wrap is useless.

## 4. Plugins (D6)
Same model in both apps: signed manifest, capability list, risk tier (standards / official / tolerated / closed-never), consent gate per tier, remote kill switch, first-party only at first. Defined in the research brief §2.9.

## 5. Self-host bundle (D7)
- One compose/Helm bundle: Lockdown server, Mailbird server, mail server, DNS helpers, backup job.
- Backup follows 3-2-1 (three copies, two media, one off-site). Lockdown's backups are encrypted at rest with keys the user holds; a restore drill is a first-class feature.
- Mail server and DNS tooling are Mailbird scope. Lockdown supplies identity, secrets and the backup-encryption primitives.

## Open questions
- Detection order and what counts as a trusted existing install (signature checks per OS).
- Whether Mailbird's existing Phase 33 note lock migrates into the shared format before or after the crate exists (it predates it).
- Does the self-host bundle ship under one name, or as two products with a documented pairing?
- Team and timeline given D9 (feature-complete v1).
