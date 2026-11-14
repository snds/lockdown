import { useCallback, useEffect, useState } from "react"

import { useClient } from "@/core/client"
import type { LockState } from "@/core/types"

/** How often the UI asks the core whether a timer has locked the vault. */
const POLL_MS = 3000
/** How often activity is reported, at most. */
const TOUCH_MS = 10_000
/** The warning before an idle lock, with "Stay unlocked" (brief X2: the status line and the strip, D750). */
const WARN_BEFORE_S = 10

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
    const seen = (state: LockState) => onState(state)
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

/**
 * The idle lock's countdown, from the lock state the watch polls every few
 * seconds: the seconds left while it is 10 or fewer away, ticking each
 * second between polls, and `stay`, which is activity (`touch`), so the
 * core starts the idle timer over. The status line and the strip show it
 * (brief X2); nothing here decides a lock.
 */
export function useIdleCountdown(state: LockState | null): { left: number | null; stay: () => void } {
  const client = useClient()
  const near = state ? idleLockIn(state) : null
  const [deadline, setDeadline] = useState<number | null>(null)
  const [now, setNow] = useState(() => Date.now())
  // Stay was pressed: hidden until a poll says the lock is no longer near.
  const [stayed, setStayed] = useState(false)
  useEffect(() => {
    if (near === null) {
      setDeadline(null)
      setStayed(false)
      return
    }
    const at = Date.now()
    setNow(at)
    setDeadline(at + near * 1000)
  }, [near, state])
  useEffect(() => {
    if (deadline === null) return
    const t = window.setInterval(() => setNow(Date.now()), 1000)
    return () => window.clearInterval(t)
  }, [deadline])
  const stay = useCallback(() => {
    setStayed(true)
    void client.touch().catch(() => {})
  }, [client])
  const left = deadline === null || stayed ? null : Math.max(0, Math.min(WARN_BEFORE_S, Math.ceil((deadline - now) / 1000)))
  return { left, stay }
}
