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

function lockdown(args: string[], input?: string): string {
  const run = spawnSync(cli, ["--root", path.join(home, "identity"), ...args], { input, encoding: "utf8", timeout: 120_000 })
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

async function signIn(page: Page, secretKey: string) {
  const form = page.getByRole("form", { name: "Sign in" })
  await form.getByLabel("Login name").fill(LOGIN)
  await form.getByLabel("Account password").fill(PASSWORD)
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
  await form.getByLabel("Account password").fill(PASSWORD)
  await form.getByLabel("Secret Key").fill("A1-NOT-THE-KEY")
  await form.getByRole("button", { name: "Sign in" }).click()
  await expect(form.getByRole("alert")).toBeVisible({ timeout: 90_000 })

  // With the kit's Secret Key it does, and the desktop's login is there.
  await signIn(page, secretKey)
  await expect(page.getByRole("heading", { name: "All items" })).toBeVisible({ timeout: 90_000 })
  await expect(page.getByRole("button", { name: /Bank from the desktop/ }).first()).toBeVisible()

  // Create a login.
  await page.getByRole("button", { name: "New item" }).click()
  await page.getByRole("menuitem", { name: "Login" }).click()
  const editor = page.getByRole("dialog")
  await editor.getByLabel("Title").fill("Example mail")
  await editor.getByLabel("Username").fill("me@example.com")
  await editor.getByLabel("Password", { exact: true }).fill("a-long-unique-password-1")
  await editor.getByLabel("Website").first().fill("https://mail.example.com")
  await editor.getByRole("button", { name: "Add" }).click()
  await expect(editor).toBeHidden()
  const row = page.getByRole("button", { name: /Example mail/ }).first()
  await expect(row).toBeVisible()

  // Edit it.
  await row.click()
  await page.getByRole("button", { name: "Edit", exact: true }).click()
  const edit = page.getByRole("dialog")
  await edit.getByLabel("Title").fill("Example mail (work)")
  await edit.getByRole("button", { name: "Save" }).click()
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
  await unlock.getByLabel("Account password").fill("not the password at all")
  await unlock.getByRole("button", { name: "Unlock" }).click()
  await expect(page.getByRole("alert")).toContainText("didn't match")
  await unlock.getByLabel("Account password").fill(PASSWORD)
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
