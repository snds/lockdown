// The engine check: the built harness (dist/) opened in each browser engine
// the desktop app ships on, asserting that unicron's icons draw as one glyph,
// that the print stylesheet prints a page alone, and taking a short smoke of
// the main desktop screens as PNGs. The Mac app runs in WKWebView, so WebKit
// is the engine that matters; Chromium runs the same assertions so the check
// is honest (docs/plan/38-mac-fixes.md §2).
//
//   pnpm build && pnpm engine:check [--engines chromium,webkit] [--out <dir>]
//
// Browsers come from PLAYWRIGHT_BROWSERS_PATH; this script never installs
// one (CI installs WebKit on its runner first). Exits 1 when any assertion
// fails in any engine, after writing every engine's report and screenshots.

import { mkdirSync, writeFileSync } from "node:fs"
import path from "node:path"
import { parseArgs } from "node:util"

import { chromium, firefox, webkit, type BrowserType, type Page } from "@playwright/test"

import { serve } from "./serve.ts"

const here = import.meta.dirname
const { values: args } = parseArgs({
  options: {
    engines: { type: "string", default: "chromium,webkit" },
    out: { type: "string", default: "/home/user/design-capture/engine-check" },
  },
})

const OUT = path.resolve(args.out)
const PINNED = new Date("2026-10-29T16:20:00-07:00")
const ENGINES: Record<string, BrowserType> = { chromium, webkit, firefox }

/** The main desktop screens: first run, unlock, the vault list and an item, settings, status, the kit page. */
const SMOKE = [
  "desktop-first-run-welcome",
  "desktop-unlock-locked",
  "desktop-vault-filled",
  "desktop-item-login-full",
  "desktop-settings",
  "desktop-status",
  "desktop-first-run-kit",
  "desktop-recovery-emergency-sheet",
]

/** States whose open dialog is a page that prints alone (`[data-print-page]`, index.css). */
const PRINTED = ["desktop-first-run-kit", "desktop-recovery-emergency-sheet"]

/** One icon as the engine drew it: its name, and how wide its glyph came out against the icon's em. */
interface IconMeasure {
  name: string
  /** The `<text>`'s advance in the SVG's own units (the icon is set at 24 of them). */
  advance: number
  /** The text's box, in the same units. */
  box: { width: number; height: number }
  /** Drawn by its name (the ligature) rather than by its codepoint. */
  byName: boolean
}

interface StateReport {
  state: string
  ready: boolean
  fontLoaded: boolean
  fontFaces: Array<{ family: string; status: string }>
  icons: IconMeasure[]
  wrong: IconMeasure[]
  print: { rootHidden: boolean | null; pageShown: boolean | null } | null
  screenshot: string
  errors: string[]
}

/** The width one glyph may take, in SVG units: the icon's em is 24; a ligature's advance is that em. */
const ONE_GLYPH = { min: 16, max: 30 }

async function measure(page: Page): Promise<{ fontLoaded: boolean; fontFaces: StateReport["fontFaces"]; icons: IconMeasure[] }> {
  return page.evaluate(async () => {
    await document.fonts.ready
    const fontLoaded = document.fonts.check('24px "Material Symbols Rounded"')
    const fontFaces: Array<{ family: string; status: string }> = []
    document.fonts.forEach((f) => fontFaces.push({ family: f.family, status: f.status }))
    const icons: IconMeasure[] = []
    for (const svg of Array.from(document.querySelectorAll<SVGSVGElement>('svg[data-slot="icon"]'))) {
      const r = svg.getBoundingClientRect()
      if (r.width === 0 || r.height === 0) continue
      const text = svg.querySelector("text")
      // An icon drawn some other way (a path, a foreignObject) is measured by its box against its em.
      if (!text) {
        const glyph = svg.querySelector<SVGGraphicsElement>("[data-glyph]") ?? svg
        const b = glyph.getBBox()
        icons.push({ name: svg.getAttribute("data-icon-name") ?? "?", advance: b.width, box: { width: b.width, height: b.height }, byName: false })
        continue
      }
      const name = text.getAttribute("data-icon-name") ?? text.textContent ?? ""
      const box = text.getBBox()
      icons.push({ name, advance: text.getComputedTextLength(), box: { width: box.width, height: box.height }, byName: text.textContent === name })
    }
    return { fontLoaded, fontFaces, icons }
  })
}

/** The open page under print media: whether the app behind it is gone and the page shown, and how it would look on paper. */
async function checkPrint(page: Page, shot: string): Promise<NonNullable<StateReport["print"]>> {
  await page.emulateMedia({ media: "print" })
  const r = await page.evaluate(() => {
    const root = document.getElementById("root")
    const sheet = document.querySelector<HTMLElement>("[data-print-page]")
    return {
      rootHidden: root ? getComputedStyle(root).display === "none" : null,
      pageShown: sheet ? sheet.getBoundingClientRect().height > 0 && getComputedStyle(sheet).visibility !== "hidden" : null,
    }
  })
  await page.screenshot({ path: shot, fullPage: true, animations: "disabled" })
  await page.emulateMedia({ media: "screen" })
  return r
}

async function runEngine(name: string, site: string): Promise<{ engine: string; version: string; states: StateReport[]; failures: string[] }> {
  const type = ENGINES[name]
  if (!type) throw new Error(`no engine named ${name}`)
  const browser = await type.launch()
  const version = browser.version()
  const dir = path.join(OUT, name)
  mkdirSync(dir, { recursive: true })
  const states: StateReport[] = []
  const failures: string[] = []
  for (const state of SMOKE) {
    const ctx = await browser.newContext({ viewport: { width: 1280, height: 860 }, deviceScaleFactor: 1, colorScheme: "light", reducedMotion: "reduce", locale: "en-US", timezoneId: "America/Los_Angeles" })
    const page = await ctx.newPage()
    const errors: string[] = []
    page.on("console", (m) => {
      if (m.type() === "error") errors.push(m.text())
    })
    page.on("pageerror", (e) => errors.push(`uncaught: ${e.message}`))
    try {
      await page.clock.setFixedTime(PINNED)
      await page.addInitScript(() => {
        ;(window as unknown as { __LD_CAPTURE__: boolean }).__LD_CAPTURE__ = true
      })
      await page.goto(`${site}#/${state}--w1280-light`)
      const ready = await page
        .waitForFunction(() => document.documentElement.dataset.captureReady === "1", null, { timeout: 30_000 })
        .then(() => true)
        .catch(() => false)
      const m = await measure(page)
      const wrong = m.icons.filter((i) => i.advance < ONE_GLYPH.min || i.advance > ONE_GLYPH.max)
      const print = PRINTED.includes(state) ? await checkPrint(page, path.join(dir, `${state}--print.png`)) : null
      const screenshot = `${state}.png`
      await page.screenshot({ path: path.join(dir, screenshot), animations: "disabled" })
      states.push({ state, ready, fontLoaded: m.fontLoaded, fontFaces: m.fontFaces, icons: m.icons, wrong, print, screenshot, errors })
      if (!ready) failures.push(`${state}: the state did not report ready`)
      // Engines load a face only once something uses it, so a screen with no icon has no font to check.
      if (m.icons.length > 0 && !m.fontLoaded) failures.push(`${state}: the Material Symbols font did not load`)
      if (wrong.length) {
        const sample = wrong.slice(0, 4).map((i) => `${i.name} ${i.advance.toFixed(1)} wide`)
        failures.push(`${state}: ${wrong.length} of ${m.icons.length} icons are not one glyph (${sample.join("; ")})`)
      }
      // Every icon by its codepoint: a name the build missed would draw as a ligature, which Apple's WebKit leaves blank.
      const byName = [...new Set(m.icons.filter((i) => i.byName).map((i) => i.name))]
      if (byName.length) failures.push(`${state}: ${byName.length} icons drawn by name, not codepoint (${byName.slice(0, 6).join(", ")})`)
      if (print && (print.rootHidden !== true || print.pageShown !== true)) {
        failures.push(`${state}: printing would not print the page alone (app hidden: ${print.rootHidden}, page shown: ${print.pageShown})`)
      }
    } catch (e) {
      failures.push(`${state}: ${e instanceof Error ? e.message.split("\n")[0] : String(e)}`)
    } finally {
      await ctx.close().catch(() => {})
    }
  }
  await browser.close()
  return { engine: name, version, states, failures }
}

async function main() {
  mkdirSync(OUT, { recursive: true })
  const site = await serve(path.join(here, "../dist"))
  const engines = args.engines.split(",").map((e) => e.trim()).filter(Boolean)
  const reports = []
  for (const engine of engines) reports.push(await runEngine(engine, site.url))
  await site.close()
  writeFileSync(path.join(OUT, "engine-check.json"), JSON.stringify(reports, null, 2) + "\n")
  let failed = false
  for (const r of reports) {
    const icons = r.states.reduce((n, s) => n + s.icons.length, 0)
    const wrong = r.states.reduce((n, s) => n + s.wrong.length, 0)
    const sample = r.states.flatMap((s) => s.icons).slice(0, 3).map((i) => `${i.name}: ${i.advance.toFixed(1)} wide`)
    console.log(`${r.engine} ${r.version}: ${icons} icons over ${r.states.length} screens, ${wrong} not one glyph; e.g. ${sample.join("; ")}`)
    if (icons === 0) r.failures.push("no icon was drawn on any screen")
    for (const f of r.failures) console.log(`  FAIL ${f}`)
    if (r.failures.length) failed = true
  }
  console.log(`report and screenshots in ${OUT}`)
  if (failed) process.exit(1)
}

await main()
