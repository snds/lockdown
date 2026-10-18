# Lockdown: v1 feature status

_Generated from `feature-status.json` (as of `a3ca4f8`) by `render-feature-status.py`; edit the JSON, not this file. The feature list is the 122 v1 rows of docs/research/2026-10-15/feature-matrix.json with the moves in 06-roadmap.md §1 (D8, D9)._

**Done** means it works end to end in today's surfaces (core, desktop, CLI, server) with tests. **Partial** means some layer exists and the row says what is missing. **Not started** means nothing in code yet. **Not code yet** is work that is not software (an audit, a licence call, a bug bounty).

| | Done | Partial | Not started | Not code yet |
|---|---|---|---|---|
| **All 122 v1 features** | **12** | **53** | **47** | **10** |
| [Vault & item types](#vault--item-types) | 0 | 12 | 0 | 0 |
| [Organization & search](#organization--search) | 1 | 2 | 4 | 0 |
| [Autofill (browser)](#autofill-browser) | 0 | 3 | 6 | 0 |
| [Autofill (mobile & desktop OS)](#autofill-mobile--desktop-os) | 0 | 0 | 6 | 0 |
| [Passkeys & WebAuthn](#passkeys--webauthn) | 0 | 2 | 6 | 0 |
| [Two-factor & authenticator](#two-factor--authenticator) | 1 | 3 | 0 | 0 |
| [Vendor login bridges](#vendor-login-bridges) | 1 | 4 | 2 | 0 |
| [Sharing & collaboration](#sharing--collaboration) | 0 | 2 | 4 | 0 |
| [Family & emergency/recovery](#family--emergencyrecovery) | 0 | 2 | 3 | 0 |
| [Security monitoring & phishing defense](#security-monitoring--phishing-defense) | 3 | 5 | 0 | 0 |
| [Privacy](#privacy) | 0 | 1 | 1 | 0 |
| [Import/export & portability](#importexport--portability) | 0 | 3 | 5 | 0 |
| [Self-hosting & deployment](#self-hosting--deployment) | 0 | 2 | 1 | 1 |
| [Security architecture & crypto](#security-architecture--crypto) | 4 | 8 | 5 | 0 |
| [Platforms & apps](#platforms--apps) | 2 | 3 | 3 | 1 |
| [Trust & transparency](#trust--transparency) | 0 | 1 | 1 | 8 |

13 rows are deferred by D122 (OS-level integration and the native mobile apps); they stay on the list, marked.

## Where things stand

No mobile apps exist (no Swift or Kotlin; crates/ffi is a skeleton). No browser extension exists: extension/ holds only the wasm matcher boundary, its JSON schema and a Playwright clickjacking harness. The shipped surfaces today are the core crates, the Tauri desktop app (built and tested on Linux in CI; macOS and Windows only compile-checked for platform-auth), the `lockdown` CLI and the self-host server. The CLI still carries its own copy of the flows and handles logins only for item kinds. policy, approval, bridge-host and ffi are skeletons. The code is licensed Apache-2.0 today; the plan leaves the licence open (05-self-host §12 asks Sean between AGPL-3.0 and a source-available licence for the server), so oss-clients and oss-server wait on that call. Client TLS uses rustls on ring (no post-quantum key exchange) and the server speaks plain HTTP. Rows judged `done` are done for desktop, CLI and server only; every one still needs its mobile and extension half when those surfaces exist.

## Vault & item types

- [ ] **Logins**: Partial · core, desktop, cli. Several websites in the editor (only url.0 is offered), a sign-in-method field (password, passkey, federated, SSO), and passkeys made on the record rather than only imported.
- [ ] **Secure notes**: Partial · core, desktop. Render the Markdown (the body is shown as written); the encrypted note item and the notes field on every kind already work.
- [ ] **Payment cards**: Partial · core, desktop. The card fill itself (extension or OS provider) with the second deliberate action and the optional CVC re-prompt.
- [ ] **Identities and addresses**: Partial · core, desktop. Typed subtypes (person, address, business, vehicle, passport) and a separate contacts list that is never filled; plus the form fill itself.
- [ ] **Typed item templates**: Partial · core, desktop. Reach 25+ types (passport and licence are a Document type choice today) and add user-defined templates.
- [ ] **Custom fields**: Partial · core, desktop. Add the TOTP and linked field kinds.
- [ ] **File attachments**: Partial · core, cli, server. Attachments in lockdown-app and the desktop editor and detail, streaming for large files, blobs on object storage, and the 5 GB quota.
- [ ] **Password and passphrase generator**: Partial · core, desktop. Word passphrases, per-site rule profiles, length and class options in the UI, and statistical randomness tests in CI.
- [ ] **Wi-Fi network items**: Partial · core, desktop. The self-hiding guest QR card, the printable card, and export through CXF's own Wi-Fi type (it rides in the Lockdown extension today).
- [ ] **Item expiry reminders**: Partial · core, desktop. Actual reminders before a date lapses, renewal kinds for domains and certificates, and the client-side signed-date check.
- [ ] **Trash with recovery**: Partial · core, desktop. The 30-day grace timer that purges on every device, the per-vault setting, holding purges while a conflict is open, and trash in the CLI.
- [ ] **Item archive**: Partial · core. An archive and unarchive action in the core and the desktop, and leaving archived items out of search (the state exists but nothing sets it).

## Organization & search

- [x] **Smart categories**: Done · core, desktop. Nothing for the desktop; Signal-hidden passkeys join Needs attention once the Signal API exists.
- [ ] **Multiple vaults**: Partial · core. Creating a second vault, switching between vaults in the desktop, and per-vault sync and limits.
- [ ] **Global quick-access palette**: Partial · desktop. The system-wide hotkey outside the window, fill and show-code actions, and a shared Rust search index (search runs over summaries in TypeScript).
- [ ] **Folders**: Not started · nowhere yet. Everything: a folder tree in the vault model, the UI, and folder import.
- [ ] **Tags**: Not started · nowhere yet. Everything: an encrypted tag field, nesting, cross-vault views and the UI.
- [ ] **Duplicate finder**: Not started · nowhere yet. Everything: detection on URL, username and code secret after import, and the side-by-side merge.
- [ ] **Favorites and recency ranking**: Not started · nowhere yet. Favorites, last-fill and frequency tracking on the device, and ranking fill suggestions and search by them.

## Autofill (browser)

- [ ] **Browser extension coverage**: Partial · core. The MV3 extension for Chromium browsers and Firefox and the Safari Web Extension; only the wasm core they will share exists.
- [ ] **Clickjacking-hardened fill**: Partial · core. The real extension using it: the top-layer popover, IntersectionObserver v2 tracking, and the second action wired to TOTP, card and passkey fills.
- [ ] **Precise URL matching**: Partial · core, cli. A way to set per-item rules in the desktop editor (new sites get exact_host) and an extension that applies the decision.
- [ ] **Inline fill menu**: Not started · nowhere yet. Everything: the top-layer menu in an extension-origin frame inside a closed shadow root.
- [ ] **Save and update prompts**: Not started · nowhere yet. Everything: sign-up and change-password detection and the save or update prompt.
- [ ] **Per-site fill fixes**: Not started · nowhere yet. Per-site user overrides synced end to end and a signed community fix list.
- [ ] **Fill keyboard shortcut**: Not started · nowhere yet. Everything: fill and fill-code shortcuts in every browser, cycling on repeat.
- [ ] **Card and address fill**: Not started · nowhere yet. Everything: the checkout field classifier, split card fields and the confirm step.
- [ ] **HTTP Basic Auth fill**: Not started · nowhere yet. Everything: answering HTTP auth challenges from the extension on Chromium and Firefox.

## Autofill (mobile & desktop OS)

- [ ] **iOS AutoFill provider** _(deferred: native mobile, D122)_: Not started · nowhere yet. Everything: the iOS app and its AutoFill credential provider extension.
- [ ] **Android autofill and Credential Manager** _(deferred: native mobile, D122)_: Not started · nowhere yet. Everything: the Android app, AutofillService and CredentialProviderService.
- [ ] **Autofill health check** _(deferred: OS-level, D122)_: Not started · nowhere yet. Everything: there is no OS provider or extension link to check yet.
- [ ] **One-time-code AutoFill on iOS** _(deferred: native mobile, D122)_: Not started · nowhere yet. Everything: one-time-code AutoFill needs the iOS app first.
- [ ] **Native macOS AutoFill** _(deferred: OS-level, D122)_: Not started · nowhere yet. Everything: the native macOS credential provider extension for passwords, passkeys and codes.
- [ ] **Fill into any Mac app** _(deferred: OS-level, D122)_: Not started · nowhere yet. Everything: accessibility-based fill on a global shortcut with a code-signature check.

## Passkeys & WebAuthn

- [ ] **Passkey storage and sync**: Partial · core, desktop, server. Creating and using passkeys (WebAuthn L3 provider on every platform); today a passkey can only be imported, stored, synced and viewed.
- [ ] **Passkey management view**: Partial · core, desktop. Provider ID (AAGUID), backup flags, last use, and a delete that warns when it is the account's only sign-in.
- [ ] **Phone as authenticator (iPhone)** _(deferred: native mobile, D122)_: Not started · nowhere yet. Everything: needs the iOS credential provider.
- [ ] **Phone as authenticator (Android)** _(deferred: native mobile, D122)_: Not started · nowhere yet. Everything: needs the Android CredentialProviderService.
- [ ] **Windows 11 passkey plugin** _(deferred: OS-level, D122)_: Not started · nowhere yet. Everything: the COM IPluginAuthenticator plugin, Windows Hello verification and MSIX packaging.
- [ ] **Browser passkey fallback**: Not started · nowhere yet. Everything: the WebAuthn override where no OS provider exists, and the coexistence convention.
- [ ] **Automatic passkey upgrade**: Not started · nowhere yet. Everything: needs the OS providers and passkey creation.
- [ ] **WebAuthn Signal API handling**: Not started · nowhere yet. Everything: handling the three Signal API calls with soft delete and undo.

## Two-factor & authenticator

- [x] **Built-in TOTP codes**: Done · core, desktop, cli. Nothing in code for desktop; keeping it on the free tier is a pricing rule.
- [ ] **TOTP autofill**: Partial · core. The extension filling single and split six-box code fields after the password.
- [ ] **Authenticator importers**: Partial · core, desktop, cli. Google Authenticator transfer batches and Aegis (incl. encrypted), 2FAS, Ente, Proton and Bitwarden files; only single otpauth:// and steam:// links import today.
- [ ] **Code display aids**: Partial · core, desktop. Clock-skew correction against server time and search focus on launch (haptics are a phone item).

## Vendor login bridges

- [x] **Steam Guard codes**: Done · core, desktop, cli. Nothing in code for desktop; no UI test exercises the Steam variant specifically.
- [ ] **Steam maFile import**: Partial · core. File parsers for SDA (incl. passkey-encrypted), WinAuth and Steam v2 JSON, with identity_secret put in the high-risk class.
- [ ] **TV and console activation by scan** _(under review)_: Partial · core, desktop. The scanner, the activation-page allow-list, opening the real page with the code and autofill, and the core producing approval requests (the sheet is a seam).
- [ ] **Anti-phishing approval gate** _(under review)_: Partial · core, desktop. Wire the gate into real device-code, Steam QR and passkey QR flows; nothing calls it yet.
- [ ] **Large-type display**: Partial · desktop. Large type for any password (Logins lack it), and the hotel and rental mode that logs the TV sign-in and reminds you to sign out.
- [ ] **Federated sign-in map**: Not started · nowhere yet. Everything: capturing provider buttons and redirects, the blast-radius view and recovery warnings.
- [ ] **Service 2FA setup guides**: Not started · nowhere yet. Everything: setup cards from 2fa.directory and the passkeys directory with attribution.

## Sharing & collaboration

- [ ] **Shared vaults**: Partial · core. Adding and removing members, wrapping the Vault Key to another person's key, rotation on removal, server support and UI.
- [ ] **Granular share permissions**: Partial · core. Enforcing the rights in clients, groups, and logging; nothing reads the rights yet.
- [ ] **Single-item sharing**: Not started · nowhere yet. Everything: per-item keys wrapped to a recipient and the two-tap share.
- [ ] **Share links for non-users**: Not started · nowhere yet. Everything: the Send feature for items, files and text.
- [ ] **Request a secret**: Not started · nowhere yet. Everything: single-use request links encrypting to the requester's key.
- [ ] **Fill-only sharing**: Not started · nowhere yet. Everything: needs sharing and an extension fill path.

## Family & emergency/recovery

- [ ] **Recovery kit**: Partial · core, desktop, cli, server. Make the "test it now" step forced at signup; today it is an optional drill with a reminder.
- [ ] **Recovery drill**: Partial · core, desktop, cli. The yearly prompt, a last-success date, and checks of heirs' keys and legacy settings.
- [ ] **Family plan**: Not started · nowhere yet. Everything: shared vaults, a household vault, and plan and billing logic.
- [ ] **Emergency access**: Not started · nowhere yet. Everything: contact keys checked through key transparency and a QR fingerprint, the waiting period and release.
- [ ] **Password reminder for biometric users**: Not started · nowhere yet. Everything: a periodic account-password prompt (default 14 days) once biometric unlock exists.

## Security monitoring & phishing defense

- [x] **Breached password check**: Done · core, desktop. The extension's and web vault's view of the same findings.
- [x] **Weak and reused password report**: Done · core, desktop. Reuse across more than one vault once multiple vaults land.
- [x] **Missing 2FA report**: Done · core, desktop. A refresh schedule for the bundled 2fa.directory snapshot (D129).
- [ ] **Email breach monitoring**: Partial · server. A breach feed key (the built-in feed needs a paid key) and alias monitoring.
- [ ] **Breach-response playbooks**: Partial · core, desktop. The seven guided incident flows from the plan; seven plain playbooks exist.
- [ ] **Lookalike-domain warning**: Partial · core, desktop. The hard stop at fill time, in the browser extension.
- [ ] **One-tap change-password link**: Partial · core, desktop. Curated per-site change-password recipes beyond the W3C well-known page.
- [ ] **Vault account sign-in alerts**: Partial · server, desktop. OS notifications, a session list that ends tokens, email verification for new devices.

## Privacy

- [ ] **Clipboard and screenshot hygiene**: Partial · desktop. Keep copies out of Windows clipboard history and cloud clipboard, and block screen capture on reveal screens.
- [ ] **Third-party alias services**: Not started · nowhere yet. Everything: alias generation through the six services' API keys.

## Import/export & portability

- [ ] **Lossless transfer**: Partial · core, desktop. Attachments, folders and history in the transfer, Wi-Fi in CXF's own type, and a per-item report of what the target app dropped.
- [ ] **Importers from other managers**: Partial · core, desktop. Importers for 1PUX, Bitwarden JSON, Proton, KDBX, Dashlane, Keeper, LastPass and Enpass, sandboxed.
- [ ] **Complete export**: Partial · core, desktop. Attachments, history and folders in the export, as an encrypted archive; export in the CLI.
- [ ] **iOS credential exchange** _(deferred: native mobile, D122)_: Not started · nowhere yet. Everything: needs the iOS app; the CXF model it would map through exists.
- [ ] **Android credential exchange** _(deferred: native mobile, D122)_: Not started · nowhere yet. Everything: needs the Android app; the CXF model it would map through exists.
- [ ] **Encrypted export**: Not started · nowhere yet. Everything: Argon2id plus AEAD and age-recipient exports, with plaintext behind an extra confirm.
- [ ] **Published backup format**: Not started · nowhere yet. Everything: the published backup format, its open-licence reference decoder, and KDBX export.
- [ ] **Automatic local backups**: Not started · nowhere yet. Everything: scheduled encrypted backups to a chosen folder, the last-backup indicator and the emergency sheet.

## Self-hosting & deployment

- [ ] **Official self-hosting**: Partial · server. Packaging, operator docs and support, and the server features later slices need (sharing, push); it is axum plus rusqlite, not sqlx.
- [ ] **Lightweight single container**: Partial · server. A published container image, Postgres, S3-compatible attachment storage, and the memory measurement.
- [ ] **Self-hosted live sync**: Not started · nowhere yet. Everything: WebSocket live sync and a push relay for mobile.
- [ ] **Same code as the cloud**: Not code yet · nowhere yet. Needs a cloud, a feature-flag release process and advisory IDs; nothing to build until a hosted service exists.

## Security architecture & crypto

- [x] **End-to-end encryption**: Done · core, server, desktop, cli. Decide whether item kinds and field names (visible per D48) must be hidden too; then the external audit.
- [x] **Password-authenticated login (PAKE)**: Done · core, server, desktop, cli. Nothing in code; it goes into the external audit.
- [x] **Bound authenticated encryption**: Done · core. Nothing in code; it goes into the external audit.
- [x] **Rollback and deletion protection**: Done · core, server, cli, desktop. Nothing for v1 beyond the audit.
- [ ] **Secret Key**: Partial · core, desktop, cli, server. QR transfer of the Secret Key to a new device (it is typed from the kit today) and the opt-out warning path.
- [ ] **Strong key derivation**: Partial · core. Per-device calibration above the floor and renegotiation on a stronger device (every device uses the desktop floor, D49).
- [ ] **Malicious-server resistance**: Partial · core, server, cli, desktop. The Tamarin or ProVerif model of login, sharing and recovery, and the academic review (not code); sharing and recovery defences follow those features.
- [ ] **Authenticated public keys**: Partial · core, server, cli, desktop. Lookups of other people's keys, QR fingerprint checks and passphrase-verified invites (they need sharing), and log mirrors.
- [ ] **Post-quantum key wrapping**: Partial · core. Wrapping vault, sharing, organisation and recovery keys to hybrid recipients, and ML-DSA signatures.
- [ ] **New-device approval with code match**: Partial · core, desktop. Approving a new device from an existing one over an authenticated channel (a device joins with the password and kit today).
- [ ] **Memory hygiene**: Partial · core, desktop. Locked memory pages kept out of swap; the webview still holds typed and revealed strings briefly (D91).
- [ ] **High-risk item class**: Partial · core, desktop. Keep crown jewels out of bulk export unless opted in per item (they are exported today), moving an item into or out of the class, and the legacy-release rule.
- [ ] **Post-quantum transport**: Not started · nowhere yet. Everything: hybrid X25519MLKEM768 TLS on the client and the self-host server.
- [ ] **Hardware-backed biometric unlock**: Not started · nowhere yet. Everything: key slots in the Secure Enclave, StrongBox or TPM that reset when biometrics change.
- [ ] **Security-key 2FA on the account**: Not started · nowhere yet. Everything: FIDO2 keys and passkeys as account second factors.
- [ ] **Verified local connections**: Not started · nowhere yet. Everything: native-messaging and IPC peer checks by code signature or process identity on every OS.
- [ ] **Per-item exposure tiers**: Not started · nowhere yet. Everything: the three per-item tiers and the approval default for high-risk items.

## Platforms & apps

- [x] **Offline access**: Done · core, desktop, cli. Nothing for desktop and CLI beyond a background offline queue; the extension and mobile clients need it when they exist.
- [x] **Non-Electron desktop**: Done · desktop. Nothing; native targets come with the OS-provider features.
- [ ] **Windows app**: Partial · desktop. A Windows build and test run in CI, MSIX packaging, the passkey plugin and auto-type.
- [ ] **macOS app**: Partial · desktop. A signed macOS build for Apple silicon and Intel, and the native AutoFill and passkey extensions.
- [ ] **Linux app**: Partial · desktop. Flatpak, deb and rpm packaging, the SSH agent and TPM-sealed unlock.
- [ ] **iPhone and iPad app** _(deferred: native mobile, D122)_: Not started · nowhere yet. Everything: the SwiftUI app and the UniFFI bindings.
- [ ] **Android app** _(deferred: native mobile, D122)_: Not started · nowhere yet. Everything: the Jetpack Compose app and F-Droid build.
- [ ] **Web vault**: Not started · nowhere yet. Everything: a web vault on the wasm core.
- [ ] **Usable free tier**: Not code yet · nowhere yet. Needs the plan and pricing decision and its enforcement on the server.

## Trust & transparency

- [ ] **Signed releases and SBOMs**: Partial · nowhere yet. Signed, attested releases, SBOMs and cargo-vet; there are no releases yet.
- [ ] **Reproducible builds**: Not started · nowhere yet. Everything: reproducible extension, desktop and Android builds and an independent rebuilder.
- [ ] **Open-source clients**: Not code yet · nowhere yet. Needs the licence choice: the plan says GPL-3.0 but the code is Apache-2.0 today; and a public repository. Licence settled as Apache-2.0 for now (D120).
- [ ] **Open-source server**: Not code yet · nowhere yet. Needs the licence choice: the plan says AGPL but the server is Apache-2.0 today. Licence settled as Apache-2.0 for now (D120).
- [ ] **Published independent audits**: Not code yet · nowhere yet. Needs firms engaged and reports published, plus SOC 2.
- [ ] **Public bug bounty**: Not code yet · nowhere yet. Needs a public bounty programme with malicious-server scenarios in scope.
- [ ] **Security advisories in release notes**: Not code yet · nowhere yet. Needs a release process that lists advisory IDs and CVEs.
- [ ] **Ownership and exit guarantee**: Not code yet · nowhere yet. Needs the ownership-change notice, the no-custody-move promise and a funded succession plan.
- [ ] **Data residency choice**: Not code yet · nowhere yet. Needs hosted US and EU regions and a region picker at signup.
- [ ] **Readable security white paper**: Not code yet · nowhere yet. Needs a public white paper with sequence diagrams for every flow and the documented vault format.

## Built ahead of its phase

- `cli`: Partly built: the `lockdown` binary (bins/cli/src/main.rs) does init, add/edit/list/show logins, account register/login, sync, kit, device list/revoke, attach/fetch, audit, checkpoint, code and match, with end-to-end tests (bins/cli/tests/end_to_end.rs:42). No JSON output, no biometric unlock through the desktop app, no secret references.
- `local-only-mode`: Largely built: "This device only" on first run makes a vault and kit with no account, and an account can be added later from Status (apps/desktop/src/features/unlock/first-run.tsx:122); `lockdown init` works without a server.
- `version-history`: Data layer built: every edit is a new signed version under its own key and old versions open (crates/vault/src/vault.rs:1240 seal_open_and_history); no history view, restore action or retention rule.
- `seed-safe-mode`: Detector only: crates/vault/src/crown_jewel.rs:112 looks_like_seed_phrase (test at :173); not wired to any paste or field.

Evidence (file and line, test, or decision row) for every row is in `feature-status.json`.
