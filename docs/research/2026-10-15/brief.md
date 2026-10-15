# Lockdown: product and architecture brief

_As of 15 October 2026. Written for the owner, a principal product designer. Built from seven research reports and a separate fact-check pass._

**How to read this brief.**
- Where the fact-check disagreed with a research report, the fact-check wins.
- Claims the fact-check refuted are left out, or shown only as corrections.
- **[uncertain]** means nobody could confirm the claim. Do not build on it until it is checked.
- **[background]** means general knowledge that this research did not re-check.
- Each section starts with the answer. Jargon is explained the first time it appears.

---

## 1. The short answer

- **The market has split in two.** Free built-in managers (Apple Passwords, Google Password Manager, Microsoft Edge) are now good enough for plain logins. Paid managers (1Password, Bitwarden, Proton Pass, Dashlane, Keeper) win only on extras: more item types, sharing, every platform, developer tools and business features. Prices rose in 2026. 1Password Individual is now $47.88 a year. Bitwarden Premium went from $10 to $19.80 a year.
- **Switching is getting easy, in both directions.** Passkeys are logins built on public-key cryptography, so there is no password to steal. A new standard called credential exchange lets people move passwords and passkeys between apps on the iPhone (iOS 26) and on Android (rolling out since June 2026, with a system-wide switching flow since September 2026). That lowers the cost of joining Lockdown. It also lowers the cost of leaving, so loyalty has to come from quality.
- **The biggest open gaps are trust, reliability and recovery.** A 2026 ETH Zurich study imagined the company's own server turning hostile. It found 12 attacks on Bitwarden, 7 on LastPass, 6 on Dashlane and 2 on 1Password. Autofill that misses fields is the top complaint about every product. Recovery is either brittle (1Password lockouts) or a weak point attackers aim at (Bitwarden's emergency access).
- **Several things nobody ships well yet.** Post-quantum protection for shared vault keys. Vault history you can verify. An audit log for AI-agent access from day one. Approving a TV or console sign-in from your phone. One place that knows every way you sign in to each site.
- **Your vendor-login idea is right in spirit and wrong in mechanism for most vendors.** Copying each vendor's private phone app is legally risky, breaks without warning, and makes a breach far worse. Open standards give you the same "scan and approve" experience. Be a passkey provider, generate one-time codes, and approve TV sign-ins by scanning their QR codes. Steam is the one private bridge worth building, as a fenced, opt-in feature. Battle.net and Adobe now support passkeys, so use those.
- **Lockdown should be "every way you sign in, in one place, built so even we can't read or tamper with it."** The breadth is the promise users feel. The security design is the proof. Open-source apps, a home-server option and free export are the policy that makes the proof believable.
- **Build it on one Rust core with thin native shells.** SwiftUI on Apple devices, Jetpack Compose on Android, and one web UI shared by the browser extension, the web vault and the desktop app. That is one design-token system feeding three UI stacks.
- **Plan for a small team.** A credible v1 takes about 12 to 18 months with 6 to 8 engineers. It matches the incumbents on the basics and gets the cryptography right from the start. Business features, the Windows passkey plug-in, the Linux desktop app, the SSH agent and the full Steam bridge wait for v1.x or v2.

---

## 2. Your vendor-login idea

### 2.1 The verdict

Your goal is to sign people into services using each service's own system, and to replace their separate authenticator apps. That goal is achievable. The way to get there is mostly not to imitate each vendor's phone app.

- **Steam QR approval:** feasible and well understood. It relies on reverse engineering that Valve has tolerated for years. Build it as a clearly labeled, opt-in feature in v1.x.
- **Battle.net:** do not imitate Blizzard's "approve this login" push. Nobody has publicly reimplemented it. Battle.net now supports passkeys, so use them.
- **Adobe:** do not imitate Adobe Account Access. Nobody has publicly reimplemented it either. Adobe supports passkeys, so use them.
- **The general version:** five legitimate mechanisms cover almost every service you named. Four of them need no private interfaces at all.

### 2.2 Three different things hiding inside one idea

Your idea bundles three separate capabilities. They have very different risks.

1. **One-time codes.** TOTP (time-based one-time password, RFC 6238) is the standard behind six-digit authenticator codes. The site gives you a shared secret once, through a QR code, and your app turns it into a new code every 30 seconds. This is open, safe and expected.
2. **Being the vendor's phone app.** Some vendors send a push ("Is this you signing in? Approve?") or show a QR that only their own app may approve. Doing that from Lockdown means speaking the vendor's private interface. That is almost always reverse-engineered. It usually breaks the vendor's terms of service (ToS). It also means storing secrets that are far more dangerous than a code seed.
3. **Standards-based sign-in from another device.** FIDO2 is the standard behind passkeys. Its "hybrid" mode is the familiar flow where a laptop shows a QR code and your phone approves. Device-code activation (RFC 8628) is the flow where a TV shows a code and you approve on your phone. Both are open standards built for exactly this purpose.

Capability 2 is where the risk lives. Capabilities 1 and 3 deliver most of the experience you want without that risk.

### 2.3 Steam: feasible, fenced, opt-in

**How Steam's QR sign-in works.** A new Steam client shows a QR code containing a link of the form `https://s.team/q/<version>/<client_id>`. The official Steam app scans it, shows where the request comes from, and approves it. Approval is a signature: an HMAC-SHA256 (a keyed fingerprint) made with the account's `shared_secret` over the version, the client ID and the account's Steam ID. The approving app also needs a valid sign-in session for that Steam account. Two independent open-source projects implement this from source: steamguard-cli (Rust) and node-steam-session.

**Steam holds two important secrets.**
- `shared_secret` makes the 5-character Steam Guard codes and signs QR approvals.
- `identity_secret` signs trade and Community Market confirmations. Whoever holds it can approve trades silently. It is arguably more dangerous than the password.

**Steam codes are not standard codes.** They use the TOTP clock and HMAC-SHA1, but print 5 characters from the alphabet `23456789BCDFGHJKMNPQRTVWXY`. Bitwarden, KeePassXC, Aegis, Ente Auth and 2FAS all support them. Bitwarden stores the key on any plan but generates codes only on Premium. One engineering trap: Steam files store `shared_secret` as base64, while `steam://` and `otpauth://` links expect base32. steamguard-cli fixed an interoperability bug about this in September 2026.

**How Lockdown can get the secrets, and what each path costs the user.**

| Path | What the user does | Trading and Market hold | Notes |
|---|---|---|---|
| **Import** a file from Steam Desktop Authenticator (SDA), steamguard-cli or WinAuth, or a backup of the official Steam app (usually taken from a rooted Android phone) | Picks the file and, if it is encrypted, types the SDA passphrase | **None.** Steam sees no change. | Only for people who already use a third-party authenticator or hold a backup of the official app's secrets. The file holds `shared_secret` and `identity_secret` as separate fields. |
| **Transfer** ("move authenticator") from the official app | Confirms with a text-message code. The account needs a phone number (since about February 2025). | **About 2 days** | New secrets are created. The official app on the old phone stops working. The 2-day figure comes from steamguard-cli source and community guides, not from Valve **[uncertain]**. |
| **Fresh enrollment** on an account with no authenticator | Confirms by text message or email | Adding it does not clear any existing hold. Trades and listings made in the first 7 days can still be held for up to 15 days. | Lockdown becomes the account's only mobile authenticator. |
| **Removal or revocation** (for example with the R-code) | Not a Lockdown path | **15 days** | This is what users fear. Tell them to keep their revocation code safe and never use it to switch. |

The original research said any move costs a 15-day hold. The fact-check found that too broad. Only removal costs 15 days.

**Risks to accept consciously.**
- **Terms of service.** Steam's Subscriber Agreement forbids unauthorized third-party programs that interact with Steam **[background; the agreement's wording was not re-read]**. Valve has tolerated authenticators like SDA and steamguard-cli for years. The fact-check found no legal action and no reports of account bans tied to them. That is absence of evidence, not a promise **[uncertain]**.
- **Valve keeps tuning the endpoints.** In July 2026 steamguard-cli shipped fixes for confirmation rate limits, a language header and a browser user-agent change. Expect upkeep.
- **Higher stakes.** To approve QR sign-ins, Lockdown must hold a live Steam session as well as `shared_secret`. With `identity_secret` too, a breach becomes silent inventory theft.
- **One authenticator per account.** If the user moves to Lockdown, the official Steam app no longer approves anything.
- **Fake tools.** Fake SDA builds that stole accounts circulated in 2018. Lockdown's import screen should tell users to take files only from their own computer, and it must treat every file as hostile input.
- **Separate trade rules.** Counter-Strike 2 "Trade Protection" (around July 2025, per third-party reports) can reverse trades for 7 days. It is a separate system from authenticator holds.

**What to build.**
- v1: Steam codes, plus import from SDA, steamguard-cli and WinAuth files.
- v1.x: QR sign-in approval, trade and Market confirmations, and transfer or fresh enrollment.
- Never: automatic or "approve all" confirmations. steamguard-cli has a `--dangerously-approve-all` flag and NebulaAuth advertises auto-confirm. Lockdown should show each trade's items and require a deliberate tap every time.
- Reuse the `steamguard` Rust crate (v0.18.4). It is licensed MIT or Apache-2.0, so Lockdown can embed it without GPL obligations. The steamguard-cli program itself is GPL-3.0.
- Re-encrypt imported Steam files into Lockdown's own vault on import. Both SDA's and steamguard-cli's file formats use AES-CBC without an integrity check, which Lockdown should not keep.

### 2.4 Battle.net: use passkeys, decline the push

- **Push approval: not feasible.** No public, open-source reimplementation of Blizzard's "Approve this login" push exists. The push now lives inside the main Battle.net mobile app; the standalone authenticator app was retired on 5 January 2024.
- **Code secrets: possible, but through a private interface.** Users report that an undocumented flow still works as of 10 September 2026: take a sign-in token from the Battle.net website, exchange it with Blizzard's servers, and read the authenticator's secret. The result is a standard 8-digit code. The `bna` Python library (last release 5.1.0, February 2022) cannot read an existing authenticator by itself, and WinAuth is end-of-life. Building this into Lockdown means calling Blizzard's private API, which is a ToS risk. Blizzard may also be stricter about automation than Valve **[uncertain; a judgment from the research, not a verified fact]**.
- **Passkeys: the right bridge.** The community passkeys directory lists Battle.net as passkey-capable, citing Blizzard support article 380922. The entry was added on 4 May 2026. Blizzard's own launch date is **[uncertain]**.
- **Verdict.** Support Battle.net passkeys in v1. In v1.x, accept code secrets that users bring from other tools (Aegis can export them). Do not ship an extractor that calls Blizzard's private API. Decline the push.

### 2.5 Adobe: passkeys only

- **Passkeys: supported.** Adobe's passkey help page was last updated on 25 February 2026. It allows several passkeys per account and accepts FIDO2 security keys.
- **Third-party codes: effectively gone.** Community posts from September and November 2025 say Adobe removed the authenticator-app option. That leaves Adobe Account Access, email and text messages. Adobe has not confirmed the removal **[uncertain]**. Adobe's own Authenticator app can no longer be downloaded.
- **Account Access push: not feasible.** No reverse-engineering write-up or open-source project exists.
- **Verdict.** Passkeys in v1. Decline the push.

### 2.6 The general version: five legitimate mechanisms

| # | Mechanism | Plain-language description | Covers |
|---|---|---|---|
| 1 | **Passkey provider** | Lockdown stores passkeys and registers with the operating system as a passkey provider. The OS runs the "scan this QR on your phone" step for you. | Adobe, Battle.net, PlayStation, Nintendo, Microsoft, Google, GitHub, X and every other passkey site |
| 2 | **Standard codes** | Lockdown stores the code secret and shows the code, including Steam's 5-character variant | Epic, EA, Ubisoft, Rockstar, Riot, GOG, Twitch, Nintendo, Roblox, Microsoft/Entra, Steam |
| 3 | **TV and console activation** | Scan the code on the TV, open the service's real activation page inside Lockdown, fill the login, approve | Netflix, Disney+, YouTube, Xbox, Epic, Discord, Amazon |
| 4 | **"Sign in with…" map** | Record which provider each site uses (Google, Apple, Microsoft, Steam, Battle.net, Discord) and take one tap to that provider's own sign-in | Every site that uses federated sign-in |
| 5 | **Text and email codes** | Use the platform's sanctioned code-reading interfaces only | Sites that send one-time codes |

**The passkey provider is the centerpiece.** Verified facts that make it work:
- **iPhone and iPad.** An Apple engineer confirmed in June 2026 that every enabled credential provider extension "gets Hybrid support for free through the system's implementation." Third-party providers have had this since iOS 17. Only synced passkeys qualify. A device-bound key "cannot be returned" by a provider. Apps cannot send the Bluetooth signal themselves, and attestation (proof of which device made the key) is not supported outside some managed-device settings.
- **Android.** Third-party providers can answer a desktop QR scan on Android 14 and later. Google's own docs do not say so directly. Evidence comes from Bitwarden, 1Password and the Thales SDK documentation. Some Android 14 devices from a few manufacturers do not support third-party providers. Bluetooth must be on. One 2024 user report says the scan opened Google Password Manager instead of 1Password. Test on real Pixel and Samsung devices before promising this.
- **Automatic passkey upgrades on iOS 18 and later.** After a password sign-in, the system can ask the provider to create a passkey silently. A provider opts in with the `SupportsConditionalPasskeyRegistration` key under `ASCredentialProviderExtensionCapabilities` in its Info.plist, and implements `performWithoutUserInteractionIfPossible(passkeyRegistration:)`. If Lockdown skips this, it falls behind Apple Passwords on passkey adoption.
- **Windows 11.** Third-party passkey plug-ins became generally available with the November 2025 security update. Microsoft announced 1Password and Bitwarden together on the same day. 1Password shipped a stable version and needs its MSIX-packaged app. Bitwarden's was beta at launch.
- **Persistent pairing.** Only Android phones skip the QR scan on repeat sign-ins, once linked to Windows 11 23H2 or later (or to Chrome or Edge on older Windows). iPhones, iPads and Macs need a scan every time.
- **A bonus.** Bitwarden users can sign in to a Windows PC itself by scanning its QR code. This needs a PC joined to Microsoft Entra ID with the right policy. Lockdown gets the same ability by being a provider.

### 2.7 The device-code generalization: "point your phone at the TV"

This is the most literal, legitimate answer to "approve the login from my phone."

**How the standard works.** RFC 8628, the OAuth 2.0 Device Authorization Grant, is an IETF standard from 2019. A TV, console or command-line tool asks the service for a short `user_code` and a web address to visit. Often it also gets a "complete" address with the code built in, which the TV shows as a QR code. The user opens that address on a phone, signs in on the service's own page, and approves. The TV then receives its sign-in.

**Lockdown's role is the approving browser.** Lockdown scans the QR code, opens the service's real page in its own secure in-app browser, fills the username, password and code, and the user taps approve. Nothing private is touched. Lockdown behaves exactly like a person typing.

**Activation addresses found in the wild.** These came from open-source code and one official vendor guide, not from a full vendor survey.

| Service | Activation address | Evidence |
|---|---|---|
| Microsoft / Xbox | `microsoft.com/link` | Open-source device-code clients |
| YouTube / Google | `youtube.com/activate` | Open-source clients |
| Epic Games | `epicgames.com/id/activate` | EpicResearch documentation |
| Discord (console account linking) | `discord.com/activate` | Discord's official developer guide |
| Netflix | `netflix.com/tv8` | Community documentation |
| Disney+ | `disneyplus.com/begin` | Community documentation |
| Amazon / Prime Video | `amazon.com/code` | Widely documented |

**The hard safety rule.** Device-code sign-in is one of the most abused sign-in patterns of 2025 and 2026. An attacker starts a device-code sign-in, sends the victim the code by email or chat, and the victim approves the attacker's session. Microsoft's Entra documentation (March 2026) calls device-code flow "a high-risk authentication method" and tells admins to "block device code flow wherever possible." Palo Alto Networks' Unit 42 described the same phishing pattern in March 2026. So:
- Lockdown approves a device code **only when the user scans it from a screen in front of them**.
- Lockdown **refuses codes that arrive by message, email, link or paste**. No setting can override this.
- Lockdown opens only addresses on an allow-list of known activation pages, and always shows the full domain.
- Before approval, Lockdown shows which service, which account and, where the page shows it, which device.
- Lockdown stays on the browser side of the flow. It never calls a vendor's token endpoints with borrowed or undocumented client IDs.

**Still to check.** The exact wording of RFC 8628 section 5.4 (the security considerations) could not be fetched **[uncertain]**. Each vendor's developer terms also apply to its own endpoints. Acting as a normal browser on the vendor's page is low-risk.

### 2.8 Recommended bridge catalog

Legitimacy key: **Official** = a standard or vendor-supported flow. **Tolerated** = reverse-engineered but long accepted in practice. **Closed** = private, never publicly reimplemented, or against the vendor's terms.

| Service | Mechanism | Legitimacy | Verdict | Phase |
|---|---|---|---|---|
| Any passkey site (Adobe, Battle.net, PlayStation, Nintendo, Microsoft, Google, GitHub, X) | Lockdown is an OS passkey provider; the OS runs cross-device QR | Official | **Build. Centerpiece.** | v1 on iOS, Android, Mac and extension; Windows plug-in v1.x |
| Adobe | Passkey. Authenticator-app codes effectively removed **[uncertain]**. Account Access push is closed. | Official (passkey); Closed (push) | Passkey only. Decline push. | v1 |
| Battle.net | Passkey **[launch date uncertain]**. Code secret via undocumented web flow. Push is closed. | Official (passkey); Tolerated at best (extraction); Closed (push) | Passkey. Accept secrets users bring. No in-app extractor. Decline push. | v1 passkey; v1.x import |
| Steam codes | 5-character Steam Guard codes from `shared_secret` | Tolerated (secret comes from Steam's enrollment or the user's own file) | **Build** | v1, with file import |
| Steam QR sign-in approval | Sign Steam's challenge with `shared_secret` plus a stored Steam session | Tolerated | **Build, fenced and opt-in,** behind the proximity rule | v1.x |
| Steam trade and Market confirmations | Sign confirmations with `identity_secret` | Tolerated | **Build, fenced and opt-in.** Crown-jewel storage. Never auto-confirm. | v1.x |
| Steam move or enroll | Transfer (about a 2-day hold) or fresh enrollment | Tolerated | Build after codes and import prove stable | v1.x |
| Epic, EA, Ubisoft, Rockstar, Riot, GOG, Twitch, Nintendo, Roblox, Microsoft/Entra | Standard 6-digit codes. Riot's newer multi-factor sign-in supports authenticator apps. GOG requires turning off email codes first. Twitch requires a phone number first. Roblox was not re-checked. | Official | **Build** | v1 |
| PlayStation, Nintendo | Passkeys (PlayStation since February 2024, Nintendo since September 2023, up to 10 per account) plus codes on Nintendo | Official | **Build** (falls out of passkeys and codes) | v1 |
| Netflix, Disney+, YouTube, Xbox, Epic, Discord, Amazon on TVs and consoles | Scan the TV's QR, open the real activation page in Lockdown, fill, approve | Official (RFC 8628) | **Build. Flagship.** With the proximity rule. | v1.x |
| "Sign in with Google, Apple, Microsoft, Steam, Battle.net, Discord" | Record each site's sign-in provider; one tap to the provider's own page | Official | **Build** | v1 |
| Text-message and email codes | iOS one-time-code provider; Android SMS User Consent; email only read on the device | Official | **Build** in stages | iOS v1; Android v1.x; email v2 |
| Discord QR sign-in | Only Discord's app can approve; open-source code reimplements the computer side | Closed (against terms) | **Decline.** Use codes, passkeys and `discord.com/activate`. | Never |
| Microsoft, Google, Okta, Duo push; Apple's "approve on trusted device" | Private push channels | Closed | **Decline.** Use passkeys and codes. | Never |
| Okta and Auth0 custom authenticator kits | Become an organization's chosen push app | Official, but per organization | Business sales track only | Later |

**Public stance.** "We sign you in through each service's own supported login. The one exception, Steam, is opt-in and clearly labeled." This turns a limitation into a trust message.

### 2.9 The bridge architecture

The rule: standards do 90% of the work, and the risky part is quarantined.

```
┌──────────────────────────────────────────────────────────────┐
│ Vault core (Rust): keys, items, sync, OS provider registration│
└──────┬──────────────┬───────────────┬───────────────┬────────┘
       │              │               │               │
 ┌─────▼─────┐ ┌──────▼──────┐ ┌──────▼──────┐ ┌──────▼────────────┐
 │ Passkey   │ │ Code engine │ │ Approval    │ │ Bridge host       │
 │ provider  │ │ TOTP, Steam │ │ service     │ │ (first-party,     │
 │ (OS runs  │ │ 5-char,     │ │ proximity   │ │ signed, opt-in)   │
 │ hybrid QR)│ │ 8-digit     │ │ rule, number│ │  ├ steam          │
 └───────────┘ └─────────────┘ │ matching    │ │  ├ device-code    │
                               └─────────────┘ │  └ (future)       │
                                               └───────────────────┘
```

**1. Capability manifest.** Each bridge ships a signed manifest. The UI draws itself from the manifest, so Steam shows a "Confirm trade" control and Epic does not. Illustrative example:

```yaml
id: steam
tier: tolerated            # official | tolerated | (closed bridges are never built)
capabilities: [generate_code, approve_qr_login, confirm_transaction, import, transfer, enroll]
secrets:
  shared_secret:   { class: high }
  steam_session:   { class: high }
  identity_secret: { class: crown_jewel }
  revocation_code: { class: crown_jewel, display_only: true }
network_allowlist: [api.steampowered.com, steamcommunity.com]   # illustrative
disclosures: [tos_risk, trade_hold_by_path, one_authenticator_per_account]
requires_proximity_gate: [approve_qr_login]
kill_switch: true
```

**2. Risk tiers.**

| Tier | Examples | Default | Gate |
|---|---|---|---|
| Standards | Passkeys, TOTP, "sign in with" map | Always on | None beyond normal unlock |
| Official vendor flow | Device-code activation | On, per use | Proximity rule; allow-list; full domain shown |
| Tolerated | Steam QR approval and confirmations | Off until the user opts in | Plain-language disclosure; typed confirmation; logged consent |
| Closed | Vendor push emulation | Never built | Not applicable |

**3. Consent gates.** Before a tolerated bridge turns on, the user reads a short disclosure and confirms. For Steam it says: the ToS risk, the hold for their chosen path, that the official app stops approving, and what Lockdown will store. Consent is recorded in the vault's history with the disclosure version.

**4. One shared approval service.** Passkey QR, device-code activation and Steam QR approval all boil down to "approve a sign-in I am being shown." Build that once.
- The proximity rule: approve only codes the user scanned from a screen in front of them.
- Number matching where the vendor supports it, and context (service, account, location if known).
- Rate limits and a "this wasn't me" action. "Push fatigue" (bombarding a user with prompts until they tap yes) was used in the 2022 Uber and Cisco breaches. Microsoft has forced number matching in its own app since May 2023.

**5. Higher-friction storage for high-blast-radius secrets.** "Blast radius" means how much damage one stolen secret can do.
- Crown-jewel secrets (Steam `identity_secret`, crypto recovery phrases, the master login of an email account) get their own key, wrapped separately.
- Using one to sign a confirmation needs a fresh biometric check and a deliberate tap. Revealing or exporting one needs the full account password, not just a fingerprint.
- They are never autofilled, never shown in search previews, never given to AI agents, and left out of bulk exports and legacy handover unless the user opts in.
- The user can choose "this device only," so the secret never syncs.

**6. Kill switch and upkeep budget.** Lockdown can switch off a bridge remotely with a signed configuration if Steam changes its interface or objects. Each tolerated bridge needs a named owner and a maintenance budget.

**7. First-party bridges only, at first.** A public plug-in kit invites malicious bridges. Open it later, if ever, with code review and signing.

**8. Hostile imports.** Every imported file (Steam files, CSV, backups, icon packs) is treated as hostile. Aegis fixed a malicious icon pack that could overwrite the vault through path traversal (v3.4.3). Parse in isolation, cap sizes and validate.

**9. Graceful fallback.** Where a device lacks third-party passkey support, fall back to codes and say so plainly.

### 2.10 Legal note

- Using the TOTP and passkey paths carries essentially no anti-circumvention risk, because they use each vendor's own enrollment.
- Emulating a private login-approval protocol is a legal grey area under the US DMCA section 1201 rules on bypassing access controls. A commercial product is more exposed than a hobby project. This is the research's analysis, not legal advice.
- Get counsel in the US and EU before shipping the Steam bridge. Ask specifically about the Steam Subscriber Agreement and anti-circumvention law.

---

## 3. Positioning

**Where the competitors stand.**
- **1Password** is polished and seen as the most secure, but it costs more after March 2026 and puts many new features in business plans.
- **Bitwarden** is open source and cheap, but its price doubled in January 2026, its 2024 redesign drew long complaint threads, and it took the most hits in the ETH study.
- **Proton Pass** sells privacy and email aliases, but users complain loudly about autofill.
- **Apple and Google** are free and built in, but each stays inside its own ecosystem and stores mostly logins.

### Option A: "Every way you sign in, in one place."

> Lockdown is the one app for every way you sign in: passwords, passkeys, codes, game accounts and the TV, on every device you own.

- **For:** It matches your original vision. It has a memorable demo: point your phone at the TV and you are signed in. It answers a real annoyance, because people juggle a password manager, an authenticator, a Steam app and TV codes. The breadth is hard for Apple or Google to copy across ecosystems.
- **Against:** It risks feature sprawl for a small team. Built-in managers are already "good enough" for logins. It only works if autofill is excellent, because breadth means nothing if the basics miss. Some pieces are easy to copy; 1Password already picks the sign-in method for you with Universal sign-in.

### Option B: "The password manager that can't betray you."

> Lockdown can't read, swap or roll back your secrets, even if our own servers are hacked, and you can check that yourself.

- **For:** It targets a gap the ETH study made public. It is a durable advantage, because rivals would have to redesign their cryptography. It wins security-minded users and the press.
- **Against:** It is abstract for most people. It is hard to prove and slow to build. Every rival also says "secure." A single serious bug would hurt this position more than any other.

### Option C: "Open, fair and yours."

> Lockdown is open source, fairly priced and yours to keep: run it on our cloud or your own server, and export everything, free, forever.

- **For:** Bitwarden's base is unsettled by the price rise and leadership changes. Vaultwarden shows strong demand for a simple home server. Free export answers the stranding seen with Raivo, Buttercup and Kaspersky's US exit.
- **Against:** This space is crowded (Bitwarden, Vaultwarden, Proton, KeePassXC). People who care most about open source often pay least. Openness alone rarely makes anyone switch.

### Recommendation

Lead with **A**, prove it with **B**, and make **C** your policy.

> **Every way you sign in. Locked down, even from us.**

- A is what a person feels in the first week.
- B is why a careful person trusts Lockdown with everything.
- C is not a slogan. It is a set of commitments: open-source apps under an OSI-approved licence, a self-hosted server identical to the cloud one, and free export forever.

**Launch wedge.** Gamers and multi-device households. They feel the Steam, console and TV pain most, and they spread the word. Make the TV moment the hero of the launch video.

**Pricing hint.** Make codes and passkeys free. Bitwarden charges for code generation, and Proton limits it on its free plan. Charge for family plans, sharing with outsiders, attachments, the digital-legacy features and the Steam bridge. Model this carefully: free features cost money to run.

---

## 4. Security architecture in plain language

### 4.1 Threat model: assume our own server turns hostile

- **What it means.** Design as if Lockdown's server, or someone who controls it, actively lies to the apps. The server may swap keys, replay old data, drop items or tamper with fields. The apps must catch it.
- **Why.** That is exactly the model ETH Zurich and USI used (Scarlata and colleagues, USENIX Security 2026, ePrint 2026/058). It is also what a real breach or a coerced insider looks like.
- **What competitors got wrong.**
  - The study found 12 attacks on Bitwarden, 7 on LastPass and 6 on Dashlane. They ranged from tampering with single items to exposing an organization's vaults.
  - In recovery and business flows, Bitwarden's and LastPass's apps fetched public keys from the server without checking them. A hostile server could swap in its own key and collect escrowed keys.
  - 1Password was also studied. The press reports 2 attacks. Its sharing does not authenticate public keys or the encrypted key packages (RSA-OAEP ciphertexts). That allows key or vault substitution in sharing. 1Password says these limits were already documented in its white paper. So "1Password held up because of its Secret Key" is too simple. The Secret Key does not stop the server from swapping a public key.
  - Bitwarden says all issues have been addressed. A claim that it fixed 7 and called 3 "design decisions" is **[uncertain]**.
  - LastPass stored website addresses unencrypted. After its 2022 breach, thieves used them to target people with crypto wallets. More than $35M was stolen.
- **What Lockdown does.** Publish a malicious-server analysis before launch. Invite academics to attack it. Every item below follows from this model.

### 4.2 Key hierarchy: a tree of locked boxes

- **What it is.** Each key locks the next one down.
  1. **Account unlock key.** Made from your password (stretched with Argon2id, explained in 4.3), the login protocol's output and, if enabled, the Secret Key.
  2. **User key.** A random key the account unlock key opens. It opens your identity keys.
  3. **Identity keys.** A hybrid encryption key pair (classic plus post-quantum, see 4.5) for receiving shared keys, and a signing key pair for proving edits are yours.
  4. **Vault keys.** One random 256-bit key per vault.
  5. **Item keys.** One per item, locked by the vault key. Sharing or removing one item does not mean re-encrypting everything.
  6. **Fields.** Encrypted with AEAD (authenticated encryption with associated data). AEAD both hides the data and detects tampering. The "associated data" glues each field to its item ID, vault ID, field name and format version. A server cannot move an encrypted password into a different item or field.
- **Unlock "slots."** Like Aegis, the user key can be opened by several independent slots: password, biometrics held in the Secure Enclave, StrongBox or TPM chip, a passkey with the PRF extension (which lets a passkey produce a secret key), or a recovery kit. Adding a slot never weakens another one. Aegis's lesson is worth stating to users: the vault is only as strong as its weakest slot. Biometric-only users get a periodic password reminder so they don't forget the password they never type.
- **Crown-jewel class.** High-blast-radius items get their own key (see 2.9).
- **All metadata is encrypted.** Titles, addresses, usernames, tags, icons and the "sign in with" map. Icons load through a privacy proxy.
- **What competitors got wrong.**
  - As of October 2026, Bitwarden's shared code still offers only RSA-OAEP with SHA-1 for public-key encryption. That is the same unauthenticated building block the ETH study flagged in 1Password's sharing.
  - The ETH study showed fields swapped between items, and old encryption modes forced back into use.
  - LastPass left addresses in plain text.
  - Ente Auth's app lock is a screen lock only. It does not re-encrypt anything, and its docs have to say so. Lockdown's PIN must be a real key slot, never just a screen lock.

### 4.3 Login: OPAQUE instead of SRP, and strong password stretching

- **What it is.** A PAKE (password-authenticated key exchange) lets you prove you know your password without sending it.
  - **SRP** (Secure Remote Password) is the older PAKE. 1Password and Proton use it.
  - **OPAQUE** is the modern one. It was published as RFC 9807 in July 2025. It is an Informational document from the IRTF's crypto research group (CFRG), not a Standards Track RFC. WhatsApp uses it in production for encrypted backups. A Rust implementation exists (`opaque-ke`).
- **Why OPAQUE.** SRP hands the password's salt to anyone who asks. An attacker can prepare guesses before stealing the server's data. OPAQUE never reveals the salt, so that preparation is impossible **[background: this is OPAQUE's main design property]**. OPAQUE also hands the app an "export key" it can use to build the account unlock key.
- **One caveat.** A formal analysis (ePrint 2023/843) found a corrupted server may get somewhat more password guesses than earlier analyses assumed. Rate limits and the Secret Key cover this.
- **Password stretching.** A KDF (key-derivation function) makes each password guess slow and memory-hungry. Use **Argon2id** on the device at **at least 64 MiB of memory, 3 passes, 4 lanes** on desktop.
  - On phones and in the extension, measure each device and pick parameters above a fixed floor.
  - The server stores the parameters signed, and apps refuse anything below the floor. A server cannot quietly weaken them.
  - Ente Auth needed about 1 GB of free memory to log in, so it failed on 2 to 3 GB phones. Lockdown must re-tune on a strong device and show a clear message on a weak one.
- **What competitors got wrong.**
  - Bitwarden is not a PAKE. The app derives a hash and sends it to the server. Its salt is the user's email address **[background]**, so an attacker can prepare guesses for a known person in advance.
  - LastPass vaults with low stretching settings were cracked offline after the 2022 breach.

### 4.4 The Secret Key option

- **What it is.** A random 128-bit code, created on your first device and mixed into your account unlock key. Think of it as a second password you never memorize. 1Password calls this two-secret key derivation.
- **Why.** If thieves steal the whole server database, they still cannot test password guesses. Without the Secret Key, every guess is useless.
- **What competitors got wrong.**
  - 1Password has the strongest version, but users get locked out. A 2025 forum report describes a reset that produced a new Secret Key without a usable Emergency Kit.
  - Its passkey-only unlock, which drops the Secret Key, has been a beta for new Individual accounts since December 2023. A staff reply on 21 November 2025 said "more work is still needed." Its status in 2026 is **[uncertain]**.
- **What Lockdown does.**
  - **On by default.** An opt-out is allowed with a plain warning about what it costs.
  - **Rarely typed.** New devices get the Secret Key by scanning a QR code on an existing device, confirmed by a short code both screens show.
  - **Printed once, practiced once.** Onboarding ends with a recovery kit and a short recovery drill (see 4.8).
  - The Secret Key protects against stolen server data. It does not stop key swapping in sharing; key transparency does that (4.6).

### 4.5 Post-quantum hybrid wrapping

- **What it is.**
  - Normal vault encryption (AES-256 or XChaCha20) already resists future quantum computers.
  - The weak point is the public-key layer: sharing, family and organization keys, recovery and device enrollment.
  - An attacker can copy wrapped keys today and decrypt them later, when quantum computers exist. This is called "harvest now, decrypt later."
  - The fix is a **hybrid** key wrap: combine a classic algorithm (X25519) with a post-quantum one (ML-KEM-768, the NIST standard formerly called Kyber). An attacker must break both.
- **What Lockdown does.**
  - Wrap every vault, family, organization and recovery key to hybrid recipient keys from day one. Use HPKE (Hybrid Public Key Encryption, RFC 9180) with a vetted hybrid such as X-Wing (X25519 plus ML-KEM-768) **[background: confirm the construction's standards status before the spec]**.
  - Sign with Ed25519 and leave a slot for ML-DSA, the post-quantum signature standard. Signatures are less urgent, because a forger would need a quantum computer at the moment of forging.
  - Use hybrid TLS to the server.
- **What competitors got wrong, or haven't done yet.**
  - **1Password:** hybrid X25519MLKEM768 for web-app TLS only (31 March 2026). Nothing at the vault level was found.
  - **Keeper:** hybrid ECDH-P256 plus ML-KEM-1024 for app-to-server traffic (February 2026). Record sharing still uses only an elliptic-curve key.
  - **Bitwarden:** new signing keys use ML-DSA-44 (April 2026), but key wrapping is still RSA-OAEP with SHA-1.
  - **Proton:** post-quantum keys for Proton Mail (opt-in, May 2026), and a co-authored RFC 9980 on post-quantum OpenPGP (June 2026). Nothing for Proton Pass.
  - NordPass and Dashlane were not checked, so "nobody does this" is likely but unproven **[uncertain]**.

### 4.6 Key transparency

- **What it is.** A public, append-only log of every user's public keys. If the server shows you a fake key for your sister, the fake key must appear in the log, where your sister's app will notice it. It works like a public ledger of phone-book entries.
- **For high-value shares,** add an in-person check: both people compare a short code or scan each other's QR code. Padloc uses a short passphrase spoken out of band. Ente Auth shows a verification ID.
- **Why.** It closes the attack the ETH study confirmed: unauthenticated public keys in recovery, emergency access and sharing.
- **What competitors got wrong.** Bitwarden, LastPass and 1Password sharing all trusted the server's word for public keys. Proton is the exception: it verifies sharing keys through key transparency.
- **What Lockdown does.** Run the log as part of the server. Publish its checkpoints so outsiders can audit them. Apps check the log before wrapping any key to another person, including legacy heirs, family members and agents.

### 4.7 Anti-rollback manifests

- **What it is.** Each vault keeps a signed table of contents: every item ID and its version. Each new table links to the previous one with a hash, like a receipt roll. Every device remembers the newest table it has seen.
- **Why.** Without it, a hostile server can serve an old version of an item, hide an item, or reorder changes, and nobody notices.
- **What competitors got wrong.** No mainstream manager advertises verifiable vault history to its users. The ETH study demonstrated integrity attacks of this kind.
- **What Lockdown does.**
  - Check the chain on every sync. Warn loudly, for example "Rollback attempt blocked."
  - Use the same chain for consumer-visible item history: "Dad changed the Netflix password on 3 October." It also gives one-click restore.
  - When someone asks for deletion, delete the per-version keys so old versions become unreadable ("crypto-shredding").

### 4.8 Recovery menu

People choose at least one option at onboarding. Lockdown recommends two.

| Option | What it is | Good for | Main risk and the guard |
|---|---|---|---|
| **Recovery kit** (default) | A printed page or a word list holding the Secret Key and a recovery code | Everyone | Lost along with every device. Guard: a recovery drill and a yearly reminder. |
| **Another device** | An existing device approves the new one by QR code and a short matching code | Everyday use | Needs two devices. Guard: nudge to enroll a second device. |
| **Trusted contacts** | A contact can request access. A waiting period starts, and you can veto it. | Families | A server swaps the contact's key. Guard: key transparency plus an in-person check at enrollment. |
| **Split recovery (Shamir k-of-n)** | The recovery key is split so any 3 of 5 chosen people can rebuild it, and fewer learn nothing | Power users, digital legacy | Coordination. Guard: printable share cards and a share drill. |
| **PIN backed by secure hardware** | A short PIN, checked by tamper-resistant servers that allow only a few guesses (as WhatsApp backups and Signal's Secure Value Recovery do) | People who will lose paper | Trust in Lockdown's hardware and operations. Guard: optional; ship in v2 with an outside review. |
| **Organization recovery** (business) | Admins recover a member's access | Business plans | An admin back door. Guard: several admins must approve (M-of-N), the member consents, and everyone is notified and logged. |

- **Recovery drill.** A practice run that proves recovery works without losing anything. Most users only find out at the worst moment that their recovery doesn't work.
- **What competitors got wrong.**
  - Bitwarden's emergency access was exactly the surface the ETH study attacked.
  - 1Password has no emergency access, and its Secret Key causes lockouts.
  - Passbolt recovers organizations through one admin key, and its docs ask the admin to phone the user first.
  - Heylogin documents no path for when every login device is lost.
  - Apple ties everything to the device passcode.
  - Norton (2022) let the account password double as the vault password, so credential stuffing opened vaults.

### 4.9 Other must-haves, briefly

- **Autofill that resists clickjacking.** At DEF CON 33 (August 2025), researcher Marek Tóth showed that hidden fill controls could leak credentials in 10 of 11 managers with one click. 1Password's fix (version 8.11.7) adds an "Ask before filling" prompt, but it is off by default. Tóth's list, updated in January 2026, still shows 1Password as not fixed. Lockdown should draw menus in the browser's top layer, check the visibility of the whole page structure, and require a second deliberate action for codes, cards and passkeys **by default**.
- **Check who is calling.** Desktop apps talk to the browser extension and command-line tools over local channels. Authenticate every caller by code signature or process identity. An NDSS 2025 paper reportedly showed credential theft over these channels on Windows. Its title, authors and findings are **[uncertain]**, so do not cite it until they are confirmed. The defence is sound either way.
- **Memory hygiene.** Keep secrets inside the Rust core. Wipe them after use (`zeroize`), keep them out of swap (`mlock`), and never pass them through JavaScript or Java strings.
- **Supply chain.** Reproducible builds, a software bill of materials (SBOM), signed releases, pinned dependencies, and a small JavaScript dependency tree.
- **Honest generators and imports.** Use only a cryptographically secure random generator. Kaspersky (CVE-2020-27020) and old RoboForm versions seeded passwords from the clock **[background]**. Treat every import as hostile.
- **Never sync code secrets into the account they protect.** In the 2023 Retool breach, one hijacked Google account exposed every synced code **[background]**. Warn when a code's issuer is the same account as the sync login.

---

## 5. Tech stack

### 5.1 Rust core

- **What.** One Rust workspace holds the crypto, the vault model, sync, credential-exchange import and export, the address matcher (Public Suffix List plus known equivalent domains), the code engine (TOTP, Steam, 8-digit), the policy engine, the approval service and the bridge host.
- **Why.** It is the industry consensus. Bitwarden's shared SDK is Rust, Proton Pass uses a Rust common library, and 1Password 8 has a Rust core. Rust prevents whole classes of memory bugs and runs everywhere.
- **Guardrails.** Dependency vetting with `cargo-vet` and `cargo-deny`. Fuzz every parser.

### 5.2 Bindings

- **UniFFI** generates Swift and Kotlin bindings for the mobile apps.
- **wasm-bindgen** compiles the core to WebAssembly (WASM) for the browser extension and web vault.
- The command-line tool and server use Rust directly.

### 5.3 Desktop shell

| Option | For | Against |
|---|---|---|
| **Electron** | Lowest risk. Reuses the web UI exactly. One rendering engine (Chromium) everywhere. | Large download and memory use. A big npm supply chain, and npm worms hit the ecosystem in 2025 **[background]**. 1Password and Bitwarden both use it, and "Electron app" is a long-running complaint about 1Password **[background]**. |
| **Tauri 2** | Small and fast. Rust-native communication with the core. A smaller attack surface. Reuses the web UI. | Uses each OS's own web engine: WKWebView (Safari's engine) on Mac, WebView2 (Chromium) on Windows, WebKitGTK on Linux. That means three engines to test, and WebKitGTK is the weakest. |
| **Fully native** (SwiftUI/AppKit on Mac, WinUI on Windows, GTK on Linux) | Best OS integration and feel | Roughly three times the cost. Too much for a small team. |

**Recommendation: Tauri 2 on all three desktops for v1,** sharing the web UI with the extension and web vault. Native pieces sit beside it where the OS demands them:
- the **macOS AutoFill credential provider extension**, in Swift, sharing code with the iOS extension (1Password shipped its own to everyone on 11 August 2026; its beta needed macOS 14 and Apple silicon);
- the **Windows passkey plug-in**, a COM component. 1Password's needs its MSIX-packaged app, so plan for MSIX packaging. Whether Tauri's tooling produces MSIX cleanly is **[uncertain]**; test early.

Revisit a native SwiftUI Mac app in v2 if Mac users ask for it.

### 5.4 Mobile

- **Native SwiftUI on iPhone and iPad, native Jetpack Compose on Android.** Autofill extensions, the Android Credential Manager, the platform key stores and the camera all need native code.
- iOS: AutoFill credential provider extension, the credential identity store (so suggestions appear without launching the app), one-time-code provider (iOS 18 and later), conditional passkey registration, and credential exchange import and export (iOS 26).
- Android: AutofillService with keyboard suggestions, Credential Manager provider, and onboarding for Chrome's "Autofill using another service" setting (Chrome 135 and later). Credential exchange uses `androidx.credentials:providerevents`, which is still alpha.
- Keep the extensions small. iOS extensions have tight memory limits **[background]**, and the Rust core helps.
- No Kotlin Multiplatform for crypto. The Rust core already shares the logic.

### 5.5 Browser extension

- Manifest V3 (MV3, the current Chrome extension format) for Chrome, Edge and Firefox. A Safari Web Extension inside the Mac and iOS apps.
- TypeScript UI, Rust core compiled to WASM. No long-lived secrets in the background service worker.
- The fill menu sits in an extension-owned frame in the top layer (see 4.9).
- Fill only on the exact site by default. Base-domain matching is opt-in. Never fill into a cross-site frame without confirmation.
- **Passkeys.** Chrome and Firefox have no official passkey-provider interface for extensions. Vendors replace the page's passkey functions, and they clash: a June 2026 forum report says 1Password began blocking other managers' replacements. On Mac and Windows, prefer the OS provider path. Keep the extension override only as a fallback, mainly for Linux and ChromeOS.
- **Signal API.** This lets sites tell the manager that a passkey was deleted or a username changed. It ships in desktop Chrome and Edge 132, Chrome for Android 144 and Safari 26. Firefox does not support it. On desktop, extension providers decide for themselves whether to act. Implement all three signals, and hide rather than delete, with undo, because a buggy site can send a wrong list.

### 5.6 Server and self-hosting

- **One Rust binary** (axum web framework, sqlx database layer) with SQLite or Postgres, and S3-compatible storage for attachments. One container image.
- **The cloud and the self-hosted server run the same code,** with features switched by configuration. Security fixes ship to both at once. Vaultwarden shows the demand and the trap: it is unofficial, unaudited, had many security advisories in 2026, and still lacks passkey login and new-device protection as of 1.37.4.
- **Mobile push for self-hosters.** Apple's and Google's push services only accept messages signed with the app publisher's keys. Lockdown must run a relay. The relay carries only content-free "sync now" pings. Vaultwarden depends on Bitwarden's relay for this.
- The server also hosts the key-transparency log and the anti-rollback checkpoints.

### 5.7 Design-system implications

You will run one design system across three UI stacks: SwiftUI, Compose and web. The web stack itself runs in three browser engines plus Tauri's three. Practical consequences:

- **One token source.** Author tokens as Figma variables, store them in the W3C Design Tokens Community Group (DTCG) JSON format **[background: check the format's current version]**, and generate Swift, Kotlin and CSS from them with a build step (for example Style Dictionary **[background]**). Nobody hand-edits generated platform files.
- **Three token tiers.** Primitive (raw values), semantic (roles like "surface/danger"), and component (specific uses). Modes for light, dark and increased contrast. A density setting for compact desktop versus touch.
- **Type follows the platform.** Map semantic type roles to Dynamic Type on Apple, font scale on Android and `rem` on the web. Never fix pixel sizes.
- **One spec, three implementations.** Each component gets one written spec (anatomy, states, accessibility) and three builds. Link them to Figma with Code Connect. Run screenshot tests per platform and per web engine.
- **Brand in tokens, behavior in the platform.** Colors, type and shape carry the brand. Navigation, sheets and gestures follow each platform's conventions.
- **Security components belong in the design system.**
  - An approval sheet with number matching and context.
  - Risk-tier badges for bridges.
  - A secret field: masked, reveal on tap, copy with a clipboard timer.
  - A code display that shows the countdown, colors and a haptic cue before expiry, and the next code early (as Aegis does). It handles 6-digit, 8-digit and Steam 5-character codes.
  - A TV "large type" view that groups characters in threes and colors digits and letters differently.
  - "Crown-jewel" styling that web content cannot imitate.
- **Surfaces you cannot style.** The iOS keyboard suggestion bar, system passkey sheets, Android keyboard suggestions (limited styling), Windows Hello prompts and browser passkey dialogs. Design graceful hand-offs to them and list them in the system docs.
- **Clickjacking is a design constraint.** In-page menus are always fully opaque, have a minimum size and sit in the top layer. Sensitive approvals happen in the app or OS, never inside the web page.
- **Accessibility.** Target WCAG 2.2 AA. Screen readers should read codes digit by digit.

---

## 6. Roadmap

**Team assumption.** Six to eight engineers: two on the Rust core and crypto, one iOS, one Android, two web/extension/desktop, one server and infrastructure. Plus you as design lead, one product lead, and a contracted auditor and cryptographer. With fewer people, cut more from the lists below.

### v1: "Trustworthy parity" (about 12 to 18 months, with alpha and beta along the way)

**Goal.** Someone can move from 1Password, Bitwarden or Apple Passwords in under 10 minutes and lose nothing they use daily. The cryptography is final-grade from day one, because retrofitting it later is the most expensive mistake possible.

**Scope.**
- **Platforms:** iPhone and iPad (iOS 17 and later; codes and automatic upgrades on 18 and later), Android (passkeys on 14 and later), Chrome, Edge, Firefox and Safari extensions, Tauri desktop for Mac and Windows with the native Mac AutoFill extension, and a web vault.
- **Items:** login (with its sign-in method), passkey, codes (including Steam), secure note, card, identity, Wi-Fi (with a QR "guest card"), attachments with a size cap, custom fields.
- **Crypto:** the full key tree, OPAQUE login, Argon2id, Secret Key on by default, hybrid post-quantum wrapping, key transparency with in-person verification, signed vault manifests, encrypted metadata.
- **Sharing:** shared vaults, family plan up to 6, share links to non-users with expiry and view limits.
- **Autofill:** hardened extension menu, iOS provider with one-time codes, Android AutofillService and Credential Manager, Mac AutoFill.
- **Passkeys:** provider on iOS (with automatic upgrades), Android, Mac and the extension. Signal API with hide-and-undo.
- **Import and export:** credential exchange on iOS 26 and Android, CSV, 1Password 1PUX, Bitwarden JSON, Proton Pass, Google Authenticator transfer QR codes, Aegis, 2FAS, Ente, and Steam files (codes only). Export to credential-exchange format, encrypted JSON and CSV, free forever.
- **Health:** breach checks with Have I Been Pwned using k-anonymity (only a hash prefix leaves the device), reused and weak passwords, passkey-available nudges, the "sign in with" map, and three breach playbooks: email account compromised, site breach, malware on a device.
- **Recovery:** recovery kit, device-to-device approval, trusted contact with a waiting period, and a recovery drill.
- **Self-hosting:** the single binary, same code as the cloud.
- **Operations:** independent audit, published malicious-server analysis, public bug bounty, reproducible builds, SBOM.

**Exit criteria.**
- The independent audit has no open high or critical findings. The malicious-server analysis is published.
- Autofill succeeds on at least 95% of a 500-site login test set. Lockdown builds and maintains this set.
- Passkey registration and sign-in pass on iOS 17, 18 and 26; Android 14, 15 and 16 on Pixel and Samsung; and desktop Chrome, Edge, Firefox and Safari.
- Round-trip import tests from 1Password, Bitwarden, Apple Passwords, Google Password Manager and Proton Pass lose no supported item.
- At least 9 of 10 usability-test participants recover a vault from the recovery kit alone.
- At least 99.5% of sessions are crash-free during the public beta.

### v1.x: "The sign-in hub" (about 6 to 9 months after v1)

**Goal.** Deliver your vendor-login vision through legitimate means, and close power-user gaps.

**Scope.**
- **TV and console activation** (scan, fill, approve) with the proximity rule and allow-list.
- **Steam bridge**, fenced and opt-in: QR approval, trade and Market confirmations, transfer and fresh enrollment, crown-jewel storage, kill switch.
- **Imports** of Battle.net and other code secrets that users bring from other tools.
- **Windows passkey plug-in** (MSIX) and the **Linux desktop app**.
- **SSH agent with Git commit signing,** plus a command-line tool with scoped tokens. 1Password, Bitwarden, KeePassXC and now Proton Pass all ship SSH agents (Proton's shipped in its command-line tool in December 2025 and in its desktop app in August 2026, as a beta on macOS).
- **Passkey PRF unlock** on browsers and authenticators that support it. Bitwarden offers this only on its web vault and Chromium extensions, not on mobile. Add an optional two-key mode in which a security key is required beside the password, as KeePassXC does with YubiKey challenge-response; allow it only after two keys are enrolled.
- **Digital legacy:** a scoped legacy vault, an inactivity timer with check-ins and an owner veto, and split recovery among heirs.
- **Visible item history** and shared-vault activity logs.
- **Quick wins:** an account-deletion helper (using JustDeleteMe's MIT-licensed data), passwordless coaching, seed-phrase safe mode (detect recovery phrases in notes and move them to a protected item), expiry reminders, and automatic encrypted local backups with a "last successful backup" indicator (as Aegis does).
- **Android text-message codes** through the SMS User Consent interface, relayed end-to-end encrypted to the desktop.
- **Panic lock** (lock everything, clear the clipboard, end sessions) and a **travel profile**.
- **Family roles:** guardian, teen, child and assisted adult, with "graduation" at adulthood.
- **Learned autofill** that classifies fields on the device, behind a flag, measured against the test set.

**Exit criteria.**
- TV activation works end to end on at least 5 of the 7 listed services, and the proximity rule passes a red-team phishing test.
- The Steam bridge runs 60 days in beta with no auto-confirm path, consent logged for 100% of users, and a tested kill switch.
- The Windows plug-in passes on the current Windows 11 release. Linux runs on the two most common desktop environments.
- Learned autofill beats the v1 heuristics on the test set before it becomes the default.

### v2: "Teams and agents" (about 9 to 12 months after v1.x)

**Goal.** Earn business revenue and lead on safe AI-agent access.

**Scope.**
- **Teams and Business:** single sign-on through OIDC with device-approved unlock (no master password), SCIM user provisioning, policies, admin recovery that needs several admins (M-of-N), "expire on permission loss" (Passbolt marks every item a departing member could see as "rotate now"), and access requests with a stated reason and a time limit.
- **Agent-safe credential broker,** using MCP (Model Context Protocol, the standard way AI assistants call tools). Tools are actions, not getters: "log in to this site for this task," never "give me the password." Capability tokens are scoped to item, site, action and time. Each task needs human approval with number matching. The audit log is end-to-end encrypted and hash-chained, on day one. Crown-jewel items are denied by default. The MCP 2025-11-25 spec already says servers must not collect passwords through form-mode prompts. 1Password's agent features shipped in July 2026, but its Browserbase audit logs were still "planned."
- **Watch approvals** (Apple Watch, Wear OS). Proximity can only lock, never unlock.
- **Email codes read on the device** from a mailbox the user connects. Nothing passes through Lockdown's servers, which avoids Google's paid security assessment (CASA) for restricted Gmail access.
- **Document scanning** into typed items (passport machine-readable zone, driver's licence barcodes). Data only by default; images are optional and separately encrypted.
- **Natural-language vault search** on the device. Retrieval only; no generative model ever runs over secrets.
- **age and GPG key storage** with an approval-gated decryption plug-in.
- **PIN backed by secure hardware** for recovery.
- **A second independent audit.**

**Exit criteria.**
- Five or more paying pilot organizations complete SSO and SCIM onboarding without engineering help.
- An outside red team cannot extract a secret through the agent broker in a prompt-injection test.
- The second audit has no open high or critical findings.

### Later

- **Agentic password change.** Start with `/.well-known/change-password` links, then signed per-site recipes, and only then an AI fallback that navigates but never sees the secret. Use a two-phase commit (save the pending password, change it, re-check the login, then promote it) with rollback. Apple announced this for iOS 27 but delayed it past 27.0; as of 15 October 2026 it has not shipped.
- **Remote credential exchange** across platforms, once the transfer protocol (CXP) stabilizes.
- **First-party email aliases,** which also unlock safe magic-link handling.
- **Short-lived SSH certificates** issued after a passkey approval.
- **Collaborative notes** that merge offline edits without conflict copies.
- **A public binary-transparency log** that apps check before unlocking.

### Never

- Hidden vaults that pretend to be empty. Sync traffic and the OS leak their existence, the law can compel disclosure, and a coercer can never be satisfied. Ship the visible panic lock instead.
- Emulation of private vendor push channels.
- Reading Gmail on Lockdown's servers.
- Automatically following email magic links.
- Automatically confirming Steam trades.

### What to cut first if v1 slips

1. Web vault editing (keep read-only).
2. Share links to non-users.
3. The third breach playbook.
4. Windows desktop (Mac, mobile and extensions first).
5. Self-hosting at launch. Keep the single-binary architecture, but publish the self-host image a quarter later.

Never cut the cryptography, the audit, credential-exchange import, or autofill quality.

---

## 7. Open questions and risks

### 7.1 Facts that are uncertain, disputed or corrected

| Item | What we know | Status | Next step |
|---|---|---|---|
| Valve's stance on third-party Steam authenticators | No legal action or bans found. Valve keeps adjusting the confirmation endpoints. | **Uncertain** (absence of evidence) | Legal review. Watch steamguard-cli's issue tracker. |
| Steam's 2-day hold for transfers | Comes from steamguard-cli source and community guides | **Uncertain** (Valve's support page could not be reached) | Test with a real account. |
| Battle.net passkey launch date | Directory entry added 4 May 2026 | **Uncertain** | Read Blizzard support article 380922. |
| Battle.net secret extraction | Users report the web flow working on 10 September 2026, with some failures | Confirmed working; ToS risk unresolved | Decide not to build an extractor. |
| Adobe's removal of authenticator-app codes | Community reports only | **Uncertain** | Check a fresh Adobe account. |
| Riot's two-factor options | The fact-check found authenticator apps in Riot's current MFA. The gap report's "email only" is out of date. | Corrected | None. |
| Roblox authenticator support | Carried from earlier research | Not re-checked | Check Roblox's help page. |
| Android third-party providers answering desktop QR codes | Works on Android 14 and later per Bitwarden, 1Password and Thales. Some manufacturers' devices lack support. | Confirmed with gaps | Device lab testing. |
| Who appears in Apple's "Export to another app" list | A correctly set-up third-party manager reportedly did not appear. The rules are undocumented. | **Uncertain** | Build early; contact Apple developer relations. |
| Credential exchange on macOS 26 | The interface exists; third parties barely use it. 1Password was not an export target on Mac as of February 2026. | Confirmed thin | Lead with iOS and Android. |
| Credential exchange transfer protocol (CXP) | Still the October 2024 Working Draft, which says it is not a basis for implementation. The data format (CXF) became a Proposed Standard on 14 August 2025. Shipping systems use same-device, OS-mediated transfers. | Confirmed | Build on CXF and the OS interfaces only. |
| Wi-Fi items in credential exchange | The format defines Wi-Fi. Whether Apple's flow drops it is unknown. Apple's CSV export does drop it. | Partly uncertain | Test, and keep Wi-Fi in Lockdown's own exports. |
| Passkeys with signature counters failing to transfer | Only Bitwarden says so. Apple's passkeys don't use counters. | **Uncertain** | Test. |
| 1Password passkey-only unlock in 2026 | Latest staff word (November 2025): still beta | **Uncertain** | Re-check before positioning against it. |
| NDSS 2025 paper on desktop IPC theft | No title or authors found | **Uncertain** | Find the paper before citing it. |
| Keeper Forcefield (memory-scraping protection) | Launch date and scope conflict | **Uncertain** | Do not cite. |
| "No manager wraps sharing keys post-quantum" | True for 1Password, Keeper, Bitwarden and Proton Pass. NordPass and Dashlane unchecked. | Likely but unproven | Check before saying "first." |
| Bitwarden's clickjacking fix | Bitwarden says it is fixed. One summary suggests Tóth's list still shows it unfixed. | Disputed | Read marektoth.com directly. |
| Bitwarden's ETH response ("7 fixed, 3 design decisions") | Secondary source only. Bitwarden says all were addressed. | **Uncertain** | Read the paper and Bitwarden's post. |
| Apple's AI password change | Announced June 2026, delayed past iOS 27.0, may come in 27.1 | Corrected | Watch iOS 27.1. |
| Proton Pass SSH agent | The research said it hadn't shipped. It has. | **Refuted** claim, corrected | Count it as a parity feature. |
| 1Password Environments "GA in August 2026" | Launched the week of 28 July 2026. Local `.env` mounting is still labelled beta and works on Mac and Linux only. | Corrected | None. |
| 1Password native Mac AutoFill "still beta" | Generally available since 11 August 2026 | Corrected | Treat it as the bar on Mac. |
| Bitwarden Authenticator sync | Syncs with the Bitwarden app on the same device; no independent cloud sync | Corrected | None. |
| Windows plug-in order | 1Password and Bitwarden were announced the same day. The sequencing claim was wrong. | Corrected | None. |
| When Bitwarden's Windows passkey plug-in left beta | Beta-only at launch on 11 November 2025 (installed from GitHub) | **Uncertain** | Check before citing Bitwarden as a stable Windows provider. |
| 1Password passkey transfer on Android | Google lists 1Password as a launch partner of the September 2026 switch flow. One outlet says its passkey transfer was still pending. | Disputed | Test a passkey round trip before claiming it. |
| Android OTP notification hiding and SMS User Consent details | From secondary research and background knowledge | **Uncertain** | Read Google's docs before the Android OTP spec. |
| RFC 8628 section 5.4 wording | Not fetched | **Uncertain** | Quote the RFC directly. |
| Gmail CASA requirement | Quoted from Google policy via third-party repositories | Mostly confirmed | Confirm on Google's site. |
| Edge passkey sync beyond Windows | Only a secondary blog claims it | **Uncertain** | Not needed for v1. |
| Samsung Pass for Windows and passkeys | Unconfirmed | **Uncertain** | Not needed for v1. |

### 7.2 Product and business risks

- **Autofill quality decides everything.** It is the top complaint about every competitor. Budget for a dedicated autofill engineer and the site test set from the start.
- **A new brand must earn trust.** Users trust incumbents by default. Audits, open source and the malicious-server analysis help. One serious bug early would hurt badly.
- **Free built-ins keep improving.** Apple's delayed AI password change will likely ship in a later iOS 27 update. Google is testing a similar feature.
- **Price pressure.** Bitwarden is $19.80 a year, Proton Pass Plus is about $2.49 to $2.99 a month (sources conflict), and Apple and Google are free.
- **The Secret Key versus ease of use.** It is the right default, but it causes lockouts if onboarding is weak. The recovery drill is the mitigation.
- **Approval fatigue.** Every approval prompt teaches people to tap "yes." Approve per task, not per action, and use number matching.
- **Platform policy.** Apple's export-list rules are undocumented. Google Play restricts accessibility-service autofill and SMS permissions. Either platform can change its rules.
- **Legal exposure from the Steam bridge.** See 2.10.
- **Licence choice.** 2FAS shows that a source-available licence (BUSL) and proprietary graphics weaken trust claims. Decide early: an OSI-approved licence for the apps, and a licence for the server that keeps self-hosting free.
- **Succession.** Raivo OTP was sold and an update made codes disappear. Buttercup was abandoned. Kaspersky moved US users to another product without real consent. Publish a commitment: free export forever, in-app notice of any ownership change, and no custody change without consent.
- **Small-team burnout.** Every platform interface (iOS, Android, Windows, three browsers, Steam) changes yearly. Keep the platform list short in v1.

---

## 8. Sources

**Standards and platform interfaces**
- WebAuthn Level 3 Recommendation (25 August 2026): https://www.w3.org/TR/2026/REC-webauthn-3-20260825/
- Apple forums, hybrid for credential providers (Apple staff, June 2026): https://developer.apple.com/forums/thread/829541
- Apple conditional passkey registration: https://developer.apple.com/documentation/authenticationservices/ascredentialproviderviewcontroller/performwithoutuserinteractionifpossible(passkeyregistration:)
- Apple credential export and import: https://developer.apple.com/documentation/authenticationservices/ascredentialexportmanager.md and https://developer.apple.com/documentation/authenticationservices/ascredentialimportmanager.md
- Android Credential Manager FAQ: https://developer.android.com/identity/sign-in/credential-manager-faq
- Android credential transfer: https://developer.android.com/identity/sign-in/credential-transfer
- Google, switching password managers on Android (10 September 2026): https://blog.google/products-and-platforms/platforms/android/switch-password-managers/
- Windows third-party passkey providers: https://raw.githubusercontent.com/MicrosoftDocs/windows-dev-docs/docs/hub/apps/develop/security/third-party.md
- passkeys.dev platform references: https://raw.githubusercontent.com/passkeydeveloper/passkeys.dev/main/content/en/docs/reference/ios.md
- Signal API compatibility data (MDN): https://raw.githubusercontent.com/mdn/browser-compat-data/main/api/PublicKeyCredential.json
- FIDO credential exchange format (Proposed Standard): https://fidoalliance.org/specs/cx/cxf-v1.0-ps-20250814.html
- FIDO credential exchange protocol (Working Draft): https://fidoalliance.org/specs/cx/cxp-v1.0-wd-20241003.html
- RFC 8628, device authorization grant: https://datatracker.ietf.org/doc/rfc8628/
- RFC 9807, OPAQUE: https://www.rfc-editor.org/rfc/rfc9807.html
- RFC 9980, post-quantum OpenPGP: https://www.rfc-editor.org/rfc/rfc9980.html
- MCP 2025-11-25 elicitation spec: https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/docs/specification/2025-11-25/client/elicitation.mdx

**Vendor bridges**
- steamguard-cli (source for QR approval, transfer, holds, imports): https://github.com/dyc3/steamguard-cli
- node-steam-session (QR approval in Node): https://github.com/DoctorMcKay/node-steam-session
- python-bna issues on Battle.net extraction: https://github.com/jleclanche/python-bna/issues/42
- Adobe passkeys overview: https://helpx.adobe.com/account/individual/sign-in-and-security/sign-in-methods/passkeys-overview.html
- Adobe community thread on authenticator apps: https://community.adobe.com/t5/creative-cloud-desktop-ideas/third-party-authenticator-apps/idc-p/14357562
- Community passkeys directory: https://github.com/2factorauth/passkeys
- Discord console account linking (device code): https://github.com/discord/discord-api-docs
- Microsoft Entra authentication flows (device-code risk): https://github.com/MicrosoftDocs/entra-docs
- PlayStation passkeys: https://www.playstation.com/passkey/
- GOG authenticator apps: https://www.gog.com/en/news/you_can_now_use_authenticator_apps_to_keep_your_gog_account_secure
- Riot MFA: https://support-valorant.riotgames.com/hc/articles/4415709691667

**Competitors**
- 1Password price change: https://9to5google.com/2026/02/24/1passwords-annual-subscription-plans-are-getting-a-price-hike-next-month/
- 1Password Mac AutoFill for everyone: https://www.1password.community/announcements-52/macos-autofill-is-now-available-to-everyone-25254
- 1Password September 2026 update (Universal sign-in, 1Password 7 retirement): https://www.1password.community/announcements-52/september-2026-at-1password-universal-sign-in-for-everyone-retiring-1password-7-and-a-quick-access-change-on-macos-25734
- 1Password for Claude (16 July 2026): https://www.techrepublic.com/article/news-1password-claude-passwordless-sign-in-ai-agents/
- 1Password post-quantum TLS: https://1password.com/blog/post-quantum-cryptography
- Bitwarden 2026 plan changes: https://www.businesswire.com/news/home/20260121289564/en/Bitwarden-Enhances-Premium-and-Families-Plans-With-Proactive-Security-Protections-for-All-Users
- Bitwarden passkey login and unlock: https://bitwarden.com/help/login-with-passkeys/
- Bitwarden portability blog: https://bitwarden.com/blog/security-vendors-join-forces-to-make-passkeys-more-portable-for-everyone/
- Bitwarden SDK (crypto algorithms): https://github.com/bitwarden/sdk-internal
- Proton Pass desktop changelog (SSH agent): https://github.com/ProtonMail/WebClients/blob/main/applications/pass-desktop/CHANGELOG.md
- Proton Pass CLI changelog: https://github.com/protonpass/pass-cli/blob/main/CHANGELOG.md
- Proton Pass security model: https://proton.me/blog/proton-pass-security-model
- Vaultwarden releases and wiki: https://github.com/dani-garcia/vaultwarden/releases and https://github.com/dani-garcia/vaultwarden/wiki
- Keeper post-quantum (Commander): https://docs.keeper.io/keeperpam/commander-cli/commander-installation-setup/quantum-resistant-cryptography-qrc
- Apple iOS 27 delayed features: https://www.macrumors.com/2026/09/11/ios-27-delayed-features/
- Apple Passwords shared groups: https://support.apple.com/en-us/120758
- Microsoft Authenticator autofill retirement: https://support.microsoft.com/en-au/account-billing/changes-to-microsoft-authenticator-autofill-09fd75df-dc04-4477-9619-811510805ab6
- Passbolt changelog (expire on permission loss): https://github.com/passbolt/passbolt_api/blob/master/CHANGELOG.md
- Aegis vault design and changelog: https://github.com/beemdevelopment/Aegis/blob/master/docs/vault.md
- Ente Auth features: https://github.com/ente-io/ente/blob/main/docs/docs/auth/features/index.md
- Padloc security design: https://github.com/padloc/padloc/blob/main/security.md

**Security research**
- ETH Zurich and USI, malicious-server analysis: https://eprint.iacr.org/2026/058 and https://www.usenix.org/conference/usenixsecurity26/presentation/scarlata
- 1Password's response: https://1password.com/blog/eth-zurich-zero-knowledge-malicious-server-review
- Bitwarden's response: https://bitwarden.com/blog/security-through-transparency-eth-zurich-audits-bitwarden-cryptography/
- DOM-based extension clickjacking (Tóth): https://marektoth.com/blog/dom-based-extension-clickjacking/
- 1Password autofill confirmation: https://support.1password.com/autofill-confirmation/
- Meta's encrypted-backup update (OPAQUE with hardware): https://engineering.fb.com/2026/05/01/security/meta-strengthening-end-to-end-encrypted-backups/
- OPAQUE security analysis: https://eprint.iacr.org/2023/843
- LastPass breach thefts (Krebs): https://krebsonsecurity.com/2025/03/feds-link-150m-cyberheist-to-2022-lastpass-hacks/

**Feature data sources**
- JustDeleteMe data (MIT): https://github.com/jdm-contrib/jdm
- SLIP-39 spec: https://github.com/satoshilabs/slips/blob/master/slip-0039.md
- Wi-Fi QR format: https://github.com/zxing/zxing/wiki/Barcode-Contents
