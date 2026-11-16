import { useEffect } from "react"

import type { NewsItem } from "@/app/banners"

import { useUpdateClient, type UpdateStatus } from "./update-client"
import { useUpdates } from "./use-updates"

/**
 * A ready update that waits too long (Lockdown 7; cross-app decisions §3,
 * its second exception). An update is quiet by default: a mark on the way
 * into Settings and the rows in About, never a toast. But a password
 * manager left unpatched is a real risk, and people skip restarts for
 * weeks, so once a downloaded update has waited seven days it also shows as
 * the one dismissable info Banner on All items (channel 5, news), with
 * RESTART TO UPDATE. Dismissed, it stays away for the day, as all news does.
 */

/** How long a ready update waits before it shows as news. */
export const UPDATE_WAIT_DAYS = 7

const KEY = "lockdown.update-ready-since"

interface Since {
  /** The version that became ready. */
  version: string
  /** When this device first saw it ready, in Unix seconds. */
  at: number
}

function read(): Since | null {
  try {
    const raw = window.localStorage.getItem(KEY)
    const kept = raw ? (JSON.parse(raw) as Partial<Since>) : null
    return kept && typeof kept.version === "string" && typeof kept.at === "number" ? { version: kept.version, at: kept.at } : null
  } catch {
    return null
  }
}

function write(since: Since | null) {
  try {
    if (since) window.localStorage.setItem(KEY, JSON.stringify(since))
    else window.localStorage.removeItem(KEY)
  } catch {
    // Kept for this session only: the clock starts again next launch, which errs on the quiet side.
  }
}

/**
 * When the update now ready became ready, in Unix seconds, noted on this
 * device the first time it is seen ready; `null` when no update is ready.
 * A newer version that becomes ready starts its own clock; an update that
 * is no longer ready (installed, or replaced) forgets it.
 */
export function readySince(status: UpdateStatus | null, nowSecs: number = Date.now() / 1000): number | null {
  if (!status) return null
  if (status.state !== "ready") {
    // Only an installed or failed update forgets it: checking again on the way does not restart the week.
    if (status.state === "up_to_date" || status.state === "failed") write(null)
    return null
  }
  const version = status.offered ?? ""
  const kept = read()
  if (kept && kept.version === version) return kept.at
  write({ version, at: nowSecs })
  return nowSecs
}

/** Whether a ready update has waited long enough to show as news. */
export function waitedTooLong(since: number | null, nowSecs: number = Date.now() / 1000): boolean {
  return since !== null && nowSecs - since >= UPDATE_WAIT_DAYS * 86_400
}

/** The news for an update that waited a week. */
export function updateWaitingNews(status: UpdateStatus, restart: () => void): NewsItem {
  const v = status.offered
  return {
    id: `update:${v ?? "ready"}`,
    title: v ? `Lockdown ${v} has waited a week` : "An update has waited a week",
    body: "It may fix a security problem. Restarting locks Lockdown; unlock it with your master password as usual.",
    action: { label: "Restart to update", onClick: restart },
  }
}

/** Adds the news once a ready update has waited a week, while the vault is open. */
export function useUpdateWaiting(open: boolean, add: (item: NewsItem) => void) {
  const client = useUpdateClient()
  const status = useUpdates()
  useEffect(() => {
    if (!client || !open) return
    const since = readySince(status)
    if (status && waitedTooLong(since)) add(updateWaitingNews(status, () => void client.restart()))
  }, [client, open, status, add])
}
