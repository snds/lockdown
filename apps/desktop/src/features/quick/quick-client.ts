import { createContext, useContext } from "react"

import type { GateInput, ItemSummary } from "@/core/types"

/** One quick-access result: the item's masked summary, and whether every word matched exactly. */
export type QuickHit = ItemSummary & { exact: boolean }

/** "Fill in the browser": the address the shell opened and how long the extension has. */
export interface FillStarted {
  url: string
  host: string
  waits_secs: number
}

/** The system-wide shortcut's state, for Settings. */
export interface HotkeyStatus {
  on: boolean
  registered: boolean
  label: string
  /** Why it isn't working while wanted (a Wayland session, a shortcut taken). */
  problem?: string
}

/**
 * Quick access (docs/plan/26-desktop-finish.md §1–§2): the core's one
 * search index, the code shown in place, "Fill in the browser", and the
 * system-wide shortcut. The desktop and the web vault each provide one;
 * feature code never reaches a shell directly.
 */
export interface QuickClient {
  /** Ranked and typo-tolerant, over masked summaries only; live items. */
  search(query: string, limit?: number): Promise<QuickHit[]>
  /** The code valid now as bytes (a crown jewel's through the `Use` gate); counts as a use. */
  quickCode(itemId: string, fieldId: string, gate?: GateInput): Promise<Uint8Array>
  /** Opens the login's site; the extension's menu there offers it first for two minutes. */
  quickFill(itemId: string): Promise<FillStarted>
  /** Whether this shell can fill in a browser tab ("Fill in the browser" is shown only then). */
  canFill: boolean
  /** The quick-access window, where there is one: hide it, or open an item in the main window. */
  hide(): Promise<void>
  openMain(itemId: string | null): Promise<void>
  hotkeyStatus(): Promise<HotkeyStatus | null>
  setHotkey(on: boolean): Promise<HotkeyStatus | null>
}

const refuse = (what: string) => Promise.reject({ code: "refused", message: `${what} isn't available here` })

/** Until a shell provides one: every call refuses in words. */
export const noQuickClient: QuickClient = {
  search: () => refuse("Quick access"),
  quickCode: () => refuse("Showing a code"),
  quickFill: () => refuse("Filling in the browser"),
  canFill: false,
  hide: () => Promise.resolve(),
  openMain: () => Promise.resolve(),
  hotkeyStatus: () => Promise.resolve(null),
  setHotkey: () => Promise.resolve(null),
}

const QuickClientContext = createContext<QuickClient>(noQuickClient)

export const QuickClientProvider = QuickClientContext.Provider

export function useQuickClient(): QuickClient {
  return useContext(QuickClientContext)
}
