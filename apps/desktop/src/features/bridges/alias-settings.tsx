import { useCallback, useEffect, useRef, useState } from "react"

import {
  Button,
  Field,
  FieldDescription,
  FieldGroup,
  FieldLabel,
  Input,
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemTitle,
  NativeSelect,
  NativeSelectOption,
  Switch,
} from "@unicron/ui"

import { describeError, useClient } from "@/core/client"
import { encodeSecret } from "@/core/secret-bytes"

import { useBridges, type AliasOverview, type MonitoringView } from "./bridges-client"

/**
 * Settings → Email aliases (docs/plan/23-bridges.md §5, §9): the alias
 * services set up here, adding one (its API key goes into the core once,
 * as bytes, and becomes a crown jewel), and watching aliases for breaches
 * on the account's server. Hidden without a bridges client.
 */
export function AliasSettings({ onNotice, onChanged }: { onNotice: (text: string) => void; onChanged?: () => void }) {
  const bridges = useBridges()
  const client = useClient()
  const [overview, setOverview] = useState<AliasOverview | null>(null)
  const [monitoring, setMonitoring] = useState<MonitoringView | null>(null)
  const [service, setService] = useState("simple_login")
  const [domain, setDomain] = useState("")
  const [base, setBase] = useState("")
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const key = useRef<HTMLInputElement>(null)

  const reload = useCallback(() => {
    if (!bridges) return
    bridges.aliasOverview().then(setOverview).catch(() => setOverview(null))
    bridges.aliasMonitoring().then(setMonitoring).catch(() => setMonitoring(null))
  }, [bridges])
  useEffect(() => reload(), [reload])

  if (!bridges || !overview) return null
  const info = overview.services.find((s) => s.id === service) ?? overview.services[0]
  const chosen = info?.id ?? service

  const add = async () => {
    const typed = key.current?.value ?? ""
    if (!typed.trim()) {
      setError("Paste the service's API key first.")
      return
    }
    const bytes = encodeSecret(typed.trim())
    setBusy(true)
    setError(null)
    try {
      await bridges.aliasServiceAdd({ service: chosen, key: bytes, base: base.trim() || null, domain: domain.trim() || null })
      if (key.current) key.current.value = ""
      setDomain("")
      setBase("")
      onNotice(`${info?.name ?? "The service"} is set up. Its key is kept as a protected item.`)
      onChanged?.()
      reload()
    } catch (e) {
      setError(describeError(e))
    } finally {
      bytes.fill(0)
      setBusy(false)
    }
  }
  const watch = async (on: boolean) => {
    setBusy(true)
    setError(null)
    try {
      setMonitoring(await bridges.aliasMonitoringSet(on))
      onNotice(on ? "Your server now watches your aliases for breaches." : "Your server no longer watches your aliases.")
    } catch (e) {
      setError(describeError(e))
    } finally {
      setBusy(false)
    }
  }
  return (
    <section aria-labelledby="alias-settings" className="flex flex-col gap-3">
      <h2 id="alias-settings" className="text-base font-semibold">
        Email aliases
      </h2>
      <p className="text-sm text-muted-foreground">
        Give each site its own forwarding address from an alias service you use. Lockdown makes one from a login's editor or the browser's sign-up
        form.
      </p>
      {overview.configured.length > 0 && (
        <ItemGroup role="list" aria-label="Alias services">
          {overview.configured.map((c) => (
            <Item key={c.item_id} role="listitem" variant="outline" size="sm">
              <ItemContent>
                <ItemTitle>{c.service_name}</ItemTitle>
                <ItemDescription>
                  {[c.domain, c.base, c.made === 1 ? "1 alias made" : `${c.made} aliases made`].filter(Boolean).join(" · ")}
                </ItemDescription>
              </ItemContent>
              <ItemActions>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() =>
                    void client
                      .trash(c.item_id)
                      .then(() => {
                        onNotice(`${c.service_name}'s key is in Deleted.`)
                        onChanged?.()
                        reload()
                      })
                      .catch((e) => onNotice(describeError(e)))
                  }
                >
                  Remove
                </Button>
              </ItemActions>
            </Item>
          ))}
        </ItemGroup>
      )}
      <FieldGroup>
        <Field>
          <FieldLabel htmlFor="alias-new-service">Add a service</FieldLabel>
          <NativeSelect id="alias-new-service" value={chosen} onChange={(e) => setService(e.target.value)} disabled={busy}>
            {overview.services.map((s) => (
              <NativeSelectOption key={s.id} value={s.id}>
                {s.name}
              </NativeSelectOption>
            ))}
          </NativeSelect>
          {info && <FieldDescription>{info.key_hint}</FieldDescription>}
        </Field>
        <Field>
          <FieldLabel htmlFor="alias-key">API key</FieldLabel>
          <Input id="alias-key" ref={key} type="password" autoComplete="off" spellCheck={false} disabled={busy} />
        </Field>
        {info?.takes_domain && (
          <Field>
            <FieldLabel htmlFor="alias-domain">{info.needs_domain ? "Your domain" : "Your domain (optional)"}</FieldLabel>
            <Input id="alias-domain" value={domain} onChange={(e) => setDomain(e.target.value)} placeholder="example.com" disabled={busy} />
          </Field>
        )}
        {info?.self_hostable && (
          <Field>
            <FieldLabel htmlFor="alias-base">Your own server (optional)</FieldLabel>
            <Input id="alias-base" value={base} onChange={(e) => setBase(e.target.value)} placeholder="https://" disabled={busy} />
            <FieldDescription>Only if you run {info.name} yourself.</FieldDescription>
          </Field>
        )}
        <div>
          <Button variant="outline" onClick={() => void add()} disabled={busy}>
            Set it up
          </Button>
        </div>
        {monitoring && (
          <>
            <Field orientation="horizontal">
              <FieldLabel htmlFor="alias-watch">Watch my aliases for data breaches</FieldLabel>
              <Switch id="alias-watch" checked={monitoring.enabled} onCheckedChange={(on) => void watch(on)} disabled={busy || !monitoring.has_account} />
            </Field>
            <FieldDescription>
              {monitoring.has_account
                ? `Your account's server then checks ${monitoring.addresses.length === 1 ? "1 alias" : `${monitoring.addresses.length} aliases`} with its breach service, the way it checks your account's address, so it learns which aliases you have.`
                : "This needs an account: the checks run on your account's server."}
            </FieldDescription>
          </>
        )}
      </FieldGroup>
      {error && (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      )}
    </section>
  )
}
