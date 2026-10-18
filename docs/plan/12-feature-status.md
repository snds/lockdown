# Lockdown: v1 feature status

_Generated from `feature-status.json` (as of `2e5a39b`) by `render-feature-status.py`; edit the JSON, not this file. The feature list is the 122 v1 rows of docs/research/2026-10-15/feature-matrix.json with the moves in 06-roadmap.md §1 (D8, D9)._

**Done** means it works end to end in today's surfaces (core, desktop, CLI, server, browser extension, web vault) with tests. **Partial** means some layer exists and the row says what is missing. **Not started** means nothing in code yet. **Not code yet** is work that is not software (an audit, a licence call, a bug bounty).

| | Done | Partial | Not started | Not code yet |
|---|---|---|---|---|
| **All 122 v1 features** | **82** | **16** | **14** | **10** |
| [Vault & item types](#vault--item-types) | 12 | 0 | 0 | 0 |
| [Organization & search](#organization--search) | 7 | 0 | 0 | 0 |
| [Autofill (browser)](#autofill-browser) | 8 | 1 | 0 | 0 |
| [Autofill (mobile & desktop OS)](#autofill-mobile--desktop-os) | 0 | 0 | 6 | 0 |
| [Passkeys & WebAuthn](#passkeys--webauthn) | 5 | 0 | 3 | 0 |
| [Two-factor & authenticator](#two-factor--authenticator) | 4 | 0 | 0 | 0 |
| [Vendor login bridges](#vendor-login-bridges) | 6 | 1 | 0 | 0 |
| [Sharing & collaboration](#sharing--collaboration) | 6 | 0 | 0 | 0 |
| [Family & emergency/recovery](#family--emergencyrecovery) | 4 | 1 | 0 | 0 |
| [Security monitoring & phishing defense](#security-monitoring--phishing-defense) | 8 | 0 | 0 | 0 |
| [Privacy](#privacy) | 1 | 1 | 0 | 0 |
| [Import/export & portability](#importexport--portability) | 5 | 1 | 2 | 0 |
| [Self-hosting & deployment](#self-hosting--deployment) | 1 | 2 | 0 | 1 |
| [Security architecture & crypto](#security-architecture--crypto) | 12 | 4 | 1 | 0 |
| [Platforms & apps](#platforms--apps) | 3 | 3 | 2 | 1 |
| [Trust & transparency](#trust--transparency) | 0 | 2 | 0 | 8 |

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
- [x] **Global quick-access palette**: Done · core, desktop, web, extension, cli. The system-wide shortcut on Wayland, which has no global-shortcut API for apps (OS-level); macOS, Windows and X11 have it.
- [x] **Smart categories**: Done · core, desktop, web. Nothing for the desktop; Signal-hidden passkeys join Needs attention once the Signal API exists.
- [x] **Duplicate finder**: Done · core, desktop, web. Run it on its own after an import; a side-by-side compare.
- [x] **Favorites and recency ranking**: Done · core, desktop, web. Fill counts from the browser extension.

## Autofill (browser)

- [x] **Inline fill menu**: Done · extension. A Firefox test run; Safari waits on OS-level work.
- [x] **Clickjacking-hardened fill**: Done · core, extension. Firefox has a hit test instead of IntersectionObserver v2.
- [x] **Precise URL matching**: Done · core, cli, extension. Per-item match rules in the desktop editor (new sites get exact_host).
- [x] **Save and update prompts**: Done · core, desktop, extension. A Firefox test run.
- [x] **Per-site fill fixes**: Done · core, desktop, extension. Entries in the signed community fix list (it ships empty; the format and signing are built).
- [x] **Fill keyboard shortcut**: Done · extension. A Firefox test run; letting people change the keys.
- [x] **Card and address fill**: Done · core, extension. A Firefox test run; more checkout layouts as they turn up.
- [x] **HTTP Basic Auth fill**: Done · core, extension. Tested on Chromium only; Firefox run.
- [ ] **Browser extension coverage**: Partial · core, desktop, extension. Store submission (Sean's store accounts) and Safari (OS-level); store-ready packages, the Windows named pipe, the popup on unicron, and pairing and fill tested in Chromium and Firefox are built.

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
- [x] **Automatic passkey upgrade**: Done · core, extension. The OS-level automatic upgrade (deferred); in the extension it follows any sign-in Lockdown saw.
- [x] **WebAuthn Signal API handling**: Done · core, desktop, extension. A Firefox test run.
- [x] **Passkey management view**: Done · core, desktop. Last use is per computer, not synced.
- [ ] **Phone as authenticator (iPhone)** _(deferred: native mobile, D122)_: Not started · nowhere yet. Everything: needs the iOS credential provider.
- [ ] **Phone as authenticator (Android)** _(deferred: native mobile, D122)_: Not started · nowhere yet. Everything: needs the Android CredentialProviderService.
- [ ] **Windows 11 passkey plugin** _(deferred: OS-level, D122)_: Not started · nowhere yet. Everything: the COM IPluginAuthenticator plugin, Windows Hello verification and MSIX packaging.

## Two-factor & authenticator

- [x] **Built-in TOTP codes**: Done · core, desktop, cli, web. Nothing in code for desktop; keeping it on the free tier is a pricing rule.
- [x] **TOTP autofill**: Done · core, extension. A Firefox test run.
- [x] **Authenticator importers**: Done · core, desktop. Proton Authenticator's locked export (refused in words today); the web vault (imports are refused there).
- [x] **Code display aids**: Done · core, desktop, web, extension. Nothing for desktop and web; haptics are a phone item.

## Vendor login bridges

- [x] **Steam Guard codes**: Done · core, desktop, cli, web. Nothing in code for desktop; no UI test exercises the Steam variant specifically.
- [x] **Steam maFile import**: Done · core, desktop. Nothing for desktop; trades are never confirmed by design.
- [x] **Anti-phishing approval gate** _(under review)_: Done · core, desktop, web. Nothing for desktop and web: device codes, passkey QR and Steam QR go through the gate (only a camera scan passes).
- [x] **Large-type display**: Done · core, desktop, web. Nothing for desktop and web; phones.
- [x] **Federated sign-in map**: Done · core, desktop, extension. Capture on mobile; more provider sign-in pages as they turn up.
- [x] **Service 2FA setup guides**: Done · core, desktop. Refreshing the bundled directories on a schedule.
- [ ] **TV and console activation by scan** _(under review)_: Partial · core, desktop, web, extension. Checking the allow-list's parameter names against the live vendor pages, and exercising the desktop camera in a real engine; scan, allow-list, approval, opening the page and filling are built and tested in Chromium.

## Sharing & collaboration

- [x] **Shared vaults**: Done · core, desktop, server. Sharing in the CLI and the web vault.
- [x] **Granular share permissions**: Done · core, desktop, server, extension. Groups of people as members.
- [x] **Single-item sharing**: Done · core, desktop, server. Sharing from the extension and the web vault.
- [x] **Share links for non-users**: Done · core, desktop, server, web. Item and file links made from the web vault (the desktop makes them; any browser opens them).
- [x] **Request a secret**: Done · core, desktop, server, web. Keeping an answer as a note from the web vault.
- [x] **Fill-only sharing**: Done · core, desktop, extension. Nothing for desktop and the extension; the web vault.

## Family & emergency/recovery

- [x] **Emergency access**: Done · core, desktop, server. Reminders to the owner during the wait by email or OS notification.
- [x] **Recovery kit**: Done · core, desktop. The web vault (it can't run the kit test).
- [x] **Recovery drill**: Done · core, desktop. The web vault.
- [x] **Password reminder for biometric users**: Done · core, desktop. Nothing for desktop; never shown in the web vault by design.
- [ ] **Family plan**: Partial · core, desktop, server. Plan and billing logic, which waits on the pricing decision; the household vault and organizer are built.

## Security monitoring & phishing defense

- [x] **Breached password check**: Done · core, desktop. The extension's and web vault's view of the same findings.
- [x] **Weak and reused password report**: Done · core, desktop. Reuse across more than one vault once multiple vaults land.
- [x] **Email breach monitoring**: Done · core, desktop, server. Nothing for desktop and the server: the paid feed is optional, and an operator range file or service works without a key.
- [x] **Missing 2FA report**: Done · core, desktop. A refresh schedule for the bundled 2fa.directory snapshot (D129).
- [x] **Breach-response playbooks**: Done · core, desktop. The web vault (the security center is refused there today).
- [x] **Lookalike-domain warning**: Done · core, desktop, extension. Nothing for desktop and the extension; the web vault.
- [x] **One-tap change-password link**: Done · core, desktop, extension. More curated per-site pages in the signed data.
- [x] **Vault account sign-in alerts**: Done · core, desktop, server. Phone notifications (mobile, deferred); on macOS, OS notifications show only for the bundled app.

## Privacy

- [x] **Third-party alias services**: Done · core, desktop, extension. Aliases from the extension's save prompt (the popup makes them, D268); the web vault.
- [ ] **Clipboard and screenshot hygiene**: Partial · desktop, web. Verifying the Windows and macOS marks on those platforms (built, not yet run there); Linux has no capture-blocking call.

## Import/export & portability

- [x] **Importers from other managers**: Done · core, desktop. Check each format against one real export (samples are built from the documented formats); 1Password .1pif, Dashlane .dash, passkeys from Proton and Keeper; the CLI on the core importers.
- [x] **Complete export**: Done · core, desktop. Export in the CLI; restored versions keep their original dates.
- [x] **Encrypted export**: Done · core, desktop. An external review of Lockdown's own age v1 code (audit item, D273); it interoperates with rage both ways.
- [x] **Published backup format**: Done · core. A stand-alone open-licence reference decoder outside the workspace; the spec, test vectors and KDBX export are built.
- [x] **Automatic local backups**: Done · core, desktop. Nothing for desktop; the web vault (backups are refused there).
- [ ] **Lossless transfer**: Partial · core, desktop. File bytes through CXF, which waits on CXP (still a draft); Wi-Fi and identity documents now travel in CXF's own types, and Lockdown backups stay lossless.
- [ ] **iOS credential exchange** _(deferred: native mobile, D122)_: Not started · nowhere yet. Everything: needs the iOS app; the CXF model it would map through exists.
- [ ] **Android credential exchange** _(deferred: native mobile, D122)_: Not started · nowhere yet. Everything: needs the Android app; the CXF model it would map through exists.

## Self-hosting & deployment

- [x] **Self-hosted live sync**: Done · core, desktop, web, server. The mobile push relay (D229, mobile deferred); the desktop and the web vault both hold the live channel.
- [ ] **Official self-hosting**: Partial · server, desktop, web. Update checks, the two-media backup check, and support (not code); the admin page with passkeys, invites, scheduled backups, restore drills that sign a client in, and the operator kit are built.
- [ ] **Lightweight single container**: Partial · server. Publishing the image (a release step, never automatic); Postgres, S3-compatible attachments and the smoke-tested image are built.
- [ ] **Same code as the cloud**: Not code yet · nowhere yet. Needs a cloud, a feature-flag release process and advisory IDs; nothing to build until a hosted service exists.

## Security architecture & crypto

- [x] **End-to-end encryption**: Done · core, server, desktop, cli, web. Decide whether item kinds and field names (visible per D48) must be hidden too; then the external audit.
- [x] **Secret Key**: Done · core, desktop, web, server. Camera scanning on phones (mobile, deferred); the QR or typed code moves it today.
- [x] **Password-authenticated login (PAKE)**: Done · core, server, desktop, cli, web. Nothing in code; it goes into the external audit.
- [x] **Strong key derivation**: Done · core, desktop, web, server. Nothing for desktop and web; mobile calibration.
- [x] **Authenticated public keys**: Done · core, desktop, web, server. A public mirror run by Lockdown, and mirrors the person adds in the web vault (reachable only when the server names their origin); mirror mode, the comparison, phrase-verified checks and the People screen are built.
- [x] **Bound authenticated encryption**: Done · core. Nothing in code; it goes into the external audit.
- [x] **Rollback and deletion protection**: Done · core, server, cli, desktop, web. Nothing for v1 beyond the audit.
- [x] **Post-quantum transport**: Done · core, desktop, cli, server. The web vault's transport is the browser's own TLS; an external audit of the hybrid group code (audit item, D215).
- [x] **Security-key 2FA on the account**: Done · core, desktop, web, server. An in-window FIDO2 prompt on the desktop (it uses the system browser today; the OS API is OS-level, deferred).
- [x] **New-device approval with code match**: Done · core, desktop, web, server. Nothing for desktop and web; mobile.
- [x] **High-risk item class**: Done · core, desktop, web. Nothing for desktop and web.
- [x] **Per-item exposure tiers**: Done · core, desktop, web. Nothing for desktop and web; mobile.
- [ ] **Malicious-server resistance**: Partial · core, server. The Tamarin or ProVerif model of login, sharing and recovery, and the academic review (not code); sharing's defences (signed envelopes, membership chain, KT-checked keys) are built.
- [ ] **Post-quantum key wrapping**: Partial · core, desktop, web, server, cli. Shared vaults and items shared on their own (Ed25519, D343), session tokens and the device's live sign-in proof, a post-quantum state line in Security (D346), and an outside review; hybrid wraps, ML-DSA-65 on identity manifests, vault heads (covering every item version), the log's tree heads and checkpoints, and the rollout rule are built.
- [ ] **Memory hygiene**: Partial · core, desktop, web. Locking the identity crate's own key types, and measuring web engines other than Chromium; secret pages are out of swap, core dumps are off, and typed secrets clear on hide and after a minute out of focus.
- [ ] **Verified local connections**: Partial · desktop, extension. Code-signature checks on macOS and Windows, and a Windows variant of the identity crate's peer type; both ends check each other by process, path and SHA-256 on every platform, Windows through the named pipe.
- [ ] **Hardware-backed biometric unlock**: Not started · nowhere yet. Everything: key slots in the Secure Enclave, StrongBox or TPM that reset when biometrics change.

## Platforms & apps

- [x] **Web vault**: Done · core, web, server. Import, export, backups, attachments, the security center, the recovery kit, passkeys and sharing in the web vault (each refused in words today); offline use; Firefox and Safari test runs; code splitting (the module is about 10 MB).
- [x] **Offline access**: Done · core, desktop, cli. Nothing for desktop and CLI beyond a background offline queue; the extension and mobile clients need it when they exist.
- [x] **Non-Electron desktop**: Done · desktop. Nothing; native targets come with the OS-provider features.
- [ ] **Windows app**: Partial · desktop. The passkey plugin and auto-type (OS-level, deferred), and signing; Windows builds and tests in CI, with MSI, NSIS and unsigned MSIX packages.
- [ ] **macOS app**: Partial · desktop. Signing and notarisation (waits on a Developer ID; the steps are written), and the native AutoFill and passkey extensions (OS-level, deferred); a universal build and dmg are made in CI.
- [ ] **Linux app**: Partial · desktop. A browser reaching the app inside the Flatpak, the SSH agent and TPM-sealed unlock (OS-level, deferred); deb, rpm and Flatpak packages are built and smoke-tested.
- [ ] **iPhone and iPad app** _(deferred: native mobile, D122)_: Not started · nowhere yet. Everything: the SwiftUI app and the UniFFI bindings.
- [ ] **Android app** _(deferred: native mobile, D122)_: Not started · nowhere yet. Everything: the Jetpack Compose app and F-Droid build.
- [ ] **Usable free tier**: Not code yet · nowhere yet. Needs the plan and pricing decision and its enforcement on the server.

## Trust & transparency

- [ ] **Reproducible builds**: Partial · extension, server, cli. Reproducible desktop bundles and Android builds, and an independent rebuilder; the extension zips, server and CLI binaries and the server image are byte-identical across two runner images.
- [ ] **Signed releases and SBOMs**: Partial · nowhere yet. Signing certificates and a first tag; the tag-only workflow, SBOMs, checksums, Sigstore provenance, the verification guide and cargo-vet (632 crates still exempted) are built.
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
