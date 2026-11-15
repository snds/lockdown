import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from "react"

interface CrumbValue {
  extra: readonly string[]
  setExtra: (segments: readonly string[]) => void
}

const CrumbContext = createContext<CrumbValue | null>(null)

/** Holds what the open screen adds to the breadcrumb. The shell mounts one around the content. */
export function CrumbProvider({ children }: { children: ReactNode }) {
  const [extra, setExtra] = useState<readonly string[]>([])
  const value = useMemo(() => ({ extra, setExtra }), [extra])
  return <CrumbContext.Provider value={value}>{children}</CrumbContext.Provider>
}

/** The parts the open screen added, for the status line. */
export function useCrumbExtra(): readonly string[] {
  return useContext(CrumbContext)?.extra ?? []
}

/**
 * A screen names its deeper place (brief 3.2): an item opened in Health
 * (`useCrumb(["Ticketmaster"])` reads `TOOLS // HEALTH // TICKETMASTER`), a
 * finding, a person. The parts follow the place's own breadcrumb and go when
 * the screen does. Pass `null` or nothing open to add none.
 */
export function useCrumb(segments: readonly string[] | null | undefined): void {
  const ctx = useContext(CrumbContext)
  const setExtra = ctx?.setExtra
  // The words, not the array: a screen may build a new array on every draw.
  const key = segments?.join("\u0000") ?? ""
  useEffect(() => {
    if (!setExtra) return
    setExtra(key ? key.split("\u0000") : [])
    return () => setExtra([])
  }, [setExtra, key])
}
