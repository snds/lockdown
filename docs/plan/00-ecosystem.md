# Lockdown × Mailbird ecosystem — contract sketch

_Status: sketch, 2026-10-15. Decisions D2–D7. Nothing here is built._

## Rule zero
Each app works alone. Every shared feature below has a "not linked" behaviour that is complete on its own.

## 1. Identity contract (D3)
- **Profile**: one person, one recovery story.
- **Persona**: a named wall (personal, work, side project). Owns accounts, devices, mailboxes, vaults.
- **Device**: enrolled per persona, with an attested key.
- Storage: each app keeps its own copy; linking reconciles by stable IDs, never by display name.
- Owner of the schema: to be decided between Synapse (which already has personas), Mailbird and Lockdown. One schema, three implementations.

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
- Which product owns the identity schema (Synapse, Mailbird, or Lockdown)?
- Does the self-host bundle ship under one name, or as two products with a documented pairing?
- Team and timeline given D9 (feature-complete v1).
