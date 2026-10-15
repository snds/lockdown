# Password manager and authenticator feature inventory, as of 2026-10-15

**How this was researched.** I ran about 30 WebSearch queries. Direct fetches failed with DNS errors for bitwarden.com, community.bitwarden.com, proton.me, keepassxc.org, helpnetsecurity.com and releasebot.io. GitHub was reachable, so the Vaultwarden release and wiki pages and the Bitwarden clients release page were read first-hand. Everything else comes from search-engine summaries of the URLs cited.

**Flags used in this report:**
- **[TK]** means background or training knowledge (cutoff June 2026) that I did not re-verify this session.
- **[UNVERIFIED]** means only one secondary source supports it, or sources conflict.

---

## 1. Bitwarden Password Manager

### Plans and pricing (2026)
- **Free:** unlimited items and devices, passkey storage, text-only Send, and a 2-person free organization [TK].
- **Premium:** $19.80/yr, nearly double the old $10. It was announced around 2026-01-22. It adds TOTP generation, emergency access, health reports and 5 GB of attachments (up from 1 GB), up to 10 security keys, and the phishing blocker. Renewing subscribers got a 25% loyalty discount. ([AlternativeTo](https://alternativeto.net/news/2026/1/bitwarden-enhances-premium-and-families-plans-with-more-security-features-and-raises-prices/), [Thurrott](https://www.thurrott.com/?p=332249), [PricingSaaS](https://pricingsaas.com/companies/bitwarden/diffs/2026Q1))
- **Families:** $47.88/yr for 6 users, up from $40.
- **Teams:** $4 per user per month. **Enterprise:** $6 per user per month. These come from aggregators, not the official page. ([costbench](https://www.costbench.com/software/password-management/bitwarden/))

### Item types
- Login, Card, Identity, Secure Note, and SSH Key. SSH Key arrived in 2025.1 [TK].
- Custom fields (text, hidden, boolean, linked) [TK].
- File attachments (Premium).
- Folders, favorites, and master-password re-prompt per item.
- **Archive:** Premium and paid plans can archive items so they are hidden from search and autofill without being deleted. The bulk "action bar" reached web and admin in 2026.7 and desktop in 2026.8. ([Bitwarden blog](https://bitwarden.com/blog/keep-your-vault-tidy-with-item-archiving/), [release notes](https://bitwarden.com/help/releasenotes/))
- **Fixed in Web 2026.10.0:** vault loading failures caused by certain SSH key items. The same release imports more Proton Pass item types (Wi-Fi and unknown items). ([GitHub clients releases](https://github.com/bitwarden/clients/releases))

### Send
- Text Send is free. File Send is Premium.
- Options:
  - deletion date (max 31 days) and an optional expiry date [TK]
  - optional password
  - maximum access count
  - hide the sender's email
  - disable [TK]
- Person-restricted Sends require email verification of the recipient. The email field accepts up to 2,500 characters (2026.7).
- **New in Web 2026.9.1:** share a vault item itself through a secure link (item Sends), plus expanded "Manage Send" policies. ([GitHub clients releases](https://github.com/bitwarden/clients/releases), [Sept 2026 business update](https://bitwarden.com/blog/september-2026-product-update-for-bitwarden-business-plans/))

### Emergency Access (Premium)
Trusted contacts get "View" or "Takeover" access after a waiting period you set. It is end-to-end encrypted through public-key exchange [TK].

### Organizations (Teams and Enterprise)
- Collections with per-collection permissions (can view, can edit, hide passwords, manage), groups, and an admin console [TK].
- Enterprise adds:
  - custom roles
  - 20+ policies: require 2FA, master password rules, single org, remove individual vault, remove Send, Send options, account recovery, vault timeout, remove export, require SSO, generator rules, URI match default, remove card type, PIN removal, and others [TK]
- Event logs on Teams and above. A generic HTTP Event Collector (HEC) SIEM integration arrived in 2026.8.
- Admins can edit a member's email and name for claimed accounts with no master password (2026.8). ([release notes](https://bitwarden.com/help/releasenotes/))
- Member CSV export shows 2FA and Secrets Manager status (2026.1).

### Enterprise identity
- **SSO:** SAML 2.0 and OIDC (Enterprise).
- **Trusted Device Encryption:** no master password needed. Unlock is approved by another device or an admin.
- **Key Connector:** self-hosted key service for SSO without a master password.
- **SCIM:** Entra ID, Okta, OneLogin, JumpCloud and others (Teams and Enterprise) [TK].
- **Directory Connector:** LDAP/AD, Entra, Google, Okta, OneLogin.
- **Account recovery** (admin password reset) policy.
- **Domain verification and claimed accounts** [TK].

### Reports, Access Intelligence and phishing protection
- **Individual vault health reports:** exposed passwords (HIBP), reused, weak, unsecured websites, inactive 2FA, and data breach [TK].
- **Personal health alerts and "password coaching"** were added to Premium in 2026.
- **Access Intelligence (Enterprise):** first announced in April 2025, generally available by January 2026. It gives an application-level view of at-risk credentials, guided remediation, and an "Advanced Phishing Blocker". Graphs of risk over time arrived in 2026.4 (April 21, 2026). ([BusinessWire 2025](https://www.businesswire.com/news/home/20250429232614/en/Bitwarden-Launches-Access-Intelligence-to-Defend-Against-Credential-Risks-and-Block-AI-Driven-Phishing), [Bitwarden blog](https://bitwarden.com/blog/measure-credential-risks-over-time-with-access-intelligence/), [Morningstar/BW](https://www.morningstar.com/news/business-wire/20260421368880/bitwarden-access-intelligence-introduces-reporting-for-risk-over-time-to-help-organizations-measure-and-reduce-credential-exposure))
- **Consumer phishing blocker:** Premium and Families, browser extension 2026.1.1 and later, not Safari. It uses the open-source Phishing.Database list and redirects you to a warning page. ([help](https://bitwarden.com/help/phishing-blocked/))

### Two-factor codes and passkeys
- **TOTP:** the integrated TOTP generator is Premium. Two-step login methods are authenticator app, email, Duo, YubiKey OTP, and FIDO2. FIDO2 security keys now go up to 10.
- **Bitwarden Authenticator:** a separate free, open-source app launched in May 2024 for iOS and Android. Android gained an auto-lock timer in 2026.3.
  - Sync and import status is conflicting. 2025 community threads say there is no independent sync. A 2026 third-party guide claims Credential Exchange (CXP) import/export and vault sync **[UNVERIFIED]**. ([ghacks](https://www.ghacks.net/2024/05/02/bitwarden-launches-standalone-bitwarden-authenticator-app/), [community](https://community.bitwarden.com/t/sync-between-two-or-more-instances-of-the-authenticator-app-independent-of-the-bw-password-manager-vault/68330))
- **Passkeys:**
  - Stored and synced passkeys on all clients, including the iOS and Android credential providers.
  - Native Windows 11 passkey provider, built with Microsoft.
  - "Log in with passkey" on the web vault. Browser extensions got it in November 2025 through WebAuthn PRF.
  - **Unlock with a PRF passkey** on web and Chromium extensions (2026.1.1). PRF supplies key material, so this is true decryption, not just authentication. Windows 10 has PRF issues. ([BusinessWire Jan 2026](https://www.businesswire.com/news/home/20260114341558/en/Bitwarden-Strengthens-Identity-Security-Posture-for-2026-With-New-Enterprise-Capabilities-Passkey-Innovation-and-Secure-AI-Assisted-Workflows), [community](https://community.bitwarden.com/t/you-can-now-unlock-your-vault-with-a-passkey/93556))
- **Credential Exchange (CXP):** Bitwarden was the first third-party manager on iOS 26 to support it (September 2025). It is also on Android 14+ with Play Services 26.21+. Passkeys that use a hidden signature counter cannot be transferred. ([Bitwarden blog](https://bitwarden.com/blog/security-vendors-join-forces-to-make-passkeys-more-portable-for-everyone/))

### SSH, CLI, AI and secrets
- **SSH agent (desktop):** graduated from experimental in 2026.3. SSH key items autofill through the extension in 2026.9. "SSH Agent v1" was removed in desktop 2026.9.1. ([tech-insider (low reliability)](https://tech-insider.org/ca/bitwarden-ssh-agent-setup-2026/), [GitHub desktop-v2026.9.1](https://github.com/bitwarden/clients/releases/tag/desktop-v2026.9.1))
- **CLI:** `bw` covers full vault CRUD, Send, and JSON output. `bw serve` exposes a local Vault Management REST API [TK]. CLI 2026.10 adds desktop biometric unlock behind a flag. ([GitHub](https://github.com/bitwarden/clients/releases))
- **MCP server:** local-first, for AI assistants (expanded in 2025).
- **Secrets Manager:** a separate product with projects, machine accounts and access tokens. It has SDKs, GitHub Actions and a Kubernetes operator, and a free tier [TK].

### Autofill
- **Inline autofill menu** (2024 redesign) [TK].
- **"Fill assist"** for a curated list of sites known to break autofill (2026.7).
- **Desktop Autotype:** a Windows MVP behind a flag in 2026.9. The 2026.10 PR list says "Autotype GA" **[UNVERIFIED]** as to platform scope.
- Linux biometrics no longer needs a secret-service daemon (2026.7).

### Account security
New-device login verification began in February 2025. It emails a one-time code when the account has no 2FA. Accounts with 2FA, SSO, passkey or API-key logins are exempt, as are self-hosted servers. The lockout risk is real if your email password lives in the vault. ([help](https://bitwarden.com/help/new-device-verification/), [PCWorld](https://www.pcworld.com/article/2591729/stop-saving-your-email-login-in-your-password-manager.html))

### Username generator
- **Forwarded email aliases** through six services: SimpleLogin, addy.io, Firefox Relay, Fastmail, DuckDuckGo, and Forward Email. Each uses an API key. ([Bitwarden blog](https://bitwarden.com/blog/how-to-use-the-bitwarden-forwarded-email-alias-generator/))
- Also plus-addressing, catch-all, and random words.

### Import
- 50+ formats **[TK; exact count not found]**.
- 2026.10 improved LastPass character handling, grouped the 1Password import sources, and added more Proton Pass item types.

### Self-hosting
- **Standard deployment:** multi-container Docker.
- **Bitwarden Lite:** formerly "unified". It is a single container that supports SQLite, Postgres, MySQL/MariaDB and MSSQL [TK]. It left beta around December 2025 **[date UNVERIFIED]**. It is aimed at individuals and homelabs. ([Bitwarden blog](https://bitwarden.com/blog/lightweight-and-flexible-bitwarden-lite-self-host-deployment/))
- Paid features still need a license file when self-hosting.

### Security record
- **2025 audits:**
  - Cure53: browser extension and autofill overlay, core app, desktop, the RustCrypto crate, and the web vault
  - Fracture Labs: web app and network
  - Unit 42: mobile and Authenticator. One Android flaw could have exposed TOTP secrets, and it was fixed.
  - SOC 2 Type 2 and SOC 3 ([help](https://bitwarden.com/help/is-bitwarden-audited), [CyberInsider](https://cyberinsider.com/?p=302204))
- **ETH Zurich / USI "malicious server" paper (February 2026):** about 12 attacks on Bitwarden, out of 25 to 27 in total across Bitwarden, LastPass and Dashlane. Examples are field-swap via icon loading and legacy encryption-mode downgrade. Vendors reportedly fixed 7 of the 12 and called 3 "design decisions" **[UNVERIFIED]**. 1Password was judged more resilient because of its Secret Key. ([secure.com summary](https://www.secure.com/blog/researchers-expose-major-security-gaps-in-leading-password-managers), [heise](https://heise.de/-11179524), [1Password response](https://1password.com/blog/eth-zurich-zero-knowledge-malicious-server-review))

### Encryption model
- The master key comes from PBKDF2-SHA256 (600k iterations by default, minimum raised to 600k in 2026.2.1, plus 100k server-side) or Argon2id (64 MiB [TK; one summary said "32"], 3 iterations, parallelism 4 [TK]). ([KDF help](https://bitwarden.com/help/kdf-algorithms/))
- The key is stretched with HKDF and then decrypts a 512-bit symmetric user key (AES-256-CBC with HMAC-SHA256) [TK].
- An RSA key pair handles organization sharing [TK].
- Encryption is end-to-end and zero-knowledge, with the caveats above.

### Open source and the Rust SDK
- Clients are GPL-3.0. The server is AGPL-3.0, with enterprise features under the "Bitwarden License" [TK].
- **The internal Rust SDK (`sdk-internal`)** compiles to WebAssembly (WASM) for web and extensions and runs natively on mobile. It now holds the shared crypto and business logic.
- **A 2024 license controversy** was resolved by splitting out a GPL-licensed `sdk-internal`. Secrets Manager's SDK keeps its commercial license. ([contributing docs](https://contributing.bitwarden.com/getting-started/sdk/internal/), [oschina](https://www.oschina.net/comment/news/317985))

### Other 2025–2026 changes
- **Corporate:**
  - CEO Michael Crandell stepped back in February 2026 and was replaced by Michael Sullivan. The CFO also changed.
  - "Always free" wording briefly disappeared from the website in May 2026. Bitwarden called that an oversight.
  - Sources: [It's FOSS](https://itsfoss.com/news/bitwarden-quiet-changes.md), [Linuxiac](https://linuxiac.com/bitwarden-faces-questions-after-quiet-leadership-and-messaging-changes/), [WebProNews](https://www.webpronews.com/bitwardens-quiet-shift-free-tier-rhetoric-fades-as-veteran-leaders-exit)
- **UI redesign:** a new design is in beta (September 2026 blog), with general availability planned in the coming months.

### Differentiators
- Generous free tier.
- Fully open-source clients, with an official option to self-host.
- Deepest enterprise identity stack among open-source managers: SSO, Trusted Device Encryption, Key Connector, SCIM.
- First mover on CXP and PRF unlock.
- Six alias integrations.
- Integrated SSH agent and Secrets Manager.

### Top complaints
- The 2024.12 extension redesign made the Fill button smaller and added clicks, which produced a 48+ page forum thread. ([forum](https://community.bitwarden.com/t/usability-issues-ux-in-redesigned-ui-2024-12-0/76836?page=48))
- Premium nearly doubled in price, and some users called the new bundle "enshittification".
- Concern about direction under the new CEO and the free-tier wording.
- Autofill misses on card fields.
- The new-device email OTP can lock people out.
- The Authenticator app lacks robust sync.
- The ETH malicious-server findings.

---

## 2. Vaultwarden

### What it is
- An unofficial server written in Rust (formerly "bitwarden_rs") that implements the Bitwarden client API.
- AGPL-3.0, using the Rocket web framework, with SQLite, MySQL or Postgres [TK].
- One container using about 50 MB of RAM [TK].
- It ships a patched official web vault. It is not affiliated with Bitwarden. ([wiki](https://github.com/dani-garcia/vaultwarden/wiki))

### Supported features (official wiki, read first-hand)
- **Vaults and sharing:** personal and organization vaults, collections, folders, favorites, and password sharing with access control.
  - Groups require an environment variable to enable.
  - Event logs must be enabled separately.
- **Items and tools:**
  - attachments (Premium-gated on Bitwarden, free here)
  - website icons
  - the built-in TOTP generator (Premium-gated on Bitwarden, free here)
  - Send
  - emergency access (Premium-gated on Bitwarden, free here)
  - trash with configurable auto-purge
  - master password re-prompt
  - personal API keys
- **Two-step login:** email, Duo, YubiKey OTP, and FIDO2/WebAuthn.
- **Username generator:** SimpleLogin, addy.io and Firefox Relay are listed.
- **Directory Connector** (partial public API).
- **Account recovery** (needs SMTP).
- **Live sync:** over WebSocket for desktop and browser. Push for iOS and Android goes through Bitwarden's push relay, which needs an installation ID and key from bitwarden.com/host [TK].
- **SSO through OpenID Connect (OIDC):**
  - Merged in **1.35.0 (around late December 2025)**.
  - The master password is still required. SSO does not replace it, so there is no Trusted Device Encryption or Key Connector.
  - Provider discovery uses `SSO_*` environment variables. `SSO_SIGNUPS_ALLOWED` was added in 1.37.3.
  - The earlier Timshel/OIDCWarden fork was the origin. ([GitHub OIDCWarden](https://github.com/Timshel/OIDCWarden))
- **Policies:**
  - require 2FA, master password, account recovery admin, generator, single org, remove individual vault, remove Send, Send options, remove card type, default URI match, remove PIN unlock
- **Item archive:** supported from 1.36.0.

### Not supported (wiki)
- **Could be added if someone contributes it:**
  - the full Public API and organization API keys
  - **login with passkey** (PR #5929)
  - **new-device login protection**
- **Unlikely to be added:**
  - custom roles
  - policies for require SSO, vault timeout, and remove export (their UI is not open source)
- **Not mentioned on the wiki but absent [TK]:**
  - Secrets Manager
  - Access Intelligence and risk insights
  - Trusted Device Encryption and Key Connector
  - SCIM

### Admin panel
- At `/admin`, protected by `ADMIN_TOKEN`, which is recommended to be an Argon2 PHC hash [TK].
- It covers user and org management, invites, SMTP, signups and domain whitelist, and the feature toggles.

### Release cadence (GitHub, first-hand)
- **1.36.0 (2026-05-03):**
  - Fixed: SSO login CSRF, enumeration, SSO account binding, and SSRF through the icon endpoint.
  - Added: item archive, with web vault 2026.4.1.
- **1.37.0 (2026-07-24):**
  - Fixed nine advisories, including cross-organization cipher access, a Send access-count bypass, and WebSocket flooding.
  - Added: trusted-proxy support and S3 support through OpenDAL.
  - Required for clients 2026.7 and later.
- **1.37.2:** required for clients 2026.8 and later.
- **1.37.3:** revokes 2FA "remember" tokens and adds rate limits on prelogin.
- **1.37.4 (2026-10-05):** seven advisories, one rated High (organization member revocation). Older CLI versions break `bw send receive`. ([releases](https://github.com/dani-garcia/vaultwarden/releases))
- **CVEs in 2026** include:
  - CVE-2026-43914: rate-limit bypass through the email-2FA path
  - CVE-2026-27802: Manager privilege escalation
  - CVE-2026-31835: WebAuthn flag tampering
  - CVE-2026-43911: refresh tokens not revoked, CVSS 8.1
  - Sources: [NVD](https://nvd.nist.gov/vuln/detail/cve-2026-31835), [MITRE](https://cveawg.mitre.org/api/cve/CVE-2026-43914)
- WZ-IT counted 13 advisories in the three months before 1.36. ([WZ-IT](https://wz-it.com/en/blog/vaultwarden-1-36-0-nis2-self-hosted-password-manager-enterprise/))

### Deployment ergonomics
- Single Docker image. WebSocket now shares the main port [TK].
- HTTPS is required because browsers block WebCrypto on plain HTTP (localhost excepted).
- Reverse-proxy friendly, with trusted-proxy configuration since 1.37.
- Backups mean the database file plus the attachments folder.

### Risks
- **Not audited** by a third party, unlike Bitwarden's audit set.
- **Rising advisory volume in 2026.**
- **Lockstep dependence on official clients:** new client versions force server upgrades.
- **Push relies on Bitwarden's relay.**
- **Small maintainer team** (dani-garcia, BlackDex and others).
- **Possible license or policy risk** if Bitwarden changes its client API [TK speculation].

### Differentiators
- All Premium and organization features for free, including SSO through OIDC, which Bitwarden sells only on Enterprise.
- Tiny footprint.

---

## 3. Proton Pass

### Plans
- **Free:**
  - unlimited logins, notes and cards on unlimited devices
  - 10 hide-my-email aliases
  - passkeys
  - weak and reused password alerts
  - import
  - 2 vaults (per TechRadar and CostBench) **[some conflict]**
  - The 2FA code generator works for only the first 3 TOTP items after a downgrade.
  - ([Proton plans](https://proton.me/support/proton-pass-plans-explained), [pricing](https://proton.me/pass/pricing))
- **Pass Plus** ($2.49–2.99/mo annual; sources conflict):
  - unlimited aliases and vaults
  - integrated TOTP
  - vault sharing (up to 10)
  - Dark Web Monitoring
  - Proton Sentinel
  - Secure Links
  - emergency access
  - CLI
  - attachments and custom items
- **Pass Family:** 6 users.
- **Business tiers:** Pass Essentials, Pass Professional, and Business suites. Admins get policies for alias and Secure Link use, and folders are excluded on Essentials [TK partial].
- **Lifetime Pass + SimpleLogin plan.** ([support](https://187.124.44.115.sslip.io/support/pass-lifetime))
- **"Pass Lite":** NOT FOUND in any source. Treat it as nonexistent or unconfirmed.

### Item types
- Login with integrated TOTP, alias, note, card, identity (2024) [TK].
- **Custom items** (2025, paid), with templates: API credential, database, server, software license, **SSH key**, **Wi-Fi network**, passport and other identity and financial types, or start from scratch. ([support](https://proton.me/support/create-custom-item), [Techlore](https://discuss.techlore.tech/t/proton-pass-adds-custom-item-support-ssh-keys-and-more/14302))
- **File attachments** (spring 2025, paid).

### Organization
- **Folders and subfolders**, plus improved URL matching. Rollout began in 2026. Folders inherit the vault's sharing settings, and folder sharing is still on the roadmap. ([blog](https://proton.me/blog/introducing-pass-folders-url-matching))

### Sharing
- Vault sharing and item-level sharing with roles [TK].
- **Secure Links** (September 2024, paid):
  - shares one item with anyone, no account needed
  - default expiry 7 days, maximum 30
  - optional view limit, revocable
  - the decryption key lives in the URL fragment [TK]
  - ([blog](https://proton.me/blog/pass-secure-link-sharing), [It's FOSS](https://itsfoss.com/news/proton-pass-secure-links/))

### Aliases
Native hide-my-email through SimpleLogin (Proton-owned). You can create aliases inline during signup autofill, use custom domains and mailboxes on SimpleLogin, and reply from an alias [TK].

### Monitoring
- **Pass Monitor (2024):** weak, reused and missing-2FA checks, plus dark web monitoring of Proton addresses, aliases and custom emails [TK]. The name "Pass Monitor" was not re-verified this session.
- **Proton Sentinel:** human-reviewed protection for account takeover, included in Plus. ([Proton blog](https://proton.me/blog/proton-updates/proton-pass))

### Passkeys and two-factor
- Passkeys are stored and synced on all platforms, and passkey creation works in Safari through the extension (2025).
- An integrated TOTP generator autofills codes.
- **Proton Authenticator:** a separate app launched around 2025-07-31. It is free, open source, and runs on iOS, Android, Windows, macOS and Linux. It offers optional end-to-end encrypted sync and import from other authenticators. F-Droid version 1.3.4 shipped November 2025. ([Michael Tsai](https://mjtsai.com/blog/2025/08/01/proton-pass-and-proton-authenticator/), [AlternativeTo](https://alternativeto.net/software/proton-pass/news))

### Emergency access
Added August 2025. A trusted contact gets access after a set period of inactivity.

### CLI and SSH
- **CLI (`pass-cli`):** beta in November 2025, launched 2025-12-11.
- **Personal Access Tokens:** scoped vault access for scripts and AI agents (fall/winter 2025). ([roadmap](https://proton.me/blog/pass-roadmap-spring-summer-2026), [dmcc.io tutorial](https://blog.dmcc.io/journal/proton-pass-cli-linux-secrets/))
- **SSH agent:** announced and **not confirmed shipped** as of the latest sources (April 2026 coverage). Its design uses biometric prompts so keys stay off disk. ([Neowin](https://www.neowin.net/news/proton-pass-is-getting-folders-an-ssh-agent-and-other-features-later-this-year/), [Proton on X](https://x.com/ProtonPrivacy/status/2047686255769506278))

### Apps and autofill
- Apps for Windows, macOS, **Linux** (2024), iOS, Android (including F-Droid), and all major browsers.
- 2025 autofill work:
  - credit card autofill
  - auto-type
  - HTTP Basic Auth
  - a right-click menu
  - an offline mode
  - a configurable clipboard-clear time
- A full offline mode for the extension is still on the roadmap. Smarter autofill (iframes, URL matching) is on the 2026 roadmap.
- Imports from iCloud Keychain, Chrome, Bitwarden, 1Password, Dashlane, LastPass, NordPass, Keeper and others (1.37.0, January 2026). ([APKMirror](https://www.apkmirror.com/?p=12268322))

### Encryption, licensing and audits
- **Encryption [TK]:**
  - per-item AES-256-GCM keys, wrapped by vault keys
  - vault keys encrypted to the user's PGP key
  - the key password is derived with bcrypt
  - SRP login
  - Proton Sentinel and 2FA (TOTP or FIDO2) on the account
- **Licensing:** clients are GPLv3 [TK].
- **Audits:** Cure53 audited the mobile apps, extensions and API (2023 [TK]). Findings were mostly low severity, and one medium Android issue could not be fixed at the platform level. ([blog](https://proton.me/blog/pass-open-source-security-audit))

### Differentiators
- First-party alias system, with 10 aliases even on the free plan.
- Proton ecosystem bundle, including Sentinel.
- Secure Links that work for non-users.
- Custom item templates, including Wi-Fi and SSH.
- Swiss jurisdiction.
- An authenticator app with encrypted cross-platform sync.

### Top complaints
- Autofill reliability: "doesn't work 80% of the time", missing overlays, and no NTLM support.
- No autofill hotkey.
- Only the top-level domain is saved per URL. The 2026 URL matching is meant to fix this.
- Folders arrived late, and the SSH agent has still not shipped.
- PIN prompt focus bugs.
- UI and UX weaker than the security story.
- ([Firefox AMO reviews](https://update.mozilla.org/en-US/firefox/addon/proton-pass/reviews/?score=4))

---

## 4. Other products

### Dashlane
- **Free plan dropped around September 2025.** One review says it ended in 2026 **[date conflict]**. What remains is Premium, Friends & Family and Business, with a 14-day trial. ([netzwelt](https://www.netzwelt.de/passwort-manager/dashlane-testbericht.html), [security.org](https://www.security.org/password-manager/dashlane/))
- **Business identity:** Confidential SSO, which runs SSO inside a secure enclave, plus self-hosted SSO and SCIM provisioning. ([Cybernews](https://password-managers.cybernews.com/dashlane-review/))
- **Passwordless:** passkey-based Dashlane accounts with no master password (read-only when offline).
- **Phishing:** real-time phishing alerts, and AI phishing detection in the "Credential Protection" business tier.
- **Monitoring:** dark web monitoring.
- **VPN:** **not dropped.** It is still bundled with Premium and F&F through Hotspot Shield, and excluded from trials [verify].
- **Weaknesses:**
  - price ($4.99/mo)
  - no free tier
  - the ETH paper found 6 attacks
  - no Linux desktop app [TK]

### Keeper
- **Records and sharing:** zero-knowledge records with custom fields and files, and **One-Time Share**, which became bidirectional in 2025.
- **BreachWatch:** dark web monitoring that runs on Keeper's own HSM-backed infrastructure.
- **KeeperPAM:** privileged access management launched in the cloud 2025-02-05. It covers secrets, connections, session recording and Zero Trust, delivered through a Gateway.
- **Keeper Forcefield:** launched 2025-11-06. Kernel-level protection against memory scraping on Windows 11, for browsers and Keeper apps. A release-note entry dated May 2025 conflicts with that launch date. ([SiliconANGLE](https://siliconangle.com/2025/11/06/keeper-security-debuts-forcefield-block-memory-based-attacks-windows/), [Keeper docs](https://docs.keeper.io/release-notes))
- **Other:** Commander CLI and FedRAMP/StateRAMP [TK].
- **Weaknesses:** heavy upselling of add-ons (BreachWatch, storage, chat), closed source, and a cluttered UI [TK].

### KeePassXC
- A local KDBX 4 file (AES-256 or ChaCha20, Argon2id).
- **KeeShare** for syncing shared groups.
- **KeePassXC-Browser** through native messaging.
- **YubiKey/OnlyKey HMAC-SHA1 challenge-response.**
- An SSH agent integration, TOTP, auto-type, and the Secret Service API on Linux [TK].
- **Passkeys:** stored in the database, made editable in 2.7.8.
  - 2.7.10 (2025-03-04) added group selection and the Proton Pass importer.
  - 2.7.12 set the backup flags (BE/BS) to true, which **may break existing passkeys**.
  - ([KeePassXC blog](https://keepassxc.org/blog/2025-03-04-2.7.10-released/), [AlternativeTo](https://alternativeto.net/news/2025/3/keepassxc-2-7-10-brings-proton-pass-importer-font-size-setting-and-other-improvements/))
- **Version 2.8 not found.**
- **AI contribution controversy:** in November 2025 the project disclosed AI-assisted pull requests. It now promises no AI features in the app and that nothing merges without tests and review. ([blog](https://keepassxc.org/blog/2025-11-09-about-keepassxcs-code-quality-control/))
- **Weaknesses:** no built-in sync (you bring your own file sync), no official mobile app (KeePassDX or KeePassium fill that role), and dated UX.

### NordPass
- Email masking, Password Health, data breach scanner, passkeys, a built-in authenticator (paid), and XChaCha20 encryption [TK].
- Business tiers: Teams, Business, and Enterprise, with SSO, shared folders and a Vanta integration. ([Cybernews](https://password-managers.cybernews.com/nordpass-review/))
- **Weaknesses:** closed source, a thin free tier (one active device session [TK]), and upsell-heavy.

### Enpass
- Offline-first, with bring-your-own-cloud sync through iCloud, Google Drive, OneDrive, Dropbox, Box, WebDAV, or local Wi-Fi [providers TK].
- Passkey storage and sync (6.11.x, July 2026). ([APKMirror](https://www.apkmirror.com/apk/enpass-technologies-inc/enpass-password-manager/enpass-password-manager-6-11-25-1316-release/))
- A lifetime license option.
- **Weaknesses:** closed source, sync conflicts, and dated UI.

### Strongbox and KeePassium (iOS and macOS KeePass clients)
- **Strongbox was sold to Applause around March 2025.** Privacy Guides removed it from its recommendations. Critics say it is not truly open source. ([heise](https://heise.de/-10316743), [Privacy Guides](https://discuss.privacyguides.net/t/is-strongbox-still-recommended-by-privacyguides/26914))
- **KeePassium:** offline-first KDBX, a lifetime purchase option, AutoFill and a Cure53-audited iOS app (user-reported).

### LastPass (cautionary tale)
- **The 2022 breach:** encrypted vault backups were stolen, along with plaintext URLs. Vaults with low iteration counts were cracked offline.
  - More than $35M in crypto was stolen from more than 150 victims. Estimates run as high as about $100M.
  - In March 2025 US authorities seized $23–24M linked to a $150M heist.
  - A $24.5M class-action settlement in 2025.
  - The UK ICO fined LastPass in November 2025, with £1.2M reported **[UNVERIFIED amount]**.
  - ([Wikipedia](https://en.wikipedia.org/wiki/2022_LastPass_data_breach), [Krebs](https://krebsonsecurity.com/2025/03/feds-link-150m-cyberheist-to-2022-lastpass-hacks/), [SecurityAffairs](https://securityaffairs.com/175156/cyber-crime/feds-seized-23-million-in-crypto-stolen-using-keys-from-lastpass-breaches.html))
- **Lessons:**
  - Encrypt all metadata.
  - Enforce KDF upgrades on old accounts.
  - Never store seed phrases with low KDF settings.

### Authenticators
- **Ente Auth:** open source, end-to-end encrypted cloud sync, with desktop, mobile and web apps. Free.
- **Aegis:** Android only, local encrypted vault, manual or automatic backups, no cloud sync.
- **2FAS:** iOS and Android, Google Drive or iCloud backup, browser extension, Apple Watch. A "2FAS Pass" was **not found**. ([Ente comparison (vendor)](https://ente.io/compare/ente-auth-vs-others))
- **Authy:** desktop apps ended in August 2024 and users were force-logged-out. There is **no export**, so migration means re-enrolling every site, and the app was pulled from the Mac App Store. ([BleepingComputer](https://www.bleepingcomputer.com/news/security/twilio-kills-off-authy-for-desktop-forcibly-logs-out-all-users/), [ITPro](https://itpro.com/software/the-authy-desktop-app-has-officially-reached-end-of-life-heres-what-you-need-to-know))

---

## 5. Features no single competitor fully has

1. **Native aliases and an enterprise identity stack in one product.** Proton has first-party aliases. Bitwarden has Trusted Device Encryption, Key Connector and SCIM but only forwards to third-party alias services.
2. **Free, audited, open-source self-hosting with all features.** Vaultwarden gives every feature free but is unaudited and lacks new-device protection, passkey login and custom roles. Bitwarden Lite is audited but needs a license for paid features.
3. **Passkey PRF vault unlock everywhere.** Bitwarden has it on web and Chromium only. Nobody has it on mobile, Firefox and Safari together.
4. **Local-first file with real-time E2EE cloud sync and org sharing.** KeePassXC's KeeShare is clunky. Enpass syncs through your own cloud but has no organization model.
5. **Memory-scraping defense on the endpoint.** Only Keeper Forcefield, and only on Windows.
6. **A shippable SSH agent plus a secrets CLI with scoped tokens.** Bitwarden has the SSH agent, Secrets Manager machine accounts and `bw serve`. Proton has `pass-cli` with PATs but no agent yet. KeePassXC has an agent but no secrets CLI.
7. **Share-to-anyone links with expiry and view limits for both items and files.** Bitwarden's Send covers files and text and, since 2026.9, items. Proton Secure Links cover items only. Keeper One-Time Share is now bidirectional. None covers every case.
8. **Authenticator app with E2EE cross-device sync, an open-source audit, and two-way sync with the password manager.** Ente and Proton Authenticator sync. Bitwarden Authenticator's sync is unclear.
9. **Resistance to a malicious server**, like 1Password's Secret Key model. It is absent from Bitwarden, Dashlane and LastPass per the ETH study.
10. **Credential Exchange (CXP) import and export on every platform**, desktop included. Today it exists only on iOS 26 and Android 14+.
11. **Desktop auto-type on all OSes in a cloud manager.** KeePassXC has it. Bitwarden's Windows auto-type is only just reaching general availability, and Proton has auto-type only in the browser.
12. **Org-level risk-over-time analytics** combined with consumer-grade phishing blocking. Bitwarden is closest, but its blocker is not in Safari.

## 6. Best-in-class pick per feature area

| Area | Pick | Why |
|---|---|---|
| Free tier | Bitwarden (Proton close) | Unlimited items and devices plus Send. Proton adds 10 aliases. |
| Email aliases | Proton Pass | First-party SimpleLogin, inline creation. |
| Alias integrations (bring your own) | Bitwarden | Six forwarders. |
| Enterprise identity (SSO without a master password, SCIM) | Bitwarden Enterprise | TDE, Key Connector, SCIM. Dashlane's Confidential SSO is a close second. |
| Self-hosting | Vaultwarden for homelabs; Bitwarden (standard or Lite) for audited deployments | Features vs. assurance. |
| Passkeys | Bitwarden | Storage, login, PRF unlock, CXP, Windows 11 provider. |
| Credential portability | Bitwarden | First CXP on iOS and Android, 50+ importers [TK]. |
| Secure sharing to outsiders | Bitwarden Send (files, text, items) / Proton Secure Links | Expiry plus access limits. |
| Breach and dark web monitoring | Keeper BreachWatch / Proton (Monitor + Sentinel) | — |
| Org risk analytics | Bitwarden Access Intelligence | App-level risk, trends over time. |
| Phishing defense | Dashlane (business AI detection) / Bitwarden (consumer blocker) | — |
| Endpoint and memory hardening | Keeper Forcefield | Unique. |
| PAM and secrets | KeeperPAM (PAM), Bitwarden Secrets Manager (developer secrets) | — |
| Developer CLI and SSH | Bitwarden (SSH agent GA, `bw serve`) | Proton's agent is pending. |
| Offline-first and local control | KeePassXC (+ KeePassium on iOS) | No vendor server. |
| Hardware-key vault encryption | KeePassXC (YubiKey challenge-response) | — |
| Bring-your-own-cloud sync | Enpass | — |
| Standalone TOTP | Ente Auth (sync) / Aegis (Android, local) | Avoid Authy, which has no export. |
| Custom item types | Proton Pass (templates: Wi-Fi, SSH, API, database) | — |
| Transparency and audits | Bitwarden (yearly multi-firm audits, SOC 2) | — |
| Cautionary design anti-patterns | LastPass | Unencrypted URLs, legacy KDF settings. |

**Main gaps in this research:**
- The official Bitwarden and Proton pages could not be fetched directly.
- Not confirmed or not found:
  - the import-format count
  - Proton's "Pass Lite" plan and 2FAS Pass
  - KeePassXC 2.8
  - Bitwarden Authenticator sync status
  - the Proton Pass SSH agent release
  - the Dashlane free-plan end date
  - the LastPass ICO fine amount