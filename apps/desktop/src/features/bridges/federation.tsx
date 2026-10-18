import { useCallback, useEffect, useState } from "react"

import {
  Badge,
  Button,
  Icon,
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemTitle,
  NativeSelect,
  NativeSelectOption,
} from "@unicron/ui"

import { describeError } from "@/core/client"

import { useBridges, type FederationMap, type MapLogin, type PendingView, type ProviderView } from "./bridges-client"

interface FederationProps {
  onOpenItem: (id: string) => void
  /** A record wrote a new version of an item: reload the list. */
  onChanged: () => void
  onNotice: (text: string) => void
}

/**
 * "Accounts you sign in with" on the Security screen (docs/plan/23-bridges.md
 * §4): each provider (Google, Apple, Microsoft, Steam, …) with its own login,
 * the sites entered through it (what goes with it if it is lost), and what
 * to fix first; and the sign-ins the browser extension noticed that wait
 * for a yes.
 */
export function FederationSection({ onOpenItem, onChanged, onNotice }: FederationProps) {
  const bridges = useBridges()
  const [map, setMap] = useState<FederationMap | null>(null)
  const [busy, setBusy] = useState(false)
  const reload = useCallback(() => {
    bridges?.federationMap().then(setMap).catch(() => setMap(null))
  }, [bridges])
  useEffect(() => reload(), [reload])
  if (!bridges || !map) return null
  const run = async (call: () => Promise<unknown>, notice: string) => {
    setBusy(true)
    try {
      await call()
      onNotice(notice)
      onChanged()
      reload()
    } catch (e) {
      onNotice(describeError(e))
    } finally {
      setBusy(false)
    }
  }
  return (
    <section aria-labelledby="federation" className="flex flex-col gap-2">
      <h2 id="federation" className="text-base font-semibold">
        Accounts you sign in with
      </h2>
      {map.providers.length === 0 && map.pending.length === 0 ? (
        <p className="text-sm text-muted-foreground">
          When you sign in to a site with Google, Apple, Microsoft or another account, Lockdown's browser extension notices and lists it here, with
          what you'd lose if that account were lost. You can also set "Signed in with" on a login.
        </p>
      ) : (
        <p className="text-sm text-muted-foreground">If one of these accounts is lost or taken over, every site under it goes with it.</p>
      )}
      {map.pending.length > 0 && (
        <ItemGroup role="list" aria-label="Sign-ins to confirm">
          {map.pending.map((p) => (
            <PendingRow
              key={p.id}
              pending={p}
              busy={busy}
              onRecord={(item) =>
                void run(() => bridges.federationAccept(p.id, item), `Noted: you sign in to ${p.host} with ${p.provider_name}.`)
              }
              onDismiss={() => void run(() => bridges.federationDismiss(p.id), "Forgotten.")}
            />
          ))}
        </ItemGroup>
      )}
      {map.providers.map((p) => (
        <ProviderCard
          key={p.id}
          provider={p}
          busy={busy}
          onOpenItem={onOpenItem}
          onPick={(item) => void run(() => bridges.federationSetAccount(p.id, item), `That's your ${p.name} account now.`)}
        />
      ))}
    </section>
  )
}

function PendingRow({
  pending: p,
  busy,
  onRecord,
  onDismiss,
}: {
  pending: PendingView
  busy: boolean
  onRecord: (item: string | null) => void
  onDismiss: () => void
}) {
  const [item, setItem] = useState(p.logins[0]?.item_id ?? "")
  return (
    <Item role="listitem" variant="outline" size="sm">
      <ItemContent>
        <ItemTitle>
          Did you sign in to {p.host} with {p.provider_name}?
        </ItemTitle>
        <ItemDescription>
          {p.confirmed ? `${p.provider_name}'s own sign-in page sent you back to this site.` : "The browser extension saw the button pressed."}
        </ItemDescription>
      </ItemContent>
      <ItemActions>
        {p.logins.length > 1 && (
          <NativeSelect aria-label="Which login" value={item} onChange={(e) => setItem(e.target.value)}>
            {p.logins.map((l) => (
              <NativeSelectOption key={l.item_id} value={l.item_id}>
                {l.title}
              </NativeSelectOption>
            ))}
          </NativeSelect>
        )}
        <Button size="sm" variant="outline" disabled={busy} onClick={() => onRecord(p.logins.length > 0 ? item : null)}>
          {p.logins.length > 0 ? "Yes, note it" : "Yes, save a login"}
        </Button>
        <Button size="sm" variant="ghost" disabled={busy} onClick={onDismiss}>
          No
        </Button>
      </ItemActions>
    </Item>
  )
}

function siteLine(l: MapLogin): string {
  return [l.title, l.host && l.host !== l.title ? l.host : null, l.has_password ? null : "no password of its own"].filter(Boolean).join(" · ")
}

function ProviderCard({
  provider: p,
  busy,
  onOpenItem,
  onPick,
}: {
  provider: ProviderView
  busy: boolean
  onOpenItem: (id: string) => void
  onPick: (item: string) => void
}) {
  const [pick, setPick] = useState(p.candidates[0]?.item_id ?? "")
  return (
    <section aria-label={p.name} className="flex flex-col gap-2 rounded-md border p-3">
      <div className="flex flex-wrap items-center gap-2">
        <h3 className="text-sm font-semibold">{p.name}</h3>
        <Badge variant="outline">{p.sites.length === 1 ? "1 site" : `${p.sites.length} sites`}</Badge>
        {p.account && (
          <Button size="sm" variant="ghost" onClick={() => p.account && onOpenItem(p.account.item_id)}>
            Your {p.name} login: {p.account.username || p.account.title}
          </Button>
        )}
      </div>
      {p.warnings.length > 0 && (
        <ul aria-label={`What to fix for ${p.name}`} className="flex flex-col gap-1 text-sm">
          {p.warnings.map((w) => (
            <li key={w} className="flex gap-2">
              <span className="text-attention-text">
                <Icon name="warning" />
              </span>
              <span>{w}</span>
            </li>
          ))}
        </ul>
      )}
      {p.candidates.length > 1 && (
        <div className="flex gap-2">
          <NativeSelect aria-label={`Your ${p.name} account`} value={pick} onChange={(e) => setPick(e.target.value)}>
            {p.candidates.map((c) => (
              <NativeSelectOption key={c.item_id} value={c.item_id}>
                {c.username || c.title}
              </NativeSelectOption>
            ))}
          </NativeSelect>
          <Button size="sm" variant="outline" disabled={busy || !pick} onClick={() => onPick(pick)}>
            This is the one
          </Button>
        </div>
      )}
      <ul aria-label={`Sites you sign in to with ${p.name}`} className="flex flex-col text-sm">
        {p.sites.map((s) => (
          <li key={s.item_id}>
            <Button size="sm" variant="ghost" onClick={() => onOpenItem(s.item_id)}>
              {siteLine(s)}
            </Button>
          </li>
        ))}
      </ul>
    </section>
  )
}
