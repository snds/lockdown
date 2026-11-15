import { spawnSync } from "node:child_process"
import { mkdtempSync, rmSync } from "node:fs"
import { tmpdir } from "node:os"
import path from "node:path"

import { expect, test, type Page } from "@playwright/test"

import { LOGIN as LOGIN_ENTRY, add, closeManager, editor, field, firstField, generateWithOptions, generatorMake, generatorStrength, makeTemplate, newItem, save, templateNamed } from "./steps/editor.ts"
import { exportSwitch, guestCode, hideGuestCard, itemAction, openItem, plainText, row, rows, section, showGuestCard } from "./steps/items.ts"
import { alert, go, lock, root, vaultOpen } from "./steps/nav.ts"
import { search } from "./steps/search.ts"
import { signOut, theme } from "./steps/settings.ts"
import { askForSecret, linkDone, newLink, openAnswer, shareText, showAnswer } from "./steps/sharing.ts"
import { answerRequest, linkTitle, linkValue, openLink, requestAsk, requestSent, sharedHeading, showLinkValue, signIn as signInWith, signInForm, signInTab, unlock, unlockForm } from "./steps/unlock.ts"

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
  test.setTimeout(600_000)
  home = mkdtempSync(path.join(tmpdir(), "lockdown-web-e2e-cli-"))
  const kit = lockdown(["init", "--name", "First"])
  secretKey = /^Secret Key:\s*(\S+)/m.exec(kit)?.[1] ?? ""
  expect(secretKey, "the kit page names the Secret Key").toMatch(/^A1-/)
  lockdown(["account", "register", "--server", baseURL!, "--login", LOGIN])
  lockdown(["add", "login", "--site", "https://bank.example.com", "--username", "me", "--title", "Bank from the desktop", "--password-stdin"], "a-bank-password-1\n")
  lockdown(["sync"])
  const linksKit = lockdown(["init", "--name", "Links"], undefined, "links")
  linksSecretKey = /^Secret Key:\s*(\S+)/m.exec(linksKit)?.[1] ?? ""
  lockdown(["account", "register", "--server", baseURL!, "--login", LINKS_LOGIN], undefined, "links")
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

const signIn = (page: Page, secretKey: string, login = LOGIN) => signInWith(page, login, PASSWORD, secretKey)

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
  const source = await res!.text()
  expect(source).toMatch(/<script[^>]+integrity="sha384-/)

  // The password alone doesn't open a new device.
  await signIn(page, "A1-NOT-THE-KEY")
  await expect(alert(signInForm(page))).toBeVisible({ timeout: 90_000 })

  // With the kit's Secret Key it does, and the desktop's login is there.
  await signIn(page, secretKey)
  await expect(vaultOpen(page)).toBeVisible({ timeout: 90_000 })
  await expect(row(page, /Bank from the desktop/)).toBeVisible()

  // Create a login.
  await newItem(page, LOGIN_ENTRY, true)
  const created = editor(page)
  await field(created, "Title").fill("Example mail")
  await field(created, "Username").fill("me@example.com")
  await field(created, "Password", true).fill("a-long-unique-password-1")
  await firstField(created, "Website").fill("https://mail.example.com")
  await add(created)
  await expect(created).toBeHidden()
  const made = row(page, /Example mail/)
  await expect(made).toBeVisible()

  // Edit it.
  await made.click()
  await itemAction(page, "Edit")
  const edit = editor(page)
  await field(edit, "Title").fill("Example mail (work)")
  await save(edit)
  await expect(edit).toBeHidden()

  // Find it, and only it.
  const find = search(page)
  await find.fill("work")
  await expect(row(page, /Example mail \(work\)/)).toBeVisible()
  await find.fill("nothing like this anywhere")
  await expect(rows(page, /Example mail/)).toHaveCount(0)
  await find.fill("")

  // Lock: the keys go; the password opens it again.
  await lock(page)
  await expect(unlockForm(page)).toBeVisible()
  await unlock(page, "not the password at all")
  await expect(alert(page)).toContainText("didn't match")
  await unlock(page, PASSWORD)
  await expect(row(page, /Example mail \(work\)/)).toBeVisible({ timeout: 60_000 })

  // The theme (doc 62 §3): this browser prefers light, so the page starts on stone, with System on.
  await go(page, "appearance")
  const html = root(page)
  await expect(html).toHaveAttribute("data-theme", "lockdown")
  await expect(html).not.toHaveClass(/(^|\s)dark(\s|$)/)
  await expect(theme(page, "System")).toBeChecked()
  // Graphite changes the whole page at once.
  await theme(page, "Graphite").click()
  await expect(html).toHaveClass(/(^|\s)dark(\s|$)/)
  await expect(theme(page, "Graphite")).toBeChecked()

  // Sign out: the edit reaches the server first, then the tab forgets it all.
  await go(page, "this-browser")
  await signOut(page)
  await expect(signInTab(page)).toBeVisible()
  const listed = lockdown(["sync"]) && lockdown(["list"])
  expect(listed).toContain("Example mail (work)")

  // The choice is the browser's, not the account's: it outlives the sign-out and a reload, and
  // the page's own bundle applies it at the start, under the policy (no inline script).
  await page.reload()
  await expect(signInTab(page)).toBeVisible()
  await expect(html).toHaveClass(/(^|\s)dark(\s|$)/)
  expect(await page.evaluate(() => window.localStorage.getItem("lockdown.theme"))).toBe("graphite")

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
  await expect(vaultOpen(tab)).toBeVisible({ timeout: 90_000 })
  await go(tab, "links")

  // A text link, with a password, that opens once.
  await shareText(tab, "Cabin Wi-Fi", "pine-needles-42", "told you on the phone")
  const shareLink = (await newLink(tab).textContent())!.trim()
  expect(shareLink).toMatch(new RegExp(`^${origin}/s/[0-9a-f-]+#`))
  await linkDone(tab)

  // A request link.
  await askForSecret(tab, "The alarm code")
  const requestLink = (await newLink(tab).textContent())!.trim()
  expect(requestLink).toMatch(new RegExp(`^${origin}/r/[0-9a-f-]+#`))
  await linkDone(tab)

  // Someone with no account opens the share link: the server's /s/ page
  // hands over to the web vault, and the key never leaves the fragment.
  const stranger = await browser.newContext({ permissions: ["clipboard-read", "clipboard-write"] })
  const page = await stranger.newPage()
  const { problems, origins } = watch(page)
  const sent: string[] = []
  page.on("request", (r) => sent.push(r.url()))
  await page.goto(shareLink)
  await expect(page).toHaveURL(/\/vault\/s\/[0-9a-f-]+$/)
  await expect(sharedHeading(page)).toBeVisible({ timeout: 60_000 })
  await openLink(page, "not it")
  await expect(alert(page)).toContainText("password")
  await openLink(page, "told you on the phone")
  await expect(linkTitle(page, "Cabin Wi-Fi")).toBeVisible({ timeout: 60_000 })
  await showLinkValue(page, "Text")
  await expect(linkValue(page, 0)).toHaveText("pine-needles-42")
  const key = shareLink.split("#")[1]!
  expect(sent.filter((u) => u.includes(key))).toEqual([])

  // And answers the request.
  await page.goto(requestLink)
  await expect(requestAsk(page, "The alarm code")).toBeVisible({ timeout: 60_000 })
  await answerRequest(page, "Alarm", "8642")
  await expect(requestSent(page)).toBeVisible({ timeout: 60_000 })

  // The asker reads it in the vault.
  await go(tab, "devices")
  await go(tab, "links")
  const opened = await openAnswer(tab)
  await expect(opened).toContainText("Alarm")
  await showAnswer(opened)
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
  await expect(vaultOpen(page)).toBeVisible({ timeout: 90_000 })

  // A passport: a variant of the identity kind, drawn from its own template.
  await newItem(page, /Passport$/)
  let dialog = editor(page)
  await field(dialog, "Title").fill("My passport")
  await field(dialog, "Passport number", true).fill("X1234567")
  await field(dialog, "Issuing country").fill("GB")
  await add(dialog)
  await expect(dialog).toBeHidden()
  await openItem(page, /My passport/)
  await expect(section(page, "Passport")).toContainText("Issuing country")

  // A template of your own, then an item from it.
  await newItem(page, /Make a template/)
  const manager = editor(page)
  await makeTemplate(manager, "Gym locker", "Locker")
  await expect(templateNamed(manager, "Gym locker")).toBeVisible()
  await closeManager(manager)
  await newItem(page, /Gym locker$/)
  dialog = editor(page)
  await field(dialog, "Locker", true).fill("41")
  await add(dialog)
  await expect(dialog).toBeHidden()
  await expect(row(page, /Gym locker/)).toBeVisible()

  // A Wi-Fi network whose password the core makes as a passphrase, then its guest card.
  await newItem(page, /Wi-Fi network$/)
  dialog = editor(page)
  await field(dialog, "Network name").fill("cabin-net")
  await generateWithOptions(dialog)
  await generatorMake(dialog).selectOption("words")
  await expect(generatorStrength(dialog)).toContainText("bits")
  await add(dialog)
  await expect(dialog).toBeHidden()
  await openItem(page, /cabin-net/)
  await showGuestCard(page)
  await expect(guestCode(page, "cabin-net")).toBeVisible()
  await hideGuestCard(page)
  await expect(guestCode(page, "cabin-net")).toBeHidden()

  expect([...origins]).toEqual([new URL(baseURL!).origin])
  expect(problems).toEqual([])
  await context.close()
})

test("a crown jewel's opt-in says plain exports leave it out and locked backups carry it (doc 48)", async ({ browser, baseURL }) => {
  const context = await browser.newContext()
  const page = await context.newPage()
  const { problems, origins } = watch(page)
  await page.goto("/vault/")
  await signIn(page, secretKey)
  await expect(vaultOpen(page)).toBeVisible({ timeout: 90_000 })

  // A wallet is a crown jewel by its kind.
  await newItem(page, /Crypto wallet$/)
  const dialog = editor(page)
  await field(dialog, "Title").fill("Cold wallet")
  await field(dialog, "Recovery phrase", true).fill("abandon ability able about above absent absorb abstract absurd abuse access accident")
  await add(dialog)
  await expect(dialog).toBeHidden()
  await openItem(page, /Cold wallet/)

  // The shared item view's words: plain exports leave it out unless opted in; locked backups carry it.
  const optIn = section(page, "Exports and backups")
  await expect(exportSwitch(optIn)).not.toBeChecked()
  await expect(optIn).toContainText("Crown jewels stay out of plain exports unless you include them here, one by one.")
  await expect(optIn).toContainText("Locked backups carry them unless you turn that off in Settings, Backups.")
  await expect(plainText(page, "abandon ability")).toHaveCount(0)

  expect([...origins]).toEqual([new URL(baseURL!).origin])
  expect(problems).toEqual([])
  await context.close()
})
