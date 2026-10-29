// The review build's own pages: the start list of every surface, screen
// and state, and the bar over a state (back to the list, light or dark, the
// width, and the state's id). Plain markup and the harness's own small
// sheet (styles.css, `.ld-*`), so it never borrows the apps' components.

import { useEffect, useMemo, useRef, useState } from "react"

import { STATES } from "./registry"
import { prefs, type Frame } from "./prefs"
import type { Density, Surface } from "./types"

const SURFACES: Array<[Surface, string]> = [
  ["desktop", "Desktop app"],
  ["web", "Web vault"],
  ["extension", "Browser extension"],
  ["server", "Server"],
]

const currentTheme = () => prefs.theme() ?? (window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light")

export function StartScreen() {
  const [query, setQuery] = useState("")
  const q = query.trim().toLowerCase()
  const shown = STATES.filter((s) => !q || `${s.screen} ${s.state} ${s.id} ${s.brief_row ?? ""}`.toLowerCase().includes(q))
  useEffect(() => {
    document.documentElement.classList.toggle("dark", currentTheme() === "dark")
  }, [])
  return (
    <main className="ld-start">
      <header>
        <h1>Lockdown design review</h1>
        <p>
          {STATES.length} states across {new Set(STATES.map((s) => `${s.surface}/${s.screen}`)).size} screens, drawn by the apps' own components over one
          invented fixture (Sam Rivera's vaults). Every screen is live: open items, menus and dialogs as you would in the app.
        </p>
        <div className="ld-row">
          <label className="ld-search">
            <span className="ld-sr">Find a screen</span>
            <input type="search" placeholder="Find a screen or state" value={query} onChange={(e) => setQuery(e.target.value)} />
          </label>
          <ThemeButton />
        </div>
      </header>
      {SURFACES.map(([surface, label]) => {
        const states = shown.filter((s) => s.surface === surface)
        if (!states.length) return null
        const screens = [...new Set(states.map((s) => s.screen))]
        return (
          <section key={surface} aria-labelledby={`ld-${surface}`}>
            <h2 id={`ld-${surface}`}>{label}</h2>
            {screens.map((screen) => (
              <div key={screen} className="ld-screen">
                <h3>{screen}</h3>
                <ul>
                  {states
                    .filter((s) => s.screen === screen)
                    .map((s) => (
                      <li key={s.id}>
                        <a href={`#/${s.id}`}>
                          <span>{s.state}</span>
                          <small>{s.id}</small>
                        </a>
                      </li>
                    ))}
                </ul>
              </div>
            ))}
          </section>
        )
      })}
    </main>
  )
}

function ThemeButton() {
  const dark = currentTheme() === "dark"
  return (
    <button
      type="button"
      className="ld-button"
      aria-label={dark ? "Switch to light" : "Switch to dark"}
      onClick={() => {
        prefs.setTheme(dark ? "light" : "dark")
        window.location.reload()
      }}
    >
      {dark ? "Light" : "Dark"}
    </button>
  )
}

const FRAMES: Array<[Frame, string]> = [
  ["phone", "Phone"],
  ["900", "900"],
  ["1280", "1280"],
]

/** A state, with the bar. At a desktop width the state draws in a frame of that width, scaled to fit. */
export function ReviewShell({ id, density }: { id: string; density: Density | null }) {
  const frame = prefs.frame()
  const def = STATES.find((s) => s.id === id)
  return (
    <div className="ld-shell">
      {def ? frame === "phone" ? <div id="ld-stage" className="ld-stage" /> : <Framed id={id} width={Number(frame)} density={density} /> : <p className="ld-missing">There is no state called {id}.</p>}
      <nav className="ld-bar" aria-label="Review">
        <a className="ld-button" href="#/" aria-label="Back to the list of screens">
          ‹ All
        </a>
        <ThemeButton />
        <div className="ld-seg" role="group" aria-label="Width">
          {FRAMES.map(([f, label]) => (
            <button
              key={f}
              type="button"
              aria-pressed={frame === f}
              onClick={() => {
                prefs.setFrame(f)
                window.location.reload()
              }}
            >
              {label}
            </button>
          ))}
        </div>
        <code className="ld-id" title={def ? `${def.screen}: ${def.state}` : id}>
          {id}
        </code>
      </nav>
    </div>
  )
}

/**
 * The state in an iframe as wide as a desktop window, so the apps' own
 * breakpoints see 900 or 1280 px, scaled down to the phone's width. The
 * frame loads this same bundle (its scripts and sheets, by their built
 * addresses) and draws the state named in its `name`.
 */
function Framed({ id, width, density }: { id: string; width: number; density: Density | null }) {
  const box = useRef<HTMLDivElement>(null)
  const [size, setSize] = useState({ w: window.innerWidth, h: window.innerHeight })
  useEffect(() => {
    const el = box.current
    if (!el) return
    const ro = new ResizeObserver(() => setSize({ w: el.clientWidth, h: el.clientHeight }))
    ro.observe(el)
    return () => ro.disconnect()
  }, [])
  const srcDoc = useMemo(() => {
    const scripts = Array.from(document.querySelectorAll<HTMLScriptElement>("script[type=module][src]"))
      .map((s) => `<script type="module" src="${s.src}"></script>`)
      .join("")
    const sheets = Array.from(document.querySelectorAll<HTMLLinkElement>("link[rel=stylesheet]"))
      .map((l) => `<link rel="stylesheet" href="${l.href}">`)
      .join("")
    return `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=${width}">${sheets}</head><body><div id="root"></div>${scripts}</body></html>`
  }, [width])
  const scale = Math.min(1, size.w / width)
  return (
    <div ref={box} className="ld-stage ld-framed">
      <iframe
        title={`${id} at ${width} px`}
        name={`ld-frame:${id}|${currentTheme()}|${density ?? ""}`}
        srcDoc={srcDoc}
        style={{ width, height: size.h / scale, transform: `scale(${scale})` }}
      />
    </div>
  )
}
