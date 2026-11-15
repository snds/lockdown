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

/**
 * When an expiring item lapses, as the right-hand word of its row on Health
 * (brief H1): "Expired", "Today", "In 6 days", "In 3 weeks", "In 2 months".
 * The row's status role sets the capitals.
 */
export function expiryStatus(days: number | null | undefined): string {
  if (days == null) return ""
  if (days < 0) return "Expired"
  if (days === 0) return "Today"
  if (days === 1) return "Tomorrow"
  if (days < 14) return `In ${days} days`
  if (days < 60) return `In ${Math.round(days / 7)} weeks`
  if (days < 730) return `In ${Math.round(days / 30)} months`
  return `In ${Math.round(days / 365)} years`
}

/** Expiring rows, soonest first (Health's EXPIRING list). */
export function soonestFirst(rows: ItemSummary[]): ItemSummary[] {
  return [...rows].sort((a, b) => (a.expires_in_days ?? 1e9) - (b.expires_in_days ?? 1e9) || a.title.localeCompare(b.title))
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

/** The words of the expiry news (brief X2 table 5): what is due as the title, the soonest as the body. */
export function expiryNews(due: ReadonlyArray<{ title: string; days: number }>): { title: string; body?: string } | null {
  const first = due[0]
  if (!first) return null
  const soonest = `${first.title} ${expiryWords(first.days)}`
  return due.length === 1 ? { title: soonest } : { title: `${due.length} items expire soon`, body: `${soonest}.` }
}

/**
 * Once a day, after unlock: one piece of news naming what lapses within
 * its reminder lead (a card from sixty days out, a passport from ninety),
 * with "Show" opening Health at Expiring. It is an info Banner at the top
 * of All items (D753), not a toast. The dates come from the core; what is
 * remembered here is only the day the news last came.
 */
export function useExpiryReminder(
  client: Client,
  open: boolean,
  notify: (news: { title: string; body?: string }, show: () => void) => void,
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
        const news = expiryNews(due)
        if (!news) return
        markReminded(today)
        notify(news, onShow)
      })
      .catch(() => {})
  }, [client, open, notify, onShow])
}
