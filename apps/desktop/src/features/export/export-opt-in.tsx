import { useState } from "react"

import { Field, FieldContent, FieldDescription, FieldError, FieldLabel, Switch } from "@unicron/ui"

import { useGate } from "@/app/gate"
import { describeError, useClient } from "@/core/client"
import type { ItemDetail } from "@/core/types"

interface ExportOptInProps {
  item: ItemDetail
  /** The item as it is after the change. */
  onChanged: (item: ItemDetail) => void
  onNotice: (text: string) => void
}

/**
 * A crown jewel's "Include in exports and backups" (D121). Off by default:
 * a crown jewel stays out of every bulk export and backup until the person
 * turns this on for that one item, which asks first (the `Reveal` gate, the
 * master password where one exists). Turning it off asks nothing. The
 * choice is a setting on the item, so it syncs to every device.
 */
export function ExportOptIn({ item, onChanged, onNotice }: ExportOptInProps) {
  const client = useClient()
  const gate = useGate()
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const on = item.export_opt_in === true

  const change = async (next: boolean) => {
    setBusy(true)
    setError(null)
    try {
      await gate.run((g) => client.setExportOptIn(item.id, next, g))
      onChanged(await client.show(item.id))
      onNotice(next ? "This item will be included in exports and backups." : "This item stays out of exports and backups.")
    } catch (e) {
      setError(describeError(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <section aria-label="Exports and backups" className="flex flex-col gap-2 rounded-xl border p-3">
      <Field orientation="horizontal">
        <FieldContent>
          <FieldLabel htmlFor={`export-opt-in-${item.id}`}>Include in exports and backups</FieldLabel>
          <FieldDescription>
            {on
              ? "This crown jewel leaves in every export and backup you make, protected only as well as that file is."
              : "Crown jewels stay out of exports and backups unless you choose to include them, one by one."}
          </FieldDescription>
        </FieldContent>
        <Switch id={`export-opt-in-${item.id}`} checked={on} onCheckedChange={(v) => void change(v)} disabled={busy} />
      </Field>
      {error && <FieldError role="alert">{error}</FieldError>}
    </section>
  )
}
