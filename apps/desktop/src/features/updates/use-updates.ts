import { useEffect, useState } from "react"

import { useUpdateClient, type UpdateStatus } from "./update-client"

/** The updater's status, kept current from its event; `null` where the host has no updater. */
export function useUpdates(): UpdateStatus | null {
  const client = useUpdateClient()
  const [status, setStatus] = useState<UpdateStatus | null>(null)
  useEffect(() => {
    if (!client) return
    let live = true
    let off: (() => void) | null = null
    // An event that arrives first is newer than the first read's answer.
    let heard = false
    client.status().then(
      (s) => live && !heard && setStatus(s),
      () => live && !heard && setStatus(null),
    )
    void client
      .onStatus((s) => {
        heard = true
        if (live) setStatus(s)
      })
      .then((f) => {
        if (live) off = f
        else f()
      })
    return () => {
      live = false
      off?.()
    }
  }, [client])
  return status
}
