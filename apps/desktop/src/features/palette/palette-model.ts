import type { IconName } from "@unicron/ui"

import { deviceName, placesFor, vaultPlace, type Surface } from "@/app/places"
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

/** How long a code shown in quick access stays on screen. */
export const SHOWN_CODE_MS = 30_000

/** The keys for the two actions beyond Enter and the copy: shown and spoken. */
export function actionKeys() {
  return { fill: isMac() ? "⇧↵" : "Shift+Enter", code: isMac() ? "⌥↵" : "Alt+Enter" }
}

/** One row of GO TO: a place by its id (app/places.ts), or the Lock command. */
export interface GoToRow {
  /** The place id `go` takes, or `lock`. */
  id: string
  label: string
  icon: IconName
  /** Where it sits, in words ("Settings", "Sharing"), shown after the label and found by. */
  within?: string
  /** The key that goes there too, drawn in a Kbd ("⌘,"). */
  key?: string
}

/** The id of the Lock command among GO TO's rows. */
export const LOCK_ROW = "lock"

/**
 * GO TO (brief X4): the places of this build whose name matches, then
 * Lock. All items and each view, each vault, Codes, Health and its lists,
 * Sharing and its tabs, Settings and each section; the places another
 * build draws are left out (the web vault lists its own). Search itself is
 * the last row ("Search everything for …"), not a place here. GO TO is
 * found by name, from two letters: one letter names nearly every place,
 * and would bury the items under them.
 */
export const GO_TO_FROM = 2

export function goToRows(query: string, surface: Surface, vaults: ReadonlyArray<{ id: string; name: string }>, mac: boolean): GoToRow[] {
  if (query.trim().length < GO_TO_FROM) return []
  const rows: GoToRow[] = []
  for (const p of placesFor(surface)) {
    if (p.id === "search") continue
    const label = p.id === "settings-this-device" ? deviceName(surface, mac) : p.label
    const within = p.group === "settings" && p.id !== "settings" ? "Settings" : p.group === "sharing" && p.id !== "sharing" ? "Sharing" : p.group === "health" ? "Health" : undefined
    rows.push({ id: p.id, label, icon: p.icon, within, key: p.id === "settings" ? (mac ? "⌘," : "Ctrl+,") : undefined })
    // Each vault after the views, as the sidebar lists them.
    if (p.id === "deleted") for (const v of vaults) rows.push({ id: vaultPlace(v.id), label: v.name, icon: "folder", within: "Vaults" })
  }
  rows.push({ id: LOCK_ROW, label: "Lock", icon: "locked" })
  return rows.filter((r) => matches(query, [r.label, r.within ?? ""]))
}
