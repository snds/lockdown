// Steps the vault lane owns: the list, an item's detail and its actions, and
// Health. A state of another lane that opens an item or presses one of its
// actions calls these, so a renamed control is fixed here and nowhere else.

import { runSteps } from "../../harness/driver"
import type { Step } from "../../harness/types"
import { starts } from "./shell"

/** Opens an item from the list by its title and waits for its detail. */
export const openItem = (title: string): Step[] => [{ click: starts(title), within: "main" }, { wait: title, role: "heading" }]

/** A button with this name is on screen now. */
const shown = (name: string | RegExp) =>
  Array.from(document.querySelectorAll<HTMLElement>("button")).some((b) => {
    const r = b.getBoundingClientRect()
    const label = b.getAttribute("aria-label") ?? b.textContent?.replace(/\s+/g, " ").trim() ?? ""
    return r.width > 0 && r.height > 0 && (typeof name === "string" ? label === name : name.test(label))
  })

/** Opens Details at the foot of the item (brief I4), unless it is open. */
export const openDetails = (): Step => ({
  run: async () => {
    const toggle = Array.from(document.querySelectorAll<HTMLElement>("button[aria-expanded]")).find((b) => /^Details/.test(b.textContent?.trim() ?? ""))
    if (toggle && toggle.getAttribute("aria-expanded") !== "true") await runSteps([{ click: /^Details/, role: "button" }])
  },
})

/** The item's ⋯ menu, open: a menu on a desktop, a sheet of row buttons on a phone (AdaptiveMenu). */
const ITEM_MENU: Step = { see: '[role="menu"], [role="dialog"]' }

/** Opens the item's ⋯ menu (brief I1): the overline's line on a desktop, the top bar on a phone. */
export const openItemMenu = (): Step[] => [{ click: "More for this item" }, ITEM_MENU]

/** A row of the item's ⋯ menu: a menu item on a desktop, a row button in the sheet on a phone. */
const fromMenu = (row: string | RegExp): Step => ({
  run: async () => {
    await runSteps(openItemMenu())
    await runSteps([{ click: row, role: document.querySelector('[role="menu"]') ? "menuitem" : "button" }])
  },
})

const ACTIONS = {
  // Share stays ItemShare's own button until edges-8 lets the ⋯ open it: on the overline's line on a
  // desktop, in Details on a phone.
  Share: {
    run: async () => {
      if (!shown(/^Share$/)) await runSteps([openDetails()])
      await runSteps([{ click: /^Share$/ }])
    },
  },
  Edit: { click: "Edit", role: "button" },
  // Moving to Recently deleted asks nothing; its toast offers UNDO (Q14).
  Trash: fromMenu("Move to Recently deleted"),
  "Delete for good": fromMenu("Delete for good"),
  "Signing in on a TV?": fromMenu("Signing in on a TV?"),
  History: fromMenu("History"),
  "Keep as a crown jewel": fromMenu("Keep as a crown jewel"),
} satisfies Record<string, Step>

/** One of the open item's own actions, by its name. */
export const itemAction = (name: keyof typeof ACTIONS): Step => ACTIONS[name]

/** Reveals a masked field of the open item (Show with the toggle setting, Hold to show with hold). */
export const reveal = (label: string): Step => ({ click: new RegExp(`^(Show|Reveal) ${label}`) })

/** Opens a passkey's details on the open item, by its site or the first one. */
export const passkeyDetails = (site?: string): Step => ({ click: site ? `Details of the passkey for ${site}` : /^Details of the passkey for /, role: "button" })

/** A device named on Health's quantum-safe line: pressing it brings the device into view on Devices. */
export const healthDevice = (name: string): Step => ({ click: name, role: "button" })
