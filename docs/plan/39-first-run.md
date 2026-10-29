# 39: First run for people, a local account, and finding a server

_Slice: `slice/first-run`. Status: **built** (2026-10-29). §1 is the plan Sean approved on 2026-10-29 with every recommendation taken (§1.15); §2 onward is what was built, where it differs from §1 and why, how to run it, the gaps, and the final proposed decisions (§8). Answers Sean's notes after his first run of the macOS app: the first run talked about a sync server nobody had helped him set up; it should make a local account with a name and a master password, find servers on its own and let the person correct the pick, run as its own server until a real one exists, set expectations plainly, and feel roomy. Builds to the coordinator's eight calls under D375 (restated in §1.0) and to D11, D38, D62, D92, D94, D95, D101, D122. Proposed decisions D460–D479 are at the end. Sections 2 onward (what was built, how to run it, gaps, audit notes) are written in phase 2._

## For future agent

- **TL;DR:** First run makes a **local account**: a name (a label for the lock screen) and a **master password**, registered with OPAQUE and a Secret Key against a Lockdown server that **the app runs inside itself**, with no socket, storing under the app's data directory. The master password is what opens the vault on this computer: it seals the device's keys at rest (a local password slot), so a stolen disk needs the password. Touch ID or Windows Hello can stand in for it within one launch for up to 14 days. A background **discovery** module finds real servers (managed policy, this machine's config, a server on this computer, the local network, the person's email domain), ranks them and never connects without a confirm showing the address and the server's key fingerprint. **Move to your own server** copies the identity, vaults and signed history to a real server. The first run has its own roomy layout; Comfortable becomes roomier across the app.
- **Read with:** `crates/app/src/local_unlock.rs` (the sealed device keys), `crates/app/src/first_run.rs` (first run, unlock, migration, forgotten password, the move), `crates/app/src/session/local_account.rs`, `crates/app/src/onboarding.rs`, `crates/app/src/discovery.rs`, `crates/sync/src/transport.rs` (`LOCAL_BASE`, `install_local_server`, `set_local_home`), `crates/sync/src/dnssd.rs`, `bins/server/src/local.rs` and `announce.rs`, `crates/policy/src/managed.rs`, `bins/cli/src/master.rs`, `apps/desktop/src/features/unlock/` (`first-run.tsx`, `join.tsx`, `migration.tsx`, `forgot-password.tsx`, `unlock-screen.tsx`, `welcome.tsx`), `apps/desktop/src/features/server/server-settings.tsx`, `apps/desktop/src-tauri/src/first_run.rs`, the tier-3 knobs in `apps/desktop/src/styles/app-extension.css`; `docs/plan/04-security.md` §3–§4, §8–§9, §14.
- **Do not:** let the local unlock depend on any server (it must work offline and after a move); keep a plaintext device key beside a sealed one longer than one successful unlock; send a login, token or vault byte to a discovered server before the person confirmed its address and fingerprint; show server words on the create path; mix the demo vault into a real one.

## 1. The plan

### 1.0 The calls this builds to, and what this plan changes in how they are met

| # | Call (coordinator, under D375) | Met by | Raised |
|---|---|---|---|
| 1 | A local account made in the app: a name and a master password; OPAQUE, Secret Key, recovery kit; registered with a server the app runs itself, in process, no socket; moving later copies the account | §1.1, §1.2 | **R1, R2, R5** below |
| 2 | The master password unlocks the vault; Touch ID / Windows Hello once after setup, password after a restart and after N days; the CLI unlocks the same way; existing passwordless installs choose a name and password on next open, nothing lost, with a test | §1.3–§1.6 | **R2, R3, R4, R6** |
| 3 | No server in the create flow | §1.8, §1.9 | none |
| 4 | Detection, never auto-connect, in this source order, with pick-or-type everywhere | §1.7 | **R7** |
| 5 | First run is roomy; Comfortable roomier; Compact about today's Comfortable; tokens only; the unicron request | §1.12 | none |
| 6 | The six-screen flow and its expectations | §1.9 | **R8** |
| 7 | The unlock screen | §1.10 | none |
| 8 | Move to your own server | §1.11 | **R5** (built, not a gap) |

**What is unsafe or unworkable as written, and the alternative** (each is reflected in the sections it names):

- **R1. OPAQUE against the in-process server can't be the local unlock.** OPAQUE's export key depends on the server's OPRF key, so after a move to a real server the account slot opens only with that server's answer: unlock would need the network, and offline unlock is a promise (03-architecture §6). **Alternative:** the in-process server holds the *account* (registration, sessions, identity records, the gates' password check); the *unlock* is a separate **local password slot** on this device that never talks to any server (§1.3). Same password, two independent uses; nothing changes for the person.
- **R2. A password that only the UI checks protects nothing on disk.** Today the identity crate's file keystore (`device-key.cbor`) keeps the key that opens the identity's device wrap in plain bytes (D83, D87): anyone who copies the data directory opens the vault. **Alternative:** the device's three secrets (unlock key, signing seed, receiving seed) are sealed under a key stretched from the master password, in a Lockdown keystore that implements the crate's public `DeviceKeystore` trait (§1.3). This amends D87 ("the shell must not grow a second backend") for the core, and goes to the crate owner as a request to adopt it upstream, so Mailbird and Synapse unlock the same way.
- **R3. Sibling apps can reopen the identity without the password.** An ecosystem app on the same computer that holds its own plaintext device key for the same identity (D11's local handshake) can open the IRK, then Lockdown's App Data Key, without the master password. No sibling ships today, so this slice states the limit in the docs (§1.3) and the crate request in R2 closes it.
- **R4. Touch ID unlock needs a key the OS holds behind the fingerprint.** On macOS that is a keychain item with a biometry access control, which needs the Developer ID's keychain entitlement; the ad-hoc signed build (D450) can't make one. A Touch ID prompt in front of a key kept in memory would only be a curtain. **Alternative:** offer Touch ID unlock only where a biometry-bound item can be made; on the ad-hoc build the step is skipped and Settings says why (open question Q1).
- **R5. "Moving copies the account, doesn't re-create it" holds for everything but the login record.** The identity (profile, keys, devices, kit), every vault and its signed history move as they are: same profile id, same vault ids, same head chains. The OPAQUE login record can't move between servers (it is bound to each server's OPRF key), so the move registers the same login and password on the new server and re-wraps the account slot under its export key. The person types the master password once during the move (§1.11).
- **R6. Passwordless installs don't keep their Secret Key.** A "This device only" kit is made without an account, and the Secret Key is not kept (`secret_key::forget`). Registering the local account needs it, so the migration asks for the kit's Secret Key, or makes a new kit (the old page stops working) (§1.6, Q2).
- **R7. A cargo cycle.** `lockdown-server` already depends on `lockdown-app` (the restore drill reads an item back), so the core can't start the server itself. **Alternative:** `lockdown-sync` gains a second, additive hook for one reserved address, `lockdown-local://this-device`; the desktop and the CLI (which may depend on both crates) install the in-process server there at start (§1.1). The 42 transport call sites in `lockdown-app` stay as they are.
- **R8. A local-only vault has one copy.** The kit gets a person back in on *this* computer after a forgotten password. If the computer is lost, the kit alone can't bring the vault back: nothing else holds it until backups are on (D143, off by default) or a second device exists through a server. The kit screen says so in its Learn more, and the plan proposes a later nudge (§1.14 gaps). Also, D251 puts the kit test straight after first run; 04-security §9 says "put it away and come back". Q4 asks to move it to the first unlock a day later.

### 1.1 The in-process local server

| Part | Plan |
|---|---|
| What runs | `lockdown_server::app(state)` (the router the server binary serves), built over `AppState::new(SqlStorage::open(<data>/local-server/server.sqlite), ServerKeys::load_or_create(<data>/local-server/server-key.cbor), Config::local())`, driven as a tower service: each request is `router.clone().oneshot(req)` on a small current-thread tokio runtime the host owns. No listening socket, no port, no TLS. |
| `Config::local()` | Signups open (one account in practice), the same session and renewal limits as any server (D401), rate limits off (there is no address to brake), blobs on with no quota, mail, admin page, breach monitor, mirrors, backups, push and the web vault off. The live channel isn't used: the app skips `/v1/events` when the account is local. |
| How the core reaches it | `lockdown-sync` gets `install_local_server(hook)` and the reserved base `lockdown-local://this-device` (R7). `check_base` accepts that base only when the hook is installed; `HttpTransport::exchange` routes it to the hook and everything else as today. `SyncConfig.server` holds the reserved base for a local account. No call site in `lockdown-app` changes. |
| Who installs it | The desktop shell at start (`apps/desktop/src-tauri`, before the core opens anything) and the CLI (`bins/cli`, which moves `lockdown-server` from a dev-dependency to a dependency). The web vault never does; its first run stays server-backed (D176). |
| Two processes | The desktop app and the CLI share the data directory (D86), so both may run the local server over one store. Every request holds an exclusive `std::fs::File::lock` on `<data>/local-server/lock` for its duration, so the log, the heads and the sessions see one writer at a time. |
| Where it lives | `<data>/local-server/` (`server.sqlite`, `server-key.cbor`, `blobs/`, `lock`), files 0600 in a 0700 folder, inside Lockdown's data directory so a reset (§1.13) removes it without knowing about it. |
| What it costs | A second copy of the vault's ciphertext and attachments on the same disk, and the server crate's code in the desktop and CLI binaries. The server crate gains a `local` feature set (no Postgres, S3, SMTP) so the desktop links only what it uses; the server binary keeps every feature by default. |
| What it isn't | A server for other devices. Serving this computer's vault to a phone is a later, advanced, off-by-default choice and not in this slice. |

The local server's words never reach a screen: the core reports an account's kind (`local` or `server`) beside its address, and the shells draw server words only for `server` (§1.8).

### 1.2 The account, the name and the kit

- **The name** is a label: what the lock screen shows, 1 to 64 characters, trimmed, no control characters; not an email, never sent anywhere. It is also the login on the local server (where nobody else sees it). Renaming later changes the label only (Settings → Lock → "Your name").
- **Creating** (one core call, `first_run_create(name, password)`): create the identity and the first vault (as `open_or_create(true)` does today), register the local account with OPAQUE and a fresh recovery kit (as `account_register` does, so the account slot holds the Secret Key, D70, and the Secret Key is kept sealed here, D247), write the local password slot and the sealed keystore (§1.3), then return the kit page once. Every step is the existing code apart from the slot.
- **The kit page for a local account** omits the server line and the `lockdown recover` line (they would print the reserved address). It says instead: "To use this kit, open Lockdown on this computer and choose Forgot your password? Use your recovery kit." The page keeps working after a move: the Secret Key and the kit wrap don't change.
- **Save as PDF and Print** come from `docs/plan/38-mac-fixes.md` (the mac-fixes slice): this slice reuses its kit PDF and the print fix and does not rebuild them. If that slice hasn't merged when phase 2 starts, the kit screen ships with today's print rule and the PDF button waits for the merge (written up as a gap, not stubbed).
- **An interrupted first run.** `onboarding.cbor` beside `settings.cbor` records the stage (`account_made`, `kit_saved`, `quick_offered`, `import_done`, `done`), the name, and whether this first run created the identity root (§1.13). Nothing secret. If the app quits after the account is made but before the kit was confirmed saved, the next unlock (with the master password) resumes at the kit screen with a **new** kit (`kit_create(replace: true)` with the password in hand): the first page was never confirmed saved and was never kept.

### 1.3 Lock and unlock: the local password slot and the sealed keystore

```
master password ──Argon2id(salt, params ≥ desktop floor, ~1 s)──► KEK (32 bytes)
                                                                   │ XChaCha20-Poly1305
device-key.sealed.cbor:  plain  {v, device_id, device_pub, kem_alg, kem_pub, platform, created_at, salt, params}
                         sealed {unlock_key, signing_seed, kem_seed}   AAD = {v, device_id, "lockdown/local-unlock/v1"}
unlock_key ──► the identity's device wrap of the IRK ──► persona keys ──► App Data Key ──► vaults (unchanged)
```

| Rule | Detail |
|---|---|
| The slot is local | It never syncs and never talks to a server, so unlock works offline and after a move (R1). |
| Stretching | Argon2id at or above the desktop floor (64 MiB, 3 passes, 4 lanes; 04-security §4), calibrated to about one second on this computer; the parameters only rise; a file below the floor is refused. |
| What is sealed | The three device secrets. The public halves stay plain, so a locked app can still say which device it is. Lock drops the opened keystore with the session; every secret type zeroes on drop (04-security §14). |
| The keystore | `lockdown_app::local_unlock::SealedKeystore` implements the identity crate's `DeviceKeystore` and the vault's `Signer`; `Session` holds it where it holds `FileKeystore` today. `unlock()` returns the unlock key only while the keystore is open. |
| Wrong password | The AEAD tag fails: "That isn't the master password for this vault." Nothing else is said, nothing is counted on disk (an attacker with the disk isn't slowed by a counter anyway; the stretching is the brake). |
| Unlock | `unlock_with_password(password)`: open the keystore, then `Session::open(Mode::Open)` exactly as today. |
| The gates (D101) | `Reveal` asks the master password and checks it against the local slot when the account is local (no server round, no extra session); against the account's server when the account is on a server, as today. `Use` and `Approve` go to Touch ID or Windows Hello when installed, as today. |
| At rest, honestly | With the disk alone, someone can guess the master password offline at one stretched try per guess; the Secret Key doesn't help on this computer, because this computer keeps it to open the account. That is the same as any password manager's local vault. On a new computer, the password and the Secret Key are both needed (unchanged). Sibling ecosystem apps with plaintext device keys weaken this (R3). |
| Password changed elsewhere | When a sign-in to the account's server succeeds with a password the local slot doesn't open (changed on another device), the slot is sealed again under it. Until then the old password unlocks this computer. |

### 1.4 Touch ID and Windows Hello ("quick unlock")

| Rule | Detail |
|---|---|
| Offered | Once, on first-run screen 3 (and in Settings → Lock any time), only where `quick_unlock::available()` says this build can hold a biometry-bound key (R4). |
| What it holds | On a password unlock with quick unlock on, the core makes a random 32-byte **R** that lives only in memory for this launch, and writes the KEK sealed under R into the OS: a macOS keychain item with `kSecAccessControlBiometryCurrentSet` (service `design.snds.lockdown`, account `quick-unlock:<device id>`); on Windows, the KEK sealed under a key derived from a Windows Hello `KeyCredentialManager` signature over a fixed challenge (a TPM-held key; the signature is deterministic). Memory alone (R) or the keychain alone opens nothing. |
| Unlock with it | The OS asks for the fingerprint (or face, or PIN); the core unseals with R, opens the keystore. |
| Password again | After the app restarts (R is gone), **14 days** after the last password unlock (the time is kept in memory, so editing a file can't stretch it), after the fingerprints enrolled on the computer change (the item is invalidated by the OS), and after 5 failed quick unlocks. Turning quick unlock off deletes the item. |
| D252 | The 14-day password reminder becomes redundant on the desktop (the password is now asked at least that often); it stays for any device whose unlock never asks. |
| Verification | Both platform halves compile-check against their targets in CI; neither can run in this environment, so the first run on real hardware confirms the prompt and the invalidation (gap until then). |

### 1.5 The command line

- Every command that opens the vault takes the master password from `--password-stdin`, `LOCKDOWN_PASSWORD`, or, when stdin is a terminal and neither is given, a prompt without echo: `Master password for Sean:`. The prompt is new (amends D47, which allowed only stdin and the variable).
- `lockdown init --name NAME` makes the local account: the password twice (stdin or the prompt), then the kit printed or saved as `account register` does today. `lockdown init` without `--name` says: `lockdown init needs --name NAME: the name your vault shows when it's locked.`
- A locked vault and no password: `This vault is locked with a master password. Give it with --password-stdin or LOCKDOWN_PASSWORD, or run the command in a terminal to be asked.`
- Wrong password: `That isn't the master password for this vault.`
- An install without a master password yet keeps working and prints once per command to stderr: `This vault has no master password yet. Run lockdown password set --name NAME, or open the Lockdown app.` `lockdown password set` runs the migration of §1.6.
- `lockdown password reset --kit-stdin` is the forgotten-password path of §1.10.
- The CLI's own copy of the flows (`bins/cli/src/app.rs`) takes the sealed keystore the same way the app's session does.

### 1.6 Existing installs

| Install | What it sees on next open | What happens |
|---|---|---|
| "This device only", no account (desktop or `lockdown init`) | The vault opens with the device key one last time, and before the list draws, screen M1 (§1.9) asks for a name and a master password. It can't be skipped (Q2). | Register the local account. The identity already has a kit, so its Secret Key is needed (R6): screen M2 asks for it, or "I don't have it: make a new kit", which replaces the kit and shows the new one (the old page stops working). Then the local slot, then the sealed keystore. |
| An account on a real server | Screen M3: one password field. | The password is checked with a sign-in to the account's server, then becomes the master password locally. Offline: the vault opens as before and M3 comes back next time. The name is the login. |
| Identity present, this device not enrolled | Unchanged ("This device isn't enrolled yet"). | Nothing. |

The switch is ordered so nothing is lost: (1) register or check; (2) write `device-key.sealed.cbor` through a temporary file and rename; (3) reopen it in memory with the KEK and check it yields the same device id and public keys; (4) only then overwrite and delete `device-key.cbor`. A crash between (2) and (4) leaves both files: the probe reports the sealed one, and the next successful password unlock removes the plain one. The test makes an install the old way with items in two vaults, a folder, a tag, an attachment and a crown jewel, migrates it, and checks the same item ids, versions, head hashes and attachment bytes, that the plain key file is gone, that unlock without the password is refused, and that a crash between (2) and (4) recovers.

### 1.7 Discovery

A background module in the core (`lockdown_app::discovery`, native builds only) that **finds and describes** servers. It never signs in, never sends a token, never moves anything. "Connecting" means any request that carries a login, a token or vault data; only a confirm (below) starts one.

**Sources, in rank order:**

| # | Source | What is asked | When |
|---|---|---|---|
| 1 | Managed policy (`crates/policy`, new `managed` module) | A JSON file an administrator places: `/Library/Application Support/Lockdown/policy.json` (macOS), `/etc/lockdown/policy.json` (Linux): `{"v":1, "server":"https://…", "server_key":"<hex>", "lock":false}`. Read only when owned by root and not writable by group or others. | At start |
| 2 | This computer's config | `sync.cbor` (an account the desktop or CLI already uses) and `settings.cbor`'s suggested server. | At start |
| 3 | A server on this computer | `GET http://127.0.0.1:8787/.well-known/lockdown` and `http://[::1]:8787/…` (the server binary's default listen address), 500 ms. | At start, and on "Look again" |
| 4 | The local network | DNS-SD browse for `_lockdown._tcp.local.` for 3 s; each answer's target and port, then that address's `/.well-known/lockdown`. The server gains `--announce` (§1.14, Q6). | At start, on opening "I already use Lockdown" or Settings → Server, and on "Look again" |
| 5 | The person's email domain | After the person types an email-shaped login on the "I already use Lockdown" path (and only there), `https://<domain>/.well-known/lockdown`, then the SRV record `_lockdown._tcp.<domain>` and that target's well-known document. | 600 ms after the login field stops changing |

**Ranking.** By source as above; within a source, a usable answer before an unusable one, then by address A–Z (no hash order anywhere). The same address found twice is one candidate listing every source. A candidate is **usable** when its well-known document decodes, names an API version this build speaks, and carries a 32-byte log key.

**Every answer is hostile.** Responses capped at 16 KiB, read with a 2 s connect and 3 s total timeout, redirects not followed, `https://` required except on loopback (`check_base`), no cookies, no credentials, at most 16 candidates per source, names and versions kept only when printable and short (63 and 64 characters). TXT records are ignored apart from `path`. Nothing found is written to disk until the person confirms it.

**Vaultwarden.** When a typed address or a domain's apex answers no Lockdown document, one more `GET /api/config`: a Bitwarden-shaped answer (a `version` and a `server.name`) is listed as "Not a Lockdown server: this looks like Vaultwarden or Bitwarden" with **Bring its passwords in** (opens Import with Bitwarden JSON chosen). Nothing else is guessed.

**The confirm.** Before any connection, a sheet shows the address, where it was found, and the server key fingerprint: the log key through `derive_key("lockdown/server-fingerprint/v1", key)` shown as six groups of five digits, the same form as people's fingerprints (D190). A policy that names `server_key` turns the line into "Matches the key your organization set." or refuses a mismatch. On **Connect**, the confirmed log key is pinned for that address before the first request, so the first sync refuses a log signed by any other key (today's first-sight pin is replaced by this one). An address whose pinned key changed is refused in words (§1.11). Integrity never rests on the confirm: OPAQUE never shows a server the password, and the signed history and the log checks catch a server that lies. The confirm is for the person to recognize where their vault is going.

**What the shells see:** `discovery_candidates()` (the list, with each candidate's `address`, `sources`, `state`: `ready` / `unreachable` / `needs_https` / `not_lockdown` / `vaultwarden` / `wrong_key`, `fingerprint`, `version`) and `discovery_check(address)` for a typed address, through a `DiscoveryClient` provider so the web vault compiles the views without it.

### 1.8 Where server words may appear

Server words (server, sync, account address, the public log, mirrors, sessions on a server) appear only when (a) the account is on a server, (b) discovery found a candidate and the screen is showing it, or (c) the person chose "I already use Lockdown" or opened Settings → Server. For a local account:

| Screen | Today | Local account |
|---|---|---|
| Unlock | Server field in tabs | None (§1.10) |
| Status | Account, sync, log, mirrors, quantum-safe line, restore drill | Signed history and kit lines only, plus "Your vault is kept on this computer only." with **Set up sync…** (opens Settings → Server) |
| Devices | Add a device, Signed in, Security keys | This device; **Add a device** opens one sentence: "To use Lockdown on another device, both need a server they can reach. You can set one up in Settings → Server." |
| Sharing | Every section | One sentence: "Sharing with people needs a server you and they can reach. You can set one up in Settings → Server." |
| Settings → Approvals | "account password" | "master password" (and everywhere else, §1.9 copy rule) |
| Settings → Server, This server | Suggest field, drills, mirrors | §1.11 |

**Copy rule:** every user-facing "account password" becomes "master password" (core messages and views). CLI flags keep their names.

### 1.9 The screens, in order, with their words

**Layout for every first-run screen** (density-independent, §1.12): the window's canvas (`bg-background`, not the locked wash), one centered column `--c-welcome-column` wide, `--c-welcome-pad-block` above and below, the page gutter at the sides (16 px below `sm`). Top to bottom: the step line, the heading, the one sentence with **Learn more**, the controls, then actions stacked full width, primary first, secondary as outline, a tertiary as a link. One `h1` per screen; focus moves to it on each step; errors are `role="alert"` under the control they concern; every button keeps its words at every width.

```
          ┌──────────────── 512 px column, centered ────────────────┐
          │                      [mark 48]                          │   ← Welcome only
          │  Step 2 of 4                         (muted, text-sm)   │
          │                                                         │   gap 3u
          │  Heading                                (text-3xl)      │
          │  One sentence. Learn more               (text-lg)       │
          │                                                         │   gap 8u
          │  Field label                            (text-base)     │
          │  [__________________________________________]           │
          │  help line                              (muted)         │
          │                                                         │   gap 8u
          │  [            Primary action             ]              │
          │  [            Secondary (outline)        ]              │
          │               tertiary link                             │
          └─────────────────────────────────────────────────────────┘
```

The step line counts the screens that ask something: "Step 1 of 4" with Touch ID or Windows Hello (name and password, kit, Touch ID, bring your passwords), "Step 1 of 3" without. Welcome and the proof screen carry no step line.

#### Screen 0 · Welcome

```
                 [Lockdown mark]
   Every way you sign in. Locked down, even from us.
   Lockdown keeps your passwords, passkeys and one-time
   codes in one vault on this computer. Learn more

   [            Create a new vault            ]
   [           I already use Lockdown         ]
     We found a Lockdown server on your network: nas.local      ← only when found
                Explore the demo                                ← dev builds, §1.13
```

| Part | Words |
|---|---|
| Heading | Every way you sign in. Locked down, even from us. (the tagline, shown once in the app; brief §1) |
| Sentence | Lockdown keeps your passwords, passkeys and one-time codes in one vault on this computer. |
| Primary | Create a new vault |
| Secondary | I already use Lockdown |
| Found line (only when discovery has a usable candidate) | "We found a Lockdown server on this computer: localhost:8787" · "…on your network: nas.local" · "Your organization set a server: vault.example.org". Clicking it opens screen J with that server chosen. |
| Tertiary (only when the demo is compiled in) | Explore the demo |
| Learn more (dialog, title = heading) | "Your vault is encrypted on this computer, and only your master password opens it. Lockdown never sees that password." · "Each sign-in keeps its password, passkeys and one-time codes together, so one account is one record." · "Later you can add your other devices, or move to your own server. Nothing here needs an account with us." · "You can take everything out at any time, in a file other apps can read." · button "Got it" |
| Busy | none (nothing happens until a button) |
| Error | The probe failed: heading "Lockdown can't start", the core's sentence below (unchanged). |

#### Screen 1 · Your name and master password

```
   Step 1 of 4
   Choose your master password
   This is the one password to remember: it opens your vault,
   and Lockdown can't see it or reset it. Learn more

   Your name
   [ Sean                                          ]
   Shown when Lockdown is locked. It stays on this computer.

   Master password
   [ ••••••••••••••••••••••••              ] [Show]
   ▓▓▓▓▓▓▓▓▓░░░  Strength: Fair
   A long phrase beats a clever word: four or more ordinary
   words that don't belong together are hard to guess.

   Type it again
   [ ••••••••••••••••••••••••              ]

   [                Continue                ]
   [                  Back                  ]
   If you forget it, only your recovery kit gets you back in.
   You'll make the kit next.
```

| Part | Words |
|---|---|
| Heading | Choose your master password |
| Sentence | This is the one password to remember: it opens your vault, and Lockdown can't see it or reset it. |
| Field 1 | Label "Your name". Help "Shown when Lockdown is locked. It stays on this computer." Filled in with the computer account's full name when the OS gives one. |
| Field 2 | Label "Master password". Help "A long phrase beats a clever word: four or more ordinary words that don't belong together are hard to guess." Toggle "Show" / "Hide" (`aria-pressed`). Meter: the core's `password_strength` score with the name as context, through unicron's `secretStrengthFromScore`: "Strength: Weak" / "Strength: Fair" / "Strength: Strong"; nothing drawn while empty. "Caps Lock is on." under the field while it is. |
| Field 3 | Label "Type it again". |
| Note under the actions | If you forget it, only your recovery kit gets you back in. You'll make the kit next. |
| Primary / secondary | Continue / Back |
| Errors | Name empty: "Type a name. It's what the lock screen shows." · Name too long: "Keep the name to 64 characters or fewer." · Weak (score under 3, Q3): "This password protects everything else, so it needs to be at least fair. Add another word or two." · Mismatch: "The two passwords don't match yet." · Core failure: the core's sentence, e.g. "This computer ran out of memory while protecting your password. Close some apps and try again." |
| Busy (after Continue, about 3 s) | The button shows a spinner and "Setting up your vault…"; the fields are read-only; under the button: "This takes a few seconds." |
| Learn more | Title "Your master password". "Lockdown turns your master password into the key that opens your vault, here on this computer. The password itself is never stored and never sent anywhere." · "That's why nobody, Lockdown included, can tell you what it was or reset it. If you forget it, your recovery kit is the only way back in." · "Length matters most. A phrase of four or more ordinary words is easier to remember, and much harder to guess, than a short word with symbols." · "Your name is only a label for the lock screen. It isn't an email address and isn't sent anywhere." |

#### Screen 2 · Your recovery kit

```
   Step 2 of 4
   Save your recovery kit
   If you forget your master password, this page is the only
   way back into your vault. Learn more

   ┌ Recovery kit for Sean ───────────────────────────┐
   │ Secret Key      A1-XXXXXX-XXXXXX-XXXXX-…          │   ← shown on arrival,
   │ Recovery code   XXXX-XXXX-XXXX-…                  │     hidden on blur, lock
   └───────────────────────────────────────────────────┘     or after 60 s
   Keep it somewhere safe away from this computer, such as a
   drawer at home or with your important papers.

   [            Save as PDF             ]   ← primary until checked
   [               Print                ]
                Show again                   ← when hidden
   [x] I saved, printed or wrote down the kit
   [              Continue              ]   ← primary once checked
```

| Part | Words |
|---|---|
| Heading | Save your recovery kit |
| Sentence | If you forget your master password, this page is the only way back into your vault. |
| Panel | Title "Recovery kit for *name*"; rows "Secret Key", "Recovery code", in mono, grouped for reading aloud (LargeTypeView's print variant). The window is capture-protected while the panel shows (D316). |
| Where to keep it | Keep it somewhere safe away from this computer, such as a drawer at home or with your important papers. |
| After a PDF is saved | "Saved. A PDF on this computer alone won't help if this computer is lost: print it, or copy it somewhere else safe." |
| Actions | Save as PDF · Print · Show again (only while the panel is hidden) · checkbox "I saved, printed or wrote down the kit" · Continue (disabled until checked). Save as PDF is the primary until the box is checked; then Continue is, and Save as PDF turns outline. |
| Errors | Save failed: "Lockdown couldn't save the PDF: *reason*. Try another folder, or print it instead." · Print cancelled: nothing. |
| Busy | Saving: the button shows a spinner and "Saving…". |
| Learn more | Title "Your recovery kit". "The kit holds your Secret Key and a recovery code. Lockdown made them here, keeps no copy of the page, and has never seen your Secret Key." · "With the kit you can choose a new master password on this computer if you forget yours. Anyone with both the kit and this computer could open your vault, so keep them apart." · "If this computer is lost, the kit brings your vault back only once you've turned on backups or added another device. You can do both later." |

#### Screen 3 · Unlock with Touch ID? (only where quick unlock is available)

Layout: step line · heading · sentence and Learn more · a fingerprint glyph (decorative, 12 u) · primary · secondary · the note.

| Part | Words |
|---|---|
| Heading | Unlock with Touch ID? (Windows: "Unlock with Windows Hello?") |
| Sentence | Use your fingerprint instead of typing your master password most of the time. (Windows: "Use your face, fingerprint or PIN instead of typing your master password most of the time.") |
| Primary / secondary | Use Touch ID (Use Windows Hello) / Not now |
| Note | You can change this later in Settings → Lock. |
| Busy | The OS prompt is up; the buttons are disabled. |
| Errors | Cancelled or failed: "Touch ID didn't finish, so it's off for now. You can turn it on in Settings → Lock." (stays on the screen; the primary becomes Continue) |
| Learn more | Title "Unlocking with Touch ID". "Lockdown still asks for your master password each time it starts, and at least every 14 days, so you don't forget it." · "Your fingerprint never reaches Lockdown: macOS checks it and then releases a key kept for this vault." |

#### Screen 4 · Bring your passwords (optional)

```
   Step 4 of 4
   Bring your passwords
   Move your passwords in from the app you use now, or skip
   this and add them as you go. Learn more

   ┌──────────────────────────────────────────────────┐
   │ 1Password (.1pux)                               ›│
   │ In 1Password: File, Export, choose your account,  │
   │ and pick the 1PUX format.                         │
   ├──────────────────────────────────────────────────┤
   │ Bitwarden or Vaultwarden (.json)                ›│
   │ …                                                │
   └──────────────────────────────────────────────────┘
            Other apps and authenticator files
   [              Skip for now              ]
```

| Part | Words |
|---|---|
| Heading | Bring your passwords |
| Sentence | Move your passwords in from the app you use now, or skip this and add them as you go. |
| Rows (each opens the import wizard with that format chosen; names and how-to lines come from `lockdown-cxf`'s `Format::name` / `Format::how`, so only importers that exist are listed) | 1Password (.1pux) · Bitwarden or Vaultwarden (.json) · Your browser (.csv): "In your browser's password settings, export your passwords as a .csv file." (the generic CSV importer; Chrome, Edge, Firefox and Safari headers checked by a test in phase 2) · KeePass or KeePassXC (.kdbx) · Proton Pass · Dashlane · LastPass (.csv) · Keeper · Enpass (.json) · A CXF file (credential exchange) |
| Link | Other apps and authenticator files (the full wizard, every format) |
| Actions | Skip for now (outline) until something was brought in; then the primary is Continue and "Bring in another" is a link. |
| After an import | The wizard's own done line ("12 items brought in."), then: "Delete the exported file now: it isn't protected." |
| Errors | The wizard's own (a hostile file refused with the parser's sentence). |
| Learn more | Title "Bringing your passwords". "Lockdown reads the file on this computer and shows what it holds, and anything it can't bring along, before anything is saved." · "Most apps export a file that isn't protected. Once your passwords are in, delete that file." |

#### Screen 5 · The proof moment (D95)

Layout: heading · sentence · three lines, each with a decorative check glyph and the word in the line itself · the primary. No step line, no Learn more (the proofs are the detail).

| Part | Words |
|---|---|
| Heading | Locked, even from us |
| Sentence | Three things are true from now on; Status can check them again any time. |
| List | Your master password never leaves this computer. · Lockdown has never seen the Secret Key on your kit. · Your vault's history is signed on this computer, so a swapped or older copy is caught. |
| Primary | Open the vault |

The kit test (D251) follows as today, unless Q4 moves it to the first unlock a day later.

#### Screen J · I already use Lockdown

```
   Add this computer to your Lockdown
   Let a device you already use approve this one, or sign in
   to your server. Learn more

   ( ) Approve from another device        ← each a radio row
       Show a code on a device that's signed in, then match six digits.
   ( ) Sign in with your login and password
   ( ) Use your recovery kit
       For when you've lost every device.

   Server
   (•) nas.local · On your network · Ready
   ( ) localhost:8787 · On this computer · Ready
   ( ) Type an address
       [ https://                                  ]
   Look again

   [               Continue               ]
   [                 Back                 ]
```

| Part | Words |
|---|---|
| Heading | Add this computer to your Lockdown |
| Sentence | Let a device you already use approve this one, or sign in to your server. |
| Ways | Approve from another device ("Show a code on a device that's signed in, then match six digits.") · Sign in with your login and password · Use your recovery kit ("For when you've lost every device.") |
| Server list | One row per candidate: address · where ("On this computer", "On your network", "Set by your organization", "From your settings", "From *domain*") · state ("Ready", "Can't reach it", "Needs https", "Not a Lockdown server", "This looks like Vaultwarden or Bitwarden"). The top usable one is chosen; nothing is chosen when none is usable. "Type an address" reveals the field (placeholder `https://`, help "The address of your Lockdown server. Whoever runs it can tell you."). A policy with `lock` hides the other rows and the field. "Look again" reruns discovery. Approving from another device with a scanned QR needs no server choice (the QR carries it, D246). |
| Then | The existing forms, re-laid in the column: login and master password (label "Master password", help "The one you use on your other devices."), Secret Key when asked, the recovery code for the kit; the waits for a security key or an email approval (D250, D300) unchanged. After the sign-in, the typed password becomes this computer's master password (§1.3), the name is the login, then screen 3 (Touch ID) and the vault. No kit screen, no proof screen (D95: once, at account creation). |
| Domain line | While an email-shaped login is typed: "Looking for a server at example.com…" then the found candidate joins the list (chosen only if none was). |
| Confirm (before the first sign-in request) | Title "Connect to *host*?" · rows "Address", "Found", "Server key" (six groups of five digits) · sentence "Only continue if you recognize this server. Lockdown checks this key every time it syncs." · "Connect" / "Cancel". |
| Errors | No server chosen: "Choose a server, or type its address." · Not https: "Lockdown only talks to a server on another computer over https." · Unreachable: "Lockdown can't reach *host*: *reason*." · Key changed: "*host* has a different key from the one you confirmed before, so Lockdown won't connect. If whoever runs it changed it on purpose, ask them, then remove the server in Settings → Server and add it again." · Vaultwarden: "*host* is a Vaultwarden or Bitwarden server, not a Lockdown server. You can bring its passwords in instead." with "Bring its passwords in". · Sign-in errors: the core's words, unchanged. |
| Learn more | Title "Using Lockdown on more than one device". "Your devices stay in step through a Lockdown server. It stores only locked data it can't read, and Lockdown checks its work every time it syncs." · "Approving from another device is quickest: no kit to type." |

#### Screens M1–M3 · Existing installs (§1.6)

Layout: as screen 1 (no step line; M1 is followed by M2 when a kit exists, then screen 2 when a new kit was made, then screen 3). Busy: the primary shows a spinner and "Locking your vault…".

| Screen | Heading · sentence | Fields and actions | Learn more |
|---|---|---|---|
| M1 (device-only) | "Lock your vault with a master password" · "From now on Lockdown asks for a master password to open your vault; everything in it stays as it is." | Screen 1's three fields and help; Continue. No Back, no skip. | "Until now your vault opened with a key kept in a file on this computer, so anyone who copied that file could open it. Now that key is locked with your master password." · then screen 1's paragraphs. |
| M2 (a kit exists) | "Type your Secret Key" · "Your vault already has a recovery kit; its Secret Key also protects your master password." | "Secret Key" (help "On your recovery kit page; it starts with A1-."); Continue; link "I don't have it: make a new kit" → confirm "Make a new kit? Your old kit page stops working." / "Make a new kit" / "Cancel" → screen 2. | Error: "That Secret Key doesn't match this vault's kit. Check it letter by letter, or make a new kit." |
| M3 (an account on a server) | "Unlock with your password from now on" · "Type your password once and Lockdown will use it to lock this vault on this computer; everything in it stays as it is." | "Master password" (help "The password you sign in to *host* with."); Continue. | Offline: "Lockdown needs to reach *host* once to check your password. You can keep using your vault for now; it asks again next time." with "Continue for now". |

### 1.10 The unlock screen, and a forgotten password

```
                 [mark 40]
                   Sean
   Locked after a while without you.        ← the lock reason, or "Your vault is locked."

   Master password
   [ ••••••••••••••••••••••••              ] [Show]
   [                 Unlock                 ]
   [         Unlock with Touch ID           ]   ← when quick unlock is ready
   Forgot your password? Use your recovery kit
```

Same column and spacing as the first run, at `--c-welcome-*`, on the locked wash's canvas (brief §4: locked content goes first, then this draws).

| Part | Words |
|---|---|
| Heading | The name (the login for a device that joined a server account) |
| Sentence | The lock reason ("Locked.", "Locked after a while without you.", "Locked while the window was out of view.", "Locked while this computer slept.") or "Your vault is locked." |
| Field | "Master password", with Show / Hide and the Caps Lock line. Focused on show. |
| Primary | Unlock |
| Quick unlock | "Unlock with Touch ID" (outline). The OS prompt opens by itself once when the screen shows with the window focused. When it isn't ready, one muted line says why: "Touch ID works again after you unlock with your password once." (restart) · "It's been 14 days, so Lockdown asks for your master password." · "Your fingerprints changed, so Lockdown asks for your master password." |
| Link | Forgot your password? Use your recovery kit |
| Errors | "That isn't the master password for this vault." After three wrong tries the link reads "Still not it? Use your recovery kit". |
| Busy | "Unlocking…" in the button (about a second of stretching). |
| Server words | None, ever, on this screen. |

**Forgot your password** (two steps in the same column, laid out as screen 1 with no step line; core call `password_reset_with_kit`; busy: "Checking the kit…", then "Saving…"):

| Step | Words | What happens |
|---|---|---|
| K1 | Heading "Use your recovery kit" · sentence "Type the Secret Key and recovery code from your kit to choose a new master password." · fields "Secret Key" (help "It starts with A1-."), "Recovery code" · "Check the kit" / "Back" | The kit opens the IRK through its wrap (as `kit_verify` does). Error: "That kit doesn't open this vault. Check each character against the page." |
| K2 | Heading "Choose a new master password" · sentence "Your old password stops working once you save the new one." · screen 1's password, meter and confirm fields · "Save new password" | A session from the device key (D400's renewal, no password), the authenticated re-registration (`/v1/account/password/start|finish`, as `strengthen_account` does) with the new password, the account slot re-wrapped under the new export key and the kept Secret Key, the local slot and quick unlock sealed again, then the vault opens. With the account on a real server this needs the network: "Lockdown needs to reach *host* to change your password there. Connect and try again." The CLI's `lockdown password reset --kit-stdin` does the same. |

### 1.11 Settings → Server, and moving to your own server

**For a local account:**

```
   Server
   Your vault is kept on this computer only. A server keeps your
   devices in step and lets you share. Learn more

   Found
   ┌──────────────────────────────────────────────────────────┐
   │ nas.local            On your network · Ready      [Use…] │
   │ localhost:8787       On this computer · Ready     [Use…] │
   │ vault.example.com    From example.com · Can't reach it    │
   └──────────────────────────────────────────────────────────┘
   Look again
   Or type an address
   [ https://                                ] [Check]
```

| Part | Words |
|---|---|
| Title / sentence | "Server" · "Your vault is kept on this computer only. A server keeps your devices in step and lets you share." |
| List | Discovery's candidates as in screen J; each usable one has "Move here…". Empty: "No Lockdown server found on this computer or your network." |
| Manual | "Or type an address" + "Check" (runs `discovery_check`, adds the row). |
| Learn more | Title "Your own server". "A Lockdown server stores only locked data it can't read, and Lockdown checks its work every time it syncs." · "You can run one yourself (the self-host guide shows how) or use one run by someone you trust." · "Moving copies your vault and its signed history; nothing on this computer is lost." |

**For an account on a server:** the same list and field, with the current server first, marked "In use"; today's "Server to suggest" field becomes the manual override; This server (drills, mirrors) stays below.

**Move to your own server** (dialog with steps; core call `move_to_server(address, login, password)`):

| Step | Words | What happens |
|---|---|---|
| 1 Confirm | The connect sheet of §1.7: "Move your vault to *host*?" · Address, Found, Server key · "Only continue if this is a server you or someone you trust runs." · Continue / Cancel | The key is pinned for that address. |
| 2 Login | Heading "Your login on *host*" · "Login on this server" (filled with the name; help "Often an email address. Ask whoever runs the server if they set one up for you.") · "Master password" (help "Your master password stays the same; Lockdown uses it to make your login there.") · "Move my vault" | The password is checked against the local slot first: "That isn't your master password." |
| 3 Copying | "Copying your vault to *host*…" with "*n* of *m* vaults" | Register on the new server with the kept Secret Key (same profile id; the account slot re-wrapped under that server's export key); publish the identity's records; push every vault from its first head (today's sync from an empty `SyncConfig.vaults`), with blobs; check the server's heads equal this device's for every vault. |
| 4 Done | Heading "Your vault is on *host* now" · "This computer keeps working offline and catches up when it can reach the server. You can add your other devices from Devices." · Done | Only now `sync.cbor` names the new server; the local server's folder is deleted. |
| Errors | Signups closed: "*host* isn't taking new accounts. Ask whoever runs it for an invite." · Login taken: "*login* is already taken on *host*. Choose another login." · Any failure in step 3: "The move stopped: *reason*. Your vault is unchanged and still on this computer. You can try again." (the local account stays the account; the half-filled remote account is left for the operator, and the retry signs in to it instead of registering again) | |

### 1.12 Density and the welcome layout, as tokens

Every value is `calc(var(--spacing) * n)` on Tailwind's spacing base (`--spacing`, 0.25 rem = 1 unit "u") or a Tailwind type token (`--text-*`), declared in `app-extension.css` tier 3 under `:root` and `:root[data-density="compact"]` (D92's attribute). No raw px.

| Knob | Comfortable (new default) | Compact | Today's comfortable · compact |
|---|---|---|---|
| `--c-row-height` (list rows) | 12 u (48 px) | 10 u (40 px) | 10 u · 8 u |
| `--c-row-pad-y` | 3 u | 2 u | 2 u · 1 u |
| `--c-row-gap` | 3 u | 2.5 u | 2.5 u · 2 u |
| `--c-group-pad-y` | 4 u | 3 u | 3 u · 2 u |
| `--c-nav-row-height` (new: sidebar entries, so navigation doesn't balloon) | 10 u (40 px) | 8 u (32 px) | sidebar used the row knob |
| `--c-page-gutter` (new: pane padding at `md` and up) | 8 u (32 px) | 6 u (24 px) | `md:p-6` (24 px) |
| `--c-page-gutter-narrow` (new: below `sm`) | 4 u (16 px) | 4 u (16 px) | `p-4` |
| `--c-section-gap` (new: between sections of a pane) | 10 u (40 px) | 6 u (24 px) | `gap-6` |
| `--c-shell-list-width` | 96 u (24 rem) | 88 u (22 rem) | 22 rem |
| `--c-shell-sidebar-width` | 60 u (15 rem) | 56 u (14 rem) | 14 rem |

Settings → Appearance's line becomes: "Comfortable is roomier, with taller rows and more space between sections. Compact fits more on the screen."

**The welcome layout** (first run, J, M1–M3, K1–K2, unlock), the same at either density:

| Knob | Value |
|---|---|
| `--c-welcome-column` | 128 u (32 rem, 512 px) |
| `--c-welcome-pad-block` | 16 u (64 px); 8 u below `sm` |
| `--c-welcome-gap` (between groups) | 8 u |
| `--c-welcome-gap-tight` (inside a group: label, field, help) | 3 u |
| `--c-welcome-mark` | 12 u (48 px); 10 u on the unlock screen |
| `--c-welcome-title` | `--text-3xl` with its line height |
| `--c-welcome-lede` | `--text-lg` |
| `--c-welcome-body` (labels, help, notes) | `--text-base` |
| Actions | Full column width at Button `size="lg"` (9 u) until unicron has a control-height knob; the request asks for 11 u (44 px) |

**The unicron request** (for `snds/unicron`, joins brief §11.1): density as a **token mode**, not app CSS. `component.json` knobs gain `$extensions["unicron.modes"].compact`, emitted under `[data-density="compact"]`, and the core primitives read them: `--c-row-height`, `--c-row-pad-y`, `--c-control-height` and `--c-control-text` (Button, Input, NativeSelect, Toggle heights and text, today fixed at `h-8` and `md:text-sm`), `--c-page-gutter`, `--c-section-gap`, `--c-nav-row-height`. Comfortable would set controls to 10 u with `--text-sm`, compact to 8 u. Also: a Button size for one-column flows (11 u), a "Step *n* of *m*" pattern, a radio-card list (the server picker, the import list), a Meter for strength, and a password input with show/hide and a Caps Lock line. Until they land, Lockdown keeps its knobs in `app-extension.css` and moves them when unicron ships the mode.

### 1.13 Where first run meets the demo vault and the developer reset

Both are built by another slice as removable code (`crates/devtools` and a front-end folder, behind the Cargo features `dev-reset` and `demo-vault`, off in tagged releases). First run's side of the seam:

- **Everything first run creates is findable by a reset that knows nothing about it:** the sealed device key, `onboarding.cbor`, the local server's folder and every other file under Lockdown's data directory; quick unlock and session tokens in the OS keystore under the service `design.snds.lockdown`; no WebView storage (the views keep nothing in `localStorage` beyond today's per-viewer conveniences).
- **One thing lives outside the data directory: the ecosystem identity root** (`…/snds/identity`, beside `…/snds/lockdown`, D47), shared by design with Mailbird and Synapse (D10, D11). A reset that leaves it behind relaunches into "This device isn't enrolled yet", a dead end. So first run records in `onboarding.cbor` that it **created** the identity root (the path and the profile id), and the reset removes that root only when first run created it and the identity lists no device of another app. The core exposes this as `onboarding::created_identity()`; the devtools slice decides what its menu does with it.
- **The demo** is offered from Welcome, recommended over a demo vault beside the person's own: **Explore the demo** (shown only when the core reports `demo_available`, i.e. the `demo-vault` feature is compiled in) opens the demo with no account and no master password, with a bar that stays on every screen: "You're exploring the demo. Nothing here is real." and **Leave the demo**, which returns to Welcome and leaves nothing behind. Why this over a vault next to the person's own: someone coming from 1Password can look around before choosing a master password; fake logins never show up in search, quick access, the browser's fill menu, the Security report or backups; and "remove in one click" is simply leaving. The first-run views don't import the demo code: Welcome takes an `onExploreDemo` callback and a `demoAvailable` flag from a provider the devtools front-end folder installs, so deleting that folder and the feature leaves first run whole.

### 1.14 What is in this slice, and what is a gap

**In this slice:** the in-process local server and its hook (§1.1); the local account, name and kit for local accounts (§1.2); the local password slot and the sealed keystore (§1.3); quick unlock's core rules and the macOS and Windows halves, compile-checked (§1.4); the CLI (§1.5); the migration with its test (§1.6); discovery from policy (macOS and Linux file), this computer's config, loopback, DNS-SD and the email domain's well-known document and SRV record, the hostile-answer rules, the confirm with the pinned key, and Vaultwarden recognition (§1.7); the server's `--announce` (an mDNS advertisement of `_lockdown._tcp`, a flag appended to the server's options); the server-word rules and the "master password" copy sweep (§1.8); every screen in §1.9–§1.11, including **Move to your own server**; the density and welcome tokens (§1.12); the seams of §1.13; tests in the core, the app, the CLI, the desktop, and the web vault kept green; the harness re-takes of every first-run, unlock and settings state.

**Gaps (written up in phase 2, not stubbed):**

1. Touch ID unlock on the ad-hoc signed build (R4, Q1), and running both quick-unlock halves on real hardware.
2. Managed policy through an MDM profile on macOS and the registry on Windows; the Windows policy file needs an ACL check before it can be trusted, so Windows has no policy source in this slice.
3. Serving this computer's vault to other devices (decided out of scope).
4. Changing the master password from Settings while you still know it (the kit path exists; a plain change reuses it and is small, but isn't asked for here).
5. A nudge toward backups or a second device for local-only vaults (R8).
6. The identity crate adopting the sealed keystore, so siblings unlock with the same password (R2, R3).
7. The DNS SRV lookup needs a resolver crate (`hickory-resolver`); if it doesn't clear `cargo deny` and `cargo vet` cheaply, SRV becomes a gap and the well-known document alone serves the domain source.
8. Everything in the unicron request (§1.12).
9. Save as PDF, if the mac-fixes slice hasn't merged when phase 2 starts (§1.2).

### 1.15 Open questions for Sean, and his answers

Only the ones that change what a person sees or does (D38). **Sean answered on 2026-10-29: "Go ahead." Every recommendation is taken**, and the answers below are what phase 2 builds.

| # | Answer (Sean, 2026-10-29) |
|---|---|
| Q1 | Skip Touch ID unlock on the ad-hoc build. Settings → Lock says "Touch ID unlock needs the signed Lockdown app." |
| Q2 | The migration is required on next open, with no "Not now". |
| Q3 | Refuse a master password under "fair". |
| Q4 | The kit test moves to the first unlock a day or more later. |
| Q5 | The demo opens from Welcome. In Sean's words: "once we have a stable demo experience, we can look at making my own or importing from a 3rd party. I just want a demo experience to help drive improvements, new features, and QA." Doc 40 builds the demo mode; Welcome reads its `onExploreDemo` / `demoAvailable` provider (§1.13). |
| Q6 | `--announce` is on by default on a network address and off on loopback. |

The questions as asked, with the recommendations that were taken:

| # | Question | Recommendation | Tradeoff |
|---|---|---|---|
| Q1 | **Touch ID unlock on the test build.** The ad-hoc signed app can't hold a fingerprint-bound key (R4). Skip Touch ID unlock there, or offer a weaker form (the key kept in memory between locks, behind the Touch ID prompt)? | **Skip it until the Developer ID build.** Settings → Lock says "Touch ID unlock needs the signed Lockdown app." Touch ID still confirms copies and approvals as today. | You type the master password on each unlock while dogfooding. The weaker form would make a locked app's memory enough to open the vault. |
| Q2 | **The migration for existing installs.** Required on next open, with no "Not now"? | **Required.** Until it runs, anyone who copies the data folder can open the vault. | One interruption, once, for each existing install; you have the kit page or make a new one. |
| Q3 | **A floor for the master password.** Refuse anything under "fair" (the strength meter's middle word), or allow any with a warning? | **Refuse under fair.** It is the one password protecting everything, and on a stolen disk it is the only brake. Four ordinary words clear it. | Some people will need a second try. |
| Q4 | **When the kit test happens.** D251 asks for the kit back straight after first run. | **At the first unlock a day or more later**, with D251's skip words unchanged. The security doc says "put it away and come back", and it keeps the first run to one job per screen. | Someone who never comes back never tests the kit; the yearly drill and Status still remind them. |
| Q5 | **The demo.** "Explore the demo" on Welcome (dev builds), or a demo vault beside your own after setup? | **On Welcome** (§1.13). | Someone already set up gets to the demo through the developer menu, not the vault list. |
| Q6 | **Servers announcing themselves on the local network.** On by default, or opt in? | **On by default when the server listens on a network address; off on loopback**, with `--announce off`. Without it, the "on your network" source finds nothing unless an operator knows to turn it on. | A server tells devices on the network that a Lockdown server is there; nothing else (no names, no accounts). |

### 1.16 Proposed decisions as drafted for Sean's review (the final table is §8)

| D | Decision | Plain version | Status |
|---|---|---|---|
| D460 | The local account's server is `lockdown_server::app` in process: no socket, `Config::local()`, stored under `<data>/local-server/`, reached through `lockdown-sync`'s local hook at the reserved base `lockdown-local://this-device`, installed by the desktop and the CLI; one request at a time across processes by a file lock. | The app runs a private copy of the Lockdown server inside itself, so your account is a real account from day one. | Under D38; audit item |
| D461 | The master password opens the vault through a local password slot (Argon2id at or above the desktop floor) that seals the device's three secrets in `device-key.sealed.cbor`; a Lockdown keystore implements the crate's `DeviceKeystore`. Amends D11 (local open, for Lockdown's own unlock), D83 and D87; the crate is asked to adopt it. | Your master password now locks the key file on disk, so copying the folder isn't enough to open your vault. | Under D38; audit item |
| D462 | Quick unlock: a per-launch secret in memory plus a biometry-bound OS item; password after a restart, after 14 days, after a fingerprint change, after five failures; offered once after setup, only where the OS can bind the key. Makes D252's reminder redundant on the desktop. | Touch ID unlocks most of the time; the password is still asked at start and every two weeks. | Sean (2026-10-29), as recommended |
| D463 | The name: a 1–64 character label, the local login, never an email, never sent anywhere; renaming changes the label only. | Your name is just what the lock screen shows. | **Flagged** |
| D464 | The master password must read at least "fair" (zxcvbn 3), typed twice, with the name as a guessable word. | Lockdown won't take a weak master password. | Sean (2026-10-29), as recommended |
| D465 | The migration of existing installs: required on next open; the kit's Secret Key or a new kit; server accounts confirm their password once online; the plain key file is removed only after the sealed one reopens. | Older installs choose a master password once; nothing in the vault changes. | Sean (2026-10-29), as recommended; audit item |
| D466 | The CLI unlocks with the master password from stdin, the variable or a terminal prompt (amends D47); `init --name`; `password set`; `password reset --kit-stdin`. | The command line asks for the same master password. | **Flagged** (the prompt) |
| D467 | A forgotten password: the kit opens the identity, a device-key session and the authenticated re-registration set the new password, the account slot and the local slot are sealed again; needs the network when the account is on a server. | Your recovery kit lets you choose a new master password. | Under D38; audit item |
| D468 | Discovery: five sources in a fixed order, background only, domain lookups only after an email-shaped login is typed on the join path, deterministic ranking. | Lockdown looks for your server so you don't have to type it, and asks before using it. | **Flagged** (what it looks at) |
| D469 | Discovery treats every answer as hostile: caps, timeouts, no redirects, https except loopback, 16 per source, printable short names; Vaultwarden recognised by `/api/config` and pointed at Import. | A fake answer can't do more than show up in a list. | Under D38; audit item |
| D470 | The confirm shows the address, the source and the server key's six groups of five digits; the confirmed key is pinned before the first request; a changed key is refused in words. | You see which server you're connecting to, and Lockdown holds it to that. | Under D38; audit item; the words **flagged** |
| D471 | `lockdown-server --announce`: a DNS-SD advertisement of `_lockdown._tcp`, on by default on a network address, off on loopback. | Your home server can be found by your devices on the same network. | Sean (2026-10-29), as recommended |
| D472 | A managed policy file (macOS and Linux paths, root-owned, not writable by others) names a server, may pin its key, may lock the choice. | An organization can preset its server. | Under D38 |
| D473 | Move to your own server: confirm, the same login and password registered there with the kept Secret Key, the identity's records and every vault pushed from its first head, heads compared, then the switch; the local server's folder deleted after. | Moving copies everything to your server, and only switches once it's all there. | **Flagged** (the flow); audit item |
| D474 | The first-run flow and its words (§1.9): Welcome, name and password, kit, Touch ID, bring your passwords, the proof moment; join, migration and unlock screens; the kit page for local accounts. | First run is six short screens with plain expectations. | Sean (2026-10-29), as recommended |
| D475 | Density: Comfortable roomier (48 px rows and the knobs of §1.12), Compact about today's Comfortable; the welcome layout fixed at either density; all tokens; the unicron density-mode request. | The app is roomier by default; Compact keeps it tight. | **Flagged** |
| D476 | Server words appear only for a server account, a found server, or the server paths; local-mode lines on Status, Devices and Sharing. | You won't hear about servers until one matters. | **Flagged** |
| D477 | "Account password" becomes "master password" in every user-facing string. | One name for the one password. | **Flagged** |
| D478 | `onboarding.cbor`: the first run's stage, the name, and whether it created the identity root; an interrupted first run resumes at the kit with a new kit; the reset removes the identity root only when first run made it and no other app is enrolled. | Quitting halfway through setup is safe, and a developer reset leaves nothing behind. | Under D38 |
| D479 | The demo opens from Welcome in builds with `demo-vault`, with no account, a persistent bar and Leave the demo; never mixed into a real vault. | You can look around before you set anything up. | Sean (2026-10-29), as recommended |

## 2. What was built

| # | Call (§1.0) | State | Where |
|---|---|---|---|
| 1 | A local account, made in the app, on a server the app runs itself | **Built** | `bins/server/src/local.rs`, `crates/sync/src/transport.rs`, `crates/app/src/first_run.rs` |
| 2 | The master password unlocks the vault; quick unlock; the CLI; existing installs | **Built**, apart from quick unlock (Q1: not on the ad-hoc build; written up as gap 1) | `crates/app/src/local_unlock.rs`, `first_run.rs`, `bins/cli/src/master.rs` |
| 3 | No server in the create flow | **Built**, tested: no "server" or "sync" on any create screen | `apps/desktop/src/features/unlock/first-run.tsx` |
| 4 | Detection, never auto-connect | **Built**: policy (macOS, Linux), this computer's config, this computer's port, DNS-SD, the email domain's well-known document and SRV record, typed addresses; the confirm pins the key; Vaultwarden recognised | `crates/app/src/discovery.rs`, `crates/sync/src/dnssd.rs`, `crates/policy/src/managed.rs`, `bins/server/src/announce.rs` |
| 5 | First run is roomy; Comfortable roomier; tokens only; the unicron request | **Built** (request in §5) | `app-extension.css` tier 3, `features/unlock/welcome.tsx` |
| 6 | The six-screen flow | **Built**; the Touch ID screen exists but no build offers it yet (Q1) | `first-run.tsx` |
| 7 | The unlock screen | **Built**, with "Forgot your password? Use your recovery kit" | `unlock-screen.tsx`, `forgot-password.tsx` |
| 8 | Move to your own server | **Built** | `Session::move_to_server` (`session/local_account.rs`), `features/server/server-settings.tsx` |

### 2.1 The pieces

- **The local server** (`lockdown_server::local`). `lockdown_server::app` over `SqlStorage` and `ServerKeys` under `<data>/local-server/` (0600 files in a 0700 folder), driven with `tower::ServiceExt::oneshot` on a current-thread runtime the module owns. A request holds an exclusive `File::lock` on `<data>/local-server/lock`, so the app and the command line can share it. `local::config()`: signups open, the usual eight-hour session, a year's idle window for renewal by the device key, no brakes, no attachment quota. The store keeps nothing in memory between requests (checked: only the SQLite handle and prepared statements), so a second process never sees stale state.
- **The seam** (`lockdown-sync`): `LOCAL_BASE = "lockdown-local://this-device"`, `install_local_server(hook)`, and the data directory a request is for, named per thread (`set_local_home`; the core's worker thread names its own when it starts, and again on `rehome`) or per process (`set_default_local_home`, the command line). `check_base` accepts the reserved address only when a server is installed; the live channel and the clock check refuse it. No call site in `lockdown-app` changed. The demo's `hold_network` holds this address too, which is right: the demo has no account.
- **The account** (`App::first_run_create(name, password)`): the name and the password are checked (§2.3), fresh device keys are sealed under the password **before** the identity names them, the identity and the first vault are made, the account is registered on the local server with OPAQUE and a new kit (the Secret Key kept sealed, D247), and the keys are sealed under the kit too. `onboarding.cbor` records the stage, the name, when the kit was made, and the identity root this first run created (`onboarding::created_identity()`, for the developer reset, doc 40 §2.4).
- **The sealed keys** (`local_unlock`). `DeviceKeys` holds the three secrets in memory, implements `DeviceKeystore` (and so the vault's `Signer`), and replaces `Rc<FileKeystore>` throughout `lockdown-app` and the command line's copy of the flows. `device-key.sealed.cbor` holds the public halves in the clear and the secrets twice: under Argon2id of the master password (calibrated at or above the build's floor to about a second, parameters only rise, a file below the floor is refused) and under a key derived from the recovery kit. Both are XChaCha20-Poly1305 bound to `{label, v, device_id, slot}`. The plain `device-key.cbor` reads byte for byte as the identity crate's, and goes only after the sealed file reads back the same device.
- **Unlock** (`App::unlock_with_password`): opens the keys with the password, then `Session::open_with(Mode::Open)` as before; never a server. `App::unlock()` on a sealed install says "This vault is locked with a master password." The gates' password check (`Reveal`, D101; the 14-day reminder) runs against this computer's slot for a local account, and D252's reminder stays quiet on a sealed install, which asks for the password at every unlock anyway.
- **The kit test** (Q4): `reminders` holds `kit_test_pending` back until a day after the first run made the kit.
- **The migration** (`App::set_master_password`): an install with no account registers the local account with its kit (the Secret Key **and** the recovery code, §2.2) or a new kit, then seals the keys; an install with an account on a server checks the password there with an OPAQUE sign-in and seals under it. A first run that stopped after the keys were sealed but before the account was made finishes here too.
- **A forgotten password** (`App::password_reset_with_kit`): the kit opens the keys through their kit slot; with a session from the device's key (D400), the account's password file is made again on its server under the new password (the account's own stretching kept) and the account slot written again with the kept Secret Key; then the keys are sealed under the new password. For an account on a real server this needs the network and says so.
- **Moving** (`App::move_to_server`): checks the master password against this computer's slot, retires the local account slot, registers the same identity and kit on the confirmed server under the login given (`account_register_with` with the kept Secret Key), starts that server's record of the identity afresh, and syncs every vault from its first head. Only a finished sync leaves `sync.cbor` naming the new server; then the server inside the app is closed (its router, `AppState`, SQLite store and runtime dropped, so every file is let go) and `<data>/local-server/` removed. Where a file can't be removed yet (Windows refuses to delete an open one, for instance one the command line holds), `local-server.retired` marks it: the server refuses every request while the mark stands, and the next launch finishes the removal before anything could open it. On any failure the local account is put back (its slot written again with the same password) and the words say so.
- **Discovery** (`lockdown_app::discovery`, native builds): `discover(fetch, sources)` over `lockdown_security::http::Fetch` (no redirects, no cookies, no headers, three-second timeout; every answer capped at 16 KiB here). Ranked by best source, then the account's server first, then ready before not, then address; one candidate per address listing every source; at most 16 per source; `https://` required except on this computer. States: `ready`, `unreachable`, `needs_https`, `not_lockdown`, `vaultwarden` (a Bitwarden-shaped `/api/config`), `wrong_key`. The fingerprint is `contact::digits(derive_key("lockdown/server-fingerprint/v1", log_key))`. `App::discovery_confirm` checks the address again and pins its log key in `confirmed-servers.cbor`; `Session::sync` refuses a server whose verified log key isn't the pinned one, before and after the sync.
- **DNS-SD** (`lockdown_sync::dnssd`): a bounded codec (messages up to 9000 bytes, names up to 255, compression pointers only backwards and at most 16 jumps, at most 64 records, hosts letters, digits, hyphens and dots only). The browse asks `_lockdown._tcp.local` from an **ephemeral port**, so responders answer it directly (RFC 6762 §6.7) and nothing needs port 5353; answers are read only from private, link-local or loopback addresses or this computer's own /24. The SRV lookup asks the first nameserver in `/etc/resolv.conf` and checks the answer's id and question. The server's `--announce` (`auto` by default: on for a network address, off on loopback, Q6) binds 5353 with `SO_REUSEADDR` and `SO_REUSEPORT` beside the system's own responder, answers a legacy asker directly and a multicast asker on the group.
- **The managed policy** (`lockdown_policy::managed`): `{"v":1,"server":…,"server_key":…,"lock":…}`, unknown members refused, read only when root owns it and neither group nor others may write it. A policy key that doesn't match marks the candidate `wrong_key`; `lock` hides every other server and the typed field.
- **The command line**: `init --name NAME` (the password from `--password-stdin`, `LOCKDOWN_PASSWORD` or asked twice at a terminal; the kit printed or `--save`d), `password set --name NAME [--kit-stdin | --new-kit]`, `password reset --kit-stdin [--password-stdin]`, and every command on a sealed vault takes the master password from `--master-password-stdin`, `LOCKDOWN_MASTER_PASSWORD` or a prompt with echo off (`master.rs`). It installs the local server and names the data directory at start.
- **The desktop**: the shell installs the local server before the core opens anything and adds fourteen commands at the end of the handler list (`src-tauri/src/first_run.rs`). The views reach them through `FirstRunClient` (`features/unlock/first-run-client.ts`), which the desktop provides (`app/tauri-first-run.ts`) and the web vault does not, so the web vault compiles the views unchanged and keeps its server-backed first run (D176).
- **The words**: every user-facing "account password" in the desktop, the web vault and the core reads "master password", apart from unicron's own gate (§5).

### 2.2 Where this differs from §1, and why

| §1 said | Built | Why |
|---|---|---|
| K2: a forgotten password re-registers with a device-key session (§1.10) | The same, plus a **kit slot**: the device's keys are also sealed under a key from the recovery kit | The keys themselves are sealed under the forgotten password; without a second way to open them, the kit could open the identity but not this computer's keys. The kit already opens everything on a new device, so this adds no new way in. |
| M2 asks for the Secret Key | M2 asks for the **Secret Key and the recovery code** ("Type your recovery kit") | The kit slot needs the whole kit. |
| K1 checks the kit before K2 ("Checking the kit…") | K1 takes the two lines; the core checks them when K2 saves, and a kit that doesn't open the vault returns to K1 with its words | One core call, so the kit is never held open between screens. |
| The CLI reads the master password from `--password-stdin` and `LOCKDOWN_PASSWORD` | `--master-password-stdin` and `LOCKDOWN_MASTER_PASSWORD` (and the prompt) | Those two already mean a login's password for `add` and the account's for `account …`; reusing them would unlock with an item's password by accident. A test checks `LOCKDOWN_PASSWORD` never unlocks. |
| `lockdown init` without `--name` is refused | It still makes a passwordless install, and every later command says once, on stderr, "This vault has no master password yet. Run lockdown password set --name NAME, or open the Lockdown app." | Scripts and the existing end-to-end tests make throwaway vaults this way; the notice and `password set` close the gap without breaking them. |
| The local server in a `local` feature set without Postgres, S3 and SMTP | The whole server crate links into the desktop and the command line | Splitting the crate's features touches every module; the cost is binary size only (gap 6). |
| Blobs in `<data>/local-server/blobs/` | Blobs in the local server's SQLite | The server's default; one file fewer to lock. |
| Bring your passwords lists "Bitwarden or Vaultwarden (.json)", "Your browser (.csv)" | The rows use `lockdown-cxf`'s own names ("Bitwarden (.json)", "Another app's CSV") | One source of truth for the names; the importers are the same. |
| Settings → Server, for a server account, makes a typed address the "server to suggest" | Choosing a found or typed server confirms its key and makes it the suggestion | The same, through the confirm. |
| The kit page's Print prints the page | The kit screen keeps a print-only copy of the page up while it shows | The print rule shows a `[data-print-page]` alone; the copy means the browser's own Print prints the kit and nothing else too. |
| The Welcome line for a server from this computer's config | "Your settings name a Lockdown server: *host*" | It wasn't found on this computer. |

### 2.3 The exact words of the new screens

Every screen of §1.9–§1.11 is built with the words written there, apart from the differences in §2.2. The checks on screen 1, M1 and K2 read: "Type a name. It's what the lock screen shows." · "Keep the name to 64 characters or fewer." · "Type a master password." · "This password protects everything else, so it needs to be at least fair. Add another word or two." · "The two passwords don't match yet." The core refuses the same (Q3: zxcvbn 3, the name counted as guessable). M2 reads "Type your recovery kit" · "Your vault already has a recovery kit; its Secret Key also protects your master password, and the kit will open this vault if you forget it." with "Secret Key" ("On your recovery kit page; it starts with A1-.") and "Recovery code". The move dialog reads "Move your vault to *host*?", then "Your login on *host*" with "Login on this server" and "Master password", "Move my vault", "Copying your vault to *host*…", and "Your vault is on *host* now". Settings → Lock gains "Your name" and, on a Mac or Windows, "Touch ID unlock needs the signed Lockdown app." ("Windows Hello unlock needs the signed Lockdown app."). Status on a local account: "Your master password never leaves this computer: it opens your vault here and is never stored." and, under Sync, "Your vault is kept on this computer only. To use it on another device, set up a server in Settings → Server."

### 2.4 The Mac and the local network

Whether the "on your network" source works on Sean's Mac depends on two things this environment can't show. The browse sends its question from an ephemeral port, so the Mac's mDNSResponder, which holds port 5353, doesn't stand in its way; answers come back to that socket directly. A Lockdown server running **on** a Mac must share 5353 with mDNSResponder; it binds with `SO_REUSEPORT`, which is how a second responder shares the port on BSD systems, but that a Mac delivers the multicast question to both sockets is untested (gap 3). If it doesn't, the Mac-native path is the system's DNS-SD API (`DNSServiceRegister` and `DNSServiceBrowse` in `dns_sd.h`). On Linux a server beside Avahi shares the port the same way. CI tests the responder and the browse against each other over a loopback port (`bins/server/tests/announce.rs`); real multicast is opt-in with `LOCKDOWN_MULTICAST_TEST=1` (it passes in this environment).

## 3. How to run it and test it

- **Desktop**: `pnpm tauri dev` in `apps/desktop`. A data directory with nothing in it lands on Welcome. `LOCKDOWN_ROOT` / `LOCKDOWN_DATA` point it elsewhere.
- **Command line**: `lockdown init --name Sean` (asks twice), then `lockdown list` (asks `Master password for Sean:`), `lockdown password set --name Sean --kit-stdin`, `lockdown password reset --kit-stdin`.
- **A server to find**: `lockdown-server --listen 0.0.0.0:8443 --tls-cert … --tls-key …` announces itself; `--announce off` stops it. A policy file at `/etc/lockdown/policy.json` (root-owned, 0644) presets one.
- **Tests**: `cargo test -p lockdown-app --test local_account` (first run, unlock, restart, migration with items in two vaults, a folder, a tag, an attachment and a crown jewel, the forgotten password, the move and a second computer recovering from the moved server, a failed move), `--test discovery` (a real server found, confirmed and held to its key), `--lib local_unlock onboarding discovery kit_pdf`; `cargo test -p lockdown-sync --lib dnssd local_base`; `cargo test -p lockdown-server --test announce`; `cargo test -p lockdown-policy`; `cargo test -p lockdown-cli --test master_password`; `cargo test -p lockdown-desktop --features shell --lib first_run`. In `apps/desktop`: `pnpm vitest run src/features/unlock src/features/server src/features/depth`.
- **The harness**: `pnpm capture --only desktop-first-run --only desktop-unlock --only desktop-migration --only desktop-settings --only desktop-status` in `design/capture`.

## 4. Gaps

1. **Touch ID and Windows Hello unlock** (Q1, R4). No build offers them: the Mac's needs a keychain item bound to the fingerprint, which only the Developer ID-signed app can make. The screen and the rules (a per-launch secret, 14 days, five failures, a fingerprint change) are designed in §1.4; the platform halves are the next piece once the signed build exists. Touch ID still confirms copies and approvals as before.
2. **Windows policy and the registry, and an MDM profile on the Mac.** Windows has no policy source: a file there needs an ACL check before it can be trusted.
3. **Real multicast on a Mac**, as §2.4 says, and the fallback to `dns_sd.h` if a Mac doesn't share 5353.
4. **The SRV lookup on Windows** (there is no `/etc/resolv.conf`); the domain's well-known document still works there.
5. **Changing the master password from Settings** while you know it. The forgotten-password path does the work; a plain change is small and wasn't asked for.
6. **The server crate's features**: the desktop and the command line link the whole server (Postgres, S3 and SMTP code included). A `local` feature set would shrink them.
7. **A nudge toward backups or a second device** for a vault kept on one computer (R8).
8. **The identity crate adopting the sealed keystore**, so Mailbird and Synapse unlock with the same password and a sibling's plain device key no longer opens the identity without it (R2, R3). The request goes to the crate's owner.
9. **The web vault's own kit PDF** is laid out on A4 unless the worker's environment names a Letter region: the browser's locale isn't passed in.
10. **A half-made account on a server after a failed move** is left for its operator; a retry registers again, which that server refuses as a taken login. Signing in to it instead is the fix.
11. **unicron's gate** still says "Account password" (§5).

## 5. unicron requests (host-contract gaps)

1. **Density as a token mode** (§1.12): `component.json` knobs with `$extensions["unicron.modes"].compact` emitted under `[data-density="compact"]`, read by the core primitives: `--c-row-height`, `--c-row-pad-y`, `--c-control-height` and `--c-control-text` (Button, Input, NativeSelect, Toggle, today fixed at `h-8` and `md:text-sm`), `--c-page-gutter`, `--c-section-gap`, `--c-nav-row-height`. Until then Lockdown keeps these in `app-extension.css` tier 3.
2. A Button size for one-column flows (44 px), a "Step *n* of *m*" pattern, a radio-card list (the server picker and the ways to join), a strength Meter, and a password Input with show/hide and a Caps Lock line. Lockdown draws these from plain elements and tokens today.
3. **The gate's password words**: `CrownJewelGate` labels its field "Account password" and says "account password" in its help. The host needs to name it ("Master password"); a `passwordLabel` prop would do.

## 6. Audit notes

- **What the disk alone gives an attacker**: the sealed keys need the master password (one Argon2id try per guess at about a second on the computer that made them) or the whole kit. The local server's store holds what any Lockdown server holds: ciphertext, the OPAQUE record and signed history.
- **Two slots for one secret**: the kit slot is as strong as the kit, which already opens the identity anywhere.
- **What leaves the core**: the kit page, once, as bytes (`Registered::kit`), as before. The master password crosses the IPC boundary as a string on the calls that take it, like the account password before it; the core wraps it in `Zeroizing` at once.
- **Discovery** sends a GET with no headers to each candidate's `/.well-known/lockdown` (and `/api/config` when that isn't a Lockdown document), and nothing else until the person confirms. A hostile answer can at most appear in the list. The domain lookup runs only after an email-shaped login is typed on the join path.
- **`--announce`** says a Lockdown server is at this host and port, and its scheme; no account, no person, no version.
- **The move** is the only new path that sends the vault anywhere, and only to a server whose key the person confirmed and the sync verifies.

## 7. Test counts

Desktop vitest: 256 before, 261 after (the first-run and unlock suites rewritten for the new screens, Settings → Server added). Web vault vitest: 49 before and after; its Playwright suite: 10 of 10. The Rust tests added are listed in §3; the workspace totals are in the PR.

## 8. Proposed decisions

User-visible ones are flagged for Sean; the ones he answered on 2026-10-29 say so. Audit items are marked.

| D | Decision | Plain version | Status |
|---|---|---|---|
| D460 | The local account's server is `lockdown_server::app` in process: no socket, `local::config()`, stored under `<data>/local-server/`, reached through `lockdown-sync`'s hook at `lockdown-local://this-device`, the data directory named per thread or per process; installed by the desktop and the command line; one request at a time across processes by a file lock. | The app runs a private copy of the Lockdown server inside itself, so your account is a real account from day one. | Under D38; audit item |
| D461 | The master password opens the vault through `device-key.sealed.cbor`: the device's three secrets sealed under Argon2id of the password (at or above the floor, calibrated to about a second) and under a key from the recovery kit; `DeviceKeys` implements `DeviceKeystore`; the plain file goes only after the sealed one reads back the same device. Amends D11 (local open, for Lockdown's own unlock), D83 and D87; the identity crate is asked to adopt it. | Your master password locks the key file on disk, so copying the folder isn't enough to open your vault. | Under D38; audit item |
| D462 | Quick unlock: a per-launch secret plus a biometry-bound OS item; the password after a restart, 14 days, a fingerprint change or five failures; offered only where the OS can bind the key, which no build does yet. Settings → Lock says "Touch ID unlock needs the signed Lockdown app." | Touch ID unlock waits for the signed app; until then the password opens the vault. | Sean (2026-10-29), as recommended |
| D463 | The name: a 1–64 character label, no control characters, the local login, never an email, never sent anywhere; renaming (Settings → Lock) changes the label only. | Your name is just what the lock screen shows. | **Flagged** |
| D464 | The master password must read at least "fair" (zxcvbn 3), typed twice, with the name as a guessable word; the core refuses below it. | Lockdown won't take a weak master password. | Sean (2026-10-29), as recommended |
| D465 | The migration is required on next open: a name and a master password, then the kit's Secret Key and recovery code, or a new kit; an account on a server confirms its password there once; the plain key file goes only after the sealed one reads back. | Older installs choose a master password once; nothing in the vault changes. | Sean (2026-10-29), as recommended; audit item |
| D466 | The command line: `--master-password-stdin`, `LOCKDOWN_MASTER_PASSWORD` or a prompt with echo off (amends D47; `LOCKDOWN_PASSWORD` keeps its meaning); `init --name`; `password set`; `password reset --kit-stdin`; `init` without a name still makes a passwordless install, and later commands say so. | The command line asks for the same master password. | **Flagged** (the variable's name, the prompt) |
| D467 | A forgotten password: the kit opens the device's keys through their kit slot, a device-key session makes the account's password file again under the new password and rewrites the account slot, and the keys are sealed again; needs the network when the account is on a server. | Your recovery kit lets you choose a new master password. | Under D38; audit item |
| D468 | Discovery: policy, this computer's config, this computer's port, the local network, an email's domain (only after an email-shaped login is typed on the join path), then typed addresses; background only; one deterministic ranking. | Lockdown looks for your server so you don't have to type it, and asks before using it. | **Flagged** |
| D469 | Discovery treats every answer as hostile: 16 KiB, three seconds, no redirects, no cookies or headers, https except on this computer, 16 per source, printable short versions, the DNS-SD codec's bounds; Vaultwarden recognised by `/api/config` and pointed at Import. | A fake answer can't do more than show up in a list. | Under D38; audit item |
| D470 | The confirm shows the address, the source and the server key's six groups of five digits; confirming pins the key in `confirmed-servers.cbor`; a sync refuses a server whose verified key differs, in words. | You see which server you're connecting to, and Lockdown holds it to that. | Under D38; audit item; the words **flagged** |
| D471 | `lockdown-server --announce auto|on|off`: a multicast DNS answer for `_lockdown._tcp` with host, port and scheme, `auto` on for a network address and off on loopback. | Your home server can be found by your devices on the same network. | Sean (2026-10-29), as recommended |
| D472 | A managed policy file (macOS and Linux paths, root-owned, not writable by group or others, unknown members refused) names a server, may pin its key, may lock the choice. | An organization can preset its server. | Under D38 |
| D473 | Move to your own server: the confirm, a login and the master password, the same identity and kit registered there with the kept Secret Key, that server's record of the identity started afresh, every vault pushed from its first head, and only then the switch and the local server's folder removed; a failure leaves the local account as it was. | Moving copies everything to your server, and only switches once it's all there. | **Flagged** (the flow); audit item |
| D474 | The first-run flow and its words (§1.9, §2.2, §2.3): Welcome, name and master password, kit, Touch ID where offered, bring your passwords, the proof; join, migration and unlock screens; the local kit page; the kit test a day later (Q4). | First run is a few short screens with plain expectations. | Sean (2026-10-29), as recommended |
| D475 | Density: Comfortable roomier (48 px rows, 32 px gutters, 40 px between sections, a 40 px navigation row), Compact about the old Comfortable; the welcome layout fixed at either density; every value a token on Tailwind's spacing base; the unicron density-mode request (§5). | The app is roomier by default; Compact keeps it tight. | **Flagged** |
| D476 | Server words appear only for an account on a server, a found server, or the server paths; Status, Devices and Sharing say the vault is kept on this computer. | You won't hear about servers until one matters. | **Flagged** |
| D477 | "Account password" becomes "master password" in every user-facing string Lockdown owns; unicron's gate is asked to follow (§5). | One name for the one password. | **Flagged** |
| D478 | `onboarding.cbor`: the stage, the name, when the kit was made and the identity root this first run created; an interrupted first run resumes at the kit with a new kit; the developer reset may use `created_identity()`. | Quitting halfway through setup is safe, and a developer reset leaves nothing behind. | Under D38 |
| D479 | The demo opens from Welcome in builds with `demo-vault` (doc 40's provider), with no account and its bar; never mixed into a real vault. | You can look around before you set anything up. | Sean (2026-10-29), as recommended |
