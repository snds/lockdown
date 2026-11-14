// The shell's states (foundation): what the frame around every screen
// draws by itself. Entry, navigation, chrome, messages, needs-you, the gate,
// the lock, the backup line, the toasts that arrive on their own and the
// reminders. Same App, same fixture; each state is a few steps deep.

import { LIVE_EVENT } from "@/features/sessions/live-client"

import { ago } from "../fixture/time"
import { as } from "../host/core"
import type { StateDef, Step } from "../harness/types"
import { WIDE, d } from "./defs"
import { open } from "./steps/screens"
import { DIALOG, MENU, go, lock, openMore, openSwitcher } from "./steps/shell"
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
  d({ id: "desktop-reminder-kit-test", screen: "Reminders", state: "Test your recovery kit", brief_row: "Reminders", notes: "Right after the first run: test the kit, or skip with \"I understand an untested kit can fail\" (D251).", answers: () => ({ reminders: () => reminders(false, false, true) }), steps: [...open, DIALOG] }),
  d({ id: "desktop-reminder-password", screen: "Reminders", state: "Your password, once more", brief_row: "Reminders", notes: "Every 14 days where unlock never asks for the password (D252).", answers: () => ({ reminders: () => reminders(true, false, false) }), steps: [...open, DIALOG] }),
  d({ id: "desktop-reminder-drill", screen: "Reminders", state: "Time for the drill", brief_row: "Reminders", notes: "The yearly drill (D253).", answers: () => ({ reminders: () => reminders(false, true, false) }), steps: [...open, DIALOG] }),
  d({ id: "desktop-live-session-ended", screen: "Shell", state: "Session ended elsewhere", brief_row: null, notes: "Another device ended this one's session: the toast.", steps: [...open, { emit: LIVE_EVENT, payload: { kind: "session_ended" } }, { pause: 600 }] }),
  d({ id: "desktop-live-split", screen: "Shell", state: "Log split warning", brief_row: "Status", notes: "A sync's mirrors caught a split view: the sticky warning.", steps: [...open, { emit: LIVE_EVENT, payload: { kind: "synced", pulled: 2, lines: ["Split view: the mirror at kt.witness.example disagrees."] } }, { pause: 600 }] }),
  d({ id: "desktop-locked-wash-narrow", screen: "Unlock", state: "Locked, behind the wash", brief_row: "Vault", notes: "The vault locked: the wash and skeleton rows, nothing readable (brief §4).", steps: [...open, ...lock(), { wait: "Unlock", role: "button" }] }),
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
  d({ id: "desktop-backup-late", screen: "Backup line", state: "Late", brief_row: "Backup line", notes: "A backup is overdue. The sidebar's line is gone (doc 64, D728): the state is told in Settings → Backups until found-5's Banner.", answers: () => ({ backup_status: () => ({ ...(as("backup_status")({}) as object), last_success_at: ago(4), next_due_at: ago(3) }) }), steps: [...open, ...go("settings-backups")] }),
]
