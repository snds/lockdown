import { useCallback, useEffect, useMemo, useState } from "react"

import { Banner, Button, EmptyState, Skeleton } from "@unicron/ui"

import { describeError } from "@/core/client"
import { KeyResets } from "@/features/sharing/key-resets"
import { PeopleTab } from "@/features/sharing/people-section"
import { SharingClientProvider, type SharingOverview } from "@/features/sharing/sharing-client"
import { SharingPaused } from "@/features/sharing/sharing-paused"

import type { Link } from "../core/link"
import { webSharingClient } from "../core/web-finish-client"

/**
 * The web vault's People tab (docs/plan/26-desktop-finish.md §8, brief S2
 * and S1's web row): the desktop's own People list and detail over the
 * worker. Someone is looked up in the server's key-transparency log and
 * pinned; comparing fingerprints takes the digits read out, the code under
 * their QR pasted, or their QR code scanned with this tab's camera (the
 * core reads the frames). Your own fingerprint and code show here for them
 * to compare. Someone who reset their account and waits on you to let them
 * back in is the needs-you (docs/plan/55-member-key-change.md). The web's
 * sharing client has no clipboard path, so your code has no COPY here.
 */
export function PeopleView({ link, onNotice }: { link: Link; onNotice: (text: string) => void }) {
  const sharing = useMemo(() => webSharingClient(link), [link])
  const [overview, setOverview] = useState<SharingOverview | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [failed, setFailed] = useState<string | null>(null)
  const [person, setPerson] = useState<string | null>(null)
  const reload = useCallback(() => {
    sharing
      .overview()
      .then((o) => {
        setOverview(o)
        setError(null)
      })
      .catch((e) => setError(describeError(e)))
  }, [sharing])
  useEffect(reload, [reload])
  return (
    <SharingClientProvider value={sharing}>
      <div className="flex w-full flex-col gap-6 pt-6">
        {error && overview && (
          <Banner
            tone="critical"
            title="People didn't load."
            body={error}
            action={
              <Button type="button" variant="secondary" size="sm" onClick={reload}>
                Try again
              </Button>
            }
          />
        )}
        {error && !overview && <EmptyState tone="critical" overline="People" title="People didn't load." body={error} actionLabel="Try again" onAction={reload} />}
        {!overview && !error && (
          <div className="flex flex-col gap-2" aria-label="Loading people">
            <Skeleton variant="row" />
            <Skeleton variant="row" />
          </div>
        )}
        {overview && (
          <>
            <SharingPaused text={overview.paused} />
            {!overview.online && <Banner tone="info" size="compact" title="You're offline." body="Sharing needs the server." />}
            {failed && <Banner tone="critical" size="compact" title="Not done." body={failed} onDismiss={() => setFailed(null)} />}
            <KeyResets resets={overview.resets ?? []} online={overview.online} onChanged={reload} onNotice={onNotice} onError={setFailed} />
            <PeopleTab overview={overview} selected={person} onSelect={setPerson} onChanged={reload} onNotice={onNotice} />
          </>
        )}
      </div>
    </SharingClientProvider>
  )
}
