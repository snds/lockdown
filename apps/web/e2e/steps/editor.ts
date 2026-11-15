// The edges lane: the New item menu, the editor, the generator and the
// templates manager.

import type { Locator, Page } from "@playwright/test"

/** Starts a new item from the New item menu; `first` when the name matches more than one entry. */
export async function newItem(page: Page, entry: RegExp, first = false) {
  await page.getByRole("button", { name: "New item" }).click()
  const item = page.getByRole("menuitem", { name: entry })
  await (first ? item.first() : item).click()
}

/** The menu entry for a plain Login (its icon's name may come first). */
export const LOGIN = /^\W*\w*\s*Login$/

/** The open editor (or the templates manager: both are the one dialog). */
export const editor = (page: Page): Locator => page.getByRole("dialog")

/** A field of the editor by its label; `exact` when another label holds the same words. */
export const field = (dialog: Locator, label: string, exact = false): Locator => (exact ? dialog.getByLabel(label, { exact: true }) : dialog.getByLabel(label))

/** The first of a repeated field (Website). */
export const firstField = (dialog: Locator, label: string): Locator => dialog.getByLabel(label).first()

/** Adds the new item. */
export async function add(dialog: Locator) {
  await dialog.getByRole("button", { name: "Add", exact: true }).click()
}

/** Saves the edit. */
export async function save(dialog: Locator) {
  await dialog.getByRole("button", { name: "Save", exact: true }).click()
}

// --- the generator ------------------------------------------------------------------

/** Generates a password and opens the generator's options. */
export async function generateWithOptions(dialog: Locator) {
  await dialog.getByRole("button", { name: "Generate password" }).click()
  await dialog.getByRole("button", { name: "Options" }).click()
}

/** What the generator makes: characters or words. */
export const generatorMake = (dialog: Locator): Locator => dialog.getByLabel("Make")
/** The generator's strength line. */
export const generatorStrength = (dialog: Locator): Locator => dialog.getByRole("status")

// --- templates of your own -------------------------------------------------------------

/** Makes a template with one field, in the templates manager. */
export async function makeTemplate(manager: Locator, name: string, fieldLabel: string) {
  await manager.getByRole("button", { name: "New template" }).click()
  await manager.getByLabel("Name").fill(name)
  await manager.getByLabel("Field 1's label").fill(fieldLabel)
  await manager.getByRole("button", { name: "Save template" }).click()
}

/** A template in the manager's list. */
export const templateNamed = (manager: Locator, name: string): Locator => manager.getByText(name)

/** Closes the templates manager. */
export async function closeManager(manager: Locator) {
  await manager.getByRole("button", { name: "Close", exact: true }).first().click()
}
