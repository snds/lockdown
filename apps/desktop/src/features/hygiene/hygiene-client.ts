import { createContext, useContext, useEffect } from "react"

/** What the core keeps out of swap (lockdown_crypto::locked::MemoryReport). */
export interface MemoryReport {
  supported: boolean
  pages_locked: number
  page_size: number
  pages_locked_ever: number
  refused: number
  core_dumps_off: boolean
}

/**
 * Screen and memory hygiene (docs/plan/26-desktop-finish.md §6–§7): a
 * reveal holds the window out of screen captures where the OS allows it;
 * Settings reads what the core keeps out of swap.
 */
export interface HygieneClient {
  /** One reveal drawn (`true`) or gone; resolves whether captures are blocked now. */
  protect(on: boolean): Promise<boolean>
  memory(): Promise<MemoryReport | null>
}

export const noHygieneClient: HygieneClient = {
  protect: () => Promise.resolve(false),
  memory: () => Promise.resolve(null),
}

const HygieneContext = createContext<HygieneClient>(noHygieneClient)

export const HygieneClientProvider = HygieneContext.Provider

export function useHygieneClient(): HygieneClient {
  return useContext(HygieneContext)
}

/** Holds the window out of screen captures while `open` (a reveal, large type, a guest card). */
export function useCaptureProtect(open: boolean) {
  const hygiene = useHygieneClient()
  useEffect(() => {
    if (!open) return
    void hygiene.protect(true).catch(() => {})
    return () => {
      void hygiene.protect(false).catch(() => {})
    }
  }, [open, hygiene])
}

/** How long a reveal's protection lasts at most, beyond the field's own lifetime. */
export const REVEAL_PROTECT_MS = 35_000

/** Protection for one reveal the component can't see the end of: released after `ms`. */
export function protectFor(hygiene: HygieneClient, ms = REVEAL_PROTECT_MS) {
  void hygiene.protect(true).catch(() => {})
  setTimeout(() => void hygiene.protect(false).catch(() => {}), ms)
}
