import type { ItemSummary } from "@/core/types"
import { kindLabel } from "@/features/vault/list-model"

/** Whether this computer names the command key ⌘ (macOS) or Ctrl. */
export const isMac = () => typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent)

/** A URL's host, for search; the text as it is when it isn't a URL. */
export function hostOf(site: string): string {
  try {
    return new URL(site).host
  } catch {
    return site
  }
}

/**
 * The words a row is found by: title, username, the website's host and the
 * kind, from the masked summary only (D111). A crown jewel's summary
 * already carries no field (02-model §3), so it is found by title and kind.
 */
export function searchWords(row: ItemSummary, kindName: string): string[] {
  return [row.title, row.username, hostOf(row.site), row.subtitle, kindName, kindLabel(row.kind)].filter(Boolean)
}

/** Every word of the search appears in the row's words (the vault list's rule: plain substrings, any order). */
export function matches(search: string, words: string[]): boolean {
  const hay = words.join(" ").toLowerCase()
  return search
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean)
    .every((w) => hay.includes(w))
}
