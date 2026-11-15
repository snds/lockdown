// The edges lane: Devices and sessions, and adding a device.

import type { Locator, Page } from "@playwright/test"

/** Opens Add a device and hands back the dialog that offers the code. */
export async function addDevice(page: Page): Promise<Locator> {
  await page.getByRole("button", { name: "Add a device" }).click()
  return page.getByRole("dialog", { name: "Add a device" })
}

/** The code the new device types. */
export const deviceCode = (offer: Locator): Locator => offer.getByLabel("Device code")

/** The dialog that asks whether both devices show the same digits. */
export const compareDialog = (page: Page): Locator => page.getByRole("dialog", { name: "Do the digits match?" })

/** Says the digits match; the gate comes next. */
export async function approve(compare: Locator) {
  await compare.getByRole("button", { name: "Approve" }).click()
}

/** The heading over where the account is signed in. */
export const signedInHeading = (page: Page): Locator => page.getByText("Signed in", { exact: true })

/** A device's row, by its name. */
export const deviceRow = (page: Page, name: string): Locator => page.getByRole("listitem").filter({ hasText: name })
