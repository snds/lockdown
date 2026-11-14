// Steps the vault lane owns: the list, an item's detail and its actions, and
// Health. A state of another lane that opens an item or presses one of its
// actions calls these, so a renamed control is fixed here and nowhere else.

import type { Step } from "../../harness/types"
import { starts } from "./shell"

/** Opens an item from the list by its title and waits for its detail. */
export const openItem = (title: string): Step[] => [{ click: starts(title), within: "main" }, { wait: title, role: "heading" }]

const ACTIONS = {
  Share: { click: /^Share$/ },
  Edit: { click: "Edit", role: "button" },
  Trash: { click: "Trash", role: "button" },
} satisfies Record<string, Step>

/** One of the open item's own actions, by its name. */
export const itemAction = (name: keyof typeof ACTIONS): Step => ACTIONS[name]

/** Reveals a masked field of the open item (Show with the toggle setting, Reveal with hold). */
export const reveal = (label: string): Step => ({ click: new RegExp(`^(Show|Reveal) ${label}`) })

/** A device named on Health's quantum-safe line: pressing it brings the device into view on Devices. */
export const healthDevice = (name: string): Step => ({ click: name, role: "button" })
