import { Skeleton } from "@unicron/ui"

/** How many rows the wash suggests; no row says anything. */
const ROWS = 8

/**
 * What stands behind the unlock screen (brief §4): the shell's shape in
 * the locked wash (`--surface-locked`), skeleton rows where the list
 * would be, no titles, no icons, nothing to read. The core dropped the
 * keys; this draws nothing that came from them. Hidden from assistive
 * tech, since there is nothing in it to hear.
 */
export function LockedWash() {
  return (
    <div aria-hidden className="pointer-events-none flex h-full min-h-0 flex-col bg-surface-locked md:flex-row" data-slot="locked-wash">
      {/* Row skeletons are as tall as a row at the person's density (`--c-skeleton-row-height`, app-extension.css). */}
      <div className="hidden w-(--c-shell-sidebar-width) shrink-0 flex-col gap-2 border-r p-3 md:flex">
        {Array.from({ length: 8 }, (_, i) => (
          <Skeleton key={i} variant="row" />
        ))}
      </div>
      <div className="flex w-full shrink-0 flex-col gap-2 p-3 md:w-(--c-shell-list-width) md:border-r">
        {Array.from({ length: ROWS + 1 }, (_, i) => (
          <Skeleton key={i} variant="row" />
        ))}
      </div>
      <div className="hidden flex-1 md:block" />
    </div>
  )
}
