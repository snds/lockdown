// The capture: every state of the built harness (dist/), at each width,
// light and dark (the vault list compact too), as PNGs with axe-core run on
// each, written outside the repo with a manifest.
//
//   pnpm build && pnpm capture [--out <dir>] [--only <id-prefix>]
//
// Chromium comes from PLAYWRIGHT_BROWSERS_PATH (never installed by this
// script). The browser's clock is pinned, so countdowns and "edited 3 days
// ago" read the same in every run.

import { mkdirSync, writeFileSync } from "node:fs"
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
}

interface Shot {
  id: string
  surface: StateMeta["surface"]
  screen: string
  state: string
  width: number | "natural"
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
    jobs: { type: "string", default: "3" },
    "skip-axe": { type: "boolean", default: false },
  },
})

const OUT = path.resolve(args.out)
const PINNED = new Date("2026-10-29T16:20:00-07:00")
const BASE_HEIGHT = 800
const MAX_HEIGHT = 3000

const imageId = (state: string, width: number | "natural", theme: string, density: string) =>
  `${state}--${width === "natural" ? "natural" : `w${width}`}-${theme}${density === "compact" ? "-compact" : ""}`

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
  const wanted = states.filter((s) => !args.only?.length || args.only.some((p) => s.id.startsWith(p)))

  const manifest: Shot[] = []
  const axe: Record<string, unknown> = {}
  const log: Record<string, unknown> = {}
  let n = 0

  const only = args.widths?.split(",").map(Number)
  const jobs = wanted.flatMap((s) => {
    const widths = s.widths === "natural" ? (["natural"] as const) : s.widths.filter((w) => !only || only.includes(w))
    return widths.flatMap((width) =>
      themes.flatMap((theme) => (s.compact ? (["comfortable", "compact"] as const) : (["comfortable"] as const)).map((density) => ({ s, width, theme, density }))),
    )
  })
  const total = jobs.length

  async function shoot({ s, width, theme, density }: (typeof jobs)[number]) {
    const id = imageId(s.id, width, theme, density)
    const ctx = await browser.newContext({
      viewport: { width: width === "natural" ? 900 : width, height: BASE_HEIGHT },
      deviceScaleFactor: 1,
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
      // The extension's frames arm their buttons by Date.now(), so a frozen clock would keep them disabled.
      if (s.surface !== "extension") await page.clock.setFixedTime(PINNED)
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

      const file = `${id}.png`
      if (width === "natural") {
        const el = page.locator(s.frame ?? "#root > *").first()
        await el.screenshot({ path: path.join(OUT, file), animations: "disabled" })
      } else {
        const extra = await overflow(page)
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
      manifest.push({ id, surface: s.surface, screen: s.screen, state: s.state, width, theme, density, file, brief_row: s.brief_row, notes: notes.join(" ") })
      log[id] = { ready, steps: result, console_errors: errors }
      n += 1
      console.log(`${String(n).padStart(4)}/${total} ${id}${result && !result.ok ? `  (steps: ${result.error})` : ""}${errors.length ? `  (${errors.length} console errors: ${errors[0]?.slice(0, 160)})` : ""}`)
    } catch (e) {
      n += 1
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
  // The manifest in the states' order, whatever order the pages finished in.
  const order = new Map(jobs.map((j, i) => [imageId(j.s.id, j.width, j.theme, j.density), i]))
  manifest.sort((a, b) => (order.get(a.id) ?? 0) - (order.get(b.id) ?? 0))

  await browser.close()
  await site.close()
  writeFileSync(path.join(OUT, "manifest.json"), JSON.stringify(manifest, null, 2) + "\n")
  if (!args["skip-axe"]) writeFileSync(path.join(OUT, "axe.json"), JSON.stringify(axe, null, 2) + "\n")
  writeFileSync(path.join(OUT, "capture-log.json"), JSON.stringify(log, null, 2) + "\n")
  console.log(`${manifest.length} images in ${OUT}`)
}

await main()
