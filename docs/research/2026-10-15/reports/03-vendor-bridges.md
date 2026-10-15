# Vendor Login Bridges for a Password Manager: Feasibility, Mechanism, Risk & ToS

*Research date: 2026-10-15. Sources dated inline; several rest on community/forum evidence and are flagged as uncertain. This is a product feasibility and risk assessment, not an implementation guide — the "how exactly to extract a secret" detail is deliberately left at the architectural level, because the legal/security verdict matters more than the mechanics for a go/no-go decision.*

## Framing: three distinct capabilities are being conflated

The owner's pitch ("log users in using the vendor's own system, replace their authenticator app") actually bundles three technically unrelated things, and separating them is the single most important finding:

1. **TOTP code generation** — the password manager stores a shared secret and generates a 6-digit (or Steam's 5-char) code. This is RFC 6238, well-understood, and *already shipped by Bitwarden, Aegis, KeePassXC, 2FAS, Ente* etc. Low risk where the secret is obtained through the vendor's own enrollment QR.
2. **Proprietary push/approval + QR-login approval** — acting *as the vendor's phone app* to approve a "someone is signing in, approve?" prompt or a scanned QR. This requires speaking the vendor's private mobile API (Steam's `IAuthenticationService`, Blizzard push, Adobe Account Access, Discord remote-auth). Almost always reverse-engineered, almost always a ToS violation, and the secret material is far more sensitive than a TOTP seed.
3. **Standards-based cross-device sign-in** — FIDO2 hybrid transport (passkey-over-QR) and OAuth 2.0 Device Authorization Grant (RFC 8628) / CIBA. These are *open standards* that do the "scan to log in" UX natively, but — crucial finding — the phone/authenticator role in FIDO2 hybrid is reserved to the OS platform, not available to a third-party app.

The strategic conclusion up front: **a new password manager can credibly ship (1) broadly and lean hard into (3) as the future, but (2) is a legal and security minefield that should be avoided except possibly for a single flagship case (Steam) where the OSS ecosystem has already normalized it.**

---

## 1. Steam — Steam Guard Mobile Authenticator

**Mechanism.** Steam Guard is *not* standard TOTP. It uses the TOTP time-step and HMAC-SHA1 construction but maps the truncated value into a **5-character alphabet (digits + a subset of letters)** instead of 6 digits — "Steam OTP is not spec compliant, it uses both characters and numbers" ([Techlore forum](https://discuss.techlore.tech/t/steam-2fa-on-linux/630); [Wikipedia Comparison of OTP applications](https://en.wikipedia.org/wiki/Comparison_of_OTP_applications)). The account's authenticator holds two secrets: a **`shared_secret`** (generates login codes) and an **`identity_secret`** (signs/accepts trade and market confirmations), plus a `deviceid`. A confirmation call "needs the device ID and identity secret together" ([steam.readthedocs.io](https://steam.readthedocs.io/en/stable/api/steam.guard.html); [Steam-Card-Bot-PRO wiki](https://github.com/Refloow/Steam-Card-Bot-PRO/wiki/Shared-Identity-Secret-Codes-Guide)).

**QR login.** Newer Steam clients show a QR at `s.team/q/...`. The documented flow (via Doctor McKay's `steam-session` library) is: client calls `IAuthenticationService/BeginAuthSessionViaQR` → gets a challenge URL encoded in the QR → the *mobile authenticator* calls `UpdateAuthSessionWithMobileConfirmation` to approve. "steam-session can approve QR login attempts, but not standard password logins… 'you've scanned the code! Now just approve the login'" ([dev.doctormckay.com 5350](https://dev.doctormckay.com/topic/5350-listening-for-authentication-confirmations/), [5630](https://dev.doctormckay.com/topic/5630-problem-with-logging-in-via-qr-code/)). So QR-login approval is a real, reverse-engineered-but-stable API surface.

**How a third party gets the secrets.** Two documented paths: (a) *enroll* the authenticator itself via `ITwoFactorService/AddAuthenticator` — your app becomes the authenticator of record, receiving `shared_secret`/`identity_secret` directly; (b) *import* an existing `maFile` from Steam Desktop Authenticator (SDA), where the secret sits in the `uri` field (between `secret=` and `&issuer=Steam`) and must be read with SDA encryption temporarily off ([Steam-Card-Bot-PRO wiki](https://github.com/Refloow/Steam-Card-Bot-PRO/wiki/Shared-Identity-Secret-Codes-Guide)). Path (a) is what gives full QR + trade-confirmation power; path (b) only works if the user already set up SDA.

**The trade-hold catch (important UX/business finding).** Since December 2015, any account that *moves* its mobile authenticator triggers a **15-day trade/market hold** ("resetting the authenticator later would… cause the account to have 15 days trade ban penalty"; [dev.doctormckay.com](https://dev.doctormckay.com/topic/761-copying-mafile-format-for-multiple-accounts/)). So if the user removes Steam's official app to adopt your manager, they eat a 15-day hold on trading/market — a real adoption friction for the exact power-user (traders) who care most about `identity_secret`.

**OSS implementations (mature).**
- **steamguard-cli** (maintainer **dyc3**) and its underlying **`steamguard` Rust crate** — "the library used by steamguard-cli to do all the steamguard related things, such as generating 2FA codes and responding to confirmations" ([rust-digger](https://rust-digger.code-maven.com/crates/steamguard); [Ente migration guide](https://ente.com/help/auth/migration/steam/)). It can add an authenticator, generate codes, and answer confirmations — i.e. it already implements capability (2) for Steam.
- **SteamDesktopAuthenticator** (Windows, the `maFile` origin), **NebulaAuth**, **winauth/SteamAuth** ([github.com/winauth/SteamAuth](https://github.com/winauth/SteamAuth)).
- **TOTP-only consumers:** Bitwarden (`steam://YOURKEY` key prefix — [Bitwarden community](https://community.bitwarden.com/t/steam-totp-support/988)), KeePassXC ("Steam Key" TOTP type), Aegis, Ente, 2FAuth ([docs.2fauth.app](https://docs.2fauth.app/usage/steam-guard.md)). 2FAS support is **unconfirmed** in sources.

**Valve ToS / legal.** The Steam Subscriber Agreement prohibits use of unauthorized third-party programs that interact with Steam, and the Web API has no sanctioned "be the authenticator" grant. That said, Valve has tolerated the SDA/steamguard-cli ecosystem for ~8 years without enforcement against the tools themselves — a *de facto* tolerance, not a license. No DMCA actions against these projects are known. Risk is primarily **account action** (not legal) and is historically low but non-zero.

**Security risk.** `identity_secret` is arguably *more* sensitive than the password: it silently auto-confirms trades and Market listings, so a compromise = silent inventory theft with no further prompt. Storing it raises the blast radius of a vault breach well above a plain TOTP seed.

**Verdict: Supported via reverse-engineered protocol (mature ecosystem).** TOTP-only = trivial and safe. Full QR+confirmation bridge = feasible, well-trodden, but inherits the 15-day hold friction and `identity_secret` liability. **Recommended: ship Steam TOTP day one; offer the full bridge as an opt-in "advanced/trader" mode with explicit warnings.**

---

## 2. Battle.net (Blizzard) Authenticator

**Mechanism.** Also *not* plain TOTP — "Blizzard uses a proprietary algorithm… accounts must be converted to standard TOTP" ([Stratum/stratumauth wiki](https://stratumauth.com/wiki/import-from-blizzard-authenticator)). Historically the device had a **serial + restore code**; the secret is derived during enrollment and is not directly readable.

**Extraction paths.**
- *Legacy:* **WinAuth** (open source, "RFC 6238 authenticators… restore features for supported authenticators" incl. Battle.net — [github.com/djraw/winauth](https://github.com/djraw/winauth)) and the Python **`bna`** library ([pypi.org/project/bna](https://pypi.org/project/bna/4.0.1)). `bna` notes it's "impossible (without knowing the secret) to import an existing authenticator," so the serial+restore-code round-trip through Blizzard's security page is how the secret is obtained. WinAuth's maintainer has declared the project **end-of-life**, and Stratum/other tools have inherited the technique.
- *2023+ app-merge problem (flagged uncertain):* After Blizzard folded the standalone authenticator into the Battle.net mobile app, users report **no UI to enter serial/restore codes on a new device** ("Rather crucial when changing to a new device"; [us.forums.blizzard.com](https://us.forums.blizzard.com/en/wow/t/battle-net-app-and-the-authenticator/2130966)). The community workaround described in various tools is an **SSO-token → authenticator-API** flow to obtain the device secret, but I could not confirm a current, maintained public implementation of that method; treat as **uncertain / possibly broken** as of 2026.

**Push approval.** Blizzard's "Approve this login" push is a *separate* channel from TOTP. I found **no evidence of a reverse-engineered Blizzard push protocol** in public OSS. The community effort is entirely on the TOTP-secret side.

**ToS.** Blizzard EULA prohibits unauthorized tools interacting with its services; extracting the authenticator secret via automated calls is squarely against ToS. Enforcement against individuals using WinAuth-style imports has been minimal historically, but Blizzard is more aggressive about automation than Valve.

**Verdict: Import TOTP only (and even that is fragile on current app versions); push = Not feasible.** Recommend: support Battle.net as a TOTP entry *if* the user can still obtain serial+restore (older accounts), but do **not** build around it — the app merge may have closed the door, and you'd be shipping a feature that breaks for new users. Do not attempt push.

---

## 3. Adobe — Adobe Account Access

**Mechanism.** Adobe's "Account Access" app does push approvals + codes via Adobe's private API. There is **no public protocol** and no OSS replication of the push flow found.

**The good news (this reframes the whole question).** Adobe shipped **passkey support** for Adobe accounts — the [helpx passkeys overview](https://helpx.adobe.com/account/individual/sign-in-and-security/sign-in-methods/passkeys-overview.html) (page last updated **2026-02-25**) describes passkeys as "a modern, more secure alternative to passwords," using the device's built-in authenticators, and an Aug-2025 user report says "Adobe Accounts support FIDO2… save the credential in a password manager such as Bitwarden" ([community.adobe.com](https://community.adobe.com/t5/creative-cloud-desktop-ideas/third-party-authenticator-apps/idi-p/13940315)). **Standard third-party TOTP is apparently unsupported or hidden** — a long-running feature request remains open, and a Jan-2024 user claimed TOTP is "technically implemented but hidden" (unverified; same thread). A Sep-2025 user canceled a paid plan over "removal of third-party TOTP."

**Verdict: Supported natively via passkeys — do NOT replicate Account Access push.** The right bridge for Adobe is to be a **WebAuthn/passkey provider** (capability 3), which a modern password manager already should be. Replicating the proprietary push would be high-effort, high-risk, and pointless given the passkey path. **Recommended approach: Passkey/FIDO2 provider; skip proprietary push entirely.**

---

## 4. Survey of other proprietary authenticators & login-approval systems

| Service | Approval mechanism | Open standard alternative? | OSS reverse-eng? | Recommended bridge |
|---|---|---|---|---|
| **Microsoft** (Authenticator push + number-matching) | Proprietary push; number matching forced since 2023-05-08, "users cannot disable" ([learn.microsoft.com](https://learn.microsoft.com/entra/identity/authentication/how-to-mfa-number-match)) | **Yes** — MS accounts support **passkeys** + the account can use standard TOTP for MFA | Push not publicly reversed | **Passkey + TOTP import** |
| **Google** (Google prompts) | Proprietary push | **Yes** — passkeys + standard TOTP | No | **Passkey + TOTP** |
| **Discord** (QR remote-auth login) | WebSocket **remote-auth gateway**, RSA key exchange, encrypted ticket → token | No open standard; QR is Discord-proprietary | **Yes** — [Discord-QR-Auth-Client](https://github.com/malmeloo/Discord-QR-Auth-Client), agent-messenger docs. Well-documented, ToS-violating, token-handling risk | **Not recommended** (ToS; also TOTP already supported for 2FA) |
| **Epic Games** | email/SMS/**authenticator app** ([epicgames.com help](https://www.epicgames.com/help/c-34254770/a10075388)) | **Yes — standard TOTP** | n/a | **TOTP import** |
| **EA / Ubisoft / Riot / Rockstar / Twitch** | app-based 2FA; mostly standard TOTP (Ubisoft via website; Twitch uses TOTP) — *some unconfirmed in sources* | **Yes (TOTP) for most** | n/a | **TOTP import** |
| **Nintendo** | **Passkeys** since fall 2023 (iOS16+/Android9+, up to 10 per account; [channelnews](https://www.channelnews.com.au/nintendo-welcomes-password-less-sign-ins/), [corbado](https://www.corbado.com/blog/nintendo-passkeys)) + TOTP | **Yes** | n/a | **Passkey + TOTP** |
| **PlayStation** | **Passkeys** since 2024-02-21 (PS5/PS4/web/mobile; [playstation.com/passkey](https://www.playstation.com/passkey/), [FIDO Alliance](https://fidoalliance.org/the-verge-now-you-can-sign-into-your-playstation-account-without-a-password/)) | **Yes** | n/a | **Passkey** |
| **Xbox / Microsoft** | see Microsoft row | Yes | — | Passkey + TOTP |
| **GOG** | historically email-only verify ([TNW 2019](https://thenextweb.com/gaming/2019/02/19/two-factor-authentication-pc-gaming/)) — **uncertain** today | Unclear | n/a | TOTP if offered |
| **Roblox** | standard authenticator-app 2FA | **Yes — TOTP** | n/a | TOTP import |
| **Duo Mobile** | **Duo Push** (push-only, proprietary); Duo Mobile *also* generates standard TOTP for any service ([levelup.duo.com](https://levelup.duo.com/duo-mobile-for-third-party-accounts)). But Duo push itself "only available with Duo Mobile" | TOTP yes; push no | No sanctioned push API | TOTP only; push not feasible |
| **Okta Verify** | Okta push; "**public API not available to retrieve the TOTP code**" and Okta Verify not offered as generic third-party option ([support.okta.com](https://support.okta.com/help/s/article/public-api-not-available-for-retrieving-okta-verify-totp-codes)) | FIDO2/WebAuthn (Okta supports passkeys) | No | **Passkey where org allows; otherwise none** |
| **Yubico/YubiKey** | hardware FIDO2/OATH | **Yes — it IS the standard** (CTAP2) | n/a | Be a passkey provider / OATH-TOTP store |
| **Twitter/X** | SMS/TOTP/security key | **Yes — TOTP + FIDO2** | n/a | TOTP + passkey |
| **WhatsApp/Telegram/Signal** (device-link QR) | proprietary E2EE device-linking QR (Noise/Signal protocol); linking = provisioning a new *messaging* device, not an auth factor | No general standard | Partially (whatsmeow, tdlib) | **Out of scope** — these link a chat client, not "log into an account"; wrong abstraction for a password manager |
| **Apple ID** ("Approve sign-in" on trusted device) | proprietary; tied to Apple hardware + iCloud trust | **Passkeys** (Apple accounts) | No (and emulating a "trusted Apple device" is infeasible) | **Passkey only** |

**Pattern:** every proprietary push/QR system *either* has a standards-based alternative (passkey or TOTP) *or* is a reverse-engineering/ToS problem. The services worth special-casing with a proprietary bridge are vanishingly few — Steam is the main one, because its 5-char TOTP is non-standard *and* its trade-confirmation layer has real user value, *and* a mature OSS ecosystem already exists.

---

## 5. Generalizing "login by scanning a QR": the standards

### FIDO2 hybrid transport (formerly caBLE) — the key architectural finding

Hybrid transport is exactly "scan a QR on your laptop, approve on your phone, phone is the authenticator." The W3C renamed caBLE → "hybrid," and FIDO is now standardizing the **Proximity Exchange Protocol** ([FIDO Alliance working draft 2026-07-17](https://fidoalliance.org/specs/hybrid/proximity-exchange-protocol-v1.0-wd-20260717.html)). It uses a **BLE advertisement for proof of proximity** plus a tunnel-service data channel.

**Can a third-party password manager be the phone authenticator for hybrid? Finding: effectively NO, you don't implement it yourself — and that's by design.** Per the [Apple Developer Forums thread 829541](https://developer.apple.com/forums/thread/829541), (a) an app **cannot emit the CTAP hybrid BLE advertisement itself** (`CBPeripheralManager.startAdvertising` won't emit the raw bytes), and (b) a reply states that **"all enabled credential provider extensions get Hybrid support through the system's implementation, so there is no need to implement it yourself."** (Reply author not confirmed as Apple staff — *flagged*.) Also: **device-bound credentials can't go over hybrid; keys must be syncable/backup-eligible** ([Thales iOS hybrid docs](https://thalesdocs.com/oip/fido2-sdk/fido2-sdk-hybrid-transport/fido2-sdk-hybrid-transport-ios/index.html)).

So the model is: **you register as an iOS/Android credential-provider extension (iOS 17+/Android 14+), store syncable passkeys, and the OS brokers the hybrid/QR flow for you.** You get the "scan QR on another device, approve on my phone" UX *for free* for any site using WebAuthn — but only through the platform credential-manager API, never by speaking caBLE directly. Caveat: third-party credential-provider support is uneven on some Android OEM builds (RSA notes some Android devices lacked it as of late 2024; [community.securid.com](https://community.securid.com/s/article/Availability-of-Passkey-Feature-in-RSA-Authenticator-4-5-for-iOS-and-Android)).

**This is the single biggest strategic lever:** being a passkey credential provider gives the "QR login approval" experience for *the entire passkey-adopting web* (Adobe, Microsoft, Google, Nintendo, PlayStation, X, GitHub, …) through one supported integration — no per-vendor reverse engineering.

### OAuth 2.0 Device Authorization Grant (RFC 8628) & CIBA

- **RFC 8628** (IETF Standards Track, Aug 2019; [datatracker](https://datatracker.ietf.org/doc/rfc8628/)): a constrained device shows a user-code; the user approves in a browser on another device. User-initiated, device-shows-code. Relevant if *your product* is the constrained device; not a "be the phone" mechanism.
- **CIBA** (OpenID Connect Client-Initiated Backchannel Authentication; [Ping](https://docs.pingidentity.com/pingam/7.2/oidc1-guide/openid-connect-backchannel-request-flow.html), [Cloudentity](https://docs.authorization.cloudentity.com/2.0/features/oauth/grant_flows/ciba/overview/)): a backend initiates, the IdP **pushes an approval to the user's registered authentication device** (poll/ping/push token delivery). CIBA is the *standardized* version of "proprietary push approval" — but it requires the relying party and IdP to both implement it, and the authenticator is still the IdP's registered device/app. A password manager could be a CIBA **authentication device** only where an IdP explicitly enrolls it; no consumer vendor on this list exposes that. **Useful to know, not actionable for consumer bridges today.**

---

## Cross-cutting risk summary

- **Legal/anti-circumvention (DMCA §1201):** Reverse-engineering a *login-approval* protocol to interoperate is legally grey. US DMCA has a security-research/interoperability posture, but emulating an authenticator to bypass a technical access control is the kind of thing §1201 was written about. Shipping it *commercially* raises exposure far above a hobby OSS project. **The TOTP and passkey paths carry essentially no anti-circumvention risk** (you're using the vendor's own enrollment); the proprietary-push/QR paths carry the most.
- **Account bans:** Users adopting a reverse-engineered bridge risk vendor account action (Blizzard > Discord > Steam in likely aggressiveness). Your product shouldn't put users' primary accounts at risk silently.
- **Security blast radius:** storing `identity_secret`, Blizzard device secrets, or Discord tokens turns your vault into a higher-value target than a TOTP-only store. A vault breach becomes silent trade theft / account takeover, not just a stealable second factor. This *raises your own liability and threat model*.
- **Maintenance fragility:** reverse-engineered APIs break without notice (Battle.net app-merge is the cautionary tale — the serial/restore UI vanished). Each proprietary bridge is an ongoing maintenance tax.

---

## Ranked recommendation: which bridges to ship first

| Rank | Bridge | Classification | Effort | Risk | Why |
|---|---|---|---|---|---|
| **1** | **Passkey / FIDO2 credential provider** (iOS 17+/Android 14+/desktop) | Supported natively | High (one-time) | Low | Unlocks QR/hybrid "approve on phone" for the *entire* passkey web — Adobe, Microsoft, Google, Nintendo, PlayStation, X, GitHub. The real answer to the owner's vision. |
| **2** | **Standard TOTP (RFC 6238) import/generate** | Supported natively | Low | Low | Covers Epic, Riot, EA, Ubisoft, Roblox, Twitch, Microsoft, Google, X, and Duo-as-TOTP. Table stakes for any password manager. |
| **3** | **Steam (TOTP, `steam://` 5-char variant)** | Supported natively via enrollment | Low–Med | Low | Non-standard alphabet needs special code; huge user demand; precedent in Bitwarden/Aegis/KeePassXC. |
| **4** | **Steam full bridge (QR login + trade confirmations via `IAuthenticationService`)** | Reverse-engineered (mature OSS) | Med | Med | Real differentiator for traders; reuse steamguard-cli's `steamguard` crate approach. Gate behind warnings (15-day hold, `identity_secret` risk). |
| **5** | **Battle.net (TOTP import)** | Import-only, fragile | Med | Med | Only for legacy accounts that can still get serial+restore; may be dead on current app. Ship only if cheap. |
| **—** | Discord QR, Blizzard push, Adobe push, Apple approve, Okta/Duo push | Not feasible / not recommended | — | High | ToS + no open protocol + token/credential blast radius; superseded by passkey or TOTP anyway. |

**Sequencing:** ship **#2 and #3** in v1 (easy, expected), build **#1** as the strategic centerpiece for v1.x (it *is* the generalized "login by QR" the owner wants, done legally), treat **#4** as an opt-in power feature, and **decline everything in the bottom row** — publicly frame that as "we don't put your accounts at ToS/ban risk."

---

## Generalized "bridge/plugin" architecture

Design the system around **capability tiers, not per-vendor hacks**, so 90% of coverage comes from two standards engines and only a thin, clearly-fenced plugin layer touches proprietary protocols:

```
┌──────────────────────────────────────────────────────────┐
│  Vault core (secrets, sync, platform credential-provider  │
│  registration)                                            │
└───────────────┬───────────────┬───────────────┬──────────┘
                │               │               │
      ┌─────────▼──────┐ ┌──────▼───────┐ ┌─────▼────────────┐
      │ FIDO2/Passkey  │ │ TOTP/OTP     │ │ Proprietary      │
      │ provider       │ │ engine       │ │ "bridge" plugins │
      │ (OS credential │ │ (RFC6238 +   │ │ (sandboxed,      │
      │  manager,      │ │  Steam 5char │ │  opt-in, signed) │
      │  hybrid via OS)│ │  variant)    │ │                  │
      └────────────────┘ └──────────────┘ └──────┬───────────┘
                                                  │
                              ┌───────────────────┼─────────────┐
                              │ steam-bridge      │ (future)     │
                              │ (QR approve +     │              │
                              │  trade confirm)   │              │
                              └───────────────────┴─────────────┘
```

Design principles:
1. **Standards engines are first-class and always-on**; the proprietary "bridge" layer is an isolated, opt-in plugin host with its own capability manifest (what secrets it stores, what endpoints it calls, what ToS warning it must show). This keeps the ban/legal/blast-radius surface *quarantined and auditable*, not spread through the core.
2. **A bridge plugin declares a capability interface:** `generateCode()`, `approveQrLogin(challenge)`, `confirmTransaction(id)`, `enroll()/import()`. Steam implements all four; a TOTP-only vendor implements just `generateCode()`. The UI renders from the manifest, so Steam shows a "trade confirmation" affordance and Epic doesn't.
3. **Consent & warning gates per bridge:** before enabling #4/#5-class bridges, force an explicit disclosure (ToS risk, 15-day Steam hold, elevated secret sensitivity). Log it.
4. **Secret-sensitivity labeling:** tag `identity_secret`/device-secret class material as "high blast radius," require re-auth to export, and consider storing them with a separate, higher-friction key than ordinary TOTP seeds.
5. **Treat downloaded `maFile`/import artifacts as untrusted input** (parse defensively, isolate), since they originate outside your app.
6. **Degrade gracefully:** if an OS lacks third-party credential-provider/hybrid support (older Android, some OEMs), fall back to TOTP and say so.

---

## Key uncertainties to resolve before committing

- **Battle.net 2026 state:** confirm whether serial/restore extraction still works on the merged Battle.net app, or whether it's fully closed. Forum evidence says the entry UI is gone; needs first-hand verification. *(Uncertain.)*
- **Apple hybrid-provider claim:** the "all credential-provider extensions get hybrid for free" statement comes from a forum reply of unconfirmed authorship — verify against current Apple Authentication Services / WWDC docs before architecting #1 around it. *(Uncertain.)*
- **Steam 5-char encoding specifics** and whether Valve's current anti-automation stance has shifted — the tolerance is de facto, not guaranteed.
- **2FAS / GOG / several game platforms' TOTP support** was not confirmed in sources; check each vendor's current 2FA settings page.
- **Adobe "hidden TOTP"** is a user claim, not Adobe-confirmed; passkey is the documented path.

**Bottom line:** the owner's vision is achievable *and* legal if reframed from "emulate every vendor's authenticator app" to "be a best-in-class passkey credential provider plus TOTP store, with Steam as the one sanctioned-by-ecosystem proprietary bridge." That delivers the "scan a QR, approve the login" experience across the whole modern web without taking on the ban/DMCA/blast-radius liability of reverse-engineering Blizzard, Adobe, Discord, Apple, or Okta push protocols.

Now handing this back to the caller.