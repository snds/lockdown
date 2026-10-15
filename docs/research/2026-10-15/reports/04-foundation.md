# Foundation research: a cross-platform password manager in 2026

_Compiled 2026-10-15. Several pages could not be fetched (WebFetch hit DNS failures on bitwarden.com, usenix.org, eprint.iacr.org, theregister.com and others). Some claims therefore rest on search-result summaries or on my own background knowledge. They are marked **[unverified]** or **[background]**. Re-check anything marked that way before it goes into a spec._

---

## 1. Passkeys and platform integration

### 1.1 Standards baseline
- **WebAuthn Level 3 is a W3C Recommendation** as of 25 Aug 2026. The CR snapshots were dated 13 Jan 2026 and 26 May 2026. The CR request was filed in Nov 2025. ([FIDO Alliance](https://fidoalliance.org/webauthn-level-3-is-now-a-w3c-recommendation/), [W3C CR 2026-01-13](https://www.w3.org/TR/2026/CR-webauthn-3-20260113/), [transition request](https://lists.w3.org/Archives/Public/public-transition-announce/2025Nov/0002.html)). L3 brings together:
  - conditional mediation (autofill UI for `get`) and **conditional create** (`mediation:"conditional"` on `create`)
  - `getClientCapabilities()`
  - the PRF extension
  - hints
  - **related origin requests**: `/.well-known/webauthn` lists extra origins, and browsers cap it at about 5 distinct registrable-domain labels ([ppc.land](https://ppc.land/webauthn-level-3-lets-one-passkey-cover-at-least-five-related-domains/))
  - JSON serialization helpers
- **Signal API** (`signalUnknownCredential`, `signalAllAcceptedCredentials`, `signalCurrentUserDetails`):
  - Shipped in Chrome/Edge 132 and Safari 26. Firefox does not support it.
  - Chrome's docs say **extension-based providers on desktop decide for themselves whether to honour signals**. Signals are hints, not commands. ([Chrome docs](https://developer.chrome.com/docs/identity/webauthn-signal-api))
  - Android support reportedly arrived in Chrome 144 (Dec 2025) **[unverified, single non-official source]** ([MojoAuth](https://mojoauth.com/blog/webauthn-signal-api-stale-passkeys)).
  - Design implication: implement all three signals. Use soft-delete for "unknown" credentials (hide and mark, with undo), because a buggy or malicious relying party (RP) can send a wrong list.
- **Conditional create (automatic passkey upgrade):**
  - It only fires right after a password sign-in in which the provider just filled a password.
  - On iOS 18+, Chrome's docs say support "is up to the passkey provider". If the selected provider does not implement it, no passkey is created. ([Chrome docs](https://developer.chrome.com/docs/identity/webauthn-conditional-create), [Corbado](https://docs.corbado.com/passkey-ui-flows/native/passkey-creation/automatic-upgrade))
  - So a third-party provider **must** implement conditional registration. If it doesn't, it falls behind iCloud Keychain and Google Password Manager (GPM) on passkey adoption.
  - Google extended automatic creation to Android in Oct 2025 ([Chrome blog](https://developer.chrome.com/blog/automatic-passkey-creation-android)).

### 1.2 Apple (iOS/macOS)
- **iOS 17 / macOS 14:** `ASCredentialProviderViewController` gained passkey assertion and registration for third-party providers. **[background]**
- **iOS 18:**
  - Providers can supply one-time codes and arbitrary text.
  - **Conditional registration** enables automatic passkey upgrades. Providers must opt in through the extension's Info.plist capabilities. **[background, verify exact keys]**
- **iOS/macOS 26:**
  - `ASCredentialExportManager` / `ASCredentialImportManager` provide OS-mediated, app-to-app transfer of passwords and passkeys using the FIDO credential exchange format. The user starts it and confirms with Face ID or Touch ID, and no intermediate file is written. ([Apple export docs](https://developer.apple.com/documentation/authenticationservices/ascredentialexportmanager.md), [Apple import docs](https://developer.apple.com/documentation/authenticationservices/ascredentialimportmanager.md), [9to5Mac Jun 2025](https://9to5mac.com/2025/06/13/ios-26-passkeys-password-transfer/))
  - iOS 26 also added an account-creation API and native app-side signal reporting. **[background; Corbado WWDC25 summary could not be fetched]**
- **Hybrid (QR / cross-device), confirmed June 2026:** Apple says *"All enabled credential provider extensions get Hybrid support for free through the system's implementation… no need to implement it yourself."*
  - **Caveat:** a device-bound credential "is not a passkey and cannot be returned" by a provider extension.
  - Attestation for passkeys is unsupported except in some MDM contexts.
  - An open follow-up (Aug 2026, unanswered) asks whether third-party-synced keys count as "passkeys". ([Apple Developer Forums #829541](https://developer.apple.com/forums/thread/829541))
  - **Answer:** yes, on iOS a third-party provider can act as "the phone" in a QR flow, with the OS handling BLE and the tunnel.
- **iOS 27 (WWDC 2026):** the Passwords app uses Apple Intelligence to sign in and **automatically change weak or compromised passwords** from the Security tab. It reportedly requires user approval and runs on-device or on Private Cloud Compute. ([TechRadar](https://www.techradar.com/pro/new-ios-27-passwords-app-can-automatically-change-your-passwords-for-you), [BleepingComputer](https://www.bleepingcomputer.com/news/apple/new-apple-feature-automatically-changes-your-compromised-passwords/amp/)). This is now the competitive bar for "automatic rotation".

### 1.3 Android
- **Credential Manager provider API** (Android 14+): implement a `CredentialProviderService`. It offers a unified selector for passwords, passkeys and federated credentials, with single-tap passkey sign-in from Android 15. **[background]**
- **Hybrid:** the platform supports hybrid ([Credential Manager FAQ](https://developer.android.com/identity/sign-in/credential-manager-faq)).
  - Whether a third-party provider is offered after scanning a desktop QR code is **not clearly documented**.
  - An Oct 2025 WebAuthn-list thread shows Android won't forward a hybrid request to a USB security key unless the provider handles it ([W3C list](https://lists.w3.org/Archives/Public/public-webauthn/2025Oct/0114.html)).
  - **[uncertain: test on Android 15/16 devices before promising it]**
- **Credential exchange on Android:** shipped through a Google Play Services update.
  - Bitwarden's post (updated 23 Jul 2026) confirms it ([Bitwarden](https://bitwarden.com/blog/security-vendors-join-forces-to-make-passkeys-more-portable-for-everyone/)).
  - Dashlane's help page lists 1Password as a participating Android app ([Dashlane](https://support.dashlane.com/hc/articles/32779418812050)).
  - The minimum OS level is reported inconsistently (Android 10 vs 14) ([TechRepublic](https://www.techrepublic.com/article/news-transfer-passwords-passkeys-android-managers/)).

### 1.4 Windows 11
- **Third-party passkey plugin model:** generally available with the **Nov 2025** security update. 1Password was the first partner, then Bitwarden (which may still have been in beta at launch). ([Petri](https://petri.com/windows-11-1password-bitwarden-passkey-support/), [CyberInsider](https://cyberinsider.com/windows-11-integrates-native-support-for-bitwarden-and-1password/))
- **How it works:**
  - The provider registers a **COM object** that implements `IPluginAuthenticator`.
  - It uses `WebAuthNPluginAddAuthenticator` and `WebAuthNPluginPerformUv` (user verification through Windows Hello).
  - Microsoft's Passkey Manager sample shows the pattern ([MS Learn sample](https://learn.microsoft.com/es-es/samples/microsoft/windows-classic-samples/passkeymanager)).
  - Passkeys then work in every browser and native app that calls the Windows WebAuthn API.

### 1.5 Browser extensions
- **Chrome and Firefox have no official "passkey provider" extension API.** Vendors inject a page-world script that **replaces `navigator.credentials.create/get`** ([Bitwarden contributing docs](https://contributing.bitwarden.com/architecture/deep-dives/passkeys/implementations/provider/browser-extension)).
- **Consequences:**
  - Page CSP can break the injection on Firefox.
  - Managers conflict with each other. A June 2026 forum report says 1Password began blocking other managers' overrides ([1Password community](https://www.1password.community/1password-at-home-31/1password-extension-breaks-bitwarden-passkey-support-24740)).
  - Windows Hello sometimes pre-empts the extension on Firefox ([Bitwarden community](https://community.bitwarden.com/t/passkeys-do-not-work-in-firefox-under-windows-11/91496)).
  - Chrome's `webAuthenticationProxy` exists only for remote-desktop scenarios. **[background]**
- **Strategy:** on Windows and macOS, prefer the OS provider path (the Windows plugin, the macOS AutoFill extension). Keep the extension override only as a fallback for Linux and ChromeOS. Coordinate with other vendors on an override-coexistence convention, or push for a W3C/WebExtensions provider API.

### 1.6 PRF for vault unlock
- **Bitwarden:**
  - Shipped PRF passkey login and unlock on web, then in Chromium extensions in **Nov 2025** ([Help Net Security](https://www.helpnetsecurity.com/2025/11/19/bitwarden-browser-extensions/), [Bitwarden help](https://bitwarden.com/help/login-with-passkeys/)).
  - Limits: it needs a PRF-capable browser and authenticator (e.g. YubiKey 5). On macOS it needs Chromium. On Linux the popup must be popped out. Windows 10 has reported issues.
- **Design:** use PRF output → HKDF → a key-encryption key (KEK) that wraps the account's symmetric key. Treat it as an additional unlock factor, never the only one. Support multiple enrolled PRF credentials and an evaluation salt per credential.

### 1.7 Enterprise attestation
- Apple blocks attestation for synced passkeys outside MDM contexts (forum thread above).
- Plan an **enterprise policy layer** that relies on MDM and managed-device attestation rather than per-credential attestation, and on AAGUID transparency for the provider itself.

---

## 2. FIDO Credential Exchange (CXP / CXF)

- **CXF (format):**
  - Proposed Standard published Aug 2025 ([CXF PS 2025-08-14](https://fidoalliance.org/specs/cx/cxf-v1.0-ps-20250814.html)), with errata in Mar 2026 ([errata PDF](https://fidoalliance.org/specs/cx/cxf-v1.0-ps-errata-20260309.pdf)).
  - It is transport-agnostic, but the spec says CXP SHOULD be the preferred transport.
- **CXP (protocol):** still a **Working Draft** as of mid-2026 per Corbado. It is the HPKE-based (Hybrid Public Key Encryption) remote and cross-platform transfer. ([CXP WD](https://fidoalliance.org/specs/cx/cxp-v1.0-wd-20241003.html))
- **What actually ships today is OS-mediated local exchange using CXF payloads:**
  - **Apple iOS/macOS 26** (Sept 2025). Participants: Apple Passwords, 1Password, Bitwarden, Dashlane ([Dashlane](https://support.dashlane.com/hc/articles/32779418812050)).
  - **Android via Google Play Services** (mid-2026). Participants: 1Password, Bitwarden, Dashlane.
  - NordPass, Samsung and SK Telecom are working-group members, but I found no shipping confirmation for them.
  - Microsoft Password Manager reportedly **did not export passkeys as of June 2026** ([Medium](https://danfabulich.medium.com/you-can-export-import-passkeys-now-but-only-on-ios-3b90cddf5fe6)).
- **Caveats:**
  - Passkeys that rely on a signature counter may not transfer.
  - Notes and attachments do not reliably move with the credential.
- **Implication:** CXF import and export on day one is table stakes. Being the first to ship **cross-platform CXP** (e.g. Android → iOS, or desktop ↔ mobile over HPKE) would be a real differentiator.

---

## 3. Autofill

### 3.1 Browser extension (MV3)
**Architecture:**
- An MV3 service worker holds no long-lived secrets. Keep decrypted vault state in an offscreen document or popup, or re-derive it through native messaging to the desktop app.
- Content scripts run in the isolated world.
- The inline menu renders inside an **extension-origin iframe** hosted in a closed shadow root. Bitwarden's inline menu (2024) uses this pattern. **[background]**
- Safari requires a native app container (Safari Web Extension).

**Phishing defenses (defaults):**
- Fill only on the exact host. Allow base-domain matching through the Public Suffix List (PSL) as opt-in.
- Use curated equivalent-domain groups.
- Never fill into cross-origin iframes unless they share the same registrable domain and the user explicitly confirms.
- Require a user gesture to fill.
- Never auto-submit on page load.

**DOM-based extension clickjacking (Marek Tóth, DEF CON 33, Aug 2025):**
- 10 of 11 managers leaked credentials with a single click, 9 of 11 leaked TOTP codes, and 8 of 11 allowed passkey abuse. Most also filled into all subdomains.
- The attack makes the injected UI (or the page around it) invisible with opacity tricks, so a click on a decoy banner lands on the hidden fill control.
- **Patched:** Bitwarden 2025.8.0, Proton Pass 1.31.6, Dashlane, Keeper, NordPass, RoboForm.
- **Not patched:** 1Password and LastPass called it "informative". 1Password's CISO argued no complete extension-side fix exists.
- Sources: ([The Hacker News](https://thehackernews.com/2025/08/dom-based-extension-clickjacking.html), [SecurityWeek](https://www.securityweek.com/password-managers-vulnerable-to-data-theft-via-clickjacking/amp/), [CyberInsider/Proton](https://cyberinsider.com/proton-pass-patches-dom-based-clickjacking-zero-day-vulnerability/))
- **Mitigations to build:**
  - Render the menu in the browser's top layer (Popover API) or in the extension popup and side panel.
  - Verify the computed opacity and visibility of the whole ancestor chain and of `html`/`body`.
  - Use IntersectionObserver v2 `trackVisibility`.
  - Detect overlays with MutationObserver.
  - Require a second deliberate action for TOTP, card and passkey fills.
  - Default site access to "on click" for high-risk item types.

### 3.2 Mobile
- **iOS:**
  - Use the AutoFill credential provider extension plus `ASCredentialIdentityStore`, so QuickType suggestions work without the app running.
  - The iOS 18+ one-time code identity covers TOTP.
  - Keep the extension's memory budget small. It is constrained, so a Rust core with streaming decryption helps. **[background]**
- **Android:**
  - Implement both the `AutofillService` (with inline suggestions for the keyboard/IME, API 30+) and the Credential Manager provider.
  - **Chrome 135 (Apr 2025) added native third-party autofill.** Users must enable *Chrome → Autofill Services → "Autofill using another service"*, and compatibility mode was retired in summer 2025. ([Google Dev Blog](https://developers.googleblog.com/chrome-3p-autofill-services/), [9to5Google](https://9to5google.com/2025/02/07/chrome-autofill-password-manager/))
  - Onboarding must walk users through that setting.
  - Use an **Accessibility-service fallback** only as opt-in: Play policy restricts it, and it is a security risk. **[background]**

### 3.3 Desktop
- **macOS:**
  - The AutoFill provider extension covers Safari and native apps.
  - 1Password's **Universal Autofill** uses the Accessibility API and ⌘\ to fill into any app, sudo prompts and admin dialogs ([1Password support](https://support.1password.com/mac-universal-autofill/)).
- **Windows has no OS app-autofill framework.**
  - 1Password still has no Windows Universal Autofill, only Quick Access and "Type in Window" ([1Password community](https://1password.community/discussion/comment/663831)).
  - Options: UI Automation plus a global hotkey with KeePass-style auto-type (a typed keystroke sequence), verified against the target process's signed binary.
- **Desktop IPC risk:** an NDSS 2025 paper found macOS gives managers OS checks of who is calling, while Windows does not. That allowed proof-of-concept credential theft over the managers' IPC (inter-process communication) channels. ([NDSS](https://www.ndss-symposium.org/?p=23650)). Authenticate every native-messaging and IPC peer by code signature or process identity.

### 3.4 Rotation and change-password
- Support `/.well-known/change-password` (W3C) as a deep link. 1Password's own staff point to it as their "automation" ([1Password community](https://1password.community/discussion/75589/feature-request-auto-change-passwords)).
- Dashlane's old Password Changer is gone. **[background; no 2025–26 confirmation found]**
- Apple iOS 27 now does AI-driven bulk changes (§1.2). A third-party Mac "Password App" uses local AI agents ([feedbagel](https://feedbagel.com/post/password-app-automates-password-updates-using-local-ai-agents-on-mac)).
- 1Password's 2025–26 agent work is about **letting agents log in without seeing secrets**: Browserbase early access from Oct 2025, and "1Password for Claude" from Jul 2026. It is not about rotation. ([it-connect](https://www.it-connect.fr/1password-for-claude-connexion-sans-mot-de-passe/))

---

## 4. Security architecture

### 4.1 Lessons from the 2026 ETH Zurich / USI analysis
**The study:**
- "Zero Knowledge (About) Encryption: A Comparative Security Analysis of Three Cloud-based Password Managers" (Scarlata et al., USENIX Security 2026; [ePrint 2026/058](https://eprint.iacr.org/2026/058), [USENIX](https://www.usenix.org/conference/usenixsecurity26/presentation/scarlata)).
- Threat model: a **fully malicious server**.
- Results: **12 attacks on Bitwarden, 7 on LastPass, 6 on Dashlane**. They range from integrity violations to compromise of an entire organization's vaults, and most allow password recovery.
- Public reporting began in Feb 2026.
- 1Password was also analysed and reportedly held up best, thanks to its Secret Key **[secondary sources]**.
- A secondary write-up says Bitwarden fixed 7 attacks and classed 3 as design decisions **[unverified]**.
- Bitwarden's response framed the fully-malicious-server model as hypothetical **[snippet only]**. ([USI](https://www.usi.ch/en/feeds/34363), [anti-malware.ru](https://www.anti-malware.ru/news/2026-02-17-111332/49081))

**The one confirmed example:** in enterprise and account recovery, Bitwarden's and LastPass's clients **fetched public keys from the server without authentication**. The server could swap in its own key and capture escrowed keys. ([secure.com summary](https://www.secure.com/blog/researchers-expose-major-security-gaps-in-leading-password-managers))

**Anti-patterns to design out.** The list is partly inferred from that example and from prior ETH work on MEGA and Nextcloud **[background]**:
1. Public keys that are not authenticated. Fix: key transparency or signed identity keys plus device cross-signing.
2. Malleable or unauthenticated ciphertext, or item fields that can be encrypted independently and then swapped. Fix: AEAD with associated data that binds item ID, vault ID, field name and schema version.
3. Server-controlled downgrades of KDF parameters or cipher types. Fix: a client-enforced minimum and authenticated parameters.
4. Legacy formats kept for backward compatibility. Fix: hard version gates.
5. Metadata that is not encrypted. LastPass's unencrypted URLs, after the 2022 breach, enabled targeted theft of crypto-wallet seeds through 2025.
6. Organization and sharing keys the server can redistribute without the client noticing.

**Use the ETH model as your design threat model and publish a malicious-server analysis.**

### 4.2 Authentication and KDF
- **1Password 2SKD:**
  - The account key comes from the password (PBKDF2-HMAC-SHA256, 650k iterations) combined through HKDF with a 128-bit **Secret Key** generated on the device.
  - Login uses SRP-6a. A server breach alone cannot be brute-forced offline. **[background]**
- **Proton:** SRP for login. The user key is protected by a bcrypt hash of the key password and account salt. Vault keys are random 32-byte values, encrypted and signed under the user key. Items use AES-256-GCM. Sharing wraps the vault key to the recipient's public key, verified through **Key Transparency**. ([Proton Pass security model](https://proton.me/blog/proton-pass-security-model))
- **Bitwarden:**
  - The client derives the master key (PBKDF2 600k, or Argon2id), then sends a derived hash to the server. This is not a PAKE (password-authenticated key exchange).
  - Argon2id defaults are reported inconsistently: 64 MiB / 3 iterations / 4 threads at launch vs 32 MiB / 6 / 4 per a current-help summary **[unverified; bitwarden.com unreachable]** ([Bitwarden KDF help](https://bitwarden.com/help/kdf-algorithms/)).
- **OPAQUE:**
  - Published as **RFC 9807, July 2025**. It is an *Informational* CFRG document, not Standards Track ([RFC 9807](https://www.rfc-editor.org/rfc/rfc9807.html)).
  - In production at **WhatsApp** for E2EE backups, using HSMs plus the audited `opaque-ke` Rust crate. A UC analysis found that a corrupted server may get somewhat more guesses than earlier analyses assumed. ([ePrint 2023/843](https://eprint.iacr.org/2023/843), [UCI](https://ics.uci.edu/2023/06/07/on-the-road-to-internet-adoption-a-cryptographic-protocol-helps-secure-2-billion-whatsapp-users/))
  - Brave Accounts uses it too ([Brave](https://brave.com/blog/brave-accounts/)).
  - I found **no evidence that Proton has moved off SRP**.
- **Recommendation:**
  - Use **OPAQUE (opaque-ke) plus an optional 2SKD-style Secret Key**.
  - Run Argon2id client-side at **≥64 MiB, t≥3, p=4** on desktop. Use calibrated parameters on mobile and in the browser extension, with an enforced floor.
  - OWASP's minimum of 19 MiB / t=2 is tuned for servers and is too low for client-side vault keys **[background]**.

### 4.3 Post-quantum
- **Symmetric vault encryption** (AES-256 / XChaCha20) is already quantum-resistant. The exposure is in the **public-key layer**: sharing, organization keys, recovery and device enrollment. Wrapped keys stored on the server are open to harvest-now-decrypt-later.
- **What has shipped:**
  - **1Password:** hybrid X25519MLKEM768 **TLS** on its web app (31 Mar 2026). This is transport only. A July 2026 user request asks for post-quantum (PQ) vault sync. ([1Password blog](https://1password.com/blog/post-quantum-cryptography), [community](https://www.1password.community/announcements-52/april-2026-at-1password-post-quantum-protection-external-checks-close-the-access-gap-and-ai-era-security-24508))
  - **Keeper:** a headline about Keeper deploying Kyber exists, but I could not fetch it ([SecurityBrief](https://securitybrief.asia/story/keeper-deploys-kyber-to-shield-passwords-from-quantum)) **[unverified]**.
  - **Proton:** co-authored **RFC 9980 (June 2026, PQC in OpenPGP)**, which defines composite ML-KEM-768+X25519, and that construction is allowed even on v4 subkeys. I found no confirmed Proton Pass PQ rollout. ([RFC 9980](https://www.rfc-editor.org/rfc/rfc9980.txt))
  - **No major password manager is confirmed to use hybrid PQ key wrapping for sharing or recovery.**
- **Pattern to follow:**
  - Apple's iMessage PQ3 (Feb 2024): hybrid ECDH + Kyber/ML-KEM with periodic PQ rekeying **[background]**.
  - Signal's PQXDH (2023) and its 2025 sparse PQ ratchet **[background]**.
- **Recommendation:**
  - Wrap every vault and organization key to **X25519+ML-KEM-768 hybrid** recipient keys from day one, using HPKE with a hybrid KEM.
  - Sign with Ed25519, with ML-DSA crypto-agility slots.
  - Use hybrid TLS to the server.

### 4.4 Key hierarchy and sharing
- **Account root:**
  - Account Unlock Key = f(OPAQUE export_key or the KDF output, Secret Key).
  - It wraps the **User Symmetric Key**, which wraps the **User Identity Keypair** (hybrid KEM plus signature).
- **Per-vault and per-collection keys:**
  - Random 256-bit keys.
  - Items are encrypted under a per-item key, which is wrapped by the vault key. This allows cheap re-keying and selective revocation.
- **Sharing:**
  - Wrap the vault key to the recipient's hybrid public key.
  - **Authenticate public keys** with key transparency (Proton's model) plus out-of-band fingerprint or QR verification for high-value shares. This directly addresses the ETH escrow attack.
- **Revocation:** rotate the vault key when a member is removed. The client does the work and the server enforces version monotonicity.
- **Authenticated vault state:** keep a signed, hash-chained per-vault manifest (item IDs plus versions). This stops a malicious server from rolling back, deleting or reordering items.

### 4.5 Recovery
- **Options available today:**
  - 1Password Emergency Kit (Secret Key plus a sign-in template) and recovery codes **[background]**.
  - Bitwarden Emergency Access: a grantee public key with a waiting period, which is exactly the surface the ETH study attacked.
  - Proton recovery phrase plus device-based recovery.
  - Apple iCloud Keychain HSM escrow with limited attempts, plus recovery contacts.
- **Recommended menu:**
  - A recovery kit, offered as a printable PDF or a passphrase.
  - **Trusted-contact recovery with authenticated keys and a waiting period**.
  - Optional **Shamir k-of-n social recovery**.
  - Optional **HSM-backed PIN escrow** in the style of Signal's SVR (Secure Value Recovery) or WhatsApp's OPAQUE+HSM design.
  - Organization admin recovery only with explicit member consent and key-transparency verification.

### 4.6 Device trust, hardware and biometrics
- **New-device approval:** an existing device approves and transfers keys over an authenticated channel, with a QR code plus a short authentication string (a short code both screens show for the user to compare).
- **Local unlock:**
  - The biometric key is held in the Secure Enclave, StrongBox or TPM. Use `kSecAccessControlBiometryCurrentSet`-style invalidation when enrolled biometrics change.
  - Windows Hello through the TPM.
  - Linux through the Secret Service, or a TPM2 sealed key.
- **Hardware keys:** YubiKey FIDO2 as a second factor, with **PRF unlock**.
- **Desktop SSH and Git:** Bitwarden and 1Password both ship SSH agents, so this is expected **[background]**.

### 4.7 Memory hygiene
- Hold secrets in the Rust core.
  - Use the `zeroize` and `secrecy` crates.
  - `mlock` sensitive pages.
  - Never round-trip secrets as JavaScript or JVM strings.
  - Lock automatically on sleep or idle.
- **Precedent:** KeePass CVE-2023-32784 let the master password be recovered from memory, and ISE's 2019 study found secrets left in memory by several managers **[background]**.

### 4.8 Supply chain, builds and bounties
- **Builds and dependencies:**
  - Reproducible builds for the extension, desktop and Android (F-Droid-style); publish SBOMs; use SLSA provenance.
  - Pin and vendor npm dependencies. Extension and Electron stacks are exposed to npm worm-style compromises such as Shai-Hulud (2025) **[background]**.
  - Prefer a minimal JavaScript dependency tree and a Rust core under `cargo-vet` / `cargo-deny`.
- **Verification:**
  - Run independent audits every year and publish them in full.
  - Have academics run a malicious-server analysis.
  - Run a public bug bounty: HackerOne (Bitwarden-style) or Bugcrowd (1Password-style).

---

## 5. Tech stack for 2026

**Shared core in Rust is the industry consensus:**
- **Bitwarden** `sdk-internal`: Rust compiled to WASM for web and extension, and through **UniFFI** to native Swift and Kotlin apps (native rewrite announced Jan 2025). The desktop app is Electron/Angular with Rust native modules. ([Bitwarden SDK docs](https://contributing.bitwarden.com/architecture/sdk/internal), [Help Net Security](https://www.helpnetsecurity.com/2025/01/17/bitwarden-native-mobile-applications/))
- **Proton Pass:** Kotlin and Swift apps over a Rust common library ([Wikipedia](https://en.wikipedia.org/wiki/Proton_Pass)). Proton is also moving VPN and Mail cores to Rust.
- **1Password 8:** Rust core, an Electron + React desktop app, and native mobile shells **[background]**.

**Recommendation:**
- **Core:** a Rust crate workspace covering crypto, the vault model, sync, CXF import/export, the URL matcher (PSL plus equivalents), the TOTP engine and the policy engine.
- **Bindings:** UniFFI for Swift and Kotlin, `wasm-bindgen` for the extension and web vault, and native Rust for the CLI and server.
- **Mobile:** native SwiftUI and Jetpack Compose. Autofill extensions, Credential Manager and the platform keystores require native code. Kotlin Multiplatform is a reasonable choice for shared mobile UI logic, but not for crypto.
- **Desktop:**
  - **Tauri 2** (smaller footprint and attack surface, Rust-native IPC) is preferred over Electron.
  - Caveats: WebView inconsistency (WebKitGTK on Linux), and the Windows COM passkey plugin and macOS extensions still need native targets.
  - Electron is the lower-risk path if the web vault UI is reused wholesale.
  - Fully native AppKit / WinUI gives the best OS integration at about 3× the cost.
- **Sync:**
  - **Server-authoritative, E2EE, per-item versioning.** Use an optimistic-concurrency revision for each item, conflict copies, and a signed per-vault manifest hash chain for rollback and deletion integrity. Bitwarden and 1Password both sync from server revisions **[background]**.
  - CRDTs (Automerge, Keyhive-style E2EE CRDTs) are attractive for offline-first editing of notes and fields. However, they complicate access revocation and auditing. Use them **inside** an item (rich notes), not for membership.
- **Self-host server:**
  - The large Vaultwarden community shows demand for a single Rust binary. Bitwarden's official server is C#/.NET.
  - Recommendation: **Rust (axum + sqlx)** with SQLite or Postgres, S3-compatible blob storage for attachments, and one container image.
  - Run the same binary as the SaaS, configured by feature flags, so security fixes ship in parity.

---

## 6. Must-have foundation checklist

**Passkeys and standards**
- [ ] WebAuthn L3 provider on iOS/macOS (AutoFill extension, including **conditional registration**), Android (Credential Manager provider), Windows (COM plugin), with extension override as a fallback
- [ ] Signal API handling (all three, soft-delete), related origins, `getClientCapabilities`
- [ ] Hybrid: works on iOS through the system; validate on Android
- [ ] PRF unlock (platform and roaming authenticators); YubiKey second factor
- [ ] CXF import/export through the iOS 26 and Android Play Services exchange; CSV, 1PUX and Bitwarden JSON importers

**Autofill**
- [ ] MV3 extension: exact-host default, PSL plus equivalent domains, iframe policy, user-gesture fills, clickjacking-hardened top-layer UI
- [ ] iOS identity store plus OTP; Android AutofillService with inline suggestions, plus onboarding for Chrome 135's setting
- [ ] macOS accessibility-based universal fill; Windows auto-type with process verification
- [ ] TOTP storage and fill; `/.well-known/change-password` links; Watchtower-style breach, reuse and weak-password audit (HIBP k-anonymity)

**Cryptography**
- [ ] OPAQUE login + client Argon2id (≥64 MiB) + optional Secret Key
- [ ] AEAD with associated data binding item, vault, field and version; client-enforced parameter floors; no legacy formats
- [ ] All metadata encrypted, including URLs, titles and icons (fetch icons through a privacy proxy)
- [ ] Hybrid X25519+ML-KEM-768 recipient keys; Ed25519 signatures; key transparency for every public key
- [ ] Signed, hash-chained vault manifests (anti-rollback)

**Recovery and trust**
- [ ] Recovery kit, trusted contacts with authenticated keys, emergency access with a waiting period, admin recovery that requires member consent
- [ ] New-device approval from an existing device; hardware-backed biometric unlock

**Operations**
- [ ] Reproducible builds, SBOM, SLSA, pinned dependencies, signed releases
- [ ] Annual audits plus a malicious-server analysis; public bug bounty; transparency report
- [ ] Self-host as a single Rust binary with the same code as SaaS

---

## 7. Differentiating innovations (what no major password manager does well yet)

1. **A provable malicious-server guarantee.** Publish a formal model (Tamarin/ProVerif) of login, sharing and recovery under the ETH threat model, plus key transparency with a public, auditable log. No vendor offers this end to end. Proton comes closest with key transparency for addresses.
2. **Post-quantum key wrapping at rest.** Hybrid ML-KEM for sharing, organization and recovery keys. No mainstream password manager is confirmed to ship it; 1Password's PQ work is TLS-only.
3. **Cross-platform CXP over HPKE.** Live Android ↔ iOS ↔ desktop credential migration and *ongoing* two-way federation, beyond one-shot OS-local exports.
4. **Clickjack-proof fill UX.** A top-layer popover, visibility attestation (IntersectionObserver v2) and per-item-class confirmation. Ship it, and offer the code to the WebExtensions working group as the case for a real browser "credential provider" API that ends `navigator.credentials` monkey-patching.
5. **Agent-safe credentials.** Scoped, time-boxed, audited delegation to AI agents with no secret disclosure: fill through a broker, approval for each use, and a revocable capability token. 1Password has started this. Combine it with **agentic rotation** that matches Apple iOS 27, but cross-platform, open, verifiable and local-first.
6. **Passkey lifecycle intelligence.** Signal-driven cleanup, plus "this site supports passkeys: upgrade now" driven by conditional create, plus RP-side passkey health dashboards for organizations.
7. **Windows Universal Autofill done right.** UI Automation with signed-process verification and IPC peer authentication, addressing the NDSS 2025 Windows IPC gap. 1Password still has no Windows equivalent.
8. **Recovery with real UX.** Shamir social recovery plus OPAQUE+HSM PIN escrow (SVR-style), with a dry-run "recovery drill" feature. Today most users only find out at failure time that recovery doesn't work.
9. **Verifiable clients.** Reproducible builds across all platforms, plus a public binary-transparency log the clients check before unlocking.
10. **E2EE CRDT notes and fields.** Real offline collaborative editing of shared items without conflict copies, while membership stays server-authoritative.

---

## Key uncertainties to resolve before committing
- Whether Android third-party providers can act as the hybrid (QR) authenticator. Test on Android 15/16.
- The exact iOS 26 provider-side signal and account-creation entitlements (WWDC25 sessions).
- Bitwarden's current Argon2id defaults and its official per-attack response to the ETH paper.
- Keeper's PQ claim, and any Proton Pass PQ rollout.
- CXP's spec maturity: the remote protocol is still a Working Draft, so its wire format may change.
