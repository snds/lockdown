// Steps the shell owns (foundation): the waits every state shares, the way
// to a place, the lock, and the gate's answer. A state of any lane reaches a
// place through `go`, so when the navigation changes only this file does.

import type { Step } from "../../harness/types"

/** Any open dialog or alert. */
export const DIALOG: Step = { see: '[role="dialog"], [role="alertdialog"]' }
/** Any open menu. */
export const MENU: Step = { see: '[role="menu"]' }

export const esc = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
/** A name that starts with these words (a row's title before its details, an entry before its count). */
export const starts = (s: string) => new RegExp(`^${esc(s)}(\\s|$)`)

/** The unlocked app has drawn: the list's heading. The same wait on the desktop and in the web vault. */
export const VAULT_OPEN: Step = { wait: /^All items/ }

/**
 * Every place a state goes to, by an id that outlives its label. The trees'
 * entries (a vault, a folder, a tag) are drawn at desktop widths only.
 */
const PLACES = {
  favourites: "Favourites",
  passkeys: "Passkeys",
  codes: "Codes",
  "wi-fi": "Wi-Fi",
  contacts: "Contacts",
  expiring: "Expiring",
  attention: "Needs attention",
  archived: "Archived",
  deleted: "Deleted",
  security: "Security",
  sharing: "Sharing",
  devices: "Devices",
  recovery: "Recovery",
  status: "Status",
  settings: "Settings",
  links: "Links",
  people: "People",
  "vault-family": "Family",
  "folder-finance": "Finance",
  "tag-travel": "travel",
} as const

export type PlaceId = keyof typeof PLACES

const SIDEBAR = 'nav[aria-label="Sidebar"]'

/** Goes to a place: its sidebar entry today, on the desktop and in the web vault. */
export const go = (place: PlaceId): Step[] => [{ click: starts(PLACES[place]), within: SIDEBAR }]

/** Locks the vault from the shell. */
export const lock = (): Step[] => [{ click: starts("Lock"), within: SIDEBAR }]

/** Answers the gate a protected action raises, with the password it asks for. */
export const answerGate = (password: string): Step[] => [{ fill: /password/i, value: password }, { click: /^(Confirm|Continue|Unlock)$/ }]
