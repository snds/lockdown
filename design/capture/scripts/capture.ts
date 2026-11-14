// The capture: every state of the built harness (dist/), at each width,
// light and dark (the vault list compact too), as PNGs with axe-core run on
// each, written outside the repo with a manifest.
//
//   pnpm build && pnpm capture [--out <dir>] [--only <id-prefix>]
//
// `--sizes 1440x900,390x844` takes fixed windows instead: each picture is
// that window exactly, never grown to its content, so it lines up with a
// design frame. A size is taken for a state drawn at a width of its class
// (a phone, under 600 px, or wider), so the desktop-only states get no
// phone picture. `pnpm shots` builds first and takes the two sizes above.
// Any size works: 768x1024 is the medium window and 844x390 a phone on its
// side (a window 600 px wide or wider is drawn as a desktop's).
// `--scale 2` takes each picture at twice the pixel density.
// `--strict` exits non-zero when any picture could not be taken, its state
// never said it was ready, or its steps stopped.
//
// `--built` (`pnpm shots:built --out <dir> [--only <id-prefix>]`) takes the
// finished screens for review: every state that names its place with
// `built: { area, screen, kind }`, at twice the density, the phone window
// (390x844) and the desktop one (1440x900) where the state is drawn at that
// class, an extension state at its natural size. Light is the stone ground
// and dark the graphite one. Files go to
// `<dir>/<drawn|undrawn>/<area>--<screen>__<phone|desktop>__<graphite|stone>.png`.
// Several lanes write into one folder at once, so this mode only ever writes
// the pictures it took: it deletes nothing, reads no earlier manifest, and
// names its own (`manifest--<run>.json`, with the run's log and axe results).
//
// Chromium comes from PLAYWRIGHT_BROWSERS_PATH (never installed by this
// script). The browser's clock is pinned, so countdowns and "edited 3 days
// ago" read the same in every run.

import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs"
import path from "node:path"
import { parseArgs } from "node:util"

import AxeBuilder from "@axe-core/playwright"
import { chromium, type Page } from "@playwright/test"

import { serve } from "./serve.ts"

interface StateMeta {
  id: string
  surface: "desktop" | "web" | "extension" | "server"
  screen: string
  state: string
  brief_row: string | null
  notes: string
  widths: number[] | "natural"
  compact: boolean
  frame: string | null
  trusted: string[]
  hold_clock?: boolean
  live_clock?: boolean
  built?: { area: string; screen: string; kind: "drawn" | "undrawn" } | null
}

interface Shot {
  id: string
  surface: StateMeta["surface"]
  screen: string
  state: string
  width: number | "natural"
  /** The fixed window's height (`--sizes`); `null` when the window grew to the content. */
  height: number | null
  theme: "light" | "dark"
  density: "comfortable" | "compact"
  file: string
  brief_row: string | null
  notes: string
}

const here = import.meta.dirname
const { values: args } = parseArgs({
  options: {
    out: { type: "string", default: "/home/user/design-capture/2026-10-29" },
    only: { type: "string", multiple: true },
    themes: { type: "string", default: "light,dark" },
    widths: { type: "string" },
    sizes: { type: "string" },
    scale: { type: "string" },
    built: { type: "boolean", default: false },
    strict: { type: "boolean", default: false },
    jobs: { type: "string", default: "3" },
    "skip-axe": { type: "boolean", default: false },
  },
})

if (args.built && !process.argv.some((a) => a === "--out" || a.startsWith("--out="))) throw new Error("pnpm shots:built needs --out <dir>: the folder the review pictures go to")
const OUT = path.resolve(args.out)
const PINNED = new Date("2026-10-29T16:20:00-07:00")
const BASE_HEIGHT = 800
const MAX_HEIGHT = 3000
/** Under this width a window is a phone's (the harness's 390); at or over it, a desktop's. */
const PHONE_BELOW = 600

/** The review pictures' two windows (`--built`). */
const BUILT_SIZES = "1440x900,390x844"
const SCALE = Number(args.scale ?? (args.built ? 2 : 1))
if (!(SCALE >= 1 && SCALE <= 4)) throw new Error(`--scale takes a pixel density from 1 to 4, not ${args.scale}`)
/** This run's name, so runs that share a folder never write one another's manifest. */
const RUN = `${new Date().toISOString().replace(/[:.]/g, "-")}-${process.pid}`

const sizes = (args.sizes ?? (args.built ? BUILT_SIZES : ""))
  .split(",")
  .filter(Boolean)
  .map((s) => {
    const m = /^(\d+)x(\d+)$/.exec(s.trim())
    if (!m) throw new Error(`--sizes takes WIDTHxHEIGHT pairs (1440x900,390x844), not ${s}`)
    return { width: Number(m[1]), height: Number(m[2]) }
  })

const imageId = (state: string, width: number | "natural", height: number | null, theme: string, density: string) =>
  `${state}--${width === "natural" ? "natural" : `w${width}${height ? `x${height}` : ""}`}-${theme}${density === "compact" ? "-compact" : ""}`

/** Where a finished screen's picture goes: `<drawn|undrawn>/<area>--<screen>__<phone|desktop>__<graphite|stone>.png`. */
function builtFile(s: StateMeta, width: number | "natural", theme: string): string {
  const b = s.built!
  for (const part of [b.area, b.screen]) if (!/^[a-z0-9]+(-[a-z0-9]+)*$/.test(part)) throw new Error(`${s.id}: built.area and built.screen are lower-case words joined by hyphens, not "${part}"`)
  const window = width === "natural" || width >= PHONE_BELOW ? "desktop" : "phone"
  return `${b.kind}/${b.area}--${b.screen}__${window}__${theme === "dark" ? "graphite" : "stone"}.png`
}

/** How much taller the window must be for every scrolling pane to show its whole content. */
async function overflow(page: Page): Promise<number> {
  return page.evaluate(() => {
    // With a dialog or a sheet open, only what it holds counts: the page behind it stays as it is.
    const modal = Array.from(document.querySelectorAll<HTMLElement>('[role="dialog"], [role="alertdialog"]')).filter((d) => d.getBoundingClientRect().height > 0)
    let extra = modal.length ? 0 : Math.max(0, document.documentElement.scrollHeight - window.innerHeight)
    const scope = modal.length ? modal.flatMap((d) => [d, ...Array.from(d.querySelectorAll<HTMLElement>("*"))]) : Array.from(document.querySelectorAll<HTMLElement>("body *"))
    for (const el of scope) {
      const s = getComputedStyle(el)
      if (!/(auto|scroll)/.test(s.overflowY) || el.clientHeight === 0) continue
      // A dialog's body grows with the window anyway; a pane's content is what is hidden.
      extra = Math.max(extra, el.scrollHeight - el.clientHeight)
    }
    return extra
  })
}

async function main() {
  mkdirSync(OUT, { recursive: true })
  const site = await serve(path.join(here, "../dist"))
  const browser = await chromium.launch()
  const themes = args.themes.split(",") as Array<"light" | "dark">

  // The state list, from the page itself.
  const probe = await browser.newPage()
  await probe.goto(site.url)
  await probe.waitForFunction(() => (window as unknown as { __LD_STATES__?: unknown }).__LD_STATES__)
  const states = (await probe.evaluate(() => (window as unknown as { __LD_STATES__: unknown }).__LD_STATES__)) as StateMeta[]
  await probe.close()
  const wanted = states.filter((s) => (!args.built || s.built) && (!args.only?.length || args.only.some((p) => s.id.startsWith(p))))
  if (args.built) {
    // Two states that name one place would overwrite each other's picture.
    const places = new Map<string, string>()
    for (const s of states.filter((x) => x.built)) {
      const place = `${s.built!.kind}/${s.built!.area}--${s.built!.screen}`
      if (places.has(place)) throw new Error(`${s.id} and ${places.get(place)} both name the built screen ${place}`)
      places.set(place, s.id)
    }
    for (const kind of new Set(wanted.map((s) => s.built!.kind))) mkdirSync(path.join(OUT, kind), { recursive: true })
  }

  const manifest: Shot[] = []
  const axe: Record<string, unknown> = {}
  const log: Record<string, unknown> = {}
  let n = 0

  const only = args.widths?.split(",").map(Number)
  const jobs = wanted.flatMap((s) => {
    const views: Array<{ width: number | "natural"; height: number | null }> =
      s.widths === "natural"
        ? [{ width: "natural", height: null }]
        : [
            // With --sizes alone, only the fixed windows; --widths adds grown ones back.
            ...(sizes.length && !only ? [] : s.widths.filter((w) => !only || only.includes(w)).map((width) => ({ width, height: null }))),
            ...sizes.filter((z) => (s.widths as number[]).some((w) => (w < PHONE_BELOW) === (z.width < PHONE_BELOW))),
          ]
    return views.flatMap(({ width, height }) =>
      themes.flatMap((theme) => (s.compact && !args.built ? (["comfortable", "compact"] as const) : (["comfortable"] as const)).map((density) => ({ s, width, height, theme, density }))),
    )
  })
  const total = jobs.length
  const failed: string[] = []

  async function shoot({ s, width, height, theme, density }: (typeof jobs)[number]) {
    const id = imageId(s.id, width, height, theme, density)
    const ctx = await browser.newContext({
      viewport: { width: width === "natural" ? 900 : width, height: height ?? BASE_HEIGHT },
      deviceScaleFactor: SCALE,
      colorScheme: theme,
      reducedMotion: "reduce",
      locale: "en-US",
      timezoneId: "America/Los_Angeles",
    })
    const page = await ctx.newPage()
    const errors: string[] = []
    page.on("console", (m) => {
      if (m.type() === "error") errors.push(m.text())
    })
    page.on("pageerror", (e) => errors.push(`uncaught: ${e.message}`))
    // A state that hangs the page is recorded and skipped, never waited on forever.
    const guard = setTimeout(() => void ctx.close().catch(() => {}), 120_000)
    try {
      // The extension's frames arm their buttons by Date.now(), so a frozen clock would keep them disabled:
      // those states run on the real clock. A state that shows the arming wait itself holds it on purpose.
      if (!s.live_clock || s.hold_clock) await page.clock.setFixedTime(PINNED)
      await page.addInitScript(() => {
        ;(window as unknown as { __LD_CAPTURE__: boolean }).__LD_CAPTURE__ = true
      })
      await page.goto(`${site.url}#/${id}`)
      const ready = await page
        .waitForFunction(() => document.documentElement.dataset.captureReady === "1", null, { timeout: 30_000 })
        .then(() => true)
        .catch(() => false)
      const result = (await page.evaluate(() => (window as unknown as { __LD_RESULT__?: unknown }).__LD_RESULT__ ?? null)) as {
        ok: boolean
        error: string | null
        missing: string[]
      } | null

      // Real clicks inside an extension frame, which ignores synthetic ones.
      for (const name of s.trusted) {
        await page.frameLocator("iframe[name^='ld-extframe:']").getByRole("button", { name, exact: false }).first().click()
        await page.waitForTimeout(1500)
      }

      const file = args.built ? builtFile(s, width, theme) : `${id}.png`
      if (width === "natural") {
        const el = page.locator(s.frame ?? "#root > *").first()
        await el.screenshot({ path: path.join(OUT, file), animations: "disabled" })
      } else {
        // A fixed window stays the size of the design's frame; anything below the fold is left out.
        const extra = height === null ? await overflow(page) : 0
        if (extra > 0) {
          await page.setViewportSize({ width, height: Math.min(MAX_HEIGHT, BASE_HEIGHT + extra) })
          await page.waitForTimeout(250)
        }
        await page.screenshot({ path: path.join(OUT, file), animations: "disabled" })
      }

      if (!args["skip-axe"]) {
        const res = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa", "best-practice"]).analyze()
        axe[id] = res.violations.map((v) => ({
          rule: v.id,
          impact: v.impact ?? null,
          count: v.nodes.length,
          target: v.nodes[0]?.target.join(" ") ?? null,
          summary: v.help,
        }))
      }

      const notes = [s.notes]
      if (!ready) notes.push("The state did not report ready within 30 s.")
      if (result && !result.ok) notes.push(`Steps stopped: ${result.error}.`)
      if (result?.missing.length) notes.push(`The fixture had no answer for: ${result.missing.join(", ")}.`)
      if (!ready) failed.push(`${id}: never reported ready`)
      else if (result && !result.ok) failed.push(`${id}: steps stopped: ${result.error}`)
      manifest.push({ id, surface: s.surface, screen: s.screen, state: s.state, width, height, theme, density, file, brief_row: s.brief_row, notes: notes.join(" ") })
      log[id] = { ready, steps: result, console_errors: errors }
      n += 1
      console.log(`${String(n).padStart(4)}/${total} ${id}${result && !result.ok ? `  (steps: ${result.error})` : ""}${errors.length ? `  (${errors.length} console errors: ${errors[0]?.slice(0, 160)})` : ""}`)
    } catch (e) {
      n += 1
      failed.push(`${id}: no picture: ${e instanceof Error ? e.message.split("\n")[0] : String(e)}`)
      log[id] = { ready: false, failed: e instanceof Error ? e.message : String(e), console_errors: errors }
      console.log(`${String(n).padStart(4)}/${total} ${id}  FAILED: ${e instanceof Error ? e.message.split("\n")[0] : String(e)}`)
    } finally {
      clearTimeout(guard)
      await ctx.close().catch(() => {})
    }
  }

  // A few pages at once; each has its own context, so nothing carries over.
  let next = 0
  const worker = async () => {
    while (next < jobs.length) await shoot(jobs[next++]!)
  }
  await Promise.all(Array.from({ length: Number(args.jobs) }, worker))

  await browser.close()
  await site.close()
  if (args.built) {
    // Only this run's pictures, under this run's name: other lanes' files and manifests stay as they are.
    const name = `manifest--${RUN}.json`
    const order = new Map(jobs.map((j, i) => [imageId(j.s.id, j.width, j.height, j.theme, j.density), i]))
    manifest.sort((a, b) => order.get(a.id)! - order.get(b.id)!)
    writeFileSync(path.join(OUT, name), JSON.stringify({ run: RUN, scale: SCALE, shots: manifest, log, ...(args["skip-axe"] ? {} : { axe }) }, null, 2) + "\n")
    console.log(`${manifest.length} images in ${OUT} (${name})`)
    if (!wanted.length) console.log(`no state names a built screen${args.only?.length ? ` under ${args.only.join(", ")}` : ""}`)
    report()
    return
  }
  // The manifest in the states' order, whatever order the pages finished in.
  // `--only` re-takes some states: their entries replace the earlier run's, the rest stay.
  const read = <T,>(name: string, empty: T): T => (args.only?.length && existsSync(path.join(OUT, name)) ? (JSON.parse(readFileSync(path.join(OUT, name), "utf8")) as T) : empty)
  // A re-taken state's earlier images all go, even at a width it no longer has.
  const retaken = new Set(wanted.map((st) => st.id))
  const earlier = read<Shot[]>("manifest.json", [])
  for (const m of earlier) if (retaken.has(m.id.split("--")[0]!) && !manifest.some((x) => x.id === m.id)) rmSync(path.join(OUT, m.file), { force: true })
  const allShots = [...earlier.filter((m) => !retaken.has(m.id.split("--")[0]!)), ...manifest]
  const keep = new Set(allShots.map((m) => m.id))
  const prune = <T,>(o: Record<string, T>) => Object.fromEntries(Object.entries(o).filter(([k]) => keep.has(k)))
  const rank = new Map(states.map((st, i) => [st.id, i]))
  const key = (m: Shot) => [rank.get(m.id.split("--")[0]!) ?? 0, m.width === "natural" ? 0 : -m.width, m.theme === "light" ? 0 : 1, m.density === "compact" ? 1 : 0]
  allShots.sort((a, b) => {
    const [x, y] = [key(a), key(b)]
    return x.findIndex((v, i) => v !== y[i]) < 0 ? 0 : x[x.findIndex((v, i) => v !== y[i])]! - y[x.findIndex((v, i) => v !== y[i])]!
  })
  writeFileSync(path.join(OUT, "manifest.json"), JSON.stringify(allShots, null, 2) + "\n")
  if (!args["skip-axe"]) writeFileSync(path.join(OUT, "axe.json"), JSON.stringify(prune({ ...read<Record<string, unknown>>("axe.json", {}), ...axe }), null, 2) + "\n")
  writeFileSync(path.join(OUT, "capture-log.json"), JSON.stringify(prune({ ...read<Record<string, unknown>>("capture-log.json", {}), ...log }), null, 2) + "\n")
  console.log(`${manifest.length} images in ${OUT}`)
  report()

  function report() {
    if (!failed.length) return
    console.log(`${failed.length} of ${total} not as the state says (${args.built ? "this run's manifest" : "capture-log.json"} has each one's console):\n  ${failed.sort().join("\n  ")}`)
    if (args.strict) process.exitCode = 1
  }
}

await main()
