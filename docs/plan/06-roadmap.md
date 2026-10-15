# Lockdown — roadmap under D8 and D9

_Status: plan set v0.1, 2026-10-15. Re-estimates brief §6 after D8 (vendor bridges parked) and D9 (feature-complete v1). Counts come from `../research/2026-10-15/feature-matrix.json` (201 features)._

## For future agent

- **TL;DR:** D9 moves v1 from a wedge (108 matrix rows) to parity-plus (122 rows). D8 parks 5 vendor-bridge rows in a "maybe" backlog. v1 is now a parallel build of ten workstreams, about 155 person-months before contingency. The brief's 6–8 engineers would take 20–24 months. The recommended team is 10–12 engineers with a staged ramp, landing v1 in 15–18 months from an engineering start of January 2027, so April to July 2028.
- **Read with:** `decisions.md` (D1–D14), `01-identity.md` §10 (Mailbird builds the crate first), brief §6 (the roadmap this replaces).
- **Nothing here is built.** The first dependency is outside this repo: the identity crate in Mailbird.

## 1. What D9 changes

The brief planned a thin v1 and a longer tail. D9 says v1 must be the most feature-complete app on day one. In matrix terms that means three things.

1. Every `parity` row for the personal and family product moves into v1. Eight rows move from v1.x.
2. `beat` rows that are cheap when built beside their v1 neighbours move too. Four rows move.
3. The ecosystem pieces join v1 even though they are not matrix rows: consuming the identity crate (D10–D12), enclave pairing (D4), and the locked-notes recovery path (D5). The TV device-code flow (two rows) is in v1 **under review** (D8 keeps it as a standards feature, not a vendor bridge).

D8 removes the Steam QR approval, Steam trade confirmations, Steam enrolment, Battle.net import, and the fenced bridge host from the plan. They go to the "maybe" backlog. Steam Guard codes and Steam file import stay in v1. They are standard code generation and a file parser, not a bridge.

Three groups of `parity` rows do **not** move into v1, and this is a judgement call Sean should confirm (open question 1):

| Group | Rows | Why they stay put |
|---|---|---|
| Developer tooling (SSH agent, CLI, secret references, shell plug-ins, secret scanner, Linux Secret Service, Git signing) | 7 in v1.x | A different audience. They do not block "move from 1Password in ten minutes." Brief §6 and this plan's milestone map keep them in v1.x. |
| Business/admin (SSO, SCIM, policies, roles, compliance, device trust, shadow-IT discovery) | 5 in v2, 2 in later | A different product tier. v2 is "teams and agents." |
| Singletons: standalone authenticator app (v1.x), PIN-protected escrow (v2), first-party aliases and portable mode (later) | 4 | A second app, an HSM fleet, a mail platform, and a packaging mode. Each is a project, not a feature, and each has a v1 row that covers the user need (free codes in the main app, the recovery menu, third-party aliases, the self-host binary). |

### Phase counts after D8 and D9

| Category | v1 | v1.x | v2 | later | maybe | total |
|---|---|---|---|---|---|---|
| Vault & item types | 12 | 2 | 1 | 0 | 0 | 15 |
| Organization & search | 7 | 0 | 2 | 0 | 0 | 9 |
| Autofill (browser) | 9 | 2 | 0 | 0 | 0 | 11 |
| Autofill (mobile & desktop OS) | 6 | 2 | 1 | 1 | 0 | 10 |
| Passkeys & WebAuthn | 8 | 1 | 0 | 0 | 0 | 9 |
| Two-factor & authenticator | 4 | 3 | 0 | 1 | 0 | 8 |
| Vendor login bridges | 7 | 0 | 0 | 0 | 5 | 12 |
| Sharing & collaboration | 6 | 1 | 1 | 1 | 0 | 9 |
| Family & emergency/recovery | 5 | 4 | 1 | 0 | 0 | 10 |
| Security monitoring & phishing defense | 8 | 4 | 0 | 0 | 0 | 12 |
| Privacy | 2 | 3 | 1 | 1 | 0 | 7 |
| Import/export & portability | 8 | 1 | 0 | 1 | 0 | 10 |
| Developer & secrets | 0 | 9 | 3 | 1 | 0 | 13 |
| AI & agents | 0 | 0 | 8 | 1 | 0 | 9 |
| Business/admin | 0 | 0 | 8 | 2 | 0 | 10 |
| Self-hosting & deployment | 4 | 1 | 1 | 0 | 0 | 6 |
| Security architecture & crypto | 17 | 1 | 0 | 0 | 0 | 18 |
| Platforms & apps | 9 | 0 | 1 | 1 | 0 | 11 |
| Trust & transparency | 10 | 0 | 1 | 1 | 0 | 12 |
| **Total** | **122** | **34** | **29** | **11** | **5** | **201** |

The matrix had 108 / 53 / 29 / 11 / 0. Net: v1 +14, v1.x −19, maybe +5. The v1 column holds 69 parity, 43 beat and 10 novel rows. Two of the 122 (`device-code-approve`, `proximity-gate`) are under review and are the first cut in every slip scenario.

### Every move, one line each

| Feature | Matrix | Roadmap | Why |
|---|---|---|---|
| HTTP Basic Auth fill (`basic-auth-fill`, parity) | v1.x | v1 | D9 parity; a small extension auth handler |
| Fill into any Mac app (`macos-any-app`, parity) | v1.x | v1 | D9 parity; desktop workstream |
| Windows 11 passkey plug-in (`windows-passkey-plugin`, parity) | v1.x | v1 | D9 parity; first to slip to v1.x if desktop runs late |
| Request a secret (`request-link`, parity) | v1.x | v1 | D9 parity; reuses the Send code path |
| Email breach monitoring (`email-breach-monitoring`, parity) | v1.x | v1 | D9 parity; a server job on the same breach feed |
| Linux app (`linux-app`, parity) | v1.x | v1 | D9 parity; Tauri builds it; Secret Service and SSH stay v1.x |
| Open-source server (`oss-server`, parity) | v1.x | v1 | D9 parity and D1 layer C; a licence choice, not a build |
| Data residency choice (`data-residency`, parity) | v1.x | v1 | D9 parity; cheap only if regions are designed in from day one |
| Item expiry reminders (`item-expiry`, beat) | v1.x | v1 | D9 beat; the dates already live in typed templates |
| Published backup format (`open-format`, beat) | v1.x | v1 | D9 beat; publish it with the white paper |
| Automatic local backups (`auto-backups`, beat) | v1.x | v1 | D9 beat; schedules the v1 encrypted export; D7 wants backup-friendly |
| Per-item exposure tiers (`exposure-tiers`, beat) | v1.x | v1 | D9 beat; the high-risk class is already one tier |
| TV and console activation by scan (`device-code-approve`, novel) | v1.x | v1 (review) | D8 keeps it as a standards feature; under review |
| Anti-phishing approval gate (`proximity-gate`, novel) | v1.x | v1 (review) | ships with device-code-approve; also guards passkey QR |
| Steam QR login approval (`steam-qr-approve`) | v1.x | maybe | D8 vendor bridge |
| Steam trade and market confirmations (`steam-confirmations`) | v1.x | maybe | D8 vendor bridge |
| Steam authenticator enrol or move (`steam-enrol-transfer`) | v1.x | maybe | D8; calls Steam's enrolment API |
| Battle.net authenticator import (`battlenet-import`) | v1.x | maybe | D8 vendor bridge |
| Fenced bridge plug-in host (`bridge-plugin-host`) | v1.x | maybe | D8; it only exists to host parked bridges. D6's general plug-in model is unaffected. |

The full per-category assignment of all 201 rows is in the appendix.

## 2. Workstreams for v1

D9 makes v1 a parallel build, so this section is organised by workstream, not by phase. Estimates are person-months (PM) of focused work by someone who already knows the stack. They exclude hiring, holidays and the beta tail. Section 3 adds contingency.

| # | Workstream | Scope | Depends on | PM | Reasoning | Exit test |
|---|---|---|---|---|---|---|
| 1 | **Core + crypto** (Rust) | Vault format, 25+ typed templates, custom fields, attachments with per-file keys, key tree, OPAQUE login, Argon2id with floors, HPKE hybrid X25519 + ML-KEM-768 wrapping, bound AEAD, anti-rollback manifests, key-transparency client, sync engine with offline edits and conflict copies, search index, generator with CI randomness tests, TOTP and Steam codes, breach checks, health reports, 12 importers plus OTP importers in sandboxed parsers, CXF lossless mapping, exporters, UniFFI and WASM bindings | Identity crate API (workstream 2) for key wrapping | **30** | Crypto 6, sync and manifests and KT client 6, model and templates and search 5, import and export 6 (lossless round-trip is where importers get expensive), codes and health 3, bindings and CI 4 | Conformance suite passes on all bindings; every item type round-trips through CXF with zero loss; the formal model (workstream 8) matches the code |
| 2 | **Identity** (consumed) | Embed the crate Mailbird builds (D12). Detection and attach, adoption as manager (D10), recovery kit and drill (D11), device enrolment by QR plus short code, enclave pairing and scoped grants (D4), locked-notes key wrapped to the identity key with a Lockdown recovery path (D5) | Mailbird crate §10 steps 1–3 done and extracted | **9** | Consumption and adoption 4, pairing 2, notes path 1, recovery UX on every platform 2. Lockdown writes no crate code; it writes the reference consumer and the conformance CI | A Mailbird-created identity is adopted on all five platforms without migration; a locked Mailbird note opens with the Lockdown kit on a fresh device |
| 3 | **iOS** | SwiftUI app, AutoFill credential provider, one-time-code provider (iOS 18+), passkey provider with conditional create, CXF import and export managers, App Group identity root, clipboard and screenshot hygiene | Core bindings | **15** | One engineer for the build; the providers are three separate extensions with small memory limits, each a month of edge cases | Passkeys register and sign in on iOS 17, 18 and 26; AutoFill and codes appear in the keyboard bar; CXF import from Apple Passwords is lossless |
| 4 | **Android** | Compose app, AutofillService with inline suggestions, CredentialProviderService, Credential Manager passkeys, CXF provider events, ContentProvider identity root with signature permission, F-Droid reproducible build | Core bindings | **15** | Same shape as iOS plus an OEM device matrix. Samsung and Pixel behave differently on inline suggestions and third-party providers | Passkeys pass on Android 14, 15 and 16 on Pixel and Samsung; Chrome's "autofill with another service" onboarding completes in under a minute in testing |
| 5 | **Desktop** (Tauri) + macOS AutoFill extension + Windows passkey plug-in | Tauri 2 app for Mac, Windows and Linux; quick-access palette; offline access; macOS credential provider with passwords, passkeys and codes; fill into any Mac app behind a code-signature check; Windows COM `IPluginAuthenticator` plug-in with Windows Hello, MSIX packaged; identity root ACLs | Core bindings; extension native messaging | **22** | Tauri shell 8, macOS extension 4, Windows plug-in 5 (COM and MSIX are unfamiliar to most teams), Linux build 2, any-app fill 2, Windows Hello and keystore 1 | Windows plug-in passes on current Windows 11; the Mac extension fills a code without the main app open; Linux runs on GNOME and KDE |
| 6 | **Extension** | MV3 for Chromium and Firefox, Safari Web Extension, one WASM core; inline menu in a closed shadow root in the top layer; clickjacking defence on by default; exact-host URL matching with curated groups; save and update prompts; per-site fixes synced; card and address fill; Basic Auth; passkey fallback with a coexistence convention; Signal API with undo; lookalike-domain hard stop; hotkeys; native messaging with peer authentication | Core WASM; QA test set (workstream 10) from month 3 | **18** | This is where autofill quality lives. Plan on three full passes over the 500-site set. Safari costs a third extra for every feature | 95% success on the 500-site set in all four browsers; the clickjacking red-team suite passes; no cross-domain fill without an explicit confirm |
| 7 | **Server + self-host** | axum + sqlx, OPAQUE server, sync and manifest storage, key-transparency log, Send and request links, family plan and billing, breach monitoring job, push relay, WebSocket live sync, single ~50 MB container with SQLite or Postgres and S3 attachments, US and EU regions, feature flags not forks, signed releases and SBOM pipeline, web vault hosting | Core wire format | **16** | One engineer. The KT log and the two-region design are the two pieces that cost more if added later, so they go first | The same image runs in cloud and self-host with one flag; a restore drill from an encrypted backup succeeds; security releases reach both the same day |
| 8 | **Security program** | Threat model to the 2026 ETH Zurich study; formal Tamarin or ProVerif model of login, sharing and recovery; malicious-server analysis published; two-firm audit (crypto and clients, server and mobile); public bounty with malicious-server scenarios in scope; reproducible builds with an independent rebuilder; cargo-vet and cargo-deny; readable white paper | Core API freeze for the model; beta build for the audit | **8** internal + contract | Internal: model liaison 2, audit response 3, bounty and rebuilder 2, white paper 1. Contract: cryptographer for the formal model (about 3 PM) and two audit firms (budget line, not PM) | Audit has no open high or critical finding; the formal model finds no attack the design did not expect; the analysis is published before v1 |
| 9 | **Design** (Unicron components) | Tokens in DTCG JSON with Swift, Kotlin and CSS generated; three-tier tokens with light, dark and high-contrast modes; the six security components from brief §5.7: approval sheet with number matching, risk-tier badge, secret field with clipboard timer, code display with countdown and early next code (6, 8 and Steam 5-character), TV large-type view, crown-jewel styling web content cannot imitate; hand-offs to surfaces we cannot style; WCAG 2.2 AA | D14 confirmed (shared design system) | **10** | Sean as design lead part-time across the build. The six security components are one spec and three builds each; that is most of the cost. Everything else is platform convention | Each security component has a spec, three builds, Code Connect links and screenshot tests per platform and web engine |
| 10 | **QA** | Build and maintain the 500-site autofill test set with an automated browser lab; a passkey device lab (iOS 17, 18, 26; Android 14, 15, 16 on Pixel and Samsung; four desktop browsers); an import round-trip corpus from five managers; recovery-drill usability tests (10 participants); privacy-preserving crash telemetry | Extension from month 3; mobile from alpha | **12** | One QA engineer from month 2. The test set is a product in its own right; the brief says Lockdown builds and maintains it | The set runs nightly on all browsers; the device lab covers the full matrix; crash-free rate is measured, not estimated |

**Total: about 155 PM.** Engineering (workstreams 1–8): 133 PM. Design and QA: 22 PM. Add 25% contingency to engineering for a first-time team on a security product: about 166 engineering PM, 190 PM in all.

Critical path: identity crate (Mailbird) → core API freeze (month 4) → shells at full speed → beta build → audit (8–12 weeks plus a fix cycle) → v1. More engineers shorten the middle. Nothing shortens the ends.

## 3. Staffing scenarios

All dates assume engineering starts January 2027, with hiring in Q4 2026 and the identity crate maturing in Mailbird over the same quarter. The brief's rule holds in every scenario: **never cut the cryptography, the audit, credential-exchange import, or autofill quality.**

| Scenario | Team | Engineering PM available in 18 months | v1 date range | What gets cut first |
|---|---|---|---|---|
| (a) As the brief assumed | 6–8 engineers, design lead, QA, contracted audit | 108–144 | **20–24 months → September 2028 to January 2029** | 1. TV flow to v1.x. 2. Windows passkey plug-in to v1.x (Windows app ships without it). 3. Linux app to v1.x. 4. EU region a quarter after launch. 5. Email breach monitoring to v1.x. 6. Fill into any Mac app to v1.x. That is six rows back to the matrix's own phases and still 20 months. |
| (b) Parallel build | 10–12 engineers, design lead, 1–2 QA, contracted audit and cryptographer | 180–216 | **15–18 months → April to July 2028** | Nothing planned. If the TV review fails its red-team phishing test, it moves to v1.x. The Windows plug-in is the one row allowed to slip to a v1.1 point release if the MSIX path fights back. |
| (c) Small team | 4–5 engineers, design lead, QA shared, contracted audit | 72–90 | **28–34 months → May to November 2029**, and only after cutting about a quarter of v1 | Everything in (a), plus: Windows desktop entirely to v1.x (brief cut 4), web vault read-only (cut 1), share links and Send to v1.x (cut 2), playbooks reduced from seven to three (cut 3), self-host image a quarter later (cut 5), attachments capped small, typed templates reduced to ten. This is the brief's wedge, delivered later than the brief planned it. It does not satisfy D9. |

**Recommendation: (b), with a staged ramp.** Start in January 2027 with six people: two core and crypto, one extension, one identity and server, one iOS, one QA, plus Sean on design. The core API cannot absorb more hands before it freezes, and the identity crate is still in Mailbird's repo. Ramp to eleven at the API freeze around month 4: add Android, a second desktop engineer, a second extension engineer, a second server engineer, and a security engineer. Reasons:

1. D9 is a scope decision. (a) answers it with a two-year v1. The research this plan rests on is dated October 2026. By January 2029 the competitive facts have moved and the parity target has moved with them.
2. After the API freeze the platform work is almost perfectly parallel. That is the case where adding people does shorten the schedule.
3. (c) cannot deliver D9 at all. If (c) is the real budget, the honest move is to reopen D9, not to pretend.
4. The ramp keeps the first four months cheap, so the decision to go to eleven can wait for the alpha evidence.

Cost of (b) over (a): roughly 40 extra engineering PM, bought back as 5–9 months of calendar.

## 4. Milestones

Dates are for scenario (b). Shift them by the ranges in section 3 for the others.

| Milestone | When (b) | Contents | Exit criteria |
|---|---|---|---|
| **Alpha** | Month 7, August 2027 | Core and crypto feature-complete for logins, notes, cards, codes; identity crate consumed on macOS and iOS; iOS app with AutoFill and passkeys; extension on Chromium; server in one region | Core conformance suite green; a Mailbird identity adopted on iOS without migration; iOS passkey round-trip on iOS 17, 18 and 26; recovery kit rebuilds a vault on a wiped phone; 50 internal users for 30 days |
| **Beta** | Month 12, January 2028 | All five platforms, all four browsers, all item types, sharing and family, importers and CXF, self-host image, web vault; design system components landed on all three UI stacks; audit kickoff; bounty opened privately | Audit has started on a frozen crypto layer; 500-site set at 90% in all browsers; round-trip import from five managers loses no supported item; crash telemetry live; 1,000 external testers |
| **v1** | Month 15–18, April to July 2028 | Everything in the v1 column of section 1; TV flow if its review passed; Windows plug-in if MSIX cooperated | Audit clean (no open high or critical); malicious-server analysis published; **95%** autofill success on the 500-site set; lossless round-trip imports; **9 of 10** participants recover from the kit alone; **99.5%** crash-free sessions in public beta |
| **v1.x** "Sign-in hub" | 6–9 months after v1 | TV activation if not in v1; Windows plug-in if not in v1; SSH agent, Git signing, CLI, secret references, env mounts, machine tokens, shell plug-ins, secret scanner, Linux Secret Service; PRF unlock and two-key mode; digital legacy, social recovery, kid roles, organizer recovery; item version history and share activity log; standalone authenticator app with sync and live code share; travel mode, panic lock, duress PIN; phishing blocker, passwordless coaching, connected-app review, deletion helper; learned field detection behind a flag; SMS code relay; desktop auto-type; desktop CXF; local-only mode; seed-phrase safe mode | TV works on 5 of 7 listed services and passes the red-team phishing test; Windows plug-in on current Windows 11; learned autofill beats the heuristics on the set before it is default |
| **v2** "Teams and agents" | 9–12 months after v1.x | SSO unlock, SCIM, policies, roles, event logs, admin recovery, just-in-time access, risk analytics, compliance certifications; MCP broker, per-task approval, scoped agent tokens, agent audit log, deny classes, screen masking, hosted-browser agents; watch apps; email code capture on device; document scan; natural-language search; context profiles; age and GPG agent; CI OIDC and DevOps integrations; PIN escrow; rotate on revoke; virtual cards; BYO storage sync; second audit | Five paying pilots complete SSO and SCIM without engineering help; a red team cannot extract a secret through the broker; second audit clean |
| **Later** | Unscheduled | Agentic password change (tiered, two-phase commit); cross-device CXP transfer once the draft stabilises; first-party email aliases and magic-link handling; short-lived SSH certificates; collaborative notes; binary transparency; hardware-bound OATH; device trust; shadow-IT discovery; portable mode | Each gets its own exit test when scheduled |
| **Never** (from the brief) | — | Hidden vaults that pretend to be empty; emulation of private vendor push channels; reading Gmail on Lockdown's servers; automatically following magic links; automatically confirming Steam trades | — |
| **Maybe** backlog (D8) | Reopened only by a new decision | Steam QR login approval; Steam trade and market confirmations; Steam authenticator enrol or transfer; Battle.net authenticator import; the fenced bridge host; Discord QR | Preconditions to reopen: US and EU counsel sign-off, a kill switch in the plug-in model (D6), and 60 days of beta with consent logged for 100% of users |

## 5. Dependencies on sibling projects

| Project | What Lockdown needs | When | What happens if it is late |
|---|---|---|---|
| **Mailbird** | The identity crate (D12): records and envelopes with the conformance suite, create and local attach, the Phase 33 note-lock re-wrap, then extraction to its own repo (`01-identity.md` §10 steps 1–5). Mailbird's **Phase 37 should wait for it**: no cross-app consumer should start on a crate whose conformance suite is not yet green. | Steps 1–3 by the end of Q1 2027; extraction by the alpha (month 7) | Core keeps a test double behind the crate's API. Alpha ships with the double. Beta cannot: adoption without migration (D10) needs the real format. A one-quarter slip in the crate is a one-quarter slip in beta. |
| **Unicron** (shared design system, D14) | Token pipeline (DTCG JSON → Swift, Kotlin, CSS), the base component set on React 19 and Tailwind v4, and the six security components as specs the SwiftUI and Compose builds follow | Tokens by month 2; security component specs by month 5; all three builds by beta | Lockdown builds the security components locally and upstreams later. Cost: double work and drift. D14 is still **proposed** and conflicts with Synapse D87; until Sean confirms, this row is a risk, not a plan. |
| **DSA** | Pattern candidates for the security components (approval sheets, secret fields, code displays) feed Unicron before Lockdown's spec deadline | Month 4 | Unicron specs from first principles; DSA patterns land in a later revision |
| **Synapse** | Consumes the persona schema from the identity crate (D3, D13) rather than keeping its own. Lockdown needs Synapse to validate that the `Persona` record covers its cases before format v1 freezes. | Review before the crate's format freeze, Q1 2027 | A persona field added after the freeze is a format v2 and a migration for every app. Review early. |

## 6. Risks

| # | Risk | Likelihood | Impact | Early signal | Mitigation |
|---|---|---|---|---|---|
| 1 | Identity crate slips in Mailbird | Medium | High: beta blocked | Step 1 (records and envelopes) not green by end of Q1 2027 | Test double in core; a Lockdown engineer pairs on the crate from month 1; extraction criteria agreed up front |
| 2 | Hiring to eleven takes longer than the API freeze | High | Medium: shells start late | Fewer than eight signed by month 3 | Start recruiting in Q4 2026; contract specialists for the Windows plug-in and Safari extension |
| 3 | Autofill quality stalls below 95% | Medium | High: v1 gate | Set plateaus below 90% at beta | QA set from month 3; per-site fix list; learned classifier behind a flag early so it has data by v1.x |
| 4 | Audit finds a high in the crypto layer | Low–Medium | High: re-audit cycle | Formal model disagrees with the code at freeze | Freeze crypto at alpha; model it first; audit the frozen layer before beta rather than after |
| 5 | Apple or Google change provider APIs mid-build | Medium | Medium: platform rework | WWDC or Android release notes touching credential providers | Keep providers thin over the Rust core; budget one month per mobile platform per year for OS churn |
| 6 | Windows passkey plug-in (COM, MSIX) proves harder than five PM | Medium | Low: slips to v1.1 | No working Hello verification by month 10 | It is the one row allowed to slip; Windows app ships without it |
| 7 | D14 reversal not confirmed; Unicron and Lockdown drift | Medium | Medium: double design work | No decision by month 2 | Build security components in Lockdown with Unicron's token names; upstream when confirmed |
| 8 | TV device-code flow fails the phishing red team | Medium | Low: moves to v1.x | Red team gets a user to approve a pasted code | Already under review; the proximity gate refuses pasted and linked codes by design |
| 9 | Scope creep from "feature-complete" into v1.x rows | High | Medium: dates move | Any row added to v1 without a decision ID | The v1 column is frozen at alpha; additions need a new decision in `decisions.md` |
| 10 | Two-region data residency complicates key transparency | Low–Medium | Medium: late redesign | KT log design assumes one region at month 3 | Design the log as one global log with regional storage from the first server sprint |

## 7. Open questions for Sean

1. **Does D9's "full matrix" include the seven developer `parity` rows?** This plan keeps them in v1.x. Pulling them into v1 adds about 10 PM and one engineer.
2. **Confirm the staged ramp to 10–12.** If the real budget is (a) or (c), say so now; the plan then reopens D9 rather than stretching.
3. **Engineering start date.** January 2027 is assumed. Every date here moves with it.
4. **TV device-code flow: in or out of v1?** It is under review. A decision by alpha keeps the design work from being wasted.
5. **D14 and Synapse D87.** Unicron is on the critical path for the six security components. Which decision stands?
6. **Audit firms and the cryptographer.** Budget line and selection by beta minus six months, so month 6.
7. **Data residency at launch.** US and EU are in v1 by this plan. If EU can wait a quarter, say so; it saves server time early.
8. **Who owns the 500-site test set?** QA by this plan. It needs an owner from month 2, not from beta.

## Appendix: all 201 rows by category and phase

Rows marked `*` moved from the matrix's phase (section 1 explains each).

| Category | v1 | v1.x | v2 | later | maybe |
|---|---|---|---|---|---|
| Vault & item types | `login-items`, `secure-notes`, `payment-cards`, `identities`, `typed-templates`, `custom-fields`, `attachments`, `password-generator`, `wifi-items`, `item-expiry`*, `trash-recovery`, `item-archive` | `version-history`, `seed-safe-mode` | `document-scan` | — | — |
| Organization & search | `folders`, `tags`, `multiple-vaults`, `quick-access`, `smart-categories`, `duplicate-finder`, `recency-ranking` | — | `nl-search`, `context-profiles` | — | — |
| Autofill (browser) | `extension-coverage`, `inline-menu`, `clickjacking-defense`, `url-matching`, `autosave`, `site-fix-overrides`, `fill-hotkey`, `card-address-fill`, `basic-auth-fill`* | `ml-field-classifier`, `sign-in-method-picker` | — | — | — |
| Autofill (mobile & desktop OS) | `ios-provider`, `android-provider`, `autofill-health-check`, `ios-otp-provider`, `macos-autofill`, `macos-any-app`* | `desktop-autotype`, `sms-otp-relay` | `email-otp-capture` | `magic-link-handling` | — |
| Passkeys & WebAuthn | `passkey-sync`, `ios-hybrid`, `android-hybrid`, `windows-passkey-plugin`*, `extension-passkey-fallback`, `conditional-create`, `signal-api`, `passkey-management` | `prf-unlock` | — | — | — |
| Two-factor & authenticator | `totp-generator`, `totp-autofill`, `otp-importers`, `code-display-aids` | `authenticator-app`, `authenticator-sync`, `live-code-share` | — | `hardware-oath` | — |
| Vendor login bridges | `steam-guard-codes`, `steam-mafile-import`, `device-code-approve`* (review), `proximity-gate`* (review), `large-type`, `federated-login-map`, `service-2fa-guides` | — | — | — | `steam-qr-approve`*, `steam-confirmations`*, `steam-enrol-transfer`*, `battlenet-import`*, `bridge-plugin-host`* |
| Sharing & collaboration | `shared-vaults`, `granular-permissions`, `item-sharing`, `share-links`, `request-link`*, `fill-only-sharing` | `share-activity` | `rotate-on-revoke` | `collab-notes` | — |
| Family & emergency/recovery | `family-plan`, `emergency-access`, `recovery-kit`, `recovery-drill`, `password-reminder` | `kid-roles`, `digital-legacy`, `social-recovery`, `organizer-recovery` | `pin-escrow` | — | — |
| Security monitoring & phishing defense | `breached-passwords`, `weak-reused`, `email-breach-monitoring`*, `missing-2fa-report`, `breach-playbooks`, `lookalike-warning`, `change-password-link`, `account-alerts` | `passwordless-coaching`, `phishing-blocker`, `connected-app-review`, `deletion-helper` | — | — | — |
| Privacy | `alias-forwarders`, `clipboard-hygiene` | `travel-mode`, `panic-lock`, `duress-pin` | `virtual-cards` | `first-party-aliases` | — |
| Import/export & portability | `cxf-ios`, `cxf-android`, `lossless-transfer`, `importers`, `full-export`, `encrypted-export`, `open-format`*, `auto-backups`* | `cxf-desktop` | — | `remote-cxp` | — |
| Developer & secrets | — | `ssh-agent`, `commit-signing`, `cli`, `secret-injection`, `env-mounts`, `machine-tokens`, `shell-plugins`, `plaintext-secret-scan`, `secret-service` | `ci-oidc`, `devops-integrations`, `age-gpg-agent` | `ssh-certificates` | — |
| AI & agents | — | — | `agent-sign-in`, `per-task-approval`, `agent-audit-log`, `agent-tokens`, `mcp-server`, `agent-deny-classes`, `agent-screen-masking`, `hosted-browser-agents` | `agentic-password-change` | — |
| Business/admin | — | — | `sso-unlock`, `scim`, `admin-policies`, `event-logs`, `rbac`, `admin-recovery`, `jit-access`, `risk-analytics` | `device-trust`, `ai-tool-discovery` | — |
| Self-hosting & deployment | `official-self-host`, `single-container`, `saas-parity`, `self-host-push` | `local-only-mode` | `byo-storage-sync` | — | — |
| Security architecture & crypto | `e2ee`, `secret-key`, `pake-login`, `client-kdf`, `malicious-server-resistance`, `key-transparency`, `bound-encryption`, `anti-rollback`, `pq-transport`, `pq-key-wrapping`, `hardware-backed-unlock`, `account-security-keys`, `device-approval`, `memory-hygiene`, `ipc-peer-auth`, `high-risk-class`, `exposure-tiers`* | `hardware-key-factor` | — | — | — |
| Platforms & apps | `windows-app`, `macos-app`, `linux-app`*, `ios-app`, `android-app`, `web-vault`, `offline-access`, `non-electron-desktop`, `free-tier` | — | `watch-apps` | `portable-mode` | — |
| Trust & transparency | `oss-clients`, `oss-server`*, `published-audits`, `reproducible-builds`, `signed-releases`, `bug-bounty`, `security-advisories`, `exit-guarantee`, `data-residency`*, `security-whitepaper` | — | `compliance-certs` | `binary-transparency` | — |
