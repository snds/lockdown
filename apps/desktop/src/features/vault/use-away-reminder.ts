import { useEffect, useRef } from "react"

import type { Client } from "@/core/client"

/**
 * After unlock: one toast for each sign-in elsewhere whose reminder is
 * due, with "Show" opening Needs attention (D237). Comes back on every
 * unlock until the person marks it signed out.
 */
export function useAwayReminder(client: Client, open: boolean, notify: (text: string, show: () => void) => void, onShow: () => void) {
  const shown = useRef(false)
  useEffect(() => {
    if (!open) {
      shown.current = false
      return
    }
    if (shown.current) return
    shown.current = true
    Promise.resolve()
      .then(() => client.awayReminders())
      .then((all) => {
        const due = (Array.isArray(all) ? all : []).filter((a) => a.due)
        const first = due[0]
        if (!first) return
        notify(
          due.length === 1
            ? `Sign out of "${first.title}" on ${first.place}.`
            : `${due.length} sign-ins elsewhere to sign out of; first "${first.title}" on ${first.place}.`,
          onShow,
        )
      })
      .catch(() => {})
  }, [client, open, notify, onShow])
}
