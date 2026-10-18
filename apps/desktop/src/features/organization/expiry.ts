import { useEffect, useRef } from "react"

import type { Client } from "@/core/client"
import type { ItemSummary } from "@/core/types"
import type { RowGroup } from "@/features/vault/list-model"

import { EXPIRY_LABELS, expiryBucket, expiryWords, type ExpiryBucket } from "./org-model"

const ORDER: ExpiryBucket[] = ["lapsed", "week", "month", "later"]

/** The Expiring view's sections: expired, within a week, within a month, later; soonest first in each. */
export function groupByExpiry(rows: ItemSummary[]): RowGroup[] {
  const sorted = [...rows].sort((a, b) => (a.expires_in_days ?? 1e9) - (b.expires_in_days ?? 1e9) || a.title.localeCompare(b.title))
  const out: RowGroup[] = []
  for (const bucket of ORDER) {
    const list = sorted.filter((r) => expiryBucket(r.expires_in_days) === bucket)
    if (list.length) out.push({ key: `expiry:${bucket}`, label: EXPIRY_LABELS[bucket], rows: list })
  }
  return out
}

/** Where this device last reminded, so a reminder shows once a day. */
const REMINDED_KEY = "lockdown.expiry-reminded"

function remindedToday(today: string): boolean {
  try {
    return window.localStorage.getItem(REMINDED_KEY) === today
  } catch {
    return false
  }
}

function markReminded(today: string) {
  try {
    window.localStorage.setItem(REMINDED_KEY, today)
  } catch {
    // Private mode or blocked storage: the reminder may show again; nothing is lost.
  }
}

/**
 * Once a day, after unlock: one toast naming what lapses within its
 * reminder lead (a card from sixty days out, a passport from ninety), with
 * "Show" opening the Expiring view. The dates come from the core; what is
 * remembered here is only the day the toast last showed.
 */
export function useExpiryReminder(
  client: Client,
  open: boolean,
  notify: (text: string, show: () => void) => void,
  onShow: () => void,
) {
  const shown = useRef(false)
  useEffect(() => {
    if (!open) {
      shown.current = false
      return
    }
    if (shown.current) return
    shown.current = true
    const today = new Date().toISOString().slice(0, 10)
    if (remindedToday(today)) return
    client
      .expiryReminders()
      .then((due) => {
        const first = due[0]
        if (!first) return
        markReminded(today)
        const text =
          due.length === 1 ? `"${first.title}" ${expiryWords(first.days)}.` : `${due.length} items expire soon; "${first.title}" ${expiryWords(first.days)}.`
        notify(text, onShow)
      })
      .catch(() => {})
  }, [client, open, notify, onShow])
}
