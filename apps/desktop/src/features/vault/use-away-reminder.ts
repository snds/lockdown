import { useEffect, useRef } from "react"

import type { Client } from "@/core/client"
import type { AwayReminder } from "@/core/types"

/** The words of the sign-out news (brief X2 table 5, D237): who and where. */
export function awayNews(due: ReadonlyArray<Pick<AwayReminder, "title" | "place">>): { title: string; body?: string } | null {
  const first = due[0]
  if (!first) return null
  return due.length === 1
    ? { title: `Sign out of ${first.title} on ${first.place}` }
    : { title: `${due.length} sign-ins elsewhere to sign out of`, body: `First ${first.title} on ${first.place}.` }
}

/**
 * After unlock: one piece of news for the sign-ins elsewhere whose reminder
 * is due (D237), as an info Banner at the top of All items (D753). "Show"
 * opens the item when there is one, and Needs attention when there are
 * more. Comes back on every unlock until the person marks it signed out,
 * unless dismissed today.
 */
export function useAwayReminder(client: Client, open: boolean, notify: (news: { title: string; body?: string }, show: () => void) => void, onShow: (itemId: string | null) => void) {
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
        const news = awayNews(due)
        if (!news) return
        const only = due.length === 1 ? (due[0]?.item_id ?? null) : null
        notify(news, () => onShow(only))
      })
      .catch(() => {})
  }, [client, open, notify, onShow])
}
