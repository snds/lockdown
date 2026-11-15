import type { Nav } from "@/app/places"

/**
 * What Search starts from (brief X4): the words typed elsewhere, and the
 * place it was opened from. Both live here, outside React, because the
 * screen that hands them over (All items' field, a vault's list, Quick
 * access's last row) is gone by the time Search draws. Nothing secret is
 * kept: words someone typed into a search field, and a place.
 */

/** Words typed into All items' search field or Quick access, waiting for Search to open with them. */
let typed = ""
const listeners = new Set<(words: string) => void>()

/**
 * Opens Search with what was typed elsewhere: Search starts with these
 * words in its field. A Search already open takes them at once (Quick
 * access's "Search everything for …" while Search shows).
 */
export function searchWith(words: string): void {
  typed = words
  for (const listen of listeners) listen(words)
}

/** The words waiting for Search. */
export function typedWords(): string {
  return typed
}

/** Taken: the next Search starts empty. */
export function clearTyped(): void {
  typed = ""
}

/** An open Search hears words handed to it later. Returns the way to stop. */
export function onSearchWith(listen: (words: string) => void): () => void {
  listeners.add(listen)
  return () => {
    listeners.delete(listen)
  }
}

/** The list on screen before Search opened: what its second chip is, and where Escape goes back to. */
let from: Nav | null = null

/**
 * The vault list says where it is while it shows. It clears this when it
 * goes, after Search has read it: React draws the new screen before it
 * runs the old one's clean-up, so Search opened from a list still finds
 * that list, and Search opened from Codes or Settings finds nothing.
 */
export function rememberWhere(nav: Nav | null): void {
  from = nav
}

/** The list Search was opened from, if Search was opened from one. */
export function openedFrom(): Nav | null {
  return from
}
