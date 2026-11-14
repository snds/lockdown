// Steps the shell owns (foundation): the waits every state shares, the way
// to a place, the lock, and the gate's answer. A state of any lane reaches a
// place through `go`, so when the navigation changes only this file does.
//
// The shell is one of three (doc 64): the sidebar at the expanded width, its
// rail at medium width, the phone's floating rows under 600 px. A place is
// reached a different way in each, so `go` looks at what is drawn and takes
// that shell's path. A state's steps stay the same list at every width.

import { runSteps } from "../../harness/driver"
import type { Step } from "../../harness/types"

/** Any open dialog or alert. */
export const DIALOG: Step = { see: '[role="dialog"], [role="alertdialog"]' }
/** Any open menu. */
export const MENU: Step = { see: '[role="menu"]' }

export const esc = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
/** A name that starts with these words (a row's title before its details, an entry before its count). */
export const starts = (s: string) => new RegExp(`^${esc(s)}(\\s|,|$)`)

/** The unlocked app has drawn: the list's heading ("All items" beside the sidebar, "Items" where the head is the switcher). The same wait on the desktop and in the web vault. */
export const VAULT_OPEN: Step = { wait: /^(All items|Items)$/, role: "heading" }

const SIDEBAR = 'nav[aria-label="Sidebar"]'
const SECTIONS = 'nav[aria-label="Settings sections"]'
const LAYER = '[role="dialog"]'

/** Which shell is drawn: the whole sidebar, its rail, or the phone's rows. */
type Shell = "sidebar" | "rail" | "phone"
function shell(): Shell {
  const nav = document.querySelector<HTMLElement>(`${SIDEBAR}[data-slot="app-frame-sidebar"]`)
  if (!nav) return "phone"
  return nav.dataset.state === "rail" ? "rail" : "sidebar"
}

/** A step for each shell, picked when it runs. */
const by = (paths: Record<Shell, Step[]>): Step => ({ run: () => runSteps(paths[shell()]) })

/** One of the three sections: a sidebar row, a rail item, or a tab. */
const section = (label: string): Step =>
  by({
    sidebar: [{ click: starts(label), within: SIDEBAR }],
    rail: [{ click: starts(label), within: SIDEBAR }],
    phone: [{ click: starts(label), role: "tab" }],
  })

/** The Items list across all vaults: where the page head and the chips are. */
const items: Step = by({ sidebar: [{ click: starts("All items"), within: SIDEBAR }], rail: [{ click: starts("Items"), within: SIDEBAR }], phone: [{ click: starts("Items"), role: "tab" }] })

/** A view the sidebar's ITEMS group always lists: its row, or its chip (under MORE while it holds nothing). */
const mainView = (label: string): Step[] => [
  by({
    sidebar: [{ click: starts(label), within: SIDEBAR }],
    rail: [items, chipOrMore(label)],
    phone: [items, chipOrMore(label)],
  }),
]
function chipOrMore(label: string): Step {
  return {
    run: async () => {
      // The chip is drawn once the rows have loaded, and only for a view that holds something.
      await runSteps([{ see: '[role="radiogroup"][aria-label="Show"] [role="radio"]' }, { pause: 150 }])
      const chip = Array.from(document.querySelectorAll('[role="radiogroup"][aria-label="Show"] [role="radio"]')).some((el) => starts(label).test((el.textContent ?? "").trim()))
      await runSteps(chip ? [{ click: starts(label), role: "radio" }] : [{ click: "More kinds and lists" }, { click: starts(label), within: `${LAYER}, [role="menu"]` }])
    },
  }
}

/** A view under More: open More in the sidebar, or the MORE chip's list. */
const moreView = (label: string): Step[] => [
  by({
    sidebar: [
      {
        run: async () => {
          const more = Array.from(document.querySelectorAll<HTMLElement>(`${SIDEBAR} [aria-expanded]`)).find((el) => (el.getAttribute("aria-label") ?? "") === "More")
          if (more?.getAttribute("aria-expanded") === "false") await runSteps([{ click: "More", within: SIDEBAR }])
        },
      },
      { click: starts(label), within: SIDEBAR },
    ],
    rail: [items, { click: "More kinds and lists" }, { click: starts(label), within: '[role="menu"]' }],
    phone: [items, { click: "More kinds and lists" }, { click: starts(label), within: LAYER }],
  }),
]

/** Opens what the page head switches (the vaults, Tags, Sharing) where the sidebar's groups don't show. */
const switcher: Step[] = [items, { click: /Switch vault$/ }, { see: LAYER }]

/** A row of the vaults tree: in the sidebar, or in the switcher's sheet or popover. */
const tree = (...labels: string[]): Step[] => [
  by({
    sidebar: labels.map((l) => ({ click: starts(l), within: SIDEBAR })),
    rail: [...switcher, ...labels.map((l) => ({ click: starts(l), within: LAYER }))],
    phone: [...switcher, ...labels.map((l) => ({ click: starts(l), within: LAYER }))],
  }),
]

/** Settings: the pinned row, the rail's item, or the phone's profile button. */
const settings: Step = by({
  sidebar: [{ click: starts("Settings"), within: SIDEBAR }],
  rail: [{ click: starts("Settings"), within: SIDEBAR }],
  phone: [{ click: starts("Settings"), role: "button" }],
})

/** A Settings section, by the name its row carries. */
const settingsSection = (label: string | RegExp): Step[] => [settings, { see: SECTIONS }, { click: typeof label === "string" ? starts(label) : label, within: SECTIONS }]

/** The device's own section: the first one beside the list; the rows above the list where a section is pushed. */
const thisDevice: Step[] = [
  settings,
  { see: SECTIONS },
  {
    run: async () => {
      const row = Array.from(document.querySelectorAll<HTMLElement>(`${SECTIONS} [aria-label]`)).find((el) => /^This (Mac|PC|browser)$/.test(el.getAttribute("aria-label") ?? ""))
      if (row) await runSteps([{ click: /^This (Mac|PC|browser)$/, within: SECTIONS }])
    },
  },
]

/** A Health list: Health, then its row. */
const healthList = (label: string): Step[] => [section("Health"), { click: starts(label), role: "listitem" }]

/** A Sharing tab. */
const sharing = (tab?: string): Step[] => [...tree("Sharing"), ...(tab ? [{ click: tab, role: "tab" } as Step] : [])]

/**
 * Every place a state goes to, by an id that outlives its label.
 */
const PLACES = {
  all: [items],
  favourites: mainView("Favorites"),
  logins: mainView("Logins"),
  passkeys: mainView("Passkeys"),
  cards: mainView("Cards"),
  notes: mainView("Notes"),
  codes: [section("Codes")],
  "wi-fi": moreView("Wi-Fi"),
  contacts: moreView("Contacts"),
  archived: moreView("Archived"),
  deleted: moreView("Recently deleted"),
  expiring: healthList("Expiring"),
  attention: healthList("Needs attention"),
  security: [section("Health")],
  health: [section("Health")],
  search: [by({ sidebar: [{ click: starts("Search"), within: SIDEBAR }], rail: [{ click: starts("Search"), within: SIDEBAR }], phone: [{ click: "Search", role: "button" }] })],
  sharing: sharing(),
  "sharing-people": sharing("People"),
  "sharing-vaults": sharing("Vaults"),
  "sharing-links": sharing("Links"),
  "sharing-emergency": sharing("Emergency"),
  links: sharing("Links"),
  people: sharing("People"),
  settings: [settings],
  "settings-this-device": thisDevice,
  "settings-browsers": settingsSection("Browsers"),
  "settings-quick-access": settingsSection("Quick access"),
  "settings-appearance": settingsSection("Appearance"),
  devices: settingsSection("Devices"),
  recovery: settingsSection("Recovery"),
  status: settingsSection("Status"),
  "settings-server": settingsSection("Server"),
  "settings-import-export": settingsSection("Import and export"),
  "settings-backups": settingsSection("Backups"),
  "settings-aliases": settingsSection("Email aliases"),
  "settings-codes": settingsSection("Codes"),
  "settings-security-checks": settingsSection("Security checks"),
  "settings-about": settingsSection("About"),
  "vault-family": tree("Family"),
  "folder-finance": tree("Personal", "Finance"),
  "tag-travel": tree("Tags", "travel"),
} as const satisfies Record<string, Step[]>
export type PlaceId = keyof typeof PLACES

/** Goes to a place, by the path the shell that is drawn offers: on the desktop and in the web vault, at every width. */
export const go = (place: PlaceId): Step[] => [...PLACES[place]]

/** Opens the vaults' switcher where the page head is one (the phone's sheet, the popover at medium width). At the expanded width the tree is in the sidebar and this does nothing. */
export const openSwitcher = (): Step[] => [by({ sidebar: [], rail: switcher, phone: switcher })]

/** Opens the list behind the MORE chip (the phone's sheet). At the expanded width it opens More in the sidebar. */
export const openMore = (): Step[] => [
  by({
    sidebar: [{ click: "More", within: SIDEBAR }],
    rail: [items, { click: "More kinds and lists" }, MENU],
    phone: [items, { click: "More kinds and lists" }, { see: LAYER }],
  }),
]

/** Locks the vault from the shell: beside Settings in the sidebar and the rail, in Settings' top bar on a phone. */
export const lock = (): Step[] => [
  by({
    sidebar: [{ click: "Lock", within: SIDEBAR }],
    rail: [{ click: "Lock", within: SIDEBAR }],
    phone: [settings, { click: "Lock", role: "button" }],
  }),
]

/** Answers the gate a protected action raises, with the password it asks for. */
export const answerGate = (password: string): Step[] => [{ fill: /password/i, value: password }, { click: /^(Confirm|Continue|Unlock)$/ }]
