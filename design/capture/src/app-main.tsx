// The harness's page, once main.tsx has ruled out an extension frame. Three ways in:
//
// - the capture (scripts/capture.ts) sets `window.__LD_CAPTURE__`: the state
//   named by the address is drawn alone, and the page says when it is ready;
// - a framed desktop width in the review build: an iframe named
//   `ld-frame:<id>` draws the state alone at 900 or 1280 px;
// - otherwise the review build: the start list, or a state with the bar.
//
// Every state starts from a fresh fixture, so moving between states reloads.

import "./styles.css"

import { StrictMode, type ReactNode } from "react"
import { createRoot } from "react-dom/client"

import { commands } from "./fixture/commands"
import { freshWorld } from "./fixture/world"
import { install, missingCommands, override } from "./host/core"
import { runSteps } from "./harness/driver"
import { forceScheme, prefs } from "./harness/prefs"
import { ReviewShell, StartScreen } from "./harness/review"
import { parseAddress, stateById, statesMeta } from "./harness/registry"
import type { Density, Theme } from "./harness/types"

declare global {
  interface Window {
    __LD_CAPTURE__?: boolean
    __LD_STATES__?: ReturnType<typeof statesMeta>
    __LD_RESULT__?: { ok: boolean; error: string | null; missing: string[] }
  }
}

window.__LD_STATES__ = statesMeta()

const root = document.getElementById("root")!

function draw(node: ReactNode) {
  createRoot(root).render(<StrictMode>{node}</StrictMode>)
}

/** Draws one state over a fresh fixture, runs its steps, and says how it went. */
async function boot(id: string, stage: HTMLElement | null, density: Density | null) {
  const def = stateById(id)
  if (!def) throw new Error(`no state called ${id}`)
  const world = freshWorld()
  def.setup?.(world)
  if (density) world.settings = { ...world.settings, density }
  install(commands(world))
  if (def.answers) override(def.answers(world))
  const node = def.mount()
  if (stage) createRoot(stage).render(<StrictMode>{node}</StrictMode>)
  else draw(node)
  try {
    await runSteps(def.steps ?? [])
    await document.fonts.ready
    // Animations (dialogs, menus, toasts) finish before the picture.
    await new Promise((r) => setTimeout(r, 450))
    window.__LD_RESULT__ = { ok: true, error: null, missing: missingCommands() }
  } catch (e) {
    window.__LD_RESULT__ = { ok: false, error: e instanceof Error ? e.message : String(e), missing: missingCommands() }
    console.error(`[harness] ${id}: ${window.__LD_RESULT__.error}`)
  }
  document.documentElement.dataset.captureReady = "1"
}

const framed = window.name.startsWith("ld-frame:")
const address = parseAddress(window.location.hash)

if (window.__LD_CAPTURE__ || framed) {
  const id = framed ? window.name.slice("ld-frame:".length).split("|")[0]! : address.id
  const [, frameTheme, frameDensity] = window.name.split("|")
  const theme = framed ? ((frameTheme as Theme | undefined) ?? null) : address.theme
  if (theme) forceScheme(theme)
  document.documentElement.dataset.capture = "bare"
  void boot(id ?? "", null, framed ? ((frameDensity as Density | undefined) ?? null) : address.density)
} else {
  const theme = address.theme ?? prefs.theme()
  if (theme) forceScheme(theme)
  if (address.frame) prefs.setFrame(address.frame)
  if (!address.id) draw(<StartScreen />)
  else {
    draw(<ReviewShell id={address.id} density={address.density} />)
    if (prefs.frame() === "phone") {
      const wait = () => {
        const stage = document.getElementById("ld-stage")
        if (stage) void boot(address.id!, stage, address.density)
        else requestAnimationFrame(wait)
      }
      wait()
    }
  }
  // A new address is a new state: start it from a fresh fixture.
  window.addEventListener("hashchange", () => window.location.reload())
}
