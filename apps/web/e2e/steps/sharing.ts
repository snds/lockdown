// The edges lane: Links and People in the web vault.

import type { Locator, Page } from "@playwright/test"

/** Makes a share link for some text, with a password: SHARE TEXT, the Form, CREATE LINK (brief S4). */
export async function shareText(page: Page, what: string, text: string, password: string) {
  await page.getByRole("button", { name: "Share text" }).click()
  const share = page.getByRole("form", { name: "Share text" })
  await share.getByLabel("What it is (they see this)").fill(what)
  await share.getByLabel("The text").fill(text)
  await share.getByRole("textbox", { name: "Password (optional)", exact: true }).fill(password)
  await page.getByRole("dialog", { name: "Share text" }).getByRole("button", { name: "Create link" }).click()
}

/** Makes a request link: ASK FOR SECRET, the Form, CREATE LINK. */
export async function askForSecret(page: Page, what: string) {
  await page.getByRole("button", { name: "Ask for secret" }).click()
  const ask = page.getByRole("form", { name: "Ask for a secret" })
  await ask.getByLabel("What you're asking for (they see this)").fill(what)
  await page.getByRole("dialog", { name: "Ask for a secret" }).getByRole("button", { name: "Create link" }).click()
}

/** The link just made, shown once: its text is the whole link, cut in the middle only on screen. */
export const newLink = (page: Page): Locator => page.getByTestId("new-link")

/** Puts the new link away: DONE, in the layer that shows it. */
export async function linkDone(page: Page) {
  await page.getByRole("dialog").getByRole("button", { name: "Done" }).click()
  await newLink(page).waitFor({ state: "detached" })
}

/** Opens a request's answer in the Links detail and hands back the detail. */
export async function openAnswer(page: Page): Promise<Locator> {
  await page.getByRole("list", { name: "Requests" }).getByRole("button", { name: "Open answer" }).click()
  return page.getByRole("region", { name: /^Opened: / })
}

/**
 * Shows the answer's value in its SecretField: a tap, or, where the
 * settings reveal only while held, Space kept down while the caller
 * reads it.
 */
export async function showAnswer(answer: Locator) {
  const show = answer.getByRole("button", { name: /(Hold to show|Show)/ })
  const words = `${(await show.getAttribute("aria-label")) ?? ""} ${(await show.textContent()) ?? ""}`
  if (/Hold/.test(words)) {
    // Space held down on the focused control: the field shows the value until the key comes up.
    await show.focus()
    await answer.page().keyboard.down("Space")
  } else {
    await show.click()
  }
}

// --- people -----------------------------------------------------------------------

/** The code a person reads out to be checked. */
export const yourCode = (page: Page): Locator => page.getByLabel("Your code to paste")

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
