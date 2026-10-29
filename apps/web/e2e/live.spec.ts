import { spawnSync } from "node:child_process"
import { mkdtempSync, rmSync } from "node:fs"
import { tmpdir } from "node:os"
import path from "node:path"

import { expect, test, type Page } from "@playwright/test"

/**
 * docs/plan/27-web-server-finish.md in the web vault, in Chromium against
 * the real server: the tab's worker holds the live channel itself, so a
 * change another device makes shows in an open tab within seconds and
 * without a reload (well inside the minute the tab's fallback sync waits);
 * the session list is there; Settings says what the server reports about
 * its restore drills and that no log mirror was compared.
 */

const PASSWORD = "correct horse battery staple"
const LOGIN = `live-${Date.now()}@example.com`
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
  home = mkdtempSync(path.join(tmpdir(), "lockdown-web-live-"))
  lockdown(["init"])
  const kit = lockdown(["account", "register", "--server", baseURL!, "--login", LOGIN, "--password-stdin"], `${PASSWORD}\n`)
  secretKey = /^Secret Key:\s*(\S+)/m.exec(kit)?.[1] ?? ""
  lockdown(["sync"])
})

test.afterAll(() => {
  if (home) rmSync(home, { recursive: true, force: true })
})

const sidebar = (page: Page) => page.getByRole("navigation", { name: "Sidebar" })

test("a change on another device shows in an open tab by itself, through the worker's live channel", async ({ page }) => {
  page.on("pageerror", (e) => console.log(`page error: ${e.message}`))
  await page.goto("/vault/")
  const form = page.getByRole("form", { name: "Sign in" })
  await form.getByLabel("Login name").fill(LOGIN)
  await form.getByLabel("Master password").fill(PASSWORD)
  await form.getByLabel("Secret Key").fill(secretKey)
  await form.getByRole("button", { name: "Sign in" }).click()
  await expect(page.getByRole("heading", { name: "All items" })).toBeVisible({ timeout: 90_000 })
  // Give the worker a moment to open the channel (it looks every three seconds at most).
  await page.waitForTimeout(4000)

  // The command line, another device of the account, adds and syncs.
  lockdown(["add", "login", "--site", "https://live.example", "--username", "me", "--title", "Arrived live", "--password-stdin"], "live-pass-1\n")
  lockdown(["sync"])
  await expect(page.getByText("Arrived live")).toBeVisible({ timeout: 20_000 })

  // The session list comes from the same client the desktop uses.
  await sidebar(page).getByRole("button", { name: "Devices" }).click()
  await expect(page.getByText("Signed in", { exact: true })).toBeVisible()

  // Settings → This server: the drill and the mirrors, as the server reports them.
  await sidebar(page).getByRole("button", { name: "Settings" }).click()
  await expect(page.getByText("This server doesn't run restore drills", { exact: false })).toBeVisible()
  await expect(page.getByText("No mirror has been compared yet", { exact: false })).toBeVisible()

  // Locked, the channel closes; unlocked, it opens again and still carries changes.
  await sidebar(page).getByRole("button", { name: /^All items/ }).click()
  await sidebar(page).getByRole("button", { name: "Lock" }).click()
  const unlock = page.getByRole("form", { name: "Unlock" })
  await unlock.getByLabel("Master password").fill(PASSWORD)
  await unlock.getByRole("button", { name: "Unlock" }).click()
  await expect(page.getByText("Arrived live")).toBeVisible({ timeout: 90_000 })
  await page.waitForTimeout(4000)
  lockdown(["add", "login", "--site", "https://again.example", "--username", "me", "--title", "After unlock", "--password-stdin"], "live-pass-2\n")
  lockdown(["sync"])
  await expect(page.getByText("After unlock")).toBeVisible({ timeout: 20_000 })
})
