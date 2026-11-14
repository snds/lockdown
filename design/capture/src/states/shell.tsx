// The shell's states (foundation): what the frame around every screen
// draws by itself. Entry, navigation, chrome, the five message channels
// (brief X2: the status line and strip, toasts, the critical and info
// Banners), needs-you, the gate, the lock and the reminders. Same App, same
// fixture; each state is a few steps deep.

import { LIVE_EVENT } from "@/features/sessions/live-client"

import { ago } from "../fixture/time"
import { as } from "../host/core"
import type { StateDef, Step } from "../harness/types"
import { WIDE, d } from "./defs"
import { open } from "./steps/screens"
import { DIALOG, MENU, checkAgain, go, lock, openKeys, openMore, openSwitcher, scrollContent } from "./steps/shell"
import { openItem, reveal } from "./steps/vault"

const reminders = (password: boolean, drill: boolean, kitTest: boolean) => ({
  password: { applies: true, due: password, last_at: ago(15), every_days: 14, next_at: ago(1) },
  drill: { kit: kitTest ? "created" : "drilled", last_drilled_at: kitTest ? null : ago(370), next_at: ago(5), due: drill, kit_test_pending: kitTest },
})

/** Lets go of focus, so a finished screen's picture shows the screen and not the ring of the last step's press. */
const rest: Step = { run: () => (document.activeElement as HTMLElement | null)?.blur() }

/** A phone's width only: the floating rows and the sheets they open. */
const PHONE = [390]

export const SHELL_STATES: StateDef[] = [
  // The places and the three shells (doc 64; brief §3 and X1).
  d({
    id: "shell-sidebar-more-open",
    widths: WIDE,
    screen: "Sidebar",
    state: "More open",
    brief_row: null,
    notes: "The sidebar with More open in place: every other kind that holds an item, then Archived and Recently deleted (doc 64, D725).",
    built: { area: "shell", screen: "sidebar-more-open", kind: "undrawn" },
    steps: [...open, ...openMore(), { wait: /^Recently deleted/ }, rest],
  }),
  d({
    id: "shell-sidebar-folder-tree",
    widths: WIDE,
    screen: "Sidebar",
    state: "A vault's folder tree, a folder current",
    brief_row: null,
    notes: "A vault's folders open in place; the folder is the one current row and the breadcrumb names the way to it (doc 64).",
    built: { area: "shell", screen: "sidebar-folder-tree", kind: "undrawn" },
    steps: [...open, ...go("folder-finance"), rest],
  }),
  d({
    id: "shell-items-chrome",
    widths: PHONE,
    screen: "Phone chrome",
    state: "Items",
    brief_row: null,
    notes: "The phone's rows: the lockup and the profile button; the page head as the switcher; the chip row; Search, the three tabs and create (doc 64, D729).",
    built: { area: "shell", screen: "items-chrome", kind: "undrawn" },
    steps: open,
  }),
  d({
    id: "shell-switch-sheet",
    widths: PHONE,
    screen: "Phone chrome",
    state: "Switch vault",
    brief_row: null,
    notes: "The page head opens Switch vault: All vaults, each vault, Tags, New vault, the shared vaults' members, Sharing (doc 64, D721).",
    built: { area: "shell", screen: "switch-sheet", kind: "undrawn" },
    steps: [...open, ...openSwitcher()],
  }),
  d({
    id: "shell-more-sheet",
    widths: PHONE,
    screen: "Phone chrome",
    state: "More kinds and lists",
    brief_row: null,
    notes: "The MORE chip's sheet: the other kinds that hold an item, then what is kept aside (doc 64, D725).",
    built: { area: "shell", screen: "more-sheet", kind: "undrawn" },
    steps: [...open, ...openMore()],
  }),
  d({
    id: "shell-quick-create",
    widths: PHONE,
    screen: "Phone chrome",
    state: "Quick create",
    brief_row: null,
    notes: "A long press on create (Shift+Enter here): Login, Card, Secure note, One-time code, over the menu scrim (States / 03).",
    built: { area: "shell", screen: "quick-create", kind: "undrawn" },
    steps: [...open, { run: () => document.querySelector<HTMLElement>('button[aria-label="New item"]')?.focus() }, { key: "Shift+Enter", target: 'button[aria-label="New item"]' }, MENU, { wait: /^Login/, role: "menuitem" }],
  }),
  d({
    id: "shell-settings-list",
    widths: PHONE,
    screen: "Settings",
    state: "The list on a phone",
    brief_row: "Settings",
    notes: "Settings from the profile button: this device's rows, then every other section; Lock in the top bar (doc 64, D722, D727).",
    built: { area: "shell", screen: "settings-list", kind: "undrawn" },
    steps: [...open, ...go("settings"), { see: 'nav[aria-label="Settings sections"]' }],
  }),
  d({
    id: "shell-keys",
    widths: WIDE,
    screen: "Keys",
    state: "The keys list",
    brief_row: null,
    notes: "`?` opens the keys list: KEYS, Keyboard shortcuts, a Field Row per key with the key as a Kbd, no primary (brief X6, doc 65). Desktop only: a phone has no keys.",
    built: { area: "shell", screen: "keys", kind: "undrawn" },
    steps: [...open, ...openKeys()],
  }),
  d({
    id: "desktop-item-wallet-gate",
    screen: "Item detail",
    state: "Crown jewel: password asked",
    brief_row: "Item detail",
    notes: "Revealing a crown jewel's field asks for the full master password (D98, D101).",
    setup: (w) => {
      w.settings = { ...w.settings, reveal: "toggle" }
    },
    steps: [...open, ...openItem("Cold wallet"), reveal("Recovery phrase"), DIALOG],
  }),
  d({
    id: "desktop-idle-toast",
    screen: "Shell",
    state: "Idle lock warning",
    brief_row: null,
    notes: "Ten seconds before an idle lock: LOCKING IN // 08 S in the live tone with ten segments and STAY UNLOCKED, in the status line on a desktop and the live strip on a phone; said once (brief X2, D750). The id is kept from the toast it replaced.",
    built: { area: "messages", screen: "idle-lock", kind: "undrawn" },
    steps: [...open, { wait: /^Stay( unlocked)?$/, role: "button" }, rest],
    answers: (w) => ({ lock_state: () => ({ ...w.lock, idle_secs: 592, idle_timeout_secs: 600 }) }),
  }),
  d({ id: "desktop-reminder-away", screen: "Signed in elsewhere", state: "Due: news", brief_row: "Signed in elsewhere", notes: "A sign-in left on a hotel TV is due to be signed out: the info Banner on All items after unlock, with Show and a close button (D237, D753).", answers: () => ({ away_reminders: as("away_reminders_due") }), steps: [...open, { wait: "Dismiss" }, rest] }),
  d({
    id: "desktop-reminder-expiry",
    screen: "Smart views",
    state: "Expiring: news",
    brief_row: "Needs attention",
    notes: "Items expiring soon: the one info Banner at the top of All items after unlock, with Show (Health at Expiring) and a close button; dismissed, it stays away for the day (brief X2 table 5, D753).",
    built: { area: "messages", screen: "info-banner", kind: "undrawn" },
    answers: () => ({ expiry_reminders: as("expiry_reminders_due") }),
    steps: [...open, { wait: "Dismiss" }, rest],
  }),
  d({ id: "desktop-reminder-kit-test", screen: "Reminders", state: "Test your recovery kit", brief_row: "Reminders", notes: "Right after the first run: test the kit, or skip with \"I understand an untested kit can fail\" (D251).", answers: () => ({ reminders: () => reminders(false, false, true) }), steps: [...open, DIALOG] }),
  d({ id: "desktop-reminder-password", screen: "Reminders", state: "Your password, once more", brief_row: "Reminders", notes: "Every 14 days where unlock never asks for the password (D252).", answers: () => ({ reminders: () => reminders(true, false, false) }), steps: [...open, DIALOG] }),
  d({ id: "desktop-reminder-drill", screen: "Reminders", state: "Time for the drill", brief_row: "Reminders", notes: "The yearly drill (D253).", answers: () => ({ reminders: () => reminders(false, true, false) }), steps: [...open, DIALOG] }),
  d({
    id: "desktop-live-session-ended",
    screen: "Shell",
    state: "Session ended elsewhere",
    brief_row: null,
    notes: "Another device ended this one's session: the critical Banner at the top of Items, \"Lockdown isn't syncing\", with SIGN IN, which opens the account dialog. It cannot be dismissed (brief X2 table 4, D751).",
    built: { area: "messages", screen: "banner-sign-in", kind: "undrawn" },
    steps: [...open, { emit: LIVE_EVENT, payload: { kind: "session_ended" } }, { wait: "Sign in", role: "button" }, rest],
  }),
  d({
    id: "desktop-live-split",
    screen: "Shell",
    state: "Log split warning",
    brief_row: "Status",
    notes: "A sync's mirrors caught a split view: the critical Banner on Items (and Sharing, and Status) with SEE STATUS, until a sync without it (brief X2 table 4, D752).",
    built: { area: "messages", screen: "banner-split", kind: "undrawn" },
    steps: [...open, { emit: LIVE_EVENT, payload: { kind: "synced", pulled: 2, lines: ["Split view: the mirror at kt.witness.example disagrees."] } }, { wait: "See status", role: "button" }, rest],
  }),
  d({ id: "desktop-locked-wash-narrow", screen: "Unlock", state: "Locked, behind the wash", brief_row: "Vault", notes: "The vault locked: the wash and skeleton rows, nothing readable (brief §4).", steps: [...open, ...lock(), { wait: "Unlock", role: "button" }] }),
  d({
    id: "desktop-passkey-upgrade-toast",
    screen: "Shell",
    state: "A site added a passkey",
    brief_row: "Passkeys block",
    notes: "After a password sign-in Lockdown saw, the site added a passkey with no prompt; the app says so afterwards in the info Banner on All items, with Show opening the login (D210, D312 as changed by D375; D753). The id is kept from the toast it replaced.",
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
    id: "desktop-backup-late",
    screen: "Backup line",
    state: "Failed",
    brief_row: "Backup line",
    notes: "Backups are on and the last one failed, four days after the last that worked: the critical Banner at the top of Items with OPEN BACKUPS, in the core's words (brief X2 table 4, D754). The sidebar's line is gone (D728); \"Backed up 4 days ago\" is in Settings → Backups.",
    answers: () => ({ backup_status: () => ({ ...(as("backup_status")({}) as object), last_success_at: ago(4), next_due_at: ago(3), last_error: "The backup folder /Volumes/Backup isn't there. Connect the drive, or choose another folder." }) }),
    steps: [...open, { wait: "Open backups", role: "button" }, rest],
  }),
  // The scrolled phone chrome (States / 02): a sync the person asked for holds, so the strip shows; then the list scrolls.
  d({
    id: "shell-phone-scrolled",
    widths: PHONE,
    screen: "Phone chrome",
    state: "Scrolled, dock minimized",
    brief_row: null,
    notes: "States / 02: the page head collapsed to the small centred title (still the switcher), the L mark only, the tab bar shrunk to the active tab, the SYNCING strip inside the bottom row, a hairline under the top row, the scroll-edge fade. Check again started the sync (brief 3.3, X2).",
    built: { area: "items", screen: "scrolled", kind: "drawn" },
    answers: () => ({ sync: () => new Promise(() => {}) }),
    steps: [...open, ...checkAgain(), ...scrollContent(320), rest],
  }),
  d({
    id: "shell-phone-scrolled-short",
    widths: [900],
    screen: "Phone chrome",
    state: "Scrolled, on a short or medium screen",
    brief_row: null,
    notes: "The same steps on a phone held sideways (844 × 390: the trailing column, the strip at the foot of the content) and at medium width (the rail, the status line's SYNCING).",
    answers: () => ({ sync: () => new Promise(() => {}) }),
    steps: [...open, ...checkAgain(), ...scrollContent(320), rest],
  }),
]
