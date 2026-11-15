import { useId, useState, type FormEvent } from "react"

import { AdaptiveDialog, Button, ConfirmDialog, TextField } from "@unicron/ui"

import { describeError } from "@/core/client"

export interface NameAsk {
  /** The thing it is about, as the overline: `Vaults`, the vault a folder goes in, `Tags`. */
  overline?: string
  title: string
  description: string
  label: string
  /** The name as it is (a rename), or empty. */
  initial?: string
  /** The yes, which names what it does: `Create vault`, `Create folder`, `Rename`. */
  confirm: string
  hint?: string
  /** Saves the name; a rejection is shown under the field in the core's words, and the form stays. */
  onSave: (name: string) => Promise<unknown>
}

/**
 * Asks for one name (a vault, a folder, a tag): a Form with one Text Field
 * (brief X5, "Name prompts"). A dialog on a desktop, a sheet on a phone.
 * The error is the field's own, under it; the form stays open until the
 * name is saved. `null` is closed.
 */
export function NamePrompt({ ask, onClose }: { ask: NameAsk | null; onClose: () => void }) {
  const formId = useId()
  const [name, setName] = useState("")
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  // A new ask starts from its own name, with no error left from the last one.
  const [shown, setShown] = useState<NameAsk | null>(null)
  if (ask !== shown) {
    setShown(ask)
    setName(ask?.initial ?? "")
    setError(null)
    setBusy(false)
  }
  const submit = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    if (!ask || busy) return
    const typed = name.trim()
    if (!typed) {
      setError(`Type a ${ask.label.toLowerCase()}.`)
      return
    }
    setBusy(true)
    setError(null)
    try {
      await ask.onSave(typed)
      onClose()
    } catch (err) {
      setError(describeError(err))
    } finally {
      setBusy(false)
    }
  }
  return (
    <AdaptiveDialog
      open={ask !== null}
      onOpenChange={(o) => !o && !busy && onClose()}
      overline={ask?.overline}
      title={ask?.title ?? ""}
      description={ask?.description}
      // Typed words would be lost to a stray press outside; Cancel, Escape and the close button still close it.
      closeOnOutsideClick={false}
      footer={
        <>
          <Button type="button" variant="ghost" onClick={onClose} disabled={busy}>
            Cancel
          </Button>
          <Button type="submit" form={formId} loading={busy}>
            {ask?.confirm}
          </Button>
        </>
      }
    >
      <form id={formId} onSubmit={(e) => void submit(e)} noValidate>
        <TextField
          label={ask?.label}
          value={name}
          onChange={(e) => {
            setName(e.target.value)
            if (error) setError(null)
          }}
          autoComplete="off"
          spellCheck={false}
          autoFocus
          helper={ask?.hint}
          error={error ?? undefined}
        />
      </form>
    </AdaptiveDialog>
  )
}

export interface ConfirmAsk {
  /** The thing it is about, as the overline. */
  overline?: string
  title: string
  description: string
  /** The yes, which repeats the verb: `Remove folder`, `Take off`. */
  confirm: string
  destructive?: boolean
  onConfirm: () => Promise<unknown>
}

/**
 * A question before something that changes many items or can't be undone
 * (brief X5): Cancel has focus first, the yes repeats the verb, and a
 * failure stays in the question in the core's words.
 */
export function ConfirmPrompt({ ask, onClose }: { ask: ConfirmAsk | null; onClose: () => void }) {
  const [error, setError] = useState<string | null>(null)
  const [shown, setShown] = useState<ConfirmAsk | null>(null)
  if (ask !== shown) {
    setShown(ask)
    setError(null)
  }
  return (
    <ConfirmDialog
      open={ask !== null}
      onOpenChange={(o) => !o && onClose()}
      overline={ask?.overline}
      title={ask?.title ?? ""}
      description={ask?.description}
      confirmLabel={ask?.confirm ?? ""}
      tone={ask?.destructive ? "destructive" : "default"}
      error={error ?? undefined}
      onConfirm={async () => {
        if (!ask) return
        setError(null)
        try {
          await ask.onConfirm()
        } catch (e) {
          setError(describeError(e))
          throw e
        }
      }}
    />
  )
}
