// The edges lane: Codes, and signing in a TV or console (brief C2, doc 88).

import type { Locator, Page } from "@playwright/test"

/**
 * Opens Sign in a TV from Codes' page head and hands back its layer. The flow is
 * two layers in turn, the Form that reads the code and the Review that answers
 * it, and both carry the overline `CODES // TV SIGN-IN`: the locator follows
 * whichever shows (the newest, while one closes as the next opens).
 */
export async function openTvSignIn(page: Page): Promise<Locator> {
  await page.getByRole("button", { name: "Sign in TV", exact: true }).click()
  return page.getByRole("dialog").filter({ hasText: /TV sign-in/i }).last()
}

/** The host the scanned code names. */
export const scannedHost = (dialog: Locator, host: string): Locator => dialog.getByText(host, { exact: true })
/** The scanned code itself, as unicron's DigitGroups draws it: its groups, in order. */
export const scannedCode = (dialog: Locator, code: string): Locator =>
  dialog.locator('[data-slot="digit-groups"]').filter({ hasText: code.split(/[\s.-]+/).join("") }).first()

/** Picks the login to sign the TV in with. */
export async function pickLogin(dialog: Locator, name: RegExp) {
  await dialog.getByRole("radio", { name }).check()
}

/** Approves the sign-in on a host (the sheet takes the yes once its arming delay ends). */
export async function signInOn(dialog: Locator, host: string) {
  await dialog.getByRole("button", { name: `Sign in on ${host}`, exact: true }).click()
}

/** Every "Sign in on …" the dialog offers: a refused code offers none. */
export const signInOffers = (dialog: Locator): Locator => dialog.getByRole("button", { name: /Sign in on/ })

/** Pastes a code's address instead of scanning it, and checks it. */
export async function pasteAddress(dialog: Locator, address: string) {
  await dialog.getByRole("tab", { name: "Paste" }).click()
  await dialog.getByLabel("The code's address").fill(address)
  await dialog.getByRole("button", { name: "Check it" }).click()
}
