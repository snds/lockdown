// The screens lane's states: unlock, the first run, joining, migration,
// forgot, Settings and what it opens (the server, import, export, backups),
// Recovery and Status on the desktop; sign-in, unlock, Settings and the link
// and key pages in the web vault. The developer tools' states are in
// devtools.tsx and the server's in server.tsx.

import { SecondFactorPage, SecurityKeyPage } from "../../../../apps/web/src/features/key-pages"
import { followColorScheme } from "../../../../apps/web/src/theme"

import { MASTER_PASSWORD } from "../fixture/commands"
import { uid } from "../fixture/ids"
import { REVOKED, fresh, kitState, onMac, signedOut, updateStatus } from "../fixture/lane-screens"
import { PERSON } from "../fixture/seed"
import { backupStatus, status } from "../fixture/story"
import { ago } from "../fixture/time"
import { as, fail, forever } from "../host/core"
import { fakeLink, waitingCredentials } from "../host/web-link"
import type { World } from "../fixture/world"
import type { StateDef } from "../harness/types"
import { d, linkPage, w } from "./defs"
import { create, inPage, kitSaved, settingsDrawn, open, quickSkipped, settings, settingsAt, toBackups, webOpen } from "./steps/screens"
import { DIALOG, go } from "./steps/shell"

const PERSON_LOGIN = PERSON.login

/** Settings' states: the kit in the core's own word, and the updater's status (the version on the status line). */
function settingsAnswers(w: World, kit: "none" | "made" | "drilled" = "drilled") {
  const opened = () => {
    w.lock = { ...w.lock, locked: false, reason: null, idle_secs: 0 }
    return kitState(status(w), kit)
  }
  return {
    status: () => kitState(status(w), kit),
    unlock: opened,
    unlock_with_password: (a: Record<string, unknown>) => (a.password === MASTER_PASSWORD ? opened() : fail("refused", "That isn't the master password for this vault.")),
    update_status: () => updateStatus(),
  }
}

export const SCREENS_STATES: StateDef[] = [
  d({
    id: "desktop-unlock-locked",
    screen: "Unlock",
    state: "Locked",
    brief_row: "Unlock",
    notes: "Mobile / 13, Desktop / 10 (brief F1): the focus frame alone, LOCKED and the name in the overline, \"Lockdown is locked.\", the master password, Unlock (a float button on a phone) and the way back in with the kit. Touch ID is left out: this build has no quick unlock. No vault names, no server words, no wash.",
    built: { area: "locked", screen: "locked", kind: "drawn" },
    steps: [{ wait: "Lockdown is locked.", role: "heading" }, { wait: "Unlock", role: "button" }],
  }),
  d({
    id: "desktop-unlock-idle",
    screen: "Unlock",
    state: "Locked after idle",
    brief_row: "Unlock",
    notes: "Locked by the idle timer: the overline says so, with the timer that fired as the core reports it (brief F1).",
    built: { area: "locked", screen: "idle", kind: "undrawn" },
    setup: (w) => {
      w.lock = { ...w.lock, reason: "idle" }
    },
    steps: [{ wait: /^Locked \/\/ Idle 10:00/ }],
  }),
  d({
    id: "desktop-unlock-error",
    screen: "Unlock",
    state: "Wrong password",
    brief_row: "Unlock",
    notes: "A wrong master password, in the core's words, under the field (brief F1).",
    built: { area: "locked", screen: "wrong-password", kind: "undrawn" },
    steps: [{ fill: "Master password", value: "Tidal-Vesper-Orbit-84" }, { click: "Unlock", role: "button" }, { wait: /That isn't the master password/ }],
  }),
  d({
    id: "desktop-unlock-forgot",
    screen: "Unlock",
    state: "Forgot: the kit",
    brief_row: "Unlock",
    notes: "Brief F4: Forgot your password? The focus frame with no folio, the kit's two lines as SecretFields (K1), Check kit waiting with its reason until both are typed, Back.",
    built: { area: "forgot-password", screen: "kit", kind: "undrawn" },
    steps: [{ click: "Forgot your password? Use your recovery kit" }, { wait: "Use your recovery kit", role: "heading" }],
  }),
  d({
    id: "desktop-unlock-forgot-new",
    screen: "Unlock",
    state: "Forgot: a new password",
    brief_row: "Unlock",
    notes: "Brief F4: then a new master password with its strength rule, Type it again, the sentence that the old one stops working, Save new password (K2).",
    built: { area: "forgot-password", screen: "new-password", kind: "undrawn" },
    steps: [
      { click: "Forgot your password? Use your recovery kit" },
      { fill: "Secret Key", value: "A3-K7QF2M-9XWD4R-HT6PLC-8ZB3NV-Q5EY7J" },
      { fill: "Recovery code", value: "RC4F-9K2M-XQ7H-TDPL-8B3Z-NW5V-6YEA-J2KD" },
      { click: "Check kit", role: "button" },
      { wait: "Choose a new master password", role: "heading" },
    ],
  }),
  d({
    id: "desktop-unlock-starting",
    screen: "Unlock",
    state: "Loading",
    brief_row: "Unlock",
    notes: "The core has not answered `probe` yet: the lockup and one skeleton bar (brief F1).",
    built: { area: "locked", screen: "starting", kind: "undrawn" },
    answers: () => ({ probe: () => forever() }),
    steps: [{ see: '[data-slot="skeleton"]' }, { pause: 300 }],
  }),
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
    built: { area: "locked", screen: "cant-start", kind: "undrawn" },
    steps: [{ wait: "Lockdown can't start.", role: "heading" }],
  }),
  d({
    id: "desktop-unlock-not-enrolled",
    screen: "Unlock",
    state: "Not enrolled",
    brief_row: "Unlock",
    notes: "An identity exists on this computer but doesn't list this device: the title, today's paragraph, no action (brief F1).",
    built: { area: "locked", screen: "not-enrolled", kind: "undrawn" },
    setup: (w) => {
      w.device = { identity: true, deviceKey: false }
    },
    steps: [{ wait: "This device isn't enrolled yet.", role: "heading" }],
  }),
  d({
    id: "desktop-first-run-welcome",
    screen: "First run",
    state: "Welcome",
    brief_row: "First run: create",
    notes: "Brief F2: nothing on this computer. The focus frame with no folio, the tagline, once, Learn more, the server discovery found as a compact info Banner, New vault, I already use Lockdown (D94; doc 39 screen 0).",
    built: { area: "first-run", screen: "welcome", kind: "undrawn" },
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
    notes: "The detail behind the first screen, in a question dialog with Got it (D94).",
    built: { area: "first-run", screen: "learn-more", kind: "undrawn" },
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
    notes: "Step 01: the one password to remember, said plainly; the name, the master password with the 3-segment strength rule and the core's word, Type it again, Create my vault.",
    built: { area: "first-run", screen: "password", kind: "undrawn" },
    setup: (w) => {
      w.device = { identity: false, deviceKey: false }
    },
    steps: [{ click: "New vault", role: "button" }, { wait: "Choose your master password", role: "heading" }, { fill: "Your name", value: "Sam Rivera" }, { fill: "Master password", value: MASTER_PASSWORD }, { pause: 400 }],
  }),
  d({
    id: "desktop-first-run-join",
    screen: "First run",
    state: "I already use Lockdown",
    brief_row: "First run: join",
    notes: "Brief F3: three Radio Rows (another device, sign in, the kit), the servers discovery found, Login and the master password, Sign in, Back (doc 39 screen J).",
    built: { area: "join", screen: "ways", kind: "undrawn" },
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
    notes: "Before anything is sent: a question with focus on Cancel, the address, where it was found and the server key in DigitGroups, Connect.",
    built: { area: "join", screen: "confirm-server", kind: "undrawn" },
    setup: (w) => {
      w.device = { identity: false, deviceKey: false }
    },
    steps: [
      { click: "I already use Lockdown" },
      { wait: /nas\.rivera\.example/ },
      { fill: "Login", value: PERSON_LOGIN },
      { fill: "Master password", value: MASTER_PASSWORD },
      { click: "Sign in", role: "button" },
      DIALOG,
    ],
  }),
  d({
    id: "desktop-migration-name",
    screen: "Migration",
    state: "Lock your vault",
    brief_row: "Unlock",
    notes: "Brief F4: an install from before the master password gets one before the list draws (Q2: no Not now). The focus frame with no folio, name, master password with its strength rule, again, Lock my vault.",
    built: { area: "existing-install", screen: "master-password", kind: "undrawn" },
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
    notes: "Brief F4: an install with an account on a server: that password, checked there once (M3), Continue.",
    built: { area: "existing-install", screen: "server-password", kind: "undrawn" },
    setup: (w) => {
      w.device = { identity: true, deviceKey: true, masterPassword: false }
    },
    steps: [{ wait: "Unlock with your password from now on", role: "heading" }],
  }),
  d({
    id: "desktop-migration-kit",
    screen: "Migration",
    state: "That install has a kit",
    brief_row: "Unlock",
    notes: "Brief F4: an install that already has a kit types it (M2): Secret Key and recovery code as SecretFields, Continue, Back, and I don't have it, whose question's yes is New kit.",
    built: { area: "existing-install", screen: "kit", kind: "undrawn" },
    setup: (w) => {
      w.device = { identity: true, deviceKey: true, masterPassword: false }
    },
    answers: (w) => ({ unlock: () => ({ ...status(w), account: null }) }),
    steps: [
      { wait: "Lock your vault with a master password", role: "heading" },
      { fill: "Your name", value: "Sam Rivera" },
      { fill: "Master password", value: MASTER_PASSWORD },
      { fill: "Type it again", value: MASTER_PASSWORD },
      { click: "Lock my vault", role: "button" },
      { wait: "Type your recovery kit", role: "heading" },
    ],
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
  d({
    id: "desktop-settings",
    screen: "Settings",
    state: "This Mac",
    brief_row: "Settings",
    notes: "Brief T0 and T1 (Desktop / 12, Mobile / 16): the SettingsFrame, This Mac in one column (Security, Passkeys, Devices 05, Account); on a phone the drawn rows first, then Find a setting and ALL SETTINGS 13.",
    built: { area: "settings", screen: "this-mac", kind: "drawn" },
    setup: onMac,
    answers: settingsAnswers,
    steps: [...open, ...go("settings"), ...settingsDrawn],
  }),
  d({
    id: "desktop-settings-this-mac",
    screen: "Settings",
    state: "This Mac: Devices and Account",
    brief_row: "Settings",
    notes: "Brief T1: the lower half of This Mac, the first three devices with REVOKE and ALL, the recovery kit with OPEN, other sessions with END ALL.",
    setup: onMac,
    answers: settingsAnswers,
    steps: [...open, ...go("settings"), ...settingsDrawn, { run: () => [...document.querySelectorAll("h3")].find((h) => h.textContent?.startsWith("Devices"))?.scrollIntoView({ block: "start" }) }, { pause: 300 }],
  }),
  d({
    id: "desktop-settings-find",
    screen: "Settings",
    state: "Find a setting",
    brief_row: "Settings",
    notes: "Brief T0: Find a setting with \"lock\" typed: what it found, each a way to its section, over the section list.",
    built: { area: "settings", screen: "find", kind: "undrawn" },
    setup: onMac,
    answers: settingsAnswers,
    steps: [...open, ...go("settings"), { fill: "Find a setting", value: "lock" }, { wait: "Settings found", role: "region" }, { run: () => document.querySelector('[aria-label="Settings found"]')?.parentElement?.scrollIntoView({ block: "start" }) }, { pause: 300 }],
  }),
  d({
    id: "desktop-settings-section-list",
    screen: "Settings",
    state: "All settings (phone)",
    brief_row: "Settings",
    notes: "Brief T0, Mobile / 16 below the drawn rows: Find a setting, ALL SETTINGS 13 and the sections under their groups; each pushes. Recovery shows its mark: the kit is made, not tested.",
    built: { area: "settings", screen: "section-list", kind: "undrawn" },
    widths: [390],
    setup: onMac,
    answers: (w) => settingsAnswers(w, "made"),
    steps: [...open, ...go("settings"), inPage(/All settings/), { run: () => document.querySelector('[data-slot="settings-frame-search"]')?.scrollIntoView({ block: "start" }) }, { pause: 300 }],
  }),
  d({
    id: "desktop-settings-about",
    screen: "Settings → About",
    state: "Update ready",
    brief_row: "Settings",
    notes: "Brief T10: THIS APP (version, the ready update's line, RESTART TO UPDATE, the restart note, the automatic check) and SERVER; the update mark on About and on the way into Settings.",
    built: { area: "settings", screen: "about", kind: "undrawn" },
    setup: onMac,
    answers: (w) => ({ ...settingsAnswers(w), update_status: () => updateStatus({ state: "ready", offered: "1.8.1" }) }),
    steps: [...open, ...go("settings-about"), { wait: "Restart to update", role: "button" }, { pause: 300 }],
  }),
  ...(["System", "Stone", "Graphite"] as const).map((choice) =>
    d({
      id: `desktop-settings-theme-${choice.toLowerCase()}`,
      screen: "Settings → Appearance",
      state: `Theme: ${choice}`,
      brief_row: "Settings",
      notes: `Brief T0, T6 (cross-app §5): the Theme row with ${choice} chosen, and Density as two chips.${choice === "System" ? " System follows the window's light or dark." : ""}`,
      setup: onMac,
      answers: settingsAnswers,
      steps: [...settingsAt("settings-appearance"), { wait: "Density" }, { click: choice, role: "radio" }, { pause: 300 }],
    }),
  ),
  d({ id: "desktop-first-run-kit", screen: "Kit page", state: "Shown once after create", brief_row: "Kit page (D69)", notes: "Step 02: the Secret Key and recovery code in large type, Save as PDF and Print, where to keep it; Continue waits for the box, with its reason.", built: { area: "first-run", screen: "kit", kind: "undrawn" }, setup: fresh, steps: [...create, { wait: "Save your recovery kit", role: "heading" }, { pause: 400 }] }),
  d({ id: "desktop-first-run-kit-saved", screen: "Kit page", state: "Saved as a PDF", brief_row: "Kit page (D69)", notes: "Saved, with the reminder that a PDF on this computer alone won't help if it's lost (doc 38, doc 39).", built: { area: "first-run", screen: "kit-saved", kind: "undrawn" }, setup: fresh, steps: [...create, { wait: "Save your recovery kit", role: "heading" }, { click: "Save as PDF", role: "button" }, { wait: /A PDF on this computer alone/ }] }),
  d({ id: "desktop-first-run-import", screen: "First run", state: "Bring your passwords", brief_row: "First run: create", notes: "The last step: one Nav Item per importer Lockdown has, with how to export, then other apps; Continue waits for something brought in, with its reason; Not now. Screens-8 restyles the wizard it opens.", built: { area: "first-run", screen: "import", kind: "undrawn" }, setup: fresh, steps: [...create, ...kitSaved, ...quickSkipped, { wait: "Bring your passwords", role: "heading" }] }),
  d({ id: "desktop-first-run-quick", screen: "First run", state: "Unlock with Touch ID?", brief_row: "First run: create", notes: "Step 03, where the computer has Touch ID or Windows Hello: Use Touch ID disabled with the core's reason (it doesn't open the vault yet, #37), Not now.", built: { area: "first-run", screen: "quick-unlock", kind: "undrawn" }, setup: fresh, steps: [...create, ...kitSaved, { wait: "Unlock with Touch ID?", role: "heading" }, { wait: /needs the signed Lockdown app/ }] }),
  d({ id: "desktop-first-run-proofs", screen: "First run", state: "Locked, even from us", brief_row: "First run: create", notes: "The proofs, once, as part of account creation (D95): no folio, three checkmark lines, Open vault.", built: { area: "first-run", screen: "proof", kind: "undrawn" }, setup: fresh, steps: [...create, ...kitSaved, ...quickSkipped, { click: "Not now", role: "button" }, { wait: "Locked, even from us", role: "heading" }] }),
  {
    ...d({
      id: "desktop-first-run-weak-password",
      screen: "First run",
      state: "Weak password",
      brief_row: "First run: create",
      notes: "Under fair is refused (Q3), under the field, in the core's words; the strength rule shows one segment and Weak.",
      built: { area: "first-run", screen: "weak-password", kind: "undrawn" },
      setup: fresh,
      steps: [{ click: "New vault", role: "button" }, { fill: "Your name", value: "Sam Rivera" }, { fill: "Master password", value: "sam1988" }, { fill: "Type it again", value: "sam1988" }, { click: "Create my vault", role: "button" }, { wait: /at least fair/ }],
    }),
  },
  d({
    id: "desktop-first-run-join-device",
    screen: "Join from another device",
    state: "Code or QR",
    brief_row: "Join from another device",
    notes: "Brief F3: Approve from another device, Step 01 // 03: the server discovery chose, the code from the other device with its hint, Join (D246).",
    built: { area: "join", screen: "device-code", kind: "undrawn" },
    setup: fresh,
    steps: [{ click: "I already use Lockdown" }, { wait: /nas\.rivera\.example/ }, { click: /^Approve from another device/ }, { wait: "Code from the other device" }, { pause: 400 }],
  }),
  d({
    id: "desktop-first-run-join-no-server",
    screen: "Join from another device",
    state: "No server found",
    brief_row: "Join from another device",
    notes: "Brief F3: discovery found no server, so the device way asks for it in its own Server field, with its hint (P17).",
    built: { area: "join", screen: "no-server", kind: "undrawn" },
    setup: fresh,
    answers: () => ({ discovery_candidates: () => [] }),
    steps: [{ click: "I already use Lockdown" }, { wait: /No Lockdown server found/ }, { click: /^Approve from another device/ }, { wait: "Server" }, { pause: 400 }],
  }),
  d({
    id: "desktop-first-run-join-compare",
    screen: "Join from another device",
    state: "Compare six digits",
    brief_row: "Join from another device",
    notes: "Brief F3: Step 02 // 03, the core's words and the six digits to compare with the other device, Cancel.",
    built: { area: "join", screen: "compare", kind: "undrawn" },
    setup: fresh,
    answers: () => ({ join_poll: () => ({ state: "compare", sas: "418 902", server: PERSON.server, secret_key: "sent", words: "Check the other device shows the same six digits." }) }),
    steps: [
      { click: "I already use Lockdown" },
      { wait: /nas\.rivera\.example/ },
      { click: /^Approve from another device/ },
      { fill: "Code from the other device", value: "7KQ2M-9XH4P" },
      { click: "Join", role: "button" },
      { see: '[aria-label="Digits to compare"]' },
    ],
  }),
  d({
    id: "desktop-first-run-join-approved",
    screen: "Join from another device",
    state: "Approved: finish",
    brief_row: "Join from another device",
    notes: "Brief F3: Step 03 // 03, approved on the other device: Login name, the master password, Finish.",
    built: { area: "join", screen: "finish", kind: "undrawn" },
    setup: fresh,
    steps: [
      { click: "I already use Lockdown" },
      { wait: /nas\.rivera\.example/ },
      { click: /^Approve from another device/ },
      { fill: "Code from the other device", value: "7KQ2M-9XH4P" },
      { click: "Join", role: "button" },
      { wait: "Login name" },
    ],
  }),
  d({
    id: "desktop-first-run-join-device-denied",
    screen: "Join from another device",
    state: "The other device said no",
    brief_row: "Join from another device",
    notes: "Brief F3: the other device said no, or the code ran out: a compact critical Banner in the core's words, with Try again.",
    built: { area: "join", screen: "device-denied", kind: "undrawn" },
    setup: fresh,
    answers: () => ({ join_poll: () => fail("refused", "The other device denied this request, or it timed out. Ask again from the other device.") }),
    steps: [
      { click: "I already use Lockdown" },
      { wait: /nas\.rivera\.example/ },
      { click: /^Approve from another device/ },
      { fill: "Code from the other device", value: "7KQ2M-9XH4P" },
      { click: "Join", role: "button" },
      { wait: /denied this request/ },
      { run: () => document.querySelector('[role="alert"]')?.scrollIntoView({ block: "center" }) },
      { pause: 200 },
    ],
  }),
  d({
    id: "desktop-first-run-join-security-key",
    screen: "First run",
    state: "Waiting for a security key",
    brief_row: "First run: join",
    notes: "Brief F3: the account asks this new device for a security key: \"Use your security key.\", the sentence, a live line, Open page again and Cancel.",
    built: { area: "join", screen: "security-key", kind: "undrawn" },
    setup: fresh,
    answers: () => ({ account_login_sealed: () => fail("approval_needed", "use your security key", "security_key") }),
    steps: [
      { click: "I already use Lockdown" },
      { wait: /nas\.rivera\.example/ },
      { fill: "Login", value: PERSON.login },
      { fill: "Master password", value: MASTER_PASSWORD },
      { click: "Sign in", role: "button" },
      { click: "Connect", role: "button" },
      { wait: "Use your security key.", role: "heading" },
    ],
  }),
  d({
    id: "desktop-first-run-join-email",
    screen: "First run",
    state: "Waiting for an emailed approval",
    brief_row: "First run: join",
    notes: "Brief F3: the server mailed a link instead: \"Check your email.\", the core's words with the masked address, a live line, Cancel. No page opens.",
    built: { area: "join", screen: "email", kind: "undrawn" },
    setup: fresh,
    answers: () => ({
      account_login_sealed: () => fail("approval_needed", "the server emailed a link to s•••@rivera.example: open it and choose Approve, and this device carries on by itself", "email"),
    }),
    steps: [
      { click: "I already use Lockdown" },
      { wait: /nas\.rivera\.example/ },
      { fill: "Login", value: PERSON.login },
      { fill: "Master password", value: MASTER_PASSWORD },
      { click: "Sign in", role: "button" },
      { click: "Connect", role: "button" },
      { wait: "Check your email.", role: "heading" },
    ],
  }),
  d({
    id: "desktop-first-run-join-denied",
    screen: "First run",
    state: "Join: error",
    brief_row: "First run: join",
    notes: "Brief F3: the server refused the sign-in, after the server's key was confirmed: a compact critical Banner over the buttons, in the core's words.",
    built: { area: "join", screen: "sign-in-refused", kind: "undrawn" },
    setup: fresh,
    // The desktop joins through the sealed sign-in since #39 (doc 45); refusing the old `account_login` reached nothing.
    answers: () => ({ account_login_sealed: () => fail("refused", "The other device denied this request, or it timed out. Ask again from the other device.") }),
    steps: [
      { click: "I already use Lockdown" },
      { wait: /nas\.rivera\.example/ },
      { fill: "Login", value: PERSON.login },
      { fill: "Master password", value: "x" },
      { click: "Sign in", role: "button" },
      { click: "Connect", role: "button" },
      { wait: /denied this request/ },
      { run: () => document.querySelector('[role="alert"]')?.scrollIntoView({ block: "center" }) },
      { pause: 200 },
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
  // Brief T6 and T9 (screens-5, doc 80): one state per section, on a Mac so the device group says This Mac and the shortcut says ⌘K.
  d({
    id: "desktop-settings-browsers",
    screen: "Settings → Browsers",
    state: "Connected and paired",
    brief_row: "Settings → Browsers",
    notes: "Brief T6: EXTENSION (can reach this app, how the host is checked), BROWSERS ON THIS COMPUTER 03 (two CONNECTED, one CONNECT), PAIRED 02 with REMOVE.",
    built: { area: "settings", screen: "browsers", kind: "undrawn" },
    setup: onMac,
    answers: settingsAnswers,
    steps: [...settingsAt("settings-browsers"), { wait: "Firefox on Sam's MacBook Pro" }, { pause: 300 }],
  }),
  d({
    id: "desktop-settings-browsers-refused",
    screen: "Settings → Browsers",
    state: "Host refused",
    brief_row: "Settings → Browsers",
    notes: "Brief T6: every browser host refused, so a critical Banner at the top in the core's words; EXTENSION says it can't reach this app.",
    built: { area: "settings", screen: "browsers-refused", kind: "undrawn" },
    setup: onMac,
    answers: (w) => ({ ...settingsAnswers(w), browser_status: () => ({ ...(as("browser_status")({}) as object), listening: false, problem: "The browser host at /Applications/Lockdown.app/Contents/MacOS/lockdown-native-host isn't signed by this app's developer, so Lockdown refuses it.", peer_check: { by: "refused", detail: "The host's signature didn't match." } }) }),
    steps: [...settingsAt("settings-browsers"), { wait: "Lockdown refused the browser host" }, { pause: 300 }],
  }),
  d({
    id: "desktop-settings-browsers-remove",
    screen: "Settings → Browsers",
    state: "Remove a paired browser?",
    brief_row: "Settings → Browsers",
    notes: "Brief T6: REMOVE asks first; the browser pairs again with a new code.",
    built: { area: "settings", screen: "browsers-remove", kind: "undrawn" },
    setup: onMac,
    answers: settingsAnswers,
    steps: [...settingsAt("settings-browsers"), { wait: "Firefox on Sam's MacBook Pro" }, { click: "Remove", role: "button", nth: 1 }, DIALOG],
  }),
  d({
    id: "desktop-settings-quick-access",
    screen: "Settings → Quick access",
    state: "Shortcut on, Details open",
    brief_row: "Settings",
    notes: "Brief T6: the switch with the shortcut as a key, the in-app ⌘K caption, and Details opened on what the core keeps out of swap.",
    built: { area: "settings", screen: "quick-access", kind: "undrawn" },
    setup: onMac,
    answers: settingsAnswers,
    steps: [...settingsAt("settings-quick-access"), { wait: /Open quick access from anywhere/ }, { click: /^Details/, role: "button" }, { wait: "Out of swap" }, { pause: 300 }],
  }),
  d({
    id: "desktop-settings-quick-access-taken",
    screen: "Settings → Quick access",
    state: "This computer won't give the shortcut",
    brief_row: "Settings",
    notes: "Brief T6: the core's problem as the switch's description, in critical text.",
    built: { area: "settings", screen: "quick-access-refused", kind: "undrawn" },
    setup: onMac,
    answers: (w) => ({ ...settingsAnswers(w), quick_hotkey_status: () => ({ on: true, registered: false, label: "⌥ Space", problem: "Another app already uses ⌥ Space. Quit it, or turn this off and use ⌘K inside Lockdown." }) }),
    steps: [...settingsAt("settings-quick-access"), { wait: /Another app already uses/ }, { pause: 300 }],
  }),
  d({
    id: "desktop-settings-appearance",
    screen: "Settings → Appearance",
    state: "System, comfortable",
    brief_row: "Settings",
    notes: "Brief T6: THEME with SYSTEM · STONE · GRAPHITE (System on), DENSITY with COMFORTABLE · COMPACT.",
    built: { area: "settings", screen: "appearance", kind: "undrawn" },
    setup: onMac,
    answers: settingsAnswers,
    steps: [...settingsAt("settings-appearance"), { wait: "Density" }, { pause: 300 }],
  }),
  d({
    id: "desktop-settings-codes",
    screen: "Settings → Codes",
    state: "Correct for a wrong clock",
    brief_row: "Settings",
    notes: "Brief T6: edges-5's ClockSetting as it is: one switch with today's sentence.",
    built: { area: "settings", screen: "codes", kind: "undrawn" },
    setup: onMac,
    answers: settingsAnswers,
    steps: [...settingsAt("settings-codes"), { wait: "Correct for a wrong clock" }, { pause: 300 }],
  }),
  d({
    id: "desktop-settings-aliases",
    screen: "Settings → Email aliases",
    state: "One service, watching",
    brief_row: "Settings",
    notes: "Brief T9: the caption, SERVICES 01 (Relaymail, 14 aliases made, its domain, REMOVE), ADD SERVICE, and the watch switch on.",
    built: { area: "settings", screen: "email-aliases", kind: "undrawn" },
    setup: onMac,
    answers: settingsAnswers,
    steps: [...settingsAt("settings-aliases"), { wait: "Watch my aliases for data breaches" }, { pause: 300 }],
  }),
  d({
    id: "desktop-settings-aliases-add",
    screen: "Settings → Email aliases",
    state: "Add a service",
    brief_row: "Settings",
    notes: "Brief T9: the form: Service, the API key in a SecretField with the service's hint, Your domain and Your own server where the service takes them, SET IT UP.",
    built: { area: "settings", screen: "email-aliases-add", kind: "undrawn" },
    setup: onMac,
    answers: settingsAnswers,
    steps: [...settingsAt("settings-aliases"), { click: "Add service", role: "button" }, DIALOG, { pause: 300 }],
  }),
  d({
    id: "desktop-settings-aliases-no-account",
    screen: "Settings → Email aliases",
    state: "No account: watching disabled",
    brief_row: "Settings",
    notes: "Brief T9, #37: the watch switch disabled, with \"This needs an account.\" under it.",
    built: { area: "settings", screen: "email-aliases-no-account", kind: "undrawn" },
    setup: onMac,
    answers: (w) => ({ ...settingsAnswers(w), alias_monitoring: () => ({ enabled: false, addresses: [], has_account: false }) }),
    steps: [...settingsAt("settings-aliases"), { wait: "This needs an account." }, { pause: 300 }],
  }),
  d({
    id: "desktop-settings-checks",
    screen: "Settings → Security checks",
    state: "All three on",
    brief_row: "Settings",
    notes: "Brief T9 (R13): the caption, the breach check, BREACH LIST (the public list) with CHANGE, and the change-password check.",
    built: { area: "settings", screen: "security-checks", kind: "undrawn" },
    setup: onMac,
    answers: settingsAnswers,
    steps: [...settingsAt("settings-security-checks"), { wait: "Breach list" }, { pause: 300 }],
  }),
  d({
    id: "desktop-settings-checks-list",
    screen: "Settings → Security checks",
    state: "Change the breach list",
    brief_row: "Settings",
    notes: "Brief T9: CHANGE on BREACH LIST: one field with today's hint and its own Save.",
    built: { area: "settings", screen: "security-checks-list", kind: "undrawn" },
    setup: onMac,
    answers: settingsAnswers,
    steps: [...settingsAt("settings-security-checks"), { wait: "Breach list" }, { click: /^Change/, role: "button" }, DIALOG, { pause: 300 }],
  }),
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
  w({ id: "web-sign-in", screen: "Sign in", state: "Password and Secret Key", brief_row: "Sign in", notes: "Brief W1: the focus frame, WEB VAULT // the host, \"Sign in.\", the three tabs; login name, master password, Secret Key, Sign in.", built: { area: "sign-in", screen: "password", kind: "undrawn" }, setup: signedOut, steps: [{ wait: "Login name" }] }),
  w({ id: "web-sign-in-device", screen: "Sign in", state: "With a device", brief_row: "Sign in", notes: "Join with a code or QR from a signed-in device (today's JoinWithDevice; screens-3 restyles it).", setup: signedOut, steps: [{ click: "With a device", role: "tab" }, { pause: 300 }] }),
  w({ id: "web-sign-in-new", screen: "Sign in", state: "No account yet?", brief_row: "Sign in", notes: "The web vault never creates accounts (D176): two paragraphs, no button.", built: { area: "sign-in", screen: "no-account", kind: "undrawn" }, setup: signedOut, steps: [{ click: "No account yet?", role: "tab" }, { pause: 300 }] }),
  w({
    id: "web-sign-in-error",
    screen: "Sign in",
    state: "Wrong password or Secret Key",
    brief_row: "Sign in",
    notes: "The server refused the sign-in: its words in one compact critical Banner over Sign in (brief W1).",
    built: { area: "sign-in", screen: "refused", kind: "undrawn" },
    setup: signedOut,
    answers: () => ({ web_sign_in: () => fail("refused", "That login name, password and Secret Key don't open an account on this server.") }),
    steps: [{ fill: "Login name", value: PERSON.login }, { fill: "Master password", value: "not it" }, { fill: "Secret Key", value: "A3-K7QF2M" }, { click: "Sign in", role: "button", within: '[role="tabpanel"]' }, { wait: /don't open an account/ }],
  }),
  w({
    id: "web-sign-in-rate-limited",
    screen: "Sign in",
    state: "Rate limited",
    brief_row: "Sign in",
    notes: "Too many tries: the server's wait, in words, in the compact critical Banner (brief W1).",
    built: { area: "sign-in", screen: "rate-limited", kind: "undrawn" },
    setup: signedOut,
    answers: () => ({ web_sign_in: () => fail("refused", "Too many sign-in attempts for this login. Try again in 15 minutes.") }),
    steps: [{ fill: "Login name", value: PERSON.login }, { fill: "Master password", value: "x" }, { fill: "Secret Key", value: "A3" }, { click: "Sign in", role: "button", within: '[role="tabpanel"]' }, { wait: /Too many sign-in attempts/ }],
  }),
  w({
    id: "web-sign-in-security-key",
    screen: "Sign in",
    state: "Waiting for a security key",
    brief_row: "Security-key or email wait",
    notes: "The account asks for a security key on a device it has never seen (D250): \"Use your security key.\", Use my security key, Cancel (brief W1).",
    built: { area: "sign-in", screen: "security-key", kind: "undrawn" },
    setup: signedOut,
    answers: () => ({ web_sign_in: () => fail("approval_needed", "This account asks for a security key on a new device.", "security_key") }),
    steps: [{ fill: "Login name", value: PERSON.login }, { fill: "Master password", value: "x" }, { fill: "Secret Key", value: "A3" }, { click: "Sign in", role: "button", within: '[role="tabpanel"]' }, { wait: "Use your security key.", role: "heading" }, { pause: 300 }],
  }),
  w({
    id: "web-sign-in-email-wait",
    screen: "Sign in",
    state: "Waiting for an email approval",
    brief_row: "Security-key or email wait",
    notes: "The server emailed a link to approve this tab (D300); the address is masked. \"Check your email.\", a live line, Cancel (brief W1).",
    built: { area: "sign-in", screen: "email-wait", kind: "undrawn" },
    setup: signedOut,
    answers: () => ({ web_sign_in: () => fail("approval_needed", "we emailed s••@rivera.example a link to approve this tab", "email"), web_sign_in_continue: () => forever() }),
    steps: [{ fill: "Login name", value: PERSON.login }, { fill: "Master password", value: "x" }, { fill: "Secret Key", value: "A3" }, { click: "Sign in", role: "button", within: '[role="tabpanel"]' }, { wait: "Check your email.", role: "heading" }, { pause: 300 }],
  }),
  w({ id: "web-starting", screen: "Sign in", state: "Starting", brief_row: "Sign in", notes: "The worker has not answered yet.", answers: () => ({ web_state: () => forever() }), steps: [{ pause: 300 }] }),
  w({ id: "web-cant-start", screen: "Sign in", state: "Can't start", brief_row: "Sign in", notes: "The core module failed its integrity check.", answers: () => ({ web_state: () => fail("stopped", "The vault's core module didn't match its published hash, so it wasn't run. Reload the page; if this stays, tell the server's operator.") }), steps: [{ wait: "The web vault can't start" }] }),
  w({ id: "web-unlock", screen: "Unlock", state: "Locked", brief_row: "Unlock", notes: "Signed in, locked: the password only (D174). Brief W1 as F1, with Sign out of this tab.", built: { area: "web-unlock", screen: "locked", kind: "undrawn" }, steps: [{ wait: "Lockdown is locked.", role: "heading" }, { wait: "Unlock", role: "button" }] }),
  w({ id: "web-unlock-wrong", screen: "Unlock", state: "Wrong password", brief_row: "Unlock", notes: "The password didn't match: the core's words under the field.", built: { area: "web-unlock", screen: "wrong-password", kind: "undrawn" }, steps: [{ fill: "Master password", value: "wrong" }, { click: "Unlock", role: "button" }, { wait: /didn't match/ }] }),
  w({ id: "web-settings", screen: "Settings", state: "This browser", brief_row: "Settings", notes: "Brief T0, T1 and W1: four sections; This browser holds Auto-lock (no Never), the tab's out-of-view lock, Clipboard and Reveal, then THIS WEB VAULT: signed in as, the trust note (D180), Sign out of this tab.", steps: [...webOpen, ...go("settings"), inPage(/Auto-lock|Appearance/), { pause: 300 }] }),
  w({ id: "web-link-share", screen: "Link page", state: "Share link: before opening", brief_row: "Link page (`/s/…`, `/r/…`)", notes: "Brief W2: the views left and the last day as mono lines; Open link. No way into a vault.", built: { area: "link-page", screen: "share", kind: "undrawn" }, mount: () => linkPage("share"), steps: [{ wait: "Open link", role: "button" }, { pause: 300 }] }),
  w({ id: "web-link-share-opened", screen: "Link page", state: "Share link: opened", brief_row: "Link page (`/s/…`, `/r/…`)", notes: "The info Banner, then one row per value: a Field Row for text, a SecretField for a secret.", built: { area: "link-page", screen: "share-opened", kind: "undrawn" }, mount: () => linkPage("share"), steps: [{ click: /^Open/, role: "button" }, { wait: "Rivera-Guest" }, { pause: 300 }] }),
  w({ id: "web-link-request", screen: "Link page", state: "Request link", brief_row: "Link page (`/s/…`, `/r/…`)", notes: "Answer a request for a secret: what they asked for, What it is, the secret, Send.", built: { area: "link-page", screen: "request", kind: "undrawn" }, mount: () => linkPage("request"), steps: [{ wait: "Send", role: "button" }, { pause: 300 }] }),
  w({ id: "web-key-second-factor", screen: "Key pages", state: "Confirm a sign-in with a key", brief_row: "Key pages", notes: "The page the desktop opens when a new device waits for a security key (brief W2).", built: { area: "key-page", screen: "second-factor", kind: "undrawn" }, mount: () => {
    followColorScheme()
    return <SecondFactorPage link={fakeLink} id={uid("pending", 1)} credentials={waitingCredentials} />
  }, steps: [{ wait: "Use my security key", role: "button" }, { pause: 300 }] }),
  w({ id: "web-key-add", screen: "Key pages", state: "Add a security key", brief_row: "Key pages", notes: "Adding a security key to the account: Name, Add key (brief W2).", built: { area: "key-page", screen: "add", kind: "undrawn" }, mount: () => {
    followColorScheme()
    return <SecurityKeyPage link={fakeLink} id={uid("pending", 2)} credentials={waitingCredentials} />
  }, steps: [{ wait: "Add key", role: "button" }, { pause: 300 }] }),
]
