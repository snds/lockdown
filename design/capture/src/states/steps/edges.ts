// Steps the edges lane owns: the new-item menu and the editor it opens.

import type { Step } from "../../harness/types"

/** Opens the New item menu. */
export const newItemMenu: Step = { click: /^New item/ }
/** Starts a new item of one kind (or the menu's templates entry) from the New item menu. */
export const newItem = (entry: RegExp): Step[] => [newItemMenu, { click: entry, role: "menuitem" }]

/** Picks an option of a native select the way a person would, so React hears the change. */
export function choose(selector: string, value: string) {
  const el = document.querySelector<HTMLSelectElement>(selector)
  if (!el) throw new Error(`no select at ${selector}`)
  Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value")!.set!.call(el, value)
  el.dispatchEvent(new Event("change", { bubbles: true }))
}
