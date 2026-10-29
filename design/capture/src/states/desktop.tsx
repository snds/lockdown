// The desktop app's states: the real `App` (apps/desktop/src/App.tsx) over
// the fake core, driven by clicks to each screen, dialog, sheet and menu.

import App from "@/App"
import { QuickWindow } from "@/app/quick-window"

import { as, fail, forever } from "../host/core"
import type { StateDef, Step } from "../harness/types"
import { WIDTHS } from "../harness/types"
import type { World } from "../fixture/world"
import { status } from "../fixture/story"

const app = () => <App />
/** The vaults, folders and tags trees are hidden below the `md` breakpoint, so these states have no phone width. */
const WIDE = [1280, 900]
const DIALOG: Step = { see: '[role="dialog"], [role="alertdialog"]' }
const esc = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
const starts = (s: string) => new RegExp(`^${esc(s)}(\\s|$)`)

/** A sidebar entry by its label. */
const side = (label: string): Step => ({ click: starts(label), within: 'nav[aria-label="Sidebar"]' })
/** Opens the unlocked app. */
const open: Step[] = [{ click: "Unlock" }, { wait: /^All items/ }]
/** Opens an item from the list by its title. */
const item = (title: string): Step[] => [{ click: starts(title), within: "main" }, { wait: title, role: "heading" }]

type DesktopDef = Omit<StateDef, "surface" | "widths" | "mount"> & Partial<Pick<StateDef, "widths" | "mount">>

function d(s: DesktopDef): StateDef {
  return { surface: "desktop", widths: WIDTHS, mount: app, ...s }
}

/** Items that make a view empty. */
const only = (keep: (w: World["items"][number]) => boolean) => (w: World) => {
  w.items = w.items.filter(keep)
}

export const DESKTOP_STATES: StateDef[] = [
  // --- unlock and first run ------------------------------------------------------
  d({ id: "desktop-unlock-locked", screen: "Unlock", state: "Locked", brief_row: "Unlock", notes: "The vault is locked; this device opens it with its own key. The washed shell stands behind.", steps: [{ wait: "Unlock", role: "button" }] }),
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
    state: "Error",
    brief_row: "Unlock",
    notes: "The device keystore refused; the core's sentence is shown as is.",
    answers: () => ({ unlock: () => fail("refused", "The system keychain refused to hand over this device's key. Unlock your Mac's keychain, then try again.") }),
    steps: [{ click: "Unlock" }, { wait: /keychain refused/ }],
  }),
  d({ id: "desktop-unlock-starting", screen: "Unlock", state: "Loading", brief_row: "Unlock", notes: "The core has not answered `probe` yet: a spinner on the wash.", answers: () => ({ probe: () => forever() }), steps: [{ pause: 300 }] }),
  d({
    id: "desktop-unlock-cant-start",
    screen: "Unlock",
    state: "Can't start",
    brief_row: "Unlock",
    notes: "The data folder can't be read.",
    answers: () => ({ probe: () => fail("stopped", "Lockdown can't read its data folder (/Users/sam/Library/Application Support/Lockdown): permission denied.") }),
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
    id: "desktop-first-run-doors",
    screen: "First run",
    state: "Three doors",
    brief_row: "First run: create",
    notes: "Nothing on this computer: the tagline, once, and Create / Join / Recover (D94).",
    setup: (w) => {
      w.device = { identity: false, deviceKey: false }
    },
    steps: [{ wait: /Every way you sign in/ }],
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
    id: "desktop-first-run-create",
    screen: "First run",
    state: "Create: account",
    brief_row: "First run: create",
    notes: "Screen 2: the account and the kit, or this device only.",
    setup: (w) => {
      w.device = { identity: false, deviceKey: false }
    },
    steps: [{ click: "Create my vault", role: "button", within: '[role="tabpanel"]' }, { wait: "Your Secret Key and recovery kit" }],
  }),
  d({
    id: "desktop-first-run-join",
    screen: "First run",
    state: "Join",
    brief_row: "First run: join",
    notes: "Join an account from another device: server, login, password, Secret Key.",
    setup: (w) => {
      w.device = { identity: false, deviceKey: false }
    },
    steps: [{ click: "Join", role: "tab" }, { pause: 300 }],
  }),
  d({
    id: "desktop-first-run-recover",
    screen: "First run",
    state: "Recover",
    brief_row: "Recovery drill",
    notes: "Recover with the whole kit on a device with nothing on it.",
    setup: (w) => {
      w.device = { identity: false, deviceKey: false }
    },
    steps: [{ click: "Recover", role: "tab" }, { pause: 300 }],
  }),

  // --- the vault list ----------------------------------------------------------------
  d({ id: "desktop-vault-filled", screen: "Vault", state: "Filled", brief_row: "Vault", notes: "All items, grouped by kind, title A to Z (D93).", compact: true, steps: open }),
  d({ id: "desktop-vault-loading", screen: "Vault", state: "Loading", brief_row: "Vault", notes: "The list's rows have not arrived: skeleton rows.", answers: () => ({ list: () => forever() }), steps: [{ click: "Unlock" }, { wait: /^All items/ }] }),
  d({
    id: "desktop-vault-error",
    screen: "Vault",
    state: "Error",
    brief_row: "Vault",
    notes: "The list failed to load; the core's words in the list.",
    answers: () => ({ list: () => fail("refused", "The vault's index on this device is damaged. Lockdown rebuilt it from the history; reopen the vault to see it.") }),
    steps: open,
  }),
  d({ id: "desktop-vault-empty", screen: "Vault", state: "Empty", brief_row: "Vault", notes: "Nothing saved yet: import or add.", setup: only(() => false), steps: open }),
  d({ id: "desktop-vault-search", screen: "Vault", state: "Search", brief_row: "Vault", notes: "A search narrowing the list.", steps: [...open, { fill: "Search", value: "blue" }] }),
  d({ id: "desktop-vault-search-none", screen: "Vault", state: "Search, no match", brief_row: "Vault", notes: "A search with no match.", steps: [...open, { fill: "Search", value: "zebra" }] }),
  d({ id: "desktop-vault-sort-menu", screen: "Vault", state: "Sort menu", brief_row: "Vault", notes: "Sort by title, recently changed or used; group by kind or none.", steps: [...open, { click: "Sort and group" }, { wait: "Sort by" }] }),
  d({ id: "desktop-vault-ungrouped", screen: "Vault", state: "No groups, recently changed", brief_row: "Vault", notes: "One list, newest change first.", steps: [...open, { click: "Sort and group" }, { click: "Recently changed", role: "menuitemradio" }, { key: "Escape" }, { click: "No groups" }] }),
  d({ id: "desktop-vault-new-item-menu", screen: "Vault", state: "New item menu", brief_row: "Item edit", notes: "Every creatable kind and variant from the core's templates, and the person's own.", steps: [...open, { click: /^New item/ }, { pause: 300 }] }),
  d({ id: "desktop-vault-row-menu", screen: "Vault", state: "Row menu", brief_row: "Vault", notes: "A row's own menu: favourite, folder, tags, move, archive.", steps: [...open, { click: "More for Bluefin Bank" }, { pause: 300 }] }),
  d({ id: "desktop-vault-duplicates", screen: "Vault", state: "Find duplicates", brief_row: "Vault", notes: "Duplicates review: same password, same network.", steps: [...open, { click: "Find duplicates" }, DIALOG] }),
  d({ id: "desktop-vault-organize", screen: "Vault", state: "Folder and tags", brief_row: "Vault", notes: "The organize prompt from a row's menu.", steps: [...open, { click: "More for Bluefin Bank" }, { click: /Folder and tags/ }, DIALOG] }),

  // --- smart views and scopes ---------------------------------------------------------------
  d({ id: "desktop-view-favourites", screen: "Smart views", state: "Favourites", brief_row: "Vault", notes: "Starred items.", steps: [...open, side("Favourites")] }),
  d({ id: "desktop-view-passkeys", screen: "Smart views", state: "Passkeys", brief_row: "Vault", notes: "Logins with passkeys and passkeys on their own.", steps: [...open, side("Passkeys")] }),
  d({ id: "desktop-view-wifi", screen: "Smart views", state: "Wi-Fi", brief_row: "Vault", notes: "Networks and the router.", steps: [...open, side("Wi-Fi")] }),
  d({ id: "desktop-view-contacts", screen: "Smart views", state: "Contacts", brief_row: null, notes: "Contacts (never used to fill).", steps: [...open, side("Contacts")] }),
  d({ id: "desktop-view-expiring", screen: "Smart views", state: "Expiring", brief_row: "Needs attention", notes: "Soonest first, in sections by how soon.", steps: [...open, side("Expiring")] }),
  d({ id: "desktop-view-attention", screen: "Needs attention", state: "Filled", brief_row: "Needs attention", notes: "Conflict copies, expiring items, the issuer-equals-login warning, a passkey hidden by its site, a sign-in left on a hotel TV.", steps: [...open, side("Needs attention")] }),
  d({ id: "desktop-view-attention-empty", screen: "Needs attention", state: "Empty", brief_row: "Needs attention", notes: "Nothing needs you.", setup: only((i) => ["Bluefin Bank", "Home Wi-Fi", "Spare keys and the alarm"].includes(i.title)), steps: [...open, side("Needs attention")] }),
  d({ id: "desktop-view-archived", screen: "Smart views", state: "Archived", brief_row: null, notes: "Archived items, out of the list and search.", steps: [...open, side("Archived")] }),
  d({ id: "desktop-view-deleted", screen: "Smart views", state: "Deleted", brief_row: "Item detail", notes: "The trash: readable through the grace period.", steps: [...open, side("Deleted")] }),
  d({ id: "desktop-view-deleted-empty", screen: "Smart views", state: "Deleted, empty", brief_row: "Item detail", notes: "The trash is empty.", setup: only((i) => i.state !== "trashed"), steps: [...open, side("Deleted")] }),
  d({ id: "desktop-scope-vault-family", widths: WIDE, screen: "Vaults and folders", state: "Family vault", brief_row: "Vault", notes: "The list narrowed to the shared Family vault.", steps: [...open, side("Family")] }),
  d({ id: "desktop-scope-folder-finance", widths: WIDE, screen: "Vaults and folders", state: "Finance folder", brief_row: "Vault", notes: "The list narrowed to a folder.", steps: [...open, side("Finance")] }),

  // --- item detail ---------------------------------------------------------------------------
  d({ id: "desktop-item-login-full", screen: "Item detail", state: "Login: password, passkey and code", brief_row: "Item detail", notes: "One account, every way in: the methods block first, then the websites.", steps: [...open, ...item("Northwind Mail")] }),
  d({ id: "desktop-item-login-passkey-only", screen: "Item detail", state: "Login: passkeys only, one hidden by its site", brief_row: "Item detail", notes: "No password; a passkey the site no longer knows, hidden with Undo.", steps: [...open, ...item("Pixelforge")] }),
  d({ id: "desktop-item-login-many-sites", screen: "Item detail", state: "Login: three websites, breached", brief_row: "Item detail", notes: "Several websites with their match rules; the item's findings.", steps: [...open, ...item("Parcelpost")] }),
  d({ id: "desktop-item-login-away", screen: "Item detail", state: "Login: signed in elsewhere", brief_row: "Item detail", notes: "A sign-in left on a hotel TV, due to be signed out (D237).", steps: [...open, ...item("Meadowlark Streaming")] }),
  d({ id: "desktop-item-conflict", screen: "Item detail", state: "Conflict copy", brief_row: "Needs attention", notes: "Two devices changed it at once: keep this copy or the other (D58).", steps: [...open, ...item("Tern Airlines (conflict copy from Sam's phone)")] }),
  d({ id: "desktop-item-conflict-confirm", screen: "Item detail", state: "Conflict: keep this copy?", brief_row: "Needs attention", notes: "The confirm before the other copy goes to the trash.", steps: [...open, ...item("Tern Airlines (conflict copy from Sam's phone)"), { click: "Keep this copy" }, DIALOG] }),
  d({ id: "desktop-item-stress", screen: "Item detail", state: "Long title and values", brief_row: "Item detail", notes: "Stress: a very long title, username, address and note.", steps: [...open, { click: /^Riverside Community/, within: "main" }, { wait: /Riverside Community/, role: "heading" }] }),
  d({ id: "desktop-item-shared-view", screen: "Item detail", state: "Shared with you (view)", brief_row: "Item detail", notes: "An item someone shared with view rights: nothing offered that the right doesn't allow.", steps: [...open, ...item("Kids' school portal")] }),
  d({ id: "desktop-item-code-steam", screen: "Item detail", state: "Code: Steam Guard", brief_row: "Codes view", notes: "A five-character Steam Guard code, full width, with Show large type.", steps: [...open, ...item("Kestrel Arcade · Steam Guard")] }),
  d({ id: "desktop-item-code-hotp", screen: "Item detail", state: "Code: counter (HOTP)", brief_row: "Codes view", notes: "A counter-based code with Next code.", steps: [...open, ...item("Office VPN token")] }),
  d({ id: "desktop-item-code-issuer-warning", screen: "Item detail", state: "Code: protects this account itself", brief_row: "Needs attention", notes: "The code's issuer is the Lockdown server you sync with.", steps: [...open, ...item("Lockdown server")] }),
  d({ id: "desktop-item-card", screen: "Item detail", state: "Card", brief_row: "Item detail", notes: "Number in groups of four; security code and PIN masked; asks before the code; expiring.", steps: [...open, ...item("Bluefin Visa")] }),
  d({ id: "desktop-item-bank", screen: "Item detail", state: "Bank account", brief_row: "Item detail", notes: "Account number and IBAN masked.", steps: [...open, ...item("Bluefin checking")] }),
  d({ id: "desktop-item-wifi", screen: "Item detail", state: "Wi-Fi", brief_row: "Item detail", notes: "Network, password with Show large type, and the guest card.", steps: [...open, ...item("Home Wi-Fi")] }),
  d({ id: "desktop-item-router", screen: "Item detail", state: "Router", brief_row: "Item detail", notes: "The router variant: the network and the admin sign-in.", steps: [...open, ...item("Home router")] }),
  d({ id: "desktop-item-note", screen: "Item detail", state: "Note", brief_row: "Item detail", notes: "A Markdown body.", steps: [...open, ...item("Spare keys and the alarm")] }),
  d({ id: "desktop-item-identity", screen: "Item detail", state: "Identity", brief_row: "Item detail", notes: "Name, email, phone and address for forms.", steps: [...open, ...item("Sam Rivera")] }),
  d({ id: "desktop-item-passport", screen: "Item detail", state: "Passport, expiring", brief_row: "Item detail", notes: "A passport expiring within 90 days, with its photo page attached.", steps: [...open, ...item("Passport")] }),
  d({ id: "desktop-item-contact", screen: "Item detail", state: "Contact", brief_row: null, notes: "A contact (never filled).", steps: [...open, side("Contacts"), ...item("Alex Rivera")] }),
  d({ id: "desktop-item-insurance", screen: "Item detail", state: "Document: insurance", brief_row: "Item detail", notes: "An insurance policy with two files.", steps: [...open, ...item("Home insurance")] }),
  d({ id: "desktop-item-recovery-codes", screen: "Item detail", state: "Crown jewel: recovery codes", brief_row: "Item detail", notes: "Recovery codes, a crown jewel by default.", steps: [...open, ...item("Northwind Mail recovery codes")] }),
  d({ id: "desktop-item-ssh", screen: "Item detail", state: "SSH key", brief_row: "Item detail", notes: "Public parts in plain text, private key masked, allowed hosts.", steps: [...open, ...item("Studio deploy key")] }),
  d({ id: "desktop-item-pgp", screen: "Item detail", state: "PGP key", brief_row: "Item detail", notes: "The PGP variant of an SSH key item.", steps: [...open, ...item("Personal signing key")] }),
  d({ id: "desktop-item-api", screen: "Item detail", state: "API credential, expiring", brief_row: "Item detail", notes: "A token expiring in nine days.", steps: [...open, ...item("Cobalt Cloud API token")] }),
  d({ id: "desktop-item-wallet", screen: "Item detail", state: "Crown jewel: crypto wallet", brief_row: "Item detail", notes: "The whole detail under the crown-jewel treatment.", steps: [...open, ...item("Cold wallet")] }),
  d({ id: "desktop-item-server", screen: "Item detail", state: "Server", brief_row: "Item detail", notes: "A server's host, port and sign-in.", steps: [...open, ...item("Build server")] }),
  d({ id: "desktop-item-database", screen: "Item detail", state: "Database", brief_row: "Item detail", notes: "A database's connection.", steps: [...open, ...item("Analytics database")] }),
  d({ id: "desktop-item-licence", screen: "Item detail", state: "Software licence", brief_row: "Item detail", notes: "A licence key, masked.", steps: [...open, ...item("Lumen Photo Editor")] }),
  d({ id: "desktop-item-membership", screen: "Item detail", state: "Membership", brief_row: "Item detail", notes: "A membership with its number and PIN.", steps: [...open, ...item("Trailhead Climbing Gym")] }),
  d({ id: "desktop-item-custom", screen: "Item detail", state: "Custom item", brief_row: "Item detail", notes: "Fields of the person's own.", steps: [...open, ...item("Bike lock")] }),
  d({ id: "desktop-item-passkey", screen: "Item detail", state: "Passkey on its own", brief_row: "Item detail", notes: "A passkey made by a site, view only.", steps: [...open, ...item("Lumen Photo forum")] }),
  d({ id: "desktop-item-trashed", screen: "Item detail", state: "In the trash", brief_row: "Item detail", notes: "Restore or delete now.", steps: [...open, side("Deleted"), ...item("Old gym account")] }),
  d({ id: "desktop-item-loading", screen: "Item detail", state: "Loading", brief_row: "Item detail", notes: "The item has not arrived yet.", answers: () => ({ show: () => forever() }), steps: [...open, { click: starts("Northwind Mail"), within: "main" }, { pause: 300 }] }),
  d({
    id: "desktop-item-error",
    screen: "Item detail",
    state: "Error",
    brief_row: "Item detail",
    notes: "A field failed to decrypt: the item's id is shown, never a guess.",
    answers: () => ({ show: () => fail("refused", "Its newest version couldn't be decrypted on this device; the history is intact.") }),
    steps: [...open, { click: starts("Northwind Mail"), within: "main" }, { wait: /Couldn't open this item/ }],
  }),
  d({ id: "desktop-item-trash-confirm", screen: "Item detail", state: "Move to the trash?", brief_row: "Item detail", notes: "The confirm before Trash.", steps: [...open, ...item("Quill Notes"), { click: "Trash", role: "button" }, DIALOG] }),
  d({ id: "desktop-item-purge-confirm", screen: "Item detail", state: "Delete for good?", brief_row: "Item detail", notes: "Deleting a trashed item now.", steps: [...open, side("Deleted"), ...item("Old gym account"), { click: "Delete now" }, DIALOG] }),
  d({
    id: "desktop-item-wallet-gate",
    screen: "Item detail",
    state: "Crown jewel: password asked",
    brief_row: "Item detail",
    notes: "Revealing a crown jewel's field asks for the full account password (D98, D101).",
    setup: (w) => {
      w.settings = { ...w.settings, reveal: "toggle" }
    },
    steps: [...open, ...item("Cold wallet"), { click: /^(Show|Reveal) Recovery phrase/ }, DIALOG],
  }),
  d({
    id: "desktop-item-wallet-revealed",
    screen: "Item detail",
    state: "Crown jewel: revealed",
    brief_row: "Item detail",
    notes: "The recovery phrase shown after the password, masking itself after 30 s.",
    setup: (w) => {
      w.settings = { ...w.settings, reveal: "toggle" }
    },
    steps: [...open, ...item("Cold wallet"), { click: /^(Show|Reveal) Recovery phrase/ }, { fill: /password/i, value: "correct horse" }, { click: /^(Confirm|Continue|Unlock)$/ }, { pause: 500 }],
  }),
  d({
    id: "desktop-item-revealed",
    screen: "Item detail",
    state: "Password revealed",
    brief_row: "Item detail",
    notes: "A login's password revealed (the toggle setting).",
    setup: (w) => {
      w.settings = { ...w.settings, reveal: "toggle" }
    },
    steps: [...open, ...item("Tern Airlines"), { click: /^(Show|Reveal) Password/ }, { pause: 400 }],
  }),

  // --- editor ----------------------------------------------------------------------------------
  d({ id: "desktop-editor-new-login", screen: "Item edit", state: "New login", brief_row: "Item edit", notes: "The one editor, for a new Login.", steps: [...open, { click: /^New item/ }, { click: /^Login/, role: "menuitem" }, DIALOG] }),
  d({ id: "desktop-editor-edit-login", screen: "Item edit", state: "Edit a login", brief_row: "Item edit", notes: "An existing Login; secrets kept unless retyped.", steps: [...open, ...item("Northwind Mail"), { click: "Edit", role: "button" }, DIALOG] }),
  d({ id: "desktop-editor-edit-card", screen: "Item edit", state: "Edit a card", brief_row: "Item edit", notes: "A card's editor.", steps: [...open, ...item("Bluefin Visa"), { click: "Edit", role: "button" }, DIALOG] }),

  // --- codes -----------------------------------------------------------------------------------
  d({ id: "desktop-codes-filled", screen: "Codes view", state: "Filled", brief_row: "Codes view", notes: "Every code with its countdown; TOTP, eight digits, Steam Guard and a counter.", steps: [...open, side("Codes")] }),
  d({ id: "desktop-codes-empty", screen: "Codes view", state: "Empty", brief_row: "Codes view", notes: "No codes yet.", setup: only((i) => !Object.keys(i.fields).some((k) => k.startsWith("code."))), steps: [...open, side("Codes")] }),
  d({ id: "desktop-codes-add", screen: "Codes view", state: "Add a code", brief_row: "Codes view", notes: "Add a code from its otpauth:// link, inside a Login or alone.", steps: [...open, side("Codes"), { click: /^Add (a )?code/ }, DIALOG] }),
  d({ id: "desktop-codes-scan", screen: "Codes view", state: "Scan", brief_row: "Approvals", notes: "Approve a TV or console sign-in: scan the code on the screen in front of you.", steps: [...open, side("Codes"), { click: "Sign in a TV" }, DIALOG] }),

  // --- screens -----------------------------------------------------------------------------------
  d({ id: "desktop-security-filled", screen: "Security", state: "Findings in three groups", brief_row: null, notes: "Fix now, fix soon, worth doing; the breach check and account alerts.", steps: [...open, side("Security")] }),
  d({ id: "desktop-security-empty", screen: "Security", state: "Nothing to fix", brief_row: null, notes: "No findings.", answers: () => ({ security_report: () => ({ findings: [], dismissed: [], counts: { urgent: 0, important: 0, suggested: 0 }, passwords: 19, breach: { enabled: true, source: "Pwned Passwords (k-anonymity range API)", checked_at: Math.floor(Date.now() / 1000) - 600, checked: 19, unchecked: 0, message: null }, alerts: { account: true, checked_at: Math.floor(Date.now() / 1000) - 600, message: null }, directory: { source: "2fa.directory", licence: "MIT", date: "2026-10-01", sites: 3412 }, generated_at: Math.floor(Date.now() / 1000) }) }), steps: [...open, side("Security")] }),
  d({ id: "desktop-sharing", screen: "Sharing", state: "Overview", brief_row: null, notes: "Contacts with fingerprints, the Family household, links, requests, emergency access.", steps: [...open, side("Sharing")] }),
  d({ id: "desktop-devices", screen: "Devices", state: "Filled, one unknown", brief_row: "Devices", notes: "Five devices, one nobody recognises (linux-a7f3), one revoked; live sessions.", steps: [...open, side("Devices")] }),
  d({ id: "desktop-devices-only-this", screen: "Devices", state: "Only this device", brief_row: "Devices", notes: "Empty: a nudge to add a second device.", answers: (w) => ({
      device_list: () => [{ id: "this", name: "Sam's MacBook Pro", platform: "macos", enrolled_at: Math.floor(Date.now() / 1000) - 86400 * 3, revoked_at: null, this_device: true }],
      session_list: () => [],
      status: () => ({ ...status(w), devices: 1, devices_revoked: 0 }),
    }), steps: [...open, side("Devices")] }),
  d({ id: "desktop-devices-revoke", screen: "Devices", state: "Revoke?", brief_row: "Devices", notes: "The confirm before a device is revoked.", steps: [...open, side("Devices"), { click: "Revoke", role: "button", nth: 3 }, DIALOG] }),
  d({ id: "desktop-recovery", screen: "Recovery", state: "Kit drilled", brief_row: "Recovery", notes: "Kit status, drill, emergency contact, the emergency sheet.", steps: [...open, side("Recovery")] }),
  d({ id: "desktop-status", screen: "Status", state: "Proofs", brief_row: "Status", notes: "The proofs, account and sync, this device.", steps: [...open, side("Status")] }),
  d({ id: "desktop-status-checked", screen: "Status", state: "Check now: passed", brief_row: "Status", notes: "Check now re-ran the audit; each line in words.", steps: [...open, side("Status"), { click: /^Check now/ }, { wait: /signed by one of your devices/ }] }),
  d({
    id: "desktop-status-check-failed",
    screen: "Status",
    state: "Check now: failed",
    brief_row: "Status",
    notes: "A check failed, in plain words.",
    answers: () => ({ audit: () => fail("refused", "The server's public log doesn't include your account's newest key. Sync again; if it still differs, see Help → Key transparency.") }),
    steps: [...open, side("Status"), { click: /^Check now/ }, { wait: /public log doesn't include/ }],
  }),
  d({ id: "desktop-settings", screen: "Settings", state: "All sections", brief_row: "Settings", notes: "Appearance, lock, clipboard, approvals, browsers, import and export, backups, security, aliases, quick access.", steps: [...open, side("Settings")] }),

  // --- shell ---------------------------------------------------------------------------------------
  d({ id: "desktop-palette", screen: "Quick-access palette", state: "Open", brief_row: "Quick-access palette", notes: "⌘K / Ctrl+K: recent items first.", steps: [...open, { click: "Quick access" }, DIALOG] }),
  d({ id: "desktop-palette-search", screen: "Quick-access palette", state: "Search", brief_row: "Quick-access palette", notes: "A search: codes shown in place, crown jewels by title only.", steps: [...open, { click: "Quick access" }, DIALOG, { fill: /Search/i, value: "north" }, { pause: 300 }] }),
  d({
    id: "desktop-idle-toast",
    screen: "Shell",
    state: "Idle lock warning",
    brief_row: null,
    notes: "Ten seconds before an idle lock: one toast with Stay unlocked (D107).",
    steps: [
      ...open,
      {
        run: () => {
          // The core's idle timer is near: the next poll says so.
        },
      },
      { pause: 3400 },
    ],
    answers: (w) => ({ lock_state: () => ({ ...w.lock, idle_secs: 593, idle_timeout_secs: 600 }) }),
  }),
  d({ id: "desktop-reminder-away", screen: "Signed in elsewhere", state: "Due: toast", brief_row: "Signed in elsewhere", notes: "A sign-in left on a hotel TV is due to be signed out: the toast after unlock (D237).", answers: () => ({ away_reminders: as("away_reminders_due") }), steps: [...open, { pause: 1500 }] }),
  d({ id: "desktop-reminder-expiry", screen: "Smart views", state: "Expiring: toast", brief_row: "Needs attention", notes: "Items expiring soon: the reminder toast after unlock, with Show.", answers: () => ({ expiry_reminders: as("expiry_reminders_due") }), steps: [...open, { pause: 1500 }] }),
  d({ id: "desktop-quick-window-locked", screen: "Quick-access window", state: "Locked", brief_row: "Quick-access palette", notes: "The system-wide shortcut's window while the vault is locked.", mount: () => <QuickWindow />, steps: [{ wait: "Lockdown is locked." }] }),
  d({
    id: "desktop-quick-window-open",
    screen: "Quick-access window",
    state: "Open",
    brief_row: "Quick-access palette",
    notes: "The system-wide shortcut's window: only the palette.",
    mount: () => <QuickWindow />,
    setup: (w) => {
      w.lock = { ...w.lock, locked: false }
    },
    steps: [{ pause: 600 }],
  }),
]
