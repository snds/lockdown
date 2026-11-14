// The edges lane: Links and People in the web vault.

import type { Locator, Page } from "@playwright/test"

/** Makes a share link for some text, with a password. */
export async function shareText(page: Page, what: string, text: string, password: string) {
  const share = page.getByRole("form", { name: "Share some text" })
  await share.getByLabel("What it is").fill(what)
  await share.getByLabel("The text").fill(text)
  await share.getByLabel("A password for it").fill(password)
  await share.getByRole("button", { name: "Make the link" }).click()
}

/** Makes a request link. */
export async function askForSecret(page: Page, what: string) {
  const ask = page.getByRole("form", { name: "Ask for a secret" })
  await ask.getByLabel("What you're asking for").fill(what)
  await ask.getByRole("button", { name: "Make the request link" }).click()
}

/** The link just made, shown once. */
export const newLink = (page: Page): Locator => page.getByTestId("new-link")

/** Puts the new link away. */
export async function linkDone(page: Page) {
  await page.getByRole("button", { name: "Done" }).click()
}

/** Opens a request's answer and hands back its group. */
export async function openAnswer(page: Page): Promise<Locator> {
  await page.getByRole("button", { name: "Open the answer" }).click()
  return page.getByRole("group", { name: "The answer" })
}

/** Shows the answer's value. */
export async function showAnswer(answer: Locator) {
  await answer.getByRole("button", { name: /^Show / }).click()
}

// --- people -----------------------------------------------------------------------

/** The code a person reads out to be checked. */
export const yourCode = (page: Page): Locator => page.getByLabel("Your code")

/** Looks someone up by their sign-in and hands back the compare dialog. */
export async function lookUp(page: Page, login: string): Promise<Locator> {
  await page.getByLabel("Look someone up by their sign-in").fill(login)
  await page.getByRole("button", { name: "Look up" }).click()
  return page.getByRole("dialog", { name: new RegExp(`Compare fingerprints with ${login}`) })
}

/** Types what they read out and says the fingerprints match. */
export async function theyMatch(compare: Locator, code: string) {
  await compare.getByLabel("What they read out").fill(code)
  await compare.getByRole("button", { name: "They match" }).click()
}

/** The list of people. */
export const people = (page: Page): Locator => page.getByRole("list", { name: "People" })
