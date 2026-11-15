import { useState, type FormEvent } from "react"

import { AdaptiveDialog, Button, FieldRow, FieldRowAction, TextField, ToggleRow } from "@unicron/ui"

import { useSettings } from "@/app/settings"
import { describeError } from "@/core/client"
import { SaveError, useRowSize } from "@/features/settings/setting-rows"

const PUBLIC_SOURCE = "https://api.pwnedpasswords.com/range/"

/** BREACH LIST's value (brief T9): the public list, or the address set. */
export const breachListWords = (endpoint: string | null | undefined) => (endpoint ? endpoint : "The public Pwned Passwords list")

/**
 * Settings → Security checks (brief T9, doc 80; moved here from the
 * Security screen, R13): how the checks work, and the three settings
 * behind them (settings.cbor): the breach check on or off, where its list
 * comes from (CHANGE: the public one, a mirror, or a folder on this
 * computer), and whether "Change password" asks the site first. Plain
 * words for what leaves the computer. Switches save at once and silently;
 * a refused save keeps the old value with one critical line.
 */
export function SecuritySettings({ onSaved }: { onSaved: () => void }) {
  const { settings, update } = useSettings()
  const size = useRowSize()
  const [errors, setErrors] = useState<Record<string, string>>({})
  const [busy, setBusy] = useState<string | null>(null)
  const [changing, setChanging] = useState(false)

  const save = async (row: string, change: Parameters<typeof update>[0]): Promise<boolean> => {
    setBusy(row)
    setErrors(({ [row]: _gone, ...rest }) => rest)
    try {
      await update(change)
      onSaved()
      return true
    } catch (e) {
      setErrors((all) => ({ ...all, [row]: describeError(e) }))
      return false
    } finally {
      setBusy(null)
    }
  }

  const breachOn = settings.breach_check ?? true

  return (
    <div className="flex flex-col">
      <p className="type-caption text-text-secondary pb-4">
        Weak and reused passwords, two-step sign-in and look-alike websites are worked out on this computer. Nothing about them leaves it.
      </p>
      <div>
        <ToggleRow
          id="breach-check"
          label="Check my passwords against known data breaches"
          description="Your passwords never leave this computer. Lockdown scrambles each one and sends only the first five characters of the result, which hundreds of thousands of other passwords share, then looks for a match in the list that comes back."
          checked={breachOn}
          onCheckedChange={(on) => void save("breach", { breach_check: on })}
          disabled={busy === "breach"}
        />
        <SaveError text={errors.breach ?? null} />
      </div>
      <FieldRow
        size={size}
        label="Breach list"
        disabledReason={breachOn ? undefined : "The breach check is off."}
        action={<FieldRowAction onClick={() => setChanging(true)}>Change</FieldRowAction>}
      >
        {breachListWords(settings.breach_endpoint)}
      </FieldRow>
      <div>
        <ToggleRow
          id="change-check"
          label="Find each site's change-password page before opening it"
          description="Lockdown asks the site itself whether it has a change-password page, the way a browser would when you visit. Nothing about your account is sent."
          checked={settings.change_password_check ?? true}
          onCheckedChange={(on) => void save("change", { change_password_check: on })}
          disabled={busy === "change"}
        />
        <SaveError text={errors.change ?? null} />
      </div>
      <BreachListForm
        open={changing}
        onOpenChange={setChanging}
        current={settings.breach_endpoint ?? null}
        busy={busy === "source"}
        error={errors.source ?? null}
        onSave={async (value) => {
          if (await save("source", { breach_endpoint: value })) setChanging(false)
        }}
      />
    </div>
  )
}

interface BreachListFormProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  current: string | null
  busy: boolean
  error: string | null
  onSave: (value: string | null) => Promise<void>
}

/** CHANGE on BREACH LIST (brief T9): one field and today's hint, with its own Save (it holds typed text, T0). */
function BreachListForm({ open, onOpenChange, current, busy, error, onSave }: BreachListFormProps) {
  const submit = (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const typed = String(new FormData(e.currentTarget).get("breach-source") ?? "").trim()
    void onSave(typed || null)
  }
  return (
    <AdaptiveDialog open={open} onOpenChange={onOpenChange} overline="Security checks" title="Where the breach list comes from">
      <form onSubmit={submit} className="flex flex-col gap-6" aria-label="Where the breach list comes from">
        <TextField
          id="breach-source"
          name="breach-source"
          label="Breach list address"
          defaultValue={current ?? ""}
          placeholder={PUBLIC_SOURCE}
          autoComplete="off"
          spellCheck={false}
          helper="Leave it empty for the public Pwned Passwords list. A company or a computer that is never online can use its own copy: an https address, or file:/// and a folder of downloaded list files."
          error={error ?? undefined}
          disabled={busy}
        />
        <div className="flex justify-end gap-3">
          <Button type="button" variant="secondary" onClick={() => onOpenChange(false)} disabled={busy}>
            Cancel
          </Button>
          <Button type="submit" loading={busy}>
            Save list
          </Button>
        </div>
      </form>
    </AdaptiveDialog>
  )
}
