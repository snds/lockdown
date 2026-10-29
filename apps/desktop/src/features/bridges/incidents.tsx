import { useCallback, useEffect, useState } from "react"

import {
  Button,
  Checkbox,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  Field,
  FieldLabel,
  Icon,
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemTitle,
  NativeSelect,
  NativeSelectOption,
  Spinner,
} from "@unicron/ui"

import { LabelledAction, LabelledActions } from "@/components/labelled-actions"
import { describeError } from "@/core/client"

import { useBridges, type IncidentSummary, type IncidentView, type StepView } from "./bridges-client"

interface IncidentsProps {
  onOpenItem: (id: string) => void
  /** Where a step's action goes (Devices, Recovery, Check now). */
  onAction: (action: StepView["action"]) => void
  onNotice: (text: string) => void
}

/**
 * "Something happened?" on the Security screen (docs/plan/23-bridges.md §6):
 * the seven guided flows, each opened as a step-by-step dialog with the
 * items each step is about and the person's progress, kept on this device.
 */
export function IncidentsSection({ onOpenItem, onAction, onNotice }: IncidentsProps) {
  const bridges = useBridges()
  const [list, setList] = useState<IncidentSummary[] | null>(null)
  const [open, setOpen] = useState<string | null>(null)
  const reload = useCallback(() => {
    bridges?.incidents().then(setList).catch(() => setList(null))
  }, [bridges])
  useEffect(() => reload(), [reload])
  if (!bridges || !list) return null
  return (
    <section aria-labelledby="incidents" className="flex flex-col gap-2">
      <h2 id="incidents" className="text-base font-semibold">
        Something happened?
      </h2>
      <p className="text-sm text-muted-foreground">Step-by-step help for the bigger problems, from the first thing to do to checking it worked.</p>
      <ItemGroup role="list" aria-label="Guided help">
        {list.map((i) => (
          <Item key={i.id} role="listitem" variant="outline" size="sm">
            <ItemContent>
              <ItemTitle>{i.title}</ItemTitle>
              <ItemDescription>{i.started_at ? `${i.steps_done} of ${i.steps} steps done. ${i.when}` : i.when}</ItemDescription>
            </ItemContent>
            <ItemActions>
              <Button size="sm" variant="outline" onClick={() => setOpen(i.id)}>
                {i.started_at ? "Carry on" : "Start"}
              </Button>
            </ItemActions>
          </Item>
        ))}
      </ItemGroup>
      <IncidentDialog
        key={open ?? "closed"}
        id={open}
        onClose={() => {
          setOpen(null)
          reload()
        }}
        onOpenItem={(id) => {
          setOpen(null)
          onOpenItem(id)
        }}
        onAction={(a) => {
          setOpen(null)
          onAction(a)
        }}
        onNotice={onNotice}
      />
    </section>
  )
}

const ACTION_LABELS: Partial<Record<StepView["action"], string>> = {
  sessions: "Go to Signed in",
  devices: "Go to Devices",
  recovery_kit: "Go to Recovery",
  check: "Check now",
}

function IncidentDialog({
  id,
  onClose,
  onOpenItem,
  onAction,
  onNotice,
}: {
  id: string | null
  onClose: () => void
  onOpenItem: (id: string) => void
  onAction: (a: StepView["action"]) => void
  onNotice: (text: string) => void
}) {
  const bridges = useBridges()
  const [view, setView] = useState<IncidentView | null>(null)
  const [pick, setPick] = useState("")
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    if (!id || !bridges) return
    let live = true
    bridges
      .incident(id)
      .then((v) => {
        if (!live) return
        setView(v)
        setPick(v.picked ?? "")
      })
      .catch((e) => onNotice(describeError(e)))
    return () => {
      live = false
    }
  }, [id, bridges, onNotice])

  const run = async (call: () => Promise<IncidentView>) => {
    setBusy(true)
    try {
      setView(await call())
    } catch (e) {
      onNotice(describeError(e))
    } finally {
      setBusy(false)
    }
  }
  if (!bridges) return null
  const needsPick = view !== null && view.scope !== "none" && !view.picked
  const firstOpen = view?.steps.findIndex((s) => !s.done) ?? -1
  return (
    <Dialog open={id !== null} onOpenChange={(o) => !o && onClose()}>
      <DialogContent size="lg">
        {!view && <Spinner />}
        {view && (
          <>
            <DialogHeader>
              <DialogTitle>{view.title}</DialogTitle>
              <DialogDescription>{view.when}</DialogDescription>
            </DialogHeader>
            {view.scope !== "none" && (
              <Field>
                <FieldLabel htmlFor="incident-pick">
                  {view.scope === "item" ? "Which site" : view.scope === "email" ? "Which email address" : "Which account"}
                </FieldLabel>
                <div className="flex gap-2">
                  <NativeSelect id="incident-pick" value={pick} onChange={(e) => setPick(e.target.value)} disabled={busy}>
                    <NativeSelectOption value="">Choose…</NativeSelectOption>
                    {view.choices.map((c) => (
                      <NativeSelectOption key={c.value} value={c.value}>
                        {c.label}
                      </NativeSelectOption>
                    ))}
                  </NativeSelect>
                  <Button variant="outline" disabled={busy || !pick || pick === view.picked} onClick={() => void run(() => bridges.incident(view.id, pick))}>
                    {view.picked ? "Start again with this" : "Start"}
                  </Button>
                </div>
              </Field>
            )}
            {view.finished && (
              <p role="status" className="text-sm font-medium">
                Every step is done. Well done.
              </p>
            )}
            <ol aria-label="Steps" className="flex flex-col gap-3">
              {view.steps.map((s, n) => (
                <li key={s.id} className={`flex flex-col gap-1 rounded-md border p-3 ${n === firstOpen ? "border-primary" : ""}`}>
                  <div className="flex items-start gap-2">
                    <Checkbox
                      id={`step-${s.id}`}
                      checked={s.done}
                      disabled={busy || needsPick}
                      onCheckedChange={(v) => void run(() => bridges.incidentMark(view.id, s.id, null, v === true))}
                      aria-label={`Done: ${s.title}`}
                    />
                    <div className="flex flex-col gap-1">
                      <label htmlFor={`step-${s.id}`} className="text-sm font-medium">
                        {n + 1}. {s.title}
                      </label>
                      <p className="text-sm text-muted-foreground">{s.detail}</p>
                    </div>
                  </div>
                  {s.items.length > 0 && (
                    <ul aria-label={`Items for: ${s.title}`} className="ml-7 flex flex-col gap-1">
                      {s.items.map((i) => (
                        <li key={i.item_id} className="flex items-center gap-2 text-sm">
                          <Checkbox
                            checked={i.done}
                            disabled={busy || needsPick}
                            onCheckedChange={(v) => void run(() => bridges.incidentMark(view.id, s.id, i.item_id, v === true))}
                            aria-label={`Done: ${i.title}`}
                          />
                          <span className={i.done ? "line-through text-muted-foreground" : ""}>
                            {i.title}
                            {i.host ? ` · ${i.host}` : ""}
                          </span>
                          <Button size="sm" variant="ghost" onClick={() => onOpenItem(i.item_id)}>
                            Open
                          </Button>
                        </li>
                      ))}
                    </ul>
                  )}
                  {ACTION_LABELS[s.action] && (
                    <div className="ml-7">
                      <Button size="sm" variant="outline" onClick={() => onAction(s.action)}>
                        <Icon name="arrow_forward" data-icon="inline-start" />
                        {ACTION_LABELS[s.action]}
                      </Button>
                    </div>
                  )}
                </li>
              ))}
            </ol>
            <LabelledActions>
              <Button variant="ghost" disabled={busy || !view.started_at} onClick={() => void run(() => bridges.incidentReset(view.id))}>
                Start over
              </Button>
              <LabelledAction kind="cancel" onClick={onClose}>
                Close
              </LabelledAction>
            </LabelledActions>
          </>
        )}
      </DialogContent>
    </Dialog>
  )
}
