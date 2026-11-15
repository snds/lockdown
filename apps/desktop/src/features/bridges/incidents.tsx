import { useCallback, useEffect, useRef, useState } from "react"

import {
  Badge,
  Banner,
  Button,
  Checkbox,
  EmptyState,
  ListRow,
  NativeSelect,
  NativeSelectOption,
  Progress,
  SectionHeader,
  Skeleton,
  Step,
  Steps,
} from "@unicron/ui"

import { describeError } from "@/core/client"
import { count } from "@/lib/format"

import { useBridges, type IncidentSummary, type IncidentView, type StepView } from "./bridges-client"

/** The seven flows, read once and again when a flow closes (docs/plan/23-bridges.md §6). */
export function useIncidents() {
  const bridges = useBridges()
  const [list, setList] = useState<IncidentSummary[] | null>(null)
  const [failed, setFailed] = useState<string | null>(null)
  const reload = useCallback(() => {
    bridges
      ?.incidents()
      .then((l) => {
        setList(l)
        setFailed(null)
      })
      .catch((e) => setFailed(describeError(e)))
  }, [bridges])
  useEffect(() => reload(), [reload])
  return { available: bridges !== null, list, failed, reload }
}

/** A flow's right-hand word: START, 2 OF 6 or DONE (brief H4). */
export function flowStatus(i: IncidentSummary): string {
  if (!i.started_at) return "Start"
  if (i.steps_done >= i.steps) return "Done"
  return `${i.steps_done} of ${i.steps}`
}

interface IncidentsListProps {
  list: IncidentSummary[] | null
  failed: string | null
  chosen: string | null
  onChoose: (id: string) => void
  onTryAgain: () => void
}

/** Something happened?, the list (brief H4): one row per flow, its "use this when" as the mono line. */
export function IncidentsList({ list, failed, chosen, onChoose, onTryAgain }: IncidentsListProps) {
  if (failed && !list) return <EmptyState tone="critical" overline="Something happened // not read" title="Couldn't read the guides." body={failed} actionLabel="Try again" onAction={onTryAgain} />
  if (!list)
    return (
      <div aria-busy="true" className="flex flex-col">
        <Skeleton variant="row" />
        <Skeleton variant="row" />
      </div>
    )
  return (
    <section aria-labelledby="flows" className="flex flex-col">
      <SectionHeader id="flows" title="Guided help" count={list.length} />
      <ul aria-label="Guided help" className="flex flex-col">
        {list.map((i) => (
          <ListRow key={i.id} render={<li />} metaStyle="plain" selected={chosen === i.id} title={i.title} meta={i.when} status={flowStatus(i)} onClick={() => onChoose(i.id)} />
        ))}
      </ul>
    </section>
  )
}

const ACTION_LABELS: Partial<Record<StepView["action"], string>> = {
  sessions: "Go to signed in",
  devices: "Go to devices",
  recovery_kit: "Go to recovery",
  check: "Check now",
}

interface IncidentDetailProps {
  id: string
  onOpenItem: (id: string) => void
  /** Where a step's action goes (Devices, Recovery, Check now). */
  onAction: (action: StepView["action"]) => void
  onNotice: (text: string) => void
  /** A step was ticked or the flow started over: the list's words change. */
  onProgress: () => void
}

/**
 * One flow (brief H4): `SOMETHING HAPPENED // 2 OF 6 DONE`, the flow's
 * title, one segment per step, the site, address or account it is about
 * (START AGAIN WITH THIS), then the steps, each with its checkbox, a NEXT
 * badge on the first open one, the items it is about as checkbox rows with
 * OPEN, and the one place it leads. Progress is kept on this computer.
 */
export function IncidentDetail({ id, onOpenItem, onAction, onNotice, onProgress }: IncidentDetailProps) {
  const bridges = useBridges()
  const [view, setView] = useState<IncidentView | null>(null)
  const [pick, setPick] = useState("")
  const [busy, setBusy] = useState(false)
  // The flow is read when it opens, not again when the host draws a new callback.
  const say = useRef(onNotice)
  useEffect(() => {
    say.current = onNotice
  })

  useEffect(() => {
    if (!bridges) return
    let live = true
    bridges
      .incident(id)
      .then((v) => {
        if (!live) return
        setView(v)
        setPick(v.picked ?? "")
      })
      .catch((e) => say.current(describeError(e)))
    return () => {
      live = false
    }
  }, [id, bridges])

  const run = async (call: () => Promise<IncidentView>) => {
    setBusy(true)
    try {
      setView(await call())
      onProgress()
    } catch (e) {
      onNotice(describeError(e))
    } finally {
      setBusy(false)
    }
  }
  if (!bridges) return null
  if (!view)
    return (
      <div aria-busy="true" className="flex flex-col">
        <Skeleton variant="row" />
        <Skeleton variant="row" />
        <Skeleton variant="row" />
      </div>
    )
  const needsPick = view.scope !== "none" && !view.picked
  const done = view.steps.filter((s) => s.done).length
  const firstOpen = view.steps.findIndex((s) => !s.done)
  const pickLabel = view.scope === "item" ? "Which site" : view.scope === "email" ? "Which email address" : "Which account"
  return (
    <article aria-labelledby="flow-title" className="flex flex-col gap-6">
      <header className="flex flex-col gap-2">
        <p className="type-overline text-muted-foreground">{`Something happened // ${done} of ${view.steps.length} done`}</p>
        <h2 id="flow-title" className="type-title">
          {view.title}
        </h2>
        <p className="type-body text-muted-foreground">{view.when}</p>
        <Progress variant="segmented" tone="neutral" value={done} max={view.steps.length} segments={view.steps.length} aria-label={`${done} of ${view.steps.length} steps done`} />
      </header>
      {view.finished && <Banner tone="info" title="Every step is done." />}
      {view.scope !== "none" && (
        <div className="flex flex-wrap items-end gap-2">
          <label className="flex flex-col gap-1">
            <span className="type-label text-muted-foreground">{pickLabel}</span>
            <NativeSelect value={pick} onChange={(e) => setPick(e.target.value)} disabled={busy}>
              <NativeSelectOption value="">Choose…</NativeSelectOption>
              {view.choices.map((c) => (
                <NativeSelectOption key={c.value} value={c.value}>
                  {c.label}
                </NativeSelectOption>
              ))}
            </NativeSelect>
          </label>
          <Button variant="secondary" disabled={busy || !pick || pick === view.picked} onClick={() => void run(() => bridges.incident(view.id, pick))}>
            {view.picked ? "Start again with this" : "Start"}
          </Button>
        </div>
      )}
      <Steps aria-label="Steps">
        {view.steps.map((s, n) => (
          <Step
            key={s.id}
            title={s.title}
            status={s.done ? "done" : n === firstOpen ? "current" : "upcoming"}
            statusLabel={null}
            badge={n === firstOpen ? <Badge tone="neutral">Next</Badge> : undefined}
            leading={
              <Checkbox
                checked={s.done}
                disabled={busy || needsPick}
                onCheckedChange={(v) => void run(() => bridges.incidentMark(view.id, s.id, null, v === true))}
                aria-label={`Done: ${s.title}`}
              />
            }
            action={
              ACTION_LABELS[s.action] ? (
                <Button size="sm" variant="ghost" onClick={() => onAction(s.action)}>
                  {ACTION_LABELS[s.action]}
                </Button>
              ) : undefined
            }
          >
            <p className="type-body text-muted-foreground">{s.detail}</p>
            {s.items.length > 0 && (
              <ul aria-label={`Items for: ${s.title}`} className="mt-3 flex flex-col gap-2">
                {s.items.map((i) => (
                  <li key={i.item_id} className="flex items-center gap-3">
                    <Checkbox
                      checked={i.done}
                      disabled={busy || needsPick}
                      onCheckedChange={(v) => void run(() => bridges.incidentMark(view.id, s.id, i.item_id, v === true))}
                      aria-label={`Done: ${i.title}`}
                    />
                    <span className="flex min-w-0 flex-1 flex-col">
                      <span className="type-ui">{i.title}</span>
                      {i.host && <span className="type-meta text-muted-foreground">{i.host}</span>}
                    </span>
                    <Button size="sm" variant="ghost" onClick={() => onOpenItem(i.item_id)} aria-label={`Open ${i.title}`}>
                      Open
                    </Button>
                  </li>
                ))}
              </ul>
            )}
          </Step>
        ))}
      </Steps>
      <footer className="flex flex-wrap items-center justify-between gap-2">
        <p className="type-caption text-muted-foreground">Progress is kept on this computer only.</p>
        <Button variant="ghost" disabled={busy || !view.started_at} onClick={() => void run(() => bridges.incidentReset(view.id))}>
          Start over
        </Button>
      </footer>
    </article>
  )
}

/** How many flows are under way, for the MORE HELP row's mark. */
export function flowsUnderWay(list: IncidentSummary[] | null): string | undefined {
  const n = list?.filter((i) => i.started_at && i.steps_done < i.steps).length ?? 0
  return n > 0 ? `${count(n)} under way` : undefined
}
