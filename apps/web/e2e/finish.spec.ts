import { spawnSync } from "node:child_process"
import { mkdtempSync, rmSync } from "node:fs"
import { tmpdir } from "node:os"
import path from "node:path"

import { expect, test, type BrowserContext, type Page } from "@playwright/test"

/**
 * docs/plan/26-desktop-finish.md in the web vault, in Chromium against the
 * real server: quick access over the core's one index (a typo forgiven and
 * said), a TV's code read by the tab's camera (Chromium's fake camera shows
 * a picture of the code, the core decodes the frames) and opened after the
 * password gate, the contacts screen comparing keys by a pasted contact
 * code, and the measure of what typed secrets leave in the page's memory
 * after the tab hides and the vault locks.
 */

const PASSWORD = "correct horse battery staple"
const A = `finish-a-${Date.now()}@example.com`
const B = `finish-b-${Date.now()}@example.com`
// The memory test signs in on its own account: the server allows a login
// name ten sign-in steps in a burst (two per sign-in), and A has used them.
const C = `finish-c-${Date.now()}@example.com`
const TYPED = "zq-measure-typed-71"
const target = process.env.CARGO_TARGET_DIR ?? path.resolve(import.meta.dirname, "../../../target")
const cli = process.env.LOCKDOWN_CLI_BIN ?? path.join(target, "debug", "lockdown")
const camera = path.resolve(import.meta.dirname, "fixtures/tv-code.y4m")

test.use({
  launchOptions: {
    args: ["--use-fake-ui-for-media-stream", "--use-fake-device-for-media-stream", `--use-file-for-fake-video-capture=${camera}`],
  },
})

let home = ""
const keys: Record<string, string> = {}

function lockdown(args: string[], input: string | undefined, who: string): string {
  const run = spawnSync(cli, ["--root", path.join(home, who, "identity"), ...args], { input, encoding: "utf8", timeout: 120_000 })
  if (run.status !== 0) throw new Error(`lockdown ${args.join(" ")}: ${run.stderr || run.error}`)
  return run.stdout
}

test.beforeAll(async ({ baseURL }) => {
  home = mkdtempSync(path.join(tmpdir(), "lockdown-web-finish-"))
  for (const [who, login] of [
    ["a", A],
    ["b", B],
    ["c", C],
  ] as const) {
    lockdown(["init"], undefined, who)
    const kit = lockdown(["account", "register", "--server", baseURL!, "--login", login, "--password-stdin"], `${PASSWORD}\n`, who)
    keys[who] = /^Secret Key:\s*(\S+)/m.exec(kit)?.[1] ?? ""
  }
  lockdown(["add", "login", "--site", "https://bank.example.com", "--username", "me", "--title", "Bank", "--password-stdin"], "a-bank-password-1\n", "a")
  lockdown(["add", "login", "--site", "https://www.youtube.com", "--username", "tv@example.com", "--title", "YouTube TV", "--password-stdin"], "yt-password-2\n", "a")
  lockdown(["sync"], undefined, "a")
  lockdown(["sync"], undefined, "b")
})

test.afterAll(() => {
  if (home) rmSync(home, { recursive: true, force: true })
})

async function signedIn(context: BrowserContext, who: "a" | "b" | "c"): Promise<Page> {
  const page = await context.newPage()
  page.on("pageerror", (e) => console.log(`page error: ${e.message}`))
  await page.goto("/vault/")
  const form = page.getByRole("form", { name: "Sign in" })
  await form.getByLabel("Login name").fill({ a: A, b: B, c: C }[who])
  await form.getByLabel("Master password").fill(PASSWORD)
  await form.getByLabel("Secret Key").fill(keys[who]!)
  await form.getByRole("button", { name: "Sign in" }).click()
  await expect(page.getByRole("heading", { name: "All items" })).toBeVisible({ timeout: 90_000 })
  return page
}

const sidebar = (page: Page) => page.getByRole("navigation", { name: "Sidebar" })

test("quick access asks the core's index: a typo is forgiven and says so", async ({ browser }) => {
  const context = await browser.newContext()
  const page = await signedIn(context, "a")
  await page.keyboard.press("Control+k")
  const input = page.getByRole("dialog", { name: "Quick access" }).getByPlaceholder(/Search titles/)
  await expect(input).toBeFocused()
  await input.fill("bnak")
  const option = page.getByRole("option", { name: /Bank/ })
  await expect(option).toBeVisible()
  await expect(option).toContainText("Close match")
  await input.fill("youtube tv")
  await expect(page.getByRole("option")).toHaveCount(1)
  await context.close()
})

test("a TV's code read by the tab's camera opens the real page after the password", async ({ browser }) => {
  const context = await browser.newContext()
  await context.route("https://www.youtube.com/**", (r) => r.fulfill({ contentType: "text/html", body: "<!doctype html><title>Activate</title><h1>Signed in</h1>" }))
  const page = await signedIn(context, "a")
  await sidebar(page).getByRole("button", { name: "Codes" }).click()
  await page.getByRole("button", { name: "Sign in a TV" }).click()
  const dialog = page.getByRole("dialog", { name: "Sign in a TV or console" })
  // The fake camera shows a picture of the TV's code; the core reads the frames.
  await expect(dialog.getByText("www.youtube.com", { exact: true })).toBeVisible({ timeout: 30_000 })
  await expect(dialog.getByText("WDJB-MJHT")).toBeVisible()
  await dialog.getByRole("radio", { name: /YouTube TV/ }).check()
  const opened = context.waitForEvent("page")
  await dialog.getByRole("button", { name: "Sign in on www.youtube.com" }).click()
  // The Approve gate: this tab's route is the password (unicron's gate
  // still calls it the account password).
  await page.getByLabel("Account password").fill(PASSWORD)
  await page.getByRole("button", { name: "Confirm" }).click()
  const tv = await opened
  await expect.poll(() => tv.url()).toBe("https://www.youtube.com/activate?user_code=WDJB-MJHT")
  await context.close()
})

test("a paste of the same code is refused by the proximity rule", async ({ browser }) => {
  const context = await browser.newContext()
  const page = await signedIn(context, "a")
  await sidebar(page).getByRole("button", { name: "Codes" }).click()
  await page.getByRole("button", { name: "Sign in a TV" }).click()
  const dialog = page.getByRole("dialog", { name: "Sign in a TV or console" })
  await dialog.getByRole("tab", { name: "Paste" }).click()
  await dialog.getByLabel("The code's address").fill("https://www.youtube.com/activate?user_code=QQQQ-RRRR")
  await dialog.getByRole("button", { name: "Check it" }).click()
  await expect(dialog.getByRole("alert")).toContainText("screen in front of you")
  await expect(dialog.getByRole("button", { name: /Sign in on/ })).toHaveCount(0)
  await context.close()
})

test("the contacts screen checks someone's keys by the code they paste over", async ({ browser }) => {
  const theirs = await browser.newContext()
  const b = await signedIn(theirs, "b")
  await sidebar(b).getByRole("button", { name: "People" }).click()
  const code = (await b.getByLabel("Your code").textContent())!.trim()
  expect(code).toMatch(/^lockdown:contact:v1:/)
  await theirs.close()

  const mine = await browser.newContext()
  const a = await signedIn(mine, "a")
  await sidebar(a).getByRole("button", { name: "People" }).click()
  await a.getByLabel("Look someone up by their sign-in").fill(B)
  await a.getByRole("button", { name: "Look up" }).click()
  const compare = a.getByRole("dialog", { name: new RegExp(`Compare fingerprints with ${B}`) })
  await expect(compare).toBeVisible({ timeout: 60_000 })
  // A wrong code is refused; theirs pins them as verified.
  await compare.getByLabel("What they read out").fill("lockdown:contact:v1:not-them:00")
  await compare.getByRole("button", { name: "They match" }).click()
  await expect(compare.getByRole("alert")).toBeVisible()
  await compare.getByLabel("What they read out").fill(code)
  await compare.getByRole("button", { name: "They match" }).click()
  await expect(a.getByRole("list", { name: "People" })).toContainText("Fingerprint compared")
  await mine.close()
})

test("what a typed password leaves in the page's memory after the tab hides and the vault locks", async ({ browser }) => {
  const context = await browser.newContext()
  const page = await signedIn(context, "c")
  const cdp = await context.newCDPSession(page)
  const inHeap = async () => {
    await cdp.send("HeapProfiler.collectGarbage")
    let snapshot = ""
    const chunk = (e: { chunk: string }) => {
      snapshot += e.chunk
    }
    cdp.on("HeapProfiler.addHeapSnapshotChunk", chunk)
    await cdp.send("HeapProfiler.takeHeapSnapshot", { reportProgress: false })
    cdp.off("HeapProfiler.addHeapSnapshotChunk", chunk)
    return snapshot.includes(TYPED)
  }

  await page.getByRole("button", { name: "New item" }).click()
  await page.getByRole("menuitem", { name: /^\W*\w*\s*Login$/ }).first().click()
  const editor = page.getByRole("dialog")
  const field = editor.getByLabel("Password", { exact: true })
  await field.fill(TYPED)
  expect(await inHeap(), "while typed, the page holds it (D91)").toBe(true)

  // The tab goes out of view: the field and React's copy of it are cleared.
  await page.evaluate(() => {
    Object.defineProperty(document, "visibilityState", { configurable: true, get: () => "hidden" })
    document.dispatchEvent(new Event("visibilitychange"))
  })
  await expect(field).toHaveValue("")
  const afterHide = await inHeap()

  // Locked: every view that held it is gone.
  await page.evaluate(() => {
    Object.defineProperty(document, "visibilityState", { configurable: true, get: () => "visible" })
  })
  await page.keyboard.press("Escape")
  await sidebar(page).getByRole("button", { name: "Lock" }).click()
  await expect(page.getByRole("form", { name: "Unlock" })).toBeVisible()
  const afterLock = await inHeap()
  test.info().annotations.push({ type: "measure", description: `typed password in the page's heap: after hide ${afterHide}, after lock ${afterLock}` })
  console.log(`measure: typed password in the page's heap after hide=${afterHide}, after lock=${afterLock}`)
  expect(afterLock, "nothing the page can reach still holds the typed password once locked").toBe(false)
  await context.close()
})
