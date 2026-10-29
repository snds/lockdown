# 30: The post-quantum status line

_Slice: `slice/pq-status-line`, built 2026-10-29 on Sean's go for D346. Closes doc 28 §9's first gap ("no screen shows the rollout"). Builds on D248, D318 and D335–D345. Proposed decisions D365–D374 (§10)._

## For future agent

- **TL;DR:** The Security screen, on the desktop and in the web vault, now has one line that says where the stronger signatures stand for the account: "Quantum-safe signatures: on", "Quantum-safe signatures: waiting for <devices>" (each device a link to it on Devices), or "Quantum-safe signatures: can't check right now". One function in the core decides (`lockdown_vault::pq_status::rollout`). The app wraps it as `App::pq_status`, the desktop and the web vault call it as `pq_status`, and `lockdown security` prints the same words. It says "on" or "waiting" only after the account's server confirmed the identity's records since this device unlocked. Before that, or with no account, or when the server holds no record set, it says it can't check. It never says a stale "on".
- **Read with:** `crates/vault/src/pq_status.rs` (the rule and the words), `crates/app/src/pq_status.rs` and `crates/app/src/session/pq_status.rs` (the view and what "confirmed" means), `crates/app/schema/pq-status.schema.json`, `apps/desktop/src/features/security/pq-status-{client.ts,line.tsx}`, `apps/web/src/features/web-security-view.tsx`, `bins/cli/src/app.rs` (`security_lines`).
- **Do not:** say "on" from records the server hasn't confirmed this session; let anything about a device other than its id and its Devices-screen name leave the core; colour the line without the icon and the words; add the line's words in a shell (they come from the core so every shell says the same).

## 1. What was built

| Piece | What it does | Where |
|---|---|---|
| The rule | `rollout(identity, reported)` answers a `Rollout`: the state (`On`, `Waiting`, `Unknown(why)`), the devices below level 1 and the devices at level 1 but below level 2, kept apart. `headline()` and `explanation()` give the words | `crates/vault/src/pq_status.rs` |
| What the server said | `Session.identity_confirmed`: set when a sync's record pull and push (`sync_identity_records`) succeed, and when an `Init` open does them; cleared at the start of every `sync`, so a failed one leaves it clear | `crates/app/src/session.rs`, `session/pq_status.rs` |
| The view | `PqStatusView {state, reason, headline, explanation, without_level_1, without_level_2}`; a device is `{id, name}` | `crates/app/src/pq_status.rs`, schema `crates/app/schema/pq-status.schema.json` |
| Desktop | Tauri command `pq_status`, last in the handler list; `PqStatusClient` through `PqStatusClientProvider` (`app/tauri-pq-status.ts`) | `apps/desktop/src-tauri/src/pq_status.rs`, `apps/desktop/src/app/` |
| The line | `PqStatusLine`: an outline `Item` with an icon, the headline, the explanation, and the waiting devices as link buttons. It reads again after "Check now" and after every live `synced` notice. A failed call reads as "can't check" | `apps/desktop/src/features/security/pq-status-line.tsx` |
| Devices at one device | `Nav.device` (optional) and `DevicesView.focus`: the row is marked (`aria-current`, the `muted` variant), scrolled to and focused | `apps/desktop/src/app/shell.tsx`, `features/devices/devices-view.tsx` |
| Web vault | `pq_status` at the end of the command table; `webPqStatusClient`; a **Security** screen in the sidebar (before Settings) holding the line | `crates/web/src/commands/server.rs`, `apps/web/src/core/web-pq-status-client.ts`, `apps/web/src/features/web-security-view.tsx`, `apps/web/src/app/web-app.tsx` |
| CLI | `lockdown security`: brings the identity's records in step with the server (as `audit` does), then prints the headline, the explanation, one line per waiting device (`  <id>  <name>  (<what it needs>)`), and why the server couldn't be checked when it couldn't | `bins/cli/src/{main,app}.rs` |
| Test tooling | `test_support::{add_device_at_level, set_device_level}` and the `older-device` example, which stand in for a device running an older build | `crates/vault/src/test_support.rs`, `crates/vault/examples/older-device.rs` |

## 2. The states and their words

The words live in `Rollout::{headline, explanation}`, so the desktop, the web vault and the CLI print the same text.

| State | Headline | Explanation |
|---|---|---|
| on | Quantum-safe signatures: on | Every change your devices make to your vaults is signed in a way that even a future quantum computer couldn't fake. |
| waiting, one device | Quantum-safe signatures: waiting for Lockdown on the old laptop | This device needs the latest Lockdown. Once it's updated and opened, the stronger signatures switch on by themselves. |
| waiting, several | Quantum-safe signatures: waiting for Old phone, Another old one and Kitchen tablet | These devices need the latest Lockdown. Once each is updated and opened, the stronger signatures switch on by themselves. |
| unknown, `no_account` | Quantum-safe signatures: can't check right now | They switch on through your account, and this device isn't signed in to one. |
| unknown, `unreached` | (same) | Lockdown hasn't reached your server since you unlocked. It checks again at the next sync. |
| unknown, `silent` | (same) | Your server didn't report your account's devices. It may be an older version. |
| unknown, `not_published` | (same) | All your devices are ready. Lockdown switches them on at the next sync. |
| unknown, `managed_elsewhere` | (same) | Another app looks after your account's devices, so Lockdown can't switch them on. |

Device names are the `Device` record's `name`, which is what the Devices screen shows. They are joined as a sentence would join them ("A", "A and B", "A, B and C"): the devices below level 1 first, then those at level 1. Within each list they follow the Devices screen's order (oldest enrolment first, then by id). A revoked device is never named.

If the call itself fails, the shell shows the neutral headline with "Lockdown couldn't read where they stand. It tries again when you come back to this screen."

**Not by colour alone.** Each state has its own icon (`verified_user`, `hourglass_top`, `help`) beside its words. The row is `role="status"`, named "Quantum-safe signatures", and carries `data-state`. No colour token marks the state.

## 3. How the core decides

```text
reported: NoAccount | Unreached | Silent  ─────────────────────────▶ Unknown(that)
Confirmed ─▶ key not published and another app manages the identity ─▶ Unknown(ManagedElsewhere)
          ─▶ any device not revoked below level 1 or level 2 ─────────▶ Waiting (two lists)
          ─▶ key not published ─────────────────────────────────────▶ Unknown(NotPublished)
          ─▶ otherwise ────────────────────────────────────────────▶ On
```

`On` is exactly doc 28's rule for signing heads (`pq::heads_ready`): the key is published and every device not revoked marks level 2. Only the source of the records differs. `Reported` comes from the session:

- `NoAccount`: no `sync.cbor`.
- `Unreached`: no successful record sync since unlock. Unlock is offline (`Mode::Open`), and the shells sync right after it. A failed sync (offline, a run-out session, a refused log check before the records) clears it again.
- `Silent`: the sync succeeded but the server holds no signed record set for the account (`SyncedIdentity::verified` is empty).
- `Confirmed`: otherwise.

Why "confirmed" matters: the identity's records on disk are this device's last copy. A device added since, or one revoked, shows only after a pull. Saying "on" from that copy after an offline unlock could be wrong in the direction that matters, so the line waits for the server.

**What leaves the core.** Per device: its id and its name. Not its platform, keys, enrolment date or mark. Names and ids are already on the Devices screen, and the brief allows them. No secret is involved, so no gate applies and nothing counts as a use (D151).

## 4. Wire additions (D62, additive)

- **IPC:** Tauri command `pq_status` (no arguments) → `PqStatusView`, appended at the end of `generate_handler!`.
- **Web vault table:** `pq_status`, the same name and shape, appended in `commands/server.rs` before the table's `unknown` arm.
- **Shape:** `PqStatusView` and `PqDevice` are new. Their schema file is `crates/app/schema/pq-status.schema.json`. `crates/app/tests/pq_status.rs::the_wire_shape_matches_its_schema_file` checks every emitted field is declared, every required field is emitted, and every enum value holds, as `crates/matcher/tests/wire_schema.rs` does for the extension. The desktop's TypeScript type is `PqStatus` in `pq-status-client.ts`.
- **Unchanged:** the extension's schemas (`extension/schema/`), the matcher's wasm boundary (`crates/wasm`), every existing command and field, the sync wire and every file format.
- **UI types:** `Nav.device` and `DevicesView`'s `focus` prop are optional. `SecurityView`'s `onDevices` now takes an optional device id, so callers that pass `() => void` still fit.

## 5. How to run and test

```sh
export CARGO_TARGET_DIR=… CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=line-tables-only
cargo test -p lockdown-vault --lib pq_status        # each state, the lists, the words
cargo test -p lockdown-app --test pq_status         # end to end against the real server; the wire shape against its schema
cargo test -p lockdown-web --test pq_status         # the web table's call, from a tab
cargo test -p lockdown-cli --test security          # `lockdown security` as a process
(cd apps/desktop && pnpm vitest run src/features/security)   # the line, the Security screen, Devices at one device

# The web vault end to end (needs the server, the CLI and the tool; copy them first)
cargo build -p lockdown-server -p lockdown-cli
cargo build -p lockdown-vault --example older-device   # its own build: see D373
cd apps/web && pnpm build:wasm && pnpm build && \
  LOCKDOWN_SERVER_BIN=… LOCKDOWN_CLI_BIN=… LOCKDOWN_OLDER_DEVICE_BIN=… LOCKDOWN_E2E_PORT=… pnpm e2e pq-status
```

To try it by hand: register on the desktop, then from a terminal on the same machine (with the app locked) run `older-device --root <identity root> --keystore <data>/device-key.cbor add "Old laptop" 1`. Unlock: the line says "can't check" until the sync, then "waiting for Old laptop". Run `older-device … set <id> 2` and sync again: "on". `lockdown security` prints the same.

## 6. Tests added

Rust: 11 new tests. Desktop (Vitest): 8 new. Web (Vitest): 1 new. Web end to end (Playwright): 1 new.

| Test | What it covers |
|---|---|
| `pq_status::tests::on_once_the_key_is_published_and_every_device_reads_heads` | All on |
| `pq_status::tests::waiting_on_level_1_names_the_device_that_holds_the_key_back` | Waiting on level 1: name, id, words |
| `pq_status::tests::waiting_on_level_2_names_the_device_that_holds_heads_back` | Waiting on level 2 |
| `pq_status::tests::both_levels_waiting_keep_their_lists_apart_and_skip_revoked_devices` | Both, the order, a revoked device left out |
| `pq_status::tests::unknown_is_never_on_and_names_no_device` | No account, unreached, silent, not published |
| `pq_status::tests::an_identity_another_app_manages_is_unknown_not_waiting` | Managed elsewhere |
| `pq_status::tests::names_join_as_a_sentence_does` | The joined names |
| `crates/app/tests/pq_status.rs::the_line_follows_the_rollout_and_never_says_on_unconfirmed` | Against the real server: no account, registered but not synced, on, an older device joins, offline unlock, waiting by name and id (matching `device_list`), on again, the server gone |
| `crates/app/tests/pq_status.rs::the_wire_shape_matches_its_schema_file` | D62 |
| `crates/web/tests/pq_status.rs::a_tab_shows_the_rollout_under_the_desktops_command_name` | The web table's call: on, waiting after the tab's sync, on, refused while locked |
| `bins/cli/tests/security.rs::security_prints_the_same_line_as_the_screen` | The CLI's line through each state, the server gone |
| `pq-status-line.test.tsx` (8) | On, waiting with links (order, the id passed), waiting without links, unknown and a failed call, no client, a live sync re-reads, the line on the Security screen opens Devices at the device, Devices focuses and marks that row |
| `web-pq-status-client.test.ts` | The worker call's name, nothing else sent |
| `e2e/pq-status.spec.ts` | Chromium against the real server: waiting for the older device, its link opens Devices at it, then "on" by itself once it reports level 2 and the change syncs |

## 7. Gaps

- **The web vault's Security screen holds only this line.** The desktop's password checks (weak, reused, breached, two-step, the playbooks) aren't on the web yet. The screen says so in one sentence.
- **"Confirmed" means a sync in this session.** Registering, joining and recovering also pull the records, but only `sync` and an `Init` open set the flag. Every shell syncs right after those steps, so the line is right a moment later.
- **A device that will never update** (lost, given away) holds the account back for good. The line names it and links to it on Devices, where Revoke is, but the explanation doesn't suggest revoking (D371 proposes adding it).
- **Shared vaults** stay as doc 28 §8 says. "On" speaks for the heads this identity's devices sign. Heads other members sign in a shared vault stay Ed25519, and the explanation's wording ("every change your devices make") is chosen for that.
- **Devices at one device isn't in the address.** The web vault's `#/devices` opens Devices but names no device.
- **Device names print raw in the terminal.** `lockdown security`, like `lockdown device list` before it, prints names as the signed records hold them. Only a holder of the identity's key can set one.
- **The `older-device` tool** writes a record that copies this device's receiving key and uses a fresh signing key nobody holds. It can never sign in, and it is test tooling only. It lives in `test_support`, which is public but documented "not for apps", as before.
- **`cargo deny check` fails on `main` already**, untouched by this slice (`Cargo.lock` unchanged): RUSTSEC-2026-0382, -0383, -0385 and -0387. Three have semver-compatible fixes (`rustls-webpki` 0.103.16, `webpki-root-certs` 1.0.10, `ctutils` 0.4.4). The fourth, `crypto-bigint` 0.5.5, comes through `elliptic-curve` 0.13 under `opaque-ke` 4 and needs 0.7.6. That needs an upgrade or a reasoned ignore, which is a security call for the coordinator.

**unicron host-contract gaps:** none. The line is `Item`, `ItemMedia`, `ItemContent`, `ItemTitle`, `ItemDescription`, `Icon` and link `Button`s. The focused device row uses `Item`'s own `muted` variant. No class overrides were added, and the design-system ratchet stays at its baseline of 4.

## 8. Security notes

- Read-only. `pq_status` reads the session's identity and writes nothing. `lockdown security` syncs the identity's records first, as `audit` does. That runs the rollout (D318, D337), which may mark this device and publish the key, exactly as any sync does.
- Nothing in the view is a secret, so no gate applies and nothing counts as a use.
- The view carries ids and names only. The schema test fails if a field is added without the schema file changing with it.
- A server can't make the line say "on" by itself. "On" needs the identity's signed records to say every device marks level 2 and the profile to publish the key. A server can hold back a record (a device added elsewhere) until the next pull, as it can already. A server that answers with no record set gets "can't check".

## 9. Counts

- Rust: 736 `#[test]` functions in the tree before, 747 after. `cargo test --workspace` passes 736 (2 ignored) at the end, 725 before the 11 added; `cargo test -p lockdown-desktop --features shell` passes 45, unchanged.
- Desktop Vitest: 194 before, 202 after.
- Web Vitest: 44 before (5 needed the built core), 45 after.
- Web end to end: 9 before, 10 after.

## 10. Proposed decisions

For the coordinator to record. **Flagged** rows are the ones Sean should see.

| ID | Decision | Status |
|---|---|---|
| D365 | **The post-quantum status line on Security**, desktop and web: "Quantum-safe signatures: on", "Quantum-safe signatures: waiting for <device names>", or "Quantum-safe signatures: can't check right now", each with one short sentence (§2) and its own icon. Plain version: Security tells you whether the stronger signatures are on, and which device to update if they aren't. | Proposed; **flagged for Sean** (new words on a screen) |
| D366 | **"On" and "waiting" only after the server confirmed the account's records since unlock.** Before the first sync after an unlock, with no account, after a failed sync, or when the server holds no record set, the line says it can't check, with why. Plain version: the line never says "on" from old information; right after you unlock offline it says it can't check yet. | Proposed; **flagged for Sean** (what the line says offline) |
| D367 | **One rule in the core**, `lockdown_vault::pq_status::rollout`. It gives the state and two separate lists: devices below level 1 and devices at level 1 but below level 2. Devices are in the Devices screen's order, and revoked ones are left out. Only each device's id and name leave the core. Plain version: the desktop, the web vault and the command line all ask the same question and get the same answer. | Proposed; audit item |
| D368 | **Wire additions (D62, additive):** the Tauri command `pq_status` and the web table's `pq_status`, each at the end of its list; `PqStatusView` and `PqDevice`, with the schema file `crates/app/schema/pq-status.schema.json` and its test; the optional `Nav.device` and `DevicesView.focus`. Plain version: one new question the screens can ask, added beside the others. | Accepted by default under D38 (structure) |
| D369 | **`lockdown security`** prints the same line, one row per waiting device, and why the server couldn't be checked. It brings the identity's records in step first, as `audit` does. Plain version: the terminal says what the Security screen says. | Accepted by default under D38 |
| D370 | **The web vault gets a Security screen** in the sidebar, before Settings, holding the line for now. Plain version: the web vault shows the quantum-safe line too, under Security. | Proposed; **flagged for Sean** (a new screen on the web) |
| D371 | **A waiting device is a link to it on Devices.** Devices then marks the row, scrolls to it and focuses it, and Revoke is there. Proposed for later: the explanation could add "Not using one any more? Revoke it on Devices." Plain version: tap the device's name to see it, and remove it if it's gone for good. | Proposed; **flagged for Sean** (the revoke hint is a wording choice) |
| D372 | **The line reads again after every live sync and after "Check now"**, so the last device's update shows without leaving the screen. A failed read shows "can't check", never the last answer. Plain version: the line updates by itself. | Accepted by default under D38 |
| D373 | **Test tooling for an older device**: `test_support::{add_device_at_level, set_device_level}` and the `older-device` example. CI builds the example in its own step, because building an example turns on its crate's test-only dev-dependency features, which must not reach the CLI built for the end-to-end run. Plain version: the tests can pretend an old computer is on the account. | Accepted by default under D38 (build, tests) |
| D374 | **An identity another app manages** reads "can't check" ("Another app looks after your account's devices…"), not "waiting". Lockdown never publishes the key for it (doc 26 §10), so naming devices would promise something that won't happen. Plain version: if another app is in charge of your devices, the line says so instead of waiting forever. | Proposed; audit item |
