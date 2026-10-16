# Identity crate — format v1 (draft)

_Status: draft spec v1.2 (D15, D16 settled; first implementation in Mailbird PR #44 with D18–D25 proposed; fold them into v1.3 once Sean approves), 2026-10-15 (reconciled with 02-model and 04-security: `account` KeyRef, per-persona `kem_pub`). Implements D3, D10, D11, D12, D13. The crate is built inside Mailbird first (`crates/core`), then extracted. Working crate name: `ecosystem-identity`; rename when the ecosystem has a name._

## For future agent

- **TL;DR:** One Rust crate every ecosystem app embeds. It owns the person's identity: profile, personas, devices, keys, and the recovery kit. Any app can create it alone. Later apps find it and reuse it. Lockdown adopts it without moving or re-encrypting anything. Everything below is the on-disk and on-wire contract that makes that possible.
- **Read with:** `00-ecosystem.md` (why), `decisions.md` D10–D13 (what was decided), the research brief §4 (the cryptography this reuses).

## 0. What a person holds (read this before the key tree)

The person remembers **one thing** and keeps **one piece of paper**. Everything in §3 is machinery behind that; the user never names, types, or sees those keys.

| What | Who holds it | When it is used |
|---|---|---|
| **Master password** | the person's memory (biometrics stand in for it on enrolled devices) | unlocking, every day |
| **Recovery kit** (Secret Key + recovery code, one printed page or PDF) | a drawer or a safe | only when every device is gone, or a brand-new device has no old device nearby |

Setting up a new device normally means scanning a QR on an old one. The kit stays in the drawer. This is the 1Password model (account password + Emergency Kit) and it is the whole user-facing surface of this crate.

## 1. Jobs the crate does

1. **Create** an identity with no other app present.
2. **Find** an existing identity on this device and attach to it without asking the user.
3. **Enrol** a new device from an existing one (QR plus a short code).
4. **Recover** on a new device from the recovery kit alone.
5. **Wrap and unwrap** app data keys to the identity key, so every app's data is reachable by the kit.
6. **Hand management** to Lockdown when it arrives, with no migration.
7. **Wall personas** so one person can keep work, personal, and business apart (D13).

What the crate does **not** do: sync, UI, autofill, passkeys, vault items. Those belong to the apps.

## 2. Records

All records are CBOR, signed, and versioned. `v` is the format version; readers refuse a major they do not know.

| Record | One per | Holds | Who may write |
|---|---|---|---|
| `Profile` | person | `profile_id`, `created_at`, `recovery` (kit state: none / created / drilled), `manager` (which app manages this identity, or none), `format_v` | the manager, or any app while `manager` is none |
| `Persona` | wall | `persona_id`, `label`, `kind` (personal / work / business / custom), `created_by_app`, `identity_key_ref`, `kem_pub` (hybrid X25519+ML-KEM-768 public key for shares), `color` | the app that created it, or the manager |
| `Device` | enrolled device | `device_id`, `name`, `platform`, `attestation` (platform keystore attestation when available), `device_pub`, `enrolled_at`, `enrolled_by` (device or kit), `revoked_at` | the device itself, or the manager |
| `KeyEnvelope` | wrapped key | see §4 | the owning app |
| `Consent` | tolerated-tier grant | bridge id, disclosure version, when, by which device | the app that showed the disclosure |

Persona is a **record**, never a flag on the profile (D13). A person always has at least one persona; the default is `personal`.

## 3. Keys

```
Recovery kit  (Secret Key 128-bit + recovery code)
   └─ rebuilds ─▶ Identity Root Key (IRK, 256-bit, random)
                     ├─ wraps ─▶ Persona Key (PK) per persona
                     │            ├─ wraps ─▶ App Data Key (ADK) per app per persona
                     │            └─ wraps ─▶ Crown-Jewel Key (CJK) per persona (D5, brief §2.9)
                     └─ signs ─▶ Identity Signing Key (ISK, Ed25519; ML-DSA slot reserved)
Device Key (per device, in the platform keystore)
   └─ wraps ─▶ IRK for local use (so unlock never needs the kit)
```

- **IRK** never leaves a device unwrapped. It is wrapped three ways: once per enrolled device to that device's keystore key (`device`); once to the **Account Unlock Key** (`account`), which is derived from the master password and the Secret Key through OPAQUE (04-security §3), so a fresh device with the password plus the kit's Secret Key can open it; and once to a key derived from the recovery code alone (`kit`), for the case where the password is forgotten.
- **Persona Key** is the wall. An app that holds the `work` PK cannot open `personal` data. Revoking a persona re-keys only that branch.
- **App Data Key** is what an app actually uses. Mailbird's Phase 33 note-lock key becomes Mailbird's ADK for the persona that owns the notes (D12). Re-wrap happens on the user's next unlock.
- **Crown-Jewel Key** holds the high-blast-radius class. Opening it needs a fresh biometric check and a deliberate tap; exporting needs the full account password. Never autofilled, never given to agents.
- **Public-key wrapping** between devices and for sharing uses hybrid X25519 + ML-KEM-768 through HPKE (brief §4.5). The receiving key pair lives **per persona**: `Persona.kem_pub` is public and goes into the key-transparency log; the private half is wrapped by that Persona Key. So a share sent to someone's work persona can never be opened by their personal one. Each `Device` also has a hybrid pair for enrolment only. Symmetric wrapping uses XChaCha20-Poly1305 with associated data (§4).
- **Password stretching:** Argon2id, device-calibrated, floor 64 MiB / 3 passes / 4 lanes on desktop; parameters stored signed; readers refuse anything below the floor (brief §4.3).

## 4. Key envelope

Every wrapped key is one `KeyEnvelope`:

```
KeyEnvelope {
  v: 1,
  key_id:        uuid,            // the key being wrapped
  wrapped_by:    KeyRef,          // who can open it: { kind: account|device|persona|irk|kit|peer, id }
  alg:           "xchacha20poly1305" | "hpke-x25519-mlkem768",
  aad:           bytes,           // canonical CBOR of {profile_id, persona_id, key_id, purpose, v}
  nonce:         bytes,
  ciphertext:    bytes,
  purpose:       "irk" | "persona" | "app-data" | "crown-jewel" | "share",
  created_at:    time,
  sig:           Ed25519 over the whole envelope by the ISK (or by the device key before the ISK exists)
}
```

The `aad` is what stops a hostile store from moving an envelope to another persona or purpose (brief §4.1, anti-pattern 2). An envelope with the wrong `aad` fails to open; it does not silently open as something else.

## 5. On-disk layout

One directory per profile, owned by whichever app created it, readable by every ecosystem app on the device.

```
<identity-root>/
  profile.cbor            # Profile record, signed
  personas/<persona_id>.cbor
  devices/<device_id>.cbor
  envelopes/<key_id>.<wrapped_by>.cbor
  manifest.cbor           # signed, hash-chained list of every file above and its version (anti-rollback)
  consents/<bridge>.<version>.cbor
  FORMAT                  # "ecosystem-identity/1"
```

Platform locations (the crate resolves them; apps never hardcode):

| Platform | `<identity-root>` | Shared how |
|---|---|---|
| macOS / iOS | App Group container `group.design.snds.identity` (D15) | all ecosystem apps declare the same App Group |
| Android | the creating app's private dir, exposed through a `ContentProvider` with signature-level permission | apps signed by the one ecosystem key (D16) |
| Windows | `%LOCALAPPDATA%\snds\identity` with an ACL to the ecosystem's package family | MSIX package family |
| Linux | `$XDG_DATA_HOME/snds/identity` | same user; peer check by process identity |

The layout is the same on every platform. Only the root path and the sharing mechanism differ.

## 6. Detection and attach (D10)

On first run, before showing any sign-up:

1. **Registry:** ask the OS which ecosystem apps are installed (bundle ids / package names / package family / desktop entries).
2. **Path:** probe the platform `<identity-root>`. Read `FORMAT` and `profile.cbor`. Verify the signature chain and the manifest.
3. **Handshake:** if another ecosystem app is running, open the local channel (XPC on Apple, bound service on Android, named pipe with ACL on Windows, Unix socket on Linux). Both sides verify the peer's **code signature or process identity** before any key material moves (brief §4.9). The running app wraps the IRK to the new app's device key.
4. **No running app, but a profile exists:** the new app can read the records but not the IRK. It asks for the device unlock (biometric / PIN), which opens the device-wrapped IRK. No kit needed.
5. **Nothing found:** create. Make the IRK, the first persona, the device record, and offer the recovery kit. Set `manager` to none.

The user is asked nothing in steps 1–4 except the device unlock they already use.

## 7. Adoption by Lockdown (D10)

When Lockdown attaches to an existing profile:

- It sets `Profile.manager = lockdown` (signed by the ISK, which it now holds).
- It offers the recovery kit if `Profile.recovery == none`. That is the only new ask.
- It does **not** move the root, change the layout, or re-encrypt any envelope.
- From then on it owns device enrolment, recovery, persona management, sharing, and key transparency. Other apps keep reading and writing their own ADKs.

Unlinking Lockdown sets `manager` back to none. Nothing else changes.

## 8. Three ways in (D11)

| Situation | Mechanism | User effort |
|---|---|---|
| Same device, an ecosystem app runs | §6 step 3 handshake | none |
| New device, an enrolled device is at hand | existing device shows a QR holding a one-time HPKE public key; new device scans; both show a 6-digit short authentication string; existing device wraps the IRK to it | one scan, one compare |
| New device, nothing else | recovery kit → Argon2id → unwrap the kit-wrapped IRK from the server or a backup | type the kit once |

The kit is for the last row only. It rebuilds the IRK, so everything wrapped under it opens.

## 9. Versioning and compatibility

- `FORMAT` carries `major`. A reader that sees a higher major stops and says which app must update. A lower major is read and upgraded in place by the manager only.
- New record fields are additive. Unknown fields are preserved on rewrite, never dropped.
- The crate ships the JSON Schema for every record and a conformance test suite. Every app runs it in CI.

## 10. What Mailbird builds first (D12)

In this order, inside `crates/core`:

1. Records and envelopes (§2, §4) with the conformance suite.
2. Create and local attach (§6 steps 2, 4, 5) on macOS and Linux, where the test harness runs.
3. Re-wrap the Phase 33 note-lock key as Mailbird's ADK.
4. The handshake (§6 step 3) once a second app exists to test against. Until then, a test double.
5. Extract to its own repo when Lockdown starts consuming it.

## 11. Settled and open

**Settled (D15, D16):**
- Identifier: Apple App Group `group.design.snds.identity`; Windows `%LOCALAPPDATA%\snds\identity`; Linux `$XDG_DATA_HOME/snds/identity`. Neutral and domain-based, so a brand rename never touches it.
- Android: all ecosystem apps share one signing key (Play App Signing); the store is shared by signature-level permission. Fallback for an app leaving the family: a bound service with a certificate-fingerprint allow-list.

**Open:**
- Whether the kit-wrapped IRK lives on the server, in the user's own backup, or both. Recommendation: both, with the server copy only usable with the kit.
- Key transparency for device public keys lives in Lockdown's server; before Lockdown exists, device enrolment trusts the QR channel plus the short code only. State this in Mailbird's docs.
