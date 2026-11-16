import { Tooltip, TooltipContent, TooltipTrigger } from "@unicron/ui"

import { ageWords, rowDate } from "@/lib/format"

/**
 * A time in a row, by the cross-app rule (settled across the four apps):
 * the clock time (`Today 09:02`, `MON 09:02`, `NOV 02`), with how long ago
 * as its tip, which a screen reader hears as the description. `prefix` goes
 * before the time in the same words ("Backed up", "Last used"). The time takes
 * focus, so a keyboard user sees the tip too (WCAG 1.4.13, 2.1.1).
 */
export function ClockTime({ secs, prefix, nowSecs }: { secs: number; prefix?: string; nowSecs?: number }) {
  const when = rowDate(secs, nowSecs)
  const ago = ageWords(secs, nowSecs)
  return (
    <Tooltip>
      <TooltipTrigger render={<span tabIndex={0} aria-description={ago} />}>{prefix ? `${prefix} ${when}` : when}</TooltipTrigger>
      <TooltipContent>{ago}</TooltipContent>
    </Tooltip>
  )
}
