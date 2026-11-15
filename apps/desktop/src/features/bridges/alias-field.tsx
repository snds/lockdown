import { useEffect, useState } from "react"

import { AdaptiveMenu, AdaptiveMenuItem, Button } from "@unicron/ui"

import { useGate } from "@/app/gate"
import { describeError } from "@/core/client"
import { notify } from "@/app/notify"

import { useBridges, type AliasConfigured } from "./bridges-client"
import { siteHost } from "./bridges-model"

/**
 * "Hide your email" in a Login's editor (docs/plan/23-bridges.md §5; brief
 * E2): NEW ALIAS, a text action at the end of the Username field, makes a
 * new alias from one of the person's alias services and puts it in the
 * user name. With more than one service set up it is a menu of them.
 * Making one asks for the `Use` gate (the service's key is a crown jewel);
 * only the address comes back. Nothing is drawn when no service is set up
 * or the host has no bridges client (the web vault). What happened goes
 * to a toast; a failure is the Username field's error line (`onError`).
 */
export function AliasField({ site, onAddress, onError = () => {} }: { site: string; onAddress: (address: string) => void; onError?: (text: string | null) => void }) {
  const bridges = useBridges()
  const gate = useGate()
  const [services, setServices] = useState<AliasConfigured[]>([])
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    let live = true
    bridges
      ?.aliasOverview()
      .then((o) => live && setServices(o.configured))
      .catch(() => live && setServices([]))
    return () => {
      live = false
    }
  }, [bridges])

  if (!bridges || services.length === 0) return null
  const make = async (pick: AliasConfigured) => {
    setBusy(true)
    onError(null)
    try {
      const made = await gate.run((g) => bridges.aliasGenerate(pick.item_id, siteHost(site), g))
      onAddress(made.address)
      notify(made.monitoring_problem ? `New ${made.service_name} alias in the user name: ${made.monitoring_problem}` : `New ${made.service_name} alias in the user name`)
    } catch (e) {
      // A failed action is a line beside its control, in the core's words, never a toast (brief 4.0).
      onError(describeError(e))
    } finally {
      setBusy(false)
    }
  }
  const only = services.length === 1 ? services[0] : undefined
  if (only)
    return (
      <Button type="button" variant="ghost" size="sm" loading={busy} aria-label={`New alias from ${only.service_name}`} onClick={() => void make(only)}>
        New alias
      </Button>
    )
  return (
    <AdaptiveMenu
      title="New alias from"
      overline="Hide your email"
      trigger={
        <Button type="button" variant="ghost" size="sm" loading={busy}>
          New alias
        </Button>
      }
    >
      {services.map((s) => (
        <AdaptiveMenuItem key={s.item_id} icon="at" onSelect={() => void make(s)}>
          {s.domain ? `${s.service_name} (${s.domain})` : s.service_name}
        </AdaptiveMenuItem>
      ))}
    </AdaptiveMenu>
  )
}
