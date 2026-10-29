// The reviewer's choices that outlive a state: light or dark, and how wide
// the review build draws a screen. Kept in this browser only; every read
// and write survives storage being off (a private window).

import type { Theme } from "./types"

export type Frame = "phone" | "900" | "1280"

const get = (k: string): string | null => {
  try {
    return window.localStorage.getItem(k)
  } catch {
    return null
  }
}
const set = (k: string, v: string) => {
  try {
    window.localStorage.setItem(k, v)
  } catch {
    // Storage is off; the choice lasts until the page reloads.
  }
}

let theme: Theme | null = (get("ld-review-theme") as Theme | null) ?? null
let frame: Frame = (get("ld-review-frame") as Frame | null) ?? "phone"

/** `null` follows the system (the capture sets the system's scheme). */
export const prefs = {
  theme: () => theme,
  setTheme(t: Theme) {
    theme = t
    set("ld-review-theme", t)
  },
  frame: () => frame,
  setFrame(f: Frame) {
    frame = f
    set("ld-review-frame", f)
  },
}

/**
 * Makes `prefers-color-scheme` answer `t` for everything on the page: the
 * desktop's next-themes, the web vault's and the popup's own listeners all
 * read it. Called once, before anything mounts.
 */
export function forceScheme(t: Theme) {
  const real = window.matchMedia.bind(window)
  window.matchMedia = (query: string): MediaQueryList => {
    const m = /prefers-color-scheme:\s*(dark|light)/.exec(query)
    if (!m) return real(query)
    const mql = real(query)
    return new Proxy(mql, {
      get(target, prop) {
        if (prop === "matches") return m[1] === t
        const v = Reflect.get(target, prop, target) as unknown
        return typeof v === "function" ? (v as (...a: unknown[]) => unknown).bind(target) : v
      },
    })
  }
  document.documentElement.style.colorScheme = t
}
