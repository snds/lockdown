import { useState, type FormEvent } from "react"

import { Button, Field, FieldDescription, FieldError, FieldLabel, Input, Spinner } from "@unicron/ui"

import { describeError } from "@/core/client"
import { foundWords, hostOf, stateWords, type Candidate } from "@/features/unlock/first-run-client"

interface PickerProps {
  candidates: Candidate[]
  /** The chosen address, or null. */
  chosen: string | null
  onChoose: (address: string) => void
  /** Checks a typed address; the caller adds it to the list. */
  onCheck: (address: string) => Promise<void>
  onLookAgain: () => void
  looking: boolean
  /** A label for the radio group. */
  label?: string
  /** The radio group's name, unique per screen. */
  name: string
}

/**
 * The servers discovery found, as a list to pick from, with a field to
 * type one (docs/plan/39-first-run.md §1.7, §1.9 screen J, §1.11). Each row
 * says where it was found and what a check found; only a ready one can be
 * chosen. A policy that locks the choice hides the field.
 */
export function ServerPicker({ candidates, chosen, onChoose, onCheck, onLookAgain, looking, label = "Server", name }: PickerProps) {
  const [typing, setTyping] = useState(false)
  const [typed, setTyped] = useState("")
  const [error, setError] = useState<string | null>(null)
  const [checking, setChecking] = useState(false)
  const locked = candidates.some((c) => c.locked)
  const check = async (e: FormEvent) => {
    e.preventDefault()
    setChecking(true)
    setError(null)
    try {
      await onCheck(typed)
    } catch (err) {
      setError(describeError(err))
    } finally {
      setChecking(false)
    }
  }
  return (
    <div className="flex flex-col gap-(--c-welcome-gap-tight)">
      <fieldset className="flex flex-col gap-2">
        <legend className="mb-2 text-base font-medium">{label}</legend>
        {candidates.length === 0 && !looking && <p className="text-sm text-muted-foreground">No Lockdown server found on this computer or your network.</p>}
        {candidates.map((c) => {
          const ready = c.state === "ready"
          const id = `${name}-${c.address}`
          return (
            <label
              key={c.address}
              htmlFor={id}
              data-row=""
              data-ready={ready ? "true" : undefined}
              className="flex items-start gap-3 rounded-lg border px-3 has-checked:border-ring has-checked:bg-surface-selected data-[ready]:cursor-pointer"
            >
              <input
                id={id}
                type="radio"
                name={name}
                value={c.address}
                checked={chosen === c.address}
                disabled={!ready}
                onChange={() => onChoose(c.address)}
                className="mt-1 size-4 accent-primary"
              />
              <span className="flex min-w-0 flex-col">
                <span className="truncate font-medium">{hostOf(c.address)}</span>
                <span className="text-sm text-muted-foreground">
                  {c.sources.map(foundWords).join(" · ")} · {c.in_use ? "In use" : stateWords(c.state)}
                </span>
                {!ready && c.reason && <span className="text-sm text-muted-foreground">{c.reason}</span>}
              </span>
            </label>
          )
        })}
      </fieldset>
      <div className="flex flex-wrap items-center gap-3">
        <Button type="button" variant="link" size="inline" onClick={onLookAgain} disabled={looking}>
          {looking ? "Looking…" : "Look again"}
        </Button>
        {looking && <Spinner />}
        {!locked && !typing && (
          <Button type="button" variant="link" size="inline" onClick={() => setTyping(true)}>
            Type an address
          </Button>
        )}
      </div>
      {!locked && typing && (
        <form onSubmit={check} className="flex flex-col gap-2">
          <Field data-invalid={error ? true : undefined}>
            <FieldLabel htmlFor={`${name}-typed`}>Server address</FieldLabel>
            <div className="flex gap-2">
              <div className="flex-1">
                <Input id={`${name}-typed`} value={typed} onChange={(e) => setTyped(e.target.value)} placeholder="https://" autoComplete="off" inputMode="url" />
              </div>
              <Button type="submit" variant="outline" disabled={checking || typed.trim() === ""}>
                {checking ? <Spinner data-icon="inline-start" /> : null}
                Check
              </Button>
            </div>
            <FieldDescription>The address of your Lockdown server. Whoever runs it can tell you.</FieldDescription>
            {error && <FieldError role="alert">{error}</FieldError>}
          </Field>
        </form>
      )}
    </div>
  )
}
