// More of the desktop app: the panels, dialogs and sheets a screen opens
// (brief §3.3), the approvals that arrive on their own, and the shell's
// notices. Same App, same fixture; each state is a few clicks deep.

import App from "@/App"
import { DeviceApproval } from "@/features/approvals/approval-sheet"
import { LIVE_EVENT } from "@/features/sessions/live-client"

import { PERSON } from "../fixture/seed"
import { status } from "../fixture/story"
import { ago, ahead } from "../fixture/time"
import type { World } from "../fixture/world"
import { as, fail, forever } from "../host/core"
import { Standalone } from "../harness/standalone"
import type { StateDef, Step } from "../harness/types"
import { WIDTHS } from "../harness/types"

const DIALOG: Step = { see: '[role="dialog"], [role="alertdialog"]' }
const MENU: Step = { see: '[role="menu"]' }
const esc = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
const starts = (s: string) => new RegExp(`^${esc(s)}(\\s|$)`)
const side = (label: string): Step => ({ click: starts(label), within: 'nav[aria-label="Sidebar"]' })
const open: Step[] = [{ click: "Unlock" }, { wait: /^All items/ }]
const item = (title: string): Step[] => [{ click: starts(title), within: "main" }, { wait: title, role: "heading" }]
/** The vaults, folders and tags trees are hidden below the `md` breakpoint, so these states have no phone width. */
const WIDE = [1280, 900]
const settings: Step[] = [...open, side("Settings"), { wait: "Density" }]

/** The core's words while a split view pauses sharing (crates/app/src/session/split_pause.rs). */
const SHARING_PAUSED =
  "Sharing is paused: this server's public log doesn't match a mirror, and a server showing two histories could swap someone's keys. Until it clears, Lockdown won't look anyone up or check their keys with a phrase, share a vault or an item, make share or request links, name an emergency contact or open emergency access. Your vault still opens and syncs."
const PAUSED_REASON =
  "is paused: this server's public log doesn't match a mirror, and a server showing two histories could swap someone's keys. Your vault still opens and syncs; this works again once the split clears."
/** The core's words for a revoked device (crates/sync/src/renew.rs, `SignInReason::Revoked`). */
const REVOKED = "This device was revoked from your account. Join it again from a device you still use."
/** The core's words on a shared vault whose new members wait (crates/app/src/session/split_hold.rs). */
const HELD = "New members wait until the server's log matches its mirrors again."
/** The core's words once a head from someone unmet stopped the vault (crates/app/src/session/shared_sync.rs). */
const HELD_AFTER =
  "New members wait until the server's log matches its mirrors again. Changes made after a new member's or a new device's wait too, even from members you already have."
/** Picks an option of a native select the way a person would, so React hears the change. */
function choose(selector: string, value: string) {
  const el = document.querySelector<HTMLSelectElement>(selector)
  if (!el) throw new Error(`no select at ${selector}`)
  Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value")!.set!.call(el, value)
  el.dispatchEvent(new Event("change", { bubbles: true }))
}

type DesktopDef = Omit<StateDef, "surface" | "widths" | "mount"> & Partial<Pick<StateDef, "widths" | "mount">>
const d = (s: DesktopDef): StateDef => ({ surface: "desktop", widths: WIDTHS, mount: () => <App />, ...s })
const fresh = (w: World) => {
  w.device = { identity: false, deviceKey: false }
}
const reminders = (password: boolean, drill: boolean, kitTest: boolean) => ({
  password: { applies: true, due: password, last_at: ago(15), every_days: 14, next_at: ago(1) },
  drill: { kit: kitTest ? "created" : "drilled", last_drilled_at: kitTest ? null : ago(370), next_at: ago(5), due: drill, kit_test_pending: kitTest },
})

export const DESKTOP_MORE: StateDef[] = [
  // --- first run, deeper ------------------------------------------------------------
  d({ id: "desktop-first-run-kit", screen: "Kit page", state: "Shown once after create", brief_row: "Kit page (D69)", notes: "The Secret Key and recovery code in large print, every character captioned.", setup: fresh, steps: [{ click: "Create my vault", role: "button", within: '[role="tabpanel"]' }, { fill: "Login name", value: PERSON.login }, { fill: "Account password", value: "Tidal-Vesper-Orbit-48" }, { click: /^Create account/ }, DIALOG, { pause: 400 }] }),
  d({ id: "desktop-first-run-keep-kit", screen: "First run", state: "Keep the kit", brief_row: "First run: create", notes: "Screen 3: printed, and the drill reminder.", setup: fresh, steps: [{ click: "Create my vault", role: "button", within: '[role="tabpanel"]' }, { fill: "Login name", value: PERSON.login }, { fill: "Account password", value: "Tidal-Vesper-Orbit-48" }, { click: /^Create account/ }, DIALOG, { click: "I have it" }, { wait: "Keep the kit somewhere safe" }] }),
  d({ id: "desktop-first-run-proofs", screen: "First run", state: "Locked, even from us", brief_row: "First run: create", notes: "The proofs, once, as part of account creation (D95).", setup: fresh, steps: [{ click: "Create my vault", role: "button", within: '[role="tabpanel"]' }, { fill: "Login name", value: PERSON.login }, { fill: "Account password", value: "Tidal-Vesper-Orbit-48" }, { click: /^Create account/ }, DIALOG, { click: "I have it" }, { click: "I printed or wrote down the kit" }, { click: "Continue", role: "button" }, { wait: "Locked, even from us" }] }),
  d({
    id: "desktop-first-run-weak-password",
    screen: "First run",
    state: "Error: weak password",
    brief_row: "First run: create",
    notes: "The password is under the signed floor.",
    setup: fresh,
    answers: () => ({ account_register: () => fail("refused", "That password would be guessed quickly. Use at least four random words, or let Lockdown suggest one.") }),
    steps: [{ click: "Create my vault", role: "button", within: '[role="tabpanel"]' }, { fill: "Login name", value: PERSON.login }, { fill: "Account password", value: "sam1988" }, { click: /^Create account/ }, { wait: /would be guessed quickly/ }],
  }),
  d({ id: "desktop-first-run-opt-out", screen: "First run", state: "An account without a Secret Key", brief_row: "First run: create", notes: "Making an account without a Secret Key needs an explicit confirmation (D247).", setup: fresh, steps: [{ click: "Create my vault", role: "button", within: '[role="tabpanel"]' }, { click: "An account without a Secret Key" }, DIALOG] }),
  d({ id: "desktop-first-run-join-device", screen: "Join from another device", state: "Code or QR", brief_row: "Join from another device", notes: "Join with a code or QR from an enrolled device, then compare six digits (D246).", setup: fresh, steps: [{ click: "Join", role: "tab" }, { click: "Join with another device instead" }, { pause: 400 }] }),
  d({
    id: "desktop-first-run-join-denied",
    screen: "First run",
    state: "Join: error",
    brief_row: "First run: join",
    notes: "The server refused the join.",
    setup: fresh,
    answers: () => ({ account_login: () => fail("refused", "The other device denied this request, or it timed out. Ask again from the other device.") }),
    steps: [{ click: "Join", role: "tab" }, { fill: "Login name", value: PERSON.login }, { fill: "Account password", value: "x" }, { fill: "Secret Key", value: "A3-K7QF2M" }, { click: /^Join$/, role: "button", within: '[role="tabpanel"]' }, { wait: /denied this request/ }],
  }),

  // --- organization ---------------------------------------------------------------------
  d({ id: "desktop-nav-vault-menu", widths: WIDE, screen: "Vaults and tags trees", state: "Vault menu", brief_row: "Vaults and tags trees", notes: "A vault's menu: new folder, rename.", steps: [...open, { click: "Personal options" }, MENU] }),
  d({ id: "desktop-nav-new-vault", widths: WIDE, screen: "Vaults and tags trees", state: "New vault", brief_row: "Vaults and tags trees", notes: "Naming a new vault.", steps: [...open, { click: "New vault" }, DIALOG] }),
  d({ id: "desktop-nav-tag", widths: WIDE, screen: "Vaults and tags trees", state: "Narrowed to a tag", brief_row: "Vaults and tags trees", notes: "The list narrowed to #travel.", steps: [...open, side("travel")] }),
  d({ id: "desktop-item-more-menu", screen: "Where it is, item menu", state: "Item menu", brief_row: "Where it is, item menu", notes: "History, archive, move, crown-jewel on or off.", steps: [...open, ...item("Bluefin Bank"), { click: "More for this item" }, MENU] }),
  d({ id: "desktop-item-history", screen: "Version history", state: "Versions", brief_row: "Version history", notes: "Every version, which device, which fields; hidden values stay hidden.", steps: [...open, ...item("Northwind Mail"), { click: "More for this item" }, { click: "History", role: "menuitem" }, DIALOG] }),
  d({ id: "desktop-item-jewel-on", screen: "Where it is, item menu", state: "Keep as a crown jewel?", brief_row: "Where it is, item menu", notes: "Moving an item into the crown-jewel class.", steps: [...open, ...item("Bluefin Bank"), { click: "More for this item" }, { click: "Keep as a crown jewel", role: "menuitem" }, DIALOG] }),

  // --- item detail panels ------------------------------------------------------------------
  d({ id: "desktop-item-lookalike", screen: "Lookalike warning", state: "Site imitates another", brief_row: "Lookalike warning", notes: "First in the detail: this login's site imitates the bank's.", steps: [...open, ...item("Bluefin Bank alerts")] }),
  d({ id: "desktop-item-share-menu", screen: "Item Share menu", state: "Menu", brief_row: "Item Share menu", notes: "Share with someone, or as a link.", steps: [...open, ...item("Harbor Health"), { click: /^Share$/ }, MENU] }),
  d({ id: "desktop-item-share-person", screen: "Item Share menu", state: "With someone", brief_row: "Item Share menu", notes: "Share one item with a verified contact: view or fill only.", steps: [...open, ...item("Harbor Health"), { click: /^Share$/ }, { click: /^With someone/, role: "menuitem" }, DIALOG] }),
  d({ id: "desktop-item-share-link", screen: "Item Share menu", state: "As a share link", brief_row: "Item Share menu", notes: "Lifetime, opens, password.", steps: [...open, ...item("Guest Wi-Fi"), { click: /^Share$/ }, { click: /^As a share link/, role: "menuitem" }, DIALOG] }),
  d({ id: "desktop-item-share-link-once", screen: "Link shown once", state: "Copy the link", brief_row: "Link shown once", notes: "The new link, shown one time.", steps: [...open, ...item("Guest Wi-Fi"), { click: /^Share$/ }, { click: /^As a share link/, role: "menuitem" }, DIALOG, { click: "Make the link" }, { wait: "Copy the link" }] }),
  d({ id: "desktop-item-wifi-card", screen: "Wi-Fi guest card", state: "Join the network", brief_row: "Wi-Fi guest card", notes: "A QR guests scan; hides after 60 s.", steps: [...open, ...item("Guest Wi-Fi"), { click: /Guest card/ }, DIALOG, { pause: 300 }] }),
  d({ id: "desktop-item-large-type", screen: "Large type", state: "Wi-Fi password", brief_row: "Large type", notes: "Read across a room; 120 s.", steps: [...open, ...item("Guest Wi-Fi"), { click: /Show large type/ }, DIALOG, { pause: 400 }] }),
  d({ id: "desktop-item-large-type-code", screen: "Large type", state: "A code", brief_row: "Large type", notes: "A code's current value in large type.", steps: [...open, ...item("Driftwood Forum"), { click: /Show large type/ }, DIALOG, { pause: 400 }] }),
  d({ id: "desktop-item-away-dialog", screen: "Signed in elsewhere", state: "Log a sign-in", brief_row: "Signed in elsewhere", notes: "Signing in on a TV: where, and when to remind (D237).", steps: [...open, ...item("Tern Airlines"), { click: /Signing in on a TV/ }, DIALOG] }),
  d({ id: "desktop-item-passkey-delete", screen: "Passkeys block", state: "Delete: the only way in", brief_row: "Passkeys block", notes: "Deleting the only passkey warns that the account would have no way in.", steps: [...open, ...item("Pixelforge"), { click: "Delete", role: "button" }, DIALOG] }),
  d({ id: "desktop-item-setup-guide", screen: "Set-up guide", state: "Two-step sign-in for a site", brief_row: "Set-up guide", notes: "What Harbor Health offers for two-step sign-in, and how (from its Security finding).", steps: [...open, side("Security"), { click: /Set-up guide/ }, DIALOG] }),
  d({ id: "desktop-item-playbook", screen: "Playbook (\"What to do\")", state: "From the item", brief_row: "Playbook (\"What to do\")", notes: "Steps ending in how to confirm the fix.", steps: [...open, ...item("Parcelpost"), { click: "What to do" }, DIALOG] }),
  d({ id: "desktop-item-file-remove", screen: "Files", state: "Take a file off?", brief_row: "Files", notes: "Removing an attached file.", steps: [...open, ...item("Home insurance"), { click: "Remove policy-2026.pdf" }, DIALOG] }),
  d({ id: "desktop-item-fill-only", screen: "Item detail", state: "Shared fill-only", brief_row: "Item Share menu", notes: "An item shared fill-only: no copy, no reveal, no large type.", setup: (w) => {
    const it = w.items.find((i) => i.title === "Kids' school portal")!
    it.access = "fill-only"
  }, steps: [...open, ...item("Kids' school portal")] }),
  d({ id: "desktop-item-tier", screen: "Item detail", state: "Risk tier badge", brief_row: "Item detail", notes: "The RiskTierBadge seam: no item carries a tier in the vault model yet, so this one is given one by the fixture.", setup: (w) => {
    w.items.find((i) => i.title === "Cobalt Cloud console")!.tier = "official"
  }, steps: [...open, { click: starts("Cobalt Cloud console"), within: "main" }, { wait: /^Cobalt Cloud console/, role: "heading" }] }),

  // --- editor ------------------------------------------------------------------------------
  d({ id: "desktop-editor-new-wifi", screen: "Item edit", state: "New Wi-Fi network", brief_row: "Item edit", notes: "The editor for another kind.", steps: [...open, { click: /^New item/ }, { click: /^Wi-Fi network/, role: "menuitem" }, DIALOG] }),
  d({ id: "desktop-editor-new-passport", screen: "Item edit", state: "New passport", brief_row: "Item edit", notes: "A template variant.", steps: [...open, { click: /^New item/ }, { click: /^Passport/, role: "menuitem" }, DIALOG] }),
  d({ id: "desktop-editor-generator", screen: "Generator options", state: "Characters", brief_row: "Generator options", notes: "Generate with options; strength without a value.", steps: [...open, { click: /^New item/ }, { click: /^Login/, role: "menuitem" }, DIALOG, { click: "Generate password" }, { pause: 400 }] }),
  d({ id: "desktop-editor-templates", screen: "Your templates", state: "List", brief_row: "Your templates", notes: "The person's own templates.", steps: [...open, { click: /^New item/ }, { click: /templates/, role: "menuitem" }, DIALOG] }),
  d({ id: "desktop-editor-template-new", screen: "Your templates", state: "New template", brief_row: "Your templates", notes: "Designing a form of your own.", steps: [...open, { click: /^New item/ }, { click: /templates/, role: "menuitem" }, DIALOG, { click: "New template" }, { pause: 300 }] }),
  d({
    id: "desktop-editor-save-error",
    screen: "Item edit",
    state: "Error on save",
    brief_row: "Item edit",
    notes: "The core refused the save; inline error.",
    answers: () => ({ edit_item: () => fail("refused", "This item changed on another device while you edited it. Your edit was kept as a conflict copy.") }),
    steps: [...open, ...item("Quill Notes"), { click: "Edit", role: "button" }, DIALOG, { click: "Save", role: "button" }, { pause: 500 }],
  }),

  // --- codes ---------------------------------------------------------------------------------
  d({ id: "desktop-codes-scan-paste", screen: "Sign in a TV or console", state: "Paste: refused", brief_row: "Sign in a TV or console", notes: "A pasted code is never approved: only one scanned from a screen in front of you.", steps: [...open, side("Codes"), { click: "Sign in a TV" }, DIALOG, { click: "Paste", role: "tab" }, { fill: "The code's address", value: "https://watch.meadowlark.example/activate?code=WDJB-MJHT" }, { click: "Check it" }, { pause: 600 }] }),
  d({ id: "desktop-codes-search", screen: "Codes view", state: "Search", brief_row: "Codes view", notes: "Searching issuers and accounts.", steps: [...open, side("Codes"), { fill: "Search codes", value: "cobalt" }] }),

  // --- security ----------------------------------------------------------------------------------
  d({ id: "desktop-security-checking", screen: "Security", state: "Checking", brief_row: "Security", notes: "Check now running.", answers: () => ({ security_check: () => forever() }), steps: [...open, side("Security"), { wait: /Checking/ }] }),
  d({ id: "desktop-security-dismissed", screen: "Security", state: "Dismissed and snoozed", brief_row: "Security", notes: "The collapsed section opened.", steps: [...open, side("Security"), { click: /dismissed and snoozed/ }] }),
  d({ id: "desktop-security-playbook", screen: "Playbook (\"What to do\")", state: "From the report", brief_row: "Playbook (\"What to do\")", notes: "What to do about a breached password.", steps: [...open, side("Security"), { click: "What to do" }, DIALOG] }),
  d({ id: "desktop-security-dismiss-menu", screen: "Security", state: "Dismiss menu", brief_row: "Security", notes: "Dismiss or snooze a finding.", steps: [...open, side("Security"), { click: /^Dismiss: /, nth: 3 }, MENU] }),
  d({ id: "desktop-security-incident", screen: "Something happened?", state: "In progress", brief_row: "Something happened?", notes: "A guided flow filled from the vault.", steps: [...open, side("Security"), { click: "Carry on" }, DIALOG] }),
  d({ id: "desktop-security-pq-on", screen: "Security", state: "Quantum-safe signatures: on", brief_row: "Security", notes: "Every device reads the stronger signatures (docs/plan/30-pq-status-line.md).", answers: () => ({ pq_status: () => ({ state: "on", reason: null, headline: "Quantum-safe signatures: on", explanation: "Every change your devices make to your vaults is signed in a way that even a future quantum computer couldn't fake.", without_level_1: [], without_level_2: [] }) }), steps: [...open, side("Security")] }),
  d({ id: "desktop-security-pq-unknown", screen: "Security", state: "Quantum-safe signatures: can't check", brief_row: "Security", notes: "The server didn't report the account's devices.", answers: () => ({ pq_status: () => ({ state: "unknown", reason: "silent", headline: "Quantum-safe signatures: can't check right now", explanation: "Your server didn't report your account's devices. It may be an older version.", without_level_1: [], without_level_2: [] }) }), steps: [...open, side("Security")] }),
  d({ id: "desktop-devices-from-pq", screen: "Devices", state: "Brought into view from Security", brief_row: "Devices", notes: "The quantum-safe line's device link opens Devices on that device.", steps: [...open, side("Security"), { click: "Studio desktop", role: "button" }, { pause: 500 }] }),
  d({ id: "desktop-security-error", screen: "Security", state: "Error", brief_row: "Security", notes: "The report could not be made.", answers: () => ({ security_report: () => fail("refused", "The security report needs the vault's index, which is still being rebuilt. Try again in a minute.") }), steps: [...open, side("Security"), { pause: 600 }] }),

  // --- sharing ----------------------------------------------------------------------------------------
  d({ id: "desktop-sharing-offline", screen: "Sharing", state: "Offline", brief_row: "Sharing", notes: "No account connection: actions disabled.", answers: () => ({ sharing_overview: () => ({ ...(as("sharing_overview")({}) as object), online: false }) }), steps: [...open, side("Sharing")] }),
  d({ id: "desktop-sharing-compare", screen: "People", state: "Compare fingerprints", brief_row: "People", notes: "Compare by reading out, pasting or scanning.", steps: [...open, side("Sharing"), { click: "Compare" }, DIALOG] }),
  d({ id: "desktop-sharing-phrase", screen: "Phrase check", state: "Say this phrase", brief_row: "Phrase check", notes: "Verify keys with words said aloud.", steps: [...open, side("Sharing"), { click: "Check with a phrase" }, DIALOG] }),
  d({ id: "desktop-sharing-forget", screen: "People", state: "Forget?", brief_row: "People", notes: "Forgetting a contact.", steps: [...open, side("Sharing"), { click: "Forget", nth: 2 }, DIALOG] }),
  d({ id: "desktop-sharing-vault", screen: "Manage sharing", state: "Family vault", brief_row: "Manage sharing", notes: "Members, rights, add, remove, history.", steps: [...open, side("Sharing"), { click: /^Manage sharing/ }, DIALOG] }),
  d({ id: "desktop-sharing-text-link", screen: "Share and request links", state: "Share some text", brief_row: "Share and request links", notes: "Make a text link.", steps: [...open, side("Sharing"), { click: /^Share some text/ }, DIALOG] }),
  d({ id: "desktop-sharing-request-link", screen: "Share and request links", state: "Ask for a secret", brief_row: "Share and request links", notes: "Make a request link.", steps: [...open, side("Sharing"), { click: /^Ask someone for a secret/ }, DIALOG] }),
  d({ id: "desktop-sharing-opened", screen: "Opened link", state: "An answer opened", brief_row: "Opened link", notes: "Reading an answered request one value at a time.", steps: [...open, side("Sharing"), { click: "Open the answer" }, { pause: 600 }] }),
  d({ id: "desktop-sharing-emergency-add", screen: "Emergency access", state: "Add a contact", brief_row: "Emergency access", notes: "Name a verified contact with a wait.", steps: [...open, side("Sharing"), { click: /^Add an emergency contact/ }, DIALOG] }),
  d({ id: "desktop-sharing-emergency-requested", screen: "Emergency access", state: "Asked for access", brief_row: "Emergency access", notes: "Your contact asked for access: the banner with Stop.", answers: () => ({ sharing_overview: () => {
    const o = as("sharing_overview")({}) as { emergency: Array<Record<string, unknown>> }
    o.emergency[0] = { ...o.emergency[0], state: "requested", release_at: ahead(5) }
    return o
  } }), steps: [...open, side("Sharing")] }),

  // --- devices -------------------------------------------------------------------------------------------
  d({ id: "desktop-devices-add", screen: "Add a device", state: "Code and QR", brief_row: "Add a device", notes: "A link for the new device: a code to type, or a QR to scan; ten minutes, once.", answers: () => ({ link_poll: () => as("link_offer")({}) }), steps: [...open, side("Devices"), { click: "Add a device" }, DIALOG, { pause: 400 }] }),
  d({ id: "desktop-devices-add-compare", screen: "Add a device", state: "Do the digits match?", brief_row: "Add a device", notes: "The new device shows six digits; approve if they match.", steps: [...open, side("Devices"), { click: "Add a device" }, DIALOG, { wait: "Do the digits match?" }] }),
  d({ id: "desktop-devices-end-session", screen: "Signed in", state: "End all other sessions?", brief_row: "Signed in", notes: "Ending every session but this one.", steps: [...open, side("Devices"), { click: "End all other sessions" }, DIALOG] }),

  // --- recovery and status ------------------------------------------------------------------------------------
  d({ id: "desktop-recovery-drill", screen: "Recovery drill", state: "Type the kit back", brief_row: "Recovery drill", notes: "The drill: Secret Key, recovery code, password.", steps: [...open, side("Recovery"), { click: "Run the drill" }, DIALOG] }),
  d({
    id: "desktop-recovery-drill-failed",
    screen: "Recovery drill",
    state: "Kit does not open",
    brief_row: "Recovery drill",
    notes: "The kit typed back doesn't open the vault.",
    answers: () => ({ kit_verify: () => fail("refused", "That Secret Key and recovery code don't open this vault. Check each character against the printed page; the kit may have been replaced since.") }),
    steps: [...open, side("Recovery"), { click: "Run the drill" }, DIALOG, { fill: "Secret Key", value: "A3-K7QF2M-9XWD4R" }, { fill: "Recovery code", value: "RC-4F9K" }, { click: "Check the kit" }, { wait: /don't open this vault/ }],
  }),
  d({ id: "desktop-recovery-kit-replace", screen: "Kit page", state: "Replace the kit", brief_row: "Kit page (D69)", notes: "Making a new kit replaces the old one.", steps: [...open, side("Recovery"), { click: /^Replace/ }, DIALOG] }),
  d({ id: "desktop-recovery-emergency-sheet", screen: "Emergency sheet", state: "Printable", brief_row: "Emergency sheet", notes: "One page for a future you, with no secret.", steps: [...open, side("Recovery"), { click: "Emergency sheet" }, DIALOG] }),
  d({ id: "desktop-status-no-account", screen: "Status", state: "No account", brief_row: "Status", notes: "This device only: Log in or Register.", answers: (w) => ({ status: () => ({ ...status(w), account: null }), unlock: () => ({ ...status(w), account: null }) }), steps: [...open, side("Status")] }),
  d({ id: "desktop-status-register", screen: "Status", state: "Register an account", brief_row: "Status", notes: "Adding an account later, from Status.", answers: (w) => ({ status: () => ({ ...status(w), account: null }), unlock: () => ({ ...status(w), account: null }) }), steps: [...open, side("Status"), { click: /^Register/ }, DIALOG] }),
  d({ id: "desktop-status-split", screen: "Status", state: "Mirror disagrees", brief_row: "Status", notes: "A mirror saw a different log: the split-view warning (D327, D328).", answers: (w) => ({ status: () => {
    const s = status(w)
    s.account!.transparency!.mirrors = [{ url: "https://kt.witness.example", named_by: "settings", at: ago(0.01), state: "split", message: "The mirror has a different log at size 48,209." }]
    return s
  } }), steps: [...open, side("Status")] }),

  // --- settings --------------------------------------------------------------------------------------------------
  d({ id: "desktop-settings-compact", screen: "Settings", state: "Compact density chosen", brief_row: "Settings", notes: "Appearance set to compact: the whole app tightens.", steps: [...settings, { click: "Compact" }, { pause: 300 }] }),
  d({ id: "desktop-settings-import", screen: "Import / export", state: "Import: pick a format", brief_row: "Import / export", notes: "Twenty formats; a preview before anything is kept.", steps: [...settings, { click: /^Import…/ }, DIALOG] }),
  d({ id: "desktop-settings-export", screen: "Import / export", state: "Export", brief_row: "Import / export", notes: "Four file kinds plus age; crown jewels left out unless opted in.", steps: [...settings, { click: /^Export…/ }, DIALOG, { pause: 400 }] }),
  d({ id: "desktop-settings-backup-restore", screen: "Settings → Backups", state: "Restore from a backup", brief_row: "Settings → Backups", notes: "Restore opens the import preview.", steps: [...settings, { click: /^Restore…/ }, DIALOG] }),
  d({ id: "desktop-settings-backups-off", screen: "Settings → Backups", state: "Off", brief_row: "Settings → Backups", notes: "Backups are off; the sidebar line says so too.", setup: (w) => {
    w.settings = { ...w.settings, backup: { enabled: false, folder: null, interval_hours: 24, keep_last: 10 } }
  }, steps: settings }),
  d({ id: "desktop-settings-browsers-refused", screen: "Settings → Browsers", state: "Host refused", brief_row: "Settings → Browsers", notes: "Every browser host refused (error colour).", answers: () => ({ browser_status: () => ({ ...(as("browser_status")({}) as object), listening: false, problem: "The browser host at /Applications/Lockdown.app/Contents/MacOS/lockdown-native-host isn't signed by this app's developer, so Lockdown refuses it.", peer_check: { by: "refused", detail: "The host's signature didn't match." } }) }), steps: settings }),

  // --- approvals and prompts that arrive on their own ------------------------------------------------------------------
  d({ id: "desktop-approval-browser-pairing", screen: "Browser pairing", state: "Connect Chrome?", brief_row: "Browser pairing", notes: "A browser asks to pair; compare the code.", answers: () => ({ browser_pending: () => [{ request_id: "pr-1", code: "482913", name: "Chrome on Sam's MacBook Pro", family: "chromium", extension_id: "dbdblnolpiehhlabpmhobmkbjipmebpe", expires_in_secs: 118 }] }), steps: [...open, DIALOG] }),
  d({ id: "desktop-approval-passkey", screen: "Passkey approval", state: "Sign in with a passkey?", brief_row: "Passkey approval", notes: "On a password-only machine, a passkey request waits for Approve.", answers: () => ({ browser_passkey_pending: () => [{ request_id: "pk-1", browser: "Firefox", text: "Sign in to pixelforge.example as sam_r", expires_in_secs: 117 }] }), steps: [...open, DIALOG] }),
  d({ id: "desktop-reminder-kit-test", screen: "Reminders", state: "Test your recovery kit", brief_row: "Reminders", notes: "Right after the first run: test the kit, or skip with \"I understand an untested kit can fail\" (D251).", answers: () => ({ reminders: () => reminders(false, false, true) }), steps: [...open, DIALOG] }),
  d({ id: "desktop-reminder-password", screen: "Reminders", state: "Your password, once more", brief_row: "Reminders", notes: "Every 14 days where unlock never asks for the password (D252).", answers: () => ({ reminders: () => reminders(true, false, false) }), steps: [...open, DIALOG] }),
  d({ id: "desktop-reminder-drill", screen: "Reminders", state: "Time for the drill", brief_row: "Reminders", notes: "The yearly drill (D253).", answers: () => ({ reminders: () => reminders(false, true, false) }), steps: [...open, DIALOG] }),
  d({
    id: "desktop-approval-sheet-seam",
    screen: "Approvals",
    state: "ApprovalSheet (seam)",
    brief_row: "Approvals",
    notes: "unicron's ApprovalSheet as the host binds it; nothing in the core produces a request yet, so no screen mounts it (brief §3.2).",
    mount: () => (
      <Standalone>
        <DeviceApproval
          request={{ id: "ap-1", kind: "device", origin: "pairing", context: [{ label: "Device", value: "Sam's new phone" }, { label: "Where", value: "Port Elm, WA (198.51.100.23)" }, { label: "When", value: "Just now" }], expected: "418", digits: 3 }}
          onApprove={() => {}}
          onDeny={() => {}}
          onReport={() => {}}
        />
      </Standalone>
    ),
    steps: [DIALOG, { pause: 400 }],
  }),

  // --- the shell's notices -----------------------------------------------------------------------------------------------
  d({ id: "desktop-live-session-ended", screen: "Shell", state: "Session ended elsewhere", brief_row: null, notes: "Another device ended this one's session: the toast.", steps: [...open, { emit: LIVE_EVENT, payload: { kind: "session_ended" } }, { pause: 600 }] }),
  d({ id: "desktop-live-split", screen: "Shell", state: "Log split warning", brief_row: "Status", notes: "A sync's mirrors caught a split view: the sticky warning.", steps: [...open, { emit: LIVE_EVENT, payload: { kind: "synced", pulled: 2, lines: ["Split view: the mirror at kt.witness.example disagrees."] } }, { pause: 600 }] }),
  d({ id: "desktop-locked-wash-narrow", screen: "Unlock", state: "Locked, behind the wash", brief_row: "Vault", notes: "The vault locked: the wash and skeleton rows, nothing readable (brief §4).", steps: [...open, side("Lock"), { wait: "Unlock", role: "button" }] }),
  d({ id: "desktop-palette-crown-jewel", screen: "Quick-access palette", state: "Crown jewel by title", brief_row: "Quick-access palette", notes: "Crown jewels are listed by title only; copying asks first.", steps: [...open, { click: "Quick access" }, DIALOG, { fill: /Search/i, value: "cold" }, { pause: 400 }] }),
  // --- small calls (docs/plan/34-small-calls.md) --------------------------------------------------------------------------
  d({
    id: "desktop-passkey-upgrade-toast",
    screen: "Shell",
    state: "A site added a passkey",
    brief_row: "Passkeys block",
    notes: "After a password sign-in Lockdown saw, the site added a passkey with no prompt; the app says so afterwards, with Show (D210, D312 as changed by D375).",
    answers: (w) => {
      let told = false
      const login = w.items.find((i) => i.title === "Northwind Mail")!
      return {
        passkey_upgrades: () => {
          if (told) return []
          told = true
          return [{ item_id: login.id, title: login.title, site: "mail.northwind.example", at: ago(0.001) }]
        },
      }
    },
    steps: [...open, { wait: "mail.northwind.example added a passkey to this login" }, { pause: 400 }],
  }),
  d({
    id: "desktop-item-passkey-upgraded",
    screen: "Item detail",
    state: "Passkey added by the site",
    brief_row: "Passkeys block",
    notes: "The login says who added its passkey, and when, until it is opened once (D210, D312 as changed by D375).",
    answers: () => ({
      passkeys: (a) => (as("passkeys")(a) as Array<Record<string, unknown>>).map((p, i) => (i === 0 ? { ...p, upgrade: { site: String(p.rp_id), at: ago(0.02) } } : p)),
    }),
    steps: [...open, ...item("Northwind Mail"), { wait: /^Added by / }],
  }),
  d({
    id: "desktop-editor-generator-words",
    screen: "Generator options",
    state: "Six words: strong",
    brief_row: "Generator options",
    notes: "The default passphrase, six words (77.5 bits), reads strong: weak under 50 bits, fair under 75 (D230 as changed by D375).",
    steps: [
      ...open,
      { click: /^New item/ },
      { click: /^Login/, role: "menuitem" },
      DIALOG,
      { click: "Generate password" },
      { click: "Options" },
      { pause: 300 },
      { run: () => choose('[aria-label="Generator options"] select', "words") },
      { wait: /bits, strong/ },
    ],
  }),
  d({
    id: "desktop-sharing-paused",
    screen: "Sharing",
    state: "Paused: split view",
    brief_row: "Sharing",
    notes: "The server's log doesn't match a mirror: new look-ups, shares, links and emergency steps wait; the vault keeps syncing (D328 as changed by D375).",
    answers: () => ({ sharing_overview: () => ({ ...(as("sharing_overview")({}) as object), paused: SHARING_PAUSED }) }),
    steps: [...open, side("Sharing"), { wait: /^Sharing is paused/ }],
  }),
  d({
    id: "desktop-sharing-paused-request",
    screen: "Share and request links",
    state: "Paused: a request link refused",
    brief_row: "Share and request links",
    notes: "Asking for a secret during a split view: the core refuses it and says why (D328 as changed by D375).",
    answers: () => ({
      sharing_overview: () => ({ ...(as("sharing_overview")({}) as object), paused: SHARING_PAUSED }),
      request_create: () => fail("refused", `Making a request link ${PAUSED_REASON}`),
    }),
    steps: [...open, side("Sharing"), { click: /^Ask someone for a secret/ }, DIALOG, { fill: "What you're asking for (they see this)", value: "The Wi-Fi password" }, { click: "Make the link" }, { wait: /^Making a request link is paused/ }],
  }),
  // --- a revoked device, and new members during a split (docs/plan/35-revoke-split.md) ------------------------------------
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
      return { status: revoked, unlock: revoked }
    },
    steps: [...open, side("Status"), { wait: REVOKED }],
  }),
  d({
    id: "desktop-sharing-vault-held",
    screen: "Manage sharing",
    state: "New members wait (split view)",
    brief_row: "Manage sharing",
    notes: "While the server's log doesn't match a mirror, a member added elsewhere waits unapplied; items from members already here keep syncing (doc 35 §2).",
    answers: () => ({ vault_sharing: (a) => ({ ...(as("vault_sharing")(a) as object), held: HELD }) }),
    steps: [...open, side("Sharing"), { click: /^Manage sharing/, nth: 1 }, DIALOG, { wait: HELD }],
  }),
  d({
    id: "desktop-sharing-vault-held-after",
    screen: "Manage sharing",
    state: "Changes after a new member's wait (split view)",
    brief_row: "Manage sharing",
    notes: "A head signed by someone this device hasn't met stopped the vault during a split: heads chain hash to hash, so every later change waits behind it, members' already here included (doc 36 §3).",
    answers: () => ({ vault_sharing: (a) => ({ ...(as("vault_sharing")(a) as object), held: HELD_AFTER }) }),
    steps: [...open, side("Sharing"), { click: /^Manage sharing/, nth: 1 }, DIALOG, { wait: HELD_AFTER }],
  }),
  d({ id: "desktop-backup-late", screen: "Backup line", state: "Late", brief_row: "Backup line", notes: "The sidebar's last-backup line when a backup is overdue.", answers: () => ({ backup_status: () => ({ ...(as("backup_status")({}) as object), last_success_at: ago(4), next_due_at: ago(3) }) }), steps: open }),
]
