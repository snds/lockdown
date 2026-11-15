// Steps the edges lane owns: the new-item menu and the editor it opens, and a request's needs-you and review.

import type { Step } from "../../harness/types"

/** Opens the New item menu. */
export const newItemMenu: Step = { click: /^New item/ }
/** Starts a new item of one kind (or the menu's templates entry) from the New item menu: a menu beside the sidebar, a sheet of the same rows on a phone. */
export const newItem = (entry: RegExp): Step[] => [newItemMenu, { click: entry, within: '[data-adaptive="menu"]' }]

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
