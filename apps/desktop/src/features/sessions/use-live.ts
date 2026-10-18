import { useEffect, useRef } from "react"

import { useLiveClient, type LiveNotice } from "./live-client"

/**
 * Subscribes to the live channel's notices while mounted. The handler may
 * change between renders; the subscription doesn't. Outside the desktop
 * shell (tests, a plain browser) there is no channel and nothing happens.
 */
export function useLive(handler: (notice: LiveNotice) => void) {
  const live = useLiveClient()
  const latest = useRef(handler)
  useEffect(() => {
    latest.current = handler
  })
  useEffect(() => {
    let stop: (() => void) | null = null
    let gone = false
    try {
      live
        .onLive((n) => latest.current(n))
        .then((unlisten) => {
          if (gone) unlisten()
          else stop = unlisten
        })
        .catch(() => {})
    } catch {
      // No shell here.
    }
    return () => {
      gone = true
      stop?.()
    }
  }, [live])
}
