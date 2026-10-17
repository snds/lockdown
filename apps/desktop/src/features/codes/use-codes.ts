import { useCallback, useEffect, useRef, useState } from "react"

import { describeError, useClient } from "@/core/client"
import type { CodeView } from "@/core/types"

export interface CodesState {
  codes: CodeView[] | null
  error: string | null
  /** The host clock in Unix seconds, ticked here once a second, as CodeDisplay's `now`. */
  now: number
  /** When each code (`item_id/field_id`) dies, in Unix seconds; `null` for a counter-based code. */
  expiresAt: Record<string, number | null>
  reload: () => void
}

export const codeKey = (c: Pick<CodeView, "item_id" | "field_id">) => `${c.item_id}/${c.field_id}`

const unixNow = () => Math.floor(Date.now() / 1000)

/**
 * The codes as the core computes them, with a local clock: the core is
 * asked once, each code's end is pinned to this clock from the seconds it
 * reported, the clock ticks here, and when any code runs out the core is
 * asked again. `enabled` false stops the clock (the screen is not showing
 * codes), so no code is computed for nothing.
 */
export function useCodes(enabled: boolean, filter?: (code: CodeView) => boolean): CodesState {
  const client = useClient()
  const [codes, setCodes] = useState<CodeView[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [now, setNow] = useState(unixNow)
  const [expiresAt, setExpiresAt] = useState<Record<string, number | null>>({})
  const filterRef = useRef(filter)
  useEffect(() => {
    filterRef.current = filter
  })

  const reload = useCallback(() => {
    client
      .codes()
      .then((all) => {
        const shown = filterRef.current ? all.filter(filterRef.current) : all
        const at = unixNow()
        setCodes(shown)
        setError(null)
        setExpiresAt(Object.fromEntries(shown.map((c) => [codeKey(c), c.seconds_left === null ? null : at + c.seconds_left])))
        setNow(at)
      })
      .catch((e) => setError(describeError(e)))
  }, [client])

  useEffect(() => {
    if (enabled) reload()
  }, [enabled, reload])

  // The clock: one tick a second over a ref, so the interval never reads
  // stale state and the reload runs outside the state updater.
  const endsRef = useRef(expiresAt)
  useEffect(() => {
    endsRef.current = expiresAt
  }, [expiresAt])

  useEffect(() => {
    if (!enabled || !codes || codes.length === 0) return
    const timer = window.setInterval(() => {
      const at = unixNow()
      setNow(at)
      if (Object.values(endsRef.current).some((end) => end !== null && end <= at)) reload()
    }, 1000)
    return () => window.clearInterval(timer)
  }, [enabled, codes, reload])

  return { codes: enabled ? codes : null, error, now, expiresAt, reload }
}
