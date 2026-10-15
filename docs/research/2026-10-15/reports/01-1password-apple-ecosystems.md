# Credential-Ecosystem Feature Inventory (as of 2026-10-15)

**How this was researched.** I used WebSearch only. Every WebFetch call failed with a DNS error (`ENOTFOUND`) for 1password.com, support.apple.com, 1password.community and other domains. So I could not read primary pages in full. Facts come from search-result summaries of official posts plus trade press, with dates noted.

Flags used below:
- **[bg]** means background knowledge I did not re-confirm this session. Verify before relying on it.
- **[unverified]** or **[conflict]** means the sources disagree or do not exist.

---

## 1. 1Password (8.x, consumer + Teams/Business/Enterprise)

### Core model and security
- **Account → vaults → items.**
  - Personal plans have a Private/Personal vault plus shared vaults.
  - Business adds groups, per-vault permissions, custom roles and an "Employee" vault per user. [bg]
  - **Collections** group vaults in the browser extension and apps. [bg]
- **Security model** [bg]:
  - *Two-Secret Key Derivation (2SKD):* the account password and a 128-bit *Secret Key* (shown as A3-…) together derive the account unlock key.
  - *SRP (Secure Remote Password)* proves who you are without sending the password.
  - Per-vault keys give end-to-end encryption.
  - The *Emergency Kit* is a PDF holding the Secret Key and sign-in details.
- **Recovery:**
  - *Recovery codes* for individuals launched in beta in April 2024. Recovery takes email verification plus the code, then issues a new account password and a new Secret Key (https://1password.com/blog/introducing-1password-recovery-codes).
  - Family Organizers can still recover family members' accounts.
  - A June 2025 forum report describes a lockout after a reset produced a new Secret Key without a usable Emergency Kit (https://www.1password.community/1password-at-home-31/no-option-to-save-my-recovery-code-22437).
- **Unlock with a passkey:**
  - Public beta since December 2023, for **new Individual accounts only**. No account password and no Secret Key.
  - In January and July 2025, staff said existing accounts were still unsupported and there was no public roadmap (https://1password.com/blog/unlock-1password-individual-passkey-beta). **[unverified]** I found no GA announcement.
- **Unlock with SSO** (Business) [bg]: Okta, Microsoft Entra ID, Google Workspace and generic OIDC. New devices are trusted by approving from an existing device.
- **Automated Provisioning without a SCIM bridge**, March–April 2026. A Users API for partners is in preview.

### Item types
The CLI docs list these 22 built-in categories (https://developer.1password.com/docs/cli/item-fields): Login, Password, Secure Note, Credit Card, Identity, Document, API Credential, Bank Account, Crypto Wallet, Database, Driver License, Email Account, Medical Record, Membership, Outdoor License, Passport, Reward Program, Server, Social Security Number, Software License, SSH Key, Wireless Router.

The full list is from [bg]; search confirmed API Credential, Crypto Wallet, Medical Record, SSH Key and Wireless Router. Items also support:
- custom sections and fields
- one-time password (TOTP) fields
- Markdown notes, tags, favorites and attachments
- **item expiry dates** with Watchtower alerts (Q1 2025)
- **"nearby" items** based on location (Q1 2025) (https://1password.com/blog/1password-q1-2025-usability-updates)

**Storage limits:**
- Historically 1 GB per Individual/Families user and 5 GB per Business user [bg].
- An older per-file cap of about 5 MB in the apps came from forum posts around 2020. **[unverified for 2026]**

### Autofill and everyday UX
- Logins, credit cards and identities fill through the browser extension (Chrome, Edge, Firefox, Safari, Brave) and the mobile autofill frameworks.
- **Autosave prompts and autofill suggestions ranked by favorites and recency.** Winter and Q1 2025 updates.
- **Phishing protection.** It refuses to fill, or warns, when the domain does not match (https://1password.com/blog/1password-product-enhancements-smarter-autofill-phishing-prevention).
- **Login creation from a service search.** It auto-fills the name, website and icon (Q2 2026 "Import, autofill, organize" post, https://1password.com/blog/import-autofill-organize-whats-new-in-1password-this-quarter).
- **Android Autofill health check** (Help → Autofill health check). It flags autofill being off or the wrong provider being selected, and deep-links to the Android setting (May 2026).
- **Native macOS AutoFill** (public beta, 8.12.22+, May 2026):
  - 1Password acts as a system credential provider, the same way Apple Passwords does. Passkeys are included.
  - Requires macOS 14+ on Apple silicon. One-time codes are "later in the beta."
  - It cannot populate the right-click AutoFill menu because Apple restricts it (https://piunikaweb.com/2026/05/27/1password-native-macos-autofill-public-beta/).
  - **[conflict]** One forum post says it is now production-ready; official posts still say beta.
- **Universal Autofill (macOS)** [bg]: fills credentials into any native app through Quick Access.
- **Quick Access**: a Spotlight-style global search, Cmd-Shift-Space on Mac and Ctrl-Shift-Space on Windows [bg].
  - September 2026 notes say the macOS 27 shortcut changed (https://www.1password.community/announcements-52/september-2026-at-1password-universal-sign-in-for-everyone-retiring-1password-7-and-a-quick-access-change-on-macos-25734). Exact new binding **[unverified]**.
- **Universal sign-in** (web/extension 8.12.36, rolled out September 2026):
  - 1Password works out which sign-in method a site expects (password, passkey, SSO and so on), so the user does not choose.
  - Existing items work unchanged.
  - One community report says it broke passkey login on a bank site; it can be turned off in extension settings.
- **Platforms:** Mac, Windows, Linux (native desktop app plus SSH agent), iOS, Android, web, and watchOS.
  - Apple Watch support shows selected items and OTPs [bg, **unverified** for 2026].
- **1Password 7 retirement** was announced in September 2026. **[conflict]** The official post says "next year"; a third-party site says it is already retired.

### Watchtower
Flags [bg plus 2025 posts]:
- compromised websites
- vulnerable passwords, checked against Have I Been Pwned (HIBP) using k-anonymity
- reused and weak passwords
- sites that support 2FA but the item has none
- unsecured HTTP
- expiring items
- sites where a passkey is available

There is also a Watchtower score. **Developer Watchtower** finds plaintext secrets and `.env` files. It was mentioned in a roughly August 2026 roundup, alongside `.env` import and Enterprise Password Manager (EPM) password import. Details **[unverified]**.

### Sharing, travel and privacy
- **Psst! item sharing.** A link shares a *snapshot copy* of an item. Options [bg]:
  - expiry from 1 hour to 30 days
  - view once
  - restrict to specific emails
  - Recipients do not need 1Password and can save the item if they have it (https://www.techradar.com/news/1password-now-lets-you-share-login-details-safely-with-a-link).
- **Travel Mode.** It removes from devices every vault not marked "safe for travel", including the keys, not just hiding them. It is toggled on 1Password.com, and Business admins can toggle it for members (https://1password.com/blog/introducing-travel-mode-protect-your-data-when-crossing-borders). No 2025–26 changes found.
- **Masked Email with Fastmail.** It creates per-site addresses from the signup page through the extension (since September 2021) (https://www.fastmail.com/help/account/1password.html).
- **Privacy.com virtual cards.** Integration launched around 2021. **Current status [unverified]**.

### Passkeys and portability
- Passkeys can be saved and used on all platforms [bg].
- **Windows 11 native passkey-provider plugin.** 1Password was the first external provider in the November 2025 Windows update (https://www.heise.de/en/news/Microsoft-enables-external-passkey-management-in-Windows-11-11075433.html).
- **Credential Exchange (FIDO CXP/CXF).** CXP is the transfer protocol; CXF is the data format. 1Password co-authored the standard.
  - iOS export through CXP shipped in June 2025.
  - Direct iOS/Android import was in Q2 2026.
  - Android Google Password Manager ⇄ 1Password transfer arrived in September 2026 (https://www.androidauthority.com/google-password-manager-import-export-support-3673351/).
  - **[conflict]** A November 2025 forum user did not see 1Password as a target in Apple Passwords' export menu, while Dashlane's docs list it as supported (https://support.dashlane.com/hc/en-us/articles/32907864739474).
- Bitwarden CSV import was improved in May 2026.

### Developer and secrets
- **SSH agent** [bg plus docs]:
  - Keys stay in vaults, and each use is approved with biometrics.
  - Git commit signing needs Git 2.34+.
  - WSL1/WSL2 work through the Windows agent with `ssh.exe`, plus a generated `.gitconfig` snippet.
  - An agent config file can choose which keys are offered (`agent.toml` [bg, **unverified** by search]) (https://developer.1password.com/docs/ssh/integrations/wsl).
- **CLI `op`** [bg]:
  - secret references (`op://vault/item/field`)
  - `op run` and `op inject`
  - item and vault management
  - biometric unlock through the desktop app
- **Shell Plugins.** Biometric authentication for more than two dozen third-party CLIs (AWS, GitHub `gh`, Stripe and others). Open source, so you can write your own.
- **Secrets Automation** [bg]:
  - **Service Accounts** use token-scoped vault access.
  - **Connect Server** is a self-hosted REST cache.
  - SDKs for Go, Python and JavaScript.
  - Kubernetes operator, Terraform provider, GitHub Actions and the VS Code extension.
- **Environments**:
  - Beta October 2025; **GA August 2026**.
  - A virtual or mounted `.env` file never exists as plaintext on disk. Access stops when 1Password locks.
  - Not available on mobile.
  - **[conflict]** Docs list Mac, Windows and Linux; a Novu handbook says Windows is unsupported (https://1password.com/blog/1password-environments-env-files-public-beta; https://developer.1password.com/docs/environments).
- **Credential Broker** (public preview July 28, 2026):
  - Uses GitHub Actions OIDC (OpenID Connect) trust policies to issue per-run credentials, with no stored pipeline secrets.
  - Generic OIDC is in beta (https://www.helpnetsecurity.com/2026/07/29/1password-privileged-access/).

### Enterprise and identity
- **Extended Access Management (XAM).**
  - Kolide was acquired in February 2024.
  - *Device Trust* (formerly Kolide) checks device health before allowing access and is no longer sold on its own.
  - Also includes shadow-IT and app discovery, and SaaS Manager (https://betakit.com/1password-looks-to-help-businesses-secure-unmanaged-devices-with-a-new-offering/).
- **Enterprise Password Manager admin policies (Q4 2025).** Autosave, autosubmit and sign-in-attempt policies, plus unlock presets.
- **Unified Access** (GA March 17, 2026).
  - An agent-security platform: discovers AI tools and agents on endpoints, finds exposed secrets (unencrypted SSH keys, plaintext `.env` files), and maps AI usage to users and devices. Audit is "coming soon."
  - Launch partners were Anthropic, Cursor, GitHub, Perplexity and Vercel (https://1password.com/press/2026/mar/1password-unified-access).
- **Privileged Access** (July 2026).
  - Just-in-time, zero-standing-privilege access for humans and AI identities.
  - Follows the Apono acquisition, per one profile site (https://1password.com/press/2026/july/privileged-access).

### AI and agents
- **Secure Agentic Autofill with Browserbase** (October 8, 2025, early access, Enterprise Password Manager).
  - A human approves each agent login on the desktop app.
  - Credentials go over a Noise-framework end-to-end channel to the extension. They never reach the LLM or Browserbase logs.
  - Detailed audit logs are "planned" (https://1password.com/press/2025/oct/browserbase-ai-security-partnership).
- **1Password for Claude and Agentic Mode** (July 2026):
  - When an agent drives the browser, the extension locks down and hides its UI.
  - It fills only the logins and OTPs approved for the task, and Claude never sees them.
  - On by default, even without the Claude integration.
  - Initially Mac, on Individual, Family and Business plans. Needs the 1Password desktop app and extension plus the Claude desktop app and Claude in Chrome (https://1password.com/press/2026/july/1password-for-claude; https://support.claude.com/en/articles/15936181-get-started-with-1password-for-claude).
  - Some aggregators date an "Agentic Mode" announcement to October 2026. I could only confirm July 2026.

### Pricing
Raised for renewals on or after March 27, 2026 (https://tidbits.com/2026/02/25/should-1passwords-price-hike-push-you-to-apples-passwords/):
- Individual: $35.88 → **$47.88 a year**
- Families: $59.88 → **$71.88 a year**
- Monthly figures conflict across sources.

### Unique differentiators
- 2SKD and the Secret Key: a server breach alone is not enough to brute-force vaults.
- Travel Mode.
- Psst! snapshot links that work for people without 1Password.
- 22 typed item categories.
- SSH agent with commit signing, Shell Plugins, `op` CLI, Environments (no plaintext `.env`) and Credential Broker.
- Agent-safe autofill (Browserbase, Claude) where credentials are never exposed to the model.
- Device Trust (Kolide) and Unified Access/Privileged Access for enterprises.
- Universal sign-in, which picks the auth method for you.
- Masked Email in the save flow.
- Native Linux app.

### UX patterns worth copying
- Quick Access global command palette.
- Search a service by name and get its icon and URL filled in.
- An autofill "health check" that deep-links to the broken OS setting.
- Item expiry dates surfaced in Watchtower.
- "Passkey available" nudges.
- Snapshot share links with an expiry and view-once option.
- Per-request human approval for agent logins.
- Mounted, virtual secrets files.

### Weaknesses and complaints
- **Price.** The 2026 increase was up to 33%. A Hacker News thread drew about 200 comments, and post-hike reviews focus on price.
- **Autofill reliability.** Reports of failures on Edge and intranet sites, the extension losing its link to the desktop app or opening empty panels, the iOS keyboard suggestion sometimes missing, and bugs on Windows (https://www.1password.community/1password-at-home-31/i-am-extremely-disappointed-with-1password-they-ve-raised-their-prices-but-haven-t-fixed-the-bugs-24530).
- **Slow staff response to bug reports** (May 2026 forum post).
- **Electron app.** Long-running criticism since version 8 [bg]. I found no 2026 source.
- **Passkey unlock** stuck in beta for new Individual accounts only for more than 2.5 years.
- **Secret Key and Emergency Kit friction** during recovery.
- **Native macOS AutoFill** is late and still limited compared with Apple Passwords.
- **Universal sign-in regressions** on some sites.
- **Many features are enterprise-gated** (Agentic Autofill, Credential Broker).

---

## 2. Apple Passwords (iOS/iPadOS/macOS 18 → 26 → 27; visionOS; Windows)

### What it is
- A standalone app since iOS 18 and macOS 15 (September 2024), built on **iCloud Keychain**.
- Keychain data is always end-to-end encrypted, even without Advanced Data Protection (ADP) [bg].
- ADP extends end-to-end encryption to other iCloud categories [bg].
- Recovery goes through Apple Account recovery: trusted device or passcode, a recovery contact or a recovery key [bg].

### Supported content
Sidebar categories [bg]: All, **Passkeys**, **Codes**, **Wi-Fi**, **Security**, **Deleted** (about 30-day recovery). Also:
- **Shared groups:** owner and members, any iCloud contacts, not just Family.
- Notes field on each entry.
- Built-in **TOTP verification codes**, set up by QR code or a setup key, that autofill.
- **Security recommendations:** compromised (k-anonymity check), reused and weak passwords.
- **Change Password on Website**, using `/.well-known/change-password` [bg].
- Sign in with Apple and Hide My Email entries.
- Menu-bar access on Mac.
- **Password history** with "View History" in iOS 26 and macOS Tahoe 26, plus a clear option (https://9to5mac.com/2025/11/06/ios-26-passwords-app-has-convenient-new-feature/).

### Passkeys and developer APIs
- **Automatic passkey upgrades** use conditional create: a passkey is silently added after a password sign-in. Introduced at WWDC24 for iOS 18 and refined in iOS 26 [bg].
  - **Note:** this is *not* new in iOS 26, despite how some of the research brief framed it.
- WWDC25 (iOS 26) added (https://www.corbado.com/blog/wwdc25-passkeys-os26):
  - the **Account Creation API** (`ASAuthorizationAccountCreationProvider`) for passkey-first one-tap sign-up
  - the **WebAuthn Signal API** (notify the manager of username changes or revoked credentials)
  - `/.well-known/passkey-endpoints`
- Early developers reported errors with conditional create on iOS 26.0 beta. **[unverified fixed]**

### Import and export
- **Credential Exchange** in iOS 26 and macOS 26: "Export Data to Another App".
  - Transfers logins, passkeys and verification codes. **Wi-Fi items are excluded.**
  - Passkeys that use a hidden counter cannot transfer (Bitwarden notes).
  - Interoperates with 1Password, Bitwarden and Dashlane per Dashlane docs (https://support.dashlane.com/hc/en-us/articles/32907864739474).
  - Also has macOS "Export Selected Password to File…".
- CSV import exists [bg].

### Cross-platform
- **iCloud for Windows** plus the iCloud Passwords extension for Chrome, Edge and, since **July 2025**, Firefox (https://appleinsider.com/articles/25/07/09/icloud-passwords-autofill-now-available-in-firefox-for-windows).
  - Firefox reportedly works on Windows 11 only.
  - Passkey use through the Windows extensions is **[unverified]**.
- **No Android, no Linux, no web vault** [bg for web].

### iOS 27 (WWDC June 2026)
- **AI agentic password change** using Apple Intelligence and Safari.
  - Navigates the site's change flow, generates a new password and updates the item after user approval.
  - Shows a Live Activity.
  - Only on supported sites. Fails when a site needs one-time codes, extra identity checks or managed sign-in (https://9to5mac.com/2026/06/08/ios-27s-passwords-app-can-change-your-passwords-for-you-automatically/; https://www.theregister.com/personal-tech/2026/06/09/apples-ios-27-goes-all-agentic-on-compromised-passwords/5252957).
  - **Ship status [unverified]:** an August report said the beta was inconsistent and it might slip to 27.1.
- iOS 26 also lets users enable several credential providers at once [bg; one source says "up to three", **unverified**].

### Unique differentiators
- Free, preinstalled, OS-level autofill with no extension needed on Apple devices.
- Biometric unlock is tied to the Secure Enclave.
- Sharing groups with any Apple user.
- Wi-Fi password sync and QR codes.
- Agentic auto-change of weak passwords (iOS 27).
- First major platform with CXP.

### UX patterns worth copying
- A sidebar of "smart" categories (Codes, Wi-Fi, Security).
- A Deleted bin with grace-period recovery.
- One-tap Change Password through the well-known URL.
- Invisible passkey upgrades.
- Password history hidden until more than one version exists.
- Live Activity progress for background security fixes.

### Weaknesses and complaints
- **No credit-card items, secure notes, documents, attachments, custom fields, tags or custom item types.** Cards live in Safari autofill or Wallet, and only the last four digits show (https://9to5mac.com/2025/03/25/i-love-apple-passwords-but-still-use-1password-for-these-features/; https://www.androidauthority.com/apple-passwords-hands-on-review-3451645/).
- **Apple-only.** No Android or Linux, and Windows support is extension-based.
- **No separate master password.** Anyone with the device passcode gets in.
- **Clumsy import from other managers** before CXP; CXP still drops Wi-Fi items.
- **Sync fragility.** iOS 26.4 broke iCloud push sync for Passwords and was fixed in 26.4.1 (https://9to5mac.com/2026/04/08/iphone-icloud-sync-bug-apple-passwords-fixed/).
- **No granular share permissions or expiry**; no travel mode; no CLI or SSH.
- **Legacy Contact access to Keychain** is **[unverified]**.

---

## 3a. Google Password Manager (Chrome, Android, passwords.google.com)

### Features
- Built into Chrome on all desktop OSes and into Android (Google Play services), with the web view at passwords.google.com.
- Password generator, notes on entries, CSV import and export, and a Chrome desktop shortcut "app" [bg].
- **Password Checkup:** compromised, reused and weak passwords.
- **Optional on-device encryption**, keyed to the Google password or screen lock [bg, launched 2022].
- **Passkeys:**
  - Synced across Windows, macOS, Linux and Android since September 2024; ChromeOS was in beta.
  - Protected end to end by a **Google Password Manager PIN** (6 digits or alphanumeric) or the Android screen lock (https://blog.google/innovation-and-ai/technology/safety-security/google-password-manager-passkeys-update-september-2024/).
  - **iOS** (Chrome on iOS 17+, with Chrome enabled as an AutoFill source) since January 2025 (https://developer.chrome.com/blog/passkeys-gpm-ios).
- **Family sharing:**
  - Only within a Google Family group of up to 6 people.
  - Recipients get a copy, and revoking it effectively means changing the password (https://www.techradar.com/pro/security/google-password-manager-now-lets-families-share-details-across-one-account).
- **Credential Exchange:**
  - Import and export of passwords and passkeys through Play services 26.21 (June 2026).
  - An Android-wide **switch-manager flow** (September 10, 2026, Android 8+) works with 1Password, Bitwarden and Dashlane.
  - **[conflict]** on 1Password's timing (https://www.androidpolice.com/passkeys-credential-transfer-android/).
- **Automated Password Change** was announced at Google I/O 2025 for participating sites.
  - A **Gemini "Change it for me"** for weak and reused passwords is in Chrome Canary behind a flag (August 2026) and unreliable (https://androidauthority.com/chrome-canary-gemini-change-bad-passwords-3698237).
- **Android Credential Manager API** (https://codelabs.developers.google.com/codelabs/fido2-for-android):
  - One API for passkeys, passwords and Sign in with Google.
  - Supports third-party providers; Smart Lock was removed.
- **Android 16 additions:** Restore Credentials (a restore key moves during device setup), Identity Check, and a Work-Profile credential policy. These come from a third-party summary (https://www.corbado.com/blog/android-16-passkeys); verify against Google's notes.

### Unique differentiators
- Zero setup for about 3 billion Chrome and Android users.
- Passkey sync across Android, Windows, macOS, Linux and iOS.
- OS-wide switch-manager flow on Android.
- Automated password change.

### UX patterns worth copying
- A single Checkup screen grouping compromised, reused and weak.
- A PIN that unlocks end-to-end encrypted passkeys on new devices.
- Recovery through any device that has the screen lock.

### Weaknesses
- Only the Login type, with no cards (those live separately in Chrome autofill), no IDs and no attachments.
- No TOTP generator [bg; it is the separate Google Authenticator].
- On desktop it is tied to Chrome.
- Sharing is limited to the Family group, with no expiry or revocation.
- Locked to one Google account.
- On-device encryption is opt-in. Without it, Google can technically decrypt passwords [bg].
- A 2026 critique argues it is "no longer sufficient" (https://meteoraweb.com/en/news/google-password-manager-no-longer-suffices-as-a-secure-password-manager-in-2026).

## 3b. Microsoft (Authenticator and Edge)

- **Microsoft Authenticator autofill was retired, confirmed:**
  - New saves stopped in June 2025, autofill ended in July 2025, and stored passwords became inaccessible in August 2025. Data moved to Edge and the Microsoft account.
  - Users had to set Edge as the iOS/Android autofill provider or export first.
  - Passkeys and multi-factor sign-in remain in Authenticator (https://petri.com/microsoft-authenticator-password-autofill/; https://cyberinsider.com/microsoft-to-retire-password-autofill-in-authenticator-by-august-2025/).
- **Edge's built-in password manager** [bg]: generator, Password Monitor breach alerts and password health.
- **Passkey save and sync** in **Edge 142 on Windows**, rolling out from November 3, 2025, for personal Microsoft accounts (https://blogs.windows.com/msedgedev/2025/11/03/microsoft-edge-introduces-passkey-saving-and-syncing-with-microsoft-password-manager/):
  - Protected by a Password Manager PIN, with 10 attempts before lockout.
  - Logged in Azure Confidential Ledger.
  - Other operating systems are "later."
- **Windows 11 third-party passkey-provider plugin API** (November 2025). 1Password shipped first; Bitwarden was in beta. Windows Hello lets you choose where each passkey is stored.
- **Weaknesses:**
  - The forced migration to Edge pushed users to a browser.
  - Desktop features are tied to Edge.
  - Cross-platform passkey sync is still incomplete.

## 3c. Samsung Pass (in Samsung Wallet)

- Passwords, autofill and biometric sign-in, protected by **Knox** and secured with a fingerprint.
- Samsung Pass became a card inside Samsung Wallet in 2024 (https://news.samsung.com/fr/samsung-pass-dans-wallet).
- **Passkeys** are stored in Samsung Pass and synced through **Knox Matrix Credential Sync** across Galaxy devices.
- **Samsung Pass for Windows**: Galaxy Book3 series and later, Windows 10 1903+, TPM 2.0 and Windows Hello. Set up by QR code from the phone. Chrome and Edge extensions; sync through Samsung Cloud (https://www.samsung.com/in/support/apps-services/how-to-use-samsung-pass-on-your-samsung-computer).
- I found **no 2025–26 feature announcements**. One UI 8 changes are **[unverified]**.
- **Weaknesses:** Galaxy-only (plus Galaxy Book PCs), no Mac, iOS or Linux, limited item types, and minimal export or sharing.

## 3d. Firefox (built-in Passwords; Lockwise is gone)

- **Firefox Lockwise** standalone apps reached end of life on **December 13, 2021**. The features were folded into the Firefox apps (https://en.wikipedia.org/wiki/Firefox_Lockwise).
- **Current Passwords manager** [bg]:
  - `about:logins` and the Passwords sidebar
  - an optional **Primary Password**
  - end-to-end encrypted Firefox Sync
  - generator, breach alerts from HIBP and Monitor
  - CSV and other-browser import, CSV export
- Mozilla's roadmap lists "Passwords in Sidebar", mobile import from other browsers, and passkey sign-in to Firefox Sync (https://www.firefox.com/en-US/whatsnext/).
- **Passkeys:**
  - Firefox *uses* passkeys through WebAuthn and autofill (conditional UI since about Firefox 122).
  - It has **no native synced passkey store**; it relies on the OS or a third-party extension. These are third-party claims, **[unverified against Mozilla notes]**.
- **Weaknesses:**
  - Logins only, with no TOTP, cards-in-vault or attachments.
  - No passkey sync.
  - The Primary Password is off by default.
  - Weak mobile autofill integration [bg].

---

## Cross-ecosystem takeaways for a new product

1. **Portability is now table stakes.** FIDO CXP/CXF is live on iOS/macOS 26 and Android (June/September 2026). Gaps remain: Wi-Fi items, counter-based passkeys, collections, and inconsistent vendor support. A product that loses nothing in transfer, both in and out, can win.
2. **Agentic credential use is the new frontier.** Apple (iOS 27 auto-change), Google (Gemini change in Canary) and 1Password (Agentic Mode, Browserbase) all approach it. Patterns to adopt:
   - per-task human approval
   - the model never sees the secret
   - audit logs (still "planned" at 1Password)
3. **Pick the auth method for the user.** 1Password's universal sign-in, the Signal API and passkey endpoints all point toward a manager that decides how to sign in, not the user.
4. **Platform-native managers are "good enough" for logins.** They lack typed items, documents, granular sharing, cross-ecosystem reach and developer secrets. 1Password's 2026 price rise opens room in that gap.
5. **Reliability beats features.** Recurring complaints across all products are about autofill misses and sync regressions.

**Could not verify this session:**
- full live item-category docs
- current 1Password file-size and storage limits
- Privacy.com integration status
- Apple Watch app status
- the new macOS 27 Quick Access shortcut
- passkey unlock GA
- iOS 27 auto-change ship status (27.0 vs 27.1)
- passkeys through iCloud extensions on Windows
- Samsung 2025–26 changes
- Firefox native passkey storage
- Google Password Manager TOTP absence
