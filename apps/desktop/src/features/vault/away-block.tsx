import { useState, type FormEvent } from "react"

import {
  Badge,
  Button,
  Dialog,
  DialogAction,
  DialogActions,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  Field,
  FieldError,
  FieldGroup,
  FieldLabel,
  Icon,
  Input,
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemTitle,
  NativeSelect,
  NativeSelectOption,
} from "@unicron/ui"

import { useGate } from "@/app/gate"
import { describeError, useClient } from "@/core/client"
import type { AwayView } from "@/core/types"
import { whenLocal } from "@/lib/format"

import { REMIND_CHOICES } from "./depth-model"

interface LogAwayDialogProps {
  itemId: string
  title: string
  open: boolean
  onOpenChange: (open: boolean) => void
  onLogged: (notice: string) => void
}

/**
 * "Signing in on someone else's screen?" (matrix `large-type`, the hotel
 * and rental mode; D237): where (a hotel room's TV, a rental's console)
 * and when to be reminded to sign out there. Logged on the item, so every
 * device reminds; nothing about the sign-in itself leaves the vault.
 */
export function LogAwayDialog({ itemId, title, open, onOpenChange, onLogged }: LogAwayDialogProps) {
  const client = useClient()
  const gate = useGate()
  const [place, setPlace] = useState("")
  const [hours, setHours] = useState(24)
  const [error, setError] = useState<string | null>(null)

  const submit = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    setError(null)
    try {
      await gate.run((g) => client.logAway({ id: itemId, place: place.trim(), remindInHours: hours }, g))
      onOpenChange(false)
      setPlace("")
      onLogged(`Logged "${place.trim()}". Lockdown reminds you to sign out of ${title} there.`)
    } catch (err) {
      setError(describeError(err))
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <form onSubmit={submit} className="grid gap-4">
          <DialogHeader>
            <DialogTitle>Signing in on someone else's screen?</DialogTitle>
            <DialogDescription>A hotel TV or a rental's console keeps you signed in after you leave. Lockdown will remind you to sign out of {title} there.</DialogDescription>
          </DialogHeader>
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor="away-place">Where</FieldLabel>
              <Input id="away-place" value={place} onChange={(e) => setPlace(e.target.value)} placeholder="Hotel room TV" maxLength={60} required autoComplete="off" autoFocus />
            </Field>
            <Field>
              <FieldLabel htmlFor="away-remind">Remind me</FieldLabel>
              <NativeSelect id="away-remind" value={String(hours)} onChange={(e) => setHours(Number(e.target.value))}>
                {REMIND_CHOICES.map(([h, words]) => (
                  <NativeSelectOption key={h} value={String(h)}>
                    {words}
                  </NativeSelectOption>
                ))}
              </NativeSelect>
            </Field>
          </FieldGroup>
          {error && <FieldError role="alert">{error}</FieldError>}
          <DialogActions>
            <DialogAction kind="cancel" type="button" onClick={() => onOpenChange(false)}>
              Cancel
            </DialogAction>
            <DialogAction kind="confirm" type="submit">
              Remind me
            </DialogAction>
          </DialogActions>
        </form>
      </DialogContent>
    </Dialog>
  )
}

interface AwayBlockProps {
  itemId: string
  title: string
  away: AwayView[]
  /** Whether this person may change the item. */
  canEdit: boolean
  onChanged: (notice: string) => void
}

/** The item's sign-ins on someone else's screen, each with "Signed out" (D237). */
export function AwayBlock({ itemId, title, away, canEdit, onChanged }: AwayBlockProps) {
  const client = useClient()
  const gate = useGate()
  const [logging, setLogging] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const clear = async (a: AwayView) => {
    setError(null)
    try {
      await gate.run((g) => client.clearAway(itemId, a.at, g))
      onChanged(`Marked signed out of "${a.place}".`)
    } catch (e) {
      setError(describeError(e))
    }
  }

  return (
    <section aria-label="Signed in elsewhere" className="flex flex-col gap-2">
      <div className="flex items-center justify-between">
        <h3 className="text-xs font-medium text-muted-foreground uppercase">Signed in elsewhere</h3>
        {canEdit && (
          <Button variant="ghost" size="sm" onClick={() => setLogging(true)}>
            <Icon name="tv" data-icon="inline-start" />
            Signing in on a TV?
          </Button>
        )}
      </div>
      {away.length > 0 && (
        <ItemGroup>
          {away.map((a) => (
            <Item key={a.at} size="sm">
              <ItemContent>
                <ItemTitle>
                  {a.place}
                  {a.due && <Badge variant="outline">Sign out now</Badge>}
                </ItemTitle>
                <ItemDescription>
                  Signed in {whenLocal(a.at)}; {a.due ? "the reminder is due" : `reminds you ${whenLocal(a.remind_at)}`}.
                </ItemDescription>
              </ItemContent>
              {canEdit && (
                <ItemActions>
                  <Button variant="outline" size="sm" onClick={() => void clear(a)}>
                    Signed out
                  </Button>
                </ItemActions>
              )}
            </Item>
          ))}
        </ItemGroup>
      )}
      {error && (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      )}
      <LogAwayDialog itemId={itemId} title={title} open={logging} onOpenChange={setLogging} onLogged={onChanged} />
    </section>
  )
}
