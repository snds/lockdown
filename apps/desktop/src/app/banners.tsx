import { createContext, useCallback, useContext, useMemo, useState, type ReactNode } from "react"

import { Banner, Button } from "@unicron/ui"

/*
 * Channels 4 and 5 of the five (cross-app decisions §1, brief X2):
 *
 *   4  broken, and the person can fix it: one critical Banner, the first
 *      true one, at the top of Items and of the section it belongs to. It
 *      cannot be dismissed; it leaves when it clears. Never a toast.
 *   5  news that can wait: one info Banner at the top of All items,
 *      dismissable; a dismissed one stays away for the rest of the day.
 */

/** The four ways the app can be broken in a way the person can fix, in the order they outrank each other. */
export type CriticalKind = "sign-in" | "split" | "sync" | "backup"

/** What is true now, each in the core's words; null when it isn't. */
export interface CriticalSources {
  /** This device must sign in again (`Status.account.sign_in`, or a live `session_ended`). */
  signIn: string | null
  /** A sync line that holds "Split view" (D328). */
  split: string | null
  /** The last sync failed (a live `sync_failed`, a rejected `client.sync()`). */
  sync: string | null
  /** Backups are on and the last one failed (`BackupStatus.last_error`). */
  backup: string | null
}

/** What each Banner's one action does. */
export interface CriticalActions {
  signIn: () => void
  seeStatus: () => void
  tryAgain: () => void
  openBackups: () => void
}

/** The places a critical Banner shows, besides Items. */
export type BannerPlace = "items" | "all-items" | "sharing" | "status" | "backups"

const ORDER: CriticalKind[] = ["sign-in", "split", "sync", "backup"]
const SOURCE: Record<CriticalKind, keyof CriticalSources> = { "sign-in": "signIn", split: "split", sync: "sync", backup: "backup" }
const ALSO: Record<CriticalKind, BannerPlace[]> = { "sign-in": ["status"], split: ["sharing", "status"], sync: [], backup: ["backups"] }

/** The one critical Banner that shows: the first true row of brief X2's table 4. */
export function firstCritical(s: CriticalSources): CriticalKind | null {
  return ORDER.find((k) => s[SOURCE[k]] !== null) ?? null
}

/** One critical Banner, ready to draw. */
export interface CriticalBanner {
  kind: CriticalKind
  title: string
  body: string
  /** No action where the Banner sits on the page its action would open (Status, for the split view). */
  action: { label: string; onClick: () => void } | null
  places: BannerPlace[]
}

/** The words and the action of the Banner for `kind`, in the core's words where it has them. */
export function criticalBanner(kind: CriticalKind, s: CriticalSources, act: CriticalActions): CriticalBanner {
  const places: BannerPlace[] = ["items", ...ALSO[kind]]
  switch (kind) {
    case "sign-in":
      return { kind, title: "Lockdown isn't syncing", body: s.signIn ?? "", action: { label: "Sign in", onClick: act.signIn }, places }
    case "split":
      return {
        kind,
        title: "Split view",
        body: "Your server may be showing you a different history from everyone else. New sharing is paused until it clears.",
        action: { label: "See status", onClick: act.seeStatus },
        places,
      }
    case "sync":
      return { kind, title: "Couldn't sync", body: s.sync ?? "", action: { label: "Try again", onClick: act.tryAgain }, places }
    case "backup":
      return { kind, title: "The last backup didn't work", body: s.backup ?? "", action: { label: "Open backups", onClick: act.openBackups }, places }
  }
}

/** A piece of news that can wait (brief X2 table 5): an expiry, a sign-out reminder, a passkey a site added. */
export interface NewsItem {
  /** Stable across unlocks, so a dismissal holds for the day ("expiry", "away", "upgrade:<item>"). */
  id: string
  title: string
  body?: string
  action: { label: string; onClick: () => void }
}

const DISMISSED_KEY = "lockdown.news-dismissed"

function today(): string {
  return new Date().toISOString().slice(0, 10)
}

function dismissedToday(): string[] {
  try {
    const raw = window.localStorage.getItem(DISMISSED_KEY)
    const kept = raw ? (JSON.parse(raw) as { day?: string; ids?: string[] }) : null
    return kept?.day === today() && Array.isArray(kept.ids) ? kept.ids : []
  } catch {
    return []
  }
}

function keepDismissed(ids: string[]) {
  try {
    window.localStorage.setItem(DISMISSED_KEY, JSON.stringify({ day: today(), ids }))
  } catch {
    // Kept for this session only.
  }
}

/**
 * The news that waits, one shown at a time in the order it came. `add`
 * takes a piece of news; one dismissed today, or already waiting, is left
 * out. `dismiss` puts the shown one away until tomorrow; `clear` drops all
 * of it (on lock).
 */
export function useNews(): { news: NewsItem | null; add: (item: NewsItem) => void; dismiss: () => void; clear: () => void } {
  const [queue, setQueue] = useState<NewsItem[]>([])
  const add = useCallback((item: NewsItem) => {
    if (dismissedToday().includes(item.id)) return
    setQueue((q) => (q.some((n) => n.id === item.id) ? q.map((n) => (n.id === item.id ? item : n)) : [...q, item]))
  }, [])
  const dismiss = useCallback(() => {
    setQueue((q) => {
      const first = q[0]
      if (first) keepDismissed([...dismissedToday(), first.id])
      return q.slice(1)
    })
  }, [])
  const clear = useCallback(() => setQueue([]), [])
  return { news: queue[0] ?? null, add, dismiss, clear }
}

interface BannerValue {
  critical: CriticalBanner | null
  news: NewsItem | null
  onDismissNews: () => void
}

const BannerContext = createContext<BannerValue>({ critical: null, news: null, onDismissNews: () => {} })

/** Holds the Banners that show now; the shell's `BannerSlot`s draw them where they belong. */
export function MessageBoardProvider({ critical, news, onDismissNews, children }: BannerValue & { children: ReactNode }) {
  const value = useMemo(() => ({ critical, news, onDismissNews }), [critical, news, onDismissNews])
  return <BannerContext.Provider value={value}>{children}</BannerContext.Provider>
}

/** The Banners that show now: the critical one and the news, wherever they sit. */
export function useBanner(): BannerValue {
  return useContext(BannerContext)
}

/** Whether `place` has a Banner to show now, so a slot's space is laid out only when it is filled. */
export function useBannerShows(place: BannerPlace): boolean {
  const { critical, news } = useBanner()
  return (critical !== null && critical.places.includes(place === "all-items" ? "items" : place)) || (place === "all-items" && news !== null)
}

/**
 * The Banners of one place: the critical Banner where it belongs, and on
 * All items the news under it. Nothing when nothing is true, so a slot can
 * sit on every screen it serves.
 */
export function BannerSlot({ place }: { place: BannerPlace }) {
  const { critical, news, onDismissNews } = useBanner()
  const showCritical = critical !== null && critical.places.includes(place === "all-items" ? "items" : place)
  // On Status the split view's Banner sits on the page its action opens: no action there.
  const action = critical && !(critical.kind === "split" && place === "status") ? critical.action : null
  const showNews = place === "all-items" && news !== null
  if (!showCritical && !showNews) return null
  return (
    <div className="flex flex-col gap-4">
      {showCritical && critical && (
        <Banner
          tone="critical"
          title={critical.title}
          body={critical.body || undefined}
          action={
            action ? (
              <Button variant="secondary" size="sm" onClick={action.onClick}>
                {action.label}
              </Button>
            ) : undefined
          }
        />
      )}
      {showNews && news && (
        <Banner
          tone="info"
          title={news.title}
          body={news.body}
          action={
            <Button variant="secondary" size="sm" onClick={news.action.onClick}>
              {news.action.label}
            </Button>
          }
          onDismiss={onDismissNews}
        />
      )}
    </div>
  )
}
