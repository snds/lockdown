# Gap Research: More Login Bridges for a Password Manager

*Research date: 2026-10-15. Companion to `03-vendor-bridges.md` — this file widens the lens from "emulate the vendor's authenticator app" to **every legitimate, standards-based or officially-supported way a password manager can drive a third-party service's own login**, with a focus on mechanisms that need **no reverse-engineering**. Sources dated inline. Where evidence is community/OSS-only or couldn't be verified first-hand it is flagged **[uncertain]**.*

### Tooling note (how these sources were gathered)
`WebFetch` failed with DNS/`ENOTFOUND` for most hosts in this environment, and the shared `WebSearch` budget for the turn was exhausted on the first call, so **no live web-search results were available**. Findings rest on: (a) `raw.githubusercontent.com` fetches (reachable through the proxy), (b) GitHub MCP code search, (c) shallow `git clone` of primary-source repos (Discord API docs, Microsoft `entra-docs`, `EpicResearch`, Bitwarden iOS, steamguard-cli), and (d) the `2factorauth/twofactorauth` + `2factorauth/passkeys` community directories read at their current `master`. Treat the 2fa.directory entries as crowd-maintained (good for "which methods exist," weak for exact launch dates). The prior file `03-vendor-bridges.md` was read in full first.

---

## 0. The one-paragraph reframing (carried forward and sharpened)

The owner's "log users in using the service's own system" is, cleanly separated, **four** capabilities, three of which are open standards a password manager is *invited* to implement:

1. **Device-code activation (RFC 8628)** — the "scan the TV QR / enter the code at `/activate`" flow. **The constrained device is the TV/console; the approving browser is the authenticator.** A password manager's legitimate role here is to *be the approving browser* — open the activation URL in its own secure in-app browser, autofill username + password + TOTP, and let the user approve. **No reverse-engineering. No private API.** This is the single biggest *new* finding versus the prior file, and it is the direct, legitimate answer to "approve the TV login."
2. **Passkey / FIDO2 credential provider** — covered in the prior file as rank #1; re-confirmed and extended below. The OS brokers the cross-device "scan QR, approve on phone" (hybrid) flow for you.
3. **Federated "Sign in with…" (OAuth/OIDC + Steam's OpenID 2.0)** — not an *auth* bridge but a **tracking + one-tap** feature: record which IdP each site uses, and drive the IdP sign-in through the manager's autofill.
4. **OTP delivery capture** (email magic links, email/SMS OTP) — legitimate only through sanctioned channels (IMAP/Gmail API with the user's own account, OS verification-code autofill). Platform OTP-redaction (Android 15+, iOS) has **narrowed** this since the prior file was written.

Everything that requires speaking a vendor's private mobile API (Blizzard push, Adobe Account Access push, Discord remote-auth, Apple "approve on trusted device") remains in the **do-not-build** bucket for a commercial product, for the ToS/DMCA/blast-radius reasons the prior file established. This file does not relitigate those; it adds the legitimate alternatives that make them unnecessary.

---

## 1. Device-code activation flows (RFC 8628) — the big new legitimate bridge

### 1.1 What the standard actually says (primary source)

**RFC 8628, OAuth 2.0 Device Authorization Grant** (IETF Standards Track, Aug 2019). The constrained device (TV, console, CLI) gets a `device_code` + `user_code` + `verification_uri`, and optionally `verification_uri_complete` (§3.3.1) which **embeds the code so it can be rendered as a QR** — "to carry the information needed to run the user interaction without further user input, for example, embedded within a QR code." The user visits that URI in a browser on a second, input-capable device, authenticates **with the IdP's own login page**, and approves; the device polls the token endpoint meanwhile.

Crucially for legitimacy: **the approving browser is doing nothing privileged.** It loads the vendor's real `verification_uri`, the user signs in normally, done. A password manager that opens that URL in its own secure webview and autofills is behaving exactly like the user's hand-typed login — it is *using the vendor's own system*, which is precisely the owner's stated goal.

> Source: RFC 8628 §3.3, §3.3.1, §5.4 (via `datatracker.ietf.org/doc/html/rfc8628`, referenced in `mukul975/Anthropic-Cybersecurity-Skills` SKILL and `MixV2/EpicResearch`; the RFC text host itself returned `ENOTFOUND`/proxy-403 in this environment, so §-level quotes are from the IETF doc as cited by those repos, **[verify exact §5.4 wording before publishing]**).

### 1.2 The real activation endpoints (confirmed from code in the wild)

| Service | Activation URL | Grant / evidence | Confirmed from |
|---|---|---|---|
| **Microsoft / Xbox** | `microsoft.com/link` | `urn:ietf:params:oauth:grant-type:device_code`; `verification_uri` defaults to `https://www.microsoft.com/link` | `mubaraknumann/unifideck` (`microsoft_device_auth.py`), `Likely7/...Account.cpp`, `xdzleo/xau-mobile` (`device_code.rs`) |
| **YouTube / Google** | `youtube.com/activate` | `oauth2.googleapis.com/device/code` → `user_code` + `verification_url`; falls back to `youtube.com/activate` | `anxdpanic/plugin.video.youtube`, `milika/SmartTubeIOS` (`AuthService.swift`), `cubicreates/UnboundMusic` |
| **Epic Games** | `epicgames.com/id/activate` (a.k.a. `epicgames.com/activate`) | `deviceAuthorization` endpoint → `verification_uri_complete`; `grant_type=device_code` against `account-public-service-prod` | `MixV2/EpicResearch` `docs/auth/grant_types/device_code.md` (primary) |
| **Discord** (console account-linking) | `discord.com/activate` | RFC 8628 device auth; `verification_uri_complete = discord.com/activate?user_code=…`; `grant_type=urn:ietf:params:oauth:grant-type:device_code` | `discord/discord-api-docs` `account-linking-on-consoles.mdx` (primary, official) |
| **Netflix** | `netflix.com/tv8` | TV activation code flow | `dcbolt/media-haven`, `she-lly/Firestick-TV` docs |
| **Disney+** | `disneyplus.com/begin` | TV activation code flow | same |
| **Amazon / Prime Video** | `amazon.com/code` (device registration) | device code | widely documented |
| **Steam** | `s.team/q/…` (QR) — **not** RFC 8628; Steam's own `IAuthenticationService` | see §4 and prior file | steamguard-cli |

Discord's console guide is worth quoting because it is an **official vendor doc** describing exactly this pattern: *"the user scans a QR code or enters a user code at `discord.com/activate`"* and the SDK *"automates all of the OAuth2 device authorization flow steps with a single call to `Client::GetTokenFromDevice`"* (`discord/discord-api-docs`, development-guides/account-linking-on-consoles.mdx).

### 1.3 Could the PM "scan the TV QR, open the activation URL, autofill + approve"? — Yes, with one hard safety rule

**Mechanically: yes, and legitimately.** Two sub-patterns:

- **(a) QR encodes `verification_uri_complete`** (code already embedded — Epic, Discord, many): the PM scans it, opens the URL in its **own secure, user-visible in-app browser**, autofills credentials + TOTP on the vendor's real page, user taps "Confirm this device." Nothing proprietary touched.
- **(b) QR/screen shows a short `user_code` + bare `verification_uri`** (Microsoft `microsoft.com/link`, Netflix `tv8`, YouTube `activate`): PM opens the URL, **pre-fills the user_code**, autofills login, user approves.

**The hard safety rule (this is the whole ballgame):** device-code flows are the single most-abused consumer OAuth pattern of 2025–2026. Microsoft's own Storm-2372 campaign (Feb 2025) and the Unit 42 "Device Code-based OAuth Phishing" note (2026-03-23) describe attackers who **present a victim with a device code and get them to approve the attacker's session** — "the attacker's server captures the resulting OAuth tokens in the background… long-term access to email, files, and identity" (`PaloAltoNetworks/Unit42-timely-threat-intel`). Microsoft Entra now treats device-code flow as **"a high-risk authentication method that can be part of a phishing attack"** and tells admins to **"block device code flow wherever possible"** (`entra-docs` concept-authentication-flows.md, ms.date 2026-03-24).

→ **Design consequence:** a PM may approve a device code **only when it, not a remote party, originated/possesses the physical QR** (scanned from a screen the user is looking at) — i.e. the proximity assumption of RFC 8628 §5.4 holds. It must **refuse to approve a `user_code` that arrived by message, email, link, or paste**, and must show the user *which device/app and which account* is being authorized before approving. Approving a code the user received rather than scanned *is* the Storm-2372 attack. This rule should be a first-class, non-overridable gate in the bridge, not a setting.

**Legitimacy / effort / value**

| Mechanism | Legitimacy | Effort | User value | Phase |
|---|---|---|---|---|
| Device-code **approve-by-scan** (QR encodes complete URI) | **Official / standards-based** (RFC 8628 as designed) | **Med** (secure webview + autofill + the proximity gate) | **High** — "point phone at TV, signed in" across Netflix, Disney+, YouTube, Xbox, Epic, Discord, Amazon | **v1.x flagship** |
| Device-code **pre-fill user_code** (bare URI + code) | Official / standards-based | Low–Med | High | v1.x, same workstream |

This is the mechanism that most literally delivers the owner's "approve the TV login" vision, and it is **clean** — provided the proximity/anti-phishing gate ships with it.

---

## 2. Passkey / FIDO2 credential provider — re-confirmed, with 2026 specifics

The prior file's rank #1 holds and strengthens. New confirmations from primary sources:

- **Third-party providers are first-class on-platform.** Microsoft's own docs now list **"1Password, Bitwarden"** as example passkey providers, with the platform minimums that matter for a PM: **iOS 17+** and **Android 14+** for third-party provider apps; Chrome extension on desktop (`entra-docs` how-to-synced-passkeys.md, ms.date 2026-07-05). Android < 14 and some Samsung builds remain gaps (same table) — matches the prior file's "degrade gracefully" caveat.
- **Synced passkeys are the consumer sweet spot.** Entra: synced passkeys see **"99% of users successfully register," "14× faster" than password+MFA, "3× more successful" sign-in** (how-to-synced-passkeys.md). They **don't support attestation**, which is exactly why some enterprises block them by AAGUID — but for a consumer PM that is the point.
- **AAGUID identity is public.** Each provider ships a 128-bit AAGUID; the community `passkeydeveloper/passkey-authenticator-aaguids` registry maps them to friendly names (its README explicitly names *"Google Password Manager, Dashlane, 1Password"* as examples). A new PM gets its own AAGUID and appears in RP management UIs like the incumbents. [Note: that registry warns it "could go away at any time" and may be zeroed out — don't hard-depend on it; use FIDO MDS for authoritative data.]
- **Hybrid/QR is brokered by the OS, never by you.** This confirms the prior file's "effectively NO, you don't implement caBLE yourself" finding: you register as a credential-provider extension and the OS runs the QR+BLE proximity exchange. The PM stores the syncable passkey; the platform does the cross-device dance.

**Which services on the owner's list are already passkey-enabled** (from `2factorauth/passkeys` at master):

| Service | Passkey entry | Doc / date evidence |
|---|---|---|
| **Blizzard / Battle.net** | `passwordless: allowed` | `support.blizzard.com/help/article/380922`; entry **added 2026-05-04** (PR #81) — **this is the 2026 proprietary-authenticator-adds-standards news the task asked for: Battle.net now has passkeys** |
| **Adobe** | `passwordless: allowed` | `helpx.adobe.com/.../secure-sign-in-with-passkey.html`; entry present, name-merged 2026-02-10 (PR #8536) |
| **PlayStation** | `passwordless: allowed` | `playstation.com/passkey/` |
| **Nintendo** | `passwordless: allowed` | `accounts.nintendo.com` support a_id/62531 |
| **Apple iCloud** | `mfa: allowed`, note: *"Passwordless authentication only works on Apple devices."* | Apple HT204915 |
| Microsoft / Google / X / GitHub | (not in the sampled files, but passkey support is established in the prior file) | — |

**Net:** being a passkey provider unlocks legitimate cross-device "approve on phone" for Blizzard, Adobe, PlayStation, Nintendo, Microsoft, Google, X, GitHub, … through **one** OS integration — and it's the *only* legitimate bridge for Adobe and (now) Battle.net, both of which added passkeys rather than opening their push protocols.

| Mechanism | Legitimacy | Effort | User value | Phase |
|---|---|---|---|---|
| Passkey/FIDO2 credential provider (iOS 17+/Android 14+/desktop ext) | **Official / standards** | High (one-time) | **Very high** — generalizes the whole vision | **v1 / v1.x centerpiece** |

---

## 3. "Sign in with…" tracking + one-tap (OAuth/OIDC + Steam OpenID 2.0)

This is not a second factor; it's a **login-method bridge**. Two legitimate things a PM can do:

### 3.1 Track which IdP each site uses, and drive it
A PM already stores "this site → password." Extend the record to **"this site → signs in via Google / Apple / Discord / Steam / Battle.net."** On visiting the site, surface a one-tap "continue with <IdP>" and autofill the IdP's own login if a session isn't live. 1Password's model is instructive: it records a **"sign-in method"** per item so the user isn't offered a password where the account is actually federated (seen in product docs and mirrored in community specs, e.g. `karatektus/pl_mail`, `DiluxOne/...` "Sign-in method" field). Pure UX + storage; **no protocol work, no ToS exposure.**

### 3.2 "Sign in through Steam" = OpenID 2.0 (not OAuth, not OIDC)
Valve's federation is **OpenID 2.0** against a single hardcoded endpoint `https://steamcommunity.com/openid/login` (`xPaw/SteamOpenID.php`, `src/SteamOpenID.php`). Key facts for correct, safe handling:
- The RP receives **only the SteamID64** (the `openid_identity` matches `^https://steamcommunity\.com/openid/id/(7656…)$`), nothing else — profile data needs a separate Web API call.
- Validation **must** hardcode `return_to` ("the client must not be able to choose it… it can be spoofed") and verify `openid_op_endpoint === steamcommunity.com/openid/login`, then round-trip `openid_mode=check_authentication`. These are the exact places naive OpenID RPs get popped.
- For a **password manager**, "Sign in through Steam" is relevant as a *tracked login method for sites that offer it* (autofill the Steam login, let Steam assert), **not** as something the PM itself implements as an RP.

**Battle.net** federation is standard **OAuth 2.0** via the official developer portal (`Blizzard/passport-bnet`: `oauth.battle.net`, region-scoped) — again, a login method to track and drive, not a bridge to build.

| Mechanism | Legitimacy | Effort | User value | Phase |
|---|---|---|---|---|
| Track IdP per site + one-tap federated autofill | **Official** (just uses vendor login) | Low | Med–High (removes "which button did I use?" friction, cuts phishing) | **v1** |
| Act as a Steam OpenID **relying party** in-product | Official, but niche for a PM | Low | Low (PM isn't an RP) | Skip / only if the PM has its own web account |

---

## 4. Steam Guard — Valve-tolerated QR approval + trade confirmations (deepened from source)

The prior file covered Steam as rank #3/#4. New **source-level** detail from reading `dyc3/steamguard-cli` (v0.18.4) so the PM team knows exactly what a Steam bridge entails:

- **QR-login approval is an HMAC signature over the challenge, not a password.** The client scans `https://s.team/q/<version>/<client_id>`, then calls `IAuthenticationService/UpdateAuthSessionWithMobileConfirmation` with a signature = `HMAC-SHA256(shared_secret, version ‖ client_id ‖ steam_id)` (`steamguard/src/approver.rs`, `build_signature`). So the **`shared_secret` alone** drives both TOTP codes *and* QR-login approval. The CLI subcommands are explicit: `qr-login` ("Log in to Steam on another device using the QR code that it's displaying") and `approve` ("Approve or deny pending login sessions," incl. a `--dangerously-approve-all`).
- **Trade/market confirmations need `identity_secret`.** Confirmations hit `steamcommunity.com/mobileconf/getlist` and `/ajaxop` with a hash generated from `identity_secret` + a tag + time (`steamguard/src/confirmation.rs`). This is the high-blast-radius secret the prior file flagged — a leak = silent inventory/market theft.
- **Secret storage, as a reference for the PM's own vault crypto.** steamguard-cli stores `maFile`s with **Argon2id (12 MB, t=3, p=12) → AES-256-CBC** (`src/encryption/argon2id_aes.rs`), with an optional OS-keyring passkey, and keeps a `LegacySdaCompatible` scheme for imports. The legacy SDA scheme it interops with is weaker: **PBKDF2 (50 000 iters) → AES-256-CBC (Rijndael)** (`Jessecar96/SteamDesktopAuthenticator` `FileEncryptor.cs`). Both CBC schemes are **unauthenticated** (no MAC/GCM) — if the PM ingests `maFile`s it should treat them as untrusted input and **re-encrypt into its own authenticated (AEAD) vault**, not preserve CBC. `SteamGuardAccount` carries `shared_secret`, `identity_secret`, `revocation_code`, `uri`, `secret_1`, `device_id` — tag the secret-class fields "high blast radius" per the prior file's principle.
- **Legitimacy unchanged:** Valve-tolerated, not licensed; mature OSS (steamguard-cli, SDA, NebulaAuth — the last explicitly "not affiliated with Valve or Steam," AGPL-3.0). NebulaAuth also advertises **auto-confirm** of trades/market and bulk `maFile` import. The 15-day trade-hold-on-move friction from the prior file still applies.
- **Abuse reality check:** a scanned Steam login QR *is itself* the takeover vector when shared — `banyourself/scamcam` rates `s.team/q/...` and Discord `discord.com/ra/...` QR codes as **critical "QR code login takeover"** precisely because "a … login QR code logs in whoever made it, so sharing one is the takeover itself." Same proximity gate as §1.3 applies: approve only self-scanned codes.

| Mechanism | Legitimacy | Effort | User value | Phase |
|---|---|---|---|---|
| Steam TOTP (5-char) | Official-via-enrollment | Low | High | v1 |
| Steam QR-login approve (`shared_secret` HMAC) | **Reverse-engineered, tolerated, mature OSS** | Med | High (trader/power-user) | v1.x opt-in, behind proximity gate |
| Steam trade confirmations (`identity_secret`) | Reverse-engineered, tolerated | Med | High (traders) | v1.x opt-in, high-blast-radius storage + re-auth to export |

---

## 5. Email magic links & email/SMS OTP capture — legitimate channels only, and narrowing

The owner floated "capture the code via IMAP/Gmail API or OS verification-code autofill." Each path has a 2025–2026 legitimacy/feasibility wall:

### 5.1 OS verification-code autofill — the clean path, but platform-gated
- **iOS**: Safari/AutoFill reads one-time codes from Messages and (since iOS 17) **can delete them after autofill**; the `autocomplete="one-time-code"` + **origin-bound SMS** format (`@example.com #123456`, the WICG `sms-one-time-codes` spec — Safari & Chrome positive) ties a code to the sending domain so it can't be autofilled on a phishing origin. A PM on iOS 18+ can register as an **AutoFill one-time-code provider** — Bitwarden already does: `ASOneTimeCodeCredentialRequest` / `prepareOneTimeCodeCredentialList` / `completeOneTimeCodeRequest`, gated `@available(iOS 18.0)` (`bitwarden/ios`, `CredentialProviderViewController.swift`). **This is the sanctioned API and the right target.**
- **Android**: **this got materially harder since the prior file.** Android 15+ redacts notification content that looks like a 2FA code from untrusted notification listeners (shows `"sensitive notification content hidden"`); only holders of **`RECEIVE_SENSITIVE_NOTIFICATIONS`** see the clear text, and that permission is `role`-gated — *not* grantable to an ordinary app. The documented "watch profile" workaround (grant the `COMPANION_DEVICE_WATCH` role to inherit the permission) is a **Play-policy-risky, dishonest-consent hack — NOT shipped** per independent research (`geekflyer/cliprelay` `docs/research/otp-relay-research.md`). Android 17 formalizes an OTP-SMS trust list (`isAppTrustedForSmsOtp`, `OP_READ_OTP_SMS`) limited to SMS/assistant/dialer roles (`novum-d/android-platform-research`). **Takeaway:** use the legit SMS Retriever / autofill APIs; do **not** scrape notifications. Email OTP on Android is effectively closed to third-party listeners.

### 5.2 IMAP / Gmail API email-OTP & magic-link capture — allowed but heavy
- Reading the user's own mailbox for a code is legitimate **with the user's own account/consent**, but Gmail's `gmail.readonly` / `gmail.modify` / `mail.google.com` are **Restricted Scopes**: *"If you store restricted scope data on servers (or transmit), then you must go through a security assessment"* (Google policy, quoted in `langflow-ai/langflow`, `kirodotdev/KiroCrew` OAuth-registration docs). That means **CASA Tier-2 assessment + annual recertification** for a published external app that touches Gmail server-side. **Architecture consequence:** do it **on-device only** (client reads IMAP/Gmail, extracts the code locally, nothing transits your servers) to stay out of CASA scope, or restrict to IMAP with app-passwords the user supplies. Magic links are worse to automate: "click this link" is a full session grant, so a PM auto-clicking it is approving a login — same proximity/consent gate as §1.3, and generally **surface the link to the user rather than auto-follow it.**

| Mechanism | Legitimacy | Effort | User value | Phase |
|---|---|---|---|---|
| iOS AutoFill one-time-code provider (+ origin-bound SMS) | **Official** | Low–Med | High (the expected iOS behavior) | v1 |
| Android SMS Retriever / autofill (no notif scraping) | Official | Med | Med (Android OTP UX is platform-limited) | v1 |
| On-device IMAP/Gmail OTP extraction (no server transit) | Official-ish; avoids CASA if on-device | Med–High | Med | v2, on-device only |
| Server-side Gmail OTP reading | Official **but CASA + annual audit** | High + ongoing | Med | **Avoid** unless already doing Gmail server-side |
| Auto-follow email magic links | Legit only with explicit per-use consent | Low | Low–Med (risky) | Surface to user; don't auto-approve |

---

## 6. Official vendor authenticator APIs (Entra OATH, Okta, Duo, Auth0) — mostly enterprise, mostly not for a consumer PM

The task asked whether official APIs let a PM act as a sanctioned authenticator. Findings:

- **Microsoft Entra — third-party OATH TOTP: yes, sanctioned.** *"Third-party applications that use OATH TOTP to generate codes can also be used"* (`entra-docs` concept-authentication-oath-tokens.md, ms.date 2025-03-04). So a PM generating RFC 6238 codes is explicitly allowed as a **software OATH token** for Entra users. The **hardware OATH token** path (CSV/Graph-provisioned seeds, now with end-user self-assign in the "preview refresh") is **still Preview, no GA date** — don't build on it yet. **Entra push/number-matching remains proprietary** (Authenticator-only, number match forced since 2023-05-08) — not open.
- **Entra External Authentication Methods (EAM):** an external provider can satisfy Entra MFA **over OIDC** (`concept-authentication-external-method-provider.md`, ms.date 2026-02-23) — but this is for **IdP/MFA vendors a tenant admin onboards**, not a consumer password manager. Not a consumer bridge.
- **Okta / Auth0 / Duo custom authenticators: build-your-own-app SDKs, tenant-scoped, push-only.** `okta/okta-devices-swift` lets *an app your org configures* do APNs push verification ("create a custom authenticator on your Okta service," iOS-only, no TOTP). `auth0/Guardian.swift` is the same shape for Auth0 tenants (RSA-keyed push enroll, no TOTP). **Okta Verify TOTP is explicitly not retrievable by third parties** (prior file). **Implication:** these are legitimate only if the PM operates as *that organization's chosen authenticator vendor* — an enterprise go-to-market, not a consumer feature. For consumer Okta/Duo-protected accounts, the only open bridge is **TOTP where the org allows it, else passkey/FIDO2**.

| Mechanism | Legitimacy | Effort | User value | Phase |
|---|---|---|---|---|
| Entra 3rd-party **software OATH TOTP** | **Official** | Low (already TOTP) | Med (enterprise users) | v1 (falls out of TOTP) |
| Entra hardware-OATH via Graph | Official but **Preview** | High | Low (enterprise admin) | Watch; don't build |
| Okta/Auth0/Duo custom push authenticator SDK | Official **but tenant-onboarded** | High + per-tenant | Low for consumer PM | Enterprise track only |
| Entra/Google/Okta proprietary push | Closed | — | — | Skip → use passkey/TOTP |

---

## 7. 2025–2026 proprietary-authenticator → standards movement (the news scan)

Direct evidence gathered this session, newest signal first:

- **Battle.net added passkeys** — `2factorauth/passkeys` entry created **2026-05-04** citing `support.blizzard.com/help/article/380922`. This is the headline: Blizzard, whose proprietary authenticator the prior file called a reverse-engineering minefield, now has a **standards path**. Build the passkey bridge, not a Blizzard-push bridge. **[date is the directory-entry date; confirm Blizzard's own launch date before quoting a "since" date]**
- **Adobe** — passkeys live (`helpx.adobe.com/.../secure-sign-in-with-passkey.html`), directory entry housekeeping 2026-02-10. Confirms prior file: Adobe's bridge is passkeys, **not** Account Access push. Third-party TOTP remains unsupported/hidden per prior file's user reports.
- **Epic Games** — directory entry updated **2026-01-11** (PR #8530): now lists `totp` **and** `custom-software` ("Epic Games app", "Epic Games Store app") across epicgames/fortnite/fallguys/**rocketleague**. Epic has **standard TOTP** + its own app; no need for a proprietary bridge. Its console flow is RFC 8628 (§1).
- **Riot Games** — `2factorauth` entry lists **`email` only** (no TOTP), last meaningful touch 2022/2023. **Riot has not adopted standard TOTP or passkeys** in the sampled data — a genuine gap; the only legitimate "bridge" is autofilling Riot's own login/email-OTP. **[uncertain — verify against Riot's current security page; crowd data may lag]**
- **EA** — `ea.com` lists `sms, email, call, totp`: standard TOTP available; no proprietary bridge needed.
- **PlayStation / Nintendo** — passkeys confirmed (`playstation.com/passkey/`, Nintendo a_id/62531) + TOTP (Nintendo `accounts.nintendo.com` lists `totp`).
- **No evidence** surfaced this session of Discord, Riot, or EA shipping a *new proprietary* authenticator in 2025–26; the direction is uniformly **toward passkeys/TOTP**, which validates the "lean on standards" thesis.

---

## 8. Consolidated bridge catalog

Legitimacy legend: **Official** (vendor-sanctioned/standards) · **Tolerated** (OSS-normalized, not licensed) · **Reverse-eng** · **ToS-violating/closed**.

| # | Service / mechanism | Mechanism (one line) | Legitimacy | Effort | User value | Phase |
|---|---|---|---|---|---|---|
| 1 | **Passkey/FIDO2 credential provider** | OS brokers syncable passkeys + hybrid QR; covers Blizzard, Adobe, PSN, Nintendo, MS, Google, X, GitHub | **Official** | High (1×) | Very high | **v1 centerpiece** |
| 2 | **Standard TOTP (RFC 6238)** | Store seed, generate code — Epic, EA, Ubisoft, Roblox, Rockstar, Nintendo, Amazon, Activision, Entra software-OATH | **Official** | Low | High | **v1** |
| 3 | **Device-code approve-by-scan (RFC 8628)** | Scan TV/console QR → open real `verification_uri` in secure webview → autofill+approve — Netflix `tv8`, Disney+ `begin`, YouTube `activate`, Xbox `microsoft.com/link`, Epic `activate`, Discord `activate`, Amazon | **Official/standards** | Med | **High** (the "approve the TV" vision) | **v1.x flagship** + §1.3 proximity gate |
| 4 | **Steam TOTP (5-char)** | Non-standard alphabet TOTP via enrollment | Official-via-enrollment | Low–Med | High | v1 |
| 5 | **Federated login tracking + one-tap** | Record site→IdP (Google/Apple/Discord/Steam-OpenID/Battle.net-OAuth), drive the IdP's own login | **Official** | Low | Med–High | v1 |
| 6 | **iOS AutoFill one-time-code provider** | `ASOneTimeCode*` + origin-bound SMS; Bitwarden-style | **Official** | Low–Med | High (expected on iOS) | v1 |
| 7 | **Android SMS Retriever / autofill** | Sanctioned SMS OTP read; **no notification scraping** | Official | Med | Med (platform-limited) | v1 |
| 8 | **Steam QR-login approve** | `shared_secret` HMAC over `s.team/q` challenge | **Reverse-eng, tolerated (mature OSS)** | Med | High (power-user) | v1.x opt-in, proximity gate |
| 9 | **Steam trade/market confirmations** | `identity_secret` over `mobileconf` | Reverse-eng, tolerated | Med | High (traders) | v1.x opt-in, high-blast-radius storage |
| 10 | **On-device email OTP extraction** | Local IMAP/Gmail read, code parsed on device, no server transit (avoids Gmail CASA) | Official-ish | Med–High | Med | v2, on-device only |
| 11 | Battle.net **TOTP import** | serial+restore → TOTP; fragile on merged app | Reverse-eng, fragile | Med | Low–Med | Only if cheap; **prefer #1 passkey** |
| 12 | Entra hardware-OATH via Graph | Admin/self-assign seed provisioning | Official **Preview** | High | Low | Watch; don't build |
| 13 | Okta/Auth0/Duo custom push SDK | Be a tenant's chosen push authenticator | Official **tenant-onboarded** | High | Low (consumer) | Enterprise track only |
| — | Server-side Gmail OTP reading | Restricted scope | Official but **CASA + annual audit** | High+ongoing | Med | **Avoid** |
| — | Auto-follow email magic links | Auto-clicking = granting a session | Risky even if legal | Low | Low–Med | Surface link; don't auto-approve |
| — | Blizzard push / Adobe Account Access push / Discord remote-auth / Apple "approve on device" / Entra·Google·Okta·Duo push | Private mobile API emulation | **Closed / ToS-violating** | — | — | **Decline** → superseded by #1/#2/#3 |

---

## 9. Recommended sequencing (what to build, in order)

1. **v1:** TOTP (#2, incl. Steam 5-char #4 and Entra software-OATH), passkey provider foundation (#1), iOS one-time-code autofill (#6), Android SMS Retriever (#7), federated-login tracking (#5). These are all Official and cover the bulk of the owner's named services with zero ToS exposure.
2. **v1.x flagship:** **Device-code approve-by-scan (#3)** — this is the new, legitimate, literal answer to "approve the TV/console login," spanning streaming + consoles. Ship it **with the §1.3 anti-phishing proximity gate as a hard, non-overridable rule** (approve only self-scanned QR, never a received/pasted code; show account+app before approving). Finish the passkey hybrid/QR UX in the same train (#1 end-to-end).
3. **v1.x opt-in, fenced:** Steam full bridge (#8, #9) behind explicit warnings (15-day hold, `identity_secret` blast radius), re-encrypted into the PM's own AEAD vault.
4. **v2 / on-device-only:** email OTP extraction (#10) without server transit to dodge Gmail CASA.
5. **Decline publicly:** all proprietary-push emulation; frame it as "we never put your accounts at ToS/ban risk — we use each service's own, supported login."

**Architecture:** keep the prior file's capability-tiered design (standards engines always-on + quarantined, signed, opt-in proprietary "bridge" plugins). **Add** a shared **"approval" capability** with the proximity/anti-phishing gate baked in, because device-code (#3), passkey hybrid (#1), and Steam QR (#8) all converge on the same risky primitive — "approve a login I'm being shown a code/QR for." Centralizing that gate is the highest-leverage safety decision in the whole bridge layer.

---

## 10. Key uncertainties to resolve before committing

- **RFC 8628 §5.4 exact wording** — the RFC host returned DNS/proxy errors this session; the phishing-mitigation text should be quoted from the primary RFC before publishing.
- **Battle.net passkey launch date** — directory entry is dated 2026-05-04; Blizzard's own "since" date is unconfirmed. Same for exact platform coverage.
- **Riot Games 2FA state** — crowd data shows `email` only (no TOTP/passkey); verify against Riot's current security page, as the directory may lag.
- **Per-vendor device-code ToS** — RFC 8628 is a standard, but each vendor's *developer terms* govern programmatic use of *their* device endpoint; a PM acting as the approving browser (loading the vendor's real page) is low-risk, but automated token exchange against, e.g., Epic's `account-public-service` would use undocumented client IDs and is **not** sanctioned. Keep the PM on the *browser-approval* side of the flow, not the *constrained-device/token-exchange* side.
- **iOS 18 one-time-code provider availability** confirmed in Bitwarden source (`@available(iOS 18.0)`); confirm current minimum-OS economics for the target user base.
- **Steam `maFile` CBC ingestion** — re-encrypt to AEAD on import; never persist the legacy unauthenticated CBC blobs.

**Bottom line:** the owner's "use the service's own login" vision is not only achievable without reverse-engineering — in 2026 the legitimate path is *richer* than it was: **passkeys** now cover Battle.net and Adobe (which previously looked like reverse-eng targets), and **RFC 8628 device-code approve-by-scan** delivers the exact "point my phone at the TV and I'm signed in" experience for Netflix, Disney+, YouTube, Xbox, Epic, Discord and Amazon through the vendors' own pages. Build those two, gate the shared "approve a shown code" primitive hard against device-code phishing, keep Steam as the one fenced tolerated-proprietary bridge, and decline every private-push emulation.
