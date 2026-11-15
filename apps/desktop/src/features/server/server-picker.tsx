import { useState, type FormEvent } from "react"

import { Button, RadioGroup, RadioRow, TextField } from "@unicron/ui"

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
 * type one (docs/plan/39-first-run.md §1.7, §1.9 screen J, §1.11; brief T5
 * and F3): Radio Rows under the group's overline, each saying where it was
 * found and what a check found; only a ready one can be chosen. Look again
 * and Type an address are text buttons; a typed address is a Text Field
 * with Check. A policy that locks the choice hides the field.
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
  const labelId = `${name}-label`
  return (
    <div className="flex flex-col gap-3">
      <p id={labelId} className="type-overline text-text-label">
        {label}
      </p>
      {candidates.length === 0 && !looking && <p className="type-caption text-text-secondary">No Lockdown server found on this computer or your network.</p>}
      {candidates.length > 0 && (
        <RadioGroup aria-labelledby={labelId} name={name} value={chosen ?? ""} onValueChange={(v) => onChoose(String(v))}>
          {candidates.map((c) => {
            const ready = c.state === "ready"
            const where = `${c.sources.map(foundWords).join(" · ")} · ${c.in_use ? "In use" : stateWords(c.state)}`
            return (
              <RadioRow
                key={c.address}
                value={c.address}
                disabled={!ready}
                label={hostOf(c.address)}
                description={!ready && c.reason ? `${where}. ${c.reason}` : where}
              />
            )
          })}
        </RadioGroup>
      )}
      {/* Text buttons hang their words on the column's edge, as the overline does. */}
      <div className="-ms-4 flex flex-wrap items-center gap-2">
        <Button type="button" variant="ghost" onClick={onLookAgain} loading={looking}>
          Look again
        </Button>
        {!locked && !typing && (
          <Button type="button" variant="ghost" onClick={() => setTyping(true)}>
            Type an address
          </Button>
        )}
      </div>
      {!locked && typing && (
        <form onSubmit={check} className="flex flex-col gap-3" aria-label="Type an address">
          <TextField
            id={`${name}-typed`}
            label="Server address"
            value={typed}
            onChange={(e) => setTyped(e.target.value)}
            placeholder="https://"
            autoComplete="off"
            inputMode="url"
            helper="The address of your Lockdown server. Whoever runs it can tell you."
            error={error ?? undefined}
          />
          <div>
            <Button type="submit" variant="secondary" loading={checking} disabled={typed.trim() === ""}>
              Check
            </Button>
          </div>
        </form>
      )}
    </div>
  )
}
