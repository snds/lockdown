# 22 — Security depth

_Slice: `slice/security-depth`. Rows: exposure-tiers, device-approval, secret-key, account-security-keys, pq-key-wrapping, client-kdf, recovery-kit, recovery-drill, password-reminder, high-risk-class, key-transparency. Decisions proposed here: D245–D259 (§19)._

## For future agent

Sections 1–9 explain each piece; §10 lists what each row became and where it lives, §17 what is missing. Every new secret-releasing path goes through the existing gates (`Reveal`, `Use`, `Approve`) and counts as a use (D151). Network and page input is treated as hostile: every frame, assertion and QR payload is parsed and bounded before it is used. The server relays frames it cannot read; it never sees a root key, a Secret Key, a phrase or a fingerprint it could forge.

## 1. Exposure tiers

Every item has a tier: **fill** (browsers and copies get it without asking), **ask** (every reveal, copy and fill goes through the `Use` gate, and a passkey assertion requires user verification) or **app-only** (never offered to a browser, never filled; shown in the app only). The default comes from the item: a crown jewel is never below `ask`; a login is **high risk** when it is the account's own mailbox, an identity provider or mail host (Google, Apple, Microsoft, Yahoo, Proton, Fastmail, Okta and the others in `IDENTITY_PROVIDERS`), or a provider other logins name as their sign-in route. The person's choice is the reserved field `lockdown.exposure`, a new version of the item, so it syncs. Asking more takes effect at once; asking less goes through the `Reveal` gate. `Core::exposure_gate` sits in front of `reveal_for_fill`, `code_for_fill` and `advance_code`; `browser::page_matches`, the wallet and passkey offers skip app-only items.

## 2. Device approval and the relay

An enrolled device opens a link: the server stores only the BLAKE3 of a ten-character Crockford code and hands the opener a link id. The new device joins with the code (typed, or inside the QR payload `lockdown:link:v1?s=<server>&c=<code>&f=<fingerprint>`, which also pins the opener's receiving key). The two run the identity crate's handshake (Hello, Request, Reveal, commit-then-reveal SAS) through `/v1/links/*`; both screens show six digits. Approving on the enrolled device goes through the `Approve` gate; it adds the device's records, wraps the root key to the new device's hybrid key, signs a `Welcome` with the identity key, and tells the server which device it approved (`/v1/links/{id}/device`), so that device's first sign-in counts as known (no second factor, no stranger alert, D78). The new device restores the identity from the handshake and signs in with the password only.

The relay (`bins/server/src/relay.rs`) lives in memory: ten minutes, eight frames of 256 KiB each way, three links per account, one join per link, joins braked like logins. A closed or expired link reads as closed. The phrase-check relay (`/v1/verifications`) is the same mailbox between two accounts.

## 3. The Secret Key on devices

A device that typed or made the Secret Key keeps it sealed under its keystore's unlock key (`secret-key.cbor`), or remembers that the account opted out. An approval seals it to the joiner's handshake key in the `Welcome`; the joiner checks it opens the account slot with the password before keeping it. An account made without a Secret Key (`App::account_register_opted_out`) is refused unless the person confirms, in words, that the password alone then protects the account against a stolen server database.

## 4. Security keys as a second factor

Keys are added on the server's own page (`/vault/#/security-key/<id>`): the server is the WebAuthn relying party (`rp_id` from the `Host` header; https, or http on loopback), the browser makes the credential, and `lockdown_passkey::verify` checks attestation `none`, the client data, the origin, the challenge and the counter. From then on a sign-in from a device the account has never seen passes OPAQUE and then waits: the server answers with a pending sign-in (id, a poll key only that device holds, the WebAuthn request). On the desktop the page `/vault/#/second-factor/<id>` opens in the system browser and the app polls; in the web vault the tab uses the key itself. A device that signed in before, or one another device approved, proves itself with its device key and is not asked. With every key lost, the operator runs `lockdown-server security-keys clear --login … --data …`: access control, not cryptography.

## 5. Client-side stretching

`AccountKsf` carries Argon2id parameters. A device calibrates what it does in about a second (never below the floor, never above 1 GiB / 16 passes / 16 lanes); after each sign-in, when that is more than the account's file was made under, it re-registers the password file over two authenticated round trips and rewrites the account slot. The server keeps the parameters beside the file, announces them at every login start (the default for an unknown login, so nothing leaks) and only lets them rise. The kit's and the profile's KDF rise the same way on a stronger device, and every device follows what the profile says (D49).

## 6. Post-quantum wrapping and signatures

New wraps to another key (vault keys to members and households, item keys, held-back emergency keys, the root key to an approved device, the Secret Key on approval) go only to hybrid X25519 + ML-KEM-768 receiving keys (`pq::require_hybrid`); old classical wraps still open. Beside Ed25519, the identity derives an ML-DSA-65 key from the root key (`lockdown/pq-isk/ml-dsa-65/v1`), publishes it on the profile (`lockdown.pq_isk`), and signs every identity manifest with both (`IdentityManifest.pq_sig`, additive). A reader whose profile publishes the key requires the second signature; a contact pins the key on first sight and refuses an answer that drops or changes it.

## 7. The kit test, the drill and the password reminder

After the first run makes the kit, the app asks for it back (`RemindersPrompt`); skipping takes an explicit "I understand" and the drill comes back in a week. The yearly drill writes `lockdown.drilled_at` on the profile, so every device agrees when it is due; its checks cover the kit, the Secret Key kept here, each emergency contact's keys, and the crown jewels they would get. Where unlocking never asks for the account password (keystore plus the platform check), the app asks for it every 14 days (7, 14 or 30); any password typed and checked counts. The web vault, whose every unlock is the password, shows none of these.

## 8. Crown jewels in emergency access

A crown jewel's item key is under the Crown-Jewel Key, which never leaves the owner, so an emergency contact gets every item of a vault but its crown jewels. An owner opts one in (`lockdown.legacy`, behind the `Reveal` gate); the owner's device seals that item key to the contact, the server holds the envelope with the grant (`/v1/emergency/{grant}/items`) and hands it over only once the grant is released. Crown jewels not opted in never leave and are hidden from the contact's list.

## 9. Checking keys with a phrase

The inviter's app shows nine digits; the other person types them. The two apps run OPAQUE keyed by the phrase, the inviter as the "server" with a password file made a moment before under a key made for this check, then confirm with BLAKE3 tags (keyed by sub-keys of the session key) over each side's view of both accounts and both fingerprints. A server in the middle gets one guess per check and nothing it can test offline; a right phrase with different keys fails both tags and both sides say the keys aren't the other person's. A match marks the contact verified, as comparing the thirty digits does. The contact's QR payload (`lockdown:contact:v1:<account>:<hex>`) is accepted by `verify_contact` too.

## 10. What was built, by checklist row

| Row | Status | Where |
|---|---|---|
| exposure-tiers | done | `crates/vault/src/exposure.rs`, `crates/app/src/session/exposure.rs`, `crates/app/src/protection.rs`, `browser.rs`, `passkeys.rs`; desktop `features/depth/item-protection.tsx` |
| device-approval (with server relay) | done | `crates/vault/src/device_link.rs`, `handover.rs`; `bins/server/src/relay.rs`; `crates/app/src/session/devices_link.rs`, `devices.rs`; desktop `add-device.tsx`, `join-device.tsx`; web `web_join_*` |
| secret-key (QR transfer on approval; opt-out) | done | `device_link.rs` (`SecretKeyTransfer`), `session/secret_key.rs`, `App::account_register_opted_out`; web vault camera scan (`apps/web/src/core/camera-scan.ts`) or typed code |
| account-security-keys | done (desktop through the system browser) | `crates/passkey/src/verify.rs`, `bins/server/src/second_factor.rs`, `session/second_factor.rs`; web `security.rs`, `key-pages.tsx`, sign-in screen |
| pq-key-wrapping | done (wraps, identity manifests); device signatures stay Ed25519 | `crates/vault/src/pq.rs`, `crates/crypto/src/pq_sign.rs`, `identity_set.rs`, `contact.rs` |
| client-kdf | done | `crates/crypto/src/auth.rs` (`AccountKsf`, `ksf_for`, `finish_stretched`), `second_factor.rs` (`account_ksf` table), `session/recovery.rs` (`strengthen_account`, `raise_profile_kdf`) |
| recovery-kit (test in first run) | done | `crates/app/src/reminders.rs` (`kit_test_skip`), desktop `reminders.tsx` (`RemindersPrompt`) |
| recovery-drill | done | `session/recovery.rs` (`after_drill`, `drill_checks`), desktop `DrillSection` |
| password-reminder | done | `reminders.rs`, desktop `RemindersPrompt`; never on the web vault |
| high-risk-class (crown jewels in emergency access) | done | `crates/vault/src/vault/legacy.rs`, `bins/server/src/legacy.rs`, `session/sharing.rs` |
| key-transparency (phrase invites; QR fingerprint) | phrase checks done; QR fingerprint accepted by the core, web vault has no contacts screen to scan from (§17) | `crates/vault/src/phrase_check.rs`, `session/sharing/phrase.rs`, `crates/app/src/key_check.rs`, desktop `key-check.tsx` |

## 11. Command surface (added; nothing renamed)

- Server routes, appended: `/v1/links*`, `/v1/verifications*`, `/v1/account/password/{start,finish}`, `/v1/account/security-keys*`, `/v1/account/login/second-factor*`, `/v1/emergency/{grant}/items`.
- Wire additions (D62): `LoginStartReply.ksf`, `LoginFinishReply.second_factor`, `IdentityManifest.pq_sig`, `Contact.pq_isk`; views `ContactView.post_quantum`, `LinkView.qr_rows`.
- Desktop commands (`src-tauri/src/security_depth.rs`, appended to the handler list): `exposure`, `exposures`, `set_exposure`, `legacy_opt_in`, `set_legacy_opt_in`, `reminders`, `password_check`, `password_snooze`, `set_password_reminder_days`, `drill_snooze`, `kit_test_skip`, `drill_checks`, `strengthen_account`, `phrase_*`, `security_keys`, `security_key_add`, `security_key_remove`, `second_factor_open`, `second_factor_wait`, `link_*`, `secret_key_status`, `join_*`, `account_register_opted_out`. Pages open only when on the account's server (`server_page`).
- Web commands (`crates/web/src/commands/depth.rs`): the shared ones under the same names, plus `web_sign_in_waiting`, `web_sign_in_answer`, `web_sign_in_continue`, `web_join_{start,poll,finish,cancel}`, and `page_sign_in_{request,answer}`, `page_key_{request,finish}` for the pages the desktop opens.
- Desktop views reach all of this through `DepthClientProvider` (`features/depth/depth-client.ts`); only `App.tsx` imports the Tauri implementation (`app/tauri-depth.ts`); the web vault provides `core/web-depth-client.ts`.

## 12. Security notes

- **Gates.** Approving a device: `Approve`. Removing a key: `Approve`. Letting an item ask less, and opting a crown jewel in to emergency access: `Reveal`. An `ask` item's every value out: `Use`. Each counts as a use (D151).
- **Hostile input.** Relay frames are parsed as versioned CBOR enums and refused out of turn; a frame that fails ends the check or the join. WebAuthn answers are checked by the server alone. QR payloads are parsed by prefix and fields; the web vault opens only `#/second-factor/<uuid>` and `#/security-key/<uuid>`, and joins only its own origin's links.
- **What the server learns.** The hash of a link code; that two devices of one account met; that two accounts ran a check; the security keys' public halves and counters; the account's stretching parameters. Not the root key, the Secret Key, a phrase, or a fingerprint it could stand in for.
- **Nothing logged.** No new log line names a code, phrase, key or value.

## 13. How to run and test

```
export CARGO_TARGET_DIR=… CARGO_INCREMENTAL=0
cargo test -p lockdown-vault        # phrase_check, pq, device_link, handover, exposure, legacy
cargo test -p lockdown-app --test device_approval --test exposure_tiers --test recovery_depth --test phrase_check
cargo test -p lockdown-server --test account_security
cargo test -p lockdown-web --test security_depth   # key on the page, tab sign-in with a key, join by QR
cargo test -p lockdown-desktop --features shell    # server_page
cd apps/desktop && pnpm test                         # features/depth/depth.test.tsx
cd apps/web && pnpm test                             # core/webauthn.test.ts, features/key-pages.test.tsx
```

By hand: run `lockdown-server`, register on the desktop, open Devices → Add a device, and on a second profile (`LOCKDOWN_ROOT`, `LOCKDOWN_DATA`) choose Join → "Join with another device instead". Add a security key from Devices (the system browser opens the server's page), then sign in on a third profile to see the wait.

## 14. Web vault

The tab is a new device at every sign-in, so an account with keys always asks: the sign-in screen offers "Use my security key" after the password passes and finishes with the browser's own WebAuthn. "With a device" joins from the desktop with the typed code or the camera (`BarcodeDetector` where the browser has it). Adding a key opens the server's page in a new tab (this tab's vault lives in its memory). The recovery reminders stay on the desktop.

## 15. Tests added

Rust: phrase checks (4), ML-DSA manifest signatures (1), QR rows (1), device approval (3), exposure tiers (3), recovery depth (4), phrase checks through `App` (2), server account security (4), web security depth (2), desktop `server_page` (1), plus the unit tests in `pq_sign`, `device_link`, `handover`, `verify`, `second_factor`. Desktop vitest: 12 in `features/depth/depth.test.tsx`. Web vitest: 9 in `core/webauthn.test.ts` and `features/key-pages.test.tsx`.

## 16. unicron host-contract gaps

- **QR code.** `features/depth/qr-code.tsx` draws the core's module rows as one SVG path, as the Wi-Fi card does (21 §7). One `QrCode` primitive should serve both.
- **Camera scan.** The web vault's scanner is a plain DOM overlay around `BarcodeDetector`. A `QrScanner` primitive (video, framing, cancel, permission states) belongs in unicron.
- **Large digits.** The six-digit SAS and the nine-digit phrase are mono text at a large size. A `CompareDigits` primitive (grouping, reading aloud, matching states) would serve device approval, phrase checks and fingerprints.
- **Security keys on the desktop.** No host API reaches FIDO2 authenticators from the webview, so the desktop uses the system browser on the server's page. A host WebAuthn (or CTAP over USB/NFC) contract would let the app use a key in its own window.

## 17. Gaps

- **QR fingerprint scan in the web vault.** The core accepts the contact QR payload and the camera scanner exists, but the web vault has no contacts screen yet (its Sharing area is links only), so there is nowhere to scan into.
- **Desktop FIDO2 in-window.** See §16; the system browser path works today.
- **Older clients and `pq_sig`.** A client from before this slice decodes a manifest with `pq_sig` but drops the field when it re-encodes, so its hash of the manifest differs and its chain check fails. Ship to every device before the first device publishes its post-quantum key (it does on its next identity push).
- **Device signatures, vault heads and the transparency log stay Ed25519.** Moving them to hybrid is a larger format change.
- **Relay state is in memory.** A server restart drops open links, checks and waiting sign-ins; each says to start again.
- **Browser e2e for the key pages.** Covered by vitest with a fake `navigator.credentials` and natively by `crates/web/tests/security_depth.rs` with a software authenticator; a Playwright run with a virtual authenticator is not written.
- **The kit test is a prompt after the first run, not a fourth first-run screen.** It appears as soon as the vault opens and stays until the kit passes or is skipped.

## 18. Interop and data

Every new field is additive (D62) and absent when empty: `lockdown.exposure`, `lockdown.legacy`, `lockdown.drilled_at`, `lockdown.account_ksf` and `lockdown.pq_isk` are reserved fields or profile extras the identity crate preserves. New server tables: `security_keys`, `account_ksf`, `emergency_items`. New dependency: `ml-dsa` 0.1.1 (Apache-2.0 OR MIT, no default features). `qrcode` is main's (D239).

## 19. Proposed decisions

| ID | Date | Decision | Status |
|---|---|---|---|
| D245 | 2026-10-18 | **Each item has an exposure tier: fill freely, ask each time, or keep to the app.** Stored as the reserved field `lockdown.exposure` (a new version, so it syncs). Defaults: fill; ask for high-risk logins (the account's own mailbox, identity providers' sign-ins, providers other logins sign in through); crown jewels never below ask. Asking more needs nothing; asking less goes through the `Reveal` gate. App-only items are never offered to a browser. Plain version: you choose, item by item, whether Lockdown fills it, asks first, or keeps it in the app. | Proposed; **flagged for Sean** (the three names; the high-risk defaults) |
| D246 | 2026-10-18 | **A new device joins by approval from an enrolled one.** Ten-character code or QR (server, code, the enrolled device's fingerprint), ten minutes, one join; six digits compared on both screens; approval behind the `Approve` gate; the server relays frames it cannot read and binds the approved device so its first sign-in is known. Plain version: add a phone by scanning a code on your laptop and checking six digits match; no kit needed. | Proposed; **flagged for Sean** (user-visible flow and words) |
| D247 | 2026-10-18 | **The Secret Key travels with an approval, and devices keep it sealed.** A device that has the Secret Key keeps it under its keystore's unlock key and hands it to an approved device, sealed to that device's handshake key; the new device checks it opens the account slot. An account without a Secret Key is made only after an explicit confirmation. Plain version: a new device you approve never needs the kit; skipping the Secret Key altogether is possible but warned about. | Proposed; **flagged for Sean** (the opt-out's words) |
| D248 | 2026-10-18 | **Post-quantum by rule: hybrid wraps only, and ML-DSA-65 beside Ed25519 on identity manifests.** New wraps go only to X25519 + ML-KEM-768 keys (old wraps still open). The identity's ML-DSA-65 key is derived from the root key, published on the profile (`lockdown.pq_isk`), required on every manifest once published, and pinned by contacts. Device signatures, vault heads and the log stay Ed25519 for now. | Proposed; audit item |
| D249 | 2026-10-18 | **Password stretching follows the strongest device.** Each device calibrates Argon2id to about a second (floor to 1 GiB / 16 / 16); a stronger one re-registers the account's password file and raises the kit's KDF; the server keeps and announces the parameters and only lets them rise. Plain version: your password gets harder to guess offline as your devices get faster. | Proposed; audit item |
| D250 | 2026-10-18 | **Security keys and passkeys are a second factor on the account, for devices it has never seen.** Up to 20 per account, added on the server's page; a new device passes the password then waits for a key (five minutes, a poll key only it holds); approved or known devices aren't asked. With every key lost, the operator clears them. Plain version: a stolen password alone can't sign in a new device once you add a key. | Proposed; **flagged for Sean** (user-visible; the operator reset) |
| D251 | 2026-10-18 | **The first run ends with the kit test.** After the kit is made, the app asks for it back; skipping needs "I understand an untested kit can fail", and it asks again in a week. Plain version: Lockdown checks your recovery page works before you put it away. | Proposed; **flagged for Sean** |
| D252 | 2026-10-18 | **The account password is asked for every 14 days where unlocking never asks for it** (7, 14 or 30; "Tomorrow" snoozes a day; any checked password counts). Never in the web vault. Plain version: you won't forget the password a new device will need. | Proposed; **flagged for Sean** (the 14-day default) |
| D253 | 2026-10-18 | **The recovery drill is yearly, dated on the profile, and checks more than the kit.** `lockdown.drilled_at` on the identity's profile; the drill's checks: the kit, the Secret Key kept here, each emergency contact's keys, which crown jewels they would get. Plain version: once a year Lockdown walks you through proving you can get back in. | Proposed; **flagged for Sean** |
| D254 | 2026-10-18 | **Crown jewels reach an emergency contact only when opted in, one by one.** `lockdown.legacy` behind the `Reveal` gate; the item key is sealed to the contact and held by the server until the grant is released; others stay hidden from the contact. Plain version: your most sensitive items stay yours even in an emergency, unless you choose otherwise. | Proposed; **flagged for Sean**; audit item |
| D255 | 2026-10-18 | **Keys can be checked with a nine-digit phrase said aloud.** OPAQUE keyed by the phrase between the two apps through the server's relay, then tags over both fingerprints; one try per check; a match verifies the contact as comparing digits does. Plain version: say nine digits to a friend instead of reading thirty. | Proposed; **flagged for Sean**; audit item |
| D256 | 2026-10-18 | **Wire additions (D62, additive).** The `/v1/links*`, `/v1/verifications*`, `/v1/account/password/*`, `/v1/account/security-keys*`, `/v1/account/login/second-factor*` and `/v1/emergency/{grant}/items` endpoints; `LoginStartReply.ksf`, `LoginFinishReply.second_factor`, `IdentityManifest.pq_sig`, `Contact.pq_isk`, `ContactView.post_quantum`, `LinkView.qr_rows`; the desktop and web commands of §11. | Accepted by default under D38 |
| D257 | 2026-10-18 | **The web vault uses security keys in the tab and serves the desktop's key pages.** `#/second-factor/<id>` and `#/security-key/<id>` hold only the id; a tab's own sign-in uses the browser's WebAuthn; a tab joins from another device with the code or the camera; adding a key opens a new tab. Plain version: the web vault does keys itself, and is where the desktop sends you to use one. | Proposed; **flagged for Sean** (user-visible) |
| D258 | 2026-10-18 | **The relays are in memory and small.** Ten minutes, eight frames of 256 KiB each way, three links and sixteen checks per account, one join per link, joins braked like logins; a restart drops them. | Proposed |
| D259 | 2026-10-18 | **New dependency: `ml-dsa` 0.1.1** (Apache-2.0 OR MIT, `alloc` and `zeroize` only), the RustCrypto generation `ml-kem` 0.3 already uses. | Proposed |
