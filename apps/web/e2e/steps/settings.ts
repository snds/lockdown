// The screens lane: Settings in the web vault.

import type { Locator, Page } from "@playwright/test"

/** One choice of Appearance → Theme: System, Stone or Graphite. */
export const theme = (page: Page, name: "System" | "Stone" | "Graphite"): Locator => page.getByRole("radio", { name })

/** Signs this tab out. */
export async function signOut(page: Page) {
  await page.getByRole("button", { name: "Sign out of this tab" }).click()
}

/** A line of This server, by words it holds. */
export const serverLine = (page: Page, words: string): Locator => page.getByText(words, { exact: false })
