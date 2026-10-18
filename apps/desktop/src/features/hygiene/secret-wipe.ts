import { useEffect } from "react"

/** How long the window may stay out of focus before typed secrets are cleared. */
export const BLUR_WIPE_MS = 60_000

/** What counts as a typed secret: every password input, and any field marked as one. */
const SECRET_INPUTS = 'input[type="password"], [data-secret-input] input, [data-secret-input] textarea'

/**
 * Clears every typed secret in `root` (docs/plan/26-desktop-finish.md §7):
 * the value goes through the element's own setter and an `input` event,
 * so a React-controlled field drops its state too, not only what is drawn.
 * Answers how many fields held something. A JavaScript string can't be
 * overwritten; this ends the references the page holds so the engine can
 * collect them, which is what D91 says the webview can do.
 */
export function wipeSecretInputs(root: ParentNode = document): number {
  let wiped = 0
  for (const el of Array.from(root.querySelectorAll<HTMLInputElement | HTMLTextAreaElement>(SECRET_INPUTS))) {
    if (!el.value) continue
    const proto = el instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype
    Object.getOwnPropertyDescriptor(proto, "value")?.set?.call(el, "")
    el.dispatchEvent(new Event("input", { bubbles: true }))
    wiped += 1
  }
  return wiped
}

/**
 * Clears typed secrets when the window is hidden (minimised, another
 * desktop, the tab in the background) and when it has been out of focus
 * for a minute; the lock clears them anyway, since the views unmount.
 * `onWiped` hears how many fields were cleared, for a toast.
 */
export function useSecretWipe(onWiped?: (n: number) => void) {
  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | undefined
    const wipe = () => {
      const n = wipeSecretInputs()
      if (n > 0) onWiped?.(n)
    }
    const onVisibility = () => {
      if (document.visibilityState === "hidden") wipe()
    }
    const onBlur = () => {
      clearTimeout(timer)
      timer = setTimeout(wipe, BLUR_WIPE_MS)
    }
    const onFocus = () => clearTimeout(timer)
    document.addEventListener("visibilitychange", onVisibility)
    window.addEventListener("blur", onBlur)
    window.addEventListener("focus", onFocus)
    return () => {
      clearTimeout(timer)
      document.removeEventListener("visibilitychange", onVisibility)
      window.removeEventListener("blur", onBlur)
      window.removeEventListener("focus", onFocus)
    }
  }, [onWiped])
}
