// The vault lane: the list's search and the quick-access palette.

import type { Locator, Page } from "@playwright/test"

/** The list's search field. */
export const search = (page: Page): Locator => page.getByRole("searchbox", { name: "Search" })

/** Opens quick access with its key and hands back its field. */
export async function openPalette(page: Page): Promise<Locator> {
  await page.keyboard.press("Control+k")
  return page.getByRole("dialog", { name: "Quick access" }).getByPlaceholder(/Search titles/)
}

/** Quick access's results: all of them, or those whose name matches. */
export const paletteResults = (page: Page, name?: RegExp): Locator => (name ? page.getByRole("option", { name }) : page.getByRole("option"))
