import { useLayoutEffect, useRef } from "react"
import { createPortal } from "react-dom"

import { Button } from "@unicron/ui"

import "./devtools.css"

/**
 * The bar on every screen while the demo is open (docs/plan/40-devtools.md
 * §3): the sentence and Leave the demo. It sits above the app, and the
 * app's root moves down by the bar's height so nothing hides under it.
 */
export function DemoBar({ pack, leaving, onLeave }: { pack?: string; leaving: boolean; onLeave: () => void }) {
  const bar = useRef<HTMLDivElement>(null)
  useLayoutEffect(() => {
    const root = document.documentElement
    const el = bar.current
    const size = () => root.style.setProperty("--devtools-bar", `${el?.offsetHeight ?? 0}px`)
    root.setAttribute("data-lockdown-demo", "")
    size()
    const watch = el ? new ResizeObserver(size) : null
    if (el) watch?.observe(el)
    return () => {
      watch?.disconnect()
      root.removeAttribute("data-lockdown-demo")
      root.style.removeProperty("--devtools-bar")
    }
  }, [])
  return createPortal(
    <div ref={bar} role="region" aria-label="Demo" data-devtools-bar="" className="flex flex-wrap items-center justify-center gap-x-4 gap-y-1 bg-primary px-4 py-2 text-sm text-primary-foreground">
      <p>
        <strong>You're exploring the demo.</strong> Nothing here is real.
        {pack && <span className="sr-only"> Its data is from {pack}.</span>}
      </p>
      <Button variant="secondary" size="sm" onClick={onLeave} disabled={leaving}>
        {leaving ? "Leaving…" : "Leave the demo"}
      </Button>
    </div>,
    document.body,
  )
}
