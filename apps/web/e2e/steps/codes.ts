// The edges lane: Codes, and signing in a TV or console.

import type { Locator, Page } from "@playwright/test"

/** Opens Sign in a TV from Codes and hands back its dialog. */
export async function openTvSignIn(page: Page): Promise<Locator> {
  await page.getByRole("button", { name: "Sign in a TV" }).click()
  return page.getByRole("dialog", { name: "Sign in a TV or console" })
}

/** The host the scanned code names. */
export const scannedHost = (dialog: Locator, host: string): Locator => dialog.getByText(host, { exact: true })
/** The scanned code itself. */
export const scannedCode = (dialog: Locator, code: string): Locator => dialog.getByText(code)

/** Picks the login to sign the TV in with. */
export async function pickLogin(dialog: Locator, name: RegExp) {
  await dialog.getByRole("radio", { name }).check()
}

/** Approves the sign-in on a host. */
export async function signInOn(dialog: Locator, host: string) {
  await dialog.getByRole("button", { name: `Sign in on ${host}` }).click()
}

/** Every "Sign in on …" the dialog offers: a refused code offers none. */
export const signInOffers = (dialog: Locator): Locator => dialog.getByRole("button", { name: /Sign in on/ })

/** Pastes a code's address instead of scanning it, and checks it. */
export async function pasteAddress(dialog: Locator, address: string) {
  await dialog.getByRole("tab", { name: "Paste" }).click()
  await dialog.getByLabel("The code's address").fill(address)
  await dialog.getByRole("button", { name: "Check it" }).click()
}
