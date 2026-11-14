import { spawnSync } from "node:child_process"
import { mkdtempSync, rmSync } from "node:fs"
import { tmpdir } from "node:os"
import path from "node:path"

import { expect, test, type Page } from "@playwright/test"

/**
 * The web vault's device-link join in Chromium against the real server
 * (docs/plan/22-security-depth.md §2, docs/plan/57-fence-web-join.md). One
 * tab is signed in; it opens Devices, adds a device and shows the code. A
 * second tab with nothing on it types that code under "With a device",
 * both show the same six digits, the first approves behind its password
 * gate, and the second finishes with the login name and the master
 * password alone, no kit typed, and opens the vault the CLI filled.
 *
 * This is the core's plain device-link join (`App::join_start`,
 * `App::join_finish_signed_in`), which only the wasm build and the native
 * tests compile: the spec proves the web vault still has it.
 */

const LOGIN = `link-${Date.now()}@example.com`
const PASSWORD = "correct horse battery staple"
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

test.beforeAll(async ({ baseURL }) => {
  test.setTimeout(600_000)
  home = mkdtempSync(path.join(tmpdir(), "lockdown-web-link-"))
  const kit = lockdown(["init", "--name", "Link"])
  secretKey = /^Secret Key:\s*(\S+)/m.exec(kit)?.[1] ?? ""
  expect(secretKey, "the kit page names the Secret Key").toMatch(/^A1-/)
  lockdown(["account", "register", "--server", baseURL!, "--login", LOGIN])
  lockdown(["add", "login", "--site", "https://bank.example.com", "--username", "me", "--title", "Bank from the desktop", "--password-stdin"], "a-bank-password-1\n")
  lockdown(["sync"])
})

test.afterAll(() => {
  if (home) rmSync(home, { recursive: true, force: true })
})

const sidebar = (page: Page) => page.getByRole("navigation", { name: "Sidebar" })

test("a tab with nothing on it joins through a signed-in tab's device link", async ({ browser }) => {
  test.setTimeout(300_000)
  const problems: string[] = []

  // The device that approves: a tab signed in with the kit.
  const first = await (await browser.newContext()).newPage()
  first.on("pageerror", (e) => problems.push(`first: ${e.message}`))
  await first.goto("/vault/")
  const form = first.getByRole("form", { name: "Sign in" })
  await form.getByLabel("Login name").fill(LOGIN)
  await form.getByLabel("Master password").fill(PASSWORD)
  await form.getByLabel("Secret Key").fill(secretKey)
  await form.getByRole("button", { name: "Sign in" }).click()
  await expect(first.getByRole("heading", { name: "All items" })).toBeVisible({ timeout: 90_000 })

  await sidebar(first).getByRole("button", { name: "Devices" }).click()
  await first.getByRole("button", { name: "Add a device" }).click()
  const offer = first.getByRole("dialog", { name: "Add a device" })
  const code = (await offer.getByLabel("Device code").textContent())!.trim()
  expect(code).toMatch(/^[0-9A-Z]{5}-[0-9A-Z]{5}$/)

  // The new device: a tab with nothing on it types the code.
  const second = await (await browser.newContext()).newPage()
  second.on("pageerror", (e) => problems.push(`second: ${e.message}`))
  await second.goto("/vault/")
  await second.getByRole("tab", { name: "With a device" }).click()
  await second.getByLabel("Code from the other device").fill(code)
  await second.getByRole("button", { name: "Join", exact: true }).click()

  // Both show the same six digits.
  const theirs = second.getByLabel("Digits to compare")
  await expect(theirs).toHaveText(/^\d{3} \d{3}$/, { timeout: 60_000 })
  const compare = first.getByRole("dialog", { name: "Do the digits match?" })
  await expect(compare.getByLabel("Digits to compare")).toHaveText((await theirs.textContent())!.trim(), { timeout: 60_000 })

  // The first tab approves behind its password gate.
  await compare.getByRole("button", { name: "Approve" }).click()
  // By its exact name: from unicron 1.0 the gate's field has a "Show Account password" button beside it.
  await first.getByLabel("Account password", { exact: true }).fill(PASSWORD)
  await first.getByRole("button", { name: "Confirm" }).click()
  await expect(compare).toBeHidden({ timeout: 60_000 })

  // The new tab finishes with the password alone and opens the vault.
  await expect(second.getByLabel("Login name")).toBeVisible({ timeout: 60_000 })
  await expect(second.getByText(/Approved\./)).toBeVisible()
  await second.getByLabel("Login name").fill(LOGIN)
  await second.getByLabel("Master password").fill(PASSWORD)
  await second.getByRole("button", { name: "Finish" }).click()
  await expect(second.getByRole("heading", { name: "All items" })).toBeVisible({ timeout: 90_000 })
  await expect(second.getByRole("button", { name: /Bank from the desktop/ }).first()).toBeVisible()

  expect(problems).toEqual([])
})
