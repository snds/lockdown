import { useEffect, useRef } from "react"

import { updateLine } from "@unicron/ui"

import { useUpdateClient } from "./update-client"

/**
 * The outcome of a check someone asked for (the macOS menu's "Check for
 * Updates…", or the button) as a notice, while Settings isn't showing it:
 * the same line About shows (unicron's `updateLine`).
 */
export function useUpdateNotices(show: boolean, onNotice: (text: string) => void) {
  const client = useUpdateClient()
  const showRef = useRef(show)
  useEffect(() => {
    showRef.current = show
  }, [show])
  useEffect(() => {
    if (!client) return
    let live = true
    let off: (() => void) | null = null
    void client
      .onStatus((s) => {
        if (!s.asked || !showRef.current) return
        if (s.state === "up_to_date" || s.state === "failed" || s.state === "ready" || s.state === "available") onNotice(updateLine(s, "Lockdown"))
      })
      .then((f) => {
        if (live) off = f
        else f()
      })
    return () => {
      live = false
      off?.()
    }
  }, [client, onNotice])
}
