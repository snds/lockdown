import { Icon } from "@unicron/ui"

/**
 * What a split view pauses, on the screens where sharing starts (D328 as
 * changed by D375): the core's own sentence, from the overview. The core
 * refuses each paused step by itself; this says so before anyone tries.
 */
export function SharingPaused({ text }: { text: string | null | undefined }) {
  if (!text) return null
  return (
    <div role="alert" className="flex flex-col gap-1 rounded-md border border-destructive p-3 text-sm text-destructive">
      <p className="flex items-center gap-2 font-medium">
        <Icon name="warning" />
        This server's public log doesn't match a mirror
      </p>
      <p>{text}</p>
    </div>
  )
}
