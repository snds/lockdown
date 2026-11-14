import { useEffect, useRef, type ReactNode } from "react"

import { AppFrame, AppFrameBottomRow, AppFrameFocus, AppLockup } from "@unicron/ui"

import type { LockReason } from "@/core/types"
import { countdown } from "@/lib/format"

/**
 * Everything that stands before the vault (brief F1, W1, W2; cross-app
 * decisions §4): the whole app is closed, so no shell is drawn, only
 * Unicron's focus frame: one 432 column with the lockup at its top and one
 * primary. On a phone `end` is that primary, as a float button in the
 * bottom row (Mobile / 13); on a desktop it draws nothing there.
 *
 * Locked means nothing readable is drawn: nothing in here comes from the
 * vault, and nothing animates in or out (brief 4.0, D107 as changed by Q15).
 */
export function ClosedFrame({ children, end, folio }: { children: ReactNode; end?: ReactNode; folio?: ReactNode }) {
  return (
    <AppFrame>
      <AppFrameFocus lockup={<AppLockup name="Lockdown" />} folio={folio}>
        {children}
      </AppFrameFocus>
      {end ? <AppFrameBottomRow variant="detail" end={end} /> : null}
    </AppFrame>
  )
}

interface ClosedHeadingProps {
  /** The mono caps line above the title: where this is and why (`Locked // Idle 05:00`). */
  overline?: ReactNode
  title: string
  /** One sentence under the title. */
  children?: ReactNode
  /**
   * Move focus to the title when it changes: a step of a flow, so a screen
   * reader hears it. Left off where a field takes the focus (unlock).
   */
  focusTitle?: boolean
}

/** The overline, the display title (the screen's one `h1`) and its sentence, left aligned as Desktop / 10 draws them. */
export function ClosedHeading({ overline, title, children, focusTitle = false }: ClosedHeadingProps) {
  const heading = useRef<HTMLHeadingElement>(null)
  useEffect(() => {
    if (focusTitle) heading.current?.focus()
  }, [focusTitle, title])
  return (
    <div className="flex flex-col gap-4">
      {overline ? <p className="type-overline text-text-label">{overline}</p> : null}
      <h1 ref={heading} tabIndex={-1} className="type-display-compact text-text-body outline-none">
        {title}
      </h1>
      {children ? <p className="type-body text-text-secondary">{children}</p> : null}
    </div>
  )
}

/** What the overline says after LOCKED, per reason (brief F1). */
const REASON_WORDS: Record<LockReason, string | null> = {
  requested: null,
  idle: "Idle",
  hidden: "Out of view",
  sleep: "Sleep",
}

/**
 * The overline of a lock screen (brief F1, W1): `LOCKED`, the reason, the
 * idle timer that fired when the core reports one, and the person's name
 * when one is set (Q15). Never a vault's name: nothing readable shows while
 * locked. The desktop and the web vault say it alike.
 */
export function lockedLine(reason: LockReason | null | undefined, idleSecs: number | null, name: string | null | undefined): string {
  const parts = ["Locked"]
  const why = reason ? REASON_WORDS[reason] : null
  if (why) parts.push(reason === "idle" && idleSecs ? `${why} ${countdown(idleSecs)}` : why)
  if (name) parts.push(name)
  return parts.join(" // ")
}
