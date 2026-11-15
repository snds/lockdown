import { useCallback, useEffect, useState, type FormEvent } from "react"

import {
  AdaptiveDialog,
  Button,
  ConfirmDialog,
  Field,
  FieldLabel,
  FieldRow,
  FieldRowAction,
  NativeSelect,
  NativeSelectOption,
  SectionHeader,
  TextField,
  ToggleRow,
} from "@unicron/ui"
import { SecretField } from "@unicron/ui/security/secret-field"

import { describeError, useClient } from "@/core/client"
import { encodeSecret } from "@/core/secret-bytes"
import { SaveError, useGroupLevel, useRowSize } from "@/features/settings/setting-rows"

import { useBridges, type AliasConfigured, type AliasOverview, type BridgesClient, type MonitoringView } from "./bridges-client"

/** A service's row value: how many aliases Lockdown made through it. */
export const madeWords = (made: number) => (made === 1 ? "1 alias made" : `${made} aliases made`)

/**
 * Settings → Email aliases (brief T9, doc 80; docs/plan/23-bridges.md §5,
 * §9): the alias services set up here, each with REMOVE behind a Question
 * (its key goes to Recently deleted); ADD SERVICE, a form whose API key goes
 * into the core once, as bytes, and becomes a crown jewel; and watching the
 * aliases for breaches on the account's server, disabled with its reason
 * when there is no account (#37). Hidden without a bridges client (the web
 * vault).
 */
export function AliasSettings({ onNotice, onChanged }: { onNotice: (text: string) => void; onChanged?: () => void }) {
  const bridges = useBridges()
  const client = useClient()
  const level = useGroupLevel()
  const size = useRowSize()
  const [overview, setOverview] = useState<AliasOverview | null>(null)
  const [monitoring, setMonitoring] = useState<MonitoringView | null>(null)
  const [adding, setAdding] = useState(false)
  const [removing, setRemoving] = useState<AliasConfigured | null>(null)
  const [removeError, setRemoveError] = useState<string | null>(null)
  const [watchError, setWatchError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const reload = useCallback(() => {
    if (!bridges) return
    bridges.aliasOverview().then(setOverview).catch(() => setOverview(null))
    bridges.aliasMonitoring().then(setMonitoring).catch(() => setMonitoring(null))
  }, [bridges])
  useEffect(() => reload(), [reload])

  if (!bridges || !overview) return null

  const remove = async (c: AliasConfigured) => {
    setRemoveError(null)
    try {
      await client.trash(c.item_id)
      onNotice(`${c.service_name}'s key moved to Recently deleted`)
      onChanged?.()
      reload()
    } catch (e) {
      setRemoveError(describeError(e))
      throw e
    }
  }
  // A switch saves silently (brief T0); a refusal keeps the old state with one critical line.
  const watch = async (on: boolean) => {
    setBusy(true)
    setWatchError(null)
    try {
      setMonitoring(await bridges.aliasMonitoringSet(on))
    } catch (e) {
      setWatchError(describeError(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="flex flex-col gap-8">
      <p className="type-caption text-text-secondary">Give each site its own forwarding address from an alias service you use.</p>
      <div className="flex flex-col">
        <SectionHeader level={level} title="Services" count={overview.configured.length} />
        {overview.configured.length === 0 && <p className="type-caption text-text-secondary py-3">None yet. A login's editor and the browser's sign-up form make aliases once one is set up.</p>}
        {overview.configured.map((c) => (
          <FieldRow
            key={c.item_id}
            size={size}
            label={c.service_name}
            description={[c.domain, c.base].filter(Boolean).join(" // ") || undefined}
            action={
              <FieldRowAction
                onClick={() => {
                  setRemoveError(null)
                  setRemoving(c)
                }}
              >
                Remove
              </FieldRowAction>
            }
          >
            {madeWords(c.made)}
          </FieldRow>
        ))}
        <div className="pt-4">
          <Button variant="secondary" size="sm" onClick={() => setAdding(true)}>
            Add service
          </Button>
        </div>
      </div>
      {monitoring && (
        <div className="flex flex-col">
          <ToggleRow
            label="Watch my aliases for data breaches"
            // Without an account the reason stands alone: the row dims its description, below the contrast floor (unicron gap 4, doc 80).
            description={
              monitoring.has_account
                ? `Your account's server then checks ${monitoring.addresses.length === 1 ? "1 alias" : `${monitoring.addresses.length} aliases`} with its breach service, the way it checks your account's address, so it learns which aliases you have.`
                : undefined
            }
            disabledReason={monitoring.has_account ? undefined : "This needs an account."}
            checked={monitoring.enabled}
            disabled={busy}
            onCheckedChange={(on) => void watch(on)}
          />
          <SaveError text={watchError} />
        </div>
      )}
      <AddService
        open={adding}
        onOpenChange={setAdding}
        bridges={bridges}
        overview={overview}
        onAdded={(name) => {
          onNotice(`${name}'s key is kept as a protected item`)
          onChanged?.()
          reload()
        }}
      />
      <ConfirmDialog
        open={removing !== null}
        onOpenChange={(o) => !o && setRemoving(null)}
        overline={removing?.service_name}
        title={`Remove ${removing?.service_name ?? "this service"}?`}
        description="Its key goes to Recently deleted. Lockdown makes no new aliases through it."
        confirmLabel="Remove"
        tone="destructive"
        error={removeError}
        onConfirm={() => (removing ? remove(removing) : undefined)}
      />
    </div>
  )
}

interface AddServiceProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  bridges: BridgesClient
  overview: AliasOverview
  onAdded: (name: string) => void
}

/**
 * ADD SERVICE (brief T9): which service, its API key in a SecretField,
 * "Your domain" only where the service takes one and "Your own server" only
 * where it can be self-hosted. The key leaves as bytes that are wiped after
 * the call, and the field is emptied when the form closes.
 */
function AddService({ open, onOpenChange, bridges, overview, onAdded }: AddServiceProps) {
  const [service, setService] = useState(overview.services[0]?.id ?? "")
  const [key, setKey] = useState("")
  const [domain, setDomain] = useState("")
  const [base, setBase] = useState("")
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const info = overview.services.find((s) => s.id === service) ?? overview.services[0]

  const close = (next: boolean) => {
    if (!next) {
      setKey("")
      setDomain("")
      setBase("")
      setError(null)
    }
    onOpenChange(next)
  }

  const submit = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    if (!info) return
    if (!key.trim()) {
      setError("Paste the service's API key first.")
      return
    }
    const bytes = encodeSecret(key.trim())
    setBusy(true)
    setError(null)
    try {
      await bridges.aliasServiceAdd({ service: info.id, key: bytes, base: base.trim() || null, domain: domain.trim() || null })
      onAdded(info.name)
      close(false)
    } catch (err) {
      setError(describeError(err))
    } finally {
      bytes.fill(0)
      setBusy(false)
    }
  }

  return (
    <AdaptiveDialog
      open={open}
      onOpenChange={close}
      overline="Email aliases"
      title="Add a service"
      description="The key is kept in your vault as a protected item."
      closeOnOutsideClick={false}
    >
      <form onSubmit={(e) => void submit(e)} className="flex flex-col gap-6" aria-label="Add a service">
        <Field>
          <FieldLabel htmlFor="alias-new-service">Service</FieldLabel>
          <NativeSelect id="alias-new-service" value={info?.id ?? ""} onChange={(e) => setService(e.target.value)} disabled={busy}>
            {overview.services.map((s) => (
              <NativeSelectOption key={s.id} value={s.id}>
                {s.name}
              </NativeSelectOption>
            ))}
          </NativeSelect>
        </Field>
        <SecretField
          mode="edit"
          id="alias-key"
          label="API key"
          name="api-key"
          mono
          value={key}
          onChange={setKey}
          autoComplete="off"
          disabled={busy}
          description={info?.key_hint}
          error={error ?? undefined}
        />
        {info?.takes_domain && (
          <TextField label={info.needs_domain ? "Your domain" : "Your domain (optional)"} value={domain} onChange={(e) => setDomain(e.target.value)} placeholder="example.com" disabled={busy} />
        )}
        {info?.self_hostable && (
          <TextField label="Your own server (optional)" value={base} onChange={(e) => setBase(e.target.value)} placeholder="https://" helper={`Only if you run ${info.name} yourself.`} disabled={busy} />
        )}
        <div className="flex justify-end gap-3">
          <Button type="button" variant="secondary" onClick={() => close(false)} disabled={busy}>
            Cancel
          </Button>
          <Button type="submit" loading={busy}>
            Set it up
          </Button>
        </div>
      </form>
    </AdaptiveDialog>
  )
}
