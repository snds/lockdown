// The vault lane's states: the list and its views, scopes and trees, an
// item's detail and its zones, organizing, the palette and the quick window,
// search, and Health (the Security screen and what it opens), on the desktop
// and in the web vault.

import { QuickWindow } from "@/app/quick-window"

import { SHAPE, longVault, withShape } from "../fixture/shape"
import { ago } from "../fixture/time"
import { as, fail, forever } from "../host/core"
import type { StateDef } from "../harness/types"
import { d, only, w } from "./defs"
import { open, webOpen } from "./steps/screens"
import { DIALOG, MENU, answerGate, go, openSwitcher, starts } from "./steps/shell"
import { itemAction, openItem, reveal } from "./steps/vault"

export const VAULT_STATES: StateDef[] = [
  d({ id: "desktop-vault-filled", screen: "Vault", state: "Filled", brief_row: "Vault", notes: "All items, grouped by kind, title A to Z (D93).", compact: true, steps: open }),
  d({ id: "desktop-vault-loading", screen: "Vault", state: "Loading", brief_row: "Vault", notes: "The list's rows have not arrived: skeleton rows.", answers: () => ({ list: () => forever() }), steps: open }),
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
  d({ id: "desktop-vault-long", screen: "Vault", state: "A long list", brief_row: "Vault", notes: "A vault of 300 rows: how the list groups, scrolls and keeps its place.", setup: longVault, steps: open }),
  d({ id: "desktop-vault-search", screen: "Vault", state: "Search", brief_row: "Vault", notes: "A search narrowing the list.", steps: [...open, { fill: "Search", value: "blue" }] }),
  d({ id: "desktop-vault-search-none", screen: "Vault", state: "Search, no match", brief_row: "Vault", notes: "A search with no match.", steps: [...open, { fill: "Search", value: "zebra" }] }),
  d({ id: "desktop-vault-sort-menu", screen: "Vault", state: "Sort menu", brief_row: "Vault", notes: "Sort by title, recently changed or used; group by kind or none.", steps: [...open, { click: "Sort and group" }, { wait: "Sort by" }] }),
  d({ id: "desktop-vault-ungrouped", screen: "Vault", state: "No groups, recently changed", brief_row: "Vault", notes: "One list, newest change first.", steps: [...open, { click: "Sort and group" }, { click: "Recently changed", role: "menuitemradio" }, { key: "Escape" }, { click: "No groups" }] }),
  d({ id: "desktop-vault-row-menu", screen: "Vault", state: "Row menu", brief_row: "Vault", notes: "A row's own menu: favourite, folder, tags, move, archive.", steps: [...open, { click: "More for Bluefin Bank" }, { pause: 300 }] }),
  d({ id: "desktop-vault-duplicates", screen: "Vault", state: "Find duplicates", brief_row: "Vault", notes: "Duplicates review: same password, same network.", steps: [...open, { click: "Find duplicates" }, DIALOG] }),
  d({ id: "desktop-vault-organize", screen: "Vault", state: "Folder and tags", brief_row: "Vault", notes: "The organize prompt from a row's menu.", steps: [...open, { click: "More for Bluefin Bank" }, { click: /Folder and tags/ }, DIALOG] }),
  d({ id: "desktop-view-favourites", screen: "Smart views", state: "Favourites", brief_row: "Vault", notes: "Starred items.", steps: [...open, ...go("favourites")] }),
  d({ id: "desktop-view-passkeys", screen: "Smart views", state: "Passkeys", brief_row: "Vault", notes: "Logins with passkeys and passkeys on their own.", steps: [...open, ...go("passkeys")] }),
  d({ id: "desktop-view-wifi", screen: "Smart views", state: "Wi-Fi", brief_row: "Vault", notes: "Networks and the router.", steps: [...open, ...go("wi-fi")] }),
  d({ id: "desktop-view-contacts", screen: "Smart views", state: "Contacts", brief_row: null, notes: "Contacts (never used to fill).", steps: [...open, ...go("contacts")] }),
  d({ id: "desktop-view-expiring", screen: "Smart views", state: "Expiring", brief_row: "Needs attention", notes: "Soonest first, in sections by how soon.", steps: [...open, ...go("expiring")] }),
  d({ id: "desktop-view-attention", screen: "Needs attention", state: "Filled", brief_row: "Needs attention", notes: "Conflict copies, expiring items, the issuer-equals-login warning, a passkey hidden by its site, a sign-in left on a hotel TV.", steps: [...open, ...go("attention")] }),
  d({ id: "desktop-view-attention-empty", screen: "Needs attention", state: "Empty", brief_row: "Needs attention", notes: "Nothing needs you.", setup: only((i) => ["Bluefin Bank", "Home Wi-Fi", "Spare keys and the alarm"].includes(i.title)), steps: [...open, ...go("attention")] }),
  d({ id: "desktop-view-archived", screen: "Smart views", state: "Archived", brief_row: null, notes: "Archived items, out of the list and search.", steps: [...open, ...go("archived")] }),
  d({ id: "desktop-view-deleted", screen: "Smart views", state: "Deleted", brief_row: "Item detail", notes: "The trash: readable through the grace period.", steps: [...open, ...go("deleted")] }),
  d({ id: "desktop-view-deleted-empty", screen: "Smart views", state: "Deleted, empty", brief_row: "Item detail", notes: "The trash is empty.", setup: only((i) => i.state !== "trashed"), steps: [...open, ...go("deleted")] }),
  d({ id: "desktop-scope-vault-family", screen: "Vaults and folders", state: "Family vault", brief_row: "Vault", notes: "The list narrowed to the shared Family vault.", steps: [...open, ...go("vault-family")] }),
  d({ id: "desktop-scope-folder-finance", screen: "Vaults and folders", state: "Finance folder", brief_row: "Vault", notes: "The list narrowed to a folder.", steps: [...open, ...go("folder-finance")] }),
  d({ id: "desktop-item-login-full", screen: "Item detail", state: "Login: password, passkey and code", brief_row: "Item detail", notes: "One account, every way in: the methods block first, then the websites.", steps: [...open, ...openItem("Northwind Mail")] }),
  d({ id: "desktop-item-login-methods", screen: "Item detail", state: "Login: password, passkey, a five-character code and a protected note", brief_row: "Item detail", notes: "The design's login: every method, a five-character code, and a recovery note kept masked.", setup: withShape, steps: [...open, ...openItem(SHAPE.login)] }),
  d({ id: "desktop-item-login-passkey", screen: "Item detail", state: "Login: a passkey and nothing else", brief_row: "Item detail", notes: "The design's passkey login: no password to show, copy or check.", setup: withShape, steps: [...open, ...openItem(SHAPE.passkeyLogin)] }),
  d({ id: "desktop-item-note-network", screen: "Item detail", state: "Note with a network and a password", brief_row: "Item detail", notes: "The design's secure note: a network's name and its password as fields of the note, the password masked.", setup: withShape, steps: [...open, ...openItem(SHAPE.note)] }),
  d({ id: "desktop-item-login-passkey-only", screen: "Item detail", state: "Login: passkeys only, one hidden by its site", brief_row: "Item detail", notes: "No password; a passkey the site no longer knows, hidden with Undo.", steps: [...open, ...openItem("Pixelforge")] }),
  d({ id: "desktop-item-login-many-sites", screen: "Item detail", state: "Login: three websites, breached", brief_row: "Item detail", notes: "Several websites with their match rules; the item's findings.", steps: [...open, ...openItem("Parcelpost")] }),
  d({ id: "desktop-item-login-away", screen: "Item detail", state: "Login: signed in elsewhere", brief_row: "Item detail", notes: "A sign-in left on a hotel TV, due to be signed out (D237).", steps: [...open, ...openItem("Meadowlark Streaming")] }),
  d({ id: "desktop-item-conflict", screen: "Item detail", state: "Conflict copy", brief_row: "Needs attention", notes: "Two devices changed it at once: keep this copy or the other (D58).", steps: [...open, ...openItem("Tern Airlines (conflict copy from Sam's phone)")] }),
  d({ id: "desktop-item-conflict-confirm", screen: "Item detail", state: "Conflict: keep this copy?", brief_row: "Needs attention", notes: "The confirm before the other copy goes to the trash.", steps: [...open, ...openItem("Tern Airlines (conflict copy from Sam's phone)"), { click: "Keep this copy" }, DIALOG] }),
  d({ id: "desktop-item-stress", screen: "Item detail", state: "Long title and values", brief_row: "Item detail", notes: "Stress: a very long title, username, address and note.", steps: [...open, { click: /^Riverside Community/, within: "main" }, { wait: /Riverside Community/, role: "heading" }] }),
  d({ id: "desktop-item-shared-view", screen: "Item detail", state: "Shared with you (view)", brief_row: "Item detail", notes: "An item someone shared with view rights: nothing offered that the right doesn't allow.", steps: [...open, ...openItem("Kids' school portal")] }),
  d({ id: "desktop-item-code-steam", screen: "Item detail", state: "Code: Steam Guard", brief_row: "Codes view", notes: "A five-character Steam Guard code, full width, with Show large type.", steps: [...open, ...openItem("Kestrel Arcade · Steam Guard")] }),
  d({ id: "desktop-item-code-hotp", screen: "Item detail", state: "Code: counter (HOTP)", brief_row: "Codes view", notes: "A counter-based code with Next code.", steps: [...open, ...openItem("Office VPN token")] }),
  d({ id: "desktop-item-code-issuer-warning", screen: "Item detail", state: "Code: protects this account itself", brief_row: "Needs attention", notes: "The code's issuer is the Lockdown server you sync with.", steps: [...open, ...openItem("Lockdown server")] }),
  d({ id: "desktop-item-card", screen: "Item detail", state: "Card", brief_row: "Item detail", notes: "Number in groups of four; security code and PIN masked; asks before the code; expiring.", steps: [...open, ...openItem("Bluefin Visa")] }),
  d({ id: "desktop-item-bank", screen: "Item detail", state: "Bank account", brief_row: "Item detail", notes: "Account number and IBAN masked.", steps: [...open, ...openItem("Bluefin checking")] }),
  d({ id: "desktop-item-wifi", screen: "Item detail", state: "Wi-Fi", brief_row: "Item detail", notes: "Network, password with Show large type, and the guest card.", steps: [...open, ...openItem("Home Wi-Fi")] }),
  d({ id: "desktop-item-router", screen: "Item detail", state: "Router", brief_row: "Item detail", notes: "The router variant: the network and the admin sign-in.", steps: [...open, ...openItem("Home router")] }),
  d({ id: "desktop-item-note", screen: "Item detail", state: "Note", brief_row: "Item detail", notes: "A Markdown body.", steps: [...open, ...openItem("Spare keys and the alarm")] }),
  d({ id: "desktop-item-identity", screen: "Item detail", state: "Identity", brief_row: "Item detail", notes: "Name, email, phone and address for forms.", steps: [...open, ...openItem("Sam Rivera")] }),
  d({ id: "desktop-item-passport", screen: "Item detail", state: "Passport, expiring", brief_row: "Item detail", notes: "A passport expiring within 90 days, with its photo page attached.", steps: [...open, ...openItem("Passport")] }),
  d({ id: "desktop-item-contact", screen: "Item detail", state: "Contact", brief_row: null, notes: "A contact (never filled).", steps: [...open, ...go("contacts"), ...openItem("Alex Rivera")] }),
  d({ id: "desktop-item-insurance", screen: "Item detail", state: "Document: insurance", brief_row: "Item detail", notes: "An insurance policy with two files.", steps: [...open, ...openItem("Home insurance")] }),
  d({ id: "desktop-item-recovery-codes", screen: "Item detail", state: "Crown jewel: recovery codes", brief_row: "Item detail", notes: "Recovery codes, a crown jewel by default.", steps: [...open, ...openItem("Northwind Mail recovery codes")] }),
  d({ id: "desktop-item-ssh", screen: "Item detail", state: "SSH key", brief_row: "Item detail", notes: "Public parts in plain text, private key masked, allowed hosts.", steps: [...open, ...openItem("Studio deploy key")] }),
  d({ id: "desktop-item-pgp", screen: "Item detail", state: "PGP key", brief_row: "Item detail", notes: "The PGP variant of an SSH key item.", steps: [...open, ...openItem("Personal signing key")] }),
  d({ id: "desktop-item-api", screen: "Item detail", state: "API credential, expiring", brief_row: "Item detail", notes: "A token expiring in nine days.", steps: [...open, ...openItem("Cobalt Cloud API token")] }),
  d({ id: "desktop-item-wallet", screen: "Item detail", state: "Crown jewel: crypto wallet", brief_row: "Item detail", notes: "The whole detail under the crown-jewel treatment.", steps: [...open, ...openItem("Cold wallet")] }),
  d({ id: "desktop-item-server", screen: "Item detail", state: "Server", brief_row: "Item detail", notes: "A server's host, port and sign-in.", steps: [...open, ...openItem("Build server")] }),
  d({ id: "desktop-item-database", screen: "Item detail", state: "Database", brief_row: "Item detail", notes: "A database's connection.", steps: [...open, ...openItem("Analytics database")] }),
  d({ id: "desktop-item-licence", screen: "Item detail", state: "Software licence", brief_row: "Item detail", notes: "A licence key, masked.", steps: [...open, ...openItem("Lumen Photo Editor")] }),
  d({ id: "desktop-item-membership", screen: "Item detail", state: "Membership", brief_row: "Item detail", notes: "A membership with its number and PIN.", steps: [...open, ...openItem("Trailhead Climbing Gym")] }),
  d({ id: "desktop-item-custom", screen: "Item detail", state: "Custom item", brief_row: "Item detail", notes: "Fields of the person's own.", steps: [...open, ...openItem("Bike lock")] }),
  d({ id: "desktop-item-passkey", screen: "Item detail", state: "Passkey on its own", brief_row: "Item detail", notes: "A passkey made by a site, view only.", steps: [...open, ...openItem("Lumen Photo forum")] }),
  d({ id: "desktop-item-trashed", screen: "Item detail", state: "In the trash", brief_row: "Item detail", notes: "Restore or delete now.", steps: [...open, ...go("deleted"), ...openItem("Old gym account")] }),
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
  d({ id: "desktop-item-trash-confirm", screen: "Item detail", state: "Move to the trash?", brief_row: "Item detail", notes: "The confirm before Trash.", steps: [...open, ...openItem("Quill Notes"), itemAction("Trash"), DIALOG] }),
  d({ id: "desktop-item-purge-confirm", screen: "Item detail", state: "Delete for good?", brief_row: "Item detail", notes: "Deleting a trashed item now.", steps: [...open, ...go("deleted"), ...openItem("Old gym account"), { click: "Delete now" }, DIALOG] }),
  d({
    id: "desktop-item-wallet-revealed",
    screen: "Item detail",
    state: "Crown jewel: revealed",
    brief_row: "Item detail",
    notes: "The recovery phrase shown after the password, masking itself after 30 s.",
    setup: (w) => {
      w.settings = { ...w.settings, reveal: "toggle" }
    },
    steps: [...open, ...openItem("Cold wallet"), reveal("Recovery phrase"), ...answerGate("correct horse"), { pause: 500 }],
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
    steps: [...open, ...openItem("Tern Airlines"), reveal("Password"), { pause: 400 }],
  }),
  d({ id: "desktop-security-filled", screen: "Security", state: "Findings in three groups", brief_row: null, notes: "Fix now, fix soon, worth doing; the breach check and account alerts.", steps: [...open, ...go("security")] }),
  d({ id: "desktop-security-empty", screen: "Security", state: "Nothing to fix", brief_row: null, notes: "No findings.", answers: () => ({ security_report: () => ({ findings: [], dismissed: [], counts: { urgent: 0, important: 0, suggested: 0 }, passwords: 19, breach: { enabled: true, source: "Pwned Passwords (k-anonymity range API)", checked_at: Math.floor(Date.now() / 1000) - 600, checked: 19, unchecked: 0, message: null }, alerts: { account: true, checked_at: Math.floor(Date.now() / 1000) - 600, message: null }, directory: { source: "2fa.directory", licence: "MIT", date: "2026-10-01", sites: 3412 }, generated_at: Math.floor(Date.now() / 1000) }) }), steps: [...open, ...go("security")] }),
  d({ id: "desktop-palette", screen: "Quick-access palette", state: "Open", brief_row: "Quick-access palette", notes: "⌘K / Ctrl+K: recent items first.", steps: [...open, { click: "Quick access" }, DIALOG] }),
  d({ id: "desktop-palette-search", screen: "Quick-access palette", state: "Search", brief_row: "Quick-access palette", notes: "A search: codes shown in place, crown jewels by title only.", steps: [...open, { click: "Quick access" }, DIALOG, { fill: /Search/i, value: "north" }, { pause: 300 }] }),
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
  d({ id: "desktop-nav-vault-menu", screen: "Vaults and tags trees", state: "Vault menu", brief_row: "Vaults and tags trees", notes: "A vault's menu: new folder, rename.", steps: [...open, ...openSwitcher(), { click: "Personal options" }, MENU] }),
  d({ id: "desktop-nav-new-vault", screen: "Vaults and tags trees", state: "New vault", brief_row: "Vaults and tags trees", notes: "Naming a new vault.", steps: [...open, ...openSwitcher(), { click: "New vault" }, DIALOG] }),
  d({ id: "desktop-nav-tag", screen: "Vaults and tags trees", state: "Narrowed to a tag", brief_row: "Vaults and tags trees", notes: "The list narrowed to #travel.", steps: [...open, ...go("tag-travel")] }),
  d({ id: "desktop-item-more-menu", screen: "Where it is, item menu", state: "Item menu", brief_row: "Where it is, item menu", notes: "History, archive, move, crown-jewel on or off.", steps: [...open, ...openItem("Bluefin Bank"), { click: "More for this item" }, MENU] }),
  d({ id: "desktop-item-history", screen: "Version history", state: "Versions", brief_row: "Version history", notes: "Every version, which device, which fields; hidden values stay hidden.", steps: [...open, ...openItem("Northwind Mail"), { click: "More for this item" }, { click: "History", role: "menuitem" }, DIALOG] }),
  d({ id: "desktop-item-jewel-on", screen: "Where it is, item menu", state: "Keep as a crown jewel?", brief_row: "Where it is, item menu", notes: "Moving an item into the crown-jewel class.", steps: [...open, ...openItem("Bluefin Bank"), { click: "More for this item" }, { click: "Keep as a crown jewel", role: "menuitem" }, DIALOG] }),
  d({ id: "desktop-item-lookalike", screen: "Lookalike warning", state: "Site imitates another", brief_row: "Lookalike warning", notes: "First in the detail: this login's site imitates the bank's.", steps: [...open, ...openItem("Bluefin Bank alerts")] }),
  d({ id: "desktop-item-wifi-card", screen: "Wi-Fi guest card", state: "Join the network", brief_row: "Wi-Fi guest card", notes: "A QR guests scan; hides after 60 s.", steps: [...open, ...openItem("Guest Wi-Fi"), { click: /Guest card/ }, DIALOG, { pause: 300 }] }),
  d({ id: "desktop-item-large-type", screen: "Large type", state: "Wi-Fi password", brief_row: "Large type", notes: "Read across a room; 120 s.", steps: [...open, ...openItem("Guest Wi-Fi"), { click: /Show large type/ }, DIALOG, { pause: 400 }] }),
  d({ id: "desktop-item-large-type-code", screen: "Large type", state: "A code", brief_row: "Large type", notes: "A code's current value in large type.", steps: [...open, ...openItem("Driftwood Forum"), { click: /Show large type/ }, DIALOG, { pause: 400 }] }),
  d({ id: "desktop-item-away-dialog", screen: "Signed in elsewhere", state: "Log a sign-in", brief_row: "Signed in elsewhere", notes: "Signing in on a TV: where, and when to remind (D237).", steps: [...open, ...openItem("Tern Airlines"), { click: /Signing in on a TV/ }, DIALOG] }),
  d({ id: "desktop-item-passkey-delete", screen: "Passkeys block", state: "Delete: the only way in", brief_row: "Passkeys block", notes: "Deleting the only passkey warns that the account would have no way in.", steps: [...open, ...openItem("Pixelforge"), { click: "Delete", role: "button" }, DIALOG] }),
  d({ id: "desktop-item-setup-guide", screen: "Set-up guide", state: "Two-step sign-in for a site", brief_row: "Set-up guide", notes: "What Harbor Health offers for two-step sign-in, and how (from its Security finding).", steps: [...open, ...go("security"), { click: /Set-up guide/ }, DIALOG] }),
  d({ id: "desktop-item-playbook", screen: "Playbook (\"What to do\")", state: "From the item", brief_row: "Playbook (\"What to do\")", notes: "Steps ending in how to confirm the fix.", steps: [...open, ...openItem("Parcelpost"), { click: "What to do" }, DIALOG] }),
  d({ id: "desktop-item-file-remove", screen: "Files", state: "Take a file off?", brief_row: "Files", notes: "Removing an attached file.", steps: [...open, ...openItem("Home insurance"), { click: "Remove policy-2026.pdf" }, DIALOG] }),
  d({ id: "desktop-item-fill-only", screen: "Item detail", state: "Shared fill-only", brief_row: "Item Share menu", notes: "An item shared fill-only: no copy, no reveal, no large type.", setup: (w) => {
    const it = w.items.find((i) => i.title === "Kids' school portal")!
    it.access = "fill-only"
  }, steps: [...open, ...openItem("Kids' school portal")] }),
  d({ id: "desktop-item-tier", screen: "Item detail", state: "Risk tier badge", brief_row: "Item detail", notes: "The RiskTierBadge seam: no item carries a tier in the vault model yet, so this one is given one by the fixture.", setup: (w) => {
    w.items.find((i) => i.title === "Cobalt Cloud console")!.tier = "official"
  }, steps: [...open, { click: starts("Cobalt Cloud console"), within: "main" }, { wait: /^Cobalt Cloud console/, role: "heading" }] }),
  d({ id: "desktop-security-checking", screen: "Security", state: "Checking", brief_row: "Security", notes: "Check now running.", answers: () => ({ security_check: () => forever() }), steps: [...open, ...go("security"), { wait: /Checking/ }] }),
  d({ id: "desktop-security-dismissed", screen: "Security", state: "Dismissed and snoozed", brief_row: "Security", notes: "The collapsed section opened.", steps: [...open, ...go("security"), { click: /dismissed and snoozed/ }] }),
  d({ id: "desktop-security-playbook", screen: "Playbook (\"What to do\")", state: "From the report", brief_row: "Playbook (\"What to do\")", notes: "What to do about a breached password.", steps: [...open, ...go("security"), { click: "What to do" }, DIALOG] }),
  d({ id: "desktop-security-dismiss-menu", screen: "Security", state: "Dismiss menu", brief_row: "Security", notes: "Dismiss or snooze a finding.", steps: [...open, ...go("security"), { click: /^Dismiss: /, nth: 3 }, MENU] }),
  d({ id: "desktop-security-incident", screen: "Something happened?", state: "In progress", brief_row: "Something happened?", notes: "A guided flow filled from the vault.", steps: [...open, ...go("security"), { click: "Carry on" }, DIALOG] }),
  d({ id: "desktop-security-pq-on", screen: "Security", state: "Quantum-safe signatures: on", brief_row: "Security", notes: "Every device reads the stronger signatures (docs/plan/30-pq-status-line.md).", answers: () => ({ pq_status: () => ({ state: "on", reason: null, headline: "Quantum-safe signatures: on", explanation: "Every change your devices make to your vaults is signed in a way that even a future quantum computer couldn't fake.", without_level_1: [], without_level_2: [] }) }), steps: [...open, ...go("security")] }),
  d({ id: "desktop-security-pq-unknown", screen: "Security", state: "Quantum-safe signatures: can't check", brief_row: "Security", notes: "The server didn't report the account's devices.", answers: () => ({ pq_status: () => ({ state: "unknown", reason: "silent", headline: "Quantum-safe signatures: can't check right now", explanation: "Your server didn't report your account's devices. It may be an older version.", without_level_1: [], without_level_2: [] }) }), steps: [...open, ...go("security")] }),
  d({ id: "desktop-security-error", screen: "Security", state: "Error", brief_row: "Security", notes: "The report could not be made.", answers: () => ({ security_report: () => fail("refused", "The security report needs the vault's index, which is still being rebuilt. Try again in a minute.") }), steps: [...open, ...go("security"), { pause: 600 }] }),
  d({ id: "desktop-palette-crown-jewel", screen: "Quick-access palette", state: "Crown jewel by title", brief_row: "Quick-access palette", notes: "Crown jewels are listed by title only; copying asks first.", steps: [...open, { click: "Quick access" }, DIALOG, { fill: /Search/i, value: "cold" }, { pause: 400 }] }),
  d({
    id: "desktop-item-passkey-upgraded",
    screen: "Item detail",
    state: "Passkey added by the site",
    brief_row: "Passkeys block",
    notes: "The login says who added its passkey, and when, until it is opened once (D210, D312 as changed by D375).",
    answers: () => ({
      passkeys: (a) => (as("passkeys")(a) as Array<Record<string, unknown>>).map((p, i) => (i === 0 ? { ...p, upgrade: { site: String(p.rp_id), at: ago(0.02) } } : p)),
    }),
    steps: [...open, ...openItem("Northwind Mail"), { wait: /^Added by / }],
  }),

  // --- the web vault -----------------------------------------------------------------
  w({ id: "web-vault", screen: "Vault", state: "Filled", brief_row: "Vault", notes: "The desktop's vault view in the web build: Links, People, Devices and Settings in the sidebar.", steps: webOpen }),
  w({ id: "web-item-login", screen: "Item detail", state: "Login", brief_row: "Item detail", notes: "A login in the web build: copies go through the page's clipboard.", steps: [...webOpen, ...openItem("Northwind Mail")] }),
  w({ id: "web-item-wallet", screen: "Item detail", state: "Crown jewel", brief_row: "Item detail", notes: "A crown jewel in the web build.", steps: [...webOpen, ...openItem("Cold wallet")] }),
  w({ id: "web-security", screen: "Security", state: "Quantum-safe line", brief_row: "Security", notes: "The web vault's Security screen: the quantum-safe line; password checks stay in the desktop app.", steps: [...webOpen, ...go("security")] }),
]
