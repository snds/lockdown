import { spawnSync } from "node:child_process"
import { mkdtempSync, readFileSync, rmSync } from "node:fs"
import { tmpdir } from "node:os"
import path from "node:path"

import { expect, test, type Page } from "@playwright/test"

import { add, editor, field, newItem } from "./steps/editor.ts"
import { noticeControls, notices, row } from "./steps/items.ts"
import { blockingDialogs, go, vaultOpen } from "./steps/nav.ts"
import { signIn } from "./steps/unlock.ts"

/**
 * docs/plan/49-shared-vault-quiet.md in the web vault, in Chromium against
 * the real server: Alice's only vault is shared with Bob, who wrote to it
 * last. Her tab, a device that has met nobody, signs in and opens it after
 * its first sync. Then the server rolls back the head Bob just wrote,
 * after the tab took it: the vault shows one calm line naming him, keeps
 * what it had, and keeps the tab's own new note; once the server shows
 * the head again the line goes and the note goes out.
 */

const PASSWORD = "correct horse battery staple"
const LINE_START = "Some recent changes from bob-"
const LINE_END = "couldn't be verified, so Lockdown left them out. You don't need to do anything."
const port = process.env.LOCKDOWN_E2E_PORT ?? "8797"
const target = process.env.CARGO_TARGET_DIR ?? path.resolve(import.meta.dirname, "../../../target")
// `cargo build -p lockdown-devtools --no-default-features --features shared-fixture --bin lockdown-shared-fixture`
const fixtureBin = process.env.LOCKDOWN_SHARED_FIXTURE_BIN ?? path.join(target, "debug", "lockdown-shared-fixture")

let home = ""
let login = ""
let secretKey = ""

function fixture(args: string[]): string[] {
  const out = spawnSync(fixtureBin, args, { encoding: "utf8", timeout: 170_000 })
  if (out.status !== 0) throw new Error(`lockdown-shared-fixture ${args.join(" ")}: ${out.stderr || out.error}`)
  return out.stdout.trim().split("\n")
}

/** The server's database, from the note e2e/serve.mjs leaves. */
const db = () => path.join(readFileSync(path.join(tmpdir(), `lockdown-web-e2e-${port}.data`), "utf8").trim(), "lockdown-server.sqlite")

test.beforeAll(async ({ baseURL }) => {
  home = mkdtempSync(path.join(tmpdir(), "lockdown-web-quiet-"))
  ;[login, secretKey] = fixture(["setup", "--server", baseURL!, "--home", home])
})

test.afterAll(() => {
  if (home) rmSync(home, { recursive: true, force: true })
})

/** Adds a note in the open view: a change made here, so the tab syncs at once. */
async function addNote(page: Page, title: string) {
  await newItem(page, /Note$/, true)
  const dialog = editor(page)
  await field(dialog, "Title").fill(title)
  await add(dialog)
  await expect(dialog).toBeHidden()
}

test("a shared vault leaves out what doesn't verify, says so in one line, and takes it once it does", async ({ page }) => {
  const problems: string[] = []
  page.on("pageerror", (e) => problems.push(`page error: ${e.message}`))
  await page.goto("/vault/")
  await signIn(page, login, PASSWORD, secretKey)
  await expect(vaultOpen(page)).toBeVisible({ timeout: 90_000 })

  // The only vault opens once the tab's first sync has met Bob.
  await go(page, "personal")
  await expect(row(page, /Bob one/)).toBeVisible({ timeout: 60_000 })
  await expect(notices(page)).toHaveCount(0)

  // Bob writes again; the tab hears it and takes it.
  fixture(["write", "--home", home, "--title", "Bob two"])
  await expect(row(page, /Bob two/)).toBeVisible({ timeout: 60_000 })

  // The server drops that head. The tab's next sync (its own new note)
  // leaves it alone and says so in one line, and keeps everything it had.
  fixture(["roll-back", "--home", home, "--db", db()])
  await addNote(page, "Tab note")
  const note = notices(page).filter({ hasText: LINE_END })
  await expect(note).toBeVisible({ timeout: 60_000 })
  await expect(note).toContainText(LINE_START)
  await expect(noticeControls(note)).toHaveCount(0)
  await expect(blockingDialogs(page)).toHaveCount(0)
  await expect(row(page, /Bob two/)).toBeVisible()
  await expect(row(page, /Tab note/)).toBeVisible()

  // The server shows the head again: the next sync takes the chain, the
  // line goes by itself, and the tab's notes go out.
  fixture(["put-back", "--home", home, "--db", db()])
  await addNote(page, "Tab two")
  await expect(notices(page).filter({ hasText: LINE_END })).toHaveCount(0, { timeout: 60_000 })
  await expect(row(page, /Tab note/)).toBeVisible()
  expect(problems).toEqual([])
})
