import { spawnSync } from "node:child_process"
import { mkdtempSync, rmSync } from "node:fs"
import { tmpdir } from "node:os"
import path from "node:path"

import { expect, test } from "@playwright/test"

import { signedInHeading } from "./steps/devices.ts"
import { titled } from "./steps/items.ts"
import { go, lock, vaultOpen } from "./steps/nav.ts"
import { serverLine } from "./steps/settings.ts"
import { signIn, unlock } from "./steps/unlock.ts"

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
  const env = { ...process.env, LOCKDOWN_MASTER_PASSWORD: PASSWORD }
  const run = spawnSync(cli, ["--root", path.join(home, "identity"), ...args], { input, env, encoding: "utf8", timeout: 120_000 })
  if (run.status !== 0) throw new Error(`lockdown ${args.join(" ")}: ${run.stderr || run.error}`)
  return run.stdout
}

/**
 * Every vault the CLI makes has a master password (docs/plan/43-cli-sealed.md):
 * `init --name` seals it and prints the kit, and `account register` moves it
 * to the server with the master password as the account's password, the one
 * the web vault signs in with.
 */
test.beforeAll(async ({ baseURL }) => {
  test.setTimeout(600_000)
  home = mkdtempSync(path.join(tmpdir(), "lockdown-web-live-"))
  const kit = lockdown(["init", "--name", "Live"])
  secretKey = /^Secret Key:\s*(\S+)/m.exec(kit)?.[1] ?? ""
  lockdown(["account", "register", "--server", baseURL!, "--login", LOGIN])
  lockdown(["sync"])
})

test.afterAll(() => {
  if (home) rmSync(home, { recursive: true, force: true })
})

test("a change on another device shows in an open tab by itself, through the worker's live channel", async ({ page }) => {
  page.on("pageerror", (e) => console.log(`page error: ${e.message}`))
  await page.goto("/vault/")
  await signIn(page, LOGIN, PASSWORD, secretKey)
  await expect(vaultOpen(page)).toBeVisible({ timeout: 90_000 })
  // Give the worker a moment to open the channel (it looks every three seconds at most).
  await page.waitForTimeout(4000)

  // The command line, another device of the account, adds and syncs.
  lockdown(["add", "login", "--site", "https://live.example", "--username", "me", "--title", "Arrived live", "--password-stdin"], "live-pass-1\n")
  lockdown(["sync"])
  await expect(titled(page, "Arrived live")).toBeVisible({ timeout: 20_000 })

  // The session list comes from the same client the desktop uses.
  await go(page, "devices")
  await expect(signedInHeading(page)).toBeVisible()

  // Settings → This server: the drill and the mirrors, as the server reports them.
  await go(page, "settings")
  await expect(serverLine(page, "This server doesn't run restore drills")).toBeVisible()
  await expect(serverLine(page, "No mirror has been compared yet")).toBeVisible()

  // Locked, the channel closes; unlocked, it opens again and still carries changes.
  await go(page, "all")
  await lock(page)
  await unlock(page, PASSWORD)
  await expect(titled(page, "Arrived live")).toBeVisible({ timeout: 90_000 })
  await page.waitForTimeout(4000)
  lockdown(["add", "login", "--site", "https://again.example", "--username", "me", "--title", "After unlock", "--password-stdin"], "live-pass-2\n")
  lockdown(["sync"])
  await expect(titled(page, "After unlock")).toBeVisible({ timeout: 20_000 })
})
