import { spawnSync } from "node:child_process"
import { mkdtempSync, rmSync } from "node:fs"
import { tmpdir } from "node:os"
import path from "node:path"

import { expect, test, type Page } from "@playwright/test"

/**
 * docs/plan/30-pq-status-line.md in the web vault, in Chromium against the
 * real server: the account's records name a device from an older build
 * (written by the `older-device` test tool into the command line's
 * identity, then synced), so the Security screen's line says "waiting for"
 * it by name and links to it on Devices. Once that device reports level 2
 * and the change syncs, the open tab's line turns to "on" by itself.
 */

const PASSWORD = "correct horse battery staple"
const LOGIN = `pq-${Date.now()}@example.com`
const OLD = "Lockdown on the old laptop"
const target = process.env.CARGO_TARGET_DIR ?? path.resolve(import.meta.dirname, "../../../target")
const cli = process.env.LOCKDOWN_CLI_BIN ?? path.join(target, "debug", "lockdown")
// `cargo build -p lockdown-vault --example older-device`
const olderDevice = process.env.LOCKDOWN_OLDER_DEVICE_BIN ?? path.join(target, "debug", "examples", "older-device")

let home = ""
let secretKey = ""
let oldId = ""

function run(bin: string, args: string[], input?: string): string {
  const out = spawnSync(bin, args, { input, encoding: "utf8", timeout: 120_000 })
  if (out.status !== 0) throw new Error(`${path.basename(bin)} ${args.join(" ")}: ${out.stderr || out.error}`)
  return out.stdout
}

const lockdown = (args: string[], input?: string) => run(cli, ["--root", path.join(home, "identity"), ...args], input)
const older = (args: string[]) =>
  run(olderDevice, ["--root", path.join(home, "identity"), "--keystore", path.join(home, "lockdown", "device-key.cbor"), ...args]).trim()

test.beforeAll(async ({ baseURL }) => {
  home = mkdtempSync(path.join(tmpdir(), "lockdown-web-pq-"))
  lockdown(["init"])
  const kit = lockdown(["account", "register", "--server", baseURL!, "--login", LOGIN, "--password-stdin"], `${PASSWORD}\n`)
  secretKey = /^Secret Key:\s*(\S+)/m.exec(kit)?.[1] ?? ""
  // An older Lockdown on another computer: it reads the account's key
  // (level 1) but not quantum-safe vault signatures (level 2).
  oldId = older(["add", OLD, "1"])
  expect(oldId).toMatch(/^[0-9a-f-]{32,36}$/)
  lockdown(["sync"])
  expect(lockdown(["security"])).toContain(`Quantum-safe signatures: waiting for ${OLD}`)
})

test.afterAll(() => {
  if (home) rmSync(home, { recursive: true, force: true })
})

const sidebar = (page: Page) => page.getByRole("navigation", { name: "Sidebar" })

test("the Security screen waits for the old device by name, then says on once it reports level 2", async ({ page }) => {
  const problems: string[] = []
  page.on("pageerror", (e) => problems.push(`page error: ${e.message}`))
  await page.goto("/vault/")
  const form = page.getByRole("form", { name: "Sign in" })
  await form.getByLabel("Login name").fill(LOGIN)
  await form.getByLabel("Master password").fill(PASSWORD)
  await form.getByLabel("Secret Key").fill(secretKey)
  await form.getByRole("button", { name: "Sign in" }).click()
  await expect(page.getByRole("heading", { name: "All items" })).toBeVisible({ timeout: 90_000 })

  await sidebar(page).getByRole("button", { name: "Security" }).click()
  const line = page.getByRole("status", { name: "Quantum-safe signatures" })
  await expect(line).toContainText(`Quantum-safe signatures: waiting for ${OLD}`, { timeout: 60_000 })
  await expect(line).toContainText("This device needs the latest Lockdown")
  await expect(line).toHaveAttribute("data-state", "waiting")
  await expect(line).not.toContainText(": on")

  // The device is a link to it on the Devices screen.
  await line.getByRole("button", { name: OLD }).click()
  const row = page.getByRole("listitem").filter({ hasText: OLD })
  await expect(row).toHaveAttribute("aria-current", "true")
  await expect(row).toBeFocused()

  // Back on Security, with the live channel open (the worker looks every three seconds at most).
  await sidebar(page).getByRole("button", { name: "Security" }).click()
  await expect(line).toContainText(`waiting for ${OLD}`)
  await page.waitForTimeout(4000)

  // The old computer updates: its record says level 2, and the change syncs.
  older(["set", oldId, "2"])
  lockdown(["sync"])
  expect(lockdown(["security"])).toContain("Quantum-safe signatures: on")
  await expect(line).toContainText("Quantum-safe signatures: on", { timeout: 30_000 })
  await expect(line).toHaveAttribute("data-state", "on")
  await expect(line.getByRole("button")).toHaveCount(0)
  expect(problems).toEqual([])
})
