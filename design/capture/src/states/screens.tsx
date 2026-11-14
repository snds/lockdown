// The screens lane's states: unlock, the first run, joining, migration,
// forgot, Settings and what it opens (the server, import, export, backups),
// Recovery and Status on the desktop; sign-in, unlock, Settings and the link
// and key pages in the web vault. The developer tools' states are in
// devtools.tsx and the server's in server.tsx.

import { SecondFactorPage, SecurityKeyPage } from "../../../../apps/web/src/features/key-pages"
import { followColorScheme } from "../../../../apps/web/src/theme"

import { MASTER_PASSWORD } from "../fixture/commands"
import { uid } from "../fixture/ids"
import { REVOKED, fresh, signedOut } from "../fixture/lane-screens"
import { PERSON } from "../fixture/seed"
import { backupStatus, status } from "../fixture/story"
import { ago } from "../fixture/time"
import { as, fail, forever } from "../host/core"
import { fakeLink, waitingCredentials } from "../host/web-link"
import type { StateDef } from "../harness/types"
import { d, linkPage, w } from "./defs"
import { create, kitSaved, open, settings, settingsAt, toBackups, webOpen } from "./steps/screens"
import { DIALOG, go } from "./steps/shell"

const PERSON_LOGIN = PERSON.login

export const SCREENS_STATES: StateDef[] = [
  d({ id: "desktop-unlock-locked", screen: "Unlock", state: "Locked", brief_row: "Unlock", notes: "The name, the master password and the way back in with the kit. No server words. The washed shell stands behind.", steps: [{ wait: "Unlock", role: "button" }] }),
  d({
    id: "desktop-unlock-idle",
    screen: "Unlock",
    state: "Locked after idle",
    brief_row: "Unlock",
    notes: "Locked by the idle timer: the screen says why.",
    setup: (w) => {
      w.lock = { ...w.lock, reason: "idle" }
    },
    steps: [{ wait: /Locked after a while without you/ }],
  }),
  d({
    id: "desktop-unlock-error",
    screen: "Unlock",
    state: "Wrong password",
    brief_row: "Unlock",
    notes: "A wrong master password, in the core's words.",
    steps: [{ fill: "Master password", value: "Tidal-Vesper-Orbit-84" }, { click: "Unlock", role: "button" }, { wait: /That isn't the master password/ }],
  }),
  d({
    id: "desktop-unlock-forgot",
    screen: "Unlock",
    state: "Forgot: the kit",
    brief_row: "Unlock",
    notes: "Forgot your password? The kit's two lines (K1).",
    steps: [{ click: "Forgot your password? Use your recovery kit" }, { wait: "Use your recovery kit", role: "heading" }],
  }),
  d({
    id: "desktop-unlock-forgot-new",
    screen: "Unlock",
    state: "Forgot: a new password",
    brief_row: "Unlock",
    notes: "Then a new master password (K2).",
    steps: [
      { click: "Forgot your password? Use your recovery kit" },
      { fill: "Secret Key", value: "A3-K7QF2M-9XWD4R-HT6PLC-8ZB3NV-Q5EY7J" },
      { fill: "Recovery code", value: "RC4F-9K2M-XQ7H-TDPL-8B3Z-NW5V-6YEA-J2KD" },
      { click: "Check the kit" },
      { wait: "Choose a new master password", role: "heading" },
    ],
  }),
  d({ id: "desktop-unlock-starting", screen: "Unlock", state: "Loading", brief_row: "Unlock", notes: "The core has not answered `probe` yet: a spinner on the wash.", answers: () => ({ probe: () => forever() }), steps: [{ pause: 300 }] }),
  d({
    id: "desktop-unlock-cant-start",
    screen: "Unlock",
    state: "Can't start",
    brief_row: "Unlock",
    notes: "The data folder can't be made: the core names it and a fix (docs/plan/41-mac-paths.md §2.5).",
    answers: () => ({
      probe: () =>
        fail(
          "refused",
          "Lockdown couldn't create its folder /Users/sam/Library/Application Support/snds/lockdown: macOS didn't allow it. Check that your user account can write to /Users/sam/Library/Application Support/snds, then try again.",
        ),
    }),
    steps: [{ wait: "Lockdown can't start" }],
  }),
  d({
    id: "desktop-unlock-not-enrolled",
    screen: "Unlock",
    state: "Not enrolled",
    brief_row: "Unlock",
    notes: "An identity exists on this computer but doesn't list this device.",
    setup: (w) => {
      w.device = { identity: true, deviceKey: false }
    },
    steps: [{ wait: "This device isn't enrolled yet" }],
  }),
  d({
    id: "desktop-first-run-welcome",
    screen: "First run",
    state: "Welcome",
    brief_row: "First run: create",
    notes: "Nothing on this computer: the tagline, once, Create a new vault and I already use Lockdown, and the server discovery found (D94; doc 39 screen 0).",
    setup: (w) => {
      w.device = { identity: false, deviceKey: false }
    },
    steps: [{ wait: /Every way you sign in/ }, { wait: /Lockdown server: / }],
  }),
  d({
    id: "desktop-first-run-learn-more",
    screen: "First run",
    state: "Learn more",
    brief_row: "First run: create",
    notes: "The detail behind the first screen, in a dialog (progressive disclosure, D94).",
    setup: (w) => {
      w.device = { identity: false, deviceKey: false }
    },
    steps: [{ click: /^Learn more/ }, DIALOG],
  }),
  d({
    id: "desktop-first-run-password",
    screen: "First run",
    state: "Name and master password",
    brief_row: "First run: create",
    notes: "Screen 1: the one password to remember, said plainly, with the strength meter.",
    setup: (w) => {
      w.device = { identity: false, deviceKey: false }
    },
    steps: [{ click: "Create a new vault" }, { wait: "Choose your master password", role: "heading" }, { fill: "Your name", value: "Sam Rivera" }, { fill: "Master password", value: MASTER_PASSWORD }, { pause: 400 }],
  }),
  d({
    id: "desktop-first-run-join",
    screen: "First run",
    state: "I already use Lockdown",
    brief_row: "First run: join",
    notes: "Approve from another device, sign in, or the kit, with the servers discovery found (doc 39 screen J).",
    setup: (w) => {
      w.device = { identity: false, deviceKey: false }
    },
    steps: [{ click: "I already use Lockdown" }, { wait: "Add this computer to your Lockdown", role: "heading" }, { wait: /nas\.rivera\.example/ }],
  }),
  d({
    id: "desktop-first-run-join-confirm",
    screen: "First run",
    state: "Confirm the server",
    brief_row: "First run: join",
    notes: "Before anything is sent: the address, where it was found and the server key's fingerprint.",
    setup: (w) => {
      w.device = { identity: false, deviceKey: false }
    },
    steps: [
      { click: "I already use Lockdown" },
      { wait: /nas\.rivera\.example/ },
      { fill: "Login", value: PERSON_LOGIN },
      { fill: "Master password", value: MASTER_PASSWORD },
      { click: "Continue", role: "button" },
      DIALOG,
    ],
  }),
  d({
    id: "desktop-migration-name",
    screen: "Migration",
    state: "Lock your vault",
    brief_row: "Unlock",
    notes: "An install from before the master password gets one before the list draws (Q2: no Not now).",
    setup: (w) => {
      w.device = { identity: true, deviceKey: true, masterPassword: false }
    },
    answers: (w) => ({ unlock: () => ({ ...status(w), account: null }) }),
    steps: [{ wait: "Lock your vault with a master password", role: "heading" }],
  }),
  d({
    id: "desktop-migration-server",
    screen: "Migration",
    state: "An account on a server",
    brief_row: "Unlock",
    notes: "An install with an account on a server: that password, checked there once (M3).",
    setup: (w) => {
      w.device = { identity: true, deviceKey: true, masterPassword: false }
    },
    steps: [{ wait: "Unlock with your password from now on", role: "heading" }],
  }),
  d({ id: "desktop-recovery", screen: "Recovery", state: "Kit drilled", brief_row: "Recovery", notes: "Kit status, drill, emergency contact, the emergency sheet.", steps: [...open, ...go("recovery")] }),
  d({ id: "desktop-status", screen: "Status", state: "Proofs", brief_row: "Status", notes: "The proofs, account and sync, this device.", steps: [...open, ...go("status")] }),
  d({ id: "desktop-status-checked", screen: "Status", state: "Check now: passed", brief_row: "Status", notes: "Check now re-ran the audit; each line in words.", steps: [...open, ...go("status"), { click: /^Check now/ }, { wait: /signed by one of your devices/ }] }),
  d({
    id: "desktop-status-check-failed",
    screen: "Status",
    state: "Check now: failed",
    brief_row: "Status",
    notes: "A check failed, in plain words.",
    answers: () => ({ audit: () => fail("refused", "The server's public log doesn't include your account's newest key. Sync again; if it still differs, see Help → Key transparency.") }),
    steps: [...open, ...go("status"), { click: /^Check now/ }, { wait: /public log doesn't include/ }],
  }),
  d({ id: "desktop-settings", screen: "Settings", state: "All sections", brief_row: "Settings", notes: "Appearance, lock, clipboard, approvals, browsers, import and export, backups, security, aliases, quick access.", steps: [...open, ...go("settings")] }),
  d({ id: "desktop-first-run-kit", screen: "Kit page", state: "Shown once after create", brief_row: "Kit page (D69)", notes: "Screen 2: the Secret Key and recovery code in large print, Save as PDF, Print, and where to keep it.", setup: fresh, steps: [...create, { wait: "Save your recovery kit", role: "heading" }, { pause: 400 }] }),
  d({ id: "desktop-first-run-kit-saved", screen: "Kit page", state: "Saved as a PDF", brief_row: "Kit page (D69)", notes: "Saved, with the reminder that a PDF on this computer alone won't help if it's lost (doc 38, doc 39).", setup: fresh, steps: [...create, { wait: "Save your recovery kit", role: "heading" }, { click: "Save as PDF", role: "button" }, { wait: /A PDF on this computer alone/ }] }),
  d({ id: "desktop-first-run-import", screen: "First run", state: "Bring your passwords", brief_row: "First run: create", notes: "Screen 4: the importers Lockdown has, each with how to export, and Skip for now.", setup: fresh, steps: [...create, ...kitSaved, { wait: "Bring your passwords", role: "heading" }] }),
  d({ id: "desktop-first-run-proofs", screen: "First run", state: "Locked, even from us", brief_row: "First run: create", notes: "The proofs, once, as part of account creation (D95).", setup: fresh, steps: [...create, ...kitSaved, { click: "Skip for now" }, { wait: "Locked, even from us", role: "heading" }] }),
  {
    ...d({
      id: "desktop-first-run-weak-password",
      screen: "First run",
      state: "Weak password",
      brief_row: "First run: create",
      notes: "Under fair is refused (Q3), under the field, in the core's words.",
      setup: fresh,
      steps: [{ click: "Create a new vault" }, { fill: "Your name", value: "Sam Rivera" }, { fill: "Master password", value: "sam1988" }, { fill: "Type it again", value: "sam1988" }, { click: "Continue", role: "button" }, { wait: /at least fair/ }],
    }),
  },
  d({ id: "desktop-first-run-join-device", screen: "Join from another device", state: "Code or QR", brief_row: "Join from another device", notes: "Approve from another device with a code or QR, then compare six digits (D246).", setup: fresh, steps: [{ click: "I already use Lockdown" }, { click: /^Approve from another device/ }, { pause: 400 }] }),
  d({
    id: "desktop-first-run-join-denied",
    screen: "First run",
    state: "Join: error",
    brief_row: "First run: join",
    notes: "The server refused the sign-in, after the server's key was confirmed.",
    setup: fresh,
    // The desktop joins through the sealed sign-in since #39 (doc 45); refusing the old `account_login` reached nothing.
    answers: () => ({ account_login_sealed: () => fail("refused", "The other device denied this request, or it timed out. Ask again from the other device.") }),
    steps: [
      { click: "I already use Lockdown" },
      { wait: /nas\.rivera\.example/ },
      { fill: "Login", value: PERSON.login },
      { fill: "Master password", value: "x" },
      { click: "Continue", role: "button" },
      { click: "Connect", role: "button" },
      { wait: /denied this request/ },
    ],
  }),
  d({ id: "desktop-recovery-drill", screen: "Recovery drill", state: "Type the kit back", brief_row: "Recovery drill", notes: "The drill: Secret Key, recovery code, password.", steps: [...open, ...go("recovery"), { click: "Run the drill" }, DIALOG] }),
  d({
    id: "desktop-recovery-drill-failed",
    screen: "Recovery drill",
    state: "Kit does not open",
    brief_row: "Recovery drill",
    notes: "The kit typed back doesn't open the vault.",
    answers: () => ({ kit_verify: () => fail("refused", "That Secret Key and recovery code don't open this vault. Check each character against the printed page; the kit may have been replaced since.") }),
    steps: [...open, ...go("recovery"), { click: "Run the drill" }, DIALOG, { fill: "Secret Key", value: "A3-K7QF2M-9XWD4R" }, { fill: "Recovery code", value: "RC-4F9K" }, { click: "Check the kit" }, { wait: /don't open this vault/ }],
  }),
  d({ id: "desktop-recovery-kit-replace", screen: "Kit page", state: "Replace the kit", brief_row: "Kit page (D69)", notes: "Making a new kit replaces the old one.", steps: [...open, ...go("recovery"), { click: /^Replace/ }, DIALOG] }),
  d({ id: "desktop-recovery-emergency-sheet", screen: "Emergency sheet", state: "Printable", brief_row: "Emergency sheet", notes: "One page for a future you, with no secret.", steps: [...open, ...go("recovery"), { click: "Emergency sheet" }, DIALOG] }),
  d({ id: "desktop-status-no-account", screen: "Status", state: "No account", brief_row: "Status", notes: "This device only: Log in or Register.", answers: (w) => ({ status: () => ({ ...status(w), account: null }), unlock: () => ({ ...status(w), account: null }), unlock_with_password: () => ({ ...status(w), account: null }) }), steps: [...open, ...go("status")] }),
  d({ id: "desktop-status-register", screen: "Status", state: "Register an account", brief_row: "Status", notes: "Adding an account later, from Status.", answers: (w) => ({ status: () => ({ ...status(w), account: null }), unlock: () => ({ ...status(w), account: null }), unlock_with_password: () => ({ ...status(w), account: null }) }), steps: [...open, ...go("status"), { click: /^Register/ }, DIALOG] }),
  d({ id: "desktop-status-split", screen: "Status", state: "Mirror disagrees", brief_row: "Status", notes: "A mirror saw a different log: the split-view warning (D327, D328).", answers: (w) => ({ status: () => {
    const s = status(w)
    s.account!.transparency!.mirrors = [{ url: "https://kt.witness.example", named_by: "settings", at: ago(0.01), state: "split", message: "The mirror has a different log at size 48,209." }]
    return s
  } }), steps: [...open, ...go("status")] }),
  d({ id: "desktop-settings-server", screen: "Settings → Server", state: "Found servers", brief_row: "Settings", notes: "What discovery found (the server in use, one on the network, one that needs https), Look again, and a typed address (doc 39 §1.11).", steps: [...settingsAt("settings-server"), { wait: /nas\.rivera\.example/ }, { run: () => document.getElementById("server-settings-title")?.scrollIntoView({ block: "start" }) }, { pause: 200 }] }),
  d({ id: "desktop-settings-server-local", screen: "Settings → Server", state: "Kept on this computer", brief_row: "Settings", notes: "A local account: the vault is kept on this computer only; no server words above it until one is found.", setup: (w) => { w.device = { ...w.device, local: true } }, steps: [...settingsAt("settings-server"), { wait: /kept on this computer only/ }, { run: () => document.getElementById("server-settings-title")?.scrollIntoView({ block: "start" }) }, { pause: 200 }] }),
  d({
    id: "desktop-settings-server-move",
    screen: "Settings → Server",
    state: "Move to your own server",
    brief_row: "Settings",
    notes: "A local account moving to a found server: after its key is confirmed, the login there and the master password (doc 39 §1.11).",
    setup: (w) => {
      w.device = { ...w.device, local: true }
    },
    steps: [...settingsAt("settings-server"), { click: /^nas\.rivera\.example/ }, DIALOG, { click: "Continue", role: "button" }, { wait: /Your login on/ }],
  }),
  d({ id: "desktop-status-local", screen: "Status", state: "Kept on this computer", brief_row: "Status", notes: "A local account: this computer's proofs, and one line about sync (doc 39 §1.8).", setup: (w) => { w.device = { ...w.device, local: true } }, steps: [...open, ...go("status"), { wait: /kept on this computer only/ }] }),
  d({ id: "desktop-settings-compact", screen: "Settings", state: "Compact density chosen", brief_row: "Settings", notes: "Appearance set to compact: the whole app tightens.", steps: [...settings, { click: "Compact" }, { pause: 300 }] }),
  d({ id: "desktop-settings-import", screen: "Import / export", state: "Import: pick a format", brief_row: "Import / export", notes: "Twenty formats; a preview before anything is kept.", steps: [...settingsAt("settings-import-export"), { click: /^Import…/ }, DIALOG] }),
  d({ id: "desktop-settings-export", screen: "Import / export", state: "Export", brief_row: "Import / export", notes: "Four file kinds plus age; crown jewels left out unless opted in.", steps: [...settingsAt("settings-import-export"), { click: /^Export…/ }, DIALOG, { pause: 400 }] }),
  d({ id: "desktop-settings-backup-restore", screen: "Settings → Backups", state: "Restore from a backup", brief_row: "Settings → Backups", notes: "Restore opens the import preview.", steps: [...settingsAt("settings-backups"), { click: /^Restore…/ }, DIALOG] }),
  d({ id: "desktop-settings-backups-off", screen: "Settings → Backups", state: "Off", brief_row: "Settings → Backups", notes: "Backups are off; the sidebar line says so too.", setup: (w) => {
    w.settings = { ...w.settings, backup: { enabled: false, folder: null, interval_hours: 24, keep_last: 10 } }
  }, steps: settingsAt("settings-backups") }),
  d({ id: "desktop-settings-backups-crown-waiting", screen: "Settings → Backups", state: "Crown jewels: next backup carries them", brief_row: "Settings → Backups", notes: "After the update: the switch is on and the newest backup was made without crown jewels, so one line says the next one carries them (doc 48).", answers: () => ({ backup_status: () => backupStatus(true, true, 2) }), steps: [...settingsAt("settings-backups"), { wait: /first to include your 2 crown jewels/ }, toBackups] }),
  d({ id: "desktop-settings-backups-crown-off", screen: "Settings → Backups", state: "Crown jewels left out", brief_row: "Settings → Backups", notes: "\"Include crown jewels\" off: the per-item opt-in and the count come back (doc 48).", setup: (w) => {
    w.settings = { ...w.settings, backup: { enabled: true, folder: "/Users/sam/Backups/Lockdown", interval_hours: 24, keep_last: 10, include_crown_jewels: false } }
  }, steps: [...settingsAt("settings-backups"), { wait: /crown jewels aren't in your backups/ }, toBackups] }),
  d({ id: "desktop-settings-browsers-refused", screen: "Settings → Browsers", state: "Host refused", brief_row: "Settings → Browsers", notes: "Every browser host refused (error colour).", answers: () => ({ browser_status: () => ({ ...(as("browser_status")({}) as object), listening: false, problem: "The browser host at /Applications/Lockdown.app/Contents/MacOS/lockdown-native-host isn't signed by this app's developer, so Lockdown refuses it.", peer_check: { by: "refused", detail: "The host's signature didn't match." } }) }), steps: settingsAt("settings-browsers") }),
  d({
    id: "desktop-status-revoked",
    screen: "Status",
    state: "This device was revoked",
    brief_row: "Status",
    notes: "Another device revoked this one: the password doesn't bring it back, so Status offers no Log in again; it joins again as a new device (doc 35 §1).",
    answers: (w) => {
      const revoked = () => {
        const s = status(w)
        s.account!.sign_in = { reason: "revoked", message: REVOKED }
        return s
      }
      return { status: revoked, unlock: revoked, unlock_with_password: revoked }
    },
    steps: [...open, ...go("status"), { wait: REVOKED }],
  }),

  // --- the web vault -----------------------------------------------------------------
  w({ id: "web-sign-in", screen: "Sign in", state: "Password and Secret Key", brief_row: "Sign in", notes: "This tab becomes one of your devices until you sign out.", setup: signedOut, steps: [{ wait: "Login name" }] }),
  w({ id: "web-sign-in-device", screen: "Sign in", state: "With a device", brief_row: "Sign in", notes: "Join with a code or QR from a signed-in device.", setup: signedOut, steps: [{ click: "With a device", role: "tab" }, { pause: 300 }] }),
  w({ id: "web-sign-in-new", screen: "Sign in", state: "No account yet?", brief_row: "Sign in", notes: "The web vault never creates accounts (D176).", setup: signedOut, steps: [{ click: "No account yet?", role: "tab" }, { pause: 300 }] }),
  w({
    id: "web-sign-in-error",
    screen: "Sign in",
    state: "Wrong password or Secret Key",
    brief_row: "Sign in",
    notes: "The server refused the sign-in; the words come from the core.",
    setup: signedOut,
    answers: () => ({ web_sign_in: () => fail("refused", "That login name, password and Secret Key don't open an account on this server.") }),
    steps: [{ fill: "Login name", value: PERSON.login }, { fill: "Master password", value: "not it" }, { fill: "Secret Key", value: "A3-K7QF2M" }, { click: "Sign in", role: "button", within: '[role="tabpanel"]' }, { wait: /don't open an account/ }],
  }),
  w({
    id: "web-sign-in-rate-limited",
    screen: "Sign in",
    state: "Rate limited",
    brief_row: "Sign in",
    notes: "Too many tries: the server's wait, in words.",
    setup: signedOut,
    answers: () => ({ web_sign_in: () => fail("refused", "Too many sign-in attempts for this login. Try again in 15 minutes.") }),
    steps: [{ fill: "Login name", value: PERSON.login }, { fill: "Master password", value: "x" }, { fill: "Secret Key", value: "A3" }, { click: "Sign in", role: "button", within: '[role="tabpanel"]' }, { wait: /Too many sign-in attempts/ }],
  }),
  w({
    id: "web-sign-in-security-key",
    screen: "Sign in",
    state: "Waiting for a security key",
    brief_row: "Security-key or email wait",
    notes: "The account asks for a security key on a device it has never seen (D250).",
    setup: signedOut,
    answers: () => ({ web_sign_in: () => fail("approval_needed", "This account asks for a security key on a new device.", "security_key") }),
    steps: [{ fill: "Login name", value: PERSON.login }, { fill: "Master password", value: "x" }, { fill: "Secret Key", value: "A3" }, { click: "Sign in", role: "button", within: '[role="tabpanel"]' }, { pause: 800 }],
  }),
  w({
    id: "web-sign-in-email-wait",
    screen: "Sign in",
    state: "Waiting for an email approval",
    brief_row: "Security-key or email wait",
    notes: "The server emailed a link to approve this tab (D300); the address is masked.",
    setup: signedOut,
    answers: () => ({ web_sign_in: () => fail("approval_needed", "we emailed s••@rivera.example a link to approve this tab", "email"), web_sign_in_continue: () => forever() }),
    steps: [{ fill: "Login name", value: PERSON.login }, { fill: "Master password", value: "x" }, { fill: "Secret Key", value: "A3" }, { click: "Sign in", role: "button", within: '[role="tabpanel"]' }, { pause: 800 }],
  }),
  w({ id: "web-starting", screen: "Sign in", state: "Starting", brief_row: "Sign in", notes: "The worker has not answered yet.", answers: () => ({ web_state: () => forever() }), steps: [{ pause: 300 }] }),
  w({ id: "web-cant-start", screen: "Sign in", state: "Can't start", brief_row: "Sign in", notes: "The core module failed its integrity check.", answers: () => ({ web_state: () => fail("stopped", "The vault's core module didn't match its published hash, so it wasn't run. Reload the page; if this stays, tell the server's operator.") }), steps: [{ wait: "The web vault can't start" }] }),
  w({ id: "web-unlock", screen: "Unlock", state: "Locked", brief_row: "Unlock", notes: "Signed in, locked: the password only (D174).", steps: [{ wait: "Unlock", role: "button" }] }),
  w({ id: "web-unlock-wrong", screen: "Unlock", state: "Wrong password", brief_row: "Unlock", notes: "The password didn't match.", steps: [{ fill: /password/i, value: "wrong" }, { click: "Unlock", role: "button" }, { wait: /didn't match/ }] }),
  w({ id: "web-settings", screen: "Settings", state: "Tab settings", brief_row: "Settings", notes: "Lock timers, clipboard, density, reveal, This server, the trust note; Sign out of this tab.", steps: [...webOpen, ...go("settings")] }),
  w({ id: "web-link-share", screen: "Link page", state: "Share link: before opening", brief_row: "Link page (`/s/…`, `/r/…`)", notes: "Opening counts a view; one view left.", mount: () => linkPage("share"), steps: [{ pause: 500 }] }),
  w({ id: "web-link-share-opened", screen: "Link page", state: "Share link: opened", brief_row: "Link page (`/s/…`, `/r/…`)", notes: "The link's values, one at a time.", mount: () => linkPage("share"), steps: [{ click: /^Open/, role: "button" }, { pause: 500 }] }),
  w({ id: "web-link-request", screen: "Link page", state: "Request link", brief_row: "Link page (`/s/…`, `/r/…`)", notes: "Answer a request for a secret.", mount: () => linkPage("request"), steps: [{ pause: 500 }] }),
  w({ id: "web-key-second-factor", screen: "Key pages", state: "Confirm a sign-in with a key", brief_row: "Key pages", notes: "The page the desktop opens when a new device waits for a security key.", mount: () => {
    followColorScheme()
    return <SecondFactorPage link={fakeLink} id={uid("pending", 1)} credentials={waitingCredentials} />
  }, steps: [{ pause: 500 }] }),
  w({ id: "web-key-add", screen: "Key pages", state: "Add a security key", brief_row: "Key pages", notes: "Adding a security key to the account.", mount: () => {
    followColorScheme()
    return <SecurityKeyPage link={fakeLink} id={uid("pending", 2)} credentials={waitingCredentials} />
  }, steps: [{ pause: 500 }] }),
]
