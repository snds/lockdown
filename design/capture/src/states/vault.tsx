// The vault lane's states: the list and its views, scopes and trees, an
// item's detail and its zones, organizing, the palette and the quick window,
// search, and Health (the Security screen and what it opens), on the desktop
// and in the web vault.

import { QuickWindow } from "@/app/quick-window"

import { SHAPE, longVault, withShape } from "../fixture/shape"
import { ago } from "../fixture/time"
import { as, fail, forever } from "../host/core"
import { runSteps } from "../harness/driver"
import type { StateDef, Step } from "../harness/types"
import { d, only, w } from "./defs"
import { WIDTHS } from "../harness/types"
import { open, webOpen } from "./steps/screens"
import { DIALOG, MENU, answerGate, go, openSwitcher, scrollContent, starts } from "./steps/shell"
import { besideSidebar, bringIntoView, detailsRowAction, focusOrgRow, itemAction, openBranch, openDetails, openItem, openItemMenu, orgMenu, orgMenuRow, passkeyDetails, reveal, swipeRow } from "./steps/vault"

/** Search and Quick access by width class (brief X4): Quick access is not offered on a phone. */
const PHONE = WIDTHS.filter((x) => x < 600)
const WIDE = WIDTHS.filter((x) => x >= 1024)
const NOT_PHONE = WIDTHS.filter((x) => x >= 600)

/** Northwind Mail saved seven times, so History has earlier versions to show and restore (brief I6). */
const sevenVersions = (w: { items: Array<{ title: string; version: number }> }) => {
  w.items.find((i) => i.title === "Northwind Mail")!.version = 7
}

/** The click that opened something leaves its focus ring; a picture shows the screen at rest. */
const restFocus = (): Step => ({
  run: async () => {
    if (document.activeElement instanceof HTMLElement) document.activeElement.blur()
    await new Promise((r) => setTimeout(r, 150))
  },
})

/** The failed-reload state's switch: the list reads until a step turns it on. */
const reload = { broken: false }

export const VAULT_STATES: StateDef[] = [
  d({ id: "desktop-vault-filled", built: { area: "items", screen: "list", kind: "drawn" }, screen: "Vault", state: "Filled", brief_row: "Vault", notes: "All items, title A to Z: by letter on a desktop, one list on a phone (brief V1, D780). The compact density too.", compact: true, steps: open }),
  d({ id: "desktop-vault-loading", built: { area: "items", screen: "loading", kind: "drawn" }, screen: "Vault", state: "Loading", brief_row: "Vault", notes: "The list's rows have not arrived: skeleton rows and no count (States / 04).", answers: () => ({ list: () => forever() }), steps: open }),
  d({
    id: "desktop-vault-error",
    built: { area: "items", screen: "error", kind: "undrawn" },
    screen: "Vault",
    state: "Error",
    brief_row: "Vault",
    notes: "Nothing could load: a critical Empty State with the core's words and Try again.",
    answers: () => ({ list: () => fail("refused", "The vault's index on this device is damaged. Lockdown rebuilt it from the history; reopen the vault to see it.") }),
    steps: open,
  }),
  d({ id: "desktop-vault-empty", built: { area: "items", screen: "empty", kind: "undrawn" }, screen: "Vault", state: "Empty", brief_row: "Vault", notes: "Nothing saved yet: NEW ITEM opens the New item menu.", setup: only(() => false), steps: open }),
  d({
    id: "desktop-vault-reload-failed",
    built: { area: "items", screen: "failed-reload", kind: "undrawn" },
    screen: "Vault",
    state: "A reload failed",
    brief_row: "Vault",
    notes: "A change from a row's menu, and the list could not be read again: the rows that loaded stay, under a critical Banner with Try again (cross-app decisions §9).",
    answers: () => {
      reload.broken = false
      return { list: (a) => (reload.broken ? fail("refused", "The vault's index is busy with another window. Try again in a moment.") : as("list")(a)) }
    },
    steps: [...open, { run: () => void (reload.broken = true) }, { click: /^More for Alex Rivera/ }, { click: /^(Add to favorites|Favorite)$/, role: "menuitem" }, { wait: "Couldn't load your items." }, { run: () => (document.activeElement as HTMLElement | null)?.blur() }, { pause: 300 }],
  }),
  d({ id: "desktop-vault-long", built: { area: "items", screen: "long", kind: "undrawn" }, screen: "Vault", state: "A long list", brief_row: "Vault", notes: "A vault of 300 rows: only the rows in view are drawn (brief X6), the letter headers stay sticky.", setup: longVault, steps: open }),
  d({
    id: "desktop-vault-search",
    built: { area: "search", screen: "results", kind: "drawn" },
    screen: "Search",
    state: "Results",
    brief_row: "Vault",
    notes: "Mobile / 15: the words typed, the results grouped by vault with counts, the words marked (brief X4). The search button current, no tab active.",
    widths: PHONE,
    steps: [...open, ...go("search"), { fill: /^Search/, value: "key" }, { pause: 200 }],
  }),
  d({
    id: "desktop-vault-search-wide",
    built: { area: "search", screen: "results", kind: "undrawn" },
    screen: "Search",
    state: "Results beside the detail",
    brief_row: "Vault",
    notes: "Desktop / 11's frame with results: the field and the groups in the list pane, a result open beside it; SEARCH in the breadcrumb, NN RESULTS in the status line (brief X4).",
    widths: WIDE,
    steps: [...open, ...go("search"), { fill: /^Search/, value: "key" }, ...openItem("Studio deploy key"), { run: () => (document.activeElement as HTMLElement | null)?.blur() }, { pause: 300 }],
  }),
  d({
    id: "desktop-vault-search-none",
    built: { area: "search", screen: "no-results", kind: "drawn" },
    screen: "Search",
    state: "No results",
    brief_row: "Vault",
    notes: "Mobile / 14, Desktop / 11: SEARCH // 00, No item for {words}., NEW LOGIN with the words as the website (the drawn RESULTS // 00 and ITEMS // SEARCH change, undrawn 6.3).",
    steps: [...open, ...go("search"), { fill: /^Search/, value: "zebra" }, { pause: 200 }],
  }),
  d({
    id: "desktop-vault-search-recent",
    built: { area: "search", screen: "recently-used", kind: "undrawn" },
    screen: "Search",
    state: "Before typing",
    brief_row: "Vault",
    notes: "Before typing: the field in focus and RECENTLY USED, five at most, from what this device used (brief X4).",
    steps: [...open, ...go("search"), { pause: 300 }],
  }),
  d({
    id: "desktop-vault-search-scoped",
    screen: "Search",
    state: "From a vault",
    brief_row: "Vault",
    notes: "Opened from the Family vault: EVERYWHERE and FAMILY; FAMILY chosen, SEARCH // FAMILY in the breadcrumb (brief X4).",
    steps: [...open, ...go("vault-family"), ...go("search"), { click: /^Family/, role: "radio" }, { fill: /^Search/, value: "home" }, { pause: 200 }],
  }),
  d({ id: "desktop-vault-sort-menu", built: { area: "items", screen: "sort-menu", kind: "undrawn" }, screen: "Vault", state: "Sort and group", brief_row: "Vault", notes: "Sort A to Z, recently changed or used; group by letter (A to Z only), by type or none. Kept on this device.", steps: [...open, { click: "Sort and group" }, { wait: "By type" }] }),
  d({
    id: "desktop-vault-ungrouped",
    built: { area: "items", screen: "recently-changed", kind: "undrawn" },
    screen: "Vault",
    state: "Recently changed, one list",
    brief_row: "Vault",
    notes: "Newest change first: letters make no sense out of A to Z order, so the list is one group.",
    steps: [...open, { click: "Sort and group" }, { click: "Recently changed" }, { run: async () => void (document.querySelector('[role="menu"], [role="dialog"]') && (await runSteps([{ key: "Escape" }]))) }, { gone: "Recently used" }],
  }),
  d({ id: "desktop-vault-row-menu", built: { area: "items", screen: "row-menu", kind: "undrawn" }, screen: "Vault", state: "Row menu", brief_row: "Vault", notes: "A row's own menu: favorites, folder and tags, move, archive, history, Recently deleted.", steps: [...open, { click: /^More for Bluefin Bank/ }, { pause: 300 }] }),
  // Phone row swipes (States / 01, doc 72): a phone-only gesture, so 390 wide only; the list scrolled, as the frame draws it.
  d({
    id: "desktop-vault-swipe-trailing",
    widths: [390],
    built: { area: "items", screen: "swipe-actions", kind: "drawn" },
    screen: "Vault",
    state: "Swiped left",
    brief_row: "Vault",
    notes: "A login with a code, slid left: MORE and CODE neutral, PASSWORD outermost in the primary fill (a full swipe copies it, through the gate for a crown jewel). Every tile is also in the row's ⋯ menu.",
    steps: [...open, ...scrollContent(220), swipeRow("Bluefin Bank", "trailing")],
  }),
  d({
    id: "desktop-vault-swipe-leading",
    widths: [390],
    built: { area: "items", screen: "swipe-favorite", kind: "drawn" },
    screen: "Vault",
    state: "Swiped right",
    brief_row: "Vault",
    notes: "A row slid right from clear of the back edge: FAVORITE (UNFAVORITE on a favorite), the one leading tile.",
    steps: [...open, ...scrollContent(220), swipeRow("Bluefin Bank alerts", "leading")],
  }),
  d({ id: "desktop-vault-duplicates", screen: "Vault", state: "Find duplicates", brief_row: "Vault", notes: "Duplicates review: same password, same network. Reached from Health now (D152).", steps: [...open, ...go("health"), { click: "Duplicates" }, DIALOG] }),
  d({ id: "desktop-vault-organize", built: { area: "item", screen: "organize", kind: "undrawn" }, screen: "Vault", state: "Folder and tags", brief_row: "Vault", notes: "Folder and tags (brief I4's first Form): the folder from its vault's tree, tags as chips, Favorite, SAVE. From a row's menu, as from Details' CHANGE.", steps: [...open, { click: /^More for Bluefin Bank/ }, { click: /Folder and tags/ }, DIALOG] }),
  d({ id: "desktop-view-favourites", built: { area: "items", screen: "favorites", kind: "undrawn" }, screen: "Smart views", state: "Favorites", brief_row: "Vault", notes: "Starred items.", steps: [...open, ...go("favourites")] }),
  d({ id: "desktop-view-passkeys", built: { area: "items", screen: "passkeys", kind: "undrawn" }, screen: "Smart views", state: "Passkeys", brief_row: "Vault", notes: "Logins with passkeys and passkeys on their own.", steps: [...open, ...go("passkeys")] }),
  d({ id: "desktop-view-cards", built: { area: "items", screen: "cards", kind: "undrawn" }, screen: "Smart views", state: "Cards", brief_row: "Vault", notes: "A kind's list: its Section Header with the count and SORT, the vault as each row's words (Desktop / 03's list pane).", steps: [...open, ...go("cards")] }),
  d({ id: "desktop-view-wifi", built: { area: "items", screen: "wi-fi", kind: "undrawn" }, screen: "Smart views", state: "Wi-Fi", brief_row: "Vault", notes: "Networks and the router.", steps: [...open, ...go("wi-fi")] }),
  d({ id: "desktop-view-contacts", built: { area: "items", screen: "contacts", kind: "undrawn" }, screen: "Smart views", state: "Contacts", brief_row: null, notes: "Contacts (never used to fill).", steps: [...open, ...go("contacts")] }),
  d({ id: "desktop-view-expiring", screen: "Smart views", state: "Expiring", brief_row: "Needs attention", notes: "Soonest first, in sections by how soon.", steps: [...open, ...go("expiring")] }),
  d({ id: "desktop-view-attention", screen: "Needs attention", state: "Filled", brief_row: "Needs attention", notes: "Conflict copies, expiring items, the issuer-equals-login warning, a passkey hidden by its site, a sign-in left on a hotel TV.", steps: [...open, ...go("attention")] }),
  d({ id: "desktop-view-attention-empty", screen: "Needs attention", state: "Empty", brief_row: "Needs attention", notes: "Nothing needs you.", setup: only((i) => ["Bluefin Bank", "Home Wi-Fi", "Spare keys and the alarm"].includes(i.title)), steps: [...open, ...go("attention")] }),
  d({ id: "desktop-view-archived", built: { area: "items", screen: "archived", kind: "undrawn" }, screen: "Smart views", state: "Archived", brief_row: null, notes: "Archived items, out of the list and search.", steps: [...open, ...go("archived")] }),
  d({ id: "desktop-view-deleted", built: { area: "items", screen: "recently-deleted", kind: "undrawn" }, screen: "Smart views", state: "Recently deleted", brief_row: "Item detail", notes: "Readable through each vault's own time, which the caption says; rows offer Restore and Delete for good.", steps: [...open, ...go("deleted")] }),
  d({ id: "desktop-view-deleted-empty", built: { area: "items", screen: "recently-deleted-empty", kind: "undrawn" }, screen: "Smart views", state: "Recently deleted, empty", brief_row: "Item detail", notes: "Nothing in Recently deleted.", setup: only((i) => i.state !== "trashed"), steps: [...open, ...go("deleted")] }),
  d({ id: "desktop-scope-vault-family", built: { area: "items", screen: "vault", kind: "undrawn" }, screen: "Vaults and folders", state: "Family vault", brief_row: "Vault", notes: "The list narrowed to the shared Family vault: its header and the chips of the kinds it holds.", steps: [...open, ...go("vault-family")] }),
  d({ id: "desktop-scope-folder-finance", built: { area: "items", screen: "folder", kind: "undrawn" }, screen: "Vaults and folders", state: "Finance folder", brief_row: "Vault", notes: "The list narrowed to a folder: the folder current in its open vault beside the list; its path as the head's overline where the head is the switcher.", steps: [...open, ...go("folder-finance")] }),
  d({ id: "desktop-item-login-full", built: { area: "item", screen: "login-full", kind: "undrawn" }, screen: "Item detail", state: "Login: password, passkey and code", brief_row: "Item detail", notes: "One account, every way in: the methods block first, then the websites.", steps: [...open, ...openItem("Northwind Mail")] }),
  d({ id: "desktop-item-login-methods", built: { area: "item", screen: "login", kind: "drawn" }, screen: "Item detail", state: "Login: password, passkey, a five-character code and a protected note", brief_row: "Item detail", notes: "The design's login: every method, a five-character code, and a recovery note kept masked.", setup: withShape, steps: [...open, ...openItem(SHAPE.login)] }),
  d({ id: "desktop-item-login-passkey", built: { area: "item", screen: "passkey", kind: "drawn" }, screen: "Item detail", state: "Login: a passkey and nothing else", brief_row: "Item detail", notes: "The design's passkey login: no password to show, copy or check.", setup: withShape, steps: [...open, ...openItem(SHAPE.passkeyLogin)] }),
  d({ id: "desktop-item-note-network", built: { area: "item", screen: "secure-note", kind: "drawn" }, screen: "Item detail", state: "Note with a network and a password", brief_row: "Item detail", notes: "The design's secure note: a network's name and its password as fields of the note, the password masked.", setup: withShape, steps: [...open, ...openItem(SHAPE.note)] }),
  d({ id: "desktop-item-login-passkey-only", built: { area: "item", screen: "passkey-only", kind: "undrawn" }, screen: "Item detail", state: "Login: passkeys only, one hidden by its site", brief_row: "Item detail", notes: "No password; a passkey the site no longer knows, hidden with Undo.", steps: [...open, ...openItem("Pixelforge")] }),
  d({ id: "desktop-item-login-many-sites", built: { area: "item", screen: "many-sites", kind: "undrawn" }, screen: "Item detail", state: "Login: three websites, breached", brief_row: "Item detail", notes: "Several websites with their match rules; the item's findings.", steps: [...open, ...openItem("Parcelpost")] }),
  d({ id: "desktop-item-login-away", built: { area: "item", screen: "details", kind: "undrawn" }, screen: "Item detail", state: "Login: Details open, signed in elsewhere", brief_row: "Item detail", notes: "Details open (brief I4): one list of rows, only those that apply. A sign-in left on a hotel TV is due to be signed out (D237).", steps: [...open, ...openItem("Meadowlark Streaming"), openDetails(), bringIntoView("Details")] }),
  d({
    id: "desktop-item-login-away-fill-fix",
    built: { area: "item", screen: "fill-fix", kind: "undrawn" },
    screen: "Item detail",
    state: "Filling on this site",
    brief_row: "Item detail",
    notes: "Details' FILLING ON THIS SITE → CHANGE (brief I4's second Form, review R9): never fill here, two fields fixed by name, and a new fix.",
    answers: () => ({ site_fix: () => ({ never: false, fields: [{ match: { name: "login_id" }, role: "username" }, { match: { name: "otp_box" }, role: "code" }] }) }),
    steps: [...open, ...openItem("Meadowlark Streaming"), detailsRowAction("Filling on this site", "Change"), DIALOG],
  }),
  d({
    id: "desktop-item-login-away-protection",
    built: { area: "item", screen: "protection", kind: "undrawn" },
    screen: "Item detail",
    state: "Who can get its values",
    brief_row: "Item detail",
    notes: "Details' WHO CAN GET ITS VALUES → CHANGE (D245): the tiers as RadioRows with their sentences, in a popover with its header (a sheet on a phone).",
    steps: [...open, ...openItem("Meadowlark Streaming"), detailsRowAction("Who can get its values", "Change"), { see: '[role="radiogroup"]' }],
  }),
  d({ id: "desktop-item-conflict", screen: "Item detail", state: "Conflict copy", brief_row: "Needs attention", notes: "Two devices changed it at once: keep this copy or the other (D58).", steps: [...open, ...openItem("Tern Airlines (conflict copy from Sam's phone)")] }),
  d({ id: "desktop-item-conflict-confirm", screen: "Item detail", state: "Conflict: keep this copy?", brief_row: "Needs attention", notes: "The confirm before the other copy goes to the trash.", steps: [...open, ...openItem("Tern Airlines (conflict copy from Sam's phone)"), { click: "Keep this copy" }, DIALOG] }),
  d({ id: "desktop-item-stress", built: { area: "item", screen: "stress", kind: "undrawn" }, screen: "Item detail", state: "Long title and values", brief_row: "Item detail", notes: "Stress: a very long title, username, address and note.", steps: [...open, { click: /^Riverside Community/, within: "main" }, { wait: /Riverside Community/, role: "heading" }] }),
  d({ id: "desktop-item-shared-view", built: { area: "item", screen: "shared-view", kind: "undrawn" }, screen: "Item detail", state: "Shared with you (view)", brief_row: "Item detail", notes: "An item someone shared with view rights: nothing offered that the right doesn't allow.", steps: [...open, ...openItem("Kids' school portal")] }),
  d({ id: "desktop-item-code-steam", built: { area: "item", screen: "code-steam", kind: "undrawn" }, screen: "Item detail", state: "Code: Steam Guard", brief_row: "Codes view", notes: "A five-character Steam Guard code, full width, with Show large type.", steps: [...open, ...openItem("Kestrel Arcade · Steam Guard")] }),
  d({ id: "desktop-item-code-hotp", built: { area: "item", screen: "code-counter", kind: "undrawn" }, screen: "Item detail", state: "Code: counter (HOTP)", brief_row: "Codes view", notes: "A counter-based code with Next code.", steps: [...open, ...openItem("Office VPN token")] }),
  d({ id: "desktop-item-code-issuer-warning", built: { area: "item", screen: "code-issuer-warning", kind: "undrawn" }, screen: "Item detail", state: "Code: protects this account itself", brief_row: "Needs attention", notes: "The code's issuer is the Lockdown server you sync with.", steps: [...open, ...openItem("Lockdown server")] }),
  d({ id: "desktop-item-card", built: { area: "item", screen: "card", kind: "drawn" }, screen: "Item detail", state: "Card", brief_row: "Item detail", notes: "Number in groups of four; security code and PIN masked; asks before the code; expiring.", steps: [...open, ...openItem("Bluefin Visa")] }),
  d({ id: "desktop-item-bank", built: { area: "item", screen: "bank-account", kind: "undrawn" }, screen: "Item detail", state: "Bank account", brief_row: "Item detail", notes: "Account number and IBAN masked.", steps: [...open, ...openItem("Bluefin checking")] }),
  d({ id: "desktop-item-wifi", built: { area: "item", screen: "wifi", kind: "undrawn" }, screen: "Item detail", state: "Wi-Fi", brief_row: "Item detail", notes: "Network, password with Show large type, and the guest card.", steps: [...open, ...openItem("Home Wi-Fi")] }),
  d({ id: "desktop-item-router", built: { area: "item", screen: "router", kind: "undrawn" }, screen: "Item detail", state: "Router", brief_row: "Item detail", notes: "The router variant: the network and the admin sign-in.", steps: [...open, ...openItem("Home router")] }),
  d({ id: "desktop-item-note", built: { area: "item", screen: "note-markdown", kind: "undrawn" }, screen: "Item detail", state: "Note", brief_row: "Item detail", notes: "A Markdown body.", steps: [...open, ...openItem("Spare keys and the alarm")] }),
  d({ id: "desktop-item-identity", built: { area: "item", screen: "identity", kind: "undrawn" }, screen: "Item detail", state: "Identity", brief_row: "Item detail", notes: "Name, email, phone and address for forms.", steps: [...open, ...openItem("Sam Rivera")] }),
  d({ id: "desktop-item-passport", built: { area: "item", screen: "passport", kind: "undrawn" }, screen: "Item detail", state: "Passport, expiring", brief_row: "Item detail", notes: "A passport expiring within 90 days, with its photo page attached.", steps: [...open, ...openItem("Passport")] }),
  d({ id: "desktop-item-contact", built: { area: "item", screen: "contact", kind: "undrawn" }, screen: "Item detail", state: "Contact", brief_row: null, notes: "A contact (never filled).", steps: [...open, ...go("contacts"), ...openItem("Alex Rivera")] }),
  d({ id: "desktop-item-insurance", built: { area: "item", screen: "files", kind: "undrawn" }, screen: "Item detail", state: "Document: insurance, two files", brief_row: "Item detail", notes: "Files (brief I5): FILES 02 with ATTACH, one FileChip per file (type, name, size; press to save, × to remove), and the room used on the server.", steps: [...open, ...openItem("Home insurance"), bringIntoView("Files")] }),
  d({ id: "desktop-item-recovery-codes", built: { area: "item", screen: "recovery-codes", kind: "undrawn" }, screen: "Item detail", state: "Crown jewel: recovery codes", brief_row: "Item detail", notes: "Recovery codes, a crown jewel by default.", steps: [...open, ...openItem("Northwind Mail recovery codes")] }),
  d({ id: "desktop-item-ssh", built: { area: "item", screen: "ssh-key", kind: "undrawn" }, screen: "Item detail", state: "SSH key", brief_row: "Item detail", notes: "Public parts in plain text, private key masked, allowed hosts.", steps: [...open, ...openItem("Studio deploy key")] }),
  d({ id: "desktop-item-pgp", built: { area: "item", screen: "pgp-key", kind: "undrawn" }, screen: "Item detail", state: "PGP key", brief_row: "Item detail", notes: "The PGP variant of an SSH key item.", steps: [...open, ...openItem("Personal signing key")] }),
  d({ id: "desktop-item-api", built: { area: "item", screen: "api-key", kind: "undrawn" }, screen: "Item detail", state: "API credential, expiring", brief_row: "Item detail", notes: "A token expiring in nine days.", steps: [...open, ...openItem("Cobalt Cloud API token")] }),
  d({ id: "desktop-item-wallet", screen: "Item detail", state: "Crown jewel: crypto wallet", brief_row: "Item detail", notes: "The whole detail under the crown-jewel treatment.", steps: [...open, ...openItem("Cold wallet")] }),
  d({ id: "desktop-item-server", built: { area: "item", screen: "server", kind: "undrawn" }, screen: "Item detail", state: "Server", brief_row: "Item detail", notes: "A server's host, port and sign-in.", steps: [...open, ...openItem("Build server")] }),
  d({ id: "desktop-item-database", built: { area: "item", screen: "database", kind: "undrawn" }, screen: "Item detail", state: "Database", brief_row: "Item detail", notes: "A database's connection.", steps: [...open, ...openItem("Analytics database")] }),
  d({ id: "desktop-item-licence", built: { area: "item", screen: "software-license", kind: "undrawn" }, screen: "Item detail", state: "Software licence", brief_row: "Item detail", notes: "A licence key, masked.", steps: [...open, ...openItem("Lumen Photo Editor")] }),
  d({ id: "desktop-item-membership", built: { area: "item", screen: "membership", kind: "undrawn" }, screen: "Item detail", state: "Membership", brief_row: "Item detail", notes: "A membership with its number and PIN.", steps: [...open, ...openItem("Trailhead Climbing Gym")] }),
  d({ id: "desktop-item-custom", built: { area: "item", screen: "custom", kind: "undrawn" }, screen: "Item detail", state: "Custom item", brief_row: "Item detail", notes: "Fields of the person's own.", steps: [...open, ...openItem("Bike lock")] }),
  d({ id: "desktop-item-passkey", built: { area: "item", screen: "passkey-on-its-own", kind: "undrawn" }, screen: "Item detail", state: "Passkey on its own", brief_row: "Item detail", notes: "A passkey made by a site, view only.", steps: [...open, ...openItem("Lumen Photo forum")] }),
  d({ id: "desktop-item-trashed", screen: "Item detail", state: "In the trash", brief_row: "Item detail", notes: "Restore or delete now.", steps: [...open, ...go("deleted"), ...openItem("Old gym account")] }),
  d({ id: "desktop-item-loading", built: { area: "item", screen: "loading", kind: "undrawn" }, screen: "Item detail", state: "Loading", brief_row: "Item detail", notes: "The item has not arrived yet.", answers: () => ({ show: () => forever() }), steps: [...open, { click: starts("Northwind Mail"), within: "main" }, { pause: 300 }] }),
  d({
    id: "desktop-item-error",
    built: { area: "item", screen: "error", kind: "undrawn" },
    screen: "Item detail",
    state: "Error",
    brief_row: "Item detail",
    notes: "A field failed to decrypt: the item's id is shown, never a guess.",
    answers: () => ({ show: () => fail("refused", "Its newest version couldn't be decrypted on this device; the history is intact.") }),
    steps: [...open, { click: starts("Northwind Mail"), within: "main" }, { wait: /Couldn't open this item/ }],
  }),
  d({ id: "desktop-item-trash-confirm", built: { area: "item", screen: "trash-toast", kind: "undrawn" }, screen: "Item detail", state: "Moved to Recently deleted, with UNDO", brief_row: "Item detail", notes: "Move to Recently deleted asks nothing; its toast offers UNDO (Q14).", steps: [...open, ...openItem("Quill Notes"), itemAction("Trash"), { wait: /^Moved Quill Notes to Recently deleted/ }] }),
  d({ id: "desktop-item-purge-confirm", screen: "Item detail", state: "Delete for good?", brief_row: "Item detail", notes: "Deleting a trashed item now.", steps: [...open, ...go("deleted"), ...openItem("Old gym account"), itemAction("Delete for good"), DIALOG] }),
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
    built: { area: "item", screen: "revealed", kind: "undrawn" },
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
  d({ id: "desktop-palette", screen: "Quick-access palette", state: "Open", brief_row: "Quick-access palette", notes: "⌘K / Ctrl+K: recent items first. Not offered on a phone.", widths: NOT_PHONE, steps: [...open, { key: "Control+k" }, DIALOG] }),
  d({
    id: "desktop-palette-search",
    built: { area: "quick-access", screen: "items", kind: "undrawn" },
    screen: "Quick-access palette",
    state: "Search",
    brief_row: "Quick-access palette",
    notes: "ITEMS as compact rows with the words marked, CLOSE MATCH where the index forgave a typo, the last row, and the keys for the highlighted item (brief X4).",
    widths: WIDE,
    steps: [...open, { key: "Control+k" }, DIALOG, { fill: /^Search Lockdown$/, value: "north" }, { pause: 300 }],
  }),
  d({
    id: "desktop-palette-go-to",
    built: { area: "quick-access", screen: "go-to", kind: "undrawn" },
    screen: "Quick-access palette",
    state: "GO TO",
    brief_row: "Quick-access palette",
    notes: "GO TO: the places whose name matches, Settings with its key, then the last row, Search everything for “…” (brief X4).",
    widths: WIDE,
    steps: [...open, { key: "Control+k" }, DIALOG, { fill: /^Search Lockdown$/, value: "sett" }, { pause: 300 }],
  }),
  d({
    id: "desktop-quick-window-locked",
    built: { area: "quick-access", screen: "window-locked", kind: "undrawn" },
    screen: "Quick-access window",
    state: "Locked",
    brief_row: "Quick-access palette",
    notes: "The system-wide shortcut's window while the vault is locked: the focus frame's mark, one sentence, UNLOCK and NOT NOW (brief X4).",
    widths: WIDE,
    mount: () => <QuickWindow />,
    steps: [{ wait: "Lockdown is locked." }],
  }),
  d({
    id: "desktop-quick-window-open",
    screen: "Quick-access window",
    state: "Open",
    brief_row: "Quick-access palette",
    notes: "The system-wide shortcut's window: only the palette.",
    widths: NOT_PHONE,
    mount: () => <QuickWindow />,
    setup: (w) => {
      w.lock = { ...w.lock, locked: false }
    },
    steps: [{ pause: 600 }],
  }),
  d({
    id: "desktop-nav-vault-menu",
    built: { area: "vaults", screen: "menu", kind: "undrawn" },
    screen: "Vaults and tags trees",
    state: "Vault menu",
    brief_row: "Vaults and tags trees",
    notes: "A vault's ⋯ menu (brief V2): New folder, Rename, Keep deleted items for…, Share…. Beside the sidebar a menu; over the Switch vault sheet on a phone a Menu sheet; a menu over the popover at medium width.",
    steps: [...open, ...openSwitcher(), ...orgMenu("Personal")],
  }),
  d({
    id: "desktop-nav-new-vault",
    built: { area: "vaults", screen: "name-prompt", kind: "undrawn" },
    screen: "Vaults and tags trees",
    state: "New vault",
    brief_row: "Vaults and tags trees",
    notes: "Naming a new vault: a Form with one Text Field, CREATE VAULT (brief X5). The sidebar's + beside VAULTS; the switcher's New vault row.",
    steps: [...open, ...openSwitcher(), { click: "New vault" }, DIALOG, { fill: "Name", value: "Household" }],
  }),
  d({ id: "desktop-nav-tag", built: { area: "items", screen: "tag", kind: "undrawn" }, screen: "Vaults and tags trees", state: "Narrowed to a tag", brief_row: "Vaults and tags trees", notes: "The list narrowed to #travel: Tags open beside the list with #travel current.", steps: [...open, ...go("tag-travel")] }),
  d({
    id: "desktop-nav-switch-sheet",
    widths: WIDTHS.filter((x) => x < 600),
    built: { area: "vaults", screen: "switch-sheet", kind: "drawn" },
    screen: "Vaults and tags trees",
    state: "Switch vault",
    brief_row: "Vaults and tags trees",
    notes: "Mobile / 12, extended (brief V2): VAULTS // 03, Switch vault; All vaults with its count, each vault with its count and folder chevron, Tags, New vault; SHARED WITH; Sharing with its mark. Seen as a finger leaves it: no keyboard ring.",
    steps: [...open, ...openSwitcher(), { run: () => (document.activeElement as HTMLElement | null)?.blur() }, { pause: 200 }],
  }),
  d({
    id: "desktop-nav-folders-open",
    built: { area: "vaults", screen: "folders-open", kind: "undrawn" },
    screen: "Vaults and tags trees",
    state: "Folders and tags open",
    brief_row: "Vaults and tags trees",
    notes: "Personal's folders and the tag tree opened in place, without choosing them: by the chevrons in the switcher (the sheet grows), by the chevron beside the sidebar's row.",
    steps: [...open, ...openSwitcher(), openBranch("Personal"), openBranch("Tags"), { pause: 200 }],
  }),
  d({
    id: "desktop-nav-sidebar-trees",
    widths: WIDTHS.filter((x) => x >= 1024),
    built: { area: "vaults", screen: "sidebar-trees", kind: "undrawn" },
    screen: "Vaults and tags trees",
    state: "The sidebar's trees",
    brief_row: "Vaults and tags trees",
    notes: "VAULTS // 03 with its +, Personal open on Finance (current), Tags open; a folder's ⋯ shown as it is on hover and focus.",
    steps: [...open, ...go("folder-finance"), besideSidebar([openBranch("Tags"), focusOrgRow("Taxes")])],
  }),
  d({
    id: "desktop-nav-popover",
    widths: WIDTHS.filter((x) => x >= 600 && x < 1024),
    screen: "Vaults and tags trees",
    state: "Switch vault popover",
    brief_row: "Vaults and tags trees",
    notes: "At medium width the page head opens the same rows as a popover (brief V2).",
    steps: [...open, ...openSwitcher(), openBranch("Personal")],
  }),
  d({
    id: "desktop-nav-folder-menu",
    built: { area: "vaults", screen: "folder-menu", kind: "undrawn" },
    screen: "Vaults and tags trees",
    state: "Folder menu",
    brief_row: "Vaults and tags trees",
    notes: "A folder's ⋯ menu: New folder inside, Rename, Remove folder (Move to the top for a folder inside another).",
    steps: [...open, ...openSwitcher(), openBranch("Personal"), ...orgMenu("Finance")],
  }),
  d({
    id: "desktop-nav-keep-deleted",
    built: { area: "vaults", screen: "keep-deleted", kind: "undrawn" },
    screen: "Vaults and tags trees",
    state: "Keep deleted items for…",
    brief_row: "Vaults and tags trees",
    notes: "The vault menu's Keep deleted items for…: its six choices replace the rows, the vault's own time checked.",
    steps: [...open, ...openSwitcher(), ...orgMenu("Work"), orgMenuRow("Keep deleted items for…"), { see: '[role="menuitemradio"], [data-adaptive="menu"] [role="radio"]' }, { pause: 250 }],
  }),
  d({
    id: "desktop-nav-remove-folder",
    built: { area: "vaults", screen: "remove-folder", kind: "undrawn" },
    screen: "Vaults and tags trees",
    state: "Remove folder?",
    brief_row: "Vaults and tags trees",
    notes: "Removing a folder asks (brief X5): what moves, and that no item is deleted; focus on Cancel.",
    steps: [...open, ...openSwitcher(), openBranch("Personal"), ...orgMenu("Gaming"), orgMenuRow("Remove folder"), { wait: 'Remove "Gaming"?' }, { pause: 300 }],
  }),
  d({ id: "desktop-item-more-menu", built: { area: "item", screen: "more-menu", kind: "undrawn" }, screen: "Where it is, item menu", state: "Item menu", brief_row: "Where it is, item menu", notes: "History, archive, move, crown-jewel on or off.", steps: [...open, ...openItem("Bluefin Bank"), ...openItemMenu()] }),
  d({ id: "desktop-item-history", built: { area: "item", screen: "history", kind: "undrawn" }, screen: "Version history", state: "Versions", brief_row: "Version history", notes: "History (brief I6): every version newest first, CURRENT, when and which device, which fields changed (never a value), SHOW and RESTORE.", setup: sevenVersions, steps: [...open, ...openItem("Northwind Mail"), itemAction("History"), DIALOG, { wait: /^Version \d+$/ }] }),
  d({
    id: "desktop-item-history-version",
    built: { area: "item", screen: "history-version", kind: "undrawn" },
    screen: "Version history",
    state: "One version shown",
    brief_row: "Version history",
    notes: "SHOW opens an earlier version under its row: plain values as Field Rows, every hidden value a SecretField that never reveals or copies.",
    setup: sevenVersions,
    // The fixture reads an item's newest fields for any version; the version shown is the one asked for.
    answers: () => ({ show_version: (a) => ({ ...(as("show_version")(a) as Record<string, unknown>), version: Number(a.n) }) }),
    steps: [...open, ...openItem("Northwind Mail"), itemAction("History"), DIALOG, { click: "Show version 6" }, { see: 'section[aria-label="Version 6"]' }, restFocus()],
  }),
  d({ id: "desktop-item-jewel-on", built: { area: "item", screen: "crown-jewel-ask", kind: "undrawn" }, screen: "Where it is, item menu", state: "Keep as a crown jewel?", brief_row: "Where it is, item menu", notes: "Moving an item into the crown-jewel class: the menu's question, which Details' CROWN JEWEL → CHANGE also asks.", steps: [...open, ...openItem("Bluefin Bank"), itemAction("Keep as a crown jewel"), DIALOG] }),
  d({ id: "desktop-item-lookalike", screen: "Lookalike warning", state: "Site imitates another", brief_row: "Lookalike warning", notes: "First in the detail: this login's site imitates the bank's.", steps: [...open, ...openItem("Bluefin Bank alerts")] }),
  d({ id: "desktop-item-wifi-card", screen: "Wi-Fi guest card", state: "Join the network", brief_row: "Wi-Fi guest card", notes: "A QR guests scan; hides after 60 s.", steps: [...open, ...openItem("Guest Wi-Fi"), { click: /Guest card/ }, DIALOG, { pause: 300 }] }),
  d({ id: "desktop-item-large-type", screen: "Large type", state: "Wi-Fi password", brief_row: "Large type", notes: "Read across a room; 120 s.", steps: [...open, ...openItem("Guest Wi-Fi"), { click: /Show large type/ }, DIALOG, { pause: 400 }] }),
  d({ id: "desktop-item-large-type-code", screen: "Large type", state: "A code", brief_row: "Large type", notes: "A code's current value in large type.", steps: [...open, ...openItem("Driftwood Forum"), { click: /Show large type/ }, DIALOG, { pause: 400 }] }),
  d({ id: "desktop-item-away-dialog", built: { area: "item", screen: "away", kind: "undrawn" }, screen: "Signed in elsewhere", state: "Log a sign-in", brief_row: "Signed in elsewhere", notes: "Signing in on a TV? (brief I4's third Form, D237): where, when to remind, REMIND ME.", steps: [...open, ...openItem("Tern Airlines"), itemAction("Signing in on a TV?"), DIALOG] }),
  d({ id: "desktop-item-passkey-delete", built: { area: "item", screen: "passkey-delete", kind: "undrawn" }, screen: "Passkeys block", state: "Delete: the only way in", brief_row: "Passkeys block", notes: "Deleting the only passkey warns that the account would have no way in.", steps: [...open, ...openItem("Pixelforge"), passkeyDetails(), DIALOG, { click: /^Delete passkey/ }, { wait: "Delete this passkey?" }] }),
  d({ id: "desktop-item-setup-guide", screen: "Set-up guide", state: "Two-step sign-in for a site", brief_row: "Set-up guide", notes: "What Harbor Health offers for two-step sign-in, and how (from its Security finding).", steps: [...open, ...go("security"), { click: /Set-up guide/ }, DIALOG] }),
  d({ id: "desktop-item-playbook", screen: "Playbook (\"What to do\")", state: "From the item", brief_row: "Playbook (\"What to do\")", notes: "Steps ending in how to confirm the fix.", steps: [...open, ...openItem("Parcelpost"), { click: "What to do" }, DIALOG] }),
  d({ id: "desktop-item-file-remove", built: { area: "item", screen: "files-remove", kind: "undrawn" }, screen: "Files", state: "Remove a file?", brief_row: "Files", notes: "Removing a file asks first (brief I5): the item saves a version without it; its history keeps it. Focus on Cancel.", steps: [...open, ...openItem("Home insurance"), bringIntoView("Files"), { click: "Remove policy-2026.pdf" }, DIALOG] }),
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
  d({ id: "desktop-palette-crown-jewel", screen: "Quick-access palette", state: "Crown jewel by title", brief_row: "Quick-access palette", notes: "Crown jewels are listed by title only; copying asks first.", widths: NOT_PHONE, steps: [...open, { key: "Control+k" }, DIALOG, { fill: /^Search Lockdown$/, value: "cold" }, { pause: 400 }] }),
  d({
    id: "desktop-item-passkey-upgraded",
    built: { area: "item", screen: "passkey-upgraded", kind: "undrawn" },
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
  w({ id: "web-vault", built: { area: "items", screen: "web-list", kind: "undrawn" }, screen: "Vault", state: "Filled", brief_row: "Vault", notes: "The desktop's vault view in the web build: Links, People, Devices and Settings in the sidebar.", steps: webOpen }),
  w({ id: "web-item-login", built: { area: "item", screen: "web-login", kind: "undrawn" }, screen: "Item detail", state: "Login", brief_row: "Item detail", notes: "A login in the web build: copies go through the page's clipboard.", steps: [...webOpen, ...openItem("Northwind Mail")] }),
  w({ id: "web-item-wallet", screen: "Item detail", state: "Crown jewel", brief_row: "Item detail", notes: "A crown jewel in the web build.", steps: [...webOpen, ...openItem("Cold wallet")] }),
  w({ id: "web-security", screen: "Security", state: "Quantum-safe line", brief_row: "Security", notes: "The web vault's Security screen: the quantum-safe line; password checks stay in the desktop app.", steps: [...webOpen, ...go("security")] }),
]
