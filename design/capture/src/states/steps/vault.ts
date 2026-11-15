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

/**
 * Answers the open item's conflict (brief I8, X3 row 5): the needs-you block's button on a desktop; on a
 * phone the strip (its RESOLVE, or the strip itself where a phone on its side drops the word), then the sheet's button. Either opens the Question.
 */
export const keepConflictCopy = (which: "Keep this copy" | "Keep other copy"): Step => ({
  run: async () => {
    if (!shown(which)) {
      // The strip's own button opens the sheet; a phone on its side draws the strip without RESOLVE.
      const strip = document.querySelector<HTMLElement>('[data-needs-you-row="5"] button')
      if (!strip) throw new Error("no needs-you strip for the conflict")
      strip.click()
      await runSteps([{ see: '[role="dialog"]' }, { pause: 300 }])
    }
    await runSteps([{ click: which, role: "button" }])
  },
})

/** Reveals a masked field of the open item (Show with the toggle setting, Hold to show with hold). */
export const reveal = (label: string): Step => ({ click: new RegExp(`^(Show|Reveal) ${label}`) })

/** Opens a passkey's details on the open item, by its site or the first one. */
export const passkeyDetails = (site?: string): Step => ({ click: site ? `Details of the passkey for ${site}` : /^Details of the passkey for /, role: "button" })

/** A device named on Health's quantum-safe line: pressing it brings the device into view on Devices. */
export const healthDevice = (name: string): Step => ({ click: name, role: "button" })

// --- vaults, folders and tags (brief V2, doc 71) -------------------------------------------

/** Any vault, folder or tag menu that is open: a menu beside the sidebar and in the popover, a sheet on a phone. */
export const ORG_MENU: Step = { see: '[data-adaptive="menu"]' }

/** The vaults tree's row by its name: the sidebar's tree item, or the switcher's row button. */
function orgRow(name: string): HTMLElement | null {
  const label = (el: HTMLElement) => el.getAttribute("aria-label") ?? ""
  const named = (el: HTMLElement) => label(el) === name || label(el).startsWith(`${name},`)
  const shown = (el: HTMLElement) => el.offsetParent !== null
  const inLayer = Array.from(document.querySelectorAll<HTMLElement>('[role="dialog"] [role="list"][aria-label="Vaults"] [data-slot="nav-item-main"]')).filter(shown)
  const inSidebar = Array.from(document.querySelectorAll<HTMLElement>('nav[aria-label="Sidebar"] [role="tree"] [role="treeitem"]')).filter(shown)
  return [...inLayer, ...inSidebar].find(named) ?? null
}

/**
 * Opens a vault's folders, or the tag tree, without choosing it: the switcher's Expand button (or the
 * Tags row), the chevron beside the sidebar's tree item. An open branch stays open.
 */
export const openBranch = (name: string): Step => ({
  run: async () => {
    const start = performance.now()
    while (!orgRow(name) && performance.now() - start < 6000) await new Promise((r) => setTimeout(r, 50))
    const row = orgRow(name)
    if (!row) throw new Error(`no vaults row named ${name}`)
    const disclosure = row.parentElement?.querySelector<HTMLElement>(':scope > [data-slot="nav-item-disclosure"]')
    const state = disclosure?.getAttribute("aria-expanded") ?? row.getAttribute("aria-expanded")
    // The sidebar's chevron is the pointer's way (aria-hidden), so it is read from the tree item.
    const open = state ?? row.getAttribute("aria-expanded")
    if (open === "true") return
    ;(disclosure ?? row).click()
    await new Promise((r) => setTimeout(r, 150))
  },
})

/** Opens the ⋯ menu of a vault, folder or tag row ("Personal", "Finance", "#travel"). */
export const orgMenu = (name: string): Step[] => [{ click: `${name} options` }, ORG_MENU, { pause: 250 }]

/** A row of the open vault, folder or tag menu: a menu item, or a row button in a phone's sheet. */
export const orgMenuRow = (row: string): Step => ({
  run: async () => {
    const menu = document.querySelector('[role="menu"]')
    await runSteps([{ click: row, role: menu ? (row.endsWith(" days") || row === "A year" ? "menuitemradio" : "menuitem") : row.endsWith(" days") || row === "A year" ? "radio" : "button" }])
  },
})

/** Shows a row's ⋯ the way a pointer finds it: focus on the row (its action shows on hover and focus). */
export const focusOrgRow = (name: string): Step => ({
  run: async () => {
    orgRow(name)?.focus()
    await new Promise((r) => setTimeout(r, 150))
  },
})

/** Steps that only mean something beside the sidebar's tree (the expanded width); elsewhere nothing. */
export const besideSidebar = (steps: Step[]): Step => ({
  run: async () => {
    const tree = document.querySelector<HTMLElement>('nav[aria-label="Sidebar"] [role="tree"]')
    if (tree && tree.offsetParent !== null) await runSteps(steps)
  },
})

/**
 * A finger's swipe on a phone's item row (brief V1, States / 01): pointer events on unicron's
 * `SwipeRow` around the row whose title is `title`, far enough to open its tiles and short of a full
 * swipe, so nothing runs. `trailing` slides it left (MORE, CODE, the copy); `leading` slides it right
 * from 60 px in, clear of the back edge (FAVORITE). Then the row settles.
 */
export const swipeRow = (title: string, side: "trailing" | "leading"): Step => ({
  run: async () => {
    // The title line holds the title, then its marks' names ("Favorite"): never more words after a space.
    const line = Array.from(document.querySelectorAll<HTMLElement>('[data-slot="swipe-row"] [data-slot="list-row-title"]')).find((t) => {
      const text = t.textContent ?? ""
      return text.startsWith(title) && !/^\s/.test(text.slice(title.length))
    })
    const root = line?.closest<HTMLElement>('[data-slot="swipe-row"]')
    if (!root) throw new Error(`No swiping row named ${title}`)
    const box = root.getBoundingClientRect()
    const y = box.top + box.height / 2
    const from = side === "trailing" ? box.right - 30 : box.left + 60
    const to = side === "trailing" ? from - 180 : from + 110
    const at = (type: string, x: number) =>
      root.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, pointerId: 41, pointerType: "touch", isPrimary: true, button: type === "pointermove" ? -1 : 0, clientX: x, clientY: y }))
    at("pointerdown", from)
    for (let i = 1; i <= 10; i++) {
      at("pointermove", from + ((to - from) * i) / 10)
      await new Promise((r) => setTimeout(r, 16))
    }
    at("pointerup", to)
    await new Promise((r) => setTimeout(r, 500))
  },
})

// --- an item's Details, files and history (brief I4 to I6, vault-6) ---------------------------

/**
 * Scrolls the open item until a part of it is at the top of its column: the Details header
 * (`Details`), a section by its name (`Files`), or a Field Row by its label. Waits for it first.
 */
export const bringIntoView = (name: string): Step => ({
  run: async () => {
    const find = () =>
      Array.from(document.querySelectorAll<HTMLElement>('[data-slot="section-header-trigger"], section[aria-label], [data-slot="field-row"]')).find((el) => {
        const label = el.getAttribute("aria-label") ?? el.textContent?.replace(/\s+/g, " ").trim() ?? ""
        return el.offsetParent !== null && (label === name || label.startsWith(`${name} `) || el.querySelector(`[id="${el.getAttribute("aria-labelledby") ?? ""}"]`)?.textContent === name)
      })
    const start = performance.now()
    while (!find() && performance.now() - start < 6000) await new Promise((r) => setTimeout(r, 50))
    const el = find()
    if (!el) throw new Error(`nothing named ${name} on the open item`)
    el.scrollIntoView({ block: "start" })
    // The click that opened Details leaves its focus ring; a picture of the zone shows it at rest.
    if (document.activeElement instanceof HTMLElement && document.activeElement.closest('[data-slot="section-header"]')) document.activeElement.blur()
    await new Promise((r) => setTimeout(r, 300))
  },
})

/** Presses a Details row's own action ("Change", "Move", "History") by the row's label. */
export const detailsRowAction = (row: string, action: string): Step => ({
  run: async () => {
    await runSteps([openDetails(), bringIntoView(row)])
    const el = Array.from(document.querySelectorAll<HTMLElement>('[data-slot="field-row"]')).find(
      (r) => r.offsetParent !== null && document.getElementById(r.getAttribute("aria-labelledby") ?? "")?.textContent === row,
    )
    const button = Array.from(el?.querySelectorAll<HTMLElement>("button") ?? []).find((b) => b.textContent?.trim() === action)
    if (!button) throw new Error(`no ${action} on the ${row} row`)
    button.click()
    await new Promise((r) => setTimeout(r, 300))
  },
})

// --- Health (brief H1 to H4, V3) ------------------------------------------------------------------

/** Opens a row of Health's lists by its title (a finding's item, an item in Needs attention or Expiring), and waits for its detail. */
export const healthRow = (title: string): Step[] => [{ click: starts(title), within: "main" }, { wait: title, role: "heading" }]

/** SKIP FOR NOW's menu on the open finding: its button beside the fix on a desktop, the top bar's ⋯ on a phone. */
export const skipForNow = (): Step => ({
  run: async () => {
    if (shown(/^Skip for now/)) await runSteps([{ click: /^Skip for now/, role: "button" }])
    else await runSteps([{ click: "More for this finding", role: "button" }])
    await runSteps([{ see: '[role="menu"], [role="dialog"]' }, { pause: 200 }])
  },
})

/** One of Health's MORE HELP rows, or the Find of the Duplicates row, by its name. */
export const healthPlace = (name: "Accounts you sign in with" | "Something happened?" | "Find duplicates"): Step[] => [{ click: name, role: "button" }, { pause: 300 }]
