import { useEffect, useRef } from "react"

import { useUpdateClient } from "./update-client"

/**
 * A check someone asked for (the macOS menu's "Check for Updates…") while
 * Settings → About isn't showing: About opens, where the update rows say
 * how it went. An update is never a toast or a status-line button
 * (cross-app decisions §3, D755); the automatic check opens nothing.
 * `onAsked` runs once per asked check, as it starts.
 */
export function useUpdateNotices(show: boolean, onAsked: () => void) {
  const client = useUpdateClient()
  const showRef = useRef(show)
  useEffect(() => {
    showRef.current = show
  }, [show])
  useEffect(() => {
    if (!client) return
    let live = true
    let off: (() => void) | null = null
    // An asked check starts as `checking` with `asked`: open About there, once per check.
    let before: string | null = null
    void client
      .onStatus((s) => {
        const starts = s.asked && s.state === "checking" && before !== "checking"
        before = s.state
        if (starts && showRef.current) onAsked()
      })
      .then((f) => {
        if (live) off = f
        else f()
      })
    return () => {
      live = false
      off?.()
    }
  }, [client, onAsked])
}
