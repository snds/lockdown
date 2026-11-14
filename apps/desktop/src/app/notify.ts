import { toast, type ToastAction } from "@unicron/ui"

/*
 * Channel 2 of the five (cross-app decisions §1, brief X2): something
 * finished. A toast over unicron's Toaster: bottom left of the content on a
 * desktop, the status strip's slot on a phone (or under the top row while a
 * needs-you holds the strip), three at most, 6 s with an action and 4 s
 * without (unicron's `toastDurations`). One action at most. No full stop,
 * and it names the thing.
 *
 * Errors, updates and the daily news are never toasts: they are a critical
 * Banner, a mark on the way into Settings, and an info Banner. A control
 * with its own copy (SecretField, CodeDisplay, a Field Row's COPY) says
 * "Copied" itself for 2 s and raises no toast.
 */

/** A toast's words: one line with no full stop at its end (decisions §12). An ellipsis stays. */
export function toastWords(text: string): string {
  const t = text.trim()
  return t.endsWith(".") && !t.endsWith("..") ? t.slice(0, -1) : t
}

export interface NotifyOptions {
  /** The one action (UNDO). */
  action?: ToastAction
  /** Move focus to the action, and back when the toast leaves: after a keyboard action removed the focused row. */
  takeFocus?: boolean
  /** One toast per id: a second with the same id replaces the first. */
  id?: string
}

/** Says that something finished, in a toast. Returns the toast's id. */
export function notify(text: string, options: NotifyOptions = {}): string | number {
  return toast(toastWords(text), { action: options.action, takeFocus: options.takeFocus, id: options.id })
}

// The toasts of brief X2's table 2, by what finished. Each takes the item's name as written.

/** A copy from a swipe, a row's menu or Quick access: "Copied the password for Steam, clears in 30 s". */
export function copiedWords(label: string, title: string, clearsInSecs: number): string {
  return `Copied the ${label} for ${title}, clears in ${clearsInSecs} s`
}

/** A save: "Saved Steam", or "Saved Steam with a new password" when the core made the value. */
export function savedWords(title: string | null, generated: boolean): string {
  const name = title ? `Saved ${title}` : "Saved"
  return generated ? `${name} with a new password` : name
}

/** A move to Recently deleted, with UNDO (`client.restore`). */
export function deletedWords(title: string): string {
  return `Moved ${title} to Recently deleted`
}

/** An archive, with UNDO (`client.unarchive`). */
export function archivedWords(title: string): string {
  return `Archived ${title}`
}

/** A typed password cleared while the window or tab was away (D315): `wipeWords` in `features/hygiene/secret-wipe.ts`. */
export { wipeWords } from "@/features/hygiene/secret-wipe"

/** A gate the person backed out of, where it helps to hear that nothing happened. */
export function cancelledWords(what: "copied" | "shown"): string {
  return what === "copied" ? "Nothing was copied" : "Nothing was shown"
}
