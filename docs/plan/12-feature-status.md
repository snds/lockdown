# Lockdown: v1 feature status

_Generated from `feature-status.json` (as of `e84830d`) by `render-feature-status.py`; edit the JSON, not this file. The feature list is the 122 v1 rows of docs/research/2026-10-15/feature-matrix.json with the moves in 06-roadmap.md §1 (D8, D9)._

**Done** means it works end to end in today's surfaces (core, desktop, CLI, server, browser extension, web vault) with tests. **Partial** means some layer exists and the row says what is missing. **Not started** means nothing in code yet. **Not code yet** is work that is not software (an audit, a licence call, a bug bounty).

| | Done | Partial | Not started | Not code yet |
|---|---|---|---|---|
| **All 122 v1 features** | **55** | **35** | **22** | **10** |
| [Vault & item types](#vault--item-types) | 12 | 0 | 0 | 0 |
| [Organization & search](#organization--search) | 6 | 1 | 0 | 0 |
| [Autofill (browser)](#autofill-browser) | 7 | 1 | 1 | 0 |
| [Autofill (mobile & desktop OS)](#autofill-mobile--desktop-os) | 0 | 0 | 6 | 0 |
| [Passkeys & WebAuthn](#passkeys--webauthn) | 4 | 1 | 3 | 0 |
| [Two-factor & authenticator](#two-factor--authenticator) | 2 | 2 | 0 | 0 |
| [Vendor login bridges](#vendor-login-bridges) | 2 | 3 | 2 | 0 |
| [Sharing & collaboration](#sharing--collaboration) | 6 | 0 | 0 | 0 |
| [Family & emergency/recovery](#family--emergencyrecovery) | 1 | 3 | 1 | 0 |
| [Security monitoring & phishing defense](#security-monitoring--phishing-defense) | 4 | 4 | 0 | 0 |
| [Privacy](#privacy) | 0 | 1 | 1 | 0 |
| [Import/export & portability](#importexport--portability) | 3 | 3 | 2 | 0 |
| [Self-hosting & deployment](#self-hosting--deployment) | 0 | 3 | 0 | 1 |
| [Security architecture & crypto](#security-architecture--crypto) | 5 | 9 | 3 | 0 |
| [Platforms & apps](#platforms--apps) | 3 | 3 | 2 | 1 |
| [Trust & transparency](#trust--transparency) | 0 | 1 | 1 | 8 |

13 rows are deferred by D122 (OS-level integration and the native mobile apps); they stay on the list, marked.

## Where things stand

No mobile apps exist (no Swift or Kotlin; crates/ffi is a skeleton). The shipped surfaces today are the core crates, the Tauri desktop app (built and tested on Linux in CI; macOS and Windows only compile-checked for platform-auth), the `lockdown` CLI, the self-host server, a web vault the server serves (the same core on wasm32-wasip1 in a worker, doc 17), and an MV3 browser extension for Chromium and Firefox that works through the running desktop app (doc 16; Linux and macOS only until the identity crate has a Windows named-pipe transport; only Chromium is exercised in tests). The CLI still carries its own copy of several flows. policy, approval, bridge-host and ffi are skeletons. The code is licensed Apache-2.0 (D120). Lockdown's clients and server speak TLS 1.3 with the hybrid X25519MLKEM768 group first (crates/tls, doc 20); plain HTTP is refused away from loopback. Rows judged `done` are done for today's surfaces only; each still needs its mobile half, and the web vault refuses some features in words for now (doc 17 §10). OS-level integration and native mobile are deferred (D122): those rows carry `deferred`.

## Vault & item types

- [x] **Logins**: Done · core, desktop, cli, extension, web. Nothing for desktop and the extension; the web vault and mobile.
- [x] **Secure notes**: Done · core, desktop, web. Nothing for desktop; mobile and web views later.
- [x] **Payment cards**: Done · core, desktop, web, extension. An OS-provider card fill (deferred).
- [x] **Identities and addresses**: Done · core, desktop, web, extension. Nothing for desktop, web and the extension; mobile.
- [x] **Typed item templates**: Done · core, desktop, web. Template sharing between people; more built-in types as asked for.
- [x] **Custom fields**: Done · core, desktop, web. Links across vaults (links stay in one vault by design, D235).
- [x] **File attachments**: Done · core, desktop, server. Chunked upload and download in sync (a sealed blob still moves whole), S3-compatible object storage on the server, and attachments in the web vault (refused in words today).
- [x] **Password and passphrase generator**: Done · core, desktop, web, extension, cli. Refreshing the published site-rules snapshot on a schedule.
- [x] **Wi-Fi network items**: Done · core, desktop, web. Joining a network from the item (OS-level, deferred).
- [x] **Item expiry reminders**: Done · core, desktop, web. Renewal kinds for domains and certificates; the signed-date check.
- [x] **Trash with recovery**: Done · core, desktop, web. Trash in the CLI.
- [x] **Item archive**: Done · core, desktop, web. Nothing for desktop.

## Organization & search

- [x] **Folders**: Done · core, desktop, web. Drag and drop; folders from imports.
- [x] **Tags**: Done · core, desktop, web. Nothing for desktop; the extension and web vault views.
- [x] **Multiple vaults**: Done · core, desktop, web. Several vaults in the CLI; a free-tier limit once plans exist.
- [x] **Smart categories**: Done · core, desktop, web. Nothing for the desktop; Signal-hidden passkeys join Needs attention once the Signal API exists.
- [x] **Duplicate finder**: Done · core, desktop, web. Run it on its own after an import; a side-by-side compare.
- [x] **Favorites and recency ranking**: Done · core, desktop, web. Fill counts from the browser extension.
- [ ] **Global quick-access palette**: Partial · desktop, web. The system-wide hotkey outside the window, fill and show-code actions, and a shared Rust search index (search runs over summaries in TypeScript).

## Autofill (browser)

- [x] **Inline fill menu**: Done · extension. A Firefox test run; Safari waits on OS-level work.
- [x] **Clickjacking-hardened fill**: Done · core, extension. Firefox has a hit test instead of IntersectionObserver v2.
- [x] **Precise URL matching**: Done · core, cli, extension. Per-item match rules in the desktop editor (new sites get exact_host).
- [x] **Save and update prompts**: Done · core, desktop, extension. A Firefox test run.
- [x] **Fill keyboard shortcut**: Done · extension. A Firefox test run; letting people change the keys.
- [x] **Card and address fill**: Done · core, extension. A Firefox test run; more checkout layouts as they turn up.
- [x] **HTTP Basic Auth fill**: Done · core, extension. Tested on Chromium only; Firefox run.
- [ ] **Browser extension coverage**: Partial · core, desktop, extension. Store packaging, a popup on unicron, a Firefox test run, Windows (named-pipe transport); Safari waits on OS-level work.
- [ ] **Per-site fill fixes**: Not started · nowhere yet. Per-site user overrides synced end to end and a signed community fix list.

## Autofill (mobile & desktop OS)

- [ ] **iOS AutoFill provider** _(deferred: native mobile, D122)_: Not started · nowhere yet. Everything: the iOS app and its AutoFill credential provider extension.
- [ ] **Android autofill and Credential Manager** _(deferred: native mobile, D122)_: Not started · nowhere yet. Everything: the Android app, AutofillService and CredentialProviderService.
- [ ] **Autofill health check** _(deferred: OS-level, D122)_: Not started · nowhere yet. Everything: there is no OS provider or extension link to check yet.
- [ ] **One-time-code AutoFill on iOS** _(deferred: native mobile, D122)_: Not started · nowhere yet. Everything: one-time-code AutoFill needs the iOS app first.
- [ ] **Native macOS AutoFill** _(deferred: OS-level, D122)_: Not started · nowhere yet. Everything: the native macOS credential provider extension for passwords, passkeys and codes.
- [ ] **Fill into any Mac app** _(deferred: OS-level, D122)_: Not started · nowhere yet. Everything: accessibility-based fill on a global shortcut with a code-signature check.

## Passkeys & WebAuthn

- [x] **Passkey storage and sync**: Done · core, desktop, extension. The OS providers (macOS, Windows, iOS, Android) are deferred; through the extension, passkeys are made, used and synced.
- [x] **Browser passkey fallback**: Done · core, desktop, extension. A Firefox test run; Windows waits on the named-pipe transport; Safari and the OS providers are deferred.
- [x] **WebAuthn Signal API handling**: Done · core, desktop, extension. A Firefox test run.
- [x] **Passkey management view**: Done · core, desktop. Last use is per computer, not synced.
- [ ] **Automatic passkey upgrade**: Partial · core, extension. The OS-level automatic upgrade (deferred); in the extension it works only right after Lockdown filled that login's password on that site.
- [ ] **Phone as authenticator (iPhone)** _(deferred: native mobile, D122)_: Not started · nowhere yet. Everything: needs the iOS credential provider.
- [ ] **Phone as authenticator (Android)** _(deferred: native mobile, D122)_: Not started · nowhere yet. Everything: needs the Android CredentialProviderService.
- [ ] **Windows 11 passkey plugin** _(deferred: OS-level, D122)_: Not started · nowhere yet. Everything: the COM IPluginAuthenticator plugin, Windows Hello verification and MSIX packaging.

## Two-factor & authenticator

- [x] **Built-in TOTP codes**: Done · core, desktop, cli, web. Nothing in code for desktop; keeping it on the free tier is a pricing rule.
- [x] **TOTP autofill**: Done · core, extension. A Firefox test run.
- [ ] **Authenticator importers**: Partial · core, desktop, cli. Google Authenticator transfer batches and Aegis (incl. encrypted), 2FAS, Ente, Proton and Bitwarden files; only single otpauth:// and steam:// links import today.
- [ ] **Code display aids**: Partial · core, desktop, web. Clock-skew correction against server time and search focus on launch (haptics are a phone item).

## Vendor login bridges

- [x] **Steam Guard codes**: Done · core, desktop, cli, web. Nothing in code for desktop; no UI test exercises the Steam variant specifically.
- [x] **Large-type display**: Done · core, desktop, web. Nothing for desktop and web; phones.
- [ ] **Steam maFile import**: Partial · core. File parsers for SDA (incl. passkey-encrypted), WinAuth and Steam v2 JSON, with identity_secret put in the high-risk class.
- [ ] **TV and console activation by scan** _(under review)_: Partial · core, desktop. The scanner, the activation-page allow-list, opening the real page with the code and autofill, and the core producing approval requests (the sheet is a seam).
- [ ] **Anti-phishing approval gate** _(under review)_: Partial · core, desktop. Wire the gate into real device-code, Steam QR and passkey QR flows; nothing calls it yet.
- [ ] **Federated sign-in map**: Not started · nowhere yet. Everything: capturing provider buttons and redirects, the blast-radius view and recovery warnings.
- [ ] **Service 2FA setup guides**: Not started · nowhere yet. Everything: setup cards from 2fa.directory and the passkeys directory with attribution.

## Sharing & collaboration

- [x] **Shared vaults**: Done · core, desktop, server. Sharing in the CLI and the web vault.
- [x] **Granular share permissions**: Done · core, desktop, server, extension. Groups of people as members.
- [x] **Single-item sharing**: Done · core, desktop, server. Sharing from the extension and the web vault.
- [x] **Share links for non-users**: Done · core, desktop, server, web. Item and file links made from the web vault (the desktop makes them; any browser opens them).
- [x] **Request a secret**: Done · core, desktop, server, web. Keeping an answer as a note from the web vault.
- [x] **Fill-only sharing**: Done · core, desktop, extension. Nothing for desktop and the extension; the web vault.

## Family & emergency/recovery

- [x] **Emergency access**: Done · core, desktop, server. Reminders to the owner during the wait by email or OS notification.
- [ ] **Family plan**: Partial · core, desktop, server. Plan and billing logic, which waits on the pricing decision; the household vault and organizer are built.
- [ ] **Recovery kit**: Partial · core, desktop, cli, server. Make the "test it now" step forced at signup; today it is an optional drill with a reminder.
- [ ] **Recovery drill**: Partial · core, desktop, cli. The yearly prompt, a last-success date, and checks of heirs' keys and legacy settings.
- [ ] **Password reminder for biometric users**: Not started · nowhere yet. Everything: a periodic account-password prompt (default 14 days) once biometric unlock exists.

## Security monitoring & phishing defense

- [x] **Breached password check**: Done · core, desktop. The extension's and web vault's view of the same findings.
- [x] **Weak and reused password report**: Done · core, desktop. Reuse across more than one vault once multiple vaults land.
- [x] **Missing 2FA report**: Done · core, desktop. A refresh schedule for the bundled 2fa.directory snapshot (D129).
- [x] **Lookalike-domain warning**: Done · core, desktop, extension. Nothing for desktop and the extension; the web vault.
- [ ] **Email breach monitoring**: Partial · server. A breach feed key (the built-in feed needs a paid key) and alias monitoring.
- [ ] **Breach-response playbooks**: Partial · core, desktop. The seven guided incident flows from the plan; seven plain playbooks exist.
- [ ] **One-tap change-password link**: Partial · core, desktop, extension. The popup opens the well-known page, not the security center's checked link; curated per-site recipes.
- [ ] **Vault account sign-in alerts**: Partial · core, desktop, server. OS notifications and email verification for new devices (no SMTP setting yet); the session list that ends tokens at once is built.

## Privacy

- [ ] **Clipboard and screenshot hygiene**: Partial · desktop, web. Keep copies out of Windows clipboard history and cloud clipboard, and block screen capture on reveal screens.
- [ ] **Third-party alias services**: Not started · nowhere yet. Everything: alias generation through the six services' API keys.

## Import/export & portability

- [x] **Importers from other managers**: Done · core, desktop. Check each format against one real export (samples are built from the documented formats); 1Password .1pif, Dashlane .dash, passkeys from Proton and Keeper; the CLI on the core importers.
- [x] **Complete export**: Done · core, desktop. Export in the CLI; restored versions keep their original dates.
- [x] **Published backup format**: Done · core. A stand-alone open-licence reference decoder outside the workspace; the spec, test vectors and KDBX export are built.
- [ ] **Lossless transfer**: Partial · core, desktop. File bytes through CXF, which waits on CXP (still a draft); Wi-Fi and identity documents now travel in CXF's own types, and Lockdown backups stay lossless.
- [ ] **Encrypted export**: Partial · core, desktop. Exports to a public key (age recipients); password-locked Lockdown and KDBX files and the plain-export confirm are built.
- [ ] **Automatic local backups**: Partial · core, desktop. A last-backup indicator outside Settings and the emergency sheet.
- [ ] **iOS credential exchange** _(deferred: native mobile, D122)_: Not started · nowhere yet. Everything: needs the iOS app; the CXF model it would map through exists.
- [ ] **Android credential exchange** _(deferred: native mobile, D122)_: Not started · nowhere yet. Everything: needs the Android app; the CXF model it would map through exists.

## Self-hosting & deployment

- [ ] **Official self-hosting**: Partial · server, web. An admin page, scheduled restore drills, the Backup Key on the recovery kit, and support; packaging, the operator guide and encrypted server backups are built.
- [ ] **Lightweight single container**: Partial · server. Publishing the image, Postgres, and S3-compatible attachment storage; the image is built and smoke-tested in CI (5.5 MiB idle, 6.2 MiB loaded, 53 MB image), attachments can be stored as files.
- [ ] **Self-hosted live sync**: Partial · core, desktop, server. The mobile push relay (shape in D229, mobile deferred), pings to recipients of items shared on their own, and the web vault subscribing.
- [ ] **Same code as the cloud**: Not code yet · nowhere yet. Needs a cloud, a feature-flag release process and advisory IDs; nothing to build until a hosted service exists.

## Security architecture & crypto

- [x] **End-to-end encryption**: Done · core, server, desktop, cli, web. Decide whether item kinds and field names (visible per D48) must be hidden too; then the external audit.
- [x] **Password-authenticated login (PAKE)**: Done · core, server, desktop, cli, web. Nothing in code; it goes into the external audit.
- [x] **Bound authenticated encryption**: Done · core. Nothing in code; it goes into the external audit.
- [x] **Rollback and deletion protection**: Done · core, server, cli, desktop, web. Nothing for v1 beyond the audit.
- [x] **Post-quantum transport**: Done · core, desktop, cli, server. The web vault's transport is the browser's own TLS; an external audit of the hybrid group code (audit item, D215).
- [ ] **Secret Key**: Partial · core, desktop, cli, server, web. QR transfer of the Secret Key to a new device (it is typed from the kit today) and the opt-out warning path.
- [ ] **Strong key derivation**: Partial · core. Per-device calibration above the floor and renegotiation on a stronger device (every device uses the desktop floor, D49).
- [ ] **Malicious-server resistance**: Partial · core, server. The Tamarin or ProVerif model of login, sharing and recovery, and the academic review (not code); sharing's defences (signed envelopes, membership chain, KT-checked keys) are built.
- [ ] **Authenticated public keys**: Partial · core, desktop, server, web. Passphrase-verified invites, scanning the QR fingerprint (the payload exists), and log mirrors.
- [ ] **Post-quantum key wrapping**: Partial · core. Wrapping vault, sharing, organisation and recovery keys to hybrid recipients, and ML-DSA signatures.
- [ ] **New-device approval with code match**: Partial · core, desktop. Approving a new device from an existing one over an authenticated channel (a device joins with the password and kit today).
- [ ] **Memory hygiene**: Partial · core, desktop, web. Locked memory pages kept out of swap; the webview still holds typed and revealed strings briefly (D91).
- [ ] **Verified local connections**: Partial · desktop, extension. Code-signature checks on macOS and Windows and the Windows named-pipe transport; on Linux and macOS the native host and the desktop check each other by path and SHA-256 today.
- [ ] **High-risk item class**: Partial · core, desktop, web. The legacy-release rule. Crown jewels stay out of exports and backups unless opted in per item (D121), and items move into and out of the class.
- [ ] **Hardware-backed biometric unlock**: Not started · nowhere yet. Everything: key slots in the Secure Enclave, StrongBox or TPM that reset when biometrics change.
- [ ] **Security-key 2FA on the account**: Not started · nowhere yet. Everything: FIDO2 keys and passkeys as account second factors.
- [ ] **Per-item exposure tiers**: Not started · nowhere yet. Everything: the three per-item tiers and the approval default for high-risk items.

## Platforms & apps

- [x] **Web vault**: Done · core, web, server. Import, export, backups, attachments, the security center, the recovery kit, passkeys and sharing in the web vault (each refused in words today); offline use; Firefox and Safari test runs; code splitting (the module is about 10 MB).
- [x] **Offline access**: Done · core, desktop, cli. Nothing for desktop and CLI beyond a background offline queue; the extension and mobile clients need it when they exist.
- [x] **Non-Electron desktop**: Done · desktop. Nothing; native targets come with the OS-provider features.
- [ ] **Windows app**: Partial · desktop. A Windows build and test run in CI, MSIX packaging, the passkey plugin and auto-type.
- [ ] **macOS app**: Partial · desktop. A signed macOS build for Apple silicon and Intel, and the native AutoFill and passkey extensions.
- [ ] **Linux app**: Partial · desktop. Flatpak, deb and rpm packaging, the SSH agent and TPM-sealed unlock.
- [ ] **iPhone and iPad app** _(deferred: native mobile, D122)_: Not started · nowhere yet. Everything: the SwiftUI app and the UniFFI bindings.
- [ ] **Android app** _(deferred: native mobile, D122)_: Not started · nowhere yet. Everything: the Jetpack Compose app and F-Droid build.
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
- `version-history`: Built: every version shows when, by which device and which fields changed; an earlier version shows masked and Restore saves it as the newest (15-organization). Revealing an old version's secret is not built.
- `seed-safe-mode`: Detector only: crates/vault/src/crown_jewel.rs:112 looks_like_seed_phrase (test at :173); not wired to any paste or field.

Evidence (file and line, test, or decision row) for every row is in `feature-status.json`.
