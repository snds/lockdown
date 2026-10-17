import { useEffect } from "react"
import { toast } from "sonner"

import { useClient } from "@/core/client"
import type { LockState } from "@/core/types"

/** How often the UI asks the core whether a timer has locked the vault. */
const POLL_MS = 3000
/** How often activity is reported, at most. */
const TOUCH_MS = 10_000
/** The warning before an idle lock, with "Stay unlocked" (brief §4). */
const WARN_BEFORE_S = 10
const WARN_TOAST = "idle-lock"

/**
 * The seconds until the core's idle timer locks, or `null` when no idle
 * lock is coming (locked already, no timer, or not yet near).
 */
export function idleLockIn(state: LockState): number | null {
  if (state.locked || state.idle_timeout_secs === null) return null
  const left = state.idle_timeout_secs - state.idle_secs
  return left <= WARN_BEFORE_S ? Math.max(0, left) : null
}

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
    // Ten seconds before the idle lock: one toast with "Stay unlocked",
    // which is activity (`touch`); it goes when the timer is no longer near.
    const warn = (state: LockState) => {
      const left = idleLockIn(state)
      if (left === null) {
        toast.dismiss(WARN_TOAST)
        return
      }
      toast(`Locking in ${left} s without you`, {
        id: WARN_TOAST,
        duration: (left + 1) * 1000,
        action: {
          label: "Stay unlocked",
          onClick: () => {
            last = Date.now()
            void client.touch().catch(() => {})
            toast.dismiss(WARN_TOAST)
          },
        },
      })
    }
    const seen = (state: LockState) => {
      warn(state)
      onState(state)
    }
    const visibility = () => {
      void client
        .setHidden(document.visibilityState === "hidden")
        .then(seen)
        .catch(() => {})
    }
    const poll = () => void client.lockState().then(seen).catch(() => {})
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
