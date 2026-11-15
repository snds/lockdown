import { useCallback, useEffect, useState } from "react"

import { AdaptivePopover, FieldRow, FieldRowAction, RadioGroup, RadioRow, ToggleRow } from "@unicron/ui"

import { useGate } from "@/app/gate"
import { describeError } from "@/core/client"

import { TIER_WORDS, asksMore, useDepthClient, type ExposureTier, type ExposureView } from "./depth-client"

/**
 * One item's exposure (docs/plan/22-security-depth.md §1), read once for
 * the pane: Details' shut line names the tier (brief I4), and the row reads
 * the same answer, so the core is asked once. `null` until it comes, and
 * where the host has no depth client or the core refused.
 */
export function useExposure(itemId: string, enabled = true): [ExposureView | null, (view: ExposureView) => void] {
  const depth = useDepthClient()
  const [view, setView] = useState<ExposureView | null>(null)
  useEffect(() => {
    if (!depth || !enabled) return
    let live = true
    depth
      .exposure(itemId)
      .then((v) => live && setView(v))
      .catch(() => live && setView(null))
    return () => {
      live = false
    }
  }, [depth, itemId, enabled])
  return [view, setView]
}

interface ItemProtectionProps {
  itemId: string
  /** Kept for the callers that pass it; the emergency-access switch is `EmergencyAccess`. */
  crownJewel?: boolean
  onNotice?: (text: string) => void
  /** Field Row's layout: `stacked` on a phone and in a pane under 480 px (brief 4.0). */
  size?: "default" | "stacked"
  /** Whether this person may change the item. Default true; without it the row is not drawn (brief I4). */
  canEdit?: boolean
  /** The pane's own read (`useExposure`) and where a change goes; without them the row reads for itself. */
  exposure?: ExposureView | null
  onExposure?: (view: ExposureView) => void
  /** The item's title, as the picker's overline. */
  itemTitle?: string
}

/**
 * WHO CAN GET ITS VALUES (brief I4, D245): one Field Row with the tier's
 * name, and under it the core's reason when the tier is the default. CHANGE
 * opens a picker (a popover with its header on a desktop, a sheet on a
 * phone) of the tiers the core offers as RadioRows, each with its sentence.
 * Asking more takes effect at once; asking less asks first (the gate).
 * Saving is silent: the row's value is the answer. A failure leaves the old
 * tier and says why beside the row.
 */
export function ItemProtection({ itemId, size = "default", canEdit = true, exposure, onExposure, itemTitle }: ItemProtectionProps) {
  const depth = useDepthClient()
  const gate = useGate()
  const controlled = exposure !== undefined
  const [own, setOwn] = useExposure(itemId, !controlled)
  const view = controlled ? exposure : own
  const setView = (v: ExposureView) => (controlled ? onExposure?.(v) : setOwn(v))
  const [open, setOpen] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  if (!depth || !view || !canEdit) return null

  const choose = async (tier: ExposureTier) => {
    if (tier === view.tier) return setOpen(false)
    setBusy(true)
    setError(null)
    try {
      const next = await gate.run((g) => depth.setExposure(itemId, tier === view.default_tier ? null : tier, g))
      setView(next)
      setOpen(false)
    } catch (e) {
      setError(describeError(e))
    } finally {
      setBusy(false)
    }
  }

  const words = TIER_WORDS[view.tier]
  const why = [
    view.source !== "chosen" && view.reason ? view.reason : null,
    asksMore(view.default_tier, "fill") && view.tier === "fill" ? "This item usually asks first." : null,
  ]
    .filter(Boolean)
    .join(" ")
  return (
    <>
      <FieldRow
        size={size}
        label="Who can get its values"
        description={error ? undefined : why || undefined}
        action={
          <AdaptivePopover
            open={open}
            onOpenChange={(o) => !busy && setOpen(o)}
            align="end"
            overline={itemTitle}
            title="Who can get its values"
            trigger={<FieldRowAction>Change</FieldRowAction>}
          >
            <RadioGroup aria-label="Who can get its values" value={view.tier} onValueChange={(v) => void choose(v as ExposureTier)} disabled={busy}>
              {view.choices.map((t, i) => (
                <RadioRow key={t} value={t} label={t === view.default_tier ? `${TIER_WORDS[t].name} (usual)` : TIER_WORDS[t].name} description={TIER_WORDS[t].words} rule={i === 0 ? "none" : "top"} />
              ))}
            </RadioGroup>
          </AdaptivePopover>
        }
      >
        {words.name}
      </FieldRow>
      {error && (
        <p role="alert" className="type-caption pb-3 text-critical-text">
          {error}
        </p>
      )}
    </>
  )
}

interface EmergencyAccessProps {
  itemId: string
  canEdit?: boolean
}

/**
 * A crown jewel's "Include in emergency access" (§8, brief I4): a Toggle
 * Row after the item's version. Off by default; turning it on lets an
 * emergency contact get this item after their wait, and asks first (the
 * gate). The switch is the answer, so nothing toasts; a failure puts it back
 * and says why under it.
 */
export function EmergencyAccess({ itemId, canEdit = true }: EmergencyAccessProps) {
  const depth = useDepthClient()
  const gate = useGate()
  const [on, setOn] = useState<boolean | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const load = useCallback(() => {
    if (!depth) return
    depth
      .legacyOptIn(itemId)
      .then(setOn)
      .catch(() => setOn(null))
  }, [depth, itemId])
  useEffect(load, [load])

  if (!depth || on === null || !canEdit) return null

  const change = async (next: boolean) => {
    setBusy(true)
    setError(null)
    try {
      await gate.run((g) => depth.setLegacyOptIn(itemId, next, g))
      setOn(next)
    } catch (e) {
      setError(describeError(e))
    } finally {
      setBusy(false)
    }
  }
  return (
    <>
      <ToggleRow
        label="Include in emergency access"
        description={on ? "Your emergency contacts get this item after their wait." : "Crown jewels stay out of emergency access unless you include them here."}
        checked={on}
        disabled={busy}
        onCheckedChange={(v) => void change(v)}
      />
      {error && (
        <p role="alert" className="type-caption pb-3 text-critical-text">
          {error}
        </p>
      )}
    </>
  )
}
