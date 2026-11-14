// Foundation: the shell. Where a place is, the wait every test starts
// with, the lock, and what any screen can raise (an alert, a blocking
// dialog). When the navigation changes, only this file does.
//
// The shell is one of three (docs/plan/64-redesign-places.md): the sidebar
// at the expanded width, its rail at medium width, the phone's floating rows
// under 600 px. `go` looks at which is drawn and takes that shell's path, so
// a test reads the same at every width.

import type { Locator, Page } from "@playwright/test"

/** The navigation column, by the name it has kept: the whole sidebar, or its rail. */
const sidebar = (page: Page) => page.getByRole("navigation", { name: "Sidebar" }).and(page.locator('[data-slot="app-frame-sidebar"]'))

type Shell = "sidebar" | "rail" | "phone"
async function shell(page: Page): Promise<Shell> {
  const nav = sidebar(page)
  if ((await nav.count()) === 0) return "phone"
  return (await nav.first().getAttribute("data-state")) === "rail" ? "rail" : "sidebar"
}

/** A name that starts with these words (a row before its count). */
const starts = (words: string) => new RegExp(`^${words}(,|$)`)

/** A row of the sidebar or an item of its rail: a tree item in ITEMS and VAULTS, a button elsewhere. */
const sideRow = (page: Page, name: RegExp) => sidebar(page).getByRole("treeitem", { name }).or(sidebar(page).getByRole("button", { name })).first()

/** One of the three sections: a sidebar row, a rail item, or a tab. */
async function section(page: Page, label: string, sidebarLabel = label) {
  const s = await shell(page)
  if (s === "phone") {
    await toTabs(page)
    await page.getByRole("tab", { name: starts(label) }).click()
  } else await sideRow(page, starts(s === "sidebar" ? sidebarLabel : label)).click()
}

/** On a phone a pushed screen (Settings, Sharing, an open item) has no tab bar: back to one. */
async function toTabs(page: Page) {
  const tabs = page.getByRole("tablist", { name: "Sections" })
  for (let i = 0; i < 4 && !(await tabs.isVisible()); i++) await page.getByRole("button", { name: "Back" }).click()
}

/** The layer the page head opens where the sidebar's groups don't show: the vaults, Tags and Sharing. */
async function switcher(page: Page): Promise<Locator> {
  await section(page, "Items", "All items")
  await page.getByRole("button", { name: /Switch vault$/ }).click()
  return page.getByRole("dialog", { name: "Switch vault" })
}

/** A row of the vaults tree: in the sidebar, or in the switcher. */
async function treeRow(page: Page, name: RegExp) {
  if ((await shell(page)) === "sidebar") await sidebar(page).getByRole("treeitem", { name }).click()
  else await (await switcher(page)).getByRole("list", { name: "Vaults" }).getByRole("button", { name }).click()
}

async function sharing(page: Page, tab: string) {
  if ((await shell(page)) === "sidebar") await sidebar(page).getByRole("treeitem", { name: starts("Sharing") }).click()
  else await (await switcher(page)).getByRole("button", { name: starts("Sharing") }).click()
  await page.getByRole("tab", { name: tab, exact: true }).click()
}

/** Settings' own list of sections. */
const sections = (page: Page) => page.getByRole("navigation", { name: "Settings sections" })

async function settings(page: Page) {
  const s = await shell(page)
  if (s !== "phone") return sideRow(page, starts("Settings")).click()
  // The profile button, on a section's first screen; from a Settings section, Back to the list.
  if (await sections(page).isVisible()) return
  const profile = page.getByRole("button", { name: "Settings", exact: true })
  if (!(await profile.isVisible())) await page.getByRole("button", { name: "Back" }).click()
  if (await profile.isVisible()) await profile.click()
}

async function settingsSection(page: Page, label: string) {
  await settings(page)
  await sections(page).getByRole("button", { name: starts(label) }).click()
}

/** The device's own section: first in the list beside it; the rows above the list where a section is pushed. */
async function thisBrowser(page: Page) {
  await settings(page)
  const row = sections(page).getByRole("button", { name: "This browser", exact: true })
  if (await row.count()) await row.click()
}

/** A place, by an id that outlives its label. */
const PLACES = {
  all: (page: Page) => section(page, "Items", "All items"),
  personal: (page: Page) => treeRow(page, starts("Personal")),
  codes: (page: Page) => section(page, "Codes"),
  security: (page: Page) => section(page, "Health"),
  links: (page: Page) => sharing(page, "Links"),
  people: (page: Page) => sharing(page, "People"),
  settings,
  "this-browser": thisBrowser,
  appearance: (page: Page) => settingsSection(page, "Appearance"),
  devices: (page: Page) => settingsSection(page, "Devices"),
  server: (page: Page) => settingsSection(page, "Server"),
} as const
export type Place = keyof typeof PLACES

/** Goes to a place. */
export async function go(page: Page, place: Place) {
  await PLACES[place](page)
}

/** The vault is open: its list has drawn ("All items" beside the sidebar, "Items" where the page head is the switcher). Wait on it after a sign-in, an unlock or a join. */
export const vaultOpen = (page: Page): Locator => page.getByRole("heading", { name: /^(All items|Items)$/ })

/** Locks the vault from the shell: beside Settings in the sidebar and its rail, in Settings' top bar on a phone. */
export async function lock(page: Page) {
  // Which shell is drawn is read once: once the vault locks no sidebar is drawn, which would read as a phone.
  if ((await shell(page)) === "phone") {
    await settings(page)
    await page.getByRole("button", { name: "Lock", exact: true }).click()
  } else await sidebar(page).getByRole("button", { name: "Lock", exact: true }).click()
}

/** The error a page, a form or a dialog shows. */
export const alert = (scope: Page | Locator): Locator => scope.getByRole("alert")

/** Dialogs that stop the person until answered. */
export const blockingDialogs = (page: Page): Locator => page.getByRole("alertdialog")

/** The page's root, where the theme is set. */
export const root = (page: Page): Locator => page.locator("html")
