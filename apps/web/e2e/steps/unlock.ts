// The screens lane: the pages before the vault and the pages without one.
// Sign-in, joining with a device, unlock, and the share and request link
// pages a person with no account opens.

import type { Locator, Page } from "@playwright/test"

// --- sign in ---------------------------------------------------------------------

export const signInForm = (page: Page): Locator => page.getByRole("form", { name: "Sign in" })

/** Signs in as a new device: the login name, the password and the kit's Secret Key. */
export async function signIn(page: Page, login: string, password: string, secretKey: string) {
  const form = signInForm(page)
  await form.getByLabel("Login name").fill(login)
  await form.getByLabel("Master password").fill(password)
  await form.getByLabel("Secret Key").fill(secretKey)
  await form.getByRole("button", { name: "Sign in" }).click()
}

/** The sign-in page is showing: its first tab. */
export const signInTab = (page: Page): Locator => page.getByRole("tab", { name: "Sign in" })

// --- join with a device ------------------------------------------------------------

/** Types the code another device shows, on the sign-in page's "With a device" tab. */
export async function joinWithCode(page: Page, code: string) {
  await page.getByRole("tab", { name: "With a device" }).click()
  await page.getByLabel("Code from the other device").fill(code)
  await page.getByRole("button", { name: "Join", exact: true }).click()
}

/** The digits both devices show, on the page or in a dialog. */
export const digitsToCompare = (scope: Page | Locator): Locator => scope.getByLabel("Digits to compare")

/** After the other device approved: the login name the join finishes with. */
export const joinLoginName = (page: Page): Locator => page.getByLabel("Login name")
/** The line that says the other device approved. */
export const joinApproved = (page: Page): Locator => page.getByText(/Approved\./)

/** Finishes a join with the login name and the password alone. */
export async function finishJoin(page: Page, login: string, password: string) {
  await page.getByLabel("Login name").fill(login)
  await page.getByLabel("Master password").fill(password)
  await page.getByRole("button", { name: "Finish" }).click()
}

// --- unlock ----------------------------------------------------------------------

export const unlockForm = (page: Page): Locator => page.getByRole("form", { name: "Unlock" })

/** Answers the unlock screen. */
export async function unlock(page: Page, password: string) {
  const form = unlockForm(page)
  await form.getByLabel("Master password").fill(password)
  await form.getByRole("button", { name: "Unlock" }).click()
}

// --- a share or request link, opened with no account ---------------------------------

export const sharedHeading = (page: Page): Locator => page.getByRole("heading", { name: "Someone shared something with you" })

/** Opens a share link with its password. */
export async function openLink(page: Page, password: string) {
  const form = page.getByRole("form", { name: "Open the link" })
  await form.getByLabel("The link's password").fill(password)
  await form.getByRole("button", { name: "Open" }).click()
}

/** The opened link's title. */
export const linkTitle = (page: Page, title: string): Locator => page.getByRole("heading", { name: title })

/** Shows one of the opened link's values. */
export async function showLinkValue(page: Page, label: string) {
  await page.getByRole("button", { name: `Show ${label}` }).click()
}

/** The opened link's value at `index`, shown. */
export const linkValue = (page: Page, index: number): Locator => page.getByTestId(`value-${index}`)

/** What a request link asks for. */
export const requestAsk = (page: Page, text: string): Locator => page.getByText(text)

/** Answers a request link. */
export async function answerRequest(page: Page, what: string, secret: string) {
  const answer = page.getByRole("form", { name: "Answer the request" })
  await answer.getByLabel("What it is").fill(what)
  await answer.getByLabel("The secret").fill(secret)
  await answer.getByRole("button", { name: "Send" }).click()
}

export const requestSent = (page: Page): Locator => page.getByRole("heading", { name: "Sent" })
