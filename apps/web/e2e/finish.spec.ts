import { spawnSync } from "node:child_process"
import { mkdtempSync, rmSync } from "node:fs"
import { tmpdir } from "node:os"
import path from "node:path"

import { expect, test, type BrowserContext, type Page } from "@playwright/test"

import { openTvSignIn, pasteAddress, pickLogin, scannedCode, scannedHost, signInOffers, signInOn } from "./steps/codes.ts"
import { LOGIN as LOGIN_ENTRY, editor, field, newItem } from "./steps/editor.ts"
import { answerGate } from "./steps/gate.ts"
import { alert, go, lock, vaultOpen } from "./steps/nav.ts"
import { openPalette, paletteResults } from "./steps/search.ts"
import { lookUp, people, theyMatch, yourCode } from "./steps/sharing.ts"
import { signIn, unlockForm } from "./steps/unlock.ts"

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
  const env = { ...process.env, LOCKDOWN_MASTER_PASSWORD: PASSWORD }
  const run = spawnSync(cli, ["--root", path.join(home, who, "identity"), ...args], { input, env, encoding: "utf8", timeout: 120_000 })
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
  test.setTimeout(900_000)
  home = mkdtempSync(path.join(tmpdir(), "lockdown-web-finish-"))
  for (const [who, login] of [
    ["a", A],
    ["b", B],
    ["c", C],
  ] as const) {
    const kit = lockdown(["init", "--name", who.toUpperCase()], undefined, who)
    keys[who] = /^Secret Key:\s*(\S+)/m.exec(kit)?.[1] ?? ""
    lockdown(["account", "register", "--server", baseURL!, "--login", login], undefined, who)
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
  await signIn(page, { a: A, b: B, c: C }[who], PASSWORD, keys[who]!)
  await expect(vaultOpen(page)).toBeVisible({ timeout: 90_000 })
  return page
}

test("quick access asks the core's index: a typo is forgiven and says so", async ({ browser }) => {
  const context = await browser.newContext()
  const page = await signedIn(context, "a")
  const input = await openPalette(page)
  await expect(input).toBeFocused()
  await input.fill("bnak")
  const option = paletteResults(page, /Bank/)
  await expect(option).toBeVisible()
  await expect(option).toContainText("Close match")
  await input.fill("youtube tv")
  await expect(paletteResults(page)).toHaveCount(1)
  await context.close()
})

test("a TV's code read by the tab's camera opens the real page after the password", async ({ browser }) => {
  const context = await browser.newContext()
  await context.route("https://www.youtube.com/**", (r) => r.fulfill({ contentType: "text/html", body: "<!doctype html><title>Activate</title><h1>Signed in</h1>" }))
  const page = await signedIn(context, "a")
  await go(page, "codes")
  const dialog = await openTvSignIn(page)
  // The fake camera shows a picture of the TV's code; the core reads the frames.
  await expect(scannedHost(dialog, "www.youtube.com")).toBeVisible({ timeout: 30_000 })
  await expect(scannedCode(dialog, "WDJB-MJHT")).toBeVisible()
  await pickLogin(dialog, /YouTube TV/)
  const opened = context.waitForEvent("page")
  await signInOn(dialog, "www.youtube.com")
  // The Approve gate: this tab's route is the password (unicron's gate
  // still calls it the account password).
  await answerGate(page, PASSWORD)
  const tv = await opened
  await expect.poll(() => tv.url()).toBe("https://www.youtube.com/activate?user_code=WDJB-MJHT")
  await context.close()
})

test("a paste of the same code is refused by the proximity rule", async ({ browser }) => {
  const context = await browser.newContext()
  const page = await signedIn(context, "a")
  await go(page, "codes")
  const dialog = await openTvSignIn(page)
  await pasteAddress(dialog, "https://www.youtube.com/activate?user_code=QQQQ-RRRR")
  await expect(alert(dialog)).toContainText("screen in front of you")
  await expect(signInOffers(dialog)).toHaveCount(0)
  await context.close()
})

test("the contacts screen checks someone's keys by the code they paste over", async ({ browser }) => {
  const theirs = await browser.newContext()
  const b = await signedIn(theirs, "b")
  await go(b, "people")
  const code = (await yourCode(b).textContent())!.trim()
  expect(code).toMatch(/^lockdown:contact:v1:/)
  await theirs.close()

  const mine = await browser.newContext()
  const a = await signedIn(mine, "a")
  await go(a, "people")
  const compare = await lookUp(a, B)
  await expect(compare).toBeVisible({ timeout: 60_000 })
  // A wrong code is refused; theirs pins them as verified.
  await theyMatch(compare, "lockdown:contact:v1:not-them:00")
  await expect(alert(compare)).toBeVisible()
  await theyMatch(compare, code)
  await expect(people(a)).toContainText("Fingerprint compared")
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

  await newItem(page, LOGIN_ENTRY, true)
  const typed = field(editor(page), "Password", true)
  await typed.fill(TYPED)
  expect(await inHeap(), "while typed, the page holds it (D91)").toBe(true)

  // The tab goes out of view: the field and React's copy of it are cleared.
  await page.evaluate(() => {
    Object.defineProperty(document, "visibilityState", { configurable: true, get: () => "hidden" })
    document.dispatchEvent(new Event("visibilitychange"))
  })
  await expect(typed).toHaveValue("")
  const afterHide = await inHeap()

  // Locked: every view that held it is gone.
  await page.evaluate(() => {
    Object.defineProperty(document, "visibilityState", { configurable: true, get: () => "visible" })
  })
  await page.keyboard.press("Escape")
  await lock(page)
  await expect(unlockForm(page)).toBeVisible()
  const afterLock = await inHeap()
  test.info().annotations.push({ type: "measure", description: `typed password in the page's heap: after hide ${afterHide}, after lock ${afterLock}` })
  console.log(`measure: typed password in the page's heap after hide=${afterHide}, after lock=${afterLock}`)
  expect(afterLock, "nothing the page can reach still holds the typed password once locked").toBe(false)
  await context.close()
})
