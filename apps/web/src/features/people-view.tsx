import { useCallback, useEffect, useMemo, useState } from "react"

import { Spinner } from "@unicron/ui"

import { PaneHeader } from "@/components/master-detail"
import { describeError } from "@/core/client"
import { PeopleSection } from "@/features/sharing/people-section"
import { SharingClientProvider, type SharingOverview } from "@/features/sharing/sharing-client"
import { SharingPaused } from "@/features/sharing/sharing-paused"

import type { Link } from "../core/link"
import { webSharingClient } from "../core/web-finish-client"

/**
 * The web vault's contacts (docs/plan/26-desktop-finish.md §8): the
 * desktop's own People section over the worker. Someone is looked up in
 * the server's key-transparency log and pinned; comparing fingerprints
 * takes the digits read out, the code under their QR pasted, or their QR
 * code scanned with this tab's camera (the core reads the frames). Your
 * own fingerprint shows here for them to compare.
 */
export function PeopleView({ link, onNotice }: { link: Link; onNotice: (text: string) => void }) {
  const sharing = useMemo(() => webSharingClient(link), [link])
  const [overview, setOverview] = useState<SharingOverview | null>(null)
  const [error, setError] = useState<string | null>(null)
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
    <div className="flex min-h-0 flex-1 flex-col overflow-y-auto bg-surface-chrome">
      <PaneHeader title="People" />
      <div className="flex flex-col gap-4 px-4 pb-4">
        {error && (
          <p role="alert" className="text-sm text-destructive">
            {error}
          </p>
        )}
        {!overview && !error && <Spinner />}
        {overview && (
          <>
            <SharingPaused text={overview.paused} />
            <p className="text-sm">
              Your fingerprint: <code>{overview.fingerprint}</code>
            </p>
            <p className="text-xs text-muted-foreground">
              Or give them your code to paste: <code aria-label="Your code">{overview.fingerprint_qr}</code>
            </p>
            <SharingClientProvider value={sharing}>
              <PeopleSection contacts={overview.contacts} online={overview.online} onChanged={reload} onNotice={onNotice} />
            </SharingClientProvider>
          </>
        )}
      </div>
    </div>
  )
}
