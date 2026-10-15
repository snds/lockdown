# Gap survey: password managers and authenticators the main reports skipped (as of 2026-10-15)

**Scope.** This covers products that reports 01–04 skip or mention in one line. I skimmed those reports first. I do not repeat what they cover: 1Password, Apple, Google Password Manager, Microsoft autofill retirement, Samsung, Firefox, Bitwarden, Vaultwarden, Proton Pass, Dashlane, Keeper, KeePassXC, NordPass, Enpass, Strongbox/KeePassium, LastPass, Authy, vendor bridges, CXP, autofill clickjacking. For each product below I list only:
- **Copy:** features or UX patterns that are genuinely distinctive.
- **Avoid:** notable failures.

**How this was researched:**
- **WebFetch:** failed with DNS errors (`ENOTFOUND`, e.g. passbolt.com).
- **WebSearch:** I ran 8 queries, covering Passbolt ×3, Psono ×2, RoboForm ×2, Zoho Vault, Heylogin, Buttercup and Padloc. Then the session's shared search budget ran out (200 per turn), so I could run no more searches.
- **GitHub:** I read primary sources first-hand from `raw.githubusercontent.com`:
  - READMEs, changelogs and docs for Aegis, 2FAS (Auth, Pass, extension), Ente Auth, Proton Authenticator, Bitwarden Authenticator, Yubico Authenticator, Tofu, gopass, pass, Buttercup, Padloc, Passbolt, Psono and Kee Vault.
- **Not covered by any of those:** RoboForm (beyond two searches), Zoho, Heylogin (beyond one search), Sticky, Norton/Avira/Kaspersky, Secrets, Google Authenticator, Microsoft Authenticator, Duo, Raivo. These come from **background knowledge (training cutoff June 2026)** and are flagged.

**Flags:**
- **[gh]**: read first-hand on GitHub this session.
- **[ws]**: from WebSearch result summaries.
- **[bg]**: background knowledge I did not re-verify this session. Verify before relying on it.
- **[unverified]**: sources conflict, or I have a single weak source.

---

## 0. Corrections to the earlier reports

1. **2FAS Pass exists.** Report 02 said it was "not found."
   - It is a local-first password manager. The phone is the vault, paired with a browser extension.
   - Sync options: your own iCloud or Google Drive with E2EE, a NAS (QNAP or Synology), or WebDAV.
   - Per-item "Security Tiers" decide whether an item reaches the extension at all, and whether using it needs confirmation on the phone.
   - Licence: BUSL-1.1. Each version becomes GPL-3.0 after 2 years. The app-store builds are free for business use. © 2025. **[gh]** [README](https://github.com/twofas/2fas-pass-android)
2. **Bitwarden Authenticator moved into the `bitwarden/android` and `bitwarden/ios` monorepos.** The Android app now needs Android 10 at minimum and targets SDK 37. **[gh]** ([android/authenticator](https://github.com/bitwarden/android/tree/main/authenticator), [old repo notice](https://github.com/bitwarden/authenticator-android))
   - My background knowledge is that the password manager has an "Allow authenticator syncing" toggle. It shows vault TOTP codes inside the Authenticator app, and new codes can default to "save to Bitwarden." There is still **no independent account sync** for the authenticator. **[bg]**
3. **Proton Authenticator exports are already a migration target.** Aegis 3.4.1 imports Proton Authenticator data, and 3.4.3 imports *encrypted* Proton exports. **[gh]** ([Aegis changelog](https://github.com/beemdevelopment/Aegis/blob/master/app/src/main/assets/changelog.html))

---

## A. Team and self-hosted password managers

### Passbolt (Luxembourg, AGPL-3.0; CE / Pro / Cloud) — latest 5.16.0, 2026-09-16 [gh]
**Copy:**
- **OpenPGP keys owned by each user, with end-to-end encryption.** The browser extension holds the private key. Every share re-encrypts the secret to each recipient's public key. Findings from audits run several times a year are published. No telemetry, and it can run air-gapped. **[gh]** ([README](https://github.com/passbolt/passbolt_api))
- **Passwords auto-expire when someone loses access.** "As a user I can see my passwords marked as expired when users lose permissions on these" (PB-23913). Admins set expiry policy, and owners get a notification on the expiry day (PB-41966). Expiry is on by default for new instances (PB-40712).
  - This is the cleanest offboarding-to-rotation pattern in the market. Copy it directly. **[gh]** ([CHANGELOG](https://github.com/passbolt/passbolt_api/blob/master/CHANGELOG.md))
- **Encrypted metadata (v5, stable since 5.5)** [ws]:
  - Names, usernames, URLs, descriptions and custom-field keys are encrypted.
  - Admin policy decides between a **shared metadata key** (auditable by the org) and **personal keys**. The docs warn that personal-key resources "won't be auditable." That is an honest, explicit trade-off.
  - A zero-knowledge mode means the server never holds the metadata key; admins hand it to users.
  - Admins can mark metadata keys as expired and rotate them.
  - Sources: ([metadata key](https://www.passbolt.com/docs/admin/resource-types/metadata-key/), [v5 dev docs](https://www.passbolt.com/docs/development/resources/v5-encrypted-metadata/), [5.5 post](https://www.passbolt.com/blog/passbolt-5-5))
- **Typed resources:** default, default-with-totp, totp-standalone, note, custom-fields, **pin-code** (5.12) and password-string. Each type has a schema. **[ws][gh]**
- **Secret revisions:** admins set how many past revisions the database keeps, and a config flag can lock that setting (PB-46107/46109). **[gh]**
- **Organization account recovery (Pro and Cloud):**
  - Users escrow an encrypted copy of their private key.
  - Only admins holding the **organization recovery key** can unlock it. That key is RSA-4096 and protected by a passphrase.
  - The docs tell the admin to **phone the requester before approving**, which builds an out-of-band identity check into the flow.
  - The user must finish recovery in the same browser.
  - SSO recovery flows were extended to OAuth2 and ADFS in 5.14. **[ws][gh]** ([account recovery](https://www.passbolt.com/docs/admin/authentication/account-recovery/), [design post](https://www.passbolt.com/blog/what-will-the-account-recovery-functionality-look-like))
- **Admin export policy against CSV-injection RCE** (PB-48415, 5.10). **[gh]**
- **Offline Mode** (5.16, PB-52633). **[gh]**
- **Rigorous security changelog.** Each release lists the vulnerabilities it fixes with their advisory IDs, for example atomic single-use token consumption and SSO-provider SSRF (MFK-01). **[gh]**

**Avoid:**
- **Paywalling safety features.** Account recovery is not in CE. **[ws]**
- **Recovery that runs through one org key.** It is effectively an admin back door, and it is manual. Community members asked for an API (an [open request](https://community.passbolt.com/t/ability-to-perform-organization-recovery-via-an-api-as-an-administrator-account/13051)). Prefer M-of-N approvals.
- **Slow native clients.** Safari reached GA only in 5.12 (May 2026). The desktop app was "pre-alpha" for years. **[gh][ws]**
- **No passkeys found.** There is only a January 2026 forum ticket, "PB-33600 Passkey Support." **[ws]** ([changelogs](https://www.passbolt.com/changelog/android))
- **Authorization bugs in the sharing model.** 5.15 fixed several: a personal-folder sharing bypass through a blank `metadata_key_type`, and a non-admin promoting a personal tag to shared. These are the risks that come with a dual personal/shared key model. **[gh]**

### Psono (self-hosted, open source; Community / Enterprise / SaaS) [ws][bg]
**Copy:**
- Client-side encryption plus a **file-server module** that encrypts files before sending them to pluggable storage backends. **[ws]**
- **API keys scoped to selected secrets**, with their own decryption key. Scripts and CI read only the secrets they are granted. **[bg]**
- **Link shares:** one-time or time-limited links to a secret. **[bg]**
- **Emergency codes:** a designated person gets access after a waiting period that you can cancel. **[bg]**
- SAML, LDAP and OIDC, audit logs and granular RBAC. **[ws]**

**Avoid:**
- **Opaque pricing.** Sources conflict: one says free up to 10 users, another says contact sales. **[ws]**
- **Heavy self-hosting ops.** It needs Docker and PostgreSQL. The community and documentation are small next to Bitwarden's. **[ws]**
- One review claims a 2026 Cure53 audit, but I found no second source **[unverified]**.
- Sources: ([OpenAlternative](https://openalternative.co/psono), [Toolradar](https://toolradar.com/guides/best-self-hosted-password-manager), [docs](https://doc.psono.com/), [GitLab](https://gitlab.com/esaqa/psono/psono-client))

### Padloc (open source, PWA + Electron/Tauri/Cordova + extension) [gh]
**Copy:**
- **Trustless, server-mediated key exchange for org invites:**
  1. The owner picks a short random passphrase.
  2. Both sides derive an HMAC key from it with PBKDF2 and sign each other's public keys through the server.
  3. The passphrase travels **out of band** (in person or by phone).
  4. The server cannot inject a key. Invites expire, which limits guessing.
  - This is a usable answer to the "malicious server swaps public keys" problem that the ETH study (report 04) raised.
- An org RSA key signs every member's identity.
- **SRP-style login:** the master password never reaches the server.
- The whitepaper is unusually readable, with ASCII sequence diagrams for every flow. Copy the documentation style.
- Sources: ([security.md](https://github.com/padloc/padloc/blob/main/security.md))

**Avoid:**
- **Ambiguous maintenance status.** AlternativeTo lists it as "discontinued," but I found no official notice **[unverified]**. The README still says self-hosting guides are "coming soon." **[gh][ws]** ([AlternativeTo](https://alternativeto.net/software/padlock))
- **PBKDF2** for both the master password and invite passphrases, where Argon2id is now the baseline.

### Zoho Vault (part of Zoho One) [ws][bg]
**Copy:**
- **Password Access Control:**
  - Some passwords require an access request with a **stated reason**.
  - The owner approves or rejects, and can revoke at any time.
  - Access is time-limited, and I believe it can be exclusive check-out/check-in. That part is **[bg]**, borrowed from sibling product ManageEngine PMP **[unverified]**.
  - This is PAM-lite inside a password manager.
- **One-action offboarding:** revoke all of a user's shared password and folder access at once.
- **Admin-enforced defaults** for new passwords, plus **password-reuse restriction** (December 2025).
- Sources: ([Access Control](https://help.zoho.com/portal/en/kb/vault/user-guide/sharing-passwords/articles/vault-password-access-control), [release notes](https://www.zoho.com/vault/release-notes.html))

**Avoid:**
- **Ecosystem lock-in.** Its value depends on Zoho Directory and One. **[bg]**
- **Admin and enterprise key recovery** gives super-admins wide reach. Whatever recovery you build, make it visible and auditable. **[bg]**

---

## B. Consumer password managers

### RoboForm (Siber Systems)
**Copy:**
- **Typed form-fill "Identities"** with categories Person, Address, Credit Card, Bank Account, Business, Passport and Car. A **Contacts** address book is kept separate and explicitly *not* used for filling. **[ws]** ([Mac manual](https://www.roboform.com/manual-mac))
- **Bookmarks** as a first-class item, one click away like browser bookmarks. Logins double as a launcher ("Go & Fill"). **[ws][bg]**
- **Safenotes** for activation codes and similar. **[ws]**
- **Hybrid passkey signing** (QR plus Bluetooth) inside the Android app's built-in browser, in 9.5.3.6. **[ws]** ([APKMirror 9.5.3.6](https://www.apkmirror.com/?p=5557016))
- **Fill-only sharing:** a recipient can log in but cannot view the password. **[bg]**

**Avoid:**
- **Time-seeded password generator.** Versions before mid-2015 seeded generation from the system clock. In 2024 a researcher used this to regenerate a 2013 password and unlock a wallet holding about $3M in BTC.
  - Lessons: use only a CSPRNG, and **tell users to rotate passwords made by affected versions**. **[bg]** ([Wired, URL unverified](https://www.wired.com/story/roboform-password-3-million-dollar-crypto-wallet/))
- **Affiliate-review noise.** Free-plan limits and prices conflict across reviews. **[ws]** ([CyberInsider](https://cyberinsider.com/password-manager/reviews/roboform-review/), [Adblock Tester](https://adblock-tester.com/ad-blockers/roboform-password-manager/))

### Heylogin (Germany) [ws]
**Copy:**
- **No master password.** You approve each unlock on a "login device": phone (secure element, "swipe to login"), FIDO2 key, Windows Hello, Touch ID, or even a smartwatch.
- The browser extension overlays login fields and asks the phone to approve.
- Keys reportedly never leave the phone.
- This is the purest "the device is the root of trust" consumer UX.
- Sources: ([login devices](https://www.heylogin.com/en/login-devices), [comparison](https://www.heylogin.com/en/comparison), [Token2 guide](https://www.token2.com/site/page/using-token2-fido2-security-keys-for-heylogin))

**Avoid:**
- **Opaque lost-device recovery.** My search found no public description of what happens when every login device is lost. A device-rooted design must document a tested recovery path, such as several enrolled devices or a printed recovery key. **[ws]**

### Sticky Password (Lamantine) [bg]
**Copy:**
- **Local Wi-Fi sync with no cloud** as a first-class choice next to cloud sync.
- **Lifetime licence.**
- **Portable USB** edition.
- A charity tie-in: a share of revenue goes to manatee conservation.

**Avoid:**
- Visible stagnation: dated UI and slow adoption of passkeys and new platform APIs. **[bg]**

### Norton / Avira / Kaspersky password managers (security-suite bundles) [bg]
**Copy:**
- **Norton:** unlock the vault by approving on your phone, and a free tier bundled with the suite. **[bg]**
- **Kaspersky:** stores **scanned document images** next to passwords, and has a built-in authenticator. **[bg]**
- Avira: nothing distinctive. It is Gen Digital bundleware.

**Avoid:**
- **Kaspersky Password Manager, CVE-2020-27020.**
  - The generator was seeded with the current time.
  - Its "more random-looking" character choice made passwords *easier* to brute-force.
  - Passwords generated from 2010 to 2019 could be enumerated in seconds to minutes.
  - **[bg]** ([Ledger Donjon, URL unverified](https://donjon.ledger.com/kaspersky-password-manager/))
- **Kaspersky's 2024 US exit.** After the US sales ban, US users were **silently migrated to UltraAV** without meaningful consent. A credential tool must never change custody without explicit consent and an export path. **[bg]**
- **Norton (Gen Digital), December 2022.** A credential-stuffing attack compromised about 6,450 accounts. Vaults were exposed where users had reused the **account password as the vault password**.
  - Lessons: enforce a separate vault secret, or derive keys so the vault does not depend on the account password, and require MFA on the account. **[bg]**

### Secrets (Outer Corner; macOS and iOS) [bg]
**Copy:**
- Truly native Apple UI.
- CloudKit sync with no vendor server.
- One-time purchase.

**Avoid:**
- An Apple-only indie with a bus factor of one (a single developer). Platform-native polish does not make up for the lack of Windows, Android or Linux. **[bg]**

### Kee Vault (Kee Vault Ltd; AGPL-3.0 with extra clauses) [gh]
**Copy:**
- **The hosted sync service stores a standard KeePass KDBX vault.** Users get cloud convenience *and* an exit path to any KeePass client, because the file is the export.
- v1 (web, a KeeWeb fork) and v2 (Flutter mobile) are interchangeable against the same service.
- The README names the exact first-party npm packages that run in the browser so auditors know where "funny business" could hide. That is a good transparency practice.
- Sources: ([keevault2](https://github.com/kee-org/keevault2), [keevault](https://github.com/kee-org/keevault))

**Avoid:**
- **Platform fragmentation.** v2 started Android-only, and there is no desktop app.
- **Support friction.** Issues are closed without review unless they were pre-approved on the forum. **[gh]**

### Buttercup (open source, discontinued) [gh][ws]
**Copy:**
- **One app opening many vault files from mixed storage backends**: local, Dropbox, Google Drive and WebDAV.
- A **portable mode** set by environment variables (`BUTTERCUP_HOME_DIR`, etc.). **[gh]**

**Avoid:**
- **Unmanaged end of life.**
  - The repositories are being archived and issues and pull requests are ignored.
  - The iOS app's last update was 2.6.3 (May 2024).
  - Users of a credential tool were left with no successor or migration tool.
  - Plan a funded maintainer handoff and ship a one-click export to a standard format *before* you need it.
  - Sources: ([GitHub org](https://github.com/buttercup), [Open Collective](https://opencollective.com/buttercup), [README](https://github.com/buttercup/buttercup-desktop))

---

## C. Developer and CLI stores

### pass (password-store) [gh]
**Copy:**
- **One encrypted file per secret in a git repo.** You get free history, sync, signed commits (`pass.signcommits`) and diffs.
- **Per-subfolder `.gpg-id`**: different recipients per folder, so team and personal folders live in one tree.
- An **extensions** mechanism (`.extensions/COMMAND.bash`). The ecosystem includes pass-otp, browserpass, and Android and iOS clients.
- Sources: ([man page](https://github.com/zx2c4/password-store/blob/master/man/pass.1), [README](https://github.com/zx2c4/password-store), [site](https://www.passwordstore.org/))

**Avoid:**
- **Metadata leaks.** File and folder names (site names and usernames) sit in plaintext in the repo.
- **Expensive rekeying** when someone leaves.
- **GPG UX** makes onboarding hard.

### gopass [gh]
**Copy:**
- **Mounted stores:** mount team stores inside your personal root "like file systems."
- `gopass recipients add/remove` plus `gopass fsck` to re-encrypt anything missing a recipient.
- An **age backend** (experimental). It supports SSH-key recipients and a `github:user` recipient that uses that user's published SSH keys. It is planned to replace GPG.
- **Domain aliases:** the same login answers for `rainforest.de` and `.com`.
- **Safecontent mode:** password, TOTP and otpauth fields are hidden in output, and each secret can declare extra `unsafe-keys`.
- **Password templates** that compute hashes (bcrypt, Argon2id, and others) from a stored secret.
- **Offline HIBP checks** against local dumps.
- Binary secrets with SHA-256 verification.
- Integrations: git-credential helper, kubectl plugin, Terraform provider, chezmoi, summon, the D-Bus **Secret Service API**, Raycast and Alfred.
- Sources: ([features](https://github.com/gopasspw/gopass/blob/master/docs/features.md), [backends](https://github.com/gopasspw/gopass/blob/master/docs/backends.md), [age](https://github.com/gopasspw/gopass/blob/master/docs/backends/age.md))

**Avoid:**
- The same filename metadata leak as pass. The `cryptfs` backend that would fix it is "highly experimental, likely broken."
- The iOS client supports only one repo.
- **[gh]**

---

## D. Authenticators

### Google Authenticator (cloud sync) [bg]
**Copy:**
- **"Transfer accounts" QR export** (`otpauth-migration://` protobuf batches). It is the de facto interchange format, so every new app must import it. Aegis and Ente both handle it. **[gh]**
- Sync is optional: the app still works with no account.

**Avoid:**
- **Sync shipped without E2EE** (April 2023). Researchers showed seeds were readable by Google, and E2EE came only later as an option. **[bg]** ([launch post, URL unverified](https://security.googleblog.com/2023/04/google-authenticator-now-supports.html))
- **MFA codes synced into the account they protect.** In the 2023 Retool breach, a social-engineered Google account exposed every synced TOTP seed and turned MFA into a single factor. **[bg]** ([Retool, URL unverified](https://retool.com/blog/mfa-isnt-mfa))
- Lessons:
  - Seed sync must be E2EE.
  - The sync account must not be the same identity the codes protect.
  - Warn users when they store an account's own TOTP inside it.

### Microsoft Authenticator [bg]
**Copy:**
- **Number matching** in push approvals, which users cannot disable. It has been enforced since 2023-05-08. ([Microsoft Learn](https://learn.microsoft.com/entra/identity/authentication/how-to-mfa-number-match))
- **Additional context:** the requesting app name and a sign-in location map. **[bg]**
- A **report suspicious activity** button on denial. **[bg]**
- Device-bound **passkeys for Entra ID** held in the app. **[bg]**

**Avoid:**
- **Backups that cannot cross platforms.** Restore works iOS-to-iOS through iCloud and Android-to-Android through Google, and needs a personal Microsoft account. **[bg]**
- **No export of third-party TOTP**, which means lock-in. **[bg]**
- **Feature churn:** autofill was removed in 2025 (covered in 01).

### Duo Mobile (Cisco) [bg]
**Copy:**
- **Verified Duo Push:** the user types the code shown on the login screen into the phone. **[bg]**
- **Risk-based escalation:** Duo switches to Verified Push only when it detects push-fatigue or unusual patterns, so the friction is adaptive. **[bg]**
- **Duo Restore:** third-party TOTP accounts are backed up to iCloud or Google Drive, encrypted with a recovery password. Duo-protected accounts use "Instant Restore." **[bg]**
- Also generates standard TOTP for any service. ([Duo](https://levelup.duo.com/duo-mobile-for-third-party-accounts))

**Avoid:**
- **Plain push approval.** The 2022 Uber and Cisco breaches used MFA-fatigue push bombing. Any "approve on phone" flow you build (for example, extension unlock) needs number matching and rate limits from day one.
- **Supplier breach.** In April 2024, Duo's telephony supplier breach exposed SMS-MFA log metadata. **[bg]**

### Yubico Authenticator (desktop, Android, iOS) [gh]
**Copy:**
- **OATH secrets stored *on the YubiKey***, not the phone. Codes appear when you tap over NFC or plug in.
- One app also manages FIDO2 passkeys (list and delete discoverable credentials), PIN and fingerprints, PIV certificates, and OTP slots.
- iOS adds a CryptoTokenKit smart-card extension. **[gh]**
- Offer a "hardware-bound" storage class for high-value TOTP. Some credentials can require a touch, and the OATH applet can be password-protected. **[bg]**
- Sources: ([yubioath-flutter](https://github.com/Yubico/yubioath-flutter), [yubioath-ios](https://github.com/Yubico/yubioath-ios))

**Avoid:**
- **Limited slots and no backup.** I recall about 32 OATH slots on YubiKey 5 before firmware 5.7, and 64 from 5.7 **[bg]**. You must enrol two keys by hand.
- **Firmware cannot be updated.** The 2024 "EUCLEAK" side-channel (firmware before 5.7) could be fixed only by replacing the hardware. **[bg]** ([YSA-2024-03, URL unverified](https://www.yubico.com/support/security-advisories/ysa-2024-03/))
- **Linux clipboard:** on Wayland, copying a code works only when the app has focus. **[gh]**

### Bitwarden Authenticator [gh][bg]
**Copy:**
- **A standalone app that optionally bridges to the password manager on the same device.** It shows vault TOTP and sets the default save location to Bitwarden. **[bg]**
- Free and open source.

**Avoid:**
- **No independent multi-device sync.** This has been the long-running top complaint (see the 02 report's community link).
- The help docs and repo layout make the feature set hard to discover. **[gh]**

### Ente Auth [gh]
**Copy:**
- **Accountless offline mode ("Use without backups").** The local vault key lives in the device keystore.
- **Account mode with full E2EE.** Tags, type, issuer, notes, and pinned and trash state are all encrypted, so the server "doesn't know anything."
- **Read-only web viewer** at auth.ente.com. Adding codes is limited to the apps.
- **Time-limited code sharing:** a link valid for 2, 5 or 10 minutes shows live codes **without revealing the secret**. Great for shared team accounts. Copy it.
- **Automatic clock-skew correction** against server time (4.4.0+), which fixes the "Firefox privacy mode shows wrong codes" class of bugs.
- **Trash** that never auto-empties.
- **Duplicate finder** after imports.
- Sorting by frequently or recently used.
- **Focus search on launch.**
- Hide codes (double-tap to reveal).
- **Export through the CLI** for automated backups.
- Simple-icons pack.
- A recovery key, plus a **verification ID** for sharing keys.
- Self-updating APKs straight from GitHub, independent of stores.
- Sources: ([features](https://github.com/ente-io/ente/blob/main/docs/docs/auth/features/index.md), [FAQ](https://github.com/ente-io/ente/blob/main/docs/docs/auth/faq/index.md), [architecture](https://github.com/ente-io/ente/blob/main/architecture/README.md), [README](https://github.com/ente-io/ente/tree/main/mobile/apps/auth))

**Avoid:**
- **KDF parameters fixed on a strong device.** The login KDF needs about **1 GB of free RAM**. It fails on 2–3 GB phones such as iPhone 7/8/SE and old iPads, and when a device is weaker than the one that set the parameters ("device isn't powerful enough").
  - Lessons: use adaptive parameters, renegotiate them on a capable device, and give the user a clear fallback message. **[gh]**
- **App lock is UI-only.** It does not re-encrypt data, and the docs have to say so explicitly. Don't let users mistake a PIN for encryption. **[gh]**

### Proton Authenticator [gh][bg]
**Copy:**
- The apps are open source (GPL-3.0). The iOS app uses clean-architecture Swift packages and targets iOS 18+.
- Optional E2EE sync with a Proton account, or an account-free local mode **[bg]**.
- Imports from Aegis, 2FAS, Ente, Google Authenticator and others (covered in 02). **[gh]** ([android](https://github.com/protonpass/android-authenticator), [ios](https://github.com/protonpass/ios-authenticator))

**Avoid:**
- Nothing serious is known yet. One watch item: the iOS repo ships Sentry, so check its telemetry defaults in a privacy-branded app. **[gh]**

### 2FAS Auth (+ browser extension, + 2FAS Pass) [gh]
**Copy:**
- **Phone-to-browser push of TOTP:**
  - Pair the extension with the app.
  - The extension sends a push to the phone.
  - You approve, and the token is typed into the page.
  - No account is needed.
- **2FAS Pass Security Tiers** (see §0): per-item control over whether autofill or the extension can see an item, or whether it needs approval on the phone.
- Bring-your-own sync: iCloud, Google Drive, NAS or WebDAV.
- Sources: ([extension](https://github.com/twofas/2fas-browser-extension), [Android](https://github.com/twofas/2fas-android), [iOS](https://github.com/twofas/2fas-ios), [Pass](https://github.com/twofas/2fas-pass-android))

**Avoid:**
- **Proprietary graphics** in "open source" repos, and a **BUSL** licence for Pass. Both weaken claims of community trust and forkability.
- **Lopsided contributor terms.** Contributions "become the property of the 2FAS community."
- **[gh]**

### Aegis (Android only, GPL-3.0) [gh]
**Copy:**
- **Documented vault format with LUKS-style key slots.**
  - A single master key is wrapped separately for each credential: password (scrypt) and biometrics (Android Keystore).
  - The docs state openly that the master key is only as strong as the weakest slot.
  - They also explain the 2³² random-nonce limit of AES-GCM. ([vault.md](https://github.com/beemdevelopment/Aegis/blob/master/docs/vault.md))
- **Periodic password reminder for biometric users.** It prevents "I forgot the password I never type." The FAQ asks users *not* to disable it. ([FAQ](https://github.com/beemdevelopment/Aegis/blob/master/FAQ.md))
- **Automatic backups to a location you choose**, with infinite versions or a single file for cloud providers, plus a **backup reminder** that shows when the last backup *succeeded*.
- **Audit log** of vault events (v3.1).
- **Code display aids:**
  - Shows the **next code** early.
  - Codes **change colour** before they expire.
  - **Haptic** cue before expiry.
  - Copying needs a second tap when tap-to-reveal is on.
  - Screen-capture blocking.
- **Duplicate prompts:** asks before saving a duplicate issuer/name, and can **auto-skip duplicates** on import.
- **Organisation:** multiple groups per entry, filtering on several groups, and sorting by last used.
- **Broadest importer set:** Google Authenticator QR, Authy, Microsoft, Battle.net, Steam, 2FAS, Ente, Proton (including encrypted exports), FreeOTP and others.
- **Exports:**
  - encrypted, plaintext, a **printable HTML** emergency sheet, or **group-filtered** exports
  - QR transfer with brightness boost
  - copy the otpauth URI
- Turns off autofill on the edit screen so the password manager can't overwrite fields.
- Signed APKs with published fingerprints.
- Source: ([changelog](https://github.com/beemdevelopment/Aegis/blob/master/app/src/main/assets/changelog.html))

**Avoid:**
- **Imports treated as trusted.** v3.4.3 fixed a **malicious icon pack that could overwrite internal files, including the vault** (path traversal). Treat every import (icon packs, CSV, backups) as hostile.
- **Unbounded icon sizes** caused out-of-memory errors (3.3.3/3.3.4).
- **Android-only by design.** It has no autofill, because Android allows only one autofill provider. **[gh]**

### Raivo OTP (iOS) [bg][unverified]
**Avoid** (cautionary tale):
- In 2023 the open-source app was **sold to a new owner (Mobime)**.
- An update soon after made **users' codes disappear**, and a **paid subscription** arrived.
- Users found that a "trusted open-source" TOTP app can change hands silently.
- Lessons:
  - Disclose ownership changes in the app.
  - Never ship a migration that can delete seeds without a verified backup first.
  - Keep export free forever.
- Source: [repo](https://github.com/raivo-otp/ios-app), which was unreachable this session.

### Tofu (iOS) [gh]
**Copy:**
- A minimal, offline authenticator that works in airplane mode with no account.
- A **community issuer-icon pipeline**: a pull request adds a PNG and maps the `issuer` string to it.
- A **graceful, public maintainer handoff**: the original author passed ownership to a named, experienced maintainer and said so in the README. This is the positive counterpart to Raivo and Buttercup.
- Source: ([README](https://github.com/calleerlandsson/Tofu))

**Avoid:**
- Stagnation. The README admits "few improvements… no new platform features."

---

## E. Synthesis for a new product

### Patterns to copy (ranked by leverage)
1. **Expire on permission loss** (Passbolt). When a member leaves a share or group, mark every item they could see as "rotate now." Pair this with the rotation features in report 04.
2. **Live TOTP sharing without the secret** (Ente). Time-boxed links for shared team accounts.
3. **Access requests with reason, approval and a time limit**, plus one-action offboarding (Zoho). This is PAM-lite for SMBs.
4. **Per-item exposure tiers** (2FAS Pass). Example tiers: never sent to the extension, extension needs approval on the phone, free fill. This also defuses clickjacking (report 04).
5. **Out-of-band verified org invites** (Padloc HMAC passphrase; Ente verification ID). This closes the malicious-server key-swap gap.
6. **Key-slot vault design plus a password reminder** (Aegis). Unlock methods are independent, and biometric-only users don't forget their password.
7. **Backup UX** (Aegis): automatic backups, a "last *successful* backup" reminder, a printable HTML emergency sheet, and group-filtered exports.
8. **Approval UX if you add phone unlock** (MS, Duo): number matching plus context (app, location), risk-adaptive Verified Push, and a "report fraud" action.
9. **Exit guarantee by format** (Kee Vault KDBX, pass files, Aegis' documented format). Publish the format, or use an open one.
10. **Clock-skew correction** (Ente), **next-code preview with colour and haptic expiry cues** (Aegis), and **focus search on launch** (Ente).
11. **CLI and dev ergonomics** (gopass):
    - mounted team stores and per-folder recipients
    - age with `github:user` recipients
    - domain aliases and safecontent mode
    - git-credential, kubectl, Terraform and Secret Service integrations
12. **Typed identities for form-fill**, separate from contacts, and Bookmarks as a launcher (RoboForm).
13. **Sync choice:** E2EE cloud, local Wi-Fi (Sticky), or NAS/WebDAV (2FAS Pass).
14. **Hardware-bound storage class** for crown-jewel TOTP (Yubico), and **device-rooted unlock without a master password** (Heylogin), but only with documented recovery.
15. **Ops hygiene from Passbolt:** typed resource schemas, secret-revision retention policy, export policy against CSV injection, and security fixes with advisory IDs in the changelog.

### Failures to design against
| Failure | Example | Countermeasure |
|---|---|---|
| Time-seeded or biased generator | Kaspersky CVE-2020-27020; RoboForm before 2015 ($3M wallet) | CSPRNG only, plus statistical tests in CI. Tell users when old versions generated their passwords. |
| Non-E2EE seed sync / MFA in the account it protects | Google Authenticator 2023; Retool | E2EE seed sync. Warn when a TOTP's issuer equals the sync identity. |
| Account password reused as vault key | Norton 2022 credential stuffing | Separate vault secret or key; MFA on the account; breach-check the account password. |
| Acquisition or closure strands users | Raivo, Buttercup, Kaspersky → UltraAV | Free export forever. In-app notice of ownership change. Never migrate custody without consent. Succession plan. |
| Platform-locked backup or no export | MS Authenticator; Authy (02) | Cross-platform encrypted backup; import and export in standard formats (otpauth, CXF). |
| Push fatigue | Uber and Cisco 2022 via Duo / MS push | Number matching, rate limits, risk escalation. |
| Hostile import files | Aegis icon-pack path traversal (3.4.3) | Sandbox and validate every import; enforce size limits (Aegis icons OOM). |
| KDF too heavy for weak devices | Ente: 1 GB RAM needed | Adaptive parameters, renegotiation, clear error copy. |
| Single admin recovery key | Passbolt org recovery | M-of-N approval, out-of-band verification, audit trail. |
| Opaque device-loss recovery | Heylogin | Require ≥2 login devices or a recovery kit at onboarding; recovery drills (report 04 §7.8). |
| Plaintext metadata in a file-based store | pass, gopass | Encrypt names; keep the path-to-ID index inside the vault. |
| Non-updatable hardware flaw | YubiKey EUCLEAK | Treat hardware as one slot among several; never make it the only path. |
| "Open source" with restrictive licence or assets | 2FAS (BUSL, proprietary graphics) | OSI licence for clients if trust is the selling point. |

### Gaps in this survey
I ran out of web searches, so these were not verified this session:
- Psono details beyond basics (API keys, link shares, emergency codes)
- Zoho check-out/exclusive access
- RoboForm fill-only sharing and the Wired generator story URL
- Sticky, Norton, Avira, Kaspersky and Secrets 2025–26 status
- Google Authenticator E2EE status in 2026
- Microsoft Authenticator backup rules
- Duo Restore and Verified Push specifics
- the Raivo timeline
- YubiKey slot counts and the EUCLEAK advisory URL
- Heylogin recovery
- Padloc maintenance status
- Proton Authenticator desktop stack and features

A follow-up turn with a fresh search budget should verify the **[bg]** items before any of them drive a product decision.
