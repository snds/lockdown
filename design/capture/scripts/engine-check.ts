// The engine check: the built harness (dist/) opened in each browser engine
// the desktop app ships on, asserting that unicron's icons draw, that the
// text faces load, that the print stylesheet prints a page alone, and taking
// a short smoke of the main desktop screens as PNGs. The Mac app runs in
// WKWebView, so WebKit is the engine that matters; Chromium runs the same
// assertions so the check is honest (docs/plan/38-mac-fixes.md §2).
//
// Icons: until unicron 1.0 an icon was a ligature in an icon font, which
// Apple's WebKit did not shape inside SVG text, so this check measured that
// each drew as one glyph. From 1.0 an icon is IBM Carbon's own drawing as
// inline SVG (docs/plan/62-unicron-1-0.md §2), so the check is now: every
// `svg[data-slot=icon]` on the page has Carbon's 32-unit view box and a
// drawing in it, none is the empty box unicron draws for a name it does not
// know (a name that arrived as data and was not translated), and every icon
// the engine lays out has a box and a drawing that are not empty.
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

/**
 * The main desktop screens: first run, unlock, the vault list and an item, settings, status, the kit page;
 * and the "New item" menu, whose icons are names the core's templates send as data.
 */
const SMOKE = [
  "desktop-first-run-welcome",
  "desktop-unlock-locked",
  "desktop-vault-filled",
  "desktop-item-login-full",
  "desktop-vault-new-item-menu",
  "desktop-settings",
  "desktop-status",
  "desktop-first-run-kit",
  "desktop-recovery-emergency-sheet",
]

/** States whose open dialog is a page that prints alone (`[data-print-page]`, index.css). */
const PRINTED = ["desktop-first-run-kit", "desktop-recovery-emergency-sheet"]

/** unicron's icon: IBM Carbon's 32-unit art (unicron contract §4). */
const VIEW_BOX = "0 0 32 32"

/** The two text families unicron's stylesheet sets everything in, as @fontsource-variable names them. */
const TEXT_FACES = ["Geist Variable", "Geist Mono Variable"]

/** One icon as the engine drew it. */
interface IconMeasure {
  name: string
  viewBox: string | null
  /** How many shapes (`path`, `circle`, `rect`) the svg holds. */
  shapes: number
  /** unicron marks a name it has no drawing for (`data-icon-missing`) and draws an empty box. */
  missing: boolean
  /** Whether the engine lays the svg out (an icon inside something hidden is not). */
  laidOut: boolean
  /** The svg's box on the page, in CSS pixels. */
  box: { width: number; height: number }
  /** The drawing's own box inside the svg, in the svg's units; `null` when the icon is not laid out. */
  drawing: { width: number; height: number } | null
}

interface StateReport {
  state: string
  ready: boolean
  fontFaces: Array<{ family: string; status: string }>
  /** The text families of TEXT_FACES with a face the engine loaded. */
  facesLoaded: string[]
  icons: IconMeasure[]
  wrong: Array<IconMeasure & { why: string }>
  print: { rootHidden: boolean | null; pageShown: boolean | null } | null
  screenshot: string
  errors: string[]
}

/** Why an icon is not drawn as it should be, or `null` when it is. */
function wrongIcon(i: IconMeasure): string | null {
  if (i.missing) return "unicron has no drawing for this name"
  if (i.viewBox !== VIEW_BOX) return `view box ${i.viewBox ?? "none"}, not ${VIEW_BOX}`
  if (i.shapes === 0) return "no path in it"
  if (!i.laidOut) return null
  if (i.box.width < 1 || i.box.height < 1) return `an empty box (${i.box.width.toFixed(1)} × ${i.box.height.toFixed(1)})`
  if (!i.drawing || i.drawing.width <= 0 || i.drawing.height <= 0) return "a drawing with no size"
  return null
}

async function measure(page: Page): Promise<{ fontFaces: StateReport["fontFaces"]; icons: IconMeasure[] }> {
  return page.evaluate(async () => {
    await document.fonts.ready
    const fontFaces: Array<{ family: string; status: string }> = []
    document.fonts.forEach((f) => fontFaces.push({ family: f.family.replace(/["']/g, ""), status: f.status }))
    const icons: IconMeasure[] = []
    for (const svg of Array.from(document.querySelectorAll<SVGSVGElement>('svg[data-slot="icon"]'))) {
      const r = svg.getBoundingClientRect()
      // Laid out: it has boxes. An icon under `display: none` has none, and its drawing can't be measured.
      const laidOut = svg.getClientRects().length > 0
      let drawing: IconMeasure["drawing"] = null
      if (laidOut) {
        try {
          const b = svg.getBBox()
          drawing = { width: b.width, height: b.height }
        } catch {
          drawing = { width: 0, height: 0 }
        }
      }
      icons.push({
        name: svg.getAttribute("data-icon-name") ?? "?",
        viewBox: svg.getAttribute("viewBox"),
        shapes: svg.querySelectorAll("path, circle, rect").length,
        missing: svg.hasAttribute("data-icon-missing"),
        laidOut,
        box: { width: r.width, height: r.height },
        drawing,
      })
    }
    return { fontFaces, icons }
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
      const wrong = m.icons.flatMap((i) => {
        const why = wrongIcon(i)
        return why ? [{ ...i, why }] : []
      })
      const facesLoaded = TEXT_FACES.filter((family) => m.fontFaces.some((f) => f.family === family && f.status === "loaded"))
      const print = PRINTED.includes(state) ? await checkPrint(page, path.join(dir, `${state}--print.png`)) : null
      const screenshot = `${state}.png`
      await page.screenshot({ path: path.join(dir, screenshot), animations: "disabled" })
      states.push({ state, ready, fontFaces: m.fontFaces, facesLoaded, icons: m.icons, wrong, print, screenshot, errors })
      if (!ready) failures.push(`${state}: the state did not report ready`)
      // Every screen sets words in Geist and labels in Geist Mono; the faces ship as files with the page (no CDN, no data: URL).
      for (const family of TEXT_FACES) if (!facesLoaded.includes(family)) failures.push(`${state}: the ${family} font did not load`)
      if (wrong.length) {
        const sample = wrong.slice(0, 4).map((i) => `${i.name}: ${i.why}`)
        failures.push(`${state}: ${wrong.length} of ${m.icons.length} icons are not drawn (${sample.join("; ")})`)
      }
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
    const laidOut = r.states.reduce((n, s) => n + s.icons.filter((i) => i.laidOut).length, 0)
    const wrong = r.states.reduce((n, s) => n + s.wrong.length, 0)
    const sample = r.states
      .flatMap((s) => s.icons)
      .filter((i) => i.laidOut)
      .slice(0, 3)
      .map((i) => `${i.name}: ${i.box.width.toFixed(0)} × ${i.box.height.toFixed(0)} px, ${i.shapes} shape${i.shapes === 1 ? "" : "s"}`)
    console.log(`${r.engine} ${r.version}: ${icons} icons over ${r.states.length} screens (${laidOut} laid out), ${wrong} not drawn; e.g. ${sample.join("; ")}`)
    if (laidOut === 0) r.failures.push("no icon was drawn on any screen")
    for (const f of r.failures) console.log(`  FAIL ${f}`)
    if (r.failures.length) failed = true
  }
  console.log(`report and screenshots in ${OUT}`)
  if (failed) process.exit(1)
}

await main()
