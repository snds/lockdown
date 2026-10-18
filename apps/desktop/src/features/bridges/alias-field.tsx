import { useEffect, useState } from "react"

import { Button, Field, FieldDescription, FieldTitle, Icon, NativeSelect, NativeSelectOption, Spinner } from "@unicron/ui"

import { useGate } from "@/app/gate"
import { describeError } from "@/core/client"

import { useBridges, type AliasConfigured } from "./bridges-client"
import { siteHost } from "./bridges-model"

/**
 * "Hide your email" in a Login's editor (docs/plan/23-bridges.md §5): a new
 * alias from one of the person's alias services, put into the user name.
 * Making one asks for the `Use` gate (the service's key is a crown jewel);
 * only the address comes back. Hidden when no service is set up or the
 * host has no bridges client.
 */
export function AliasField({ site, onAddress }: { site: string; onAddress: (address: string) => void }) {
  const bridges = useBridges()
  const gate = useGate()
  const [services, setServices] = useState<AliasConfigured[]>([])
  const [pick, setPick] = useState("")
  const [busy, setBusy] = useState(false)
  const [note, setNote] = useState<string | null>(null)

  useEffect(() => {
    let live = true
    bridges
      ?.aliasOverview()
      .then((o) => {
        if (!live) return
        setServices(o.configured)
        setPick(o.configured[0]?.item_id ?? "")
      })
      .catch(() => live && setServices([]))
    return () => {
      live = false
    }
  }, [bridges])

  if (!bridges || services.length === 0) return null
  const make = async () => {
    setBusy(true)
    setNote(null)
    try {
      const made = await gate.run((g) => bridges.aliasGenerate(pick, siteHost(site), g))
      onAddress(made.address)
      setNote(
        `A new ${made.service_name} alias is in the user name: mail to it reaches you.${made.monitoring_problem ? ` ${made.monitoring_problem}` : ""}`,
      )
    } catch (e) {
      setNote(describeError(e))
    } finally {
      setBusy(false)
    }
  }
  return (
    <Field>
      <FieldTitle>Hide your email address</FieldTitle>
      <div className="flex gap-2">
        {services.length > 1 && (
          <NativeSelect aria-label="Alias service" value={pick} onChange={(e) => setPick(e.target.value)} disabled={busy}>
            {services.map((s) => (
              <NativeSelectOption key={s.item_id} value={s.item_id}>
                {s.service_name}
                {s.domain ? ` (${s.domain})` : ""}
              </NativeSelectOption>
            ))}
          </NativeSelect>
        )}
        <Button type="button" variant="outline" size="sm" onClick={() => void make()} disabled={busy}>
          {busy ? <Spinner data-icon="inline-start" /> : <Icon name="alternate_email" data-icon="inline-start" />}
          New alias{services.length === 1 ? ` from ${services[0]?.service_name}` : ""}
        </Button>
      </div>
      <FieldDescription>{note ?? "Makes a forwarding address for this site, so the site never sees your real one."}</FieldDescription>
    </Field>
  )
}
