import { useLayoutEffect, useRef } from "react"
import { createPortal } from "react-dom"

import { Banner, Button } from "@unicron/ui"

import "./devtools.css"

/**
 * The demo's notice on every screen while it is open (docs/plan/40-devtools.md
 * §3, brief T10): a compact info Banner, "You're exploring the demo.
 * Nothing here is real.", with LEAVE THE DEMO and no close button. It sits
 * above the app, pushed screens too, and the app's root moves down by its
 * height so nothing hides under it.
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
    // A plain box on the ground holds the Banner at the top of the window, so no screen hides under it.
    <div ref={bar} role="region" aria-label="Demo" data-devtools-bar="" className="bg-surface-ground px-4 pt-2 pb-2">
      <Banner
        tone="info"
        size="compact"
        title="You're exploring the demo."
        body={
          <>
            Nothing here is real.
            {pack && <span className="sr-only"> Its data is from {pack}.</span>}
          </>
        }
        action={
          <Button variant="secondary" size="sm" onClick={onLeave} loading={leaving}>
            Leave the demo
          </Button>
        }
      />
    </div>,
    document.body,
  )
}
