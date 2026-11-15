import { createContext, useCallback, useContext, useEffect, useId, useMemo, useRef, useState, type ReactNode } from "react"

import { AppFrameAccessory, StatusItem, useAppFrame, type IconName } from "@unicron/ui"

/*
 * Channel 1 of the five (cross-app decisions §1, brief X2): what the app is
 * doing now. On a desktop it is the status line's end, on a phone the status
 * strip above the bottom row. One shows at a time, the first that is true:
 *
 *   1  the idle lock is 10 seconds away   LOCKING IN // 08 S, live, STAY UNLOCKED
 *   2  opening, syncing or checking       OPENING YOUR VAULT, SYNCING, CHECKING
 *   3  nothing above                      the screen's own line (`useStatusItem`)
 *
 * A needs-you outranks all of these on a phone (it takes the strip); on a
 * desktop it has its own block, and the status line is never yellow.
 */

/** What runs now that the person may wait on. */
export type LiveWork = "opening" | "syncing" | "checking"

/** A screen's own fact for the status line's end ("REFRESH EVERY 30 S", "CHECKED 09:40"). */
export interface StatusFact {
  /** The words, in sentence case: the line sets the capitals. `//` between the parts. */
  text: string
  /** Never needs-you: a status line is never yellow. */
  tone?: "neutral" | "warning" | "critical"
  mark?: IconName
  /** The whole fact is a button (HEALTH // 03 TO FIX opens Health). */
  onClick?: () => void
  /** The button's name when the words say less ("Open Health"). */
  label?: string
}

/** What the shell knows is happening, from the lock watch and the work the app runs. */
export interface LiveStatus {
  /** Seconds until the idle lock, when it is 10 or fewer away; otherwise null. */
  idleLeft: number | null
  /** Reports activity to the core, so the idle lock starts over. */
  onStay: () => void
  work: LiveWork | null
}

/** The row of the X2 table that shows: the first true one. */
export type StatusRow = { kind: "idle"; left: number } | { kind: "work"; work: LiveWork } | { kind: "screen"; fact: StatusFact } | null

/** Picks the one status that shows (brief X2 §1): the idle lock, then the work, then the screen's own line. */
export function statusRow(live: Pick<LiveStatus, "idleLeft" | "work">, fact: StatusFact | null): StatusRow {
  if (live.idleLeft !== null) return { kind: "idle", left: live.idleLeft }
  if (live.work) return { kind: "work", work: live.work }
  if (fact) return { kind: "screen", fact }
  return null
}

/** The idle lock's ten seconds: the countdown's length, and its segments. */
export const IDLE_WARN_SECS = 10
/** Said once when the countdown appears, never each second (contract 5.1c C7). */
export const IDLE_ANNOUNCE = "Locking in 10 seconds. Stay unlocked, button."

/** The desktop line's words and the phone strip's two lines for each kind of work. */
export const WORK_WORDS: Record<LiveWork, { line: string; meta: string; title: string }> = {
  opening: { line: "Opening your vault", meta: "Unlocking", title: "Opening your vault" },
  syncing: { line: "Syncing", meta: "Syncing", title: "Checking for changes" },
  checking: { line: "Checking", meta: "Checking", title: "Checking for breaches" },
}

interface StatusValue {
  live: LiveStatus
  /** What the host says when no screen says its own (the web vault's SYNCED 14:31, Items' Health line). */
  fallback: StatusFact | null
  register: (id: string, fact: StatusFact | null) => void
  fact: StatusFact | null
}

const NO_LIVE: LiveStatus = { idleLeft: null, onStay: () => {}, work: null }
const StatusContext = createContext<StatusValue | null>(null)

/**
 * Holds the status the shell shows. `live` comes from the app (the lock
 * watch and the work it runs), `fallback` is the host's line where a screen
 * has none of its own.
 */
export function StatusProvider({ live, fallback = null, children }: { live: LiveStatus; fallback?: StatusFact | null; children: ReactNode }) {
  // Each screen that says something registers under its own id; the last one to speak is the one shown.
  const [facts, setFacts] = useState<Array<[string, StatusFact]>>([])
  const register = useCallback((id: string, fact: StatusFact | null) => {
    setFacts((all) => {
      const rest = all.filter(([k]) => k !== id)
      return fact ? [...rest, [id, fact]] : rest
    })
  }, [])
  const own = facts.length ? facts[facts.length - 1]![1] : null
  const value = useMemo<StatusValue>(() => ({ live, fallback, register, fact: own ?? fallback }), [live, fallback, register, own])
  return <StatusContext.Provider value={value}>{children}</StatusContext.Provider>
}

/**
 * A screen's own line at the end of the status line (row 3 of brief X2):
 * "REFRESH EVERY 30 S" on Codes, "CHECKED 09:40" on Health, "03 RESULTS" on
 * Search. Shown while the screen is mounted and nothing live outranks it.
 * Pass `null` to say nothing. The press may change between renders; the
 * words decide when the line changes.
 */
export function useStatusItem(fact: StatusFact | null): void {
  const ctx = useContext(StatusContext)
  const register = ctx?.register
  const id = useId()
  const press = useRef(fact?.onClick)
  useEffect(() => {
    press.current = fact?.onClick
  })
  const text = fact?.text
  const tone = fact?.tone
  const mark = fact?.mark
  const label = fact?.label
  const pressable = Boolean(fact?.onClick)
  useEffect(() => {
    if (!register) return
    register(id, text === undefined ? null : { text, tone, mark, label, onClick: pressable ? () => press.current?.() : undefined })
    return () => register(id, null)
  }, [register, id, text, tone, mark, label, pressable])
}

/** The status the shell shows now, for a test or a part that reads it. */
export function useStatusRow(): StatusRow {
  const ctx = useContext(StatusContext)
  return ctx ? statusRow(ctx.live, ctx.fact) : null
}

function useLive(): LiveStatus {
  return useContext(StatusContext)?.live ?? NO_LIVE
}

/**
 * The desktop status line's end: one `StatusItem`, the first true row. The
 * idle lock counts down in the live tone with ten segments and STAY
 * UNLOCKED; it is said once when it appears.
 */
export function StatusLineItems() {
  const row = useStatusRow()
  const live = useLive()
  if (!row) return null
  if (row.kind === "idle")
    return (
      <StatusItem
        tone="live"
        mark="square"
        text="Locking in"
        countdown={row.left}
        progress={{ value: row.left, max: IDLE_WARN_SECS, segments: IDLE_WARN_SECS }}
        action={{ label: "Stay unlocked", onClick: live.onStay }}
        announce={IDLE_ANNOUNCE}
      />
    )
  if (row.kind === "work") return <StatusItem text={WORK_WORDS[row.work].line} announce={WORK_WORDS[row.work].line} />
  const f = row.fact
  return <StatusItem tone={f.tone} mark={f.mark} text={f.text} onClick={f.onClick} label={f.label} />
}

/**
 * The phone's status strip for rows 1 and 2 (row 3 has no strip): an
 * `Accessory` the frame places above the bottom row, inside it once the page
 * has scrolled, and gone on a desktop. A needs-you takes the strip before
 * these; `outranked` says one is showing.
 */
export function StatusStrip({ outranked = false }: { outranked?: boolean }) {
  const row = useStatusRow()
  const live = useLive()
  const frame = useAppFrame()
  // Where the frame draws the strip compact, inside the bottom row (the frame's own rule for AppFrameAccessory).
  const inRow = frame.layout === "rail" || frame.minimized
  if (outranked || !row || row.kind === "screen") return null
  if (row.kind === "idle")
    return (
      <AppFrameAccessory
        tone="live"
        meta="Locking in"
        countdown={row.left}
        title="No activity"
        // In the bottom row (scrolled, or a phone on its side) the strip is compact and keeps only an action with a
        // glyph; with one, STAY stays reachable everywhere the strip shows (a Unicron gap: compact drops a worded action).
        action={{ label: "Stay", icon: inRow ? "unlocked" : undefined, onClick: live.onStay }}
        progress={{ value: row.left, max: IDLE_WARN_SECS, segments: IDLE_WARN_SECS }}
        announce={IDLE_ANNOUNCE}
      />
    )
  const words = WORK_WORDS[row.work]
  return <AppFrameAccessory tone="neutral" meta={words.meta} title={words.title} announce={words.title} />
}

/**
 * The work the app runs that the person may wait on: `track("syncing", p)`
 * shows SYNCING until `p` settles. Opening is the time between unlock and
 * the first list, which the app passes as `opening`.
 */
export function useLiveWork(opening: boolean): { work: LiveWork | null; track: <T>(kind: LiveWork, p: Promise<T>) => Promise<T> } {
  const [running, setRunning] = useState<Record<LiveWork, number>>({ opening: 0, syncing: 0, checking: 0 })
  const track = useCallback(<T,>(kind: LiveWork, p: Promise<T>): Promise<T> => {
    setRunning((r) => ({ ...r, [kind]: r[kind] + 1 }))
    const done = () => setRunning((r) => ({ ...r, [kind]: Math.max(0, r[kind] - 1) }))
    p.then(done, done)
    return p
  }, [])
  const work: LiveWork | null = opening || running.opening ? "opening" : running.syncing ? "syncing" : running.checking ? "checking" : null
  return { work, track }
}

/**
 * What a pull down on the phone's content runs, and the plain "Check again"
 * beside it (cross-app decisions §8, brief 3.3): a sync on Items, Codes and
 * Sharing, CHECK NOW on Health; nothing elsewhere. The shell provides it by
 * the place that shows; the frame's columns read it.
 */
const RefreshContext = createContext<(() => Promise<unknown>) | undefined>(undefined)
export const ShellRefreshProvider = RefreshContext.Provider

/** The place's refresh, for a column's `onRefresh`; undefined where the place has none. */
export function useShellRefresh(): (() => Promise<unknown>) | undefined {
  return useContext(RefreshContext)
}

/** "14:31", the 24-hour clock the frames draw. */
export function clockWords(ms: number): string {
  const d = new Date(ms)
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`
}
