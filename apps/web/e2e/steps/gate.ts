// Foundation: the gate a protected action raises (brief X5, doc 68).

import type { Page } from "@playwright/test"

/**
 * Answers the gate with the password it asks for. The gate is unicron's
 * `CrownJewelGate`, a question-size dialog named "Confirm it's you" with the
 * reason under its title ("To share this vault."). The field is found inside
 * that dialog by its exact name: it has a "Show Account password" button
 * beside it. (This tab's route is the password; unicron's gate still calls it
 * the account password, a unicron gap in doc 68.) A wrong try would say
 * "That isn't the master password. 2 tries left." in the same dialog.
 */
export async function answerGate(page: Page, password: string) {
  const gate = page.getByRole("dialog", { name: "Confirm it's you" })
  await gate.getByLabel("Account password", { exact: true }).fill(password)
  await gate.getByRole("button", { name: "Confirm" }).click()
}
