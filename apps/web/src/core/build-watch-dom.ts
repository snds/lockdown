import { useSyncExternalStore } from "react"

import { QUIET_EVERY_MS, RELOADED_KEY, createBuildWatch, type BuildWatch, type BuildWatchState } from "./build-watch"

/**
 * The build watch wired to the page (docs/plan/66-shared-feed.md §4).
 * `startBuildWatch()` runs once from main.tsx, for the vault only: a share
 * link, a request link and a security-key page hold a key from the address
 * in memory, so they never reload by themselves and are not watched.
 *
 * It does nothing in a page with no build id (`vite dev`, the tests).
 * Elsewhere it asks `build.json` at load, when the tab becomes visible,
 * when the device comes back online, every 15 minutes while visible, and
 * when a lazy file fails to load. The request goes to the page's own
 * origin (the policy's `connect-src 'self'`), with no cookie and nothing
 * about the person.
 *
 * What a screen uses: `useBuildWatch()` for the state, `reloadForNewBuild()`
 * for the Reload action, and `holdReload()` around work a reload would
 * lose.
 */
let watch: BuildWatch | null = null

/** The name of the page's tag (build-stamp.ts, `BUILD_META`). */
const BUILD_META = "lockdown-build"

/** This page's build id (build-stamp.ts writes the tag), or null. */
export function pageBuild(doc: Document = document): string | null {
  const id = doc.querySelector(`meta[name="${BUILD_META}"]`)?.getAttribute("content")?.trim()
  return id ? id : null
}

/** `sessionStorage`, which a browser may refuse (private windows, blocked site data). */
function session(): Storage | null {
  try {
    return window.sessionStorage
  } catch {
    return null
  }
}

const holds = new Map<symbol, string>()

/**
 * Says a reload would lose something until the returned function is
 * called: a sign-in waiting for its second step, a file being read, a
 * change on its way. The page then only offers Reload.
 */
export function holdReload(why: string): () => void {
  const key = Symbol(why)
  holds.set(key, why)
  return () => {
    holds.delete(key)
  }
}

/** Why a reload would lose something now, in the holds' own words. */
export function reloadHolds(): string[] {
  return [...holds.values()]
}

const TYPED = new Set(["button", "checkbox", "color", "file", "hidden", "image", "radio", "range", "reset", "submit"])

/** The fields a person types in. */
function typedFields(doc: Document): (HTMLInputElement | HTMLTextAreaElement)[] {
  return [...doc.querySelectorAll<HTMLInputElement | HTMLTextAreaElement>("input, textarea")].filter(
    (el) => el instanceof HTMLTextAreaElement || !TYPED.has(el.type),
  )
}

/**
 * The page's own say on a reload by itself. Yes only when it shows an
 * empty form and nothing else: at least one field to type in, every one
 * empty; no editor with writing; nothing open over the page; nothing busy;
 * no hold. That is the sign-in screen at rest. A screen without a field (a
 * sign-in waiting for a security key or the emailed link, a spinner, an
 * error) is never one, so it is left alone; so is a form a browser filled
 * in by itself.
 *
 * It reads only whether a field is empty, never what is in it.
 */
export function pageAtRest(doc: Document = document): boolean {
  if (holds.size > 0) return false
  const fields = typedFields(doc)
  if (fields.length === 0) return false
  // Empty, not "unchanged": a field React controls reads as unchanged whatever is typed in it.
  if (fields.some((el) => el.value !== "")) return false
  for (const el of doc.querySelectorAll<HTMLElement>('[contenteditable=""], [contenteditable="true"], [contenteditable="plaintext-only"]')) {
    if ((el.textContent ?? "") !== "") return false
  }
  if (doc.querySelector('[role="dialog"], [role="alertdialog"], [aria-modal="true"], dialog[open]')) return false
  if (doc.querySelector('[aria-busy="true"], [role="progressbar"]')) return false
  return true
}

/** What the watch asks of the core: `WebSession.state()`, and only whether someone is signed in. */
export interface SignedInSource {
  state(): Promise<{ signed_in: boolean }>
}

/** Starts the watch. Safe to call twice. Returns whether it runs. */
export function startBuildWatch(source: SignedInSource): boolean {
  if (watch) return true
  if (typeof window === "undefined" || typeof document === "undefined") return false
  const current = pageBuild()
  if (!current) return false

  const url = new URL(`${import.meta.env.BASE_URL}build.json`, window.location.origin).toString()
  const made = createBuildWatch({
    current,
    fetchText: async () => {
      // `no-store`: past the browser's cache. No cookie, no referrer: the request says nothing but its address.
      const res = await fetch(url, { cache: "no-store", credentials: "omit", redirect: "error", referrerPolicy: "no-referrer", headers: { accept: "application/json" } })
      return res.ok ? await res.text() : null
    },
    now: () => Date.now(),
    hidden: () => document.visibilityState === "hidden",
    signedIn: async () => (await source.state()).signed_in !== false,
    canReload: () => pageAtRest(),
    reload: () => window.location.reload(),
    reloadedFor: () => {
      try {
        return session()?.getItem(RELOADED_KEY) ?? null
      } catch {
        return null
      }
    },
    noteReloaded: (id) => {
      try {
        session()?.setItem(RELOADED_KEY, id)
      } catch {
        // The rules read the note back before reloading: with no note kept, the page only offers Reload.
      }
    },
  })
  watch = made

  const activity = () => made.activity()
  for (const name of ["keydown", "pointerdown", "touchstart", "wheel"]) window.addEventListener(name, activity, { capture: true, passive: true })
  document.addEventListener("visibilitychange", () => made.visibility())
  window.addEventListener("online", () => made.online())
  // A lazy file of the old build is gone. The failed import still throws to the screen that asked;
  // the watch reloads if that is safe, and otherwise says "Reload to continue" (`needed`).
  window.addEventListener("vite:preloadError", () => void made.preloadFailed())
  window.setInterval(() => void made.tick(), QUIET_EVERY_MS)
  void made.check()
  return true
}

const NONE: BuildWatchState | null = null

/** The watch's state for a screen, or null where it does not run. */
export function useBuildWatch(): BuildWatchState | null {
  return useSyncExternalStore(
    (listener) => watch?.subscribe(listener) ?? (() => {}),
    () => watch?.state() ?? NONE,
    () => NONE,
  )
}

/** The Reload action: reloads at once. The person chose it, so nothing is asked first. */
export function reloadForNewBuild(): void {
  if (watch) watch.reloadNow()
  else window.location.reload()
}

/** Asks the server now. */
export function checkBuildNow(): void {
  void watch?.check(true)
}

/** For the tests: forgets the running watch and every hold. */
export function resetBuildWatchForTests(): void {
  watch = null
  holds.clear()
}
