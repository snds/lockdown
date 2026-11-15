import { useId, useState, type FormEvent } from "react"

import { AdaptiveDialog, Badge, Button, Field, FieldLabel, FieldRow, FieldRowAction, FieldRowValue, NativeSelect, NativeSelectOption, TextField } from "@unicron/ui"

import { useGate } from "@/app/gate"
import { describeError, useClient } from "@/core/client"
import type { AwayView } from "@/core/types"
import { fullDate } from "@/lib/format"

import { REMIND_CHOICES } from "./depth-model"

interface LogAwayDialogProps {
  itemId: string
  title: string
  open: boolean
  onOpenChange: (open: boolean) => void
  onLogged: (notice: string) => void
}

/**
 * "Signing in on a TV?" (matrix `large-type`, the hotel and rental mode;
 * D237; brief I4's third small Form): where (a hotel room's TV, a rental's
 * console) and when to be reminded to sign out there, and one primary
 * REMIND ME. Logged on the item, so every device reminds; nothing about the
 * sign-in itself leaves the vault. A dialog on a desktop, a sheet on a
 * phone; a failure stays in the form in the core's words.
 */
export function LogAwayDialog({ itemId, title, open, onOpenChange, onLogged }: LogAwayDialogProps) {
  const client = useClient()
  const gate = useGate()
  const formId = useId()
  const [place, setPlace] = useState("")
  const [hours, setHours] = useState(24)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const submit = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const where = place.trim()
    if (!where) {
      setError("Say where you signed in.")
      return
    }
    setError(null)
    setBusy(true)
    try {
      await gate.run((g) => client.logAway({ id: itemId, place: where, remindInHours: hours }, g))
      onOpenChange(false)
      setPlace("")
      onLogged(`Lockdown reminds you to sign out of ${title} on ${where}`)
    } catch (err) {
      setError(describeError(err))
    } finally {
      setBusy(false)
    }
  }

  return (
    <AdaptiveDialog
      open={open}
      onOpenChange={(o) => !busy && onOpenChange(o)}
      overline={title}
      title="Signing in on a TV?"
      description={`A hotel TV or a rental's console keeps you signed in after you leave. Lockdown will remind you to sign out of ${title} there.`}
      closeOnOutsideClick={false}
      footer={
        <>
          <Button type="button" variant="ghost" onClick={() => onOpenChange(false)} disabled={busy}>
            Cancel
          </Button>
          <Button type="submit" form={formId} loading={busy}>
            Remind me
          </Button>
        </>
      }
    >
      <form id={formId} onSubmit={(e) => void submit(e)} noValidate className="flex flex-col gap-6">
        <TextField
          label="Where"
          value={place}
          onChange={(e) => {
            setPlace(e.target.value)
            if (error) setError(null)
          }}
          placeholder="Hotel room TV"
          maxLength={60}
          autoComplete="off"
          autoFocus
          error={error ?? undefined}
        />
        <Field>
          <FieldLabel htmlFor={`${formId}-remind`}>Remind me</FieldLabel>
          <NativeSelect id={`${formId}-remind`} value={String(hours)} onChange={(e) => setHours(Number(e.target.value))}>
            {REMIND_CHOICES.map(([h, words]) => (
              <NativeSelectOption key={h} value={String(h)}>
                {words}
              </NativeSelectOption>
            ))}
          </NativeSelect>
        </Field>
      </form>
    </AdaptiveDialog>
  )
}

interface AwayBlockProps {
  itemId: string
  title: string
  away: AwayView[]
  /** Whether this person may change the item. */
  canEdit: boolean
  onChanged: (notice: string) => void
  /** Field Row's layout: `stacked` on a phone and in a pane under 480 px (brief 4.0). */
  size?: "default" | "stacked"
}

/**
 * SIGNED IN ELSEWHERE (D237, brief I4): one Field Row per sign-in on
 * someone else's screen, "Hotel room TV // reminds 2026.11.20" with the
 * DUE badge once its reminder is due, and SIGNED OUT. With none, one row
 * says "Nowhere" with ADD, which opens "Signing in on a TV?" (the item's
 * ⋯ menu opens it too, also once something is logged).
 */
export function AwayBlock({ itemId, title, away, canEdit, onChanged, size = "default" }: AwayBlockProps) {
  const client = useClient()
  const gate = useGate()
  const [logging, setLogging] = useState(false)
  const [clearing, setClearing] = useState<number | null>(null)
  const [error, setError] = useState<string | null>(null)

  const clear = async (a: AwayView) => {
    setError(null)
    setClearing(a.at)
    try {
      await gate.run((g) => client.clearAway(itemId, a.at, g))
      onChanged(`Marked signed out of ${title} on ${a.place}`)
    } catch (e) {
      setError(describeError(e))
    } finally {
      setClearing(null)
    }
  }

  if (!away.length && !canEdit) return null
  return (
    <section aria-label="Signed in elsewhere" className="flex flex-col">
      {away.length === 0 ? (
        <FieldRow size={size} label="Signed in elsewhere" action={<FieldRowAction onClick={() => setLogging(true)}>Add</FieldRowAction>}>
          Nowhere
        </FieldRow>
      ) : (
        away.map((a) => (
          <FieldRow
            key={a.at}
            size={size}
            label="Signed in elsewhere"
            badge={a.due ? <Badge tone="warning">Due</Badge> : undefined}
            action={
              canEdit ? (
                <FieldRowAction loading={clearing === a.at} onClick={() => void clear(a)}>
                  Signed out
                </FieldRowAction>
              ) : undefined
            }
          >
            <FieldRowValue truncate="wrap">{`${a.place} // reminds ${fullDate(a.remind_at)}`}</FieldRowValue>
          </FieldRow>
        ))
      )}
      {error && (
        <p role="alert" className="type-caption pb-3 text-critical-text">
          {error}
        </p>
      )}
      <LogAwayDialog itemId={itemId} title={title} open={logging} onOpenChange={setLogging} onLogged={onChanged} />
    </section>
  )
}
