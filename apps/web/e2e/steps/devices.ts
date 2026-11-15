// The edges lane: Devices and sessions, and adding a device (brief T2, doc 85).

import type { Locator, Page } from "@playwright/test"

/** Presses ADD DEVICE and hands back the dialog that offers the code. */
export async function addDevice(page: Page): Promise<Locator> {
  await page.getByRole("button", { name: "Add device", exact: true }).click()
  return page.getByRole("dialog", { name: "Add a device" })
}

/** The code the new device types. */
export const deviceCode = (offer: Locator): Locator => offer.getByLabel("Device code", { exact: true })

/**
 * The compare review the offer becomes once the new device joins: unicron's
 * ApprovalSheet, named by its title, "Add {the new device's name}?".
 */
export const compareDialog = (page: Page): Locator => page.getByRole("dialog", { name: /^Add .+\?$/ })

/** Says the digits match (THEY MATCH); the gate comes next. */
export async function approve(compare: Locator) {
  await compare.getByRole("button", { name: "They match" }).click()
}

/** The heading over where the account is signed in (SIGNED IN and its count). */
export const signedInHeading = (page: Page): Locator => page.getByRole("heading", { name: /^Signed in/ })

/** A device's row, by its name. */
export const deviceRow = (page: Page, name: string): Locator => page.getByRole("list", { name: "Devices" }).getByRole("listitem").filter({ hasText: name })
