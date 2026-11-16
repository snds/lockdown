import { createContext, useCallback, useContext, useEffect, useLayoutEffect, useMemo, useRef, useState, useSyncExternalStore, type CSSProperties, type ReactNode } from "react"
import { createPortal } from "react-dom"

import { Accessory, AdaptiveDialog, AppFrameAccessory, Button, ListRow, NeedsYou, SectionHeader, useAppFrame, useNeedsYouCompactShowing } from "@unicron/ui"

import { notify } from "./notify"

/*
 * Channel 3 of the five (cross-app decisions §1, brief X3): the one thing
 * that waits on the person. Each source registers what it has with
 * `useNeedsYouSource(entry)`; the shell draws the first true entry by the X3
 * priority table, in one place per screen:
 *
 *   desktop, medium   one NeedsYou block at the top of the main content column:
 *                     the top of the detail pane beside a list, under the page
 *                     head on a page, the top of the section in Settings
 *   phone             the status strip in the needs-you tone (an Accessory),
 *                     which keeps its yellow inside the scrolled bottom row (Q6)
 *   under a layer     rows 1 to 3 (they run out) as the compact line over the
 *                     open dialog or sheet; the block behind the scrim hides
 *
 *   1 sign-in request   2 pairing request   3 device request     (edges-1)
 *   4 emergency access                                           (edges-8)
 *   5 a conflict on the open item                                (vault-7)
 *   6 a reset account waiting to be let back in, on Sharing      (edges-7)
 *   7 kit not tested    8 master password   9 recovery drill     (the reminders)
 *
 * One per screen and one yellow shape for one thing. A second one that waits
 * adds `// +1` to the title; a screen's own lower item (rows 5 and 6) is an
 * ordinary row under WAITING ON YOU NN (`useWaitingRows`, `WaitingOnYou`).
 * No close control, never swiped away: it leaves when it is answered, when it
 * runs out, or when the person picks its text action.
 */

/** The row of the X3 table: 1 waits most. */
export type NeedsYouPriority = 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9

/**
 * Where an entry shows: on every screen, with one item open, on Sharing, or
 * on the home screen (All items) only. The cross-app rule (Synapse 4,
 * settled across apps): what is time-limited or blocking shows on every
 * screen; what can wait (the reminders, rows 7 to 9) shows as a needs-you
 * count on Settings everywhere, and as the block on the home screen.
 */
export type NeedsYouShows = "every" | "sharing" | "home" | { item: string }

export interface NeedsYouAction {
  label: string
  onClick: () => void
  /** The action runs now: the button keeps its label and shows its spinner. */
  busy?: boolean
}

export interface NeedsYouEntry {
  /** Stable for as long as the thing waits (a request's id, `reminder:kit-test`). */
  id: string
  priority: NeedsYouPriority
  /** What waits, as the overline says it, `//` between the parts ("Recovery kit // not tested"). The time left is added from `expiresAt`. */
  title: string
  /** One sentence on what it is ("Test your recovery kit."). */
  line: string
  /** The one solid action. */
  action: NeedsYouAction
  /** The text action, if there is one. */
  textAction?: NeedsYouAction
  shows: NeedsYouShows
  /** When it runs out (ms since the epoch). It leaves by itself then, and a toast says so. */
  expiresAt?: number
  /** The toast when it runs out ("Sign-in request ran out"). */
  expiredWords?: string
}

/** Where the person is: what an entry's `shows` is matched against. */
export interface NeedsYouPlace {
  sharing: boolean
  /** The home screen: All items, with nothing narrowing it. Left out, no screen is home. */
  home?: boolean
  /** The item open in the detail, if any. */
  item: string | null
}

/** What the strip's action says on a phone, by row: REVIEW opens the review, RESOLVE and OPEN a sheet (brief X3). */
export function stripActionWord(e: NeedsYouEntry): string {
  if (e.priority <= 4) return e.action.label
  return e.priority === 5 ? "Resolve" : "Open"
}

/** The entries of rows 1 to 3 run out (two minutes, D208); only they take the compact line over a layer. */
export function isTimed(e: NeedsYouEntry): boolean {
  return e.priority <= 3
}

/** Seconds left as decisions §13 writes them: `01:40`, then `08 s` under a minute. */
export function leftWords(secs: number): string {
  const s = Math.max(0, Math.ceil(secs))
  if (s < 60) return `${String(s).padStart(2, "0")} s`
  return `${String(Math.floor(s / 60)).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}`
}

/** The overline as drawn: the entry's title, then the time left for one that runs out. */
export function titleOf(e: NeedsYouEntry, now: number): string {
  return e.expiresAt === undefined ? e.title : `${e.title} // ${leftWords((e.expiresAt - now) / 1000)} left`
}

/** When the time left is said again, once (brief X3, keys and screen reader). */
export const SAY_AGAIN_AT_S = 30

/** What a screen reader hears: once when it arrives, and once more at 30 s. Never the ticks. */
export function announceOf(e: NeedsYouEntry, now: number): string {
  const base = `${e.title}. ${e.line}`
  if (e.expiresAt === undefined) return base
  return (e.expiresAt - now) / 1000 <= SAY_AGAIN_AT_S ? `${base} ${SAY_AGAIN_AT_S} seconds left.` : base
}

export function matches(e: NeedsYouEntry, place: NeedsYouPlace): boolean {
  if (e.shows === "every") return true
  if (e.shows === "sharing") return place.sharing
  if (e.shows === "home") return place.home === true
  return place.item === e.shows.item
}

/** What shows, by the X3 rules: the first true entry, how many more wait, and the screen's own lower ones. */
export function pick(entries: readonly NeedsYouEntry[], place: NeedsYouPlace, now: number): { top: NeedsYouEntry | null; more: number; waiting: NeedsYouEntry[] } {
  const here = entries.filter((e) => matches(e, place) && (e.expiresAt === undefined || e.expiresAt > now))
  // A stable sort: two of one row keep the order they arrived in.
  const sorted = here.map((e, i) => [e, i] as const).sort((a, b) => a[0].priority - b[0].priority || a[1] - b[1]).map(([e]) => e)
  const top = sorted[0] ?? null
  return { top, more: Math.max(0, sorted.length - 1), waiting: sorted.slice(1).filter((e) => e.shows !== "every") }
}

interface NeedsYouValue {
  register: (entry: NeedsYouEntry | null, id: string) => void
  /** Runs an entry's action with the newest callback its source gave. */
  press: (id: string, which: "action" | "text") => void
  top: NeedsYouEntry | null
  more: number
  waiting: NeedsYouEntry[]
  /** How many entries that can wait (`shows: "home"`) wait, wherever the person is: the nav's count. */
  later: number
  now: number
}

const NeedsYouContext = createContext<NeedsYouValue | null>(null)

/*
 * A source that is not a component (an event the host sends, a review
 * harness) adds its entry here; the host draws it with the others. It is
 * kept outside the vault's tree, so the host clears it on every lock.
 */
const outside = new Map<string, NeedsYouEntry>()
const outsideListeners = new Set<() => void>()
let outsideList: NeedsYouEntry[] = []
const outsideChanged = () => {
  outsideList = [...outside.values()]
  outsideListeners.forEach((l) => l())
}

/** Adds an entry from outside a component. Returns its removal. */
export function addNeedsYou(entry: NeedsYouEntry): () => void {
  outside.set(entry.id, entry)
  outsideChanged()
  return () => {
    if (outside.get(entry.id) !== entry) return
    outside.delete(entry.id)
    outsideChanged()
  }
}

function useOutside(): NeedsYouEntry[] {
  return useSyncExternalStore(
    (l) => {
      outsideListeners.add(l)
      return () => outsideListeners.delete(l)
    },
    () => outsideList,
    () => outsideList,
  )
}

/**
 * Holds what waits on the person, for the unlocked app. It sits inside the
 * unlocked branch, so nothing of it is drawn while locked; a source that
 * finds something while locked registers it after unlock, and it shows then.
 */
export function WaitingHost({ place, children }: { place: NeedsYouPlace; children: ReactNode }) {
  // What each source said last. A source hands a new object (and new callbacks) every render; the list
  // changes only when its words, times or busy states do, and a press always reaches the newest callback.
  const latest = useRef(new Map<string, NeedsYouEntry>())
  const [entries, setEntries] = useState<NeedsYouEntry[]>([])
  const register = useCallback((entry: NeedsYouEntry | null, id: string) => {
    if (entry) latest.current.set(id, entry)
    else latest.current.delete(id)
    setEntries((all) => {
      const at = all.findIndex((e) => e.id === id)
      if (!entry) return at < 0 ? all : all.filter((e) => e.id !== id)
      if (at < 0) return [...all, entry]
      if (JSON.stringify(all[at]) === JSON.stringify(entry)) return all
      const next = all.slice()
      next[at] = entry
      return next
    })
  }, [])
  const fromOutside = useOutside()
  // Nothing from outside outlives the unlocked app: the host unmounts on lock.
  useEffect(
    () => () => {
      outside.clear()
      outsideChanged()
    },
    [],
  )
  const press = useCallback((id: string, which: "action" | "text") => {
    const e = latest.current.get(id) ?? outside.get(id)
    if (which === "action") e?.action.onClick()
    else e?.textAction?.onClick()
  }, [])

  const all = useMemo(() => [...entries, ...fromOutside.filter((o) => !entries.some((e) => e.id === o.id))], [entries, fromOutside])
  // A clock for the time left, ticking only while something runs out.
  const timed = all.some((e) => e.expiresAt !== undefined)
  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    if (!timed) return
    setNow(Date.now())
    const t = window.setInterval(() => setNow(Date.now()), 1000)
    return () => window.clearInterval(t)
  }, [timed])

  // A request that runs out leaves by itself, and a toast says so, once.
  const told = useRef(new Set<string>())
  useEffect(() => {
    for (const e of all) {
      if (e.expiresAt === undefined || e.expiresAt > now || told.current.has(e.id)) continue
      told.current.add(e.id)
      notify(e.expiredWords ?? `${e.title} ran out`)
    }
  }, [all, now])

  const { top, more, waiting } = useMemo(() => pick(all, place, now), [all, place, now])
  const later = useMemo(() => all.filter((e) => e.shows === "home").length, [all])
  const value = useMemo<NeedsYouValue>(() => ({ register, press, top, more, waiting, later, now }), [register, press, top, more, waiting, later, now])
  return (
    <NeedsYouContext.Provider value={value}>
      {children}
      <NeedsYouOverLayer />
    </NeedsYouContext.Provider>
  )
}

/**
 * A source's entry, while it waits: `useNeedsYouSource(due ? {...} : null)`.
 * The entry may be a new object each render; the shell redraws when it changes.
 */
export function useNeedsYouSource(entry: NeedsYouEntry | null): void {
  const register = useContext(NeedsYouContext)?.register
  const id = useRef<string | null>(null)
  useEffect(() => {
    if (!register) return
    if (id.current && id.current !== entry?.id) register(null, id.current)
    id.current = entry?.id ?? null
    if (entry) register(entry, entry.id)
  })
  useEffect(
    () => () => {
      if (register && id.current) register(null, id.current)
    },
    [register],
  )
}

/** The first true entry, and how many more wait. */
export function useNeedsYouTop(): { top: NeedsYouEntry | null; more: number; now: number } {
  const ctx = useContext(NeedsYouContext)
  const top = ctx?.top ?? null
  const press = ctx?.press
  // The actions as drawn call the source's newest callbacks.
  const live = useMemo(
    () =>
      top && press
        ? { ...top, action: { ...top.action, onClick: () => press(top.id, "action") }, textAction: top.textAction ? { ...top.textAction, onClick: () => press(top.id, "text") } : undefined }
        : top,
    [top, press],
  )
  return { top: live, more: ctx?.more ?? 0, now: ctx?.now ?? 0 }
}

/**
 * How many things that can wait wait now (the reminders), wherever the
 * person is: drawn as a count in the needs-you tone on the way into
 * Settings, where they are answered (the cross-app rule; their block shows
 * on the home screen only).
 */
export function useNeedsYouLater(): number {
  return useContext(NeedsYouContext)?.later ?? 0
}

/**
 * The screen's own entries that lost to a higher one (rows 5 and 6): the
 * screen draws them as ordinary rows under WAITING ON YOU NN (`WaitingOnYou`),
 * never as a second yellow block.
 */
export function useWaitingRows(): NeedsYouEntry[] {
  const ctx = useContext(NeedsYouContext)
  const waiting = ctx?.waiting
  const press = ctx?.press
  return useMemo(
    () =>
      (waiting ?? []).map((e) =>
        press ? { ...e, action: { ...e.action, onClick: () => press(e.id, "action") }, textAction: e.textAction ? { ...e.textAction, onClick: () => press(e.id, "text") } : undefined } : e,
      ),
    [waiting, press],
  )
}

/** An entry's action as a button on the yellow: solid, or text only. */
function actionNode(action: NeedsYouAction, solid: boolean, size?: "sm") {
  return (
    <Button type="button" variant={solid ? "needs-you" : "needs-you-text"} size={size} loading={action.busy} onClick={action.onClick}>
      {action.label}
    </Button>
  )
}

/** One polite line for a screen reader, written once on arrival and again when its words change (at 30 s). */
function Announce({ text }: { text: string }) {
  const ref = useRef<HTMLSpanElement>(null)
  useEffect(() => {
    if (ref.current) ref.current.textContent = text
  }, [text])
  return <span ref={ref} role="status" data-needs-you-announce="" className="sr-only" />
}

/**
 * The desktop block (and medium width's): the NeedsYou at the top of the
 * column it is put in. Nothing on a phone (the strip says it there) and
 * nothing when nothing waits.
 */
export function NeedsYouSlot() {
  const frame = useAppFrame()
  const { top, more, now } = useNeedsYouTop()
  if (!top || frame.phone) return null
  return (
    <>
      <NeedsYou
        title={titleOf(top, now)}
        more={more || undefined}
        body={top.line}
        announce={false}
        data-needs-you-row={top.priority}
        action={actionNode(top.action, true)}
        secondaryAction={top.textAction ? actionNode(top.textAction, false) : undefined}
      />
      <Announce text={announceOf(top, now)} />
    </>
  )
}

const UnderHeadContext = createContext(false)

/** A page (Health, Sharing, Codes): the block goes under its page head. `PageColumn` wraps its screen in it. */
export function UnderPageHead({ children }: { children: ReactNode }) {
  return <UnderHeadContext.Provider value={true}>{children}</UnderHeadContext.Provider>
}

/** Whether a block shows now: something waits and this is not a phone (the strip says it there). */
export function useNeedsYouBlock(): boolean {
  const frame = useAppFrame()
  return useNeedsYouTop().top !== null && !frame.phone
}

/** For the page head: the block to draw under it on a page, or null. */
export function useNeedsYouUnderHead(): ReactNode {
  const page = useContext(UnderHeadContext)
  const shows = useNeedsYouBlock()
  return page && shows ? <NeedsYouSlot /> : null
}

/**
 * The phone's strip: an `Accessory` in the needs-you tone in the strip's
 * slot, its text one button and its action a sibling (contract C5). Rows 1 to
 * 4 open their review; rows 5 to 9 open a sheet with the line, the solid
 * action and the text action. No dismiss, no swipe away. Scrolled, it moves
 * into the bottom row and keeps its yellow (Q6).
 */
export function NeedsYouStrip() {
  const frame = useAppFrame()
  const { top, more, now } = useNeedsYouTop()
  const [sheet, setSheet] = useState<string | null>(null)
  // The compact line over a layer is the one yellow shape meanwhile; the strip behind the scrim steps aside.
  const covered = useNeedsYouCompactShowing()
  if (!top || !frame.phone || covered) return null
  const open = top.priority <= 4 ? top.action.onClick : () => setSheet(top.id)
  const title = titleOf(top, now)
  return (
    <>
      <AppFrameAccessory
        tone="needs-you"
        meta={more ? `${title} // +${more}` : title}
        title={top.line}
        action={{ label: stripActionWord(top), onClick: open }}
        onOpen={open}
        openLabel={stripActionWord(top)}
        announce={announceOf(top, now)}
        data-needs-you-row={top.priority}
      />
      <NeedsYouSheet entry={sheet === top.id ? top : null} now={now} onClose={() => setSheet(null)} />
    </>
  )
}

/** Rows 5 to 9 on a phone: the line, the solid action as the layer's primary and the text action as text. */
function NeedsYouSheet({ entry, now, onClose }: { entry: NeedsYouEntry | null; now: number; onClose: () => void }) {
  const run = (a: NeedsYouAction) => () => {
    onClose()
    a.onClick()
  }
  return (
    <AdaptiveDialog
      open={entry !== null}
      onOpenChange={(o) => !o && onClose()}
      presentation="sheet"
      overline={entry ? titleOf(entry, now) : undefined}
      title={entry?.line ?? ""}
      footer={
        entry ? (
          <>
            {entry.textAction && (
              <Button type="button" variant="ghost" onClick={run(entry.textAction)}>
                {entry.textAction.label}
              </Button>
            )}
            <Button type="button" onClick={run(entry.action)}>
              {entry.action.label}
            </Button>
          </>
        ) : undefined
      }
    />
  )
}

/**
 * The screen's own lower entries as ordinary rows (brief X3 rule 2): a
 * Section Header WAITING ON YOU NN, then a row each with its actions as
 * secondary buttons. Nothing when none waits.
 */
export function WaitingOnYou() {
  const rows = useWaitingRows()
  if (!rows.length) return null
  return (
    <section aria-label="Waiting on you" className="flex flex-col">
      <SectionHeader title="Waiting on you" count={rows.length} />
      <div role="list" aria-label="Waiting on you">
        {rows.map((e) => (
            <ListRow
              key={e.id}
              role="listitem"
              title={e.title}
              meta={e.line}
              action={
                <span className="flex items-center gap-2">
                  {e.textAction && (
                    <Button type="button" variant="ghost" size="sm" loading={e.textAction.busy} onClick={e.textAction.onClick}>
                      {e.textAction.label}
                    </Button>
                  )}
                  <Button type="button" variant="secondary" size="sm" loading={e.action.busy} onClick={e.action.onClick}>
                    {e.action.label}
                  </Button>
                </span>
              }
            />
        ))}
      </div>
    </section>
  )
}

/** The layers the over-layer line can sit on: open dialogs and sheets, not menus. */
const LAYERS = '[role="dialog"], [role="alertdialog"]'

/** The open layer on top, found in the page (the last one drawn), or null. Its box, for placing the line. */
function useTopLayer(): { el: HTMLElement; box: DOMRect; full: boolean } | null {
  const [layer, setLayer] = useState<{ el: HTMLElement; box: DOMRect; full: boolean } | null>(null)
  useLayoutEffect(() => {
    let raf = 0
    const read = () => {
      raf = 0
      const all = [...document.querySelectorAll<HTMLElement>(LAYERS)].filter((el) => !el.closest("[data-needs-you-layer]") && el.getAttribute("data-closed") === null && el.getAttribute("data-ending-style") === null && !el.closest("[hidden]"))
      const el = all[all.length - 1] ?? null
      setLayer((prev) => {
        if (!el) return null
        const box = el.getBoundingClientRect()
        const full = el.getAttribute("data-height") === "full"
        if (prev && prev.el === el && prev.full === full && prev.box.top === box.top && prev.box.left === box.left && prev.box.width === box.width && prev.box.height === box.height) return prev
        return { el, box, full }
      })
    }
    const schedule = () => {
      if (!raf) raf = window.requestAnimationFrame(read)
    }
    read()
    const watch = new MutationObserver(schedule)
    watch.observe(document.body, { childList: true, subtree: true, attributes: true, attributeFilter: ["data-open", "data-closed", "hidden"] })
    window.addEventListener("resize", schedule)
    return () => {
      watch.disconnect()
      window.removeEventListener("resize", schedule)
      if (raf) window.cancelAnimationFrame(raf)
    }
  }, [])
  return layer
}

/**
 * While a dialog or a sheet is open, its modal hides the page and the block
 * goes behind the scrim. A timed entry (rows 1 to 3) then shows as the
 * compact line at the top of the layer, in a portal of its own over the top
 * layer, so no dialog changes; unicron hides the block behind meanwhile, so
 * one yellow shape shows. A full-screen form on a phone is a screen, not a
 * layer over one: the strip shows at its bottom edge, for every row.
 *
 * The portal is put in only after the layer has opened (a frame later), so
 * the layer's modal treats it as something added on top, not as the page it
 * hides: a press on REVIEW does not close the layer under it.
 */
function NeedsYouOverLayer() {
  const { top, more, now } = useNeedsYouTop()
  const layer = useTopLayer()
  const [ready, setReady] = useState<HTMLElement | null>(null)
  const el = layer?.el ?? null
  useEffect(() => {
    setReady(null)
    if (!el) return
    const t = window.setTimeout(() => setReady(el), 60)
    return () => window.clearTimeout(t)
  }, [el])
  const root = useRef<HTMLDivElement>(null)
  // The layer's modal marks what was there before it as hidden; this is added after, and stays heard.
  useEffect(() => {
    const node = root.current
    if (!node) return
    const clear = () => {
      for (const a of ["aria-hidden", "inert", "data-base-ui-inert"]) if (node.hasAttribute(a)) node.removeAttribute(a)
    }
    clear()
    const watch = new MutationObserver(clear)
    watch.observe(node, { attributes: true, attributeFilter: ["aria-hidden", "inert", "data-base-ui-inert"] })
    return () => watch.disconnect()
  })
  if (!top || !layer || ready !== layer.el) return null
  const full = layer.full
  if (!full && !isTimed(top)) return null
  const title = titleOf(top, now)
  const box = layer.box
  const style: CSSProperties = full
    ? { position: "fixed", left: box.left, width: box.width, bottom: 0, zIndex: LINE_Z }
    : // Stood on the layer's top edge (the line is `size-control` tall), never above the window's.
      { position: "fixed", left: box.left, width: box.width, top: `max(0px, calc(${box.top}px - var(--size-control)))`, zIndex: LINE_Z }
  return createPortal(
    <div ref={root} data-needs-you-layer="" style={style}>
      {full ? (
        <Accessory tone="needs-you" meta={more ? `${title} // +${more}` : title} title={top.line} action={{ label: stripActionWord(top), onClick: top.action.onClick }} onOpen={top.action.onClick} openLabel={stripActionWord(top)} announce={announceOf(top, now)} />
      ) : (
        <NeedsYou size="compact" title={title} more={more || undefined} announce={false} action={actionNode(top.action, true, "sm")} />
      )}
      {!full && <Announce text={announceOf(top, now)} />}
    </div>,
    document.body,
  )
}

/** Over the layers (unicron's dialogs and sheets stack at 50). */
const LINE_Z = 60
