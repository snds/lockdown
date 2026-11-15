# 82: Status and Server: the proofs as rows, the sign-in form, the key review, moving the vault

_Slice: `screens-7` of the 2026-11 redesign, on `wt/lockdown-screens` (from `claude/keen-ritchie-q2hgb2`, after doc 81), built 2026-11-15. No drawn frame; briefs T4 and T5 of the undrawn-areas brief (workspace `07-projects/29-unicron/impl-2026-11/app-lockdown-undrawn.md`), with 4.0 (layouts by name, emphasis, states), X2 (table 4, the critical Banners), X5 (questions); review findings R14 and R19; cross-app decisions §1, §5, §9, §10, §12, §13; contract 5.1b #2, #3, #23, #26, #37, #51, #52. Decisions D900 to D909. Status: built in the lane; not merged._

## For future agent

- **TL;DR:** Settings → Status is rows now: LOCKED, EVEN FROM US (one Field Row per proof with a VERIFIED or NOT YET Badge, CHECK NOW secondary in the header), ACCOUNT AND SYNC (server, login, last sync, session; SYNC NOW as text), THIS DEVICE (name, persona, devices, items), and DETAILS shut (ids, folders, hashes, log numbers, the records held back). No filled button in the content. One critical Banner at the top: sign in again, revoked, or the split view in D328's whole sentence. A local account shows no server words. Settings → Server is YOUR VAULT, SERVERS FOUND NN (Radio Rows), a key review before anything is sent, the move as a Form (MOVE MY VAULT on the phone's bottom row), then THIS SERVER (restore drill, log mirrors, your own mirrors).
- **Files:** `features/status/status-view.tsx`, `features/account/account-dialog.tsx` (the Sign in and Register forms), `features/server/server-settings.tsx` (discovery, the review, the move), `server-picker.tsx` (SERVERS FOUND NN as a Section Header), `server-checks.tsx` (`SplitWarning` as a Banner, `ServerChecks` as rows). `features/settings/settings-view.tsx` lost the `Embedded` wrapper around Status.
- **Do:**
  - Keep `AccountDialog`'s props (`App.tsx` mounts it for the shell's SIGN IN Banner); it only gains `hasKit`. Keep `StatusView`, `ServerSettings`, `ServerChecks`, `SplitWarning` and `ServerPicker` props (additions only: `headingLevel`).
  - Let the shell's `BannerSlot place="status"` carry the sign-in Banner where a shell exists; Status draws its own only where no board holds one (a test, an embed), and draws the revoked and split Banners itself (§2).
  - Put ids, paths and hashes only under DETAILS (R14).
- **Do not:**
  - Put a solid primary in Status (R19), or show CHECK NOW, SYNC NOW, the public log or the mirrors for a local account.
  - Send anything to a server before its key was reviewed (doc 39 §1.11).
  - Toast a passing CHECK NOW: the rows are the answer.
- **Read with:** doc 79 (the `SettingsFrame`, `setting-rows.tsx`), doc 67 (the five channels, `BannerSlot`), doc 78 (the join's key question, D862), doc 39 (§1.8 the local account, §1.11 the move), docs 33 and 35 (sign in again, revoked), doc 27 §4 (mirrors, the split view), D95, D328, D375, D574.

## 1. Status (brief T4)

| Group | Rows | Action |
|---|---|---|
| `LOCKED, EVEN FROM US` | `MASTER PASSWORD` "Never leaves this computer." · `SECRET KEY` "Made here. The server has never seen it." ("Made on this computer." for a local account; "No recovery kit yet." with NOT YET) · `VAULT HISTORY` "Signed by your devices" · `PUBLIC LOG` "Checked Today 09:40" (or "Not checked yet. The next sync checks it.") · `LOG MIRRORS` "Other servers agree" or "02 of 03 agree" (only when mirrors were compared and none split) | `CHECK NOW` (secondary, header); only with an account on a server |
| `ACCOUNT AND SYNC` | `SERVER` (the host) · `LOGIN` · `LAST SYNC` (a row date) · `SESSION` "Until 2026.11.14 02:00", or "Ended" | `SYNC NOW` (text, header); `SIGN IN` on the session row while it is fine (today's "Log in again", kept reachable) |
| `THIS DEVICE` | `NAME` · `PERSONA` (the read-only persona, a fact) · `DEVICES` "05 // 01 revoked" · `ITEMS` | none |
| `DETAILS` (collapsible, shut) | A `MetaList`: identity folder, data folder, profile and its manager, device id, vault id, vault head (seq // hash), identity records, synced seq // revision, public log (entries // root // verified); the mirrors' words; the last CHECK NOW's lines; `HELD BACK UNTIL THEY VERIFY NN` (D574) | none |

Each proof's state is its Badge word (VERIFIED in the success tone, NOT YET neutral); the value is the fact. CHECK NOW: a pass refreshes the status and puts its lines under DETAILS, with no message; a failure is a compact critical Banner under the header: "The check failed", the core's words, "Nothing was changed. Try again when the server is reachable. If the words say the log was rewritten, stop syncing and keep this device's copy." SYNC NOW: a refusal is one critical line under the rows ("Sync refused: " and the core's words); a sign-in-needed answer opens the Sign in form with the reason; a revoked answer opens nothing.

**No account:** ACCOUNT AND SYNC holds "No account on this device. Register one on a server, yours or ours, to sync, or sign in to one you have." and REGISTER and SIGN IN, both secondary. **A local account:** the group is `SYNC` with one row, YOUR VAULT "Your vault is kept on this computer only." and SET UP SYNC (Settings → Server); no CHECK NOW, no public log, no mirrors, no session; DETAILS' summary drops "the log".

## 2. The Banner on Status

One critical Banner, the first true (cross-app §1, X2 table 4):

1. **Revoked** (`sign_in.reason == "revoked"`): "This device can't sync" and the core's sentence. No action: the password does not bring it back (doc 35).
2. **A split the mirrors caught:** `SplitWarning`, now a Banner: "This server's public log doesn't match a mirror", D328's whole sentence as D375 changed it, then each mirror's words. In place of the shell's short split Banner, which on Status has no action anyway.
3. **The shell's Banner for Status** (`BannerSlot place="status"`): sign in again with SIGN IN (found-5), which opens the shell's `AccountDialog`.
4. **No board** (a test, an embed) and a sign-in is needed: Status's own Banner, "Lockdown isn't syncing", the core's message and SIGN IN, which opens its own form.

## 3. The account forms (brief T4)

`AccountDialog` is a Form (`AdaptiveDialog` form 640; a sheet on a phone), overline ACCOUNT:

- **Sign in** (kind `login`, was "Log in"): the reason in the core's words as the description, or "A fresh session on this device."; Text Field "Server" (the suggested server filled in), "Login name", `SecretField` "Master password", and `SecretField` "Secret Key" only when a kit exists ("From your recovery kit. Finishes protecting an older account."). CANCEL and SIGN IN; SIGN IN waits with "Type your master password first."
- **Register** ("Register an account"): the same fields; the Secret Key "Only when a recovery kit already exists."; a kit made now is shown once (doc 81's kit page).
- A refusal is a compact critical Banner in the form; the form stays. It does not close on a press outside. The typed password and Secret Key clear on close and after each answer.
- `hasKit` defaults to true where the caller does not know (the shell's Banner), so the field stays, optional, as today.

## 4. Server (brief T5)

1. `YOUR VAULT`: "On this computer only" or "Syncs with vault.example", and `LEARN MORE`: a question 480 dialog with today's four paragraphs and DONE.
2. `SERVERS FOUND NN` (a Section Header with the count): one `RadioRow` per server discovery found, its host, where it was found and its state ("Ready", "In use", "Needs https" with the reason); only a ready one can be chosen. `LOOK AGAIN` and `TYPE ADDRESS` (was "Type an address", §12: no articles on buttons): a Text Field "Server address" and CHECK. A policy that locks the choice hides TYPE ADDRESS, as before.
3. Choosing one asks first: `ConfirmDialog` at review size (560; a sheet on a phone), overline SERVER KEY, "Use nas.example:8443?", "Only continue if this is a server you or someone you trust runs. Lockdown checks this key every time it syncs.", then `ServerFacts` (doc 78): ADDRESS, FOUND, SERVER KEY in `DigitGroups size="sm"`, POLICY when an organization set the key. Focus on CANCEL; Escape and CANCEL send nothing. USE THIS SERVER confirms the key; a refusal stays in the question in the core's words. For an account on a server it then becomes the server the join and sign-in forms start from (the toast names it).
4. For a vault on this computer only, the move follows as a Form: overline MOVE VAULT, "Move your vault to nas.example:8443", Text Field "Login on this server", `SecretField` "Master password" ("It stays the same. Lockdown uses it to make your login there."), the caption "Moving copies your vault and its signed history. Nothing on this computer is lost.", CANCEL and MOVE MY VAULT (waiting with its reason until both are typed). While it copies, the fields are read-only under a live Badge COPYING YOUR VAULT. A refusal is a compact critical Banner in the form. Then "Your vault is on nas.example:8443 now", one sentence and DONE.
   - **Desktop and medium:** a 640 dialog.
   - **Phone:** the pushed Server screen holds the form, and `MOVE MY VAULT` is a `FloatButton extended` at the end of the detail bottom row (`useDetailBottomRow`, #2, #3), with its reason as a caption in the form. While the keyboard is up the row is not drawn, and the form shows its own MOVE MY VAULT.
5. `THIS SERVER` (an account on a server; the web vault's only Server part): `RESTORE DRILL` with the server's own words and a Badge (PASSED, FAILED, NOT REPORTED), and "As the server reports it…"; `LOG MIRRORS NN`: one `ListRow` per mirror, its address, who named it, what it said, and a Badge (AGREES, SPLIT, NOT CHECKED); the split Banner over them when one split; the multiline Text Field "Your own mirrors" ("One address per line, up to eight. Mirrors are public: nothing of yours goes to them.") and SAVE MIRRORS (secondary). A save toasts without a full stop; a refusal is one critical line.

## 5. Tests and what is not proven

- `status-view.test.tsx`: no filled button, CHECK NOW secondary, SYNC NOW a text action (R19); the proofs as rows with their words and four VERIFIED; the persona under This device; ids and hashes only once DETAILS opens (R14); a refused sync's words beside the remembered log; a sign-in-needed sync opens the Sign in form with the reason and the Secret Key (a kit exists); the sign-in Banner with SIGN IN and no Secret Key without a kit; revoked: the core's sentence, no SIGN IN, no form on SYNC NOW; the log not checked yet; CHECK NOW: a pass says nothing and its lines are under DETAILS, a failure is a critical Banner with the words; held-back records under DETAILS; **a local account shows no server words** (no server, public log, mirror, sync now, check now, session, login, the local address, or "the log" in the summary) and SET UP SYNC; no account: REGISTER and SIGN IN, both secondary, and the Register form.
- `server-settings.test.tsx`: SERVERS FOUND 02, the one that needs https is off, the review's title, focus on CANCEL and the key in DigitGroups, USE THIS SERVER confirms, the move form waits with its reason, a refusal keeps the form with the core's words, then the move and "Your vault is on … now"; **new: moving the vault asks: Escape and CANCEL confirm nothing, send nothing and open no form.**
- `server-checks.test.tsx`: the drill's words with PASSED and the mirror's words with AGREES and LOG MIRRORS 01; a failed drill's words with FAILED; a server with no report says NOT REPORTED (new); the split Banner's whole sentence; saving and refusing mirrors; nothing without a server client.
- `join-device.test.tsx`: the locked policy hides TYPE ADDRESS (the control's new name).
- Web e2e (`live.spec.ts`, the foundation's): This server's drill and mirror lines, through `steps/settings.ts` (unchanged names).
- Capture states (strict, `--sizes 1440x900,390x844,768x1024`): `desktop-status`, `-checked`, `-check-failed`, `-no-account`, `-register`, `-split`, `-local`, `-revoked`, `-sign-in` (new), `desktop-settings-server`, `-server-local`, `-server-confirm` (new), `-server-move`. Ids kept; `built` on the seven screens Sean sees.
- Not proven: a real server's discovery, key check and move; the bottom row with the on-screen keyboard up on a real phone; a screen reader on the review's first focus.

## 6. Gaps

1. **Registering without a Secret Key** (T4's third Form, with "I understand my password alone protects this account."): `OptOutRegister` exists in `features/depth/join-device.tsx` but nothing mounts it today, and the brief places it in Status. Not built here: it would be a new way in, and `accountRegisterOptedOut` belongs to the depth client. A later screens slice can add a text action under REGISTER.
2. The shell's sign-in Banner (`App.tsx`, found-5) offers SIGN IN to a revoked device too; Status shows the revoked Banner in its place, but Items still offers SIGN IN. For the foundation: skip `signIn` when `sign_in.reason == "revoked"`, or give it its own row.
3. The fixture's candidate `vault.rivera.example` is "In use" even for a local account (`fixture/commands.ts`, foundation). The capture of a local Server shows it checked.
4. The SESSION row's SIGN IN is today's "Log in again", kept so nothing becomes unreachable; T4 does not name it.

## 7. Unicron host-contract gaps

1. `ApprovalSheet` has no kind for trusting a server, requires "This wasn't me" and times out after 60 s (doc 78 gap 2), so the key review is a `ConfirmDialog` at review size with `ServerFacts`. Screens: Settings → Server's review, the join's.
2. `FloatButton` has no `disabledReason`: a disabled MOVE MY VAULT in the bottom row says why only through a caption the screen adds. Screen: the move on a phone.
3. `SecretField mode="edit"` is controlled only (doc 81 gap 6): the master password and the Secret Key sit in React state while the forms are open. Screens: Sign in, Register, the move.
4. `SectionHeader`'s `action` is drawn as a text button slot; T4 asks for a secondary CHECK NOW there, which is a bordered button taller than the rule's line. Screen: Status.
5. `Banner` has no slot for a list (a split's mirrors): they are lines in its body. Screen: Status, Server.

## 8. Proposed decisions

- **D900.** Status is a Settings section of rows: LOCKED, EVEN FROM US with a VERIFIED or NOT YET Badge per proof, ACCOUNT AND SYNC, THIS DEVICE with the persona as a fact, and DETAILS shut. The cards and their sentences go. Seen by people (D38). T4, Q2 (Status's side: it lives in Settings).
- **D901.** Ids, folders, hashes, sequence numbers and the log's numbers move under DETAILS; the rows show the facts a person reads (host, login, row dates, counts). Seen by people (D38). R14.
- **D902.** Status has no filled button: CHECK NOW is secondary in its header, SYNC NOW a text action; before, CHECK NOW was filled and SYNC NOW outlined. Seen by people (D38). R19.
- **D903.** A passing CHECK NOW changes the rows and says nothing; its lines are under DETAILS. A failure is a critical Banner under the header in the core's words with what to do. Seen by people (D38). T4, cross-app §9.
- **D904.** Status's one critical Banner: revoked (no sign-in), the split view in D328's whole sentence, or the shell's sign-in Banner with SIGN IN. "Log in again" moves into the Banner while a sign-in is needed, and to the SESSION row as SIGN IN otherwise. Seen by people (D38). T4, X2 table 4.
- **D905.** "Log in" is "Sign in" everywhere in the account form; the form asks for the Secret Key only when a kit exists. Seen by people (D38). T4, undrawn §5 (§4 Sign-in).
- **D906.** A local account's Status has no server words: SYNC with "Your vault is kept on this computer only." and SET UP SYNC; no CHECK NOW, public log, mirrors or session. Seen by people (D38). T4.
- **D907.** Server opens with YOUR VAULT and LEARN MORE, then SERVERS FOUND NN as Radio Rows with LOOK AGAIN and TYPE ADDRESS. Seen by people (D38). T5.
- **D908.** Choosing a server always asks first, "Use <host>?" with its key in DigitGroups, focus on CANCEL and USE THIS SERVER, for a local vault too (before: "Move your vault to <host>?" with CONTINUE); the move then is its own Form, MOVE MY VAULT at the end of the bottom row on a phone. Seen by people (D38). T5, X5 (a question before a step that cannot be undone).
- **D909.** This server is rows: RESTORE DRILL with a PASSED, FAILED or NOT REPORTED Badge, LOG MIRRORS NN with a Badge per mirror, and SAVE MIRRORS secondary; the split warning is a critical Banner, not a red box (6.2 row D328: a critical Banner that stays until the split clears). The web vault shows only this part. Seen by people (D38). T5, W1.
