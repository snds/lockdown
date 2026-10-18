# Security center: breaches, weak and reused passwords, two-step sign-in, lookalikes, account alerts

_Status: built, 2026-10-18, on branch `slice/security-center` (D122: desktop and web first). The desktop app has a **Security** screen and a per-item security panel; the core decides every finding; the server keeps account alerts and runs an email breach job behind configuration. Proposed decisions D125–D134 are at the end. Companion to [`10-desktop-shell.md`](10-desktop-shell.md) (how the shell is built: core owns secrets, copy by id, the gates), [`04-security.md`](04-security.md) and [`02-model.md`](02-model.md)._

## For future agent

- **TL;DR:** One report, decided in the core (`lockdown_app::security`), lists what to fix in three groups a person acts on: **Fix now** (a password seen in a breach, your email in a breach), **Fix soon** (a reused or very guessable password, a website that looks like another site, a new device on your account) and **Worth doing** (a guessable password, a site that offers two-step sign-in you haven't added). Each finding opens its item and a short playbook, and can be dismissed or snoozed; the dismissal is a setting on the item, so it syncs. Nothing crosses to the webview but words, ids, titles and counts: never a password, a hash or a reuse key. The only network call that touches a password carries five hex characters of its SHA-1.
- **Reads before this:** `10-desktop-shell.md` §1–§3 (the layers, the command surface, secret handling), §12 (D116, strength in the core), `02-model.md` §3 (crown jewels) and §9–§10 (metadata, matching), `04-security.md` §1, decisions D85, D101, D110, D113, D116.
- **Not here:** the extension's hard stop on a lookalike fill (09-extension-boundary, unchanged), the seven guided incident flows of the feature matrix (§7), alias monitoring (no aliases exist), the web vault's copy of the screen.

## 1. What was built, by checklist row

| Row | State on this branch | Where |
|---|---|---|
| `breached-passwords` | **Done** for desktop and core: k-anonymity range check, padded answers, configurable source (public API, a mirror, or a folder of range files offline), verdicts cached without a hash, re-checked when a password changes and weekly | `crates/security/src/breach.rs`, `crates/app/src/security.rs`, Security screen |
| `weak-reused` | **Done** for desktop and core: zxcvbn on every stored password in the core, reuse by a keyed hash made for one report; across the open vault (one per session today) | `crates/app/src/security.rs`, `crates/app/src/session/security.rs` |
| `missing-2fa-report` | **Done** for desktop and core, from a bundled 2fa.directory snapshot (MIT, 1,889 sites, as of 2026-10-16) with each site's set-up page; the passkeys directory is not bundled (§4) | `crates/security/src/twofa.rs`, `crates/security/data/` |
| `change-password-link` | **Partial**: the W3C well-known page, checked first, with the site's front page as fallback; curated per-site recipe URLs are not built | `crates/security/src/change_password.rs`, `security_change_password` |
| `lookalike-warning` | **Partial → more**: the matcher's lookalike decision is now on a login's detail (a warning panel, first and loudest) and in the report, against your other logins and the directory's well-known sites; the hard stop in a real fill path stays the extension's | `crates/app/src/security.rs`, `features/security/item-security.tsx` |
| `breach-playbooks` | **Partial**: short steps for each kind of finding (seven: breached, reused, weak, missing two-step, lookalike, exposed email, new sign-in), each ending in how to confirm; the matrix's seven *incident* flows (site breach, stuffing, 2FA seed leak, email takeover, identity-provider takeover, infostealer, vault breach) are not built | `crates/security/src/playbook.rs`, `playbook-dialog.tsx` |
| `email-breach-monitoring` | **Partial (server)**: the job, the injectable feed, an HIBP v3 feed behind `--breach-feed-url` / `--breach-feed-key` (**needs a paid key**, off by default), alerts to every device; aliases not monitored | `bins/server/src/breach_monitor.rs` |
| `account-alerts` | **Partial → more**: a new device's first sign-in is an alert on the account's other devices, dismissed once for all; a session list that ends tokens and email verification for new devices are still missing | `bins/server/src/alerts.rs`, `api.rs`, the Security screen |
| Desktop **Security** view | **Done**: findings grouped by severity with counts, each with What to do (its playbook), its fix (Change password, Turn it on, Go to Devices), Open item, and Dismiss / Remind me in a week / in a month; dismissed and snoozed ones listed with Bring back; the checks' settings in plain words | `apps/desktop/src/features/security/` |

## 2. How it works

### 2.1 The layers

```
crates/security/ (lockdown-security, new)   no vault: breach range check, 2FA directory, change-password link, playbooks, one HTTP client
  src/breach.rs          Candidate (SHA-1 held as hex in Zeroizing, Debug prints the prefix only), RangeSource, HttpRange, DirRange, check()
  src/twofa.rs           TwoFaDirectory::builtin(): the snapshot, lookup by host then parent domains, method words
  src/change_password.rs resolve(site, probe?): should-not-exist probe, then /.well-known/change-password, else the front page
  src/playbook.rs        PLAYBOOKS: seven, plain words, tested for jargon and for a closing check
  src/http.rs            Fetch trait; HttpFetch: rustls on ring, no redirects, no cookies, timeout, 4 MiB cap; https or this machine only
  data/                  2fa-directory.json (the snapshot), its LICENSE, snapshot-2fa-directory.py (how it is cut)
crates/app/src/security.rs           the report: findings, severities, words; App::security_* ; security.cbor (verdicts)
crates/app/src/session/security.rs   Session::security_scan (passwords to a callback, on the worker), security_set (the item setting)
crates/vault/src/compose.rs          reserved `lockdown.` fields: kept by an edit without being named, refused when named
crates/sync/src/wire.rs, transport.rs   AccountAlert, AlertsSince, BreachNotice; HttpTransport::account_alerts / dismiss_alert
bins/server/src/alerts.rs            AlertStore (tables beside the main schema), the new-device rule
bins/server/src/breach_monitor.rs    BreachFeed, HttpBreachFeed (HIBP v3 shape), scan_once, spawn
apps/desktop/src-tauri/src/security.rs   change_password(): resolve, then the shell's opener (http and https only)
apps/desktop/src/features/security/      the Security screen, finding rows, the playbook dialog, the item panel, the settings
```

### 2.2 Commands

Appended to the handler list; every one runs off the main thread.

| Command | `App` | Network | Returns |
|---|---|---|---|
| `security_report` | `security_report()` | none | `SecurityReport` from what this session knows |
| `security_check` | `security_check()` | range requests (five hex characters each), the account's alerts | `SecurityReport` |
| `security_dismiss` | `security_dismiss(item, key, until?)` | none | `SecurityReport` (the item has a new version) |
| `security_restore` | `security_restore(item, key)` | none | `SecurityReport` |
| `security_dismiss_alert` | `security_dismiss_alert(seq)` | `POST /v1/account/alerts/{seq}/dismiss` | `SecurityReport` |
| `security_change_password` | `security_change_link(item)` + the opener | two GETs to the site itself, when the check is on | `ChangeLink { url, kind, checked }` |
| `security_playbooks` | `security_playbooks()` | none | `Playbook[]` |

Server: `GET /v1/account/alerts?since=N` (`AlertsSince`) and `POST /v1/account/alerts/{seq}/dismiss`, both needing a session.

### 2.3 The findings

| Kind | Rule | Severity | Key on the item |
|---|---|---|---|
| `breached` | the password's verdict (for this item version, from this source) counts more than zero | urgent | `breached:<field>` |
| `reused` | two or more items share a password (keyed BLAKE3, key from `random_32` per report, dropped after) | important | `reused:<field>` |
| `weak` | zxcvbn score 0–1 | important | `weak:<field>` |
| `weak` | zxcvbn score 2 (the editor's "Weak" too, D116) | suggested | `weak:<field>` |
| `missing_2fa` | a Login's first website is in the directory and the item holds neither a code nor a passkey | suggested | `missing_2fa` |
| `lookalike` | a Login's website imitates another login's or a directory site's (below); a site in the directory is never flagged | important | `lookalike` |
| `new_sign_in` | the server's new-device alert, unless that device is already revoked | important | server-side |
| `exposed_email` | the server's breach alert; a breach dated within a year (or undated) is urgent | urgent / important | server-side |

"Stored passwords" are the fields the template offers the generator for (D112): a Login's password, server and database passwords, a Wi-Fi password, a wallet's password, an SSH passphrase. A card number, a PIN, an API key and fields added by hand are not scored or checked. Trashed and archived items are left out.

**Lookalikes** use the matcher's own rules (`lockdown_matcher::lookalike`): the confusables skeleton (homoglyphs, `rn` → `m`, punycode decoded), edit distance one for labels of six or more characters, a trusted label as a hyphen part (`paypal-secure`), and a trusted registrable domain used as a leading sub-domain (`paypal.com.evil.net`). The same registrable domain and the curated equivalent groups (`amazon.com` / `amazon.co.uk`) never count. Against a few thousand well-known domains a cheap pre-test (same skeleton, length within one, or containment) runs before the full rule, so a report stays fast.

A **crown jewel**'s findings carry its title and kind only (D113): no website, and a lookalike's sentence doesn't name the host.

### 2.4 The breach check, step by step

1. On the worker, `security_plan` walks the live items. Each password with no current verdict (new, changed, older than a week, or a different source) becomes a `Candidate`: its SHA-1 in hex, in a `Zeroizing` string, and a key `<item>/<field>`.
2. On the calling thread (a Tauri `spawn_blocking` thread, not the vault's), `breach::check` groups candidates by prefix and asks the source once per prefix: `GET <endpoint><PREFIX>` with `Add-Padding: true`. The answer's lines are `SUFFIX:COUNT`; padded decoys count zero and are ignored. A failed prefix leaves only its candidates unchecked and the first failure is reported in words.
3. The candidates are dropped (wiped). Back on the worker, `security_apply` writes the verdicts to `security.cbor` beside `settings.cbor` (user-only, like every file in the data directory): per key, the item version checked, the count, the time. No hash, no password. Verdicts for items or fields no longer in the vault are forgotten.
4. A dismissal writes a new item version without touching the password, so the verdicts and scores for that item carry over to the new version (`security_carry`).

Sources: the default is `https://api.pwnedpasswords.com/range/`; the setting takes any `https://` range API (a mirror), or `file:///folder` of `<PREFIX>.txt` files (the layout the public downloader writes) for a computer that is never online. A plain `http://` address is refused unless it is this machine. The Security screen checks by itself when it opens and the last check is over a day old or some passwords are unchecked; "Check now" does it on demand.

### 2.5 Dismiss and snooze: a setting on the item

The item's reserved field `lockdown.security` holds `{"dismissed": {"<key>": {"at": t, "until": t | null}}}`. `lockdown_vault::compose` treats every `lockdown.`-prefixed field as Lockdown's own: an edit keeps it without naming it, an add or edit that names it is refused, and `show` never returns it, so no editor or detail sees it. It is an ordinary sealed field otherwise: it syncs, it is in the item's history, and a Lockdown CXF export carries it in the item's extension. A snooze must end in the future; once it ends, the finding is open again. A trashed item refuses a dismissal. Account alerts are dismissed on the server, once for the account.

### 2.6 Account alerts (server)

On `login/finish`, when the device id is new to the account and the account then has more than one device, the server records a `new_device` alert. `alerts_for(account, device, since)` never returns a device's own sign-in, nor a sign-in by a device that joined before the asking device did (both by the devices table's own clock). An email-breach alert belongs to the account whenever a device joined. Tables `account_alerts` and `breach_seen` are created beside the main schema with `IF NOT EXISTS`, so an older build opens a file that has them; `SCHEMA_VERSION` stays 3. The client fetches alerts during `security_check` (from `since=0`, every time, so a dismissal elsewhere shows) and keeps them in memory until lock.

### 2.7 Email breach monitoring (server, behind configuration)

`breach_monitor::spawn` runs `scan_once` on its own thread every `--breach-scan-hours` (default 24), pausing `--breach-scan-pause-ms` (default 6,500, about ten a minute) between accounts. Only logins that are email addresses are asked about. Each breach is reported to an account once (`breach_seen`); the first scan reports every known breach for the address. The built-in feed speaks the HIBP v3 `breachedaccount` shape and sends the address and the `hibp-api-key` header; **HIBP requires a paid key for this API**, so without `--breach-feed-url` the job does not run and nothing is sent. There is no public k-anonymity lookup for addresses, so the address itself goes to the configured feed; that is the trade a server operator turns on. The job logs counts only, never an address.

### 2.8 Change password

On "Change password" the core finds the item's first website, and with the check on (`change_password_check`, default on) asks the site, with no cookies and no account data: first the W3C's path that should not exist (a 2xx there means the site answers anything, so its answer proves nothing), then `/.well-known/change-password` (2xx or 3xx means supported). Supported opens the well-known page; anything else, or no answer, opens the site's front page and says so. The shell opens the page through its opener (http and https only); the webview launches nothing.

## 3. Security notes

- **What crosses to the webview:** the report and the playbooks. Tested in Rust (`crates/app/tests/security_center.rs`, `src-tauri/src/security.rs`) by serialising what the webview gets and looking for the passwords, the SHA-1 prefix and suffix; in the UI by the screen's text. Reveal and copy are unchanged (D85, D101, D105, D115).
- **The network:** the breach check sends five hex characters per distinct prefix, with padding asked for; the change-password probe sends two GETs to the site itself; the alert calls are the account's own server. Every endpoint is a setting (breach source) or the site itself; the server's feed is a flag. No error message or log names a password, a hash, a path or an address (`HttpFetch` reports the host and the kind of failure only).
- **Scoring stored passwords** amends D116 inside the core only: zxcvbn's working strings are unwiped heap strings, as D116 notes, so the first 100 characters are scored and nothing else leaves the worker but the score. Scores are cached per item version in memory and dropped on lock.
- **The cache file** says which item ids had a breached password and when they were checked. It holds no secret; it is user-only and sits with the other files of the data directory.
- **Crown jewels** are scored and checked like any password (only a prefix leaves), dismissed without the `Use` gate (a dismissal changes Lockdown's own field, not the secret), and named by title only in findings.

## 4. Gaps

- The matrix's **seven incident flows** (site breach, credential stuffing, 2FA seed leak, email takeover, identity-provider takeover, infostealer, vault breach), with sessions, forwarding rules, recovery info and app grants, are not built; the playbooks here are per finding.
- **Change-password recipes**: no curated per-site URL list after the well-known page.
- **Four directory entries withheld**: the snapshot script leaves out four entries whose names fall under this repository's content rule (no AI product or employer names in repository content), listed in the script by the SHA-256 of their names only; those sites get no missing-2FA finding. Sean may prefer to lift this for third-party data.
- **Passkeys directory**: not bundled; its licence was not verified. The 2fa.directory `u2f` entries say where a security key works.
- **Reuse across vaults**: one vault is open per session today, so reuse is within it.
- **Account alerts**: no OS notification (alerts arrive when the Security screen checks), no session list that ends tokens, no email verification for new devices, no sign-in alerts beyond a new device.
- **Email monitoring**: aliases (none exist), and a feed that needs no paid key.
- **Other surfaces**: the CLI and the web vault have no Security screen; the extension's fill-time hard stop is unchanged.
- **Sidebar count**: the Security entry shows no count yet (it would need the report on every list reload).
- **Cost**: each report opens every live item, as `list` does; scores are cached, hashing is BLAKE3.
- **A dismissal is a new item version**, so the item's "edited" time moves and it rises in the vault list (D132).
- **Discovery**: `/.well-known/lockdown` does not yet advertise alerts or breach monitoring.

## 5. unicron host-contract gaps found

| Component | What it lacks | What the shell did |
|---|---|---|
| (none) | an **Alert / Callout**: an inline, toned message box with a role (the lookalike warning on an item, the conflict box) | a bordered `section` with `role="alert"` and the app's text roles |
| `Icon` | a `tone` beyond `muted` and `destructive` (no attention / warning ink) | colour on a wrapping element from the app's `--attention-text` role |
| `Badge` | a warning / attention variant, or a severity badge (RiskTierBadge is for bridge tiers only) | `destructive` for Fix now, `outline` for the rest, the words always present |
| (none) | a **stat tile** (a count with its label) for summaries | `Item` with a large-number `span` inside `ItemTitle` |
| (none) | an **ordered steps** list (a checklist with progress) for playbooks | a plain `ol` |
| (none) | a **Collapsible / Disclosure** | a `Button` with `aria-expanded` toggling the dismissed list |
| `Item` | a severity or tone for a row (`variant` is muted / default / outline) | the group heading carries the severity in words and colour |

The dsa ratchet stays at 4 className overrides.

## 6. Tests

Rust 225 → 261 (`cargo test --workspace`): `lockdown-security` 17 (breach 5, twofa 3, change-password 3, playbook 1, http 2, the local-server tests 3), `lockdown-vault` compose 1, `lockdown-server` 5 (alerts 2, breach monitor 3) plus the endpoint test 1, `lockdown-app` 10 (report words and lookalike rules 2; the security settings' defaults 1; the end-to-end security-center tests 7: findings and their words, codes and crown jewels, the breach check against a local range server, a failed range, dismiss and snooze through a lock and an edit, change password against a local site, a new device against the in-process server), the desktop crate 2. Web UI 61 → 68: the Security screen (groups and counts, the playbook and Open item, a week's snooze, the background check, Change password and Turn it on, all clear and the breach switch) and the item panel (the lookalike warning first; nothing when there is nothing or the report fails).

## 7. Proposed decisions

| ID | Date | Decision | Status |
|---|---|---|---|
| D125 | 2026-10-18 | **The security center is decided in the core and speaks in three groups.** `lockdown-security` (no vault: breach range check, 2FA directory, change-password link, playbooks, one HTTP client) and `lockdown_app::security` (the report) decide every finding and its words, so every shell says the same; the webview gets words, ids, titles and counts, never a password, a hash or a reuse key. Findings are grouped **Fix now** (breached password, email in a recent breach), **Fix soon** (reused, very guessable, lookalike website, new device) and **Worth doing** (guessable, two-step sign-in available), each with a short playbook ending in how to confirm the fix. Plain version: one screen tells you what to fix first, in ordinary words, and how. | Proposed; **flagged for Sean** (the three group names, the playbook words) |
| D126 | 2026-10-18 | **Breached passwords by k-anonymity, on by default.** Five hex characters of each stored password's SHA-1 go to a range source with `Add-Padding: true`; the match is made on the device. Default source the public Pwned Passwords range API; a setting takes an `https://` mirror or a `file:///` folder of range files for a computer that is never online. The Security screen checks when it opens if the last check is a day old or a password is unchecked, and on Check now; verdicts (item version, count, time; no hash) are cached in `security.cbor` and re-checked weekly or when the password changes. Plain version: Lockdown checks your passwords against stolen-password lists without ever sending a password. | Proposed; audit item; **flagged for Sean** (on by default; a third-party service by default; the check on opening the screen) |
| D127 | 2026-10-18 | **Stored passwords are scored in the core for the weak report** (amends D116, which scored only a password being typed). zxcvbn on the first 100 characters of every live item's generator-offered secret, with the item's title, username and host as context; 0–1 is Fix soon, 2 Worth doing; only the score is kept, per item version, in memory, dropped on lock. Plain version: Lockdown now also tells you which saved passwords are easy to guess, worked out on your computer. | Proposed; audit item (unwiped zxcvbn working strings over stored secrets); **flagged for Sean** (user-visible) |
| D128 | 2026-10-18 | **Reuse is found by a keyed hash made for one report.** BLAKE3 keyed with 32 random bytes per report, never stored or sent; groups within the open vault; a finding names the other items by title. Plain version: Lockdown spots a password used twice without comparing passwords anywhere but inside the core. | Proposed; audit item |
| D129 | 2026-10-18 | **The missing-2FA report reads a bundled 2fa.directory snapshot** (MIT, commit `9352ff5`, 2026-10-16, 1,889 sites; the notice ships beside the data and in `NOTICE`), matched by a Login's host then its parent domains; a login holding a code or a passkey answers it; "I already use two-step sign-in here" dismisses it. The passkeys directory is not bundled until its licence is checked. Plain version: Lockdown knows which sites offer two-step sign-in without asking anyone which sites you use. | Proposed; **flagged for Sean** (attribution line on the Security screen; how often the snapshot is refreshed) |
| D130 | 2026-10-18 | **The matcher's lookalike decision is shown before a fill exists**: on a login's detail (a warning panel, first) and in the report, comparing each login's website with your other logins and the directory's well-known sites under the matcher's rules; a directory site and the curated equivalents are never flagged. Plain version: if a saved website's address imitates a real one, Lockdown says so on the item. | Proposed; **flagged for Sean** (user-visible; false positives are dismissible) |
| D131 | 2026-10-18 | **Change password opens the site's well-known page when the site really has one**, checked first the W3C way (a should-not-exist path, then `/.well-known/change-password`), else the front page with a sentence; the check can be turned off; the shell opens the page. No curated recipes yet. Plain version: one button takes you to where the site lets you change your password. | Proposed; **flagged for Sean** (minor: the probe on by default) |
| D132 | 2026-10-18 | **Dismiss and snooze are a reserved field on the item** (`lockdown.security`): synced, in history and export, never shown, kept by every edit without being named, refused if a shell names it; snoozes of a week or a month; no gate even on a crown jewel (it changes Lockdown's own field, not the secret). A dismissal is a new item version, so the item's edited time moves. Account alerts are dismissed on the server for every device. Plain version: dismiss a warning on one computer and it's gone on all of them. | Proposed; **flagged for Sean** (the edited time moving; no gate for a crown jewel's dismissal) |
| D133 | 2026-10-18 | **New-device sign-in alerts from the server.** When a device new to the account signs in and the account already has another device, the server records an alert; the other devices that joined before it see it in Security (Fix soon, with Go to Devices); the new device never sees its own; dismissed once for the account. `GET /v1/account/alerts`, `POST /v1/account/alerts/{seq}/dismiss`; additive tables, schema version unchanged. No OS notification yet. Plain version: if a device you don't know signs in to your account, your other devices tell you. | Proposed; **flagged for Sean** (user-visible; no push notification yet) |
| D134 | 2026-10-18 | **Email breach monitoring is a server job over an injectable feed, off unless configured.** `--breach-feed-url` / `--breach-feed-key` (the HIBP v3 `breachedaccount` shape, which **needs a paid key**); every 24 h, about ten accounts a minute; only email-address logins; each breach reported once, the first scan reporting all known ones; the address goes to the feed (there is no public k-anonymity lookup for addresses); counts only in the log. Aliases later. Plain version: a server that turns this on tells you when your email address shows up in a new breach. | Proposed; **flagged for Sean** (needs a paid key; sends the account's address to the feed; cloud default) |
