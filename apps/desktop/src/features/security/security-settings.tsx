import { useState } from "react"

import { Button, Field, FieldDescription, FieldGroup, FieldLabel, Input, Switch } from "@unicron/ui"

import { useSettings } from "@/app/settings"
import { describeError } from "@/core/client"

const PUBLIC_SOURCE = "https://api.pwnedpasswords.com/range/"

/**
 * How the checks work, and the three settings behind them (settings.cbor):
 * the breach check on or off, where its list comes from (the public one,
 * a mirror, or a folder on this computer), and whether "Change password"
 * asks the site first. Plain words for what leaves the computer.
 */
export function SecuritySettings({ onSaved }: { onSaved: () => void }) {
  const { settings, update } = useSettings()
  const [source, setSource] = useState(settings.breach_endpoint ?? "")
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const save = async (change: Parameters<typeof update>[0]) => {
    setBusy(true)
    setError(null)
    try {
      await update(change)
      onSaved()
    } catch (e) {
      setError(describeError(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <section aria-labelledby="security-settings" className="flex flex-col gap-3">
      <h2 id="security-settings" className="text-base font-semibold">
        How these checks work
      </h2>
      <p className="text-sm text-muted-foreground">
        Weak and reused passwords, two-step sign-in and look-alike websites are worked out on this computer. Nothing about them leaves it.
      </p>
      <FieldGroup>
        <Field orientation="horizontal">
          <FieldLabel htmlFor="breach-check">Check my passwords against known data breaches</FieldLabel>
          <Switch id="breach-check" checked={settings.breach_check ?? true} onCheckedChange={(on) => void save({ breach_check: on })} disabled={busy} />
        </Field>
        <FieldDescription>
          Your passwords never leave this computer. Lockdown scrambles each one and sends only the first five characters of the result, which
          hundreds of thousands of other passwords share, then looks for a match in the list that comes back.
        </FieldDescription>
        <Field>
          <FieldLabel htmlFor="breach-source">Where the breach list comes from</FieldLabel>
          <div className="flex gap-2">
            <Input
              id="breach-source"
              value={source}
              placeholder={PUBLIC_SOURCE}
              onChange={(e) => setSource(e.target.value)}
              disabled={busy || !(settings.breach_check ?? true)}
            />
            <Button
              variant="outline"
              onClick={() => void save({ breach_endpoint: source.trim() ? source.trim() : null })}
              disabled={busy || (source.trim() || null) === (settings.breach_endpoint ?? null)}
            >
              Save
            </Button>
          </div>
        </Field>
        <FieldDescription>
          Leave it empty for the public Pwned Passwords list. A company or a computer that is never online can use its own copy: an https
          address, or file:/// and a folder of downloaded list files.
        </FieldDescription>
        <Field orientation="horizontal">
          <FieldLabel htmlFor="change-check">Find each site's change-password page before opening it</FieldLabel>
          <Switch
            id="change-check"
            checked={settings.change_password_check ?? true}
            onCheckedChange={(on) => void save({ change_password_check: on })}
            disabled={busy}
          />
        </Field>
        <FieldDescription>
          Lockdown asks the site itself whether it has a change-password page, the way a browser would when you visit. Nothing about your account
          is sent.
        </FieldDescription>
      </FieldGroup>
      {error && (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      )}
    </section>
  )
}
