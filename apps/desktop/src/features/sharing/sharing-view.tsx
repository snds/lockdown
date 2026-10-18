import { useCallback, useEffect, useState } from "react"

import { Badge, Button, Icon, Item, ItemActions, ItemContent, ItemDescription, ItemGroup, ItemTitle, Separator, Spinner } from "@unicron/ui"

import { PaneHeader } from "@/components/master-detail"
import { describeError } from "@/core/client"
import type { ItemSummary } from "@/core/types"
import { useVaults } from "@/features/organization/vaults-context"
import { whenLocal } from "@/lib/format"

import { EmergencySection } from "./emergency-section"
import { RequestLinksSection, ShareLinksSection } from "./links-section"
import { PeopleSection } from "./people-section"
import { RIGHT_NAMES, useSharingClient, type SentShareView, type SharingOverview } from "./sharing-client"
import { HouseholdsSection, VaultsSection } from "./vault-sharing"

interface SharingViewProps {
  /** The rows, for the titles of items shared one by one. */
  rows: ItemSummary[]
  onOpenItem: (id: string) => void
  /** Something changed what the vault holds (a vault opened, a share stopped): reload. */
  onChanged: () => void
  onNotice: (text: string) => void
}

/**
 * The Sharing screen (docs/plan/18-sharing.md §8): your fingerprint, the
 * people pinned here, vaults and their members, the household, items
 * shared one by one, share links, request links and emergency access.
 * Nothing secret is drawn here except what a link or an answer holds,
 * which comes over one value at a time when the person asks.
 */
export function SharingView({ rows, onOpenItem, onChanged, onNotice }: SharingViewProps) {
  const sharing = useSharingClient()
  const { vaults, reload: reloadVaults } = useVaults()
  const [overview, setOverview] = useState<SharingOverview | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)

  const load = useCallback(() => {
    sharing
      .overview()
      .then((o) => {
        setOverview(o)
        setError(null)
      })
      .catch((e) => setError(describeError(e)))
      .finally(() => setLoading(false))
  }, [sharing])
  useEffect(load, [load])

  const changed = useCallback(() => {
    setLoading(true)
    load()
    reloadVaults()
    onChanged()
  }, [load, reloadVaults, onChanged])

  const unshare = async (s: SentShareView) => {
    try {
      await sharing.unshareItem(s.share_id)
      onNotice(`Stopped sharing with ${s.login}; the item moved to a new key.`)
      changed()
    } catch (e) {
      setError(describeError(e))
    }
  }
  const titleOf = (id: string) => rows.find((r) => r.id === id)?.title ?? "An item"

  return (
    <div className="mx-auto flex w-full max-w-4xl flex-col gap-4 p-4 md:p-6">
      <PaneHeader title="Sharing">
        <Button size="sm" variant="outline" onClick={changed} disabled={loading}>
          {loading ? <Spinner data-icon="inline-start" /> : <Icon name="refresh" data-icon="inline-start" />}
          Refresh
        </Button>
      </PaneHeader>
      {error && (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      )}
      {!overview && !error && <Spinner />}
      {overview && (
        <>
          {!overview.online && (
            <p role="note" className="text-sm text-muted-foreground">
              Sharing needs an account on a sync server. Set one up in Settings; until then this screen only shows what is already here.
            </p>
          )}
          {overview.waiting_on_you > 0 && (
            <p role="alert" className="flex items-center gap-2 text-sm">
              <Badge variant="destructive">Emergency</Badge>
              {overview.waiting_on_you === 1
                ? "Someone asked for emergency access to your vaults. Stop it below if you didn't expect it."
                : `${overview.waiting_on_you} people asked for emergency access to your vaults. Stop it below if you didn't expect it.`}
            </p>
          )}
          {overview.fingerprint && (
            <section aria-labelledby="sharing-you" className="flex flex-col gap-1">
              <h2 id="sharing-you" className="text-base font-semibold">
                Your fingerprint
              </h2>
              <p className="font-mono text-lg tracking-wide">{overview.fingerprint}</p>
              <p className="text-sm text-muted-foreground">
                Read this out when someone compares fingerprints with you. It changes only when your keys do.
              </p>
            </section>
          )}
          <PeopleSection contacts={overview.contacts} online={overview.online} onChanged={load} onNotice={onNotice} />
          <Separator />
          <VaultsSection vaults={vaults} contacts={overview.contacts} online={overview.online} onChanged={changed} onNotice={onNotice} />
          <HouseholdsSection
            households={overview.households}
            vaults={vaults}
            contacts={overview.contacts}
            online={overview.online}
            onChanged={changed}
            onNotice={onNotice}
          />
          {(overview.sent.length > 0 || overview.received.length > 0) && (
            <section aria-labelledby="sharing-items" className="flex flex-col gap-2">
              <h2 id="sharing-items" className="text-base font-semibold">
                Items shared one by one
              </h2>
              {overview.sent.length > 0 && (
                <ItemGroup role="list" aria-label="You shared">
                  {overview.sent.map((s) => (
                    <Item key={s.share_id} variant="outline" size="sm" role="listitem">
                      <ItemContent>
                        <ItemTitle>{titleOf(s.item_id)}</ItemTitle>
                        <ItemDescription>
                          With {s.login} · {RIGHT_NAMES[s.right]} · since {whenLocal(s.created_at)}
                        </ItemDescription>
                      </ItemContent>
                      <ItemActions>
                        <Button variant="ghost-destructive" size="sm" onClick={() => void unshare(s)}>
                          Stop sharing
                        </Button>
                      </ItemActions>
                    </Item>
                  ))}
                </ItemGroup>
              )}
              {overview.received.length > 0 && (
                <ItemGroup role="list" aria-label="Shared with you">
                  {overview.received.map((r) => (
                    <Item key={r.share_id} variant="outline" size="sm" role="listitem">
                      <ItemContent>
                        <ItemTitle>{titleOf(r.item_id)}</ItemTitle>
                        <ItemDescription>
                          From {r.from} · {RIGHT_NAMES[r.right]}
                        </ItemDescription>
                      </ItemContent>
                      <ItemActions>
                        <Button variant="outline" size="sm" onClick={() => onOpenItem(r.item_id)}>
                          Open
                        </Button>
                      </ItemActions>
                    </Item>
                  ))}
                </ItemGroup>
              )}
            </section>
          )}
          <Separator />
          <ShareLinksSection online={overview.online} onNotice={onNotice} />
          <RequestLinksSection online={overview.online} onNotice={onNotice} />
          <Separator />
          <EmergencySection
            grants={overview.emergency}
            contacts={overview.contacts}
            vaults={vaults}
            online={overview.online}
            onGrants={(emergency) => setOverview((o) => (o ? { ...o, emergency } : o))}
            onChanged={changed}
            onNotice={onNotice}
          />
        </>
      )}
    </div>
  )
}
