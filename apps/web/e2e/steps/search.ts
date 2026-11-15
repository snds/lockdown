// The vault lane: the list's search and the quick-access palette.

import type { Locator, Page } from "@playwright/test"

/** The list's search field. */
export const search = (page: Page): Locator => page.getByRole("searchbox", { name: "Search" })

/** Opens quick access with its key and hands back its field. */
export async function openPalette(page: Page): Promise<Locator> {
  await page.keyboard.press("Control+k")
  return page.getByRole("dialog", { name: "Quick access" }).getByPlaceholder("Search your items and places")
}

/**
 * Quick access's item results: all of them, or those whose name matches.
 * Only ITEMS: GO TO's places and the last row ("Search everything for …")
 * are options too (brief X4).
 */
export const paletteResults = (page: Page, name?: RegExp): Locator => {
  const items = page.getByRole("dialog", { name: "Quick access" }).getByRole("group", { name: "Items" })
  return name ? items.getByRole("option", { name }) : items.getByRole("option")
}
