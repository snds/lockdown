import { useCallback, useEffect, useState } from "react"

import { Banner, Button, EmptyState, FieldRow, FieldRowAction, ListRow, NativeSelect, NativeSelectOption, SectionHeader, Skeleton, useAppFrame } from "@unicron/ui"

import { describeError } from "@/core/client"
import { count, countName } from "@/lib/format"

import { useBridges, type BridgesClient, type FederationMap, type MapLogin, type PendingView, type ProviderView } from "./bridges-client"

/** The map, read once and again after each answer, and the three calls that change it (docs/plan/23-bridges.md §4). */
export function useFederation(onChanged: () => void, onNotice: (text: string) => void) {
  const bridges = useBridges()
  const [map, setMap] = useState<FederationMap | null>(null)
  const [failed, setFailed] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const reload = useCallback(() => {
    bridges
      ?.federationMap()
      .then((m) => {
        setMap(m)
        setFailed(null)
      })
      .catch((e) => setFailed(describeError(e)))
  }, [bridges])
  useEffect(() => reload(), [reload])
  const run = useCallback(
    async (call: (b: BridgesClient) => Promise<unknown>, notice: string) => {
      if (!bridges) return
      setBusy(true)
      try {
        await call(bridges)
        onNotice(notice)
        onChanged()
        reload()
      } catch (e) {
        onNotice(describeError(e))
      } finally {
        setBusy(false)
      }
    },
    [bridges, onChanged, onNotice, reload],
  )
  return { available: bridges !== null, map, failed, busy, run, reload }
}

/** The words a provider's row ends with: the core's first warning, as a word (brief H3). */
function warningWord(p: ProviderView): string | undefined {
  if (!p.account) return "No login saved"
  if (p.warnings.some((w) => /two-step|second step/i.test(w))) return "No second step"
  return p.warnings.length > 0 ? "Check" : undefined
}

interface AccountsListProps {
  map: FederationMap | null
  failed: string | null
  busy: boolean
  chosen: string | null
  onChoose: (providerId: string) => void
  onRecord: (p: PendingView, item: string | null) => void
  onForget: (p: PendingView) => void
  onTryAgain: () => void
}

/**
 * Accounts you sign in with, the list (brief H3): a caption on what falls
 * with an account, the sign-ins the browser extension saw that wait for a
 * yes (DID YOU SIGN IN?), then one row per provider with how many sites
 * hang on it and a warning word from the core.
 */
export function AccountsList({ map, failed, busy, chosen, onChoose, onRecord, onForget, onTryAgain }: AccountsListProps) {
  if (failed && !map) return <EmptyState tone="critical" overline="Accounts // not read" title="Couldn't read your accounts." body={failed} actionLabel="Try again" onAction={onTryAgain} />
  if (!map)
    return (
      <div aria-busy="true" className="flex flex-col">
        <Skeleton variant="row" />
        <Skeleton variant="row" />
      </div>
    )
  if (map.providers.length === 0 && map.pending.length === 0)
    return (
      <EmptyState
        overline={`Accounts // ${count(0)}`}
        title="No accounts yet."
        body="When you sign in to a site with Google, Apple or Microsoft, the browser extension lists it here. You can also set Signed in with on a login."
      />
    )
  return (
    <div className="flex flex-col gap-6">
      <p className="type-caption text-muted-foreground">If one of these accounts is lost or taken over, every site under it goes with it.</p>
      {map.pending.length > 0 && (
        <section aria-labelledby="did-you-sign-in" className="flex flex-col">
          <SectionHeader id="did-you-sign-in" title="Did you sign in?" count={map.pending.length} />
          <ul aria-label="Sign-ins to confirm" className="flex flex-col">
            {map.pending.map((p) => (
              <PendingRow key={p.id} pending={p} busy={busy} onRecord={(item) => onRecord(p, item)} onForget={() => onForget(p)} />
            ))}
          </ul>
        </section>
      )}
      {map.providers.length > 0 && (
        <section aria-labelledby="accounts" className="flex flex-col">
          <SectionHeader id="accounts" title="Accounts" count={map.providers.length} />
          <ul aria-label="Accounts" className="flex flex-col">
            {map.providers.map((p) => (
              <ListRow
                key={p.id}
                render={<li />}
                metaStyle="plain"
                selected={chosen === p.id}
                title={p.name}
                meta={countName(p.sites.length, "site")}
                status={warningWord(p)}
                onClick={() => onChoose(p.id)}
              />
            ))}
          </ul>
        </section>
      )}
    </div>
  )
}

/** One sign-in the extension saw: Did you sign in to {host} with {provider}? YES, NOTE IT (or YES, SAVE LOGIN) and NO. */
function PendingRow({ pending: p, busy, onRecord, onForget }: { pending: PendingView; busy: boolean; onRecord: (item: string | null) => void; onForget: () => void }) {
  const [item, setItem] = useState(p.logins[0]?.item_id ?? "")
  return (
    <li className="flex flex-col gap-3 px-4 py-4 rule-t">
      <div className="flex flex-col gap-1">
        <p className="type-subtitle">
          Did you sign in to {p.host} with {p.provider_name}?
        </p>
        <p className="type-meta text-muted-foreground">{p.confirmed ? `${p.provider_name}'s own sign-in page sent you back to this site` : "The browser extension saw the button pressed"}</p>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        {p.logins.length > 1 && (
          <NativeSelect aria-label="Which login" value={item} onChange={(e) => setItem(e.target.value)}>
            {p.logins.map((l) => (
              <NativeSelectOption key={l.item_id} value={l.item_id}>
                {l.title}
              </NativeSelectOption>
            ))}
          </NativeSelect>
        )}
        <Button size="sm" variant="ghost" disabled={busy} onClick={() => onRecord(p.logins.length > 0 ? item : null)}>
          {p.logins.length > 0 ? "Yes, note it" : "Yes, save a login"}
        </Button>
        <Button size="sm" variant="ghost" disabled={busy} onClick={onForget}>
          No
        </Button>
      </div>
    </li>
  )
}

/** A site under a provider, as its row's mono line. */
function siteLine(l: MapLogin): string {
  return [l.host && l.host !== l.title ? l.host : null, l.has_password ? null : "no password of its own"].filter(Boolean).join(" // ")
}

interface AccountDetailProps {
  provider: ProviderView
  busy: boolean
  onPick: (item: string) => void
  onOpenItem: (id: string) => void
}

/**
 * One provider (brief H3): SIGNED IN WITH over its name, a warning Banner
 * per warning from the core, the person's own login for it (OPEN), or a
 * choice of candidates with USE THIS ONE, then every site that falls with it.
 */
export function AccountDetail({ provider: p, busy, onPick, onOpenItem }: AccountDetailProps) {
  const [pick, setPick] = useState(p.candidates[0]?.item_id ?? "")
  // A phone lays the row's label over its value, so an address is not cut.
  const size = useAppFrame().phone ? "stacked" : "default"
  return (
    <article aria-labelledby="provider-title" className="flex flex-col gap-6">
      <header className="flex flex-col gap-1">
        <p className="type-overline text-muted-foreground">Signed in with</p>
        <h2 id="provider-title" className="type-title">
          {p.name}
        </h2>
      </header>
      {p.warnings.map((w) => (
        <Banner key={w} tone="warning" title={w} />
      ))}
      <div>
        {p.account ? (
          <FieldRow
            size={size}
            label={`Your ${p.name} login`}
            action={<FieldRowAction onClick={() => p.account && onOpenItem(p.account.item_id)}>Open</FieldRowAction>}
          >
            {p.account.username || p.account.title}
          </FieldRow>
        ) : p.candidates.length > 0 ? (
          <FieldRow
            size={size}
            label={`Your ${p.name} login`}
            action={
              <FieldRowAction disabled={busy || !pick} onClick={() => onPick(pick)}>
                Use this one
              </FieldRowAction>
            }
          >
            <NativeSelect aria-label={`Your ${p.name} account`} value={pick} onChange={(e) => setPick(e.target.value)}>
              {p.candidates.map((c) => (
                <NativeSelectOption key={c.item_id} value={c.item_id}>
                  {c.username || c.title}
                </NativeSelectOption>
              ))}
            </NativeSelect>
          </FieldRow>
        ) : (
          <FieldRow size={size} label={`Your ${p.name} login`}>
            Not saved in Lockdown
          </FieldRow>
        )}
      </div>
      <section aria-labelledby="provider-sites" className="flex flex-col">
        <SectionHeader id="provider-sites" title="Sites" count={p.sites.length} />
        <ul aria-label={`Sites you sign in to with ${p.name}`} className="flex flex-col">
          {p.sites.map((s) => (
            <ListRow key={s.item_id} render={<li />} metaStyle="plain" title={s.title} meta={siteLine(s) || undefined} onClick={() => onOpenItem(s.item_id)} />
          ))}
        </ul>
      </section>
    </article>
  )
}
