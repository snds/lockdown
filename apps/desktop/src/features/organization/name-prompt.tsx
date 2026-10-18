import { useState, type FormEvent } from "react"

import {
  Dialog,
  DialogAction,
  DialogActions,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  Field,
  FieldDescription,
  FieldError,
  FieldLabel,
  Input,
} from "@unicron/ui"

import { describeError } from "@/core/client"

export interface NameAsk {
  title: string
  description: string
  label: string
  /** The name as it is (a rename), or empty. */
  initial?: string
  confirm: string
  hint?: string
  /** Saves the name; a rejection is shown under the field and the dialog stays. */
  onSave: (name: string) => Promise<unknown>
}

/** Asks for one name (a vault, a folder, a tag) in a small dialog. `null` is closed. */
export function NamePrompt({ ask, onClose }: { ask: NameAsk | null; onClose: () => void }) {
  return (
    <Dialog open={ask !== null} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>{ask && <NameForm key={`${ask.title}:${ask.initial ?? ""}`} ask={ask} onClose={onClose} />}</DialogContent>
    </Dialog>
  )
}

function NameForm({ ask, onClose }: { ask: NameAsk; onClose: () => void }) {
  const [name, setName] = useState(ask.initial ?? "")
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const submit = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    setBusy(true)
    setError(null)
    try {
      await ask.onSave(name.trim())
      onClose()
    } catch (err) {
      setError(describeError(err))
    } finally {
      setBusy(false)
    }
  }
  return (
    <form onSubmit={submit} className="grid gap-4">
      <DialogHeader>
        <DialogTitle>{ask.title}</DialogTitle>
        <DialogDescription>{ask.description}</DialogDescription>
      </DialogHeader>
      <Field>
        <FieldLabel htmlFor="name-prompt">{ask.label}</FieldLabel>
        <Input id="name-prompt" value={name} onChange={(e) => setName(e.target.value)} autoComplete="off" autoFocus required />
        {ask.hint && <FieldDescription>{ask.hint}</FieldDescription>}
      </Field>
      {error && <FieldError role="alert">{error}</FieldError>}
      <DialogActions>
        <DialogAction kind="cancel" type="button" onClick={onClose} disabled={busy}>
          Cancel
        </DialogAction>
        <DialogAction kind="confirm" type="submit" disabled={busy || !name.trim()}>
          {ask.confirm}
        </DialogAction>
      </DialogActions>
    </form>
  )
}

export interface ConfirmAsk {
  title: string
  description: string
  confirm: string
  destructive?: boolean
  onConfirm: () => Promise<unknown>
}

/** A yes-or-no question before something that changes many items. */
export function ConfirmPrompt({ ask, onClose }: { ask: ConfirmAsk | null; onClose: () => void }) {
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const go = async () => {
    if (!ask) return
    setBusy(true)
    setError(null)
    try {
      await ask.onConfirm()
      onClose()
    } catch (e) {
      setError(describeError(e))
    } finally {
      setBusy(false)
    }
  }
  return (
    <Dialog
      open={ask !== null}
      onOpenChange={(o) => {
        if (!o) {
          setError(null)
          onClose()
        }
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{ask?.title}</DialogTitle>
          <DialogDescription>{ask?.description}</DialogDescription>
        </DialogHeader>
        {error && <FieldError role="alert">{error}</FieldError>}
        <DialogActions>
          <DialogAction kind="cancel" onClick={onClose} disabled={busy}>
            Cancel
          </DialogAction>
          <DialogAction kind={ask?.destructive ? "destructive" : "confirm"} onClick={() => void go()} disabled={busy}>
            {ask?.confirm}
          </DialogAction>
        </DialogActions>
      </DialogContent>
    </Dialog>
  )
}
