import { spawnSync } from "node:child_process"
import { mkdtempSync, rmSync } from "node:fs"
import { tmpdir } from "node:os"
import path from "node:path"

import { expect, test, type Page } from "@playwright/test"

/**
 * The web vault in Chromium against the real server (e2e/serve.mjs). The
 * account is registered the way a person's computer does it (the
 * `lockdown` CLI, the same core as the desktop app), with one login in its
 * vault. The page then signs in as a new device with the password and the
 * kit's Secret Key, finds that login, creates an item, edits it, finds it,
 * locks and unlocks, and signs out. Every request the page makes stays on
 * its own origin, and the page runs without a single Content Security
 * Policy violation.
 */

const LOGIN = `web-${Date.now()}@example.com`
const PASSWORD = "correct horse battery staple"
const target = process.env.CARGO_TARGET_DIR ?? path.resolve(import.meta.dirname, "../../../target")
const cli = process.env.LOCKDOWN_CLI_BIN ?? path.join(target, "debug", "lockdown")

let home = ""
let secretKey = ""
/** A second account, for the links test: the server brakes sign-ins per login name (D57). */
const LINKS_LOGIN = `links-${Date.now()}@example.com`
let linksSecretKey = ""

/** The CLI as one person's computer: `who` names its own identity root and data directory. */
function lockdown(args: string[], input?: string, who = "first"): string {
  const run = spawnSync(cli, ["--root", path.join(home, who, "identity"), ...args], { input, encoding: "utf8", timeout: 120_000 })
  if (run.status !== 0) throw new Error(`lockdown ${args.join(" ")}: ${run.stderr || run.error}`)
  return run.stdout
}

test.beforeAll(async ({ baseURL }) => {
  home = mkdtempSync(path.join(tmpdir(), "lockdown-web-e2e-cli-"))
  lockdown(["init"])
  const kit = lockdown(["account", "register", "--server", baseURL!, "--login", LOGIN, "--password-stdin"], `${PASSWORD}\n`)
  secretKey = /^Secret Key:\s*(\S+)/m.exec(kit)?.[1] ?? ""
  expect(secretKey, "the kit page names the Secret Key").toMatch(/^A1-/)
  lockdown(["add", "login", "--site", "https://bank.example.com", "--username", "me", "--title", "Bank from the desktop", "--password-stdin"], "a-bank-password-1\n")
  lockdown(["sync"])
  lockdown(["init"], undefined, "links")
  const linksKit = lockdown(["account", "register", "--server", baseURL!, "--login", LINKS_LOGIN, "--password-stdin"], `${PASSWORD}\n`, "links")
  linksSecretKey = /^Secret Key:\s*(\S+)/m.exec(linksKit)?.[1] ?? ""
  lockdown(["sync"], undefined, "links")
})

test.afterAll(() => {
  if (home) rmSync(home, { recursive: true, force: true })
})

function watch(page: Page) {
  const problems: string[] = []
  const origins = new Set<string>()
  page.on("console", (m) => {
    if (/Content Security Policy|Refused to|Integrity/i.test(m.text())) problems.push(m.text())
  })
  page.on("pageerror", (e) => problems.push(`page error: ${e.message}`))
  page.on("request", (r) => origins.add(new URL(r.url()).origin))
  return { problems, origins }
}

async function signIn(page: Page, secretKey: string, login = LOGIN) {
  const form = page.getByRole("form", { name: "Sign in" })
  await form.getByLabel("Login name").fill(login)
  await form.getByLabel("Master password").fill(PASSWORD)
  await form.getByLabel("Secret Key").fill(secretKey)
  await form.getByRole("button", { name: "Sign in" }).click()
}

const sidebar = (page: Page) => page.getByRole("navigation", { name: "Sidebar" })

test("sign in as a new device, find, create, edit, find, lock, unlock and sign out", async ({ browser, baseURL }) => {
  const context = await browser.newContext({ permissions: ["clipboard-read", "clipboard-write"] })
  const page = await context.newPage()
  const { problems, origins } = watch(page)

  // The page arrives locked down.
  const res = await page.goto("/vault/")
  expect(res?.status()).toBe(200)
  const headers = res!.headers()
  expect(headers["content-security-policy"]).toContain("default-src 'none'")
  expect(headers["content-security-policy"]).toContain("frame-ancestors 'none'")
  expect(headers["cross-origin-opener-policy"]).toBe("same-origin")
  const html = await res!.text()
  expect(html).toMatch(/<script[^>]+integrity="sha384-/)

  // The password alone doesn't open a new device.
  const form = page.getByRole("form", { name: "Sign in" })
  await form.getByLabel("Login name").fill(LOGIN)
  await form.getByLabel("Master password").fill(PASSWORD)
  await form.getByLabel("Secret Key").fill("A1-NOT-THE-KEY")
  await form.getByRole("button", { name: "Sign in" }).click()
  await expect(form.getByRole("alert")).toBeVisible({ timeout: 90_000 })

  // With the kit's Secret Key it does, and the desktop's login is there.
  await signIn(page, secretKey)
  await expect(page.getByRole("heading", { name: "All items" })).toBeVisible({ timeout: 90_000 })
  await expect(page.getByRole("button", { name: /Bank from the desktop/ }).first()).toBeVisible()

  // Create a login.
  await page.getByRole("button", { name: "New item" }).click()
  await page.getByRole("menuitem", { name: /^\W*\w*\s*Login$/ }).first().click()
  const editor = page.getByRole("dialog")
  await editor.getByLabel("Title").fill("Example mail")
  await editor.getByLabel("Username").fill("me@example.com")
  await editor.getByLabel("Password", { exact: true }).fill("a-long-unique-password-1")
  await editor.getByLabel("Website").first().fill("https://mail.example.com")
  await editor.getByRole("button", { name: "Add", exact: true }).click()
  await expect(editor).toBeHidden()
  const row = page.getByRole("button", { name: /Example mail/ }).first()
  await expect(row).toBeVisible()

  // Edit it.
  await row.click()
  await page.getByRole("button", { name: "Edit", exact: true }).click()
  const edit = page.getByRole("dialog")
  await edit.getByLabel("Title").fill("Example mail (work)")
  await edit.getByRole("button", { name: "Save", exact: true }).click()
  await expect(edit).toBeHidden()

  // Find it, and only it.
  const search = page.getByRole("searchbox", { name: "Search" })
  await search.fill("work")
  await expect(page.getByRole("button", { name: /Example mail \(work\)/ }).first()).toBeVisible()
  await search.fill("nothing like this anywhere")
  await expect(page.getByRole("button", { name: /Example mail/ })).toHaveCount(0)
  await search.fill("")

  // Lock: the keys go; the password opens it again.
  await sidebar(page).getByRole("button", { name: "Lock" }).click()
  const unlock = page.getByRole("form", { name: "Unlock" })
  await expect(unlock).toBeVisible()
  await unlock.getByLabel("Master password").fill("not the password at all")
  await unlock.getByRole("button", { name: "Unlock" }).click()
  await expect(page.getByRole("alert")).toContainText("didn't match")
  await unlock.getByLabel("Master password").fill(PASSWORD)
  await unlock.getByRole("button", { name: "Unlock" }).click()
  await expect(page.getByRole("button", { name: /Example mail \(work\)/ }).first()).toBeVisible({ timeout: 60_000 })

  // Sign out: the edit reaches the server first, then the tab forgets it all.
  await sidebar(page).getByRole("button", { name: "Settings" }).click()
  await page.getByRole("button", { name: "Sign out of this tab" }).click()
  await expect(page.getByRole("tab", { name: "Sign in" })).toBeVisible()
  const listed = lockdown(["sync"]) && lockdown(["list"])
  expect(listed).toContain("Example mail (work)")

  expect([...origins]).toEqual([new URL(baseURL!).origin])
  expect(problems).toEqual([])
  await context.close()
})

test("a share link and a request link work in a browser with no account", async ({ browser, baseURL }) => {
  const origin = new URL(baseURL!).origin
  const owner = await browser.newContext({ permissions: ["clipboard-read", "clipboard-write"] })
  const tab = await owner.newPage()
  const ownerWatch = watch(tab)
  await tab.goto("/vault/")
  await signIn(tab, linksSecretKey, LINKS_LOGIN)
  await expect(tab.getByRole("heading", { name: "All items" })).toBeVisible({ timeout: 90_000 })
  await sidebar(tab).getByRole("button", { name: "Links" }).click()

  // A text link, with a password, that opens once.
  const share = tab.getByRole("form", { name: "Share some text" })
  await share.getByLabel("What it is").fill("Cabin Wi-Fi")
  await share.getByLabel("The text").fill("pine-needles-42")
  await share.getByLabel("A password for it").fill("told you on the phone")
  await share.getByRole("button", { name: "Make the link" }).click()
  const shareLink = (await tab.getByTestId("new-link").textContent())!.trim()
  expect(shareLink).toMatch(new RegExp(`^${origin}/s/[0-9a-f-]+#`))
  await tab.getByRole("button", { name: "Done" }).click()

  // A request link.
  const ask = tab.getByRole("form", { name: "Ask for a secret" })
  await ask.getByLabel("What you're asking for").fill("The alarm code")
  await ask.getByRole("button", { name: "Make the request link" }).click()
  const requestLink = (await tab.getByTestId("new-link").textContent())!.trim()
  expect(requestLink).toMatch(new RegExp(`^${origin}/r/[0-9a-f-]+#`))
  await tab.getByRole("button", { name: "Done" }).click()

  // Someone with no account opens the share link: the server's /s/ page
  // hands over to the web vault, and the key never leaves the fragment.
  const stranger = await browser.newContext({ permissions: ["clipboard-read", "clipboard-write"] })
  const page = await stranger.newPage()
  const { problems, origins } = watch(page)
  const sent: string[] = []
  page.on("request", (r) => sent.push(r.url()))
  await page.goto(shareLink)
  await expect(page).toHaveURL(/\/vault\/s\/[0-9a-f-]+$/)
  await expect(page.getByRole("heading", { name: "Someone shared something with you" })).toBeVisible({ timeout: 60_000 })
  const form = page.getByRole("form", { name: "Open the link" })
  await form.getByLabel("The link's password").fill("not it")
  await form.getByRole("button", { name: "Open" }).click()
  await expect(page.getByRole("alert")).toContainText("password")
  await form.getByLabel("The link's password").fill("told you on the phone")
  await form.getByRole("button", { name: "Open" }).click()
  await expect(page.getByRole("heading", { name: "Cabin Wi-Fi" })).toBeVisible({ timeout: 60_000 })
  await page.getByRole("button", { name: "Show Text" }).click()
  await expect(page.getByTestId("value-0")).toHaveText("pine-needles-42")
  const key = shareLink.split("#")[1]!
  expect(sent.filter((u) => u.includes(key))).toEqual([])

  // And answers the request.
  await page.goto(requestLink)
  await expect(page.getByText("The alarm code")).toBeVisible({ timeout: 60_000 })
  const answer = page.getByRole("form", { name: "Answer the request" })
  await answer.getByLabel("What it is").fill("Alarm")
  await answer.getByLabel("The secret").fill("8642")
  await answer.getByRole("button", { name: "Send" }).click()
  await expect(page.getByRole("heading", { name: "Sent" })).toBeVisible({ timeout: 60_000 })

  // The asker reads it in the vault.
  await sidebar(tab).getByRole("button", { name: "Devices" }).click()
  await sidebar(tab).getByRole("button", { name: "Links" }).click()
  await tab.getByRole("button", { name: "Open the answer" }).click()
  const opened = tab.getByRole("group", { name: "The answer" })
  await expect(opened).toContainText("Alarm")
  await opened.getByRole("button", { name: /^Show / }).click()
  await expect(opened).toContainText("8642")

  expect([...origins]).toEqual([origin])
  expect(problems).toEqual([])
  expect(ownerWatch.problems).toEqual([])
  await stranger.close()
  await owner.close()
})

test("vault depth in the browser: a variant, a template of your own, generator options and the Wi-Fi guest card", async ({ browser, baseURL }) => {
  const context = await browser.newContext()
  const page = await context.newPage()
  const { problems, origins } = watch(page)
  await page.goto("/vault/")
  await signIn(page, secretKey)
  await expect(page.getByRole("heading", { name: "All items" })).toBeVisible({ timeout: 90_000 })

  // A passport: a variant of the identity kind, drawn from its own template.
  await page.getByRole("button", { name: "New item" }).click()
  await page.getByRole("menuitem", { name: /Passport$/ }).click()
  let editor = page.getByRole("dialog")
  await editor.getByLabel("Title").fill("My passport")
  await editor.getByLabel("Passport number", { exact: true }).fill("X1234567")
  await editor.getByLabel("Issuing country").fill("GB")
  await editor.getByRole("button", { name: "Add", exact: true }).click()
  await expect(editor).toBeHidden()
  await page.getByRole("button", { name: /My passport/ }).first().click()
  await expect(page.getByRole("region", { name: "Passport" })).toContainText("Issuing country")

  // A template of your own, then an item from it.
  await page.getByRole("button", { name: "New item" }).click()
  await page.getByRole("menuitem", { name: /Make a template/ }).click()
  const manager = page.getByRole("dialog")
  await manager.getByRole("button", { name: "New template" }).click()
  await manager.getByLabel("Name").fill("Gym locker")
  await manager.getByLabel("Field 1's label").fill("Locker")
  await manager.getByRole("button", { name: "Save template" }).click()
  await expect(manager.getByText("Gym locker")).toBeVisible()
  await manager.getByRole("button", { name: "Close", exact: true }).first().click()
  await page.getByRole("button", { name: "New item" }).click()
  await page.getByRole("menuitem", { name: /Gym locker$/ }).click()
  editor = page.getByRole("dialog")
  await editor.getByLabel("Locker", { exact: true }).fill("41")
  await editor.getByRole("button", { name: "Add", exact: true }).click()
  await expect(editor).toBeHidden()
  await expect(page.getByRole("button", { name: /Gym locker/ }).first()).toBeVisible()

  // A Wi-Fi network whose password the core makes as a passphrase, then its guest card.
  await page.getByRole("button", { name: "New item" }).click()
  await page.getByRole("menuitem", { name: /Wi-Fi network$/ }).click()
  editor = page.getByRole("dialog")
  await editor.getByLabel("Network name").fill("cabin-net")
  await editor.getByRole("button", { name: "Generate password" }).click()
  await editor.getByRole("button", { name: "Options" }).click()
  await editor.getByLabel("Make").selectOption("words")
  await expect(editor.getByRole("status")).toContainText("bits")
  await editor.getByRole("button", { name: "Add", exact: true }).click()
  await expect(editor).toBeHidden()
  await page.getByRole("button", { name: /cabin-net/ }).first().click()
  await page.getByRole("button", { name: "Guest card" }).click()
  await expect(page.getByRole("img", { name: "The code to join cabin-net" })).toBeVisible()
  await page.getByRole("button", { name: "Hide" }).click()
  await expect(page.getByRole("img", { name: "The code to join cabin-net" })).toBeHidden()

  expect([...origins]).toEqual([new URL(baseURL!).origin])
  expect(problems).toEqual([])
  await context.close()
})
