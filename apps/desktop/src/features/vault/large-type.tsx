import { useCallback, useEffect, useMemo, useRef, useState } from "react"

import { Button, Icon } from "@unicron/ui"
import { LargeTypeView } from "@unicron/ui/security/large-type-view"

import { useGate } from "@/app/gate"
import { useClient } from "@/core/client"
import { useCaptureProtect } from "@/features/hygiene/hygiene-client"
import { countdown } from "@/lib/format"

/** How long large type stays up before it hides itself (brief §3.2; `--c-large-type-lifetime`). */
export const LARGE_TYPE_LIFETIME_S = 120

interface LargeTypeButtonProps {
  /** What is shown, never the value: "Wi-Fi password · Home". */
  title: string
  /** One-shot bytes, as SecretField's `getSecret`. */
  getSecret: () => Promise<Uint8Array>
  groupSize?: 3 | 4
}

/**
 * "Show large type": unicron's LargeTypeView as a fullscreen sheet, read
 * across a room or typed into a TV (brief §3.2). The value comes from the
 * host as bytes and hides after two minutes, on blur and on lock. Keeping
 * the screen awake and raising brightness are a phone's hooks; on desktop
 * they are no-ops, left out here.
 */
export function ShowLargeType({ title, getSecret, groupSize = 3 }: LargeTypeButtonProps) {
  const [open, setOpen] = useState(false)
  // Disabled while the gate asks, as the field's own reveal and copy are (gate.tsx `asking`).
  const { asking } = useGate()
  useCaptureProtect(open)
  return (
    <>
      <Button variant="ghost" size="sm" disabled={asking} onClick={() => setOpen(true)}>
        <Icon name="text--scale" data-icon="inline-start" />
        Show large type
      </Button>
      <LargeTypeView open={open} onOpenChange={setOpen} getSecret={getSecret} title={title} groupSize={groupSize} lifetime={LARGE_TYPE_LIFETIME_S} />
    </>
  )
}

/** Large type for one hidden field of an item, through the core's `reveal`. */
export function ShowFieldLargeType({ itemId, fieldId, title }: { itemId: string; fieldId: string; title: string }) {
  const client = useClient()
  // `run` stays the same while the gate asks, so the viewer does not read again (and ask again) once it closes.
  const { run } = useGate()
  const getSecret = useCallback(() => run((g) => client.reveal(itemId, fieldId, g)), [client, run, itemId, fieldId])
  return <ShowLargeType title={title} getSecret={getSecret} />
}

/** The title over a code in large type: what it is, and for a timed code the seconds it has left. */
export function codeLargeTitle(named: string, now: number, expiresAt: number | null): string {
  return expiresAt === null ? `Code · ${named}` : `Code · ${named} // ${countdown(expiresAt - now)} left`
}

interface CodeLargeTypeProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  /** The code on screen now, as the core computed it; large type follows it when it changes. */
  current: string
  /** The issuer and account, or the item's title. */
  named: string
  /** The host clock and this code's end (Unix seconds); `null` for a counter-based code. */
  now: number
  expiresAt: number | null
  groupSize?: 3 | 4
}

/**
 * A one-time code in large type (Lockdown 28, amending D963): it follows the
 * live code, so it never shows a dead one. When the code turns over, the
 * same view reads the new one (it is never remounted, so Captions, the size,
 * and focus stay where the person put them); its title counts down the
 * seconds left. Still kept out of screenshots and screen sharing
 * (`useCaptureProtect`), and it still closes two minutes after it opened,
 * whichever code it shows then. Once the view has hidden its value (blur,
 * lock, lifetime), a new code does not bring it back: Show again reads the
 * code valid then.
 */
export function CodeLargeType({ open, onOpenChange, current, named, now, expiresAt, groupSize = 3 }: CodeLargeTypeProps) {
  useCaptureProtect(open)
  const openedAt = useRef<number | null>(null)
  if (open && openedAt.current === null) openedAt.current = Date.now()
  if (!open) openedAt.current = null
  // Two minutes from opening, not from the latest code.
  useEffect(() => {
    if (!open) return
    const left = LARGE_TYPE_LIFETIME_S * 1000 - (Date.now() - (openedAt.current ?? Date.now()))
    const t = window.setTimeout(() => onOpenChange(false), Math.max(0, left))
    return () => window.clearTimeout(t)
  }, [open, onOpenChange])
  // The view reads through `getSecret` when it opens, on Show again, and when the reader changes.
  // The reader always gives the code valid now; it changes (and so the view reads again) only when
  // the code turns over while the value is on screen.
  const latest = useRef(current)
  latest.current = current
  const [turn, setTurn] = useState(current)
  useEffect(() => {
    const hidden = document.querySelector('[data-slot="large-type-view"]')?.getAttribute("data-status") === "expired"
    if (!hidden) setTurn(current)
  }, [current])
  // eslint-disable-next-line react-hooks/exhaustive-deps -- `turn` is the trigger: a new reader is a new read
  const getSecret = useCallback(() => Promise.resolve(new TextEncoder().encode(latest.current)), [turn])
  // What is left of the two minutes when this code was read: the view's own "Hides in" counts from there.
  // eslint-disable-next-line react-hooks/exhaustive-deps -- fixed per read, never per render
  const lifetime = useMemo(() => Math.max(1, Math.ceil(LARGE_TYPE_LIFETIME_S - (Date.now() - (openedAt.current ?? Date.now())) / 1000)), [turn, open])
  return <LargeTypeView open={open} onOpenChange={onOpenChange} getSecret={getSecret} title={codeLargeTitle(named, now, expiresAt)} groupSize={groupSize} lifetime={lifetime} />
}
