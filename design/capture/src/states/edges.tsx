// The edges lane's states: approvals, devices and sessions, pairing, codes,
// the scan, sharing and an item's Share, the editor with the new-item menu
// and templates on the desktop; Codes, Devices, Links and People in the web
// vault. The extension's states are in extension.tsx.

import { DeviceApproval } from "@/features/approvals/approval-sheet"

import { CLOCK_CORRECTED, HELD, HELD_AFTER, PAUSED_REASON, SHARING_PAUSED, TV_SIGN_IN, fakeCamera, moreCodes } from "../fixture/lane-edges"
import { PAIRING_REQUEST, PASSKEY_REQUEST, expiringCode } from "../fixture/shape"
import { status } from "../fixture/story"
import { ahead } from "../fixture/time"
import { as, fail } from "../host/core"
import { Standalone } from "../harness/standalone"
import type { StateDef } from "../harness/types"
import { WIDE, d, only, w } from "./defs"

/** A phone only: the type sheet and quick create are the phone's forms of the menu. */
const PHONE = [390]
import { choose, newItem, newItemMenu, openGroup, openRow, openRowBeside, personAction, rest, review, waits } from "./steps/edges"
import { open, webOpen } from "./steps/screens"
import { DIALOG, MENU, go } from "./steps/shell"
import { healthDevice, itemAction, openItem } from "./steps/vault"

export const EDGES_STATES: StateDef[] = [
  d({ id: "desktop-vault-new-item-menu", screen: "Vault", state: "New item menu", brief_row: "Item edit", notes: "Brief E1 on a desktop: the menu under NEW ITEM. Login, Payment card, Note, One-time code and Wi-Fi network first with their counts in two digits (no Passkey, Q18); the four groups as submenus; YOUR TEMPLATES 01; Your templates at the foot.", widths: WIDE, built: { area: "new-item", screen: "menu", kind: "undrawn" }, steps: [...open, newItemMenu, { pause: 300 }, rest] }),
  d({ id: "desktop-vault-new-item-sheet", screen: "Vault", state: "Type sheet", brief_row: "Item edit", notes: "Brief E1 on a phone (Mobile / 06): the sheet, NEW ITEM // PERSONAL VAULT, the five first with counts, the four groups as closed Section Headers, YOUR TEMPLATES and Your templates.", widths: PHONE, built: { area: "new-item", screen: "type-sheet", kind: "drawn" }, steps: [...open, newItemMenu, { see: '[data-adaptive="menu"]' }, { pause: 300 }, rest] }),
  d({ id: "desktop-vault-new-item-sheet-group", screen: "Vault", state: "Type sheet, a group open", brief_row: "Item edit", notes: "Brief E1 on a phone: IDENTITIES AND DOCUMENTS opened in place, its kinds and variants with their counts.", widths: PHONE, built: { area: "new-item", screen: "type-sheet-group", kind: "undrawn" }, steps: [...open, newItemMenu, { see: '[data-adaptive="menu"]' }, openGroup("Identities and documents"), { pause: 300 }, rest] }),
  d({ id: "desktop-vault-quick-create", screen: "Vault", state: "Quick create", brief_row: "Item edit", notes: "States / 03: a long press on create (Shift+Enter here) over the menu scrim: CREATE // LONG PRESS, Login, Payment card, Note, One-time code, no counts; the create button turned to close.", widths: PHONE, built: { area: "new-item", screen: "quick-create", kind: "drawn" }, steps: [...open, { run: () => document.querySelector<HTMLElement>('button[aria-label="New item"]')?.focus() }, { key: "Shift+Enter", target: 'button[aria-label="New item"]' }, MENU, { wait: /^Login/, role: "menuitem" }] }),
  d({ id: "desktop-editor-new-login", screen: "Item edit", state: "New login", brief_row: "Item edit", notes: "Brief E2 (Mobile / 07, Desktop / 05): NEW LOGIN // PERSONAL VAULT, the title following the website typed, Website, Username, the password made when you save with the core's words and strength, Add a one-time code on with its field, VAULT with CHANGE, More options shut; CANCEL and SAVE LOGIN (the checkmark in the phone's top bar).", built: { area: "new-item", screen: "login", kind: "drawn" }, steps: [...open, ...newItem(/^Login/), DIALOG, { fill: "Website", value: "pinecone.example" }, { fill: "Username", value: "sam.rivera" }, { click: "Generate password" }, { click: "Add a one-time code", role: "switch" }, { pause: 400 }, rest] }),
  d({ id: "desktop-editor-edit-login", screen: "Item edit", state: "Edit a login", brief_row: "Item edit", notes: "Brief E2: EDIT LOGIN // PERSONAL VAULT, the item's title, the secrets kept unless retyped (GENERATE, REMOVE), its code, More options open because it holds values; the caption about History, CANCEL and SAVE.", built: { area: "new-item", screen: "edit-login", kind: "undrawn" }, steps: [...open, ...openItem("Northwind Mail"), itemAction("Edit"), DIALOG, { pause: 300 }, rest] }),
  d({ id: "desktop-editor-edit-card", screen: "Item edit", state: "Edit a card", brief_row: "Item edit", notes: "Brief E2: a card's essentials (Card number, Name on card, Expires, Security code), VAULT, then More options.", built: { area: "new-item", screen: "card", kind: "undrawn" }, steps: [...open, ...openItem("Bluefin Visa"), itemAction("Edit"), DIALOG, { pause: 300 }, rest] }),
  d({ id: "desktop-codes-filled", screen: "Codes view", state: "Filled", brief_row: "Codes view", notes: "Brief C1 (Mobile / 08, Desktop / 06): CODES // NN, rows on a phone and tiles from the medium width up on unicron's CodeDisplay; TOTP, eight digits, Steam Guard and a counter; one code four seconds from its end, its rule and seconds critical.", built: { area: "codes", screen: "list", kind: "drawn" }, answers: expiringCode("Northwind Mail"), steps: [...open, ...go("codes"), { wait: "Northwind" }] }),
  d({ id: "desktop-codes-expiring", screen: "Codes view", state: "A code about to change", brief_row: "Codes view", notes: "One code four seconds from its end, where the countdown turns critical; the rest have their full time.", answers: expiringCode("Northwind Mail"), steps: [...open, ...go("codes"), { wait: "Northwind" }] }),
  d({ id: "desktop-codes-empty", screen: "Codes view", state: "Empty", brief_row: "Codes view", notes: "Brief C1: CODES // 00, \"No codes yet.\", the one line, ADD CODE.", built: { area: "codes", screen: "empty", kind: "undrawn" }, setup: only((i) => !Object.keys(i.fields).some((k) => k.startsWith("code."))), steps: [...open, ...go("codes"), { wait: "No codes yet." }] }),
  d({ id: "desktop-codes-add", screen: "Codes view", state: "Add a code", brief_row: "Codes view", notes: "Brief C1: a Form. The set-up link in a SecretField (masked), Keep it (its own item, or inside a login), Title, CANCEL and ADD CODE. Paste only; no camera (R5, W11).", built: { area: "codes", screen: "add", kind: "undrawn" }, steps: [...open, ...go("codes"), { click: /^Add (a )?code/ }, DIALOG, { pause: 300 }] }),
  d({ id: "desktop-codes-scan", screen: "Codes view", state: "Scan", brief_row: "Approvals", notes: "Brief C2, the Form: CODES // TV SIGN-IN, the sentence, CAMERA · PICTURE · PASTE, the camera's square panel and its status line. No primary: a read moves on by itself.", built: { area: "codes", screen: "tv-sign-in", kind: "undrawn" }, setup: () => fakeCamera(), steps: [...open, ...go("codes"), { click: "Sign in TV" }, DIALOG, { wait: "Hold the code steady in the square." }] }),
  d({ id: "desktop-codes-scan-review", screen: "Sign in a TV or console", state: "Review: camera", brief_row: "Sign in a TV or console", notes: "Brief C2, the Review on unicron's ApprovalSheet (a sheet on a phone): the five-minute wait, SERVICE and PAGE, the TV's code in DigitGroups, Sign in with, the caption, DON'T SIGN IN and SIGN IN ON {HOST}. Only the camera gets a yes.", built: { area: "codes", screen: "tv-review", kind: "undrawn" }, setup: () => fakeCamera(), answers: () => ({ scan_frame: () => TV_SIGN_IN }), steps: [...open, ...go("codes"), { click: "Sign in TV" }, DIALOG, { wait: "A TV or console sign-in" }, { pause: 700 }] }),
  d({ id: "desktop-codes-counter", screen: "Codes view", state: "Counter, Steam and eight digits", brief_row: "Codes view", notes: "Brief C1: a counter code shows COUNTER 212 and NEXT CODE in place of the rule and seconds; Steam Guard's five characters; an eight-digit code in fours.", built: { area: "codes", screen: "counter", kind: "undrawn" }, setup: only((i) => ["Office VPN token", "Kestrel Arcade · Steam Guard", "Driftwood Forum"].includes(i.title)), steps: [...open, ...go("codes"), { wait: "Next code" }] }),
  d({ id: "desktop-codes-clock", screen: "Codes view", state: "The clock is off", brief_row: "Codes view", notes: "Brief C1: the clock notice as a compact warning Banner under the page head, the core's words and CLOCK SETTINGS.", built: { area: "codes", screen: "clock", kind: "undrawn" }, answers: () => ({ clock_status: () => CLOCK_CORRECTED }), steps: [...open, ...go("codes"), { wait: "Clock settings" }] }),
  d({ id: "desktop-sharing", screen: "Sharing", state: "People", brief_row: "Sharing", notes: "Brief S1 and S2: SHARING // 03 PEOPLE // 01 SHARED VAULT, the four tabs with their counts, PEOPLE first. The list: YOUR FINGERPRINT in DigitGroups with SHOW CODE, YOUR CODE, Look someone up, PEOPLE 03 with COMPARED or NOT COMPARED. Beside it (1440) Priya, not compared: her fingerprint, devices, COMPARE FINGERPRINTS as the one primary, CHECK WITH A PHRASE, FORGET as text, DETAILS shut.", built: { area: "sharing", screen: "people", kind: "undrawn" }, steps: [...open, ...go("sharing"), { wait: "Your fingerprint" }, openRowBeside("People", "priya@nair-design.example"), { pause: 300 }, rest] }),
  d({ id: "desktop-devices", screen: "Devices", state: "Filled, one unknown", brief_row: "Devices", notes: "Brief T2: DEVICES 05 as Field Rows, this device first with Now, the others with their last use from the session list or the day they were added, platforms cased, REVOKED and its date on the old one; ADD DEVICE; SIGNED IN with END and END ALL OTHERS; SECURITY KEYS with ADD KEY and REMOVE; the ids under Details.", built: { area: "settings-devices", screen: "list", kind: "undrawn" }, steps: [...open, ...go("devices"), { wait: "Signed in" }, rest] }),
  d({ id: "desktop-devices-only-this", screen: "Devices", state: "Only this device", brief_row: "Devices", notes: "Brief T2: one row, This device, Now, and one line under it: \"Only this device. A second device is what gets you back in when this one is lost.\" Then ADD DEVICE.", built: { area: "settings-devices", screen: "only-this", kind: "undrawn" }, answers: (w) => ({
      device_list: () => [{ id: "this", name: "Sam's MacBook Pro", platform: "macos", enrolled_at: Math.floor(Date.now() / 1000) - 86400 * 3, revoked_at: null, this_device: true }],
      session_list: () => [],
      status: () => ({ ...status(w), devices: 1, devices_revoked: 0 }),
    }), steps: [...open, ...go("devices"), { wait: /^Only this device/ }, rest] }),
  d({ id: "desktop-devices-revoke", screen: "Devices", state: "Revoke?", brief_row: "Devices", notes: "REVOKE on linux-a7f3 asks first (brief X5): unicron's ConfirmDialog, the device and its platform as the overline, CANCEL focused, REVOKE in the critical fill. The core's gate follows only if the core asks (D101).", built: { area: "settings-devices", screen: "revoke", kind: "undrawn" }, steps: [...open, ...go("devices"), { click: "Revoke", role: "button", nth: 3 }, DIALOG, { pause: 300 }] }),
  d({ id: "desktop-item-share-menu", screen: "Item Share menu", state: "Menu", brief_row: "Item Share menu", notes: "Brief I7's entry, from the item's ⋯ → Share… (brief 3.6): SHARE HARBOR HEALTH with two rows, With someone… and As a link…, and CANCEL.", built: { area: "item", screen: "share-menu", kind: "undrawn" }, steps: [...open, ...openItem("Harbor Health"), itemAction("Share"), DIALOG, { pause: 300 }] }),
  d({ id: "desktop-item-share-person", screen: "Item Share menu", state: "With someone", brief_row: "Item Share menu", notes: "Brief I7, a Form (a full screen on a phone): HARBOR HEALTH // SHARE, \"Share Harbor Health\", With (one not compared reads // not compared), They can as two RadioRows (View; Fill only with its sentence), the caption, CANCEL and SHARE (an extended Share on a phone).", built: { area: "item", screen: "share-person", kind: "undrawn" }, steps: [...open, ...openItem("Harbor Health"), itemAction("Share"), DIALOG, { click: /^With someone/, within: '[role="dialog"]' }, DIALOG, { wait: "They can" }, { pause: 300 }, rest] }),
  d({ id: "desktop-item-share-link", screen: "Item Share menu", state: "As a link", brief_row: "Item Share menu", notes: "Brief I7, a Form: GUEST WI-FI // LINK, \"A link to Guest Wi-Fi\", Lasts, Opens at most, Password (optional) in a SecretField with its hint, the caption, CANCEL and CREATE LINK (an extended Create link on a phone).", built: { area: "item", screen: "share-link", kind: "undrawn" }, steps: [...open, ...openItem("Guest Wi-Fi"), itemAction("Share"), DIALOG, { click: /^As a link/, within: '[role="dialog"]' }, DIALOG, { pause: 300 }, rest] }),
  d({ id: "desktop-item-share-link-once", screen: "Link shown once", state: "Copy link", brief_row: "Link shown once", notes: "Brief I7: the link shown once in the same layer, cut in the middle in mono; COPY LINK the layer's one primary, full width; the caption; DONE secondary in the footer (a text button at the top bar's start on a phone, nothing at its end).", built: { area: "item", screen: "share-link-once", kind: "undrawn" }, steps: [...open, ...openItem("Guest Wi-Fi"), itemAction("Share"), DIALOG, { click: /^As a link/, within: '[role="dialog"]' }, DIALOG, { click: "Create link" }, { wait: "Copy link" }, { pause: 300 }, rest] }),
  d({ id: "desktop-editor-new-wifi", screen: "Item edit", state: "New Wi-Fi network", brief_row: "Item edit", notes: "Brief E2: Network name, Password (GENERATE), Security, VAULT; the title following the network name.", built: { area: "new-item", screen: "wifi", kind: "undrawn" }, steps: [...open, ...newItem(/^Wi-Fi network/), DIALOG, { fill: "Network name", value: "cabin-net" }, { pause: 300 }, rest] }),
  d({ id: "desktop-editor-new-passport", screen: "Item edit", state: "New passport", brief_row: "Item edit", notes: "Brief E2: a variant from IDENTITIES AND DOCUMENTS; Title, the Passport section and every required field, VAULT, then More options (Holder, Notes).", built: { area: "new-item", screen: "passport", kind: "undrawn" }, steps: [...open, ...newItem(/^Passport/, "Identities and documents"), DIALOG, { pause: 300 }, rest] }),
  d({ id: "desktop-editor-generator", screen: "Generator options", state: "Characters", brief_row: "Generator options", notes: "Brief E3: OPTIONS opens the picker by the password (a popover with GENERATOR // Password options on a desktop, a sheet on a phone): Make, Length with steppers, Digits, Symbols, No look-alikes, Leave out; the core's words, bits and a strength Badge; no DONE.", built: { area: "new-item", screen: "generator", kind: "undrawn" }, steps: [...open, ...newItem(/^Login/), DIALOG, { click: "Generate password" }, { click: "Options for password" }, { see: '[data-adaptive="popover"]' }, { pause: 500 }] }),
  d({ id: "desktop-editor-templates", screen: "Your templates", state: "List", brief_row: "Your templates", notes: "Brief E4: YOUR TEMPLATES 01, one compact row per template (name, what it's for, 02 FIELDS) with CHANGE and DELETE; CLOSE and NEW TEMPLATE.", built: { area: "new-item", screen: "templates", kind: "undrawn" }, steps: [...open, ...newItem(/^Your templates$/), DIALOG, { pause: 300 }, rest] }),
  d({ id: "desktop-editor-template-new", screen: "Your templates", state: "New template", brief_row: "Your templates", notes: "Brief E4: the designer in the same layer: Name, What it's for, FIELDS 01 (label, kind, Needed, move up and down, drop), Add a field; BACK and SAVE TEMPLATE.", built: { area: "new-item", screen: "template-new", kind: "undrawn" }, steps: [...open, ...newItem(/^Your templates$/), DIALOG, { click: "New template" }, { pause: 300 }, rest] }),
  d({
    id: "desktop-editor-save-error",
    screen: "Item edit",
    state: "Error on save",
    brief_row: "Item edit",
    notes: "Brief E2: the core refused the save: a compact critical Banner above the footer (at the top of the form on a phone) with the core's sentence; the form keeps what was typed.",
    built: { area: "new-item", screen: "save-error", kind: "undrawn" },
    answers: () => ({ edit_item: () => fail("refused", "This item changed on another device while you edited it. Your edit was kept as a conflict copy.") }),
    steps: [...open, ...openItem("Quill Notes"), itemAction("Edit"), DIALOG, { click: /^Save$/, role: "button" }, { wait: "Not saved" }, { pause: 300 }, rest],
  }),
  d({ id: "desktop-codes-scan-paste", screen: "Sign in a TV or console", state: "Paste: refused", brief_row: "Sign in a TV or console", notes: "Brief C2: a pasted code is never approved. The Review says Refused in a critical Banner with the core's words, the facts, and SCAN AGAIN; no approve control.", built: { area: "codes", screen: "tv-refused", kind: "undrawn" }, steps: [...open, ...go("codes"), { click: "Sign in TV" }, DIALOG, { click: "Paste", role: "tab" }, { fill: "The code's address", value: "https://watch.meadowlark.example/activate?code=WDJB-MJHT" }, { click: "Check it" }, { wait: "Refused" }, { pause: 300 }] }),
  d({ id: "desktop-codes-search", screen: "Codes view", state: "Search", brief_row: "Codes view", notes: "Brief C1: from ten codes up, a search field in the page head (desktop; a phone finds codes through Search). One match.", built: { area: "codes", screen: "search", kind: "undrawn" }, widths: WIDE, setup: moreCodes, steps: [...open, ...go("codes"), { fill: "Search codes", value: "cobalt" }, { pause: 200 }] }),
  d({ id: "desktop-codes-search-none", screen: "Codes view", state: "Search finds nothing", brief_row: "Codes view", notes: "Brief C1: SEARCH // 00, \"No code for fastmail.\"", built: { area: "codes", screen: "search-none", kind: "undrawn" }, widths: WIDE, setup: moreCodes, steps: [...open, ...go("codes"), { fill: "Search codes", value: "fastmail" }, { wait: "No code for fastmail." }] }),
  d({ id: "desktop-devices-from-pq", screen: "Devices", state: "Brought into view from Security", brief_row: "Devices", notes: "The quantum-safe line's device link opens Devices on that device: its row selected (primary-soft, the 2 px edge) and scrolled into view.", built: { area: "settings-devices", screen: "from-health", kind: "undrawn" }, steps: [...open, ...go("security"), healthDevice("Studio desktop"), { pause: 500 }] }),
  d({ id: "desktop-sharing-offline", screen: "Sharing", state: "Offline", brief_row: "Sharing", notes: "Brief S1: online is false. A compact info Banner under the tabs, \"You're offline.\" / \"Sharing needs the server.\"; the lists stay; Look up is off with \"Needs the server. You're offline.\" under the field.", built: { area: "sharing", screen: "offline", kind: "undrawn" }, answers: () => ({ sharing_overview: () => ({ ...(as("sharing_overview")({}) as object), online: false }) }), steps: [...open, ...go("sharing"), { wait: "You're offline." }, rest] }),
  d({ id: "desktop-sharing-compare", screen: "People", state: "Compare fingerprints", brief_row: "People", notes: "Brief S2: the Form over Priya's detail. One sentence, What they read out with its hint, SCAN THEIR QR CODE as text, NOT NOW and THEY MATCH (an extended float button on a phone).", built: { area: "sharing", screen: "compare", kind: "undrawn" }, steps: [...open, ...go("sharing"), openRow("People", "priya@nair-design.example"), personAction("Compare fingerprints"), DIALOG, { pause: 300 }] }),
  d({ id: "desktop-sharing-phrase", screen: "Phrase check", state: "Say this phrase", brief_row: "Phrase check", notes: "Brief S2: a question dialog (a sheet on a phone): KEY CHECK // PRIYA…, the nine digits in DigitGroups, \"Say it in person or on a call you trust, never in a message. It works once.\", the line that follows the check, CANCEL.", built: { area: "sharing", screen: "phrase", kind: "undrawn" }, answers: () => ({ phrase_invite: (a) => ({ ...(as("phrase_invite")(a) as object), phrase: "482 913 076", words: "Waiting for Priya to type it." }), phrase_poll: (a) => ({ ...(as("phrase_poll")(a) as object), phrase: "482 913 076", words: "Waiting for Priya to type it." }) }), steps: [...open, ...go("sharing"), openRow("People", "priya@nair-design.example"), personAction("Check with a phrase"), DIALOG, { wait: "Waiting for Priya to type it." }, { pause: 300 }] }),
  d({ id: "desktop-sharing-forget", screen: "People", state: "Forget?", brief_row: "People", notes: "Brief S2 and X5: unicron's ConfirmDialog: PERSON // PRIYA…, \"Forget priya@nair-design.example?\", today's sentence, CANCEL focused, FORGET critical.", built: { area: "sharing", screen: "forget", kind: "undrawn" }, steps: [...open, ...go("sharing"), openRow("People", "priya@nair-design.example"), personAction("Forget"), DIALOG, { pause: 300 }] }),
  d({ id: "desktop-sharing-code", screen: "People", state: "Your code", brief_row: "People", notes: "Brief S2: SHOW CODE opens a question dialog (a sheet on a phone): unicron's QrCode of your contact code, dark on the stone ground in both themes, and your fingerprint in DigitGroups; DONE.", built: { area: "sharing", screen: "your-code", kind: "undrawn" }, steps: [...open, ...go("sharing"), { click: "Show code" }, DIALOG, { pause: 300 }, rest] }),
  d({ id: "desktop-sharing-reset", screen: "Sharing", state: "Someone reset their account", brief_row: "Sharing", notes: "Brief X3 row 6, on Sharing only: the needs-you under the page head (the strip on a phone), WAITING ON YOU, the core's question (D634), LET THEM BACK IN and NOT NOW.", built: { area: "sharing", screen: "reset", kind: "undrawn" }, answers: () => ({ sharing_overview: () => ({ ...(as("sharing_overview")({}) as object), waiting_on_you: 0, resets: [{ account_id: "acc-jordan", login: "jordan@rivera.example", words: "jordan@rivera.example reset their account. Let them back in?", seen_at: Math.floor(Date.now() / 1000) - 3600 }] }) }), steps: [...open, ...go("sharing"), { see: '[data-needs-you-row="6"]' }, { pause: 300 }, rest] }),
  d({ id: "desktop-sharing-vaults", screen: "Sharing", state: "Vaults", brief_row: "Sharing", notes: "Brief S3's list: VAULTS 03 with each vault's line in today's words and the right as a word, Family // HOUSEHOLD; ITEMS SHARED ONE BY ONE 02 with STOP SHARING and OPEN as text. Beside it (1440) \"Nothing open.\"", built: { area: "sharing", screen: "vaults", kind: "undrawn" }, steps: [...open, ...go("sharing-vaults"), { wait: "Items shared one by one" }, rest] }),
  d({ id: "desktop-sharing-local-only", screen: "Sharing", state: "On this computer only", brief_row: "Sharing", notes: "Brief S1: a vault kept on this computer only. The whole page is one Empty State: SHARING, \"Sharing needs a server.\", the one line, SET UP SERVER (Settings → Server).", built: { area: "sharing", screen: "local-only", kind: "undrawn" }, setup: (w) => { w.device = { ...w.device, local: true } }, steps: [...open, ...go("sharing"), { wait: "Sharing needs a server." }, rest] }),
  d({ id: "desktop-sharing-vault", screen: "Vault members", state: "Family vault", brief_row: "Manage sharing", notes: "Brief S3's detail: VAULT // SHARED // 3 PEOPLE, Family, the caption; MEMBERS 03 with YOU and OWNER Badges, the right as a word, CHANGE and REMOVE as text; ADD SOMEONE (Who, They can, ADD secondary); HISTORY 03 shut. No primary.", built: { area: "sharing", screen: "members", kind: "undrawn" }, steps: [...open, ...go("sharing-vaults"), openRow("Vaults", "Family"), { wait: "Members" }, { pause: 300 }, rest] }),
  d({ id: "desktop-sharing-links", screen: "Share and request links", state: "Links", brief_row: "Share and request links", notes: "Brief S4's list: SHARE TEXT and ASK FOR SECRET (secondary), SOMEONE SENT YOU A LINK (one field, its password in a SecretField, OPEN), SHARE LINKS 03 (Created 2026.11.14 // expires NOV 21 // opened 0 of 1 // password, DELETE as text), REQUESTS 02 (OPEN ANSWER and DELETE). Beside it (1440) \"Nothing open.\"", built: { area: "sharing", screen: "links", kind: "undrawn" }, steps: [...open, ...go("sharing-links"), { wait: "Share links" }, rest] }),
  d({ id: "desktop-sharing-text-link", screen: "Share and request links", state: "Share text", brief_row: "Share and request links", notes: "Brief S4, a Form (a full screen on a phone): LINKS // SHARE TEXT, What it is, The text (6 lines), Lasts, Opens at most, Password (optional), CANCEL and CREATE LINK (an extended Create link on a phone).", built: { area: "sharing", screen: "share-text", kind: "undrawn" }, steps: [...open, ...go("sharing-links"), { click: "Share text" }, DIALOG, { pause: 300 }, rest] }),
  d({ id: "desktop-sharing-text-link-once", screen: "Link shown once", state: "Copy link", brief_row: "Link shown once", notes: "Brief S4 and I7: the text link shown once, COPY LINK the one primary, full width, the caption, DONE.", built: { area: "sharing", screen: "link-once", kind: "undrawn" }, steps: [...open, ...go("sharing-links"), { click: "Share text" }, DIALOG, { fill: "The text", value: "Gate code 4417" }, { click: "Create link" }, { wait: "Copy link" }, { pause: 300 }, rest] }),
  d({ id: "desktop-sharing-request-link", screen: "Share and request links", state: "Ask for a secret", brief_row: "Share and request links", notes: "Brief S4, a Form: LINKS // ASK FOR A SECRET, What you're asking for, Lasts, CANCEL and CREATE LINK.", built: { area: "sharing", screen: "request", kind: "undrawn" }, steps: [...open, ...go("sharing-links"), { click: "Ask for secret" }, DIALOG, { pause: 300 }, rest] }),
  d({ id: "desktop-sharing-opened", screen: "Opened link", state: "An answer opened", brief_row: "Opened link", notes: "Brief S4's detail: OPENED // SHARED WITH YOU, Wi-Fi at the studio, the answer in a SecretField (masked, shown and copied one value at a time), KEEP IN MY VAULT (secondary), the caption. On a phone the detail is pushed.", built: { area: "sharing", screen: "opened", kind: "undrawn" }, steps: [...open, ...go("sharing-links"), { click: "Open answer" }, { see: 'section[aria-label="Links: open"]' }, { pause: 600 }, rest] }),
  d({ id: "desktop-sharing-emergency", screen: "Emergency access", state: "Contacts", brief_row: "Emergency access", notes: "Brief S5's list and detail: the caption, ADD EMERGENCY CONTACT (secondary), YOUR EMERGENCY CONTACTS 01 and PEOPLE WHO TRUST YOU 01 with wait 7 days // 02 vaults and the state's word. Beside it (1440) Alex: EMERGENCY ACCESS // ACCEPTED, WAITING PERIOD, VAULTS, END IT as critical text, WHAT HAPPENED 02.", built: { area: "sharing", screen: "emergency", kind: "undrawn" }, steps: [...open, ...go("sharing-emergency"), { wait: "Your emergency contacts" }, openRowBeside("Your emergency contacts", "alex@rivera.example", "Emergency"), { pause: 300 }, rest] }),
  d({ id: "desktop-sharing-emergency-add", screen: "Emergency access", state: "Add a contact", brief_row: "Emergency access", notes: "Brief S5, a Form: EMERGENCY // ADD A CONTACT, Who (compared people only), Waiting period with its hint, Which vaults, the caption, CANCEL and ASK THEM (through the gate; an extended Ask them on a phone).", built: { area: "sharing", screen: "emergency-add", kind: "undrawn" }, steps: [...open, ...go("sharing-emergency"), { click: "Add emergency contact" }, DIALOG, { pause: 300 }, rest] }),
  d({ id: "desktop-sharing-emergency-requested", screen: "Emergency access", state: "Asked for access", brief_row: "Emergency access", notes: "Brief X3 row 4 and S5: Alex asked. On the grant's own screen the needs-you's solid action is STOP IT (no gate): EMERGENCY ACCESS // OPENS {date}, \"alex@rivera.example asked for your vaults.\" The detail: ASKED FOR ACCESS, the line with the date and time, OPENS, END IT. On a phone the strip, and the grant pushed.", built: { area: "sharing", screen: "emergency-requested", kind: "undrawn" }, answers: () => ({ sharing_overview: () => {
    const o = as("sharing_overview")({}) as { emergency: Array<Record<string, unknown>> }
    o.emergency[0] = { ...o.emergency[0], state: "requested", release_at: ahead(5) }
    return o
  } }), steps: [...open, ...go("sharing-emergency"), { wait: "Your emergency contacts" }, openRow("Your emergency contacts", "alex@rivera.example", "Emergency"), { see: '[data-needs-you-row="4"]' }, { pause: 300 }, rest] }),
  d({ id: "desktop-devices-add", screen: "Add a device", state: "Code and QR", brief_row: "Add a device", notes: "Brief T2 step 1: a review dialog (a sheet on a phone), DEVICE LINK // 10:00 LEFT, \"Add a device\", the sentence, unicron's QrCode from the core's rows, the code with COPY, the segmented rule for the time left, and CANCEL.", built: { area: "settings-devices", screen: "add", kind: "undrawn" }, answers: () => ({ link_poll: () => as("link_offer")({}) }), steps: [...open, ...go("devices"), { click: "Add device" }, DIALOG, { pause: 400 }, rest] }),
  d({ id: "desktop-devices-add-compare", screen: "Add a device", state: "Do the digits match?", brief_row: "Add a device", notes: "Brief T2 step 2 and A1: the compare review on unicron's ApprovalSheet, NEW DEVICE // 09:59 LEFT, \"Add Sam's new phone?\", the six digits both screens show, THEY DON'T MATCH and THEY MATCH (through the gate).", built: { area: "settings-devices", screen: "add-compare", kind: "undrawn" }, steps: [...open, ...go("devices"), { click: "Add device" }, { see: '[data-slot="approval-sheet"]' }, { pause: 700 }, rest] }),
  d({ id: "desktop-devices-end-session", screen: "Signed in", state: "End all other sessions?", brief_row: "Signed in", notes: "END ALL OTHERS asks first and takes no gate (D221, brief X5): unicron's ConfirmDialog with CANCEL and END ALL OTHERS.", built: { area: "settings-devices", screen: "end-session", kind: "undrawn" }, steps: [...open, ...go("devices"), { click: "End all others" }, DIALOG, { pause: 300 }] }),
  // --- requests (doc 84; brief A1, X3 rows 1 to 3) ------------------------------------
  // A request waits as the needs-you and never opens by itself: each review here is opened from REVIEW.
  d({
    id: "desktop-approval-browser-pairing",
    screen: "Browser pairing",
    state: "Pair Chrome?",
    brief_row: "Browser pairing",
    notes: "A browser asks to pair: PAIRING REQUEST // 01:58 LEFT on the needs-you, then REVIEW: BROWSER and KIND, the code in DigitGroups with its sentence, THEY MATCH (through the gate) or CODES DIFFER, and what a paired browser can do (brief A1; D156 as changed by doc 84).",
    built: { area: "sign-in-request", screen: "pairing", kind: "undrawn" },
    answers: () => ({ browser_pending: () => [PAIRING_REQUEST] }),
    steps: [...open, ...review, rest],
  }),
  d({
    id: "desktop-approval-passkey",
    screen: "Passkey approval",
    state: "Review a sign-in",
    brief_row: "Passkey approval",
    notes: "Mobile / 11 and Desktop / 09: SIGN-IN REQUEST // 01:57 LEFT, the request's own words as the title, the BROWSER row, the segmented rule, DECLINE and APPROVE WITH TOUCH ID (the fixture's approve route is the platform check). No ACCOUNT, DEVICE, NEAR or TIME: the request carries none of them (W5).",
    built: { area: "sign-in-request", screen: "review", kind: "drawn" },
    answers: () => ({ browser_passkey_pending: () => [PASSKEY_REQUEST] }),
    steps: [...open, ...review, rest],
  }),
  d({
    id: "desktop-approval-passkey-strip",
    screen: "Passkey approval",
    state: "Waiting on Items",
    brief_row: "Passkey approval",
    notes: "The request waits; nothing opens by itself. Phone: the yellow strip over the bottom row with REVIEW (Mobile / 01). Wider: the block at the top of the detail column, over \"Nothing open.\", with REVIEW solid and DECLINE as text (X3 row 1).",
    built: { area: "needs-you", screen: "sign-in-strip", kind: "undrawn" },
    answers: () => ({ browser_passkey_pending: () => [PASSKEY_REQUEST] }),
    steps: [...open, ...waits, rest],
  }),
  d({
    id: "desktop-approval-passkey-block",
    screen: "Passkey approval",
    state: "Waiting over a login",
    brief_row: "Passkey approval",
    notes: "Desktop / 01's block: the request at the top of the open login's detail, SIGN-IN REQUEST // 01:57 LEFT, the request's words, REVIEW and DECLINE. The request names no item, so the block shows on every screen, this one included (app spec, W5). Phone: the login with the strip.",
    built: { area: "needs-you", screen: "sign-in-block", kind: "undrawn" },
    answers: () => ({ browser_passkey_pending: () => [PASSKEY_REQUEST] }),
    steps: [...open, ...openItem("Pixelforge"), ...waits, rest],
  }),
  d({
    id: "desktop-approval-passkey-gate",
    screen: "Passkey approval",
    state: "The gate over the review",
    brief_row: "Passkey approval",
    notes: "On a computer whose approve route is the master password the yes is plain APPROVE; pressing it opens the gate over the review. CANCEL returns to the review with nothing approved (brief A1, X5).",
    built: { area: "sign-in-request", screen: "gate", kind: "undrawn" },
    answers: () => ({
      browser_passkey_pending: () => [PASSKEY_REQUEST],
      approval_route: (a) => (a.kind === "approve" ? { route: "password", method: "password" } : as("approval_route")(a)),
      browser_passkey_answer: (a) => {
        if (!(a.gate as { password?: string } | undefined)?.password) fail("approval_needed", "your master password is needed to approve this sign-in", "password")
        return undefined
      },
    }),
    steps: [...open, ...review, { pause: 300 }, { click: "Approve", role: "button" }, { wait: "To approve this sign-in." }, { pause: 300 }],
  }),
  d({
    id: "desktop-approval-passkey-ran-out",
    screen: "Passkey approval",
    state: "Ran out while open",
    brief_row: "Passkey approval",
    notes: "The two minutes ran out with the review open: \"This request ran out.\", EXPIRED in the overline, the rule empty, and CLOSE alone. Nothing was answered (brief A1). The request here lasts a few seconds, on the real clock.",
    built: { area: "sign-in-request", screen: "ran-out", kind: "undrawn" },
    live_clock: true,
    answers: () => {
      let first: number | null = null
      return {
        browser_passkey_pending: () => {
          first ??= Date.now()
          const left = 10 - Math.floor((Date.now() - first) / 1000)
          return left > 0 ? [{ ...PASSKEY_REQUEST, expires_in_secs: 10 }] : []
        },
      }
    },
    steps: [...open, ...review, { pause: 5000 }, { wait: "This request ran out." }, { pause: 300 }, rest],
  }),
  d({
    id: "desktop-approval-under-editor",
    screen: "Passkey approval",
    state: "Arriving under the editor",
    brief_row: "Passkey approval",
    notes: "A request that arrives while the editor is open shows as the compact line on the editor's top edge, SIGN-IN REQUEST // 01:57 LEFT and REVIEW; the block behind the scrim hides (X3, LA8). REVIEW opens the review over the editor.",
    answers: () => ({ browser_passkey_pending: () => (document.querySelector('[role="dialog"]') ? [PASSKEY_REQUEST] : []) }),
    steps: [...open, ...newItem(/^Login/), DIALOG, { see: '[data-needs-you-layer] [data-slot="needs-you"], [data-needs-you-layer] [data-slot="accessory"]' }, { pause: 300 }],
  }),
  d({
    id: "desktop-approval-sheet-seam",
    screen: "Approvals",
    state: "Device request (seam)",
    brief_row: "Approvals",
    notes: "unicron's ApprovalSheet as the host binds it for a device request (X3 row 3): DEVICE REQUEST, the request's what and who, WHERE and WHEN, the three digits the other side shows, APPROVE (the host runs the gate), DENY and THIS WASN'T ME. Nothing in the core produces a request yet, so no screen mounts it (brief §3.2); the needs-you row is tested in approval-sheet.test.tsx.",
    built: { area: "sign-in-request", screen: "device", kind: "undrawn" },
    mount: () => (
      <Standalone>
        <DeviceApproval
          request={{ id: "ap-1", kind: "device", origin: "pairing", title: "Sam's new phone asks to join", context: [{ label: "Where", value: "Port Elm, WA (198.51.100.23)" }, { label: "When", value: "14:31" }], expected: "418", digits: 3 }}
          onApprove={() => {}}
          onDeny={() => {}}
          onReport={() => {}}
        />
      </Standalone>
    ),
    steps: [DIALOG, { pause: 700 }],
  }),
  d({
    id: "desktop-editor-generator-words",
    screen: "Generator options",
    state: "Six words: strong",
    brief_row: "Generator options",
    notes: "Brief E3, a passphrase: Words with steppers, Between words, A capital on each word, A digit on one word. The default six words (77.5 bits) read strong: weak under 50 bits, fair under 75 (D230 as changed by D375).",
    built: { area: "new-item", screen: "generator-words", kind: "undrawn" },
    steps: [
      ...open,
      ...newItem(/^Login/),
      DIALOG,
      { click: "Generate password" },
      { click: "Options for password" },
      { pause: 300 },
      { run: () => choose('[aria-label="Generator options"] select', "words") },
      { wait: /bits\s*Strong/ },
    ],
  }),
  d({
    id: "desktop-sharing-paused",
    screen: "Sharing",
    state: "Paused: split view",
    brief_row: "Sharing",
    notes: "The server's log doesn't match a mirror: new look-ups, shares, links and emergency steps wait; the vault keeps syncing (D328 as changed by D375).",
    answers: () => ({ sharing_overview: () => ({ ...(as("sharing_overview")({}) as object), paused: SHARING_PAUSED }) }),
    built: { area: "sharing", screen: "paused", kind: "undrawn" },
    steps: [...open, ...go("sharing"), { wait: /^Sharing is paused/ }, rest],
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
    built: { area: "sharing", screen: "link-refused", kind: "undrawn" },
    steps: [...open, ...go("sharing-links"), { click: "Ask for secret" }, DIALOG, { fill: "What you're asking for (they see this)", value: "The Wi-Fi password" }, { click: "Create link" }, { wait: /^Making a request link is paused/ }, rest],
  }),
  d({
    id: "desktop-sharing-vault-held",
    screen: "Vault members",
    state: "New members wait (split view)",
    brief_row: "Manage sharing",
    notes: "While the server's log doesn't match a mirror, a member added elsewhere waits unapplied; items from members already here keep syncing (doc 35 §2). The core's words in an info Banner; CHANGE, REMOVE and ADD SOMEONE are hidden meanwhile (D572).",
    answers: () => ({ vault_sharing: (a) => ({ ...(as("vault_sharing")(a) as object), held: HELD }) }),
    steps: [...open, ...go("sharing-vaults"), openRow("Vaults", "Family"), { wait: /^New members wait until/ }, rest],
  }),
  d({
    id: "desktop-sharing-vault-held-after",
    screen: "Vault members",
    state: "Changes after a new member's wait (split view)",
    brief_row: "Manage sharing",
    notes: "A head signed by someone this device hasn't met stopped the vault during a split: heads chain hash to hash, so every later change waits behind it, members' already here included (doc 36 §3).",
    answers: () => ({ vault_sharing: (a) => ({ ...(as("vault_sharing")(a) as object), held: HELD_AFTER }) }),
    steps: [...open, ...go("sharing-vaults"), openRow("Vaults", "Family"), { wait: /Changes made after a new member's/ }, rest],
  }),

  // --- the web vault -----------------------------------------------------------------
  w({ id: "web-codes", screen: "Codes view", state: "Filled", brief_row: "Codes view", notes: "Brief C1 in the web vault: the same page as the desktop's.", built: { area: "web-codes", screen: "list", kind: "undrawn" }, steps: [...webOpen, ...go("codes"), { wait: "Codes" }] }),
  w({ id: "web-devices", screen: "Devices", state: "Devices and sessions", brief_row: "Signed in", notes: "Brief T2, the web vault: Devices and Signed in, as on the desktop.", built: { area: "web-devices", screen: "list", kind: "undrawn" }, steps: [...webOpen, ...go("devices"), { wait: "Signed in" }, rest] }),
  w({ id: "web-links", screen: "Links", state: "Links and requests", brief_row: "Links", notes: "Brief S4 in the web vault: the desktop's Links list under the web's two tabs (PEOPLE, LINKS). Text links, requests and answers only; a pasted share link opens on its own page, so there is no password field and no KEEP IN MY VAULT.", built: { area: "web-sharing", screen: "links", kind: "undrawn" }, steps: [...webOpen, ...go("links"), { wait: "Someone sent you a link" }, rest] }),
  w({ id: "web-people", screen: "People", state: "Contacts", brief_row: "People", notes: "Brief S2 in the web vault: the same People list and detail under the web's two tabs (PEOPLE, LINKS); your code with no COPY (the web's sharing client has no clipboard path). Beside it (1440) Priya open.", built: { area: "web-sharing", screen: "people", kind: "undrawn" }, steps: [...webOpen, ...go("people"), { wait: "Your fingerprint" }, openRowBeside("People", "priya@nair-design.example"), { pause: 300 }, rest] }),
]
