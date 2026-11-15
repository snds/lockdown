/**
 * The ground the web vault is drawn on: stone (light) or graphite (dark),
 * as a `dark` class on the root. The app's own colours come from
 * `data-theme="lockdown"`, which index.html sets and nothing changes.
 *
 * The choice is the person's (Settings → Appearance → Theme): System
 * follows the browser, Stone and Graphite hold. It is kept in this
 * browser's local storage, on this device only, and never in the vault's
 * settings: it says nothing about the vault, and a locked or signed-out
 * page must already be drawn on the right ground
 * (docs/plan/62-unicron-1-0.md §3, proposed D700). System stores nothing.
 *
 * The desktop uses next-themes, whose provider writes an inline script; the
 * web vault's Content Security Policy runs none, so this module does the
 * same job from the page's own bundle: main.tsx calls `followColorScheme()`
 * before the first render.
 */
export type ThemeChoice = "system" | "stone" | "graphite"

/** Where the choice is kept. Absent means System. */
export const THEME_KEY = "lockdown.theme"

const listeners = new Set<() => void>()

function stored(): ThemeChoice {
  try {
    const value = window.localStorage.getItem(THEME_KEY)
    return value === "stone" || value === "graphite" ? value : "system"
  } catch {
    // Private mode or blocked storage: follow the browser.
    return "system"
  }
}

let choice: ThemeChoice = stored()
let following = false

const prefersDark = () => window.matchMedia("(prefers-color-scheme: dark)")

function apply() {
  const dark = choice === "graphite" || (choice === "system" && prefersDark().matches)
  document.documentElement.classList.toggle("dark", dark)
  document.documentElement.style.colorScheme = dark ? "dark" : "light"
}

/** The person's choice, as Settings shows it. */
export function themeChoice(): ThemeChoice {
  return choice
}

/** Takes a new choice: the whole page changes at once, and the choice outlives the tab. */
export function setThemeChoice(next: ThemeChoice) {
  choice = next
  try {
    if (next === "system") window.localStorage.removeItem(THEME_KEY)
    else window.localStorage.setItem(THEME_KEY, next)
  } catch {
    // Storage is off: the choice holds until the page reloads.
  }
  apply()
  for (const listener of listeners) listener()
}

/** Tells `listener` when the choice changes, here or in another tab. For `useSyncExternalStore`. */
export function subscribeTheme(listener: () => void): () => void {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

/**
 * Draws the page on the chosen ground and keeps it there: with the browser
 * while the choice is System, and with a change made in another tab.
 */
export function followColorScheme() {
  choice = stored()
  apply()
  if (following) return
  following = true
  prefersDark().addEventListener("change", apply)
  window.addEventListener("storage", (event) => {
    if (event.key !== null && event.key !== THEME_KEY) return
    choice = stored()
    apply()
    for (const listener of listeners) listener()
  })
}
