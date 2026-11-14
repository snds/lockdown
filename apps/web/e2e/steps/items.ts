// The vault lane: the list, an item's detail and what it holds.

import type { Locator, Page } from "@playwright/test"

/** Every row of the list whose name matches. */
export const rows = (page: Page, title: RegExp): Locator => page.getByRole("button", { name: title })
/** The first row of the list whose name matches. */
export const row = (page: Page, title: RegExp): Locator => rows(page, title).first()

/** An item's title anywhere it is drawn. */
export const titled = (page: Page, title: string): Locator => page.getByText(title)

/** Opens an item from the list. */
export async function openItem(page: Page, title: RegExp) {
  await row(page, title).click()
}

/** Presses one of the open item's own actions. */
export async function itemAction(page: Page, name: "Edit") {
  await page.getByRole("button", { name, exact: true }).click()
}

/** A named section of the open item (its template's name, "Exports and backups"). */
export const section = (page: Page, name: string): Locator => page.getByRole("region", { name })

/** A crown jewel's opt-in, inside its section. */
export const exportSwitch = (section: Locator): Locator => section.getByRole("switch", { name: "Include in exports and backups" })

/** Text that must not be on the page (a secret's plain value). */
export const plainText = (page: Page, text: string): Locator => page.getByText(text)

/** Shows a Wi-Fi item's guest card. */
export async function showGuestCard(page: Page) {
  await page.getByRole("button", { name: "Guest card" }).click()
}
/** Puts the guest card away. */
export async function hideGuestCard(page: Page) {
  await page.getByRole("button", { name: "Hide" }).click()
}
/** The guest card's code for a network. */
export const guestCode = (page: Page, network: string): Locator => page.getByRole("img", { name: `The code to join ${network}` })

/** The list's quiet notices (a shared vault that left something out). */
export const notices = (page: Page): Locator => page.getByRole("note")
/** A notice's own controls: it should have none. */
export const noticeControls = (notice: Locator): Locator => notice.getByRole("button")
