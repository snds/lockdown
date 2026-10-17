import { useCallback, useEffect, useRef, useState } from "react"

import { describeError, useClient } from "@/core/client"
import type { CodeView } from "@/core/types"

export interface CodesState {
  codes: CodeView[] | null
  error: string | null
  /** Seconds left per code (`item_id/field_id`), ticked here once a second. */
  secondsLeft: Record<string, number | null>
  reload: () => void
}

export const codeKey = (c: Pick<CodeView, "item_id" | "field_id">) => `${c.item_id}/${c.field_id}`

/**
 * The codes as the core computes them, with a local countdown: the core
 * is asked once, the seconds tick here, and when any code runs out the
 * core is asked again. `enabled` false stops the clock (the screen is not
 * showing codes), so no code is computed for nothing.
 */
export function useCodes(enabled: boolean, filter?: (code: CodeView) => boolean): CodesState {
  const client = useClient()
  const [codes, setCodes] = useState<CodeView[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [secondsLeft, setSecondsLeft] = useState<Record<string, number | null>>({})
  const filterRef = useRef(filter)
  useEffect(() => {
    filterRef.current = filter
  })

  const reload = useCallback(() => {
    client
      .codes()
      .then((all) => {
        const shown = filterRef.current ? all.filter(filterRef.current) : all
        setCodes(shown)
        setError(null)
        setSecondsLeft(Object.fromEntries(shown.map((c) => [codeKey(c), c.seconds_left])))
      })
      .catch((e) => setError(describeError(e)))
  }, [client])

  useEffect(() => {
    if (enabled) reload()
  }, [enabled, reload])

  // The clock: one tick a second over a ref, so the interval never reads
  // stale state and the reload runs outside the state updater.
  const leftRef = useRef(secondsLeft)
  useEffect(() => {
    leftRef.current = secondsLeft
  }, [secondsLeft])

  useEffect(() => {
    if (!enabled || !codes || codes.length === 0) return
    const timer = window.setInterval(() => {
      let expired = false
      const next: Record<string, number | null> = {}
      for (const [k, v] of Object.entries(leftRef.current)) {
        if (v === null) next[k] = null
        else if (v <= 1) {
          next[k] = 0
          expired = true
        } else next[k] = v - 1
      }
      setSecondsLeft(next)
      if (expired) reload()
    }, 1000)
    return () => window.clearInterval(timer)
  }, [enabled, codes, reload])

  return { codes: enabled ? codes : null, error, secondsLeft, reload }
}
