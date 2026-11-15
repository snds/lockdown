// Steps the edges lane owns: the new-item menu and the editor it opens, and a request's needs-you and review.

import type { Step } from "../../harness/types"

/** Opens the New item menu. */
export const newItemMenu: Step = { click: /^New item/ }
/** Where a group's rows are: the desktop's submenu; on a phone (no submenu) the sheet itself. */
const GROUP_ROWS = '[data-slot="dropdown-menu-sub-content"]'
/** Opens one of the menu's four groups (brief E1): a submenu on a desktop, a closed Section Header on a phone. */
export const openGroup = (group: string): Step => ({
  run: async () => {
    const menu = document.querySelector('[data-adaptive="menu"]')
    const trigger = menu && [...menu.querySelectorAll<HTMLElement>('[aria-haspopup="menu"], [data-slot="section-header"] button, button[aria-expanded]')].find((e) => e.textContent?.trim().startsWith(group))
    if (!trigger) throw new Error(`no group ${group} in the menu`)
    trigger.focus()
    trigger.click()
    // A desktop submenu also opens from the keyboard, as a person would with the arrow key.
    if (trigger.getAttribute("aria-haspopup") === "menu") {
      await new Promise((r) => setTimeout(r, 100))
      if (!document.querySelector(GROUP_ROWS)) trigger.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", code: "ArrowRight", bubbles: true, cancelable: true }))
    }
  },
})
/**
 * Starts a new item of one kind (or "Your templates") from the New item menu: a menu beside the sidebar,
 * a sheet of the same rows on a phone. A kind outside the five first is in a group: name it.
 */
export const newItem = (entry: RegExp, group?: string): Step[] => (group ? [newItemMenu, openGroup(group), pickInGroup(entry)] : [newItemMenu, { click: entry, within: '[data-adaptive="menu"]' }])

/** Picks a row inside an opened group: the desktop's submenu once it shows, or the phone's sheet (never a row of the list behind it). */
const pickInGroup = (entry: RegExp): Step => ({
  run: async () => {
    for (let i = 0; i < 80; i++) {
      const root = document.querySelector(GROUP_ROWS) ?? document.querySelector('[data-slot="sheet-content"][data-adaptive="menu"]')
      const row = root && [...root.querySelectorAll<HTMLElement>('[role="menuitem"], [data-slot="adaptive-menu-item"]')].find((e) => entry.test(e.textContent?.trim() ?? ""))
      if (row) {
        row.click()
        return
      }
      await new Promise((r) => setTimeout(r, 50))
    }
    throw new Error(`no ${String(entry)} in the open group`)
  },
})
/** The editor's More options, opened. */
export const moreOptions: Step = { click: /^More options/ }

/** Picks an option of a native select the way a person would, so React hears the change. */
export function choose(selector: string, value: string) {
  const el = document.querySelector<HTMLSelectElement>(selector)
  if (!el) throw new Error(`no select at ${selector}`)
  Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value")!.set!.call(el, value)
  el.dispatchEvent(new Event("change", { bubbles: true }))
}

/** A request waits as the needs-you: the block wider, the strip on a phone (doc 84; brief X3). */
export const waits: Step[] = [{ see: '[data-needs-you-row="1"], [data-needs-you-row="2"], [data-needs-you-row="3"]' }]
/**
 * Presses the waiting request's REVIEW: the block's, the strip's, or, on a short screen where the strip is
 * compact and has no action of its own, the strip's text (one button that opens the same review, C5).
 */
export const pressReview: Step = {
  run: () => {
    const named = [...document.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent?.trim().toLowerCase() === "review" && b.getClientRects().length > 0)
    const strip = document.querySelector<HTMLElement>('[data-slot="accessory"][data-tone="needs-you"] [data-slot="accessory-open"]')
    const target = named ?? strip
    if (!target) throw new Error("no REVIEW to press")
    target.click()
  },
}
/** Opens the waiting request's review from its REVIEW, then lets the sheet arm (half a second). */
export const review: Step[] = [...waits, pressReview, { see: '[data-slot="approval-sheet"]' }, { pause: 700 }]
/** Lets go of focus, so the picture shows the screen and not the ring of the last press. */
export const rest: Step = { run: () => (document.activeElement as HTMLElement | null)?.blur() }

// --- Sharing (edges-7, doc 90) ----------------------------------------------------------

/** Waits until a predicate holds, a frame at a time, for up to 4 s. */
async function until<T>(find: () => T | null | undefined, what: string): Promise<T> {
  for (let i = 0; i < 80; i++) {
    const found = find()
    if (found) return found
    await new Promise((r) => setTimeout(r, 50))
  }
  throw new Error(`no ${what}`)
}

const shown = (e: Element) => (e as HTMLElement).getClientRects().length > 0
const buttonNamed = (name: string | RegExp, root: ParentNode = document) =>
  [...root.querySelectorAll<HTMLElement>("button, [role=\"button\"]")].find((b) => shown(b) && (typeof name === "string" ? (b.getAttribute("aria-label") ?? b.textContent?.trim()) === name : name.test(b.getAttribute("aria-label") ?? b.textContent?.trim() ?? "")))

/** Opens a row of a Sharing list (People, Vaults) by its title: its detail beside the list, or pushed over it. */
export const openRow = (list: "People" | "Vaults", title: string): Step => ({
  run: async () => {
    const row = await until(() => {
      const l = document.querySelector(`[role="list"][aria-label="${list}"]`)
      return l && buttonNamed(title, l)
    }, `${title} in ${list}`)
    row.click()
    await until(() => document.querySelector(`section[aria-label="${list}: open"]`), `${list}'s detail`)
  },
})

/** Opens a person's row only where the detail stands beside the list (the expanded width), so a phone keeps its list. */
export const openRowBeside = (list: "People" | "Vaults", title: string): Step => ({
  run: async () => {
    if (window.innerWidth < 1024) return
    await (openRow(list, title) as { run: () => Promise<void> }).run()
  },
})

/**
 * Runs one of the open person's actions: the detail's own button, the phone's bottom row (COMPARE), or the
 * top bar's ⋯ menu (CHECK WITH A PHRASE, FORGET), as the shell draws them at this width.
 */
export const personAction = (name: "Compare fingerprints" | "Check with a phrase" | "Forget"): Step => ({
  run: async () => {
    const direct = buttonNamed(name) ?? (name === "Compare fingerprints" ? buttonNamed("Compare") : undefined)
    if (direct) {
      direct.click()
      return
    }
    const more = await until(() => buttonNamed(/^More actions for /), "the top bar's More actions")
    more.click()
    const item = await until(
      () => [...document.querySelectorAll<HTMLElement>('[role="menuitem"], [data-slot="adaptive-menu-item"]')].find((e) => shown(e) && e.textContent?.trim() === name),
      `${name} in the menu`,
    )
    item.click()
  },
})
