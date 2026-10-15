# Innovation sweep: rare, emerging and missing password-manager features (as of 2026-10-15)

**Bottom line.** Start with three features: breach-response playbooks, an on-device learned autofill classifier, and a real digital-legacy system. All three are high value. No consumer product does any of them well. None needs risky platform access. The big strategic bet is agent-safe credential brokering. 1Password leads it today, but its approach is closed and its audit trail is still "planned". Do not ship hidden vaults (plausible deniability). Ship a visible panic lock and duress wipe instead.

**How this was researched.**
- **Read first:** the four prior reports in this folder (01 to 04). Facts taken from them carry their original source URLs.
- **WebSearch was unavailable.** The turn-wide budget of 200 searches was used up before my first query, and all six attempts were refused. I did not work around that limit.
- **WebFetch:** DNS failed for grapheneos.org, developers.google.com and openssh.com. GitHub worked, so I read these first-hand: the SLIP-39 spec, GrapheneOS `features.html`, OpenSSH `PROTOCOL.u2f`, the age and age-plugin-se READMEs, Secretive, the Bitwarden MCP server, the `bitwarden/ios` repo tree, the JustDeleteMe `sites.json` data, the passkeys directory, the ZXing Wi-Fi QR format, Mozilla Fathom, and the MCP 2025-11-25 changelog and elicitation spec.
- **Flags:** **[bg]** is background knowledge from before June 2026 that I did not re-verify. **[unverified]** means the evidence conflicts or is missing. Verify both before any spec commits to them. A follow-up run with a fresh search budget should check the items listed in the last section.

---

## Scoring

- **Value (1–5):** user impact multiplied by breadth.
- **Effort:**
  - S: 1 engineer-month or less
  - M: 1 to 3 months
  - L: 3 to 9 months
  - XL: more than 9 months
  - All estimates assume the vault core, extension and mobile apps already exist.
- **Risk (Low/Med/High):** the risk that remains after the recommended design. It covers security, legal, lockout and abuse.
- **Diff (differentiation):**
  - 2: no one does it well
  - 1: one or two vendors do part of it
  - 0: table stakes
- **Score** = 2×Value + Effort points + Risk points + Diff.
  - Effort points: S = 3, M = 2, L = 1, XL = 0.
  - Risk points: Low = 2, Med = 1, High = 0.
  - The maximum is 17.

## Ranked list

| # | Feature | Value | Effort | Risk | Diff | Score | Best today | Verdict |
|---|---|---|---|---|---|---|---|---|
| 1 | Breach-response playbooks (guided incident response) | 5 | M | Low | 2 | **16** | Bitwarden Access Intelligence (Enterprise only) | **v1 core** |
| 2 | On-device ML autofill field classification | 5 | L | Low | 1 | **14** | Browsers (Chrome crowd predictions [bg]); no PM confirmed | **v1.x core** |
| 3 | Digital legacy / dead-man switch (scoped, k-of-n) | 5 | M | Med | 1 | **14** | Bitwarden Emergency Access; Proton (inactivity) | **v1 core** |
| 4 | Federated-login map ("Sign in with Google/Apple/MS/Steam") | 4 | S | Low | 1 | **14** | 1Password (sign-in-with [bg] + Universal sign-in) | **v1 quick win** |
| 5 | Agent-safe credential brokering (MCP) | 5 | L | Med | 1 | **13** | 1Password (Agentic Mode, Browserbase, Credential Broker) | **v1.x strategic bet** |
| 6 | Passwordless migration coaching | 4 | M | Low | 1 | **13** | 1Password Watchtower; Apple auto-upgrade | **v1** |
| 7 | Per-item version history and tamper-evident audit trail | 4 | M | Low | 1 | **13** | Keeper / 1Password item history [bg]; Apple password history | **v1** (shares foundation work) |
| 8 | Family and kids accounts (guardian, teen, graduation) | 4 | M | Med | 2 | **13** | Apple Family Sharing [bg] / 1Password Families | **v1.x** |
| 9 | Crypto seed-phrase safe mode (SLIP-39 aware) | 4 | M | Med | 2 | **13** | None among PMs; hardware wallets (Trezor [bg]) | **v1** |
| 10 | Account deletion / "forget me" helper | 3 | S | Low | 2 | **13** | None among PMs; JustDeleteMe, Permission Slip [bg] | **v1 quick win** |
| 11 | Email and SMS one-time-code autofill | 4 | L | Med | 2 | **12** | Apple (platform-only) | **v1.x on Android; email only via your own alias service** |
| 12 | Document scan + OCR into typed items (MRZ/NFC) | 4 | M | Med | 1 | **12** | 1Password typed items; Apple/Google Wallet IDs [bg] | **v1.x** |
| 13 | Wi-Fi QR sharing | 3 | S | Low | 1 | **12** | Apple Passwords | **v1 quick win** |
| 14 | Browser-less TV/console sign-in helper | 3 | S | Med | 2 | **12** | Platforms (Apple TV keyboard autofill [bg]); no PM | **v1.x** |
| 15 | OAuth connected-app inventory | 3 | M | Low | 2 | **12** | Enterprise only (1Password SaaS Manager/XAM) | **v1.x (deep-link review)** |
| 16 | Natural-language vault search (on-device) | 3 | M | Low | 2 | **12** | None confirmed [bg] | **v1.x** |
| 17 | Agentic password change (on-device AI) | 4 | XL | Med | 1 | **10** | Apple iOS 27; Google Gemini (Canary) | **v2; start with recipes** |
| 18 | Time- and location-aware items | 3 | M | Med | 1 | **10** | 1Password (expiry + "nearby") | **v1.x (expiry); v2 (context)** |
| 19 | Panic lock / duress wipe / travel-safe | 3 | M | Med | 1 | **10** | 1Password Travel Mode; GrapheneOS duress (OS) | **v1.x** |
| 20 | Apple Watch / Wear OS approvals and proximity | 3 | L | Med | 1 | **9** | Keeper (Watch + Wear OS) [bg]; Bitwarden watch targets | **v2** |
| 21 | age/GPG key storage and agents | 3 | M | Med | 2 | **9** (V counted as 2 for niche) | None among PMs | **v2 (dev tier)** |
| 22 | Email magic-link capture | 3 | L | High | 2 | **9** | None | **v2, only with your own alias inbox** |
| 23 | Passkeys for SSH (`sk-ecdsa`/`sk-ed25519`) | 2 | L | Med | 2 | **8** | Hardware keys; Secretive (Secure Enclave) | **Reframe: SSH certificates** |
| 24 | Hidden vaults / plausible deniability | 2 | XL | High | 2 | **6** | None (VeraCrypt hidden volumes [bg]) | **Do not ship** |

**Bundles that share infrastructure.** Build them together.
- **Account graph:** features 1, 4, 6, 10 and 15. They all need one per-site knowledge base:
  - JustDeleteMe `sites.json` (MIT)
  - the passkeys directory (CC BY 4.0)
  - `/.well-known/change-password` and `/.well-known/passkey-endpoints`
  - Have I Been Pwned (HIBP) breach data classes [bg]
- **Time engine:** features 3, 18 and 19, plus time-boxed shares.
- **On-device intelligence:** features 2, 9 (detector), 12, 16 and 17. One rule covers all of them: vault plaintext never leaves the device.
- **Approvals surface:** features 5, 3 (veto), 20 and new-device approval. Use one trusted prompt with number matching, on desktop, phone or watch.

---

## 1. Breach-response playbooks — V5 · M · Low · Score 16

**What exists.**
- 1Password Watchtower flags compromised sites and vulnerable, reused and weak passwords, using HIBP with k-anonymity. k-anonymity means only a hash prefix is sent, never the password. ([1Password blog](https://1password.com/blog/1password-q1-2025-usability-updates))
- Bitwarden added personal health alerts and "password coaching" to Premium in 2026. Its Enterprise **Access Intelligence** offers "guided remediation" and risk-over-time graphs (2026.4). ([Bitwarden blog](https://bitwarden.com/blog/measure-credential-risks-over-time-with-access-intelligence/))
- Google Password Checkup groups compromised, reused and weak passwords on one screen.
- Proton Sentinel adds human-reviewed protection against account takeover. ([Proton](https://proton.me/blog/proton-updates/proton-pass))
- Every one of these stops at "change this password."

**What is missing.** No consumer product runs an ordered, tracked response covering what *survives* a takeover:
- live sessions and stolen session cookies. Infostealers bypass multi-factor authentication (MFA) this way [bg].
- mail-forwarding rules
- changed recovery email or phone
- OAuth (delegated-access) grants to other apps
- exposed 2FA seeds
- every site that signs in through the compromised identity provider (IdP)

**What to build.**
- **Incident types**, each with its own playbook:
  1. site breach (hashed passwords)
  2. plaintext or credential stuffing
  3. 2FA seed or one-time code exposure
  4. **email account compromise**, the highest priority because it is the reset path for everything else
  5. **identity-provider compromise** (uses the feature 4 map)
  6. **infostealer on a device**, where revoking sessions matters more than changing passwords
  7. **breach of the password manager's own vault.** LastPass shows why: after 2022, crypto seeds stored in vaults were stolen, totalling more than $35M. ([Krebs](https://krebsonsecurity.com/2025/03/feds-link-150m-cyberheist-to-2022-lastpass-hacks/))
- **Each playbook** has ordered steps, deep links, checkboxes and a verification step (re-check HIBP and re-test the login). It notifies the members of any shared item and writes a history entry (feature 7) so the old password can be restored.
- **Prioritize by "reset-vector value":** email first, then identity providers, then finance, then everything else.
- **Hooks:** agentic change (17), connected-app review (15), passkey upgrade (6).

**Risks and mitigations.**
- Wrong advice: version the playbooks, cite sources and keep steps conservative.
- Alert fatigue: score severity and bundle alerts per incident.

---

## 2. On-device ML autofill field classification — V5 · L · Low · Score 14

**Why.** Autofill reliability is the top complaint across the market:
- **1Password:** failures on Edge and intranet sites, Windows bugs, and the extension losing its link to the desktop app. ([forum](https://www.1password.community/1password-at-home-31/i-am-extremely-disappointed-with-1password-they-ve-raised-their-prices-but-haven-t-fixed-the-bugs-24530))
- **Bitwarden:** card-field misses. Its 2026.7 response was "Fill assist", a *curated list* of broken sites. ([release notes](https://bitwarden.com/help/releasenotes/))
- **Proton Pass:** "doesn't work 80% of the time". ([AMO reviews](https://update.mozilla.org/en-US/firefox/addon/proton-pass/reviews/?score=4))

**What exists.**
- Chrome uses crowd-sourced, server-side field-type predictions plus heuristics [bg].
- Mozilla built **Fathom**, a "supervised-learning system for recognizing parts of web pages". Its repo is now deprecated and read-only. ([GitHub](https://github.com/mozilla/fathom)) Firefox used it for form detection [bg].
- No major password manager is confirmed to ship a learned, on-device classifier [bg].

**What to build.**
- **The model:** a small classifier running in WebAssembly or ONNX inside the content script.
  - It reads features from the page structure and the accessibility tree: the `autocomplete` attribute, label, placeholder and nearby text in many languages, input type, geometry, form action, and position in the form.
  - It outputs one of these labels: username/email, current-password, new-password, confirm, OTP (including split six-box OTP), card number/expiry/CVC, name, address, or **do-not-fill** (search boxes and the like).
- **Precedence:** explicit `autocomplete`, then the user's per-site override (end-to-end encrypted, E2EE, and synced), then the model, then heuristics.
- **Learning:** opt-in "form fingerprint" feedback. It sends hashed structure only, never values or URLs in the clear, and the model is retrained centrally. Ship signed, reproducible model files.
- **Native apps** reuse the same model: iOS/Android autofill hints and the Windows UI Automation tree.

**Risks and mitigations.**
- A password filled into a visible text field: hard rule, passwords go only into `type=password` fields unless the user confirms.
- Poisoned feedback: robust aggregation and staged rollout.
- Model supply chain: signed artifacts.

---

## 3. Digital legacy and dead-man switch — V5 · M · Med · Score 14

**What exists.**
- **Bitwarden Emergency Access** (Premium): a trusted contact gets View or Takeover access after a waiting period, through public-key exchange [TK in report 02].
- **Proton Pass emergency access** (August 2025): access after a set period of inactivity. ([report 02](https://alternativeto.net/software/proton-pass/news))
- **Keeper, LastPass and NordPass** have emergency-access features [bg].
- **Dashlane** retired its Emergency feature [bg, date unverified].
- **1Password** has *no* emergency access. It relies on the Emergency Kit and Family Organizer recovery. ([1Password](https://1password.com/blog/introducing-1password-recovery-codes))
- **Apple Legacy Contact** excludes Keychain data [bg]. Report 01 marks Keychain legacy access [unverified].
- **Google Inactive Account Manager** triggers after 3 to 18 months of inactivity and covers up to 10 contacts [bg].

**The security lesson.** The ETH Zurich / USI study (USENIX Security 2026) attacked recovery and escrow flows. In those flows the client fetched the recipient's **public key from the server without authenticating it**, so a malicious server could substitute its own key and capture escrowed keys. ([secure.com summary](https://www.secure.com/blog/researchers-expose-major-security-gaps-in-leading-password-managers), [ePrint 2026/058](https://eprint.iacr.org/2026/058))

**What is missing.** One system that combines:
- (a) a **scoped legacy vault**: only what heirs need, plus a letter and instructions
- (b) an **inactivity trigger** with check-ins over several channels and an owner veto
- (c) **k-of-n heirs** using Shamir secret sharing, so no single contact can take over
- (d) **authenticated contact keys** through key transparency plus a fingerprint or QR check at enrollment
- (e) a **legacy drill**
- (f) a legal hook. Under the US RUFADAA (the fiduciary digital-assets law), a user's direction in a custodian's **"online tool"** overrides their will and the terms of service [bg]. Design the settings so they can count as such a tool.

**What to build.**
- **Trigger states:** Armed → Check-in due → Grace → Release pending → Released.
  - Grace means notices on every owner device plus email and SMS.
  - Release pending means heirs are notified and the owner has a 7–30 day veto.
  - Released means the heirs' shares combine to unwrap the legacy-vault key.
- **What the server can do:** only run the clock. It cannot decrypt, because the shares are wrapped to each heir's hybrid public key.

**Risks and mitigations.**
- False trigger, for example a hospital stay: long windows and many channels.
- Heir collusion or coercion: a quorum, a veto, and notices to every party.
- A malicious server releasing early: the server holds no key material.
- Heirs losing their own keys: a printable heir kit.
- Abuse within a household: the owner can see and revoke enrollment at any time.

---

## 4. Federated-login map — V4 · S · Low · Score 14

**What exists.**
- 1Password records "Sign in with Google/Apple/…" on Login items [bg].
- 1Password's **Universal sign-in** (8.12.36, September 2026) picks password, passkey or SSO for you. ([1Password community](https://www.1password.community/announcements-52/september-2026-at-1password-universal-sign-in-for-everyone-retiring-1password-7-and-a-quick-access-change-on-macos-25734))
- Apple Passwords tracks Sign in with Apple and Hide My Email entries. (report 01)
- Most other managers store nothing [bg].

**What is missing.** A dependency graph and blast radius. For example: "41 accounts depend on *personal* Google ID x@gmail.com; 6 on Steam OpenID." It should also warn before an identity-provider account is deleted or loses recovery.

**What to build.**
- **Sign-in method on each login:** password, passkey, federated (provider plus *which* account), SSO, magic link, or email code.
- **Auto-capture:** detect clicks on known identity-provider buttons and redirect chains, such as:
  - `accounts.google.com`, `appleid.apple.com`, `login.microsoftonline.com`
  - `steamcommunity.com/openid`, `facebook.com/dialog/oauth`, `github.com/login/oauth` [bg]
- **IdP health card:** at least two passkeys per identity provider, recovery verified, and legacy (feature 3) covered.
- **Feeds** features 1 and 15.

**Risk.** Low. The graph is sensitive social metadata, so encrypt it like all other metadata (report 04 §4.1).

---

## 5. Agent-safe credential brokering (MCP) — V5 · L · Med · Score 13

**What exists.**
- **1Password:**
  - **Agentic Mode / 1Password for Claude** (July 2026): the extension locks its UI and fills only the logins and OTPs approved for the task, and the model never sees them. It is on by default, Mac first.
  - **Browserbase** (October 2025): human approval for each login over a Noise-protocol E2EE channel. Audit logs are "planned."
  - **Credential Broker** (July 2026): per-run credentials from GitHub Actions OIDC trust.
  - **Unified Access:** discovers agents and exposed secrets. Audit is "coming soon."
  - Sources: [press](https://1password.com/press/2026/july/1password-for-claude), [Browserbase](https://1password.com/press/2025/oct/browserbase-ai-security-partnership), [HNS](https://www.helpnetsecurity.com/2026/07/29/1password-privileged-access/)
- **Bitwarden MCP server**, read first-hand on GitHub:
  - Tools to list, retrieve, create and edit items, plus organization admin.
  - Unlock opens a native OS dialog, so the master password never passes through MCP.
  - The README warns it can "expose credentials in AI responses" and must be "exclusively for local use" and "never be hosted publicly." ([GitHub](https://github.com/bitwarden/mcp-server))
  - In other words, secrets *do* enter the model's context.
- **Proton:** Personal Access Tokens for scripts and agents. ([roadmap](https://proton.me/blog/pass-roadmap-spring-summer-2026))
- **The MCP 2025-11-25 specification** adds URL-mode elicitation, read first-hand:
  - Servers "**MUST NOT** use form mode elicitation to request sensitive information such as passwords, API keys…" and "**MUST** use URL mode" for them.
  - Third-party credentials must never reach the MCP client.
  - Clients must show the full URL and get consent.
  - Sources: [changelog](https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/docs/specification/2025-11-25/changelog.mdx), [elicitation](https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/docs/specification/2025-11-25/client/elicitation.mdx)

**What is missing.** An open broker that works across browsers and operating systems. Secrets never enter the model's context, capabilities are scoped per task, and there is a real, exportable audit log.

**What to build.**
1. **Tools are verbs, not getters.** For example `login(site, task)`, `http_with_credential(item, url)` (a local proxy injects the header and refuses any other host), `run_with_env(cmd, items)` and `ssh_sign`. Each returns status only, never the secret.
2. **Capability tokens:** item, origin, action, time limit and maximum uses. They are signed, revocable and bound to an agent identity (MCP client-ID metadata).
3. **Per-task human approval** on a trusted surface: desktop, phone or watch (feature 20). The prompt shows the agent, the task, the destination domain and a matching number.
4. **Origin binding at the point of injection.** The broker never fills a credential into a host other than the item's own.
5. **An E2EE, hash-chained audit log** of every use. Ship it on day one, which beats 1Password.
6. **Deny by default** for high-blast-radius classes: seeds (feature 9), email and identity-provider masters, and banking.
7. **Mask filled fields** during agent sessions so screenshots taken by the agent do not leak them.

**Risks and mitigations.**
- Prompt injection causing a confused-deputy exfiltration: origin binding plus no secret in tool results.
- Rogue local clients: authenticate the inter-process (IPC) peer by code signature, given the NDSS 2025 finding about Windows IPC. ([NDSS](https://www.ndss-symposium.org/?p=23650))
- Approval fatigue: approve per task, with a clear summary.

---

## 6. Passwordless migration coaching — V4 · M · Low · Score 13

**What exists.**
- 1Password Watchtower shows "passkey available" (report 01).
- Apple adds passkeys automatically after a password sign-in (conditional create).
- Chrome's docs say that on iOS 18+ conditional create "is up to the passkey provider". A provider that does not implement it falls behind. ([Chrome docs](https://developer.chrome.com/docs/identity/webauthn-conditional-create))
- **Open data:** `2factorauth/passkeys` feeds the Passkeys Directory.
  - Data license: CC BY 4.0. Icons are excluded.
  - API: `passkeys.2fa.directory/api/`.
  - Commercial users are asked to sponsor it. ([GitHub](https://github.com/2factorauth/passkeys))

**What is missing.**
- A *plan* ranked by how critical each account is.
- Tracking of the "**residual password**": the site still accepts the password, so phishing still works.
- Coaching to add a backup passkey.
- A view across the household.

**What to build.**
- A progress meter.
- Per-site steps and deep links: `/.well-known/passkey-endpoints` and `/.well-known/change-password`.
- Implement conditional create.
- Handle the Signal API, with soft-delete.
- A "2 authenticators before removing password" rule.
- Feed the family dashboard (feature 8).

**Risks and mitigations.**
- Lockout: never remove a password automatically, and require a second passkey or a recovery code first.
- Stale directory data: attribute the source and let users report errors.

---

## 7. Per-item version history and tamper-evident audit trail — V4 · M · Low · Score 13

**What exists.**
- 1Password can restore previous versions of an item [bg]. Keeper has full record history [bg].
- Apple Passwords added "View History" for passwords in iOS 26. ([9to5Mac](https://9to5mac.com/2025/11/06/ios-26-passwords-app-has-convenient-new-feature/))
- Bitwarden keeps only password history [bg].
- Business event logs exist: Bitwarden on Teams and above with SIEM export (2026.8), and 1Password Business [bg].
- **The ETH threat model:** a malicious server can roll back, delete or reorder items. The fix is a **signed, hash-chained per-vault manifest** (report 04 §4.4).

**What is missing.** Consumer-visible, *verifiable* history for shared vaults. For example: "Dad changed the Netflix password 3 Oct" or "rollback attempt blocked."

**What to build.**
- Field-level versions that are E2EE and signed by the editing device's key. Keep one year by default and restore in one click.
- For shared items, log reveal, copy and fill events, encrypted to vault members and **visible to every member** (symmetric transparency).
- Verify the manifest chain on each sync.
- A vault "time machine."
- Exportable logs.

**Risks and mitigations.**
- Storage cost: store diffs.
- Surveillance inside a family: logs are visible to all members, nothing is covert.
- Erasure requests against an immutable chain: crypto-shredding, meaning delete the per-version keys.

---

## 8. Family and kids accounts — V4 · M · Med · Score 13

**What exists.**
- 1Password Families: the Organizer can recover members (report 01).
- Bitwarden Families: 6 users, $47.88 a year (report 02).
- Proton Pass Family: 6 users.
- Google: sharing only within a Family group of up to 6, as copies; revoking means changing the password. ([TechRadar](https://www.techradar.com/pro/security/google-password-manager-now-lets-families-share-details-across-one-account))
- Apple: shared groups with any contact.
- **No product has kid-specific roles** [bg].

**What to build.**
- **Roles:** Guardian, Teen, Child, and Assisted adult (for an elderly parent, overlapping feature 3).
- **Guardian-held recovery** as a *break-glass* action that always notifies.
- **Policy for under-13s:** no export, no Send outside the family, and only approved item types.
- **Simple unlock:** PIN plus biometrics.
- **A household vault** for Wi-Fi (feature 13), streaming, door codes and school portals.
- **Passkey-first onboarding.**
- **Graduation:** at the age of majority, re-key the vault so the guardian loses access.

**Risks.**
- Children's privacy law: COPPA (US, under 13, verifiable parental consent; the FTC amended the rule in 2025) [bg], and GDPR Art. 8, where the digital consent age is 13 to 16 [bg].
- **Abuse by a controlling partner:** no covert access, break-glass always notifies, and any adult member can leave and re-key their personal vault.

---

## 9. Crypto seed-phrase safe mode (SLIP-39 aware) — V4 · M · Med · Score 13

**What exists.**
- 1Password has a **Crypto Wallet** item type (report 01).
- **LastPass precedent:** seeds stored in stolen vaults with weak key-derivation settings led to more than $35M of theft (estimates run to about $100M or more). A $150M heist has been linked to it. ([Krebs](https://krebsonsecurity.com/2025/03/feds-link-150m-cyberheist-to-2022-lastpass-hacks/))
- **SLIP-39**, read first-hand from the spec:
  - Shamir secret sharing at two levels: groups, and members within groups (1–16 each).
  - A 1024-word list. Shares are 20 words for a 128-bit secret and 33 words for a 256-bit secret.
  - A BIP-39 seed can be converted to SLIP-39, but the shares are **59 words**. SLIP-39 cannot be converted back to BIP-39.
  - The passphrase is **deliberately unverifiable**, to allow decoy wallets.
  - Implementations "should not auto-correct" checksum errors.
  - ([SLIP-0039](https://github.com/satoshilabs/slips/blob/master/slip-0039.md))
- Trezor supports SLIP-39 [bg]. Ledger Recover's custodial shards caused a backlash in 2023 [bg].

**What is missing.** No password manager detects seeds pasted into notes or treats them differently.

**What to build.**
1. **A detector:** a client-side scan for 12–24 valid BIP-39 words with a valid checksum, or 20 or 33 SLIP-39 words. It nudges the user to move the seed into a Crypto item.
2. **Seed items as high-blast-radius:**
   - a separate key that needs the full password (not biometrics) for each reveal
   - never autofilled or copied, and screenshots blocked
   - excluded from search snippets, agents (feature 5), credential exports (CXF/CSV) and legacy (feature 3) unless the user opts in
   - an option to keep it on this device only
3. **"Store the plan, not the seed":**
   - wallet and firmware
   - derivation path and output descriptor
   - xpub (watch-only)
   - where the shares or metal plates are kept
   - a passphrase *hint*, never the passphrase
4. **A Shamir helper:** the vault holds at most threshold − 1 shares, plus printable share cards and a share-verification drill. Use the audited reference implementations, not custom crypto.

**Risk.** Encouraging people to digitize seeds at all goes against hardware-wallet practice. The default copy should say "don't keep the seed here; keep the recovery plan here."

---

## 10. Account deletion / "forget me" helper — V3 · S · Low · Score 13

**What exists.**
- No password manager integrates deletion help.
- **JustDeleteMe**, read first-hand: MIT license, `_data/sites.json`. Each entry has:
  - `name`, `url`, `difficulty` (easy, medium, hard, limited or impossible)
  - `notes`, `email`, `email_subject`, `email_body`
  - `domains[]`, plus localized `url_xx` and `notes_xx`
  - It has hundreds of entries, and the file is about 1.2 MB. ([repo](https://github.com/jdm-contrib/jdm), [data](https://raw.githubusercontent.com/jdm-contrib/jdm/master/_data/sites.json))
- The App Store has required in-app account deletion since 2022 [bg].
- Consumer Reports' Permission Slip [bg], data-broker removal services (Incogni, DeleteMe, Optery) [bg], and California's Delete Act DROP platform for data brokers (2026) [bg, status unverified].

**What to build.**
- **Dormant-account detection** from local last-fill timestamps.
- **Match** each item's `domains` to JustDeleteMe and show difficulty and steps.
- **Request templates** for GDPR Art. 17 and CCPA, built from the JustDeleteMe email fields.
- **After deletion,** turn the item into a "Deleted on <date>" tombstone. Keep the username so later breaches can still be matched. Remove the TOTP seed once the user confirms.
- **Optional:** a data-broker partner module.

**Risks.**
- Irreversible mistakes: never automate the final delete click.
- Stale data: send fixes upstream.

---

## 11. Email and SMS one-time-code autofill — V4 · L · Med · Score 12

**What exists.**
- **Apple:** the system offers codes from Messages (iOS 12+) and Mail (iOS 17 / macOS 14) in the QuickType bar *whichever password manager is selected* [bg]. Third-party providers can supply only their own TOTP codes (iOS 18+) (report 04 §1.2).
- **Android:**
  - The **SMS Retriever API** needs an app hash in the message, so it only works for an app's own backend [bg].
  - The **SMS User Consent API** needs no SMS permission. The user approves each message, there is a 5-minute window, the code must be 4–10 alphanumeric characters with at least one digit, and the sender must not be in contacts [bg; Google's docs failed DNS].
  - `READ_SMS` is restricted by Play policy to default SMS apps [bg].
  - Android 15 hides one-time-code notifications from untrusted notification listeners [bg].
  - Sites can use the **WebOTP API** in Chrome [bg].
- **1Password's native macOS AutoFill:** one-time codes are "later in the beta." ([piunikaweb](https://piunikaweb.com/2026/05/27/1password-native-macos-autofill-public-beta/))
- **Email:** Gmail's restricted scopes require an annual third-party security assessment (CASA) [bg].

**What is missing.** No third-party manager relays SMS or email codes across devices. Chrome and Firefox on Windows and Linux get nothing.

**What to build.**
- **Android:** start a User Consent listener when an OTP field is focused, on the phone or reported by the desktop extension. Relay the code E2EE to the desktop, the way an iPhone relays to a Mac.
- **Origin matching:** require the code to match the page origin. Prefer the origin-bound format `@domain #code` [bg].
- **Email, two options only:**
  - (a) your **own** masked-email or alias domain, with codes parsed on the client
  - (b) a user-connected JMAP or IMAP account read **on the device**, limited to the last 10 minutes and to senders matching the item's domain
- **iOS:** do not compete with the system.

**Risks.**
- Relaying a code into a phishing page: require an origin match.
- Mailbox access is a large scope: opt-in, on-device and read-only.
- Play policy may change.

---

## 12. Document scan + OCR into typed items — V4 · M · Med · Score 12

**What exists.**
- 1Password has 22 typed categories, including Passport, Driver License and Medical Record. ([CLI docs](https://developer.1password.com/docs/cli/item-fields)) It can scan cards with the camera [bg].
- Proton has custom templates, including passport. ([support](https://proton.me/support/create-custom-item))
- Apple Passwords has no such types (report 01).
- Platform wallets now hold digital IDs made from passports: Apple Wallet (iOS 26) and Google Wallet [bg].
- **OS building blocks** [bg]:
  - VisionKit document camera and Live Text
  - ML Kit Document Scanner and Text Recognition
  - passport machine-readable zone (MRZ, ICAO 9303) with check digits
  - NFC ePassport chip reading (Core NFC, Android)
  - the PDF417 barcode on US driver's licences (AAMVA)

**What to build.**
- **Pipeline:** scan, then on-device OCR, then MRZ or barcode parsing with check-digit validation, then typed fields, then expiry reminders (feature 18).
- **Optional chip read** to verify the data.
- **Defaults:** "data only." The image is optional, encrypted under a separate item key. Don't store the chip photo by default.

**Risks.**
- ID images are valuable for identity theft: data-only default and an extra key.
- OCR errors: the user confirms each field.
- The chip photo is biometric data under GDPR: don't keep it.

---

## 13. Wi-Fi QR sharing — V3 · S · Low · Score 12

**What exists.**
- Apple Passwords has a Wi-Fi tab with QR codes (report 01).
- Android can share Wi-Fi by QR at the OS level [bg].
- 1Password has a Wireless Router item. Whether it shows a QR code is [unverified].
- Proton has a custom Wi-Fi template. Bitwarden 2026.10 imports Proton Wi-Fi items. ([GitHub releases](https://github.com/bitwarden/clients/releases))
- **Apple's Credential Exchange (CXP) export drops Wi-Fi items** (report 01).
- **The format,** read first-hand: `WIFI:T:WPA;S:<ssid>;P:<pw>;H:true;;`. Escape `\ ; , " :`. Supported on Android and iOS 11+. ([ZXing](https://github.com/zxing/zxing/wiki/Barcode-Contents))

**What to build.**
- A Wi-Fi item type: WPA2/WPA3-SAE [bg] and the EAP fields.
- A full-screen "guest card" that hides itself after a while.
- A printable card.
- NFC tag writing on Android [bg].
- Time-boxed share links.
- **Keep Wi-Fi items in exports** through a credential-exchange-format (CXF) extension field.

**Risk.** Low. QR photos persist, so suggest a guest network and a rotation reminder.

---

## 14. Browser-less TV/console sign-in helper — V3 · S · Med · Score 12

**What exists.**
- **The standard:** the OAuth 2.0 device authorization grant (RFC 8628). The TV shows a code, and the user approves on a phone. ([datatracker](https://datatracker.ietf.org/doc/rfc8628/))
- **Consoles:** PlayStation has supported passkeys since 2024-02-21, and Nintendo since 2023. ([playstation.com](https://www.playstation.com/passkey/))
- **iOS:** credential-provider extensions get hybrid (QR) passkey support from the system "for free". ([Apple forums 829541](https://developer.apple.com/forums/thread/829541))
- **TV keyboards:** Apple TV and Google TV accept phone keyboard input with autofill [bg].
- **No password manager has a "TV mode"** [bg].
- **The threat:** device-code phishing is used in real attacks; Microsoft reported the Storm-2372 campaign in 2025 [bg].

**What to build.**
- **QR scanner inside the app:** it recognizes known activation URLs from an allow-list, opens them with the right item, and shows the requesting device's name.
- **A large-type view** that groups characters in threes and colors them by type, for typing with a TV remote.
- **Passkeys over hybrid** where the service supports them.
- **Hotel and rental mode:** log the sign-in and nudge "sign out of all devices" on the checkout date.

**Risk.** Device-code phishing: start only from a scan the user initiates, use the allow-list, and always show the full domain.

---

## 15. OAuth connected-app inventory — V3 · M · Low · Score 12

**What exists.**
- **Enterprise only:**
  - 1Password Extended Access Management (XAM): shadow-IT and app discovery, and SaaS Manager. ([BetaKit](https://betakit.com/1password-looks-to-help-businesses-secure-unmanaged-devices-with-a-new-offering/))
  - Bitwarden Access Intelligence: an app-level view.
- **Consumers** rely on each provider's own page: Google connections, Microsoft consent, GitHub authorized apps, and Apple's Sign in with Apple list [bg].
- Grant lists are mostly available only through admin APIs, such as Workspace and Microsoft Entra [bg].

**What to build.**
- A guided quarterly review that uses the feature 4 map to deep-link each identity provider you actually use.
- Flag high-scope grants: mail, drive and full account.
- Insert the review automatically after incidents (feature 1).
- Admin connectors for Teams plans.

**Risk.** Holding broad OAuth tokens just to read grant lists creates a high-value secret. Consumers get deep links only.

---

## 16. Natural-language vault search (on-device) — V3 · M · Low · Score 12

**What exists.**
- No major password manager is confirmed to ship this [bg].
- Platform on-device models exist: Apple Foundation Models (OS 26), Gemini Nano through AICore and ML Kit, and Chrome's built-in AI APIs [bg].

**What to build.**
- Hybrid keyword plus semantic search over titles, tags, typed fields and notes.
- Queries like "mom's wifi", "passports expiring this year" or "all streaming logins".
- The index is encrypted with the vault key and works offline.
- **Retrieval only.** No generative step ever runs over secrets.

**Risks.**
- Leakage from the index: encrypt it.
- Text in shared notes written to manipulate a model: retrieval only.
- Model size on mobile and in the extension: use the platform model where one exists.

---

## 17. Agentic password change — V4 · XL · Med · Score 10

**What exists.**
- **Apple, iOS 27:** Apple Intelligence plus Safari.
  - Supported sites only. It fails when a site needs one-time codes, extra identity checks or managed sign-in.
  - It may slip to 27.1. ([9to5Mac](https://9to5mac.com/2026/06/08/ios-27s-passwords-app-can-change-your-passwords-for-you-automatically/), [The Register](https://www.theregister.com/personal-tech/2026/06/09/apples-ios-27-goes-all-agentic-on-compromised-passwords/5252957))
- **Google:** automated change for participating sites (I/O 2025). The Gemini "Change it for me" is in Chrome Canary behind a flag and is unreliable. ([Android Authority](https://androidauthority.com/chrome-canary-gemini-change-bad-passwords-3698237))
- **A third-party Mac app** uses local agents. ([feedbagel](https://feedbagel.com/post/password-app-automates-password-updates-using-local-ai-agents-on-mac))

**What to build, in tiers.**
1. `/.well-known/change-password` deep links.
2. Signed, deterministic per-site recipes, maintained by the community.
3. A model fallback that **navigates only**:
   - secret fields are masked from the model
   - trusted code types the new password
4. **Two-phase commit:** save the pending password as a version (feature 7), submit, re-login to verify, then promote it. Roll back on failure.
5. **Hand-offs:** stop at a one-time code or CAPTCHA and hand over to the user (feature 11).
6. **Approval:** per change, or per batch with a progress view.

**Risks.**
- Lockout: handled by the pending version and rollback.
- Prompt injection from page content: the model holds no secret and is limited to the site's origin.
- Terms of service and bot detection: recipes and site opt-outs.

---

## 18. Time- and location-aware items — V3 · M · Med · Score 10

**What exists.**
- 1Password: item expiry dates with Watchtower alerts, and "nearby" items by location (Q1 2025). ([1Password blog](https://1password.com/blog/1password-q1-2025-usability-updates))
- Bitwarden Send: deletion dates of 31 days at most. Psst! links: 1 hour to 30 days.

**What to build.**
- Expiry and renewal reminders for documents (feature 12), cards, domains and certificates.
- Time-boxed shares and guest-access windows, such as rental door codes.
- Context profiles (work, home, travel) driven by Focus mode, Wi-Fi network or a geofence, all on the device. They reorder and hide items **only**.

**Risks.**
- Location privacy: raw location never syncs.
- GPS spoofing: location is never a security boundary.
- Server clock tampering: expiry is signed and checked on the client.

---

## 19. Panic lock / duress wipe / travel-safe — V3 · M · Med · Score 10

**What exists.**
- **1Password Travel Mode** removes vaults not marked safe for travel, keys included, from devices. No changes in 2025–26. ([1Password blog](https://1password.com/blog/introducing-travel-mode-protect-your-data-when-crossing-borders))
- **GrapheneOS duress PIN/password**, read first-hand: it "irreversibly wipe[s] the device (along with any installed eSIMs)". It needs no reboot and "cannot be interrupted." If the duress value matches the real credential, the real one wins. ([GitHub features.html](https://github.com/GrapheneOS/grapheneos.org/blob/main/static/features.html))
- Keeper Self-Destruct wipes local data after failed attempts [bg].

**What to build.**
- **Panic lock:** a hotkey, gesture or watch button locks every device, clears the clipboard and ends sessions.
- **Duress PIN:** wipes the local cache and device keys and de-registers the device. The cloud copy survives, and re-login needs the password plus the Secret Key. **Allowed only when the device is fully synced.**
- **Travel profile** scheduled by trip dates.
- **Remote wipe** for a lost device.

**Risks.**
- Data loss: block duress wipe on unsynced devices.
- **Legal:** destroying data after an official demand can count as obstruction in many jurisdictions [bg]. Document this plainly. The design wipes visibly; it does not deceive.

---

## 20. Apple Watch / Wear OS approvals and proximity — V3 · L · Med · Score 9

**What exists.**
- The `bitwarden/ios` repo has **BitwardenWatchApp, BitwardenWatchShared and BitwardenWatchWidgetExtension** targets, read first-hand. ([GitHub](https://github.com/bitwarden/ios))
- 2FAS has an Apple Watch app (report 02).
- Keeper has Watch and Wear OS apps with KeeperDNA [bg].
- 1Password unlocks on the Mac with an Apple Watch [bg]. Its watch app is [unverified] for 2026.
- **Secretive** gates SSH key use behind Touch ID or Apple Watch and notifies on each use. ([GitHub](https://github.com/maxgoedjen/secretive))

**What to build.**
- **The watch as an approval surface** with number matching: new devices, agent use (feature 5), legacy veto (feature 3) and share requests.
- TOTP at a glance.
- Mac unlock with the watch.
- Lock the vault when the phone or watch leaves Bluetooth range. This only ever locks.
- Ultra-wideband (UWB) distance checks where available [bg].

**Risks.**
- Bluetooth relay attacks: proximity **never unlocks**.
- A stolen watch: wrist detection plus the watch passcode.
- Wear OS fragmentation.

---

## 21. age/GPG key storage and agents — V2–3 · M · Med · Score 9

**What exists.**
- **Password managers** ship SSH agents only: 1Password, Bitwarden (GA in 2026.3) and KeePassXC. None ships an age or GPG agent [bg].
- **age**, read first-hand:
  - Plugins are binaries on `$PATH`, for example `age-plugin-yubikey` and `age-plugin-pq`.
  - Native post-quantum hybrid keys come from `age-keygen -pq` in v1.3.0+. The recipients (`age1pq1…`) are about 2,000 characters.
  - **age does not support ssh-agent.** ([GitHub](https://github.com/FiloSottile/age))
- **age-plugin-se:** keys bound to the Secure Enclave on macOS 14+, with access-control options. `age1tag1` recipients work natively in age 1.3+. ([GitHub](https://github.com/remko/age-plugin-se))

**What to build.**
- Typed items for age identities (including post-quantum) and OpenPGP keys.
- **`age-plugin-<pm>`:** sends decrypt requests to the desktop app, which asks for biometric approval and shows the file and recipient. It returns only the file key.
- A sops recipe [bg].
- **GPG:** storage plus export-to-agent on approval first. A full `gpg-agent` (Assuan protocol) can come later. Steer Git signing to SSH signing (Git 2.34+).

**Risks.**
- Approval fatigue.
- IPC peers: check code signatures.

---

## 22. Email magic-link capture — V3 · L · High · Score 9

**What exists.** No known product does this [bg]. It uses the same channel as email codes in feature 11.

**What to build.**
- Only through your own alias inbox.
- Match the link's host to a pending sign-in item.
- Open it in the same browser profile on a tap, never automatically, and let it expire.

**Risks.** Phishing links, login-CSRF, and email security scanners that use up single-use tokens before the user does. This only makes sense if you run a first-party alias service, as Proton does with SimpleLogin.

---

## 23. Passkeys for SSH (`sk-*`) — V2 · L · Med · Score 8

**What exists.**
- **OpenSSH FIDO key types**, read first-hand:
  - `sk-ecdsa-sha2-nistp256@openssh.com` and `sk-ssh-ed25519@openssh.com`, plus certificate variants.
  - The application string is "typically `ssh:`".
  - Middleware libraries implement `sk-api.h`.
  - Attestation is returned when the key is generated but **not saved by default**. ([PROTOCOL.u2f](https://github.com/openssh/openssh-portable/blob/master/PROTOCOL.u2f))
  - The feature arrived in OpenSSH 8.2 (2020) [bg].

**Analysis.** A software `sk` key that syncs defeats the reason admins require `sk` keys: they are bound to hardware and can be attested. Better options:
- (a) store `sk` key handle files and record which hardware key they belong to
- (b) **issue short-lived OpenSSH certificates** from the vault after a passkey approval, with the vault acting as an SSH certificate authority [bg]
- (c) keep the existing agent model, which already asks for biometrics on each use

---

## 24. Hidden vaults / plausible deniability — V2 · XL · High · Score 6

**Precedent.**
- No mainstream password manager offers this.
- **VeraCrypt hidden volumes** and **Shufflecake** [bg].
- **BIP-39 and SLIP-39 passphrase decoy wallets.** SLIP-39 makes the passphrase unverifiable by design (spec, first-hand).
- **GrapheneOS chose to wipe, not to show a decoy** (first-hand).

**Why not to ship it.**
1. **Cloud sync leaks its existence.** The server sees how many blobs there are, how big they are and when they sync. The anti-rollback manifest the foundation requires (signed lists of item IDs) contradicts hidden items directly.
2. **The operating system leaks it.** iOS `ASCredentialIdentityStore` entries, Android autofill datasets, favicon caches and browser history all reveal hidden items' domains. (This is my inference from the platform designs.)
3. **The law compels disclosure.** For example, under UK RIPA Part III s49, refusing is punishable by up to 2 years, or 5 years in national-security cases [bg].
4. **"Rubber-hose" escalation.** If the product is known to support hidden vaults, a coercer can never be satisfied. That endangers every user, including those with nothing hidden.
5. **Forgotten hidden passwords** cannot be recovered.

**Verdict.** Ship feature 19 instead. If deniability is ever wanted, make it a separate, clearly labeled, local-only product.

---

## Cross-cutting guardrails

- **No vault plaintext leaves the device for AI** (features 2, 9, 12, 16, 17). Prefer platform on-device models [bg].
- **A blast-radius class for items.** Seeds, `identity_secret`-style secrets (report 03), and email and identity-provider masters get:
  - a separate key
  - no agent access
  - no autofill
  - no bulk export
  - legacy only when the user opts in
- **Authenticated keys everywhere a secret is shared to someone else:** legacy, family, sharing and agents. This is the ETH lesson.
- **Proximity, location and time are conveniences, never security boundaries.**
- **Every automated action can be undone** through history (feature 7), and the final step of anything irreversible stays human (features 10 and 17).

## Suggested sequencing

- **v1 quick wins (S):** 4, 10, 13, and the data side of 6.
- **v1 core:** 1, 3, 7 (shares the manifest work), 9, and 6.
- **v1.x:** 2, 5, 8, 11 (Android), 12, 14, 15, 16, 18 (expiry), and 19.
- **v2:** 17, 20, 21, 22, the email part of 11, and context profiles (18).
- **Reframe:** 23.
- **Decline:** 24.

## Verify before the spec

These rest on [bg] or [unverified]:
- Apple Legacy Contact excluding Keychain
- Google Inactive Account Manager covering passwords
- Dashlane retiring Emergency access
- the details of Android's SMS User Consent API and Android 15's one-time-code notification redaction
- the Gmail CASA requirement
- 1Password's "sign in with" items and its Wireless Router QR code
- 1Password and Keeper item history
- Keeper's Watch and Wear OS apps and Self-Destruct
- the status of California's DROP platform
- the COPPA 2025 amendment
- the Storm-2372 device-code phishing campaign
- availability of Apple Foundation Models and Gemini Nano to third-party apps
- that no password manager ships an ML field classifier or natural-language search

## Sources

**Read first-hand this session (GitHub):**
- [SLIP-0039 spec](https://github.com/satoshilabs/slips/blob/master/slip-0039.md)
- [GrapheneOS features.html](https://github.com/GrapheneOS/grapheneos.org/blob/main/static/features.html)
- [OpenSSH PROTOCOL.u2f](https://github.com/openssh/openssh-portable/blob/master/PROTOCOL.u2f)
- [age](https://github.com/FiloSottile/age)
- [age-plugin-se](https://github.com/remko/age-plugin-se)
- [Secretive](https://github.com/maxgoedjen/secretive)
- [Bitwarden MCP server](https://github.com/bitwarden/mcp-server)
- [bitwarden/ios](https://github.com/bitwarden/ios)
- [JustDeleteMe repo](https://github.com/jdm-contrib/jdm) and [sites.json](https://raw.githubusercontent.com/jdm-contrib/jdm/master/_data/sites.json)
- [2factorauth/passkeys](https://github.com/2factorauth/passkeys)
- [ZXing barcode contents](https://github.com/zxing/zxing/wiki/Barcode-Contents)
- [Mozilla Fathom](https://github.com/mozilla/fathom)
- [MCP 2025-11-25 changelog](https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/docs/specification/2025-11-25/changelog.mdx)
- [MCP elicitation spec](https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/docs/specification/2025-11-25/client/elicitation.mdx)

**Carried from reports 01–04** (originally from WebSearch summaries):
- 1Password: [Q1 2025 updates](https://1password.com/blog/1password-q1-2025-usability-updates), [recovery codes](https://1password.com/blog/introducing-1password-recovery-codes), [Travel Mode](https://1password.com/blog/introducing-travel-mode-protect-your-data-when-crossing-borders), [1Password for Claude](https://1password.com/press/2026/july/1password-for-claude), [Browserbase](https://1password.com/press/2025/oct/browserbase-ai-security-partnership), [Sep 2026 announcements](https://www.1password.community/announcements-52/september-2026-at-1password-universal-sign-in-for-everyone-retiring-1password-7-and-a-quick-access-change-on-macos-25734), [CLI item fields](https://developer.1password.com/docs/cli/item-fields), [Privileged Access (HNS)](https://www.helpnetsecurity.com/2026/07/29/1password-privileged-access/), [XAM (BetaKit)](https://betakit.com/1password-looks-to-help-businesses-secure-unmanaged-devices-with-a-new-offering/), [autofill complaints](https://www.1password.community/1password-at-home-31/i-am-extremely-disappointed-with-1password-they-ve-raised-their-prices-but-haven-t-fixed-the-bugs-24530), [native macOS AutoFill beta](https://piunikaweb.com/2026/05/27/1password-native-macos-autofill-public-beta/)
- Apple and Google: [iOS 26 password history](https://9to5mac.com/2025/11/06/ios-26-passwords-app-has-convenient-new-feature/), [iOS 27 auto-change (9to5Mac)](https://9to5mac.com/2026/06/08/ios-27s-passwords-app-can-change-your-passwords-for-you-automatically/), [iOS 27 (The Register)](https://www.theregister.com/personal-tech/2026/06/09/apples-ios-27-goes-all-agentic-on-compromised-passwords/5252957), [Apple forums 829541](https://developer.apple.com/forums/thread/829541), [Chrome conditional create](https://developer.chrome.com/docs/identity/webauthn-conditional-create), [Gemini change (Canary)](https://androidauthority.com/chrome-canary-gemini-change-bad-passwords-3698237), [GPM family sharing](https://www.techradar.com/pro/security/google-password-manager-now-lets-families-share-details-across-one-account)
- Bitwarden and Proton: [Bitwarden release notes](https://bitwarden.com/help/releasenotes/), [Access Intelligence](https://bitwarden.com/blog/measure-credential-risks-over-time-with-access-intelligence/), [clients releases](https://github.com/bitwarden/clients/releases), [Proton custom items](https://proton.me/support/create-custom-item), [Proton roadmap](https://proton.me/blog/pass-roadmap-spring-summer-2026), [Proton Sentinel](https://proton.me/blog/proton-updates/proton-pass), [Proton AMO reviews](https://update.mozilla.org/en-US/firefox/addon/proton-pass/reviews/?score=4), [Proton news (AlternativeTo)](https://alternativeto.net/software/proton-pass/news)
- Security research and incidents: [ETH/USI ePrint 2026/058](https://eprint.iacr.org/2026/058), [secure.com summary](https://www.secure.com/blog/researchers-expose-major-security-gaps-in-leading-password-managers), [NDSS 2025 IPC](https://www.ndss-symposium.org/?p=23650), [Krebs on LastPass thefts](https://krebsonsecurity.com/2025/03/feds-link-150m-cyberheist-to-2022-lastpass-hacks/)
- Standards and platforms: [RFC 8628](https://datatracker.ietf.org/doc/rfc8628/), [PlayStation passkeys](https://www.playstation.com/passkey/), [local-AI password app](https://feedbagel.com/post/password-app-automates-password-updates-using-local-ai-agents-on-mac)
