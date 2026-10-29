import { useSyncExternalStore } from "react"

/**
 * The first run's way into a demo, when a build has one
 * (docs/plan/39-first-run.md §1.13, docs/plan/40-devtools.md §3): the
 * first-run views never import the demo; a demo, when it is compiled in,
 * installs this entry, and the first run shows "Explore the demo" while
 * one is installed. Nothing installs it in a release.
 */
export interface DemoEntry {
  /** Opens the demo. */
  explore: () => void
  /** True while it opens. */
  busy: boolean
}

let current: DemoEntry | null = null
const listeners = new Set<() => void>()

/** Installs (or, with `null`, removes) the way into the demo. */
export function setDemoEntry(entry: DemoEntry | null) {
  current = entry
  for (const l of listeners) l()
}

function subscribe(l: () => void) {
  listeners.add(l)
  return () => {
    listeners.delete(l)
  }
}

/** The way into the demo, or `null` when this build has none. */
export function useDemoEntry(): DemoEntry | null {
  return useSyncExternalStore(
    subscribe,
    () => current,
    () => null,
  )
}
