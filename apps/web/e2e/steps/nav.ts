// Foundation: the shell. Where a place is, the wait every test starts
// with, the lock, and what any screen can raise (an alert, a blocking
// dialog). When the navigation changes, only this file does.

import type { Locator, Page } from "@playwright/test"

const sidebar = (page: Page) => page.getByRole("navigation", { name: "Sidebar" })

/** A place's entry, by an id that outlives its label. */
const PLACES = {
  all: /^All items/,
  personal: /^Personal \d+ items?$/,
  codes: "Codes",
  security: "Security",
  devices: "Devices",
  links: "Links",
  people: "People",
  settings: "Settings",
} as const

export type Place = keyof typeof PLACES

/** Goes to a place. */
export async function go(page: Page, place: Place) {
  await sidebar(page).getByRole("button", { name: PLACES[place] }).click()
}

/** The vault is open: its list has drawn. Wait on it after a sign-in, an unlock or a join. */
export const vaultOpen = (page: Page): Locator => page.getByRole("heading", { name: "All items" })

/** Locks the vault from the shell. */
export async function lock(page: Page) {
  await sidebar(page).getByRole("button", { name: "Lock" }).click()
}

/** The error a page, a form or a dialog shows. */
export const alert = (scope: Page | Locator): Locator => scope.getByRole("alert")

/** Dialogs that stop the person until answered. */
export const blockingDialogs = (page: Page): Locator => page.getByRole("alertdialog")

/** The page's root, where the theme is set. */
export const root = (page: Page): Locator => page.locator("html")
