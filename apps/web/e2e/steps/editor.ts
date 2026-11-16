// The edges lane: the New item menu, the editor, the generator and the
// templates manager (doc 89).

import type { Locator, Page } from "@playwright/test"

/** A row's name without its count (`Login, 14` reads `Login`), as the menu says it before the number. */
async function rowName(row: Locator): Promise<string> {
  return row.evaluate((el) => {
    const copy = el.cloneNode(true) as HTMLElement
    copy.querySelectorAll('[aria-hidden="true"]').forEach((hidden) => hidden.remove())
    return (copy.textContent ?? "").replace(/\s+/g, " ").replace(/,\s*[\d,]+$/, "").trim()
  })
}

/** The first row of `rows` whose name matches. */
async function find(rows: Locator, entry: RegExp): Promise<Locator | null> {
  const n = await rows.count()
  for (let i = 0; i < n; i++) {
    const row = rows.nth(i)
    if (entry.test(await rowName(row))) return row
  }
  return null
}

/**
 * Starts a new item from the New item menu (brief E1): one of the five
 * first, or a kind inside one of the four groups (a submenu on a
 * desktop). `first` is kept for callers that name a kind more than one row
 * could match; the first match is taken. "Make a template" is the foot's
 * "Your templates" now. Once the editor opens, More options is opened too,
 * so every field a step names can be reached.
 */
export async function newItem(page: Page, entry: RegExp, first = false) {
  void first
  await page.getByRole("button", { name: "New item" }).click()
  const menu = page.locator('[data-adaptive="menu"]')
  await menu.getByRole("menuitem").first().waitFor()
  if (/template/i.test(entry.source) && !(await find(menu.getByRole("menuitem"), entry))) {
    await menu.getByRole("menuitem", { name: "Your templates", exact: true }).click()
    return
  }
  let row = await find(menu.getByRole("menuitem"), entry)
  if (!row) {
    // Not among the first rows: look in each group's submenu.
    const count = await menu.locator('[aria-haspopup="menu"]').count()
    for (let i = 0; i < count && !row; i++) {
      if (i > 0) {
        // One submenu at a time: close the menu and open it again before the next group.
        while (await menu.isVisible()) await page.keyboard.press("Escape")
        await page.getByRole("button", { name: "New item" }).click()
        await menu.getByRole("menuitem").first().waitFor()
      }
      await menu.locator('[aria-haspopup="menu"]').nth(i).click()
      const sub = page.locator('[data-slot="dropdown-menu-sub-content"]').last()
      await sub.getByRole("menuitem").first().waitFor()
      row = await find(sub.getByRole("menuitem"), entry)
    }
  }
  if (!row) throw new Error(`New item has no row for ${String(entry)}`)
  await row.click()
  await openMoreOptions(editor(page))
}

/** Opens the editor's More options (brief E2), where the title of a login, the organization block and the rest wait. */
export async function openMoreOptions(dialog: Locator) {
  const more = dialog.getByRole("button", { name: /^More options/ })
  await more.waitFor()
  if ((await more.getAttribute("aria-expanded")) !== "true") await more.click()
}

/** The menu entry for a plain Login (its icon's name may come first). */
export const LOGIN = /^\W*\w*\s*Login$/

/** The open editor (or the templates manager: both are a dialog). The generator's picker is a dialog of its own, outside it. */
export const editor = (page: Page): Locator => page.getByRole("dialog").and(page.locator(':not([data-adaptive="popover"])'))

/** A field of the editor by its label; `exact` when another label holds the same words. */
export const field = (dialog: Locator, label: string, exact = false): Locator => (exact ? dialog.getByLabel(label, { exact: true }) : dialog.getByLabel(label))

/** The first of a repeated field (Website). */
export const firstField = (dialog: Locator, label: string): Locator => dialog.getByLabel(label).first()

/** The generator's picker, when it is open. */
const picker = (dialog: Locator): Locator => dialog.page().locator('[data-adaptive="popover"]')

/** Closes the generator's picker if it is open: its changes are already applied. */
async function closePicker(dialog: Locator) {
  if (await picker(dialog).isVisible()) {
    await dialog.page().keyboard.press("Escape")
    await picker(dialog).waitFor({ state: "hidden" })
  }
}

/** Adds the new item: its SAVE {KIND}. */
export async function add(dialog: Locator) {
  await closePicker(dialog)
  await dialog.getByRole("button", { name: /^Save .+/ }).click()
}

/** Saves the edit. */
export async function save(dialog: Locator) {
  await closePicker(dialog)
  await dialog.getByRole("button", { name: "Save", exact: true }).click()
}

// --- the generator ------------------------------------------------------------------

/** Has the core make the password on save, and opens the generator's options beside it (brief E3). */
export async function generateWithOptions(dialog: Locator) {
  await dialog.getByRole("button", { name: "Generate password" }).click()
  await dialog.getByRole("button", { name: "Options for password" }).click()
  await picker(dialog).waitFor()
}

/** What the generator makes: characters or words. */
export const generatorMake = (dialog: Locator): Locator => picker(dialog).getByLabel("Make")
/** The generator's strength line. */
export const generatorStrength = (dialog: Locator): Locator => picker(dialog).getByRole("status")

// --- templates of your own -------------------------------------------------------------

/** Makes a template with one field, in the templates manager. */
export async function makeTemplate(manager: Locator, name: string, fieldLabel: string) {
  await manager.getByRole("button", { name: "New template" }).click()
  await manager.getByLabel("Name", { exact: true }).fill(name)
  await manager.getByLabel("Field 1's label").fill(fieldLabel)
  await manager.getByRole("button", { name: "Save template" }).click()
}

/** A template in the manager's list. */
export const templateNamed = (manager: Locator, name: string): Locator => manager.getByRole("list", { name: "Your templates" }).getByText(name)

/** Closes the templates manager. */
export async function closeManager(manager: Locator) {
  await manager.getByRole("button", { name: "Close", exact: true }).first().click()
}

/**
 * A new login's password is made by the core when it is saved (Lockdown 29);
 * to type one, the person presses "Type one instead" first.
 */
export async function typeOwnPassword(dialog: Locator): Promise<Locator> {
  await dialog.getByRole("button", { name: "Type one instead" }).click()
  return field(dialog, "Password", true)
}
