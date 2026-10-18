# 23 — Bridges, imports and monitoring

_Status: built on `slice/bridges`. Covers the checklist rows `otp-importers`, `steam-mafile-import`, `service-2fa-guides`, `federated-login-map`, `alias-forwarders`, `site-fix-overrides`, `breach-playbooks`, `change-password-link`, `code-display-aids`, `encrypted-export`, `auto-backups` (indicator and emergency sheet) and `email-breach-monitoring` (aliases). Builds on 13 (security center), 14 (import and export), 16 (browser extension), 17 (web vault) and 20 (self-host server)._

This slice joins Lockdown to what sits around it: the authenticator apps people are leaving, the sites they sign in to with a Google or Apple account, the alias services that hide their email, the directories that say which sites offer two-step sign-in, their account's own server clock, and the paper they keep in a drawer.

## 1. What each piece does, in one line

| Row | What the person gets |
|---|---|
| `otp-importers` | Import from Google Authenticator (the transfer QR text), Aegis, 2FAS, Ente Auth, Proton Authenticator and Bitwarden Authenticator, plain or password-locked, through the import wizard. |
| `steam-mafile-import` | Import Steam Guard from SDA (including passkey-encrypted folders), WinAuth and the Steam app's JSON. Codes work at once; the identity secret is kept as a protected note and never used. |
| `service-2fa-guides` | In Security, a set-up card for each login on a site that offers two-step sign-in or passkeys but has neither saved: what the site offers, its notes, Lockdown's steps, with both directories credited. |
| `federated-login-map` | Security shows which logins sign in through Google, Apple, Microsoft and the rest, what falls if that account falls, and what to fix first. The extension notices "Sign in with …" and the provider's own sign-in page. |
| `alias-forwarders` | Set up SimpleLogin, addy.io, Firefox Relay, Fastmail, DuckDuckGo or Forward Email once; make a new alias in the login editor or from the extension's popup on a sign-up page. |
| `site-fix-overrides` | "Fix this page" in the extension's popup: say which field is which, or never fill here. Kept on the site's logins, so it syncs. A signed community list sits under it. |
| `breach-playbooks` | Seven guided incident flows in Security ("A site I use was breached", "Someone got into my email", …), each a checklist filled in from the vault, with progress kept. |
| `change-password-link` | The extension's "Change password" opens the security center's checked link, or a curated page for sites without the standard one. |
| `code-display-aids` | Codes follow the account server's clock when this computer's is a little off, and say so when it is far off. |
| `encrypted-export` | Export locked to one or more age public keys (`age1…`); it opens with the private key in Lockdown or any age tool. |
| `auto-backups` | A last-backup line in the sidebar, outside Settings; a printable emergency sheet with no secret on it. |
| `email-breach-monitoring` | With alias monitoring on, every alias Lockdown made joins the addresses the server watches for breaches. |

## 2. Architecture

```
extension (MV3)                       desktop shell (Tauri)            core (lockdown-app)                 libraries
──────────────                        ─────────────────────            ───────────────────                 ─────────
content: fixes.ts, detect.ts  ─ops─►  browser/bridges.rs  ───────────► bridges/fixes.rs        ─────────► security::site_data (signed)
  "Sign in with …" clicks             (federation_hints, _seen,        bridges/federation.rs                 security::guide, incident,
background: bridges.ts                 site_fixes, fix_set,            bridges/aliases.rs                    alias, change_password
popup: Fix this page,                  change_link, alias_options,     bridges/incidents.rs                  codes::skew
  Hide my email,                       alias_create)                   bridges/clock.rs                      cxf::import::{otp, steam}
  Change password                                                      bridges/guides.rs                     cxf::age_file
                                      bridges.rs (20 webview          bridges/emergency.rs                  sync::transport (clock,
desktop webview                        commands, appended)            bridges/age_export.rs                   monitored)
  features/bridges/*  ──invoke──►                                     session/bridges.rs (vault access)
  (provider; web vault has none)                                                                           server: /v1/account/monitored,
                                                                                                             breach monitor scans aliases
```

- **The core decides; the shells and the extension ask.** Every new behaviour lives in `crates/app/src/bridges/` (one module per row) over `session/bridges.rs`, which is the only new code that touches vault items. The desktop's webview commands (`apps/desktop/src-tauri/src/bridges.rs`) and the extension's ops (`apps/desktop/src-tauri/src/browser/bridges.rs`) are thin.
- **Reserved fields.** What this slice keeps on an item rides in reserved `lockdown.*` fields: `lockdown.signin` (how a login signs in, from the extension), `lockdown.provider` (this login is the provider's own account), `lockdown.fill` (a fill fix) and `lockdown.alias` (an alias service's settings and what it made). Like the reserved fields before them, edits keep them, a shell can't name them, `show` never returns them, and they travel with the item's history, sync and export. A bug this exposed is fixed: `keep_all_but` named reserved fields back to the vault, so editing a login that carried one failed.
- **Signed site data.** One new bundle, `crates/security/data/site-data.signed.cbor`, in the matcher's signed-data envelope (Ed25519, domain-separated, `TrustedKeys::builtin()`, the `dev-2026-10` key), holds the set-up guides, the sign-in providers, the change-password recipes and the community fill fixes. `lockdown_matcher::signed` gained `sign_envelope` and `verify_envelope` so a second format reuses the same code; the matcher's own `sign`/`verify` now sit on them. It is rebuilt by `cargo run -p lockdown-security --example sign-site-data` and checked by the same example with `--check` (and a unit test).
- **Network off the worker.** The clock sample and the alias services' APIs are called on the calling thread, never the worker (as the breach check already was). Keys leave the worker only for the one request that needs them, in `Zeroizing` buffers.
- **The web vault** compiles the same feature views. It provides no bridges client, so the bridges sections hide there (`useBridges()` is `null`); the backup line says nothing because the web vault refuses `backup_status`. No `@tauri-apps/*` import is reachable from a feature folder: the desktop's client is `app/tauri-bridges.ts`, provided in `App.tsx`.

## 3. Authenticator and Steam imports

`crates/cxf/src/import/otp.rs` and `steam.rs`, nine new `Format`s (20 in all) detected from the file's bytes and offered by the wizard as before. Each entry becomes a Code item (`otpauth://` built from the secret, issuer, account, algorithm, digits, period or counter); duplicates fold as other imports do; anything the reader can't use is listed as a problem in words, never dropped silently.

| Format | Reads | Locked files |
|---|---|---|
| Google Authenticator | `otpauth-migration://offline?data=` lines (the transfer QR's text), batches of several | not locked |
| Aegis | the JSON vault, TOTP/HOTP/Steam (Yandex and mOTP reported) | scrypt + AES-256-GCM password slots |
| 2FAS | `.2fas` backups v3/v4 | PBKDF2-SHA-256 + AES-GCM |
| Ente Auth | the plain text export and the encrypted JSON | libsodium secretstream (XChaCha20-Poly1305), Argon2id key |
| Proton Authenticator | the JSON export | a locked export is refused in words (its format isn't published); export it unlocked |
| Bitwarden Authenticator | JSON and CSV | not locked |
| Steam (SDA) | a `.maFile`, or a zip of the `maFiles` folder with `manifest.json` | SDA's passkey: PBKDF2-SHA-1 (50 000) + AES-256-CBC per file |
| Steam app v2 / WinAuth | the app's JSON (object, list or keyed); WinAuth's text export | not locked |
| `otpauth://` lines | any text file of `otpauth://` URIs | — |

**Steam.** Each account becomes a Code item `Steam (account)` that shows Steam Guard's five-character codes, plus a **crown-jewel** note "Steam Guard recovery (account)" holding the identity secret and the revocation code as hidden fields, so the authenticator can be moved later. The login session in a maFile is left out (a problem line says so). **There is no code path that confirms a trade or a market listing**: the identity secret is never read back by Lockdown, and the note says so (D8, the standing rule). `ImportedItem.crown_jewel` is new so an importer can put an item in the protected class.

The fixtures (`crates/cxf/tests/fixtures/`) are written by `src/make-authenticator-fixtures.py` with independent libraries (`cryptography`, `PyNaCl`) from each app's documented format, never by Lockdown's own code.

## 4. Set-up cards (`service-2fa-guides`)

`crates/security/src/guide.rs` joins the bundled 2fa.directory list (13's) with the signed guides: for a host, the methods the site offers, its notes, set-up, passkey and recovery links when the directories have them, and Lockdown's own steps (open the site's set-up page, choose an authenticator app, type the code Lockdown shows, keep the recovery codes). The snapshot (`data/site-data/guides.json`, 754 sites) comes from 2fa.directory (MIT) and the Passkeys Directory (CC BY 4.0, commit `b711c58`, 2026-09-03), made by `data/snapshot-guides.py`; the licence text is `data/passkeys-directory.LICENSE`. Every card and the Security screen's footer credit both. A card is built on the device from bundled data: no site learns which sites a person has logins for. In the desktop, a `missing_2fa` finding row gets a "Set-up guide" button.

## 5. Federated sign-in map

`crates/app/src/bridges/federation.rs`. Where it comes from: a login's own sign-in method and provider (the editor's fields), and what the extension saw.

- **Strong evidence**: the provider's sign-in page (one the signed data names, by exact host and path, `https` only) loads in a tab's top frame, its return address (`redirect_uri`, `origin`, …) is on the same site as the page that tab was on (registrable domain), and that site has exactly one live, everyday login. The core records it at once: a new version of that login with `lockdown.signin`, and the editor's sign-in fields when empty.
- **Weak evidence**: a trusted click on a button or link that says "Sign in / Continue / Log in with *Name*", a name the signed provider list knows. It waits in memory for the person's yes in the map (at most 20, forgotten on lock). Strong evidence for the same site answers it.
- **Refused**: the provider's own site, a return address on another site, an unknown provider, a page that isn't `http(s)`. Crown jewels are never touched from a page. A click the page made itself (`isTrusted` false) is never sent.

The map (Security → "Accounts you sign in with") lists each provider with its own account (the login marked as it, else the one login saved for one of its domains), the sites entered through it (the blast radius: what falls if it falls), and warnings: no account saved, its account has no second step, it isn't protected (02-model §3 suggests it), and sites that hang on it with no password of their own. Waiting observations offer "Yes, on this login" / "a new login" / "No".

## 6. Fill fixes (`site-fix-overrides`)

`crates/app/src/bridges/fixes.rs`, `extension/src/content/fixes.ts`. A fix says which field is the username, password, new password or code, which to ignore, or never fill on this site. A field is named by its own `id`, `name` and `autocomplete`, compared exactly; never a selector (a page is hostile and a selector is code). At most 20 fields, each value 128 plain characters.

- **The person's** fixes are set from the extension's popup ("Fix this page": the page's fields by label, never their values) or the login's detail pane, and kept in `lockdown.fill` on every everyday login saved for the site. So they sync end to end, follow the item's history and export, and never show as fields.
- **The community's** come from the signed site data's `fixes` list, read only, under the person's own. The list ships **empty but signed**: the mechanism, signature and precedence are tested; entries arrive with a data update.
- **Never** from either wins. The extension then shows no menu, the popup's Fill and the shortcut refuse with the reason, and **the core refuses every fill on that site** (`App::fill` asks first), so a "never" holds whatever the extension does. Basic Auth goes through the same path.

## 7. Aliases and alias monitoring

`crates/security/src/alias.rs` speaks to the six services (SimpleLogin, addy.io, Firefox Relay, Fastmail's masked email over JMAP, DuckDuckGo Email Protection, Forward Email); `crates/app/src/bridges/aliases.rs` keeps them.

- **Setting one up** (Settings → Email aliases): the API key goes in once as bytes and becomes a new **crown-jewel** API-credential item; its settings (service, self-hosted base for SimpleLogin, addy.io and Forward Email, domain where the service needs one) sit in `lockdown.alias`. The key is never sent back to a shell.
- **Making one**: the `Use` gate every time, counted as a use (D151), then one request to the service (`https`, or loopback for a self-hosted one), with "Made by Lockdown for *site*" as its note. In the login editor, "Hide your email address" offers "New alias". In the extension, the popup's **Hide my email** makes one for the tab's site and puts it in the page's email field; from the browser only the platform check (Touch ID, Windows Hello) can answer the gate, otherwise the popup says to make it in the editor, as a crown jewel's fill does (D98).
- **Monitoring** (Settings → Email aliases, off by default, needs an account): the addresses of every alias made (never keys) go to the account's own server (`PUT /v1/account/monitored`, at most 200), and the server's breach monitor checks them with the account's login address. Off sends an empty list. A new alias is added as it is made.

The extension offers aliases in the popup rather than the save prompt: the save prompt appears after the form has sent its address, too late to hide it (D268).

## 8. Guided incident flows (`breach-playbooks`)

`crates/security/src/incident.rs` holds the seven flows of 13 §8 as steps over the seven plain playbooks: a site breached, credential stuffing, set-up keys leaked, email taken over, a sign-in provider taken over, malware on a computer, and the Lockdown account or kit exposed. Security's "Something happened?" lists them. A flow asks what it's about (which site, address or provider), then fills its steps from the vault: the logins that share the breached password, every login on the taken-over address, every site entered through the provider (the map, §5), every code item when keys leaked. Each step and each item in it is ticked off. Progress is sealed on the device (`incidents-<persona>.cbor`, AAD `lockdown/incidents/v1`), not synced.

## 9. Change-password links

`crates/security/src/change_password.rs` now resolves in this order: the site's well-known page when the check is on and the site answers it (and doesn't answer a page that shouldn't exist), then a curated recipe from the signed data (12 sites to start), then the site's front page. With the check off, the recipe, else the unchecked well-known address. The extension's popup asks the app (`change_link`) instead of building the address itself; it opens only `http(s)` links and never a crown jewel's (change those in Lockdown).

## 10. Clock check (`code-display-aids`)

`crates/codes/src/skew.rs` measures one sample: `GET /healthz` on the account's server, timed on this device, with the answer's `Date` header. It is **checked**: a round trip over 4 s is refused, and the offset is the header against the round trip's midpoint. It is **bounded**: under 3 s it is ignored, up to 300 s it is corrected, beyond that Lockdown warns and does not correct (a wrong clock that far is better fixed in the system, and a hostile answer can't move codes further). The Codes view checks when opened (at most every few minutes) and says what it found; the setting `code_clock_correction` (on) turns it off. No account: codes use this device's clock and the view says so.

## 11. Exports locked to age keys

`crates/cxf/src/age_file.rs` is an age v1 implementation (X25519 recipients only) over primitives already in the tree; the `age` crate itself didn't fit: 0.12 needs a pre-release `kem` that conflicts with ML-KEM 0.3's, and 0.11 brings an unmaintained proc-macro crate `cargo deny` refuses (RUSTSEC-2026-0173). Interop is tested against the reference implementation (rage, through `pyrage`) both ways: its files open here, ours open there. The file is hostile on the way in (header capped, canonical base64, MAC before payload, zero shared secret refused, every chunk authenticated). Export → "A Lockdown backup, locked to a public key (age)" takes up to 20 recipients, passes the `Reveal` gate, and writes the Lockdown document sealed to them; Import opens such a file with the private key typed in the password field. Passphrase or SSH-key age files are refused in words.

## 12. Backup line and emergency sheet

The sidebar's footer shows when the last backup ran ("Backed up 3 hours ago", "No backup yet", "Backups are off") and opens Settings. Recovery → **Emergency sheet** is one printable page: the account's server and login, whether the kit was tested, the devices, where backups go and the last one, the emergency contacts, five "what to do" lines, and blanks to fill in by hand. **No password, Secret Key, recovery code or item** is on it (tested). It prints alone, without the app behind it, as the kit's page does.

## 13. Wire and IPC additions (additive, D62)

- Sync wire: `MonitoredAddresses { addresses }`, `MAX_MONITORED`; routes `GET/PUT /v1/account/monitored` appended to the server router; `SqliteAlertStore` gains the `monitored_addresses` table.
- Settings: `code_clock_correction` (default `true`), `alias_monitoring` (default `false`), both `serde(default)`.
- Import/export formats: the nine importers above; export format `age`.
- Desktop commands, appended: `clock_status`, `clock_check`, `federation_map`, `federation_accept`, `federation_dismiss`, `federation_set_account`, `site_fix`, `site_fix_set`, `alias_overview`, `alias_service_add`, `alias_generate`, `alias_monitoring`, `alias_monitoring_set`, `incidents`, `incident`, `incident_mark`, `incident_reset`, `setup_guide`, `guide_sources`, `emergency_sheet`, `export_save_age`.
- Extension ops: `federation_hints`, `federation_seen`, `site_fixes`, `fix_set`, `change_link`, `alias_options`, `alias_create`. The popup's old `change_password` message is replaced by `change_link`.
- `ChangeLink` gains kind `recipe` and an optional `note`.

## 14. Running and testing

```
cargo test -p lockdown-cxf          # importers (authenticators.rs), age interop (age_interop.rs)
cargo test -p lockdown-security     # site data, guides, incidents, aliases against a local fake service
cargo test -p lockdown-codes        # the skew check
cargo test -p lockdown-app --test bridges   # every row through the App, with a real server for clock and monitoring
cargo test -p lockdown-server --test monitored
cargo test -p lockdown-desktop --features shell
(cd apps/desktop && pnpm test)      # features/bridges/bridges.test.tsx
(cd extension && pnpm run build:native && pnpm build && pnpm test)   # test/bridges.spec.ts
cargo run -p lockdown-security --example sign-site-data -- --check  # the signed data matches its sources
```

Every third-party API is a local fixture: the alias services and the clock are fake servers in the tests; the extension suite answers Google's and GitHub's pages itself, runs Chromium with `--no-proxy-server`, and resolves those names nowhere, so nothing reaches the internet (`--no-proxy-server` was added to the existing extension suite for the same reason).

## 15. Gaps

1. The community fix list ships empty; the first entries need a review process (who may add a fix, how it is tested).
2. Proton Authenticator's password-locked export is refused; its format isn't published.
3. Ente, 2FAS, Aegis, Bitwarden and Proton fixtures follow each app's documented format; none was exported from the app itself here.
4. The web vault has no bridges: no clock check, no map, no fixes, no aliases (the views hide). Most are core calls the web worker could answer; aliases and the clock need the browser's `fetch` in place of `HttpFetch`.
5. Aliases come from the popup, not the save prompt (D268); there's no inline "Hide my email" in the field menu yet.
6. Fastmail and DuckDuckGo were tested against fakes of their documented APIs only; their real error shapes may differ.
7. Incident progress is local to the device; a flow started on one computer doesn't show on another.
8. The federated map doesn't yet learn from the provider's own list of connected apps (Google's "Third-party connections"); it knows only what the person browsed with the extension or typed.
9. The clock sample is one request; a server behind a slow proxy may often be refused as too slow (4 s), and the codes then use the device's clock.
10. An age identity is typed in the import's password field; there is no saved age identity in Lockdown yet.
11. One run of the full extension suite under heavy machine load saw the existing save-prompt test time out on a disabled Save button (the prompt's visibility check); it passed alone and on the next full run.

**unicron host-contract gaps:** none blocking. `Icon` takes no size class, so the few icons that needed one sit in a sized `span` (the design-system ratchet stays at its baseline, 4). Printing a dialog alone still relies on the app's own `data-print-page` rule in `index.css`; a `Dialog` print mode in unicron would replace it.

## 16. Proposed decisions

For the coordinator to record. **Flagged** rows are user-visible and need Sean.

| ID | Decision | Owner |
|---|---|---|
| D260 | **Signed site data**: one bundle (`lockdown-site-data`, prefix `lockdown-site-data/v1\0`) in the matcher's signed envelope and keys holds set-up guides, sign-in providers, change-password recipes and community fill fixes; the matcher exposes `sign_envelope`/`verify_envelope` so formats share one verifier; rebuilt and checked by `sign-site-data`. Plain version: everything Lockdown knows about other sites comes in one signed file it checks before use. | Accepted by default under D38 |
| D261 | **Set-up card sources**: 2fa.directory (MIT) and the Passkeys Directory (CC BY 4.0) snapshot at a named commit, credited on every card and in Security's footer; cards built on the device, nothing sent. Plain version: Security shows how to turn on two-step sign-in for your sites, and says where that came from. | **Flagged for Sean** (attribution shown in the app) |
| D262 | **Authenticator imports**: nine new formats in the doc 14 wizard; locked Aegis, 2FAS, Ente and SDA files open with their password; a locked Proton Authenticator export is refused in words; unsupported entries become problem lines. Plain version: bring your codes from any common authenticator app. | Accepted by default under D38 |
| D263 | **Steam Guard**: a Code item for the codes plus a protected note with the identity secret and revocation code as hidden fields; the maFile's session is left out; no code path confirms trades or listings, and the note says so. Plain version: Lockdown shows your Steam codes and keeps the recovery secrets safe, but never approves a trade for you. | **Flagged for Sean** (the protected note; the session dropped) |
| D264 | **The federated map's evidence**: strong = the provider's signed sign-in page in a top frame returning to the same site as the tab's last page, on that site's one everyday login, recorded at once (`lockdown.signin`); weak = a trusted "Sign in with X" click, kept in memory for the person's yes (20, forgotten on lock); crown jewels untouched; the provider's account is the login marked as it, else the one on its domains; warnings for no account, no second step, not protected, sites with no password. Plain version: Lockdown learns which logins hang on your Google or Apple account and shows what falls with it. | **Flagged for Sean** (the extension updates a login by itself on strong evidence) |
| D265 | **Wire and IPC additions** (D62, additive): `MonitoredAddresses` and `/v1/account/monitored`; settings `code_clock_correction` and `alias_monitoring`; the nine import formats and export `age`; 21 desktop commands and 7 extension ops (§13); `ChangeLink` kind `recipe` and `note`; the popup's `change_password` message replaced by `change_link`. Plain version: old and new parts keep understanding each other. | Accepted by default under D38 |
| D266 | **Fill fixes**: kept in `lockdown.fill` on every everyday login for the site (synced, versioned); fields named by exact `id`/`name`/`autocomplete`, never selectors; 20 fields, 128 characters; the person's over the signed community list; "never" from either wins and the core refuses every fill there. Plain version: tell Lockdown which box is which on a stubborn site, or to leave it alone, and every device follows. | **Flagged for Sean** |
| D267 | **Alias services**: six services; each key a new crown-jewel item; every alias through the `Use` gate, counted as a use; https or a loopback self-hosted base only; the alias's note names the site. Plain version: make a fresh forwarding address for each site from Lockdown, with your service's key kept as safe as your most important passwords. | **Flagged for Sean** (a gate prompt per alias) |
| D268 | **Aliases in the extension come from the popup** ("Hide my email", filling the page's email field), not the save prompt: the prompt appears after the form sent the real address. From the browser only the platform check answers the gate. Plain version: open Lockdown's button on a sign-up page to get a hidden address before you submit. | **Flagged for Sean** (differs from the checklist's wording) |
| D269 | **Alias monitoring**: off by default; on sends alias addresses only (never keys), at most 200, to the account's own server, whose breach monitor checks them with the login address; off sends an empty list. Plain version: if a breach includes one of your aliases, you're told. | **Flagged for Sean** (your aliases are stored on your server) |
| D270 | **Incident flows**: the seven flows as steps over the seven playbooks, filled from the vault; progress sealed on the device (`incidents-<persona>.cbor`), not synced. Plain version: when something goes wrong, Security walks you through it and remembers where you got to. | **Flagged for Sean** (progress stays on one device) |
| D271 | **Change-password link order**: checked well-known page, then a signed curated recipe, then the front page (check off: recipe, else the unchecked well-known); the extension asks the core and opens only `http(s)`, never for a crown jewel. Plain version: "Change password" lands on the right page more often. | Accepted by default under D38 |
| D272 | **Clock correction**: one timed `GET /healthz` to the account's server; round trip ≤ 4 s; ignored under 3 s, corrected up to 300 s, beyond that a warning and no correction; on by default (`code_clock_correction`); no account, no correction. Plain version: your codes stay right even when this computer's clock drifts, within limits a bad server can't push past. | **Flagged for Sean** (on by default) |
| D273 | **Exports to age keys**: Lockdown's own age v1 (X25519 only, interop-tested with rage) instead of the `age` crate (0.12: conflicting `kem`; 0.11: unmaintained proc-macro refused by `cargo deny`); up to 20 recipients; `Reveal` gate; import with the private key as the password. Plain version: export a file only someone holding a matching key can open, in Lockdown or any age tool. | **Flagged for Sean**; **audit item** (our own format code) |
| D274 | **Backup line and emergency sheet**: the last-backup line in the sidebar's footer; a printable emergency sheet (account, devices, backups, contacts, what to do, blanks) with no secret, printed alone. Plain version: you see when a backup stops, and you can print one page that tells a future you what to do. | **Flagged for Sean** |
