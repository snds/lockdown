import { useEffect } from "react"

import { useClient } from "@/core/client"
import type { LockState } from "@/core/types"

/** How often the UI asks the core whether a timer has locked the vault. */
const POLL_MS = 3000
/** How often activity is reported, at most. */
const TOUCH_MS = 10_000

/**
 * The shell never decides a lock; it reports what it sees. Visibility
 * changes go to the core's hidden-window timer, keyboard and pointer
 * activity to its idle timer (throttled), and the lock state is polled so
 * a lock the core decided on its own reaches the screen within seconds.
 */
export function useLockWatch(onState: (state: LockState) => void) {
  const client = useClient()
  useEffect(() => {
    let last = 0
    const touch = () => {
      const now = Date.now()
      if (now - last < TOUCH_MS) return
      last = now
      void client.touch().catch(() => {})
    }
    const visibility = () => {
      void client
        .setHidden(document.visibilityState === "hidden")
        .then(onState)
        .catch(() => {})
    }
    const poll = () => void client.lockState().then(onState).catch(() => {})
    const timer = window.setInterval(poll, POLL_MS)
    document.addEventListener("visibilitychange", visibility)
    window.addEventListener("keydown", touch, { passive: true })
    window.addEventListener("pointerdown", touch, { passive: true })
    return () => {
      window.clearInterval(timer)
      document.removeEventListener("visibilitychange", visibility)
      window.removeEventListener("keydown", touch)
      window.removeEventListener("pointerdown", touch)
    }
  }, [client, onState])
}
