import { useState } from "react"

import { ToggleRow } from "@unicron/ui"

import { useGate } from "@/app/gate"
import { usePlaces } from "@/app/places"
import { describeError, useClient } from "@/core/client"
import type { ItemDetail } from "@/core/types"

interface ExportOptInProps {
  item: ItemDetail
  /** The item as it is after the change. */
  onChanged: (item: ItemDetail) => void
  /** Kept for the callers that pass it: the switch is its own answer, so nothing toasts. */
  onNotice?: (text: string) => void
}

/**
 * A crown jewel's "Include in exports and backups" (D121), a Toggle Row
 * in Details (brief I4). Off by default: a crown jewel stays out of every
 * plain export until the person turns this on for that one item, which
 * asks first (the `Reveal` gate, the master password where one exists).
 * Turning it off asks nothing. The choice is a setting on the item, so it
 * syncs to every device. Locked backups carry every crown jewel anyway
 * while Settings, Backups, "Include crown jewels" is on (D138 as decided,
 * doc 48); off, this switch decides for them too.
 *
 * The web vault makes no exports, so there it draws nothing (Lockdown 22,
 * amending D827): a switch the web can never act on is noise.
 */
export function ExportOptIn({ item, onChanged }: ExportOptInProps) {
  const client = useClient()
  const gate = useGate()
  const { surface } = usePlaces()
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const on = item.export_opt_in === true

  if (surface === "web") return null

  const change = async (next: boolean) => {
    setBusy(true)
    setError(null)
    try {
      await gate.run((g) => client.setExportOptIn(item.id, next, g))
      onChanged(await client.show(item.id))
    } catch (e) {
      setError(describeError(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <section aria-label="Exports and backups" className="flex flex-col">
      <ToggleRow
        label="Include in exports and backups"
        description={
          on
            ? "This crown jewel leaves in every export and backup you make, protected only as well as that file is."
            : "Crown jewels stay out of plain exports unless you include them here, one by one. Locked backups carry them unless you turn that off in Settings, Backups."
        }
        checked={on}
        disabled={busy}
        onCheckedChange={(v) => void change(v)}
      />
      {error && (
        <p role="alert" className="type-caption pb-3 text-critical-text">
          {error}
        </p>
      )}
    </section>
  )
}
