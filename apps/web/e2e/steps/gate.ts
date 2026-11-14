// Foundation: the gate a protected action raises.

import type { Page } from "@playwright/test"

/**
 * Answers the gate with the password it asks for. The field is found by its
 * exact name: from unicron 1.0 it has a "Show Account password" button
 * beside it. (This tab's route is the password; unicron's gate still calls
 * it the account password.)
 */
export async function answerGate(page: Page, password: string) {
  await page.getByLabel("Account password", { exact: true }).fill(password)
  await page.getByRole("button", { name: "Confirm" }).click()
}
