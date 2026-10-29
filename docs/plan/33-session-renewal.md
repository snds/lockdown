# 33: Sessions that renew themselves

_Slice: `slice/session-renewal`, built 2026-10-29. Implements D52 as changed by D375 (`flag-review.json`, the D52 call): "a signed-in device renews its own session with its device key, so the account password is asked only on a new device, after a remote sign-out, or after 30 days without use." Builds on D38 (security-model calls are the building session's, marked audit items), D220–D222 and D296 (sessions as rows, ending them, the live channel), D101 and D151 (gates), D173 and D174 (the web vault's device and its lock), D246, D247, D250, D252, D300 (how a device joins and how the password stays remembered), D321 (the web vault's Signed in list). Proposed decisions D400–D408 (§9)._

## For future agent

- **TL;DR:** A session token still lasts 8 hours (`--session-ttl`), but a device no longer types the password when it runs out. Five minutes before, or when the server answers `token_expired`, the core asks `POST /v1/account/session/challenge` with the old token, signs the challenge with **its device key** under the label `lockdown/session-renew/v1` together with the hash of that token, and trades it at `POST /v1/account/session/renew` for a new token and a new session id; the old session ends in the same transaction. The server takes a token of any age here, but only one of its own, only for a session row that wasn't ended, used within the **idle limit** (`--session-idle-limit`, 30 days; `0` turns renewal off), from a device that isn't revoked and whose key is the one the server holds and the identity's records list. Only an ended session, a revoked device or the idle limit (and a server that doesn't renew) end at the password prompt, with words that say which.
- **Read with:** `crates/sync/src/renew.rs` (the protocol's client side and the reasons), `bins/server/src/renewal.rs` (the endpoints and every check), `bins/server/src/sessions.rs` (rotation, which sessions stand, pruning), `crates/crypto/src/auth.rs` (`session_renew_message`), `crates/app/src/session/renewal.rs` (the core's `token()`), `bins/cli/src/app/renewal.rs` (the command line's), the tests `bins/server/tests/session_renewal.rs`, `crates/app/tests/session_renewal.rs`, `bins/cli/tests/session_renewal.rs`, `crates/web/tests/session_renewal.rs`, the binary test in `bins/server/tests/binary.rs`.
- **Do not:** renew with anything but the device key (never the password, never the token alone); accept a token of any age outside `renewal.rs`; answer a challenge twice; let a renewed session's stream say `session_ended`; list or end only sessions whose token still runs (a renewable session that isn't listed can't be ended, and would come back by itself).

## 1. What was built

| Piece | What is real | Where |
|---|---|---|
| The signed message | `lockdown/session-renew/v1\0` ‖ challenge id (16) ‖ challenge (32) ‖ BLAKE3(old token) (32), Ed25519 by the device key | `lockdown_crypto::auth::session_renew_message` |
| The server's two endpoints | Challenge (one-time, two minutes, bound to the token's session and the token's hash; a second challenge for the same session replaces the first; at most 10,000 waiting, oldest dropped first); renewal (every check again, signature, rotation in one transaction) | `bins/server/src/renewal.rs`, routes at the end of the router |
| Rotation | New token and session id; the old row ended and pointing at the new one (`renewed_to`, a column added in place); the new row keeps the old "signed in" time and takes this request's address and client; the device's other sessions that already ran out end too | `SessionStore::renew_session` |
| Sessions that stand | Not ended, and either the token runs or the device may still renew (used within the idle limit): listed, ended by **End**, **End all other sessions** and the operator; rows kept until the idle limit and a week have passed since last use | `sessions.rs` (`STANDING`, `prune_sessions`) |
| The client protocol | Challenge, sign, renew; every reply checked (a plain token under 4 KiB, not the old one, no second factor, the account's profile); refusals mapped to five reasons | `lockdown_sync::renew` |
| The core | `Session::token()` renews when due (5 minutes before the end) with the keystore's device key and saves `sync.cbor`; sync, "Check now", the session list and its End buttons retry once after a refusal (`HttpTransport::session_refused`); the live channel renews on a 401 and tells the shell when it can't | `crates/app/src/session/renewal.rs`, `session.rs`, `live.rs` |
| The command line | The same, over its own `sync.cbor`; words end with `` (`lockdown account login`) `` | `bins/cli/src/app/renewal.rs` |
| The web vault | The tab's core renews with the device key the tab holds (D173) while it is open; a locked tab holds no key and no token, so unlocking is still the password (D174); a refused renewal locks the tab with the words | the core; `apps/web/src/app/web-app.tsx` |
| The desktop | A sync that can't renew opens **Log in** with the reason as its description; Status keeps the reason on screen (`AccountStatus.sign_in`) with **Log in again** beside it | `features/status/status-view.tsx`, `features/account/account-dialog.tsx` |
| Operator | `--session-idle-limit` (`LOCKDOWN_SESSION_IDLE_LIMIT`), seconds, default 2,592,000; `0` is off | `bins/server/src/main.rs`, `docs/self-host.md` |

## 2. The protocol

```text
 device                                                   server
   │ POST /v1/account/session/challenge                     │
   │   Authorization: Bearer <old token, any age>  ───────▶ │ token is ours (signature, v1)?
   │                                                        │ standing(): row, idle, account, revoked, key
   │ ◀──── RenewChallenge {v, challenge_id, challenge, exp} │ remember (session id, BLAKE3(token), challenge), 120 s
   │                                                        │
   │ sig = Ed25519_device("lockdown/session-renew/v1\0"     │
   │        ‖ challenge_id ‖ challenge ‖ BLAKE3(token))     │
   │ POST /v1/account/session/renew                         │
   │   Bearer <old token>, RenewFinish {v, id, sig} ──────▶ │ take the challenge (gone even if this fails)
   │                                                        │ same session and token hash as given?
   │                                                        │ standing() again; signature under the held key
   │                                                        │ one transaction: old row ended → new row
   │ ◀──── LoginFinishReply {token, expires_at, profile_id} │ `sessions` to the account's streams
```

**What the server checks, in order** (each refusal names one thing):

| Check | Refusal |
|---|---|
| The bearer is a token this server signed (`TokenSigner::verify_any_age`; its age is not checked) | `unauthenticated` |
| Renewal is on (`--session-idle-limit` not 0) and the store keeps sessions | `renewal_off` |
| The token ran out no longer ago than the idle limit (its last use can only be earlier) | `session_idle` |
| Its row exists and names the token's account and device | `session_ended` / `unauthenticated` |
| The row wasn't ended; one ended by a renewal may be renewed again only within two minutes (a lost answer, §3) | `session_ended` |
| The row was used within the idle limit (D220's "last used") | `session_idle` |
| The account exists | `session_ended` |
| The device isn't revoked: no revocation row (D73), and its identity record isn't marked revoked | `revoked_device` |
| The server holds the device's key (it signed in with it), and once the identity's records list the device, the key there is the same | `unknown_device` |
| (Renew only) the challenge is one this server gave, under two minutes old, for this session and this token's hash | `bad_request` |
| (Renew only) the signature is the device key's over the message above | `bad_signature` |
| (Renew only) the row still qualifies inside the transaction | `session_ended` |

**Why this binding.** A stolen token alone gets a challenge it can't sign. A stolen device key alone can't name a session: the message carries the hash of the exact token, and the challenge was given for that token's session. A signature can't be replayed: the challenge is gone once used, or once two minutes pass, and the old session is ended by the renewal. The label keeps the signature from standing for anything else the device key signs: a login proof is `lockdown/device-pop/v1…`, a vault head, an item and a membership event are canonical CBOR maps. The device key is the identity's, so the server's word "this is the device" is checked against what the identity's signed records say.

**Hostile server.** A server can refuse a renewal (it could always refuse a sync); it can make a device sign a challenge, which says nothing but "this device wants this token's session renewed". It learns nothing new: the token was already its own, and the device key's public half is in its log. A reply is taken only when it is a plain token of sane size, not the old one, with no second factor and the account's own profile.

## 3. Limits and rotation

- **Lifetime.** Tokens keep D52's 8 hours. Renewal is what makes a session feel long: a device in use renews about every 7 hours 55 minutes, and one that slept renews on its first call.
- **Idle limit.** 30 days from the session's last use, as D220 records it (to the minute). Measured from use, not from the sign-in or from the token's end, so a device in daily use never sees the password prompt, and one in a drawer for a month does. The password is then needed exactly once. D252's reminder still asks for the password every 14 days on devices that never ask otherwise, so it isn't forgotten.
- **When the client renews.** Five minutes before the token's end by the device's clock (`RENEW_MARGIN_SECS`), or when the server refuses a token with 401 (a clock off by more, a server clock moved): the call is renewed and made once more. A renewal that fails for a passing reason (no network) leaves a working token in use until it runs out.
- **Rotation.** The new token has a new nonce, so a new session id. The old row is ended with `renewed_to` set: the old token now answers `unauthenticated` with "this session was renewed; the device holds a newer one", and its live stream closes without the `session_ended` event (that word is for endings the person or the operator made). The new row keeps the old row's "signed in" time, so the Signed in list reads "signed in when the password was typed", and takes this request's address and client, as a use does.
- **A lost answer.** If the reply to a renewal never reaches the device, it still holds the old token. Within two minutes it may renew that old token again, with its key and a fresh challenge; the successor it never received is ended. After two minutes the old token is simply ended.
- **Two processes, one device.** The desktop app and the command line share `sync.cbor` (D68). Before renewing, each takes a newer token the other saved, so one session is renewed once.
- **Left-behind sessions.** A renewal ends the same device's other sessions whose tokens already ran out: one abandoned by a later sign-in, a tab that locked and signed in again. They can never be renewed (nobody holds their tokens) and would otherwise stand in the list for the idle limit.
- **Sessions stand while they may renew.** The Signed in list, **End**, **End all other sessions** and the operator's **End sessions** (D296) all count a session whose token ran out but whose device may still renew it. A renewable session that isn't listed couldn't be ended, and would come back on its own. Rows are kept until the idle limit and a week have passed since the last use; an ended one still goes a week after its token ran out (D220).

## 4. What a person sees

| Case | Desktop | Web vault | Command line |
|---|---|---|---|
| In use, token runs out | Nothing. The session renews by itself; "Session until" on Status moves on | Nothing, while the tab is open | Nothing |
| Asleep or offline for days, under 30 | Nothing; the first sync renews | A locked tab still asks for the password to unlock (D174), as before | Nothing |
| Ended from another device or by the operator | Toast on the live channel (D221's words); the next sync opens **Log in** with: "This device's session was ended from one of your devices or by whoever runs the server, so it can't renew itself. Log in again with your account password." | The tab locks with that sentence | `lockdown sync` fails with that sentence and `` (`lockdown account login`) `` |
| Revoked on another device | **Log in** with: "This device was revoked from your account on another device, so its session can't be renewed. Log in again with your account password." | Locks with it | The same words |
| Unused past the limit | **Log in** with: "This device went unused for longer than the server allows (30 days unless whoever runs it changed that), so its session can't be renewed. Log in again with your account password." | Locks with it | The same words |
| Server with renewal off, or older than this slice | Once the token runs out: "This server doesn't let a device renew its own session, and the session has run out. Log in again with your account password." | Locks with it | The same words |
| The server no longer knows the session or the key (its key file was replaced) | "The server no longer recognises this device's session, so it can't renew itself. Log in again with your account password." | Locks with it | The same words |

The words stay in `sync.cbor` (`sign_in`) until the next sign-in, so the desktop's Status says them after a restart too, and a later call doesn't knock on the server again.

## 5. Wire and shape additions (D62)

All additive. Old clients keep the 8-hour behaviour: they never call the new endpoints, and nothing they call answers with a new code.

- **Server routes** (end of the router): `POST /v1/account/session/challenge` → `RenewChallenge`; `POST /v1/account/session/renew` (`RenewFinish`) → `LoginFinishReply`.
- **`lockdown_sync::wire`**: `RENEW_V`, `RenewChallenge {v, challenge_id, challenge, expires_at}`, `RenewFinish {v, challenge_id, signature}`; `ErrorCode::{SessionEnded, SessionIdle, RenewalOff}` at the end of the enum.
- **`lockdown_sync`**: the `renew` module (`renew`, `RenewRounds`, `RenewError`, `SignInReason`, `renewal_due`, `RENEW_MARGIN_SECS`); `HttpTransport::session_refused`.
- **`lockdown_crypto::auth::session_renew_message`.**
- **Server**: `Config.session_idle`, `--session-idle-limit` / `LOCKDOWN_SESSION_IDLE_LIMIT`; `TokenSigner::verify_any_age`; `SessionRow.renewed_to` and the `sessions.renewed_to` column (added in place, schema version unchanged); `SessionStore::renew_session`; `live_sessions`, `end_session`, `end_other_sessions` and `prune_sessions` take the idle limit; `AppState.renewals`.
- **`lockdown-app`**: `ErrorCode::SignInNeeded` (`sign_in_needed`, `detail` the reason); `AccountStatus.sign_in: SignInNeeded {reason, message}` (absent when not needed); `SyncConfig.sign_in` (absent when not needed; the CLI drops it).
- **Desktop types** (`core/types.ts`): `ErrorCode` gains `sign_in_needed`, `AccountStatus.sign_in?`, `SignInNeeded`; `isSignInNeeded` in `core/client.ts`. No Tauri or web command was added or changed. No wasm boundary or extension schema changed: the browser bridge maps `sign_in_needed` to its existing `refused`.

## 6. Running and testing it

```sh
# A server with a shorter idle limit (a week), or with renewal off
lockdown-server --data /data --session-idle-limit 604800
lockdown-server --data /data --session-idle-limit 0

# Tests
cargo test -p lockdown-crypto renewal                    # the message's label and binding
cargo test -p lockdown-sync renew                        # the client protocol, refusal reasons, hostile replies
cargo test -p lockdown-server --lib renew               # challenges, rotation, retry window, pruning, standing sessions
cargo test -p lockdown-server --test session_renewal     # every refusal over HTTP, replay, another key, another context
cargo test -p lockdown-server --test binary a_short      # the binary with --session-ttl 2 and --session-idle-limit 0
cargo test -p lockdown-app --test session_renewal        # two devices through the real server: refused then renewed, ahead of time, ended, revoked, idle
cargo test -p lockdown-cli --test session_renewal        # the CLI end to end: past a 2 s session three times, a jumped clock, an ended session
cargo test -p lockdown-web --test session_renewal        # an open tab renews; a locked one needs the password; ended
cd apps/desktop && pnpm vitest run src/features/status   # the Log in prompt with the reason
```

The end-to-end test is the command line's: the binary syncs three times, each well past a two-second session, with no password and a renewed token each time; the server's clock jumps a day and the next sync renews after the refusal; the operator ends the account's sessions and the next sync fails with "was ended" and `lockdown account login`; signing in with the password brings it back. Nothing reaches beyond `127.0.0.1`.

## 7. Gaps

1. **Revoking a device doesn't end its sessions.** Its token keeps working until it runs out (at most 8 hours), for reads only, since D73 already refuses everything it signs; it can't renew. Ending them at revocation is a small follow-up (the server knows the device's sessions).
2. **The admin page's session counts** (overview and totals) still count sessions whose token runs; the account page lists, and **End sessions** ends, the renewable ones too.
3. **A password check opens a session.** The `Reveal` gate checks the password with an OPAQUE sign-in (D101), which the server records as a session of this device; it stands until the device's next renewal after it runs out, when it is ended as left behind. Before this slice it stood for 8 hours.
4. **No absolute lifetime.** A device in daily use never types the password again; D252's reminder is what keeps the password remembered. An absolute cap would be one more server option.
5. **The device key is Ed25519 only.** Renewal signs with the device key the identity lists; a post-quantum device key (doc 28's slot) would sign beside it when devices get one.
6. **Renewal runs on the core's worker.** Two round trips once in about eight hours, during which other calls wait.
7. **The web vault's Playwright suite** has no renewal scenario; the tab's behaviour is tested natively (`crates/web/tests/session_renewal.rs`), the page's handling of `sign_in_needed` by type and lint.
8. **Mobile** (D122) will use the same core.

**unicron host-contract gaps:** none hit. The prompt is the existing `Dialog` with its description, the reason line is plain text in the design tokens' attention colour, and **Log in again** uses `Button`'s `outline` variant when it is needed (Status keeps one primary button, D386).

## 8. Audit notes (D38)

The renewal message and its label, the binding to the token's hash and the challenge's session, the checks and their order, the two-minute retry of a lost answer, the idle limit measured from last use, sessions that stand while renewable, and the ending of left-behind sessions are security-model choices made under D38. They are marked audit items in §9 so the outside review (D377) takes them as a batch. The formal model of the login (04-security §17 item 1) should gain the renewal step.

## 9. Proposed decisions

For the coordinator to record. **Flagged** rows are user-visible and need Sean.

| ID | Decision | Owner |
|---|---|---|
| D400 | **A device renews its session with its device key.** Two steps with the old token, of any age, as the bearer: a one-time challenge (two minutes, bound to the token's session and its hash), then Ed25519 by the device key over `lockdown/session-renew/v1\0` ‖ challenge id ‖ challenge ‖ BLAKE3(token). The server checks the token is its own, the session row stands, the device isn't revoked, and the key is the one it holds and the identity's records list; every refusal names its reason. Replaces D52's "no refresh yet". Plain version: your device proves it's still your device, with a key only it holds, and gets a fresh pass without your password. | Accepted under D38 (D375 delegated the call); audit item |
| D401 | **Limits:** tokens stay 8 hours (`--session-ttl`); a device renews five minutes before the end or when the server refuses its token; a session renews only if used within the idle limit, 30 days from its last use (`--session-idle-limit`, `0` turns renewal off). No absolute lifetime; D252's reminder keeps the password remembered. Plain version: you type the password on a new device, after a remote sign-out, or after a month without using a device; otherwise never. | Accepted under D38; **flagged for Sean** (the 30 days and no absolute cap are user-visible) |
| D402 | **Renewal rotates the session.** A new token and session id; the old row ended in the same transaction and pointing at the new one, so its token answers "renewed" and its live stream closes quietly; the new row keeps the old "signed in" time and takes this request's address and client. A lost answer may be fetched again within two minutes, by the same key over a fresh challenge, ending the unused successor. Plain version: each renewal gives a new pass and tears up the old one, so a copied pass stops working at the next renewal. | Accepted under D38; audit item |
| D403 | **A session stands while its device may renew it.** The Signed in list, End, End all other sessions and the operator's End sessions include sessions whose token ran out but are still renewable; rows are kept until the idle limit and a week after the last use (amends D220's "a week after the token runs out" for sessions never ended). A renewal ends the same device's other sessions that already ran out. Plain version: a laptop asleep for a week still shows in Signed in, and ending it there stops it for good. | Accepted under D38; **flagged for Sean** (the list now shows sleeping devices for up to 30 days); audit item |
| D404 | **Only the password helps in five cases, and the words say which:** ended (from a device or by the operator), revoked, unused past the limit, a server that doesn't renew, a session the server no longer knows. The core answers `sign_in_needed` with the reason; the desktop opens Log in with the sentence and keeps it on Status, the web vault locks the tab with it, the command line adds `lockdown account login`. A passing failure (no network) never asks for the password. Plain version: when Lockdown does ask for your password, it tells you why. | **Flagged for Sean** (the words) |
| D405 | **A revoked device can't renew,** and revoking still doesn't end its current session: its token reads until it runs out (at most 8 hours), while D73 refuses anything it signs. Ending a revoked device's sessions at once is proposed as the follow-up. Plain version: a removed device loses its access within hours, and can't write at all from the moment it's removed. | Accepted under D38; audit item |
| D406 | **The web vault renews with the tab's device key while it is open** (D173); a locked tab holds neither key nor token, so unlocking is still a fresh sign-in with the password (D174 unchanged); a renewal refused for one of D404's reasons locks the tab with the sentence. Plain version: an open web vault tab stays signed in all day; a locked one still needs your password. | Accepted under D38 |
| D407 | **One device, one renewal.** The desktop app and the command line share `sync.cbor` (D68): before renewing, each takes a newer token the other saved; a call the server refused is renewed and made once more, never more. Plain version: the app and the command line on one computer don't trip over each other's sessions. | Accepted by default under D38 |
| D408 | **Shape additions** (D62): the two routes; `RenewChallenge`, `RenewFinish`, `RENEW_V`; `ErrorCode::{SessionEnded, SessionIdle, RenewalOff}`; `lockdown_sync::renew`; `HttpTransport::session_refused`; `session_renew_message`; `Config.session_idle` and `--session-idle-limit`; `TokenSigner::verify_any_age`; `sessions.renewed_to` (added in place); the idle limit on the session store's calls; `AppError` code `sign_in_needed`; `AccountStatus.sign_in`; `SyncConfig.sign_in`; the desktop types. No command, wasm boundary or extension schema changed. Plain version: everything new was added beside what was there. | Accepted by default under D38 (structure) |
