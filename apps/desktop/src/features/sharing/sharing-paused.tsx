import { Banner, Button } from "@unicron/ui"

import { useBannerShows } from "@/app/banners"
import { usePlaces } from "@/app/places"

/** The Banner's title, the same as the shell's split-view Banner (D328 as changed by D375). */
export const PAUSED_TITLE = "This server's public log doesn't match a mirror"

/**
 * What a split view pauses, on Sharing (D328 as changed by D375, brief S1
 * and X2): the critical Banner with the core's own sentence, from the
 * overview, and SEE STATUS. The core refuses each paused step by itself;
 * this says so before anyone tries. When the shell already shows its
 * split-view Banner at the top of Sharing, this draws nothing: one Banner
 * for one thing.
 */
export function SharingPaused({ text }: { text: string | null | undefined }) {
  const { go } = usePlaces()
  const shellShows = useBannerShows("sharing")
  if (!text || shellShows) return null
  return (
    <Banner
      tone="critical"
      title={PAUSED_TITLE}
      body={text}
      action={
        <Button type="button" variant="secondary" size="sm" onClick={() => go("settings-status")}>
          See status
        </Button>
      }
    />
  )
}
