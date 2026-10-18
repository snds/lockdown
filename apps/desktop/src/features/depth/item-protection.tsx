import { useCallback, useEffect, useState } from "react"

import { Field, FieldDescription, FieldError, FieldLabel, NativeSelect, NativeSelectOption, Switch } from "@unicron/ui"

import { useGate } from "@/app/gate"
import { describeError } from "@/core/client"

import { TIER_WORDS, asksMore, useDepthClient, type ExposureTier, type ExposureView } from "./depth-client"

interface ItemProtectionProps {
  itemId: string
  crownJewel: boolean
  onNotice: (text: string) => void
}

/**
 * How far one item lets its values out (docs/plan/22-security-depth.md §1)
 * and, for a crown jewel, whether an emergency contact gets it after their
 * wait (§8). Asking more takes effect at once; asking less, and letting a
 * crown jewel go to an emergency contact, ask first (the core's gates).
 */
export function ItemProtection({ itemId, crownJewel, onNotice }: ItemProtectionProps) {
  const depth = useDepthClient()
  const gate = useGate()
  const [view, setView] = useState<ExposureView | null>(null)
  const [legacy, setLegacy] = useState<boolean | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const load = useCallback(() => {
    if (!depth) return
    depth
      .exposure(itemId)
      .then(setView)
      .catch(() => setView(null))
    if (crownJewel)
      depth
        .legacyOptIn(itemId)
        .then(setLegacy)
        .catch(() => setLegacy(null))
  }, [depth, itemId, crownJewel])
  useEffect(load, [load])

  if (!depth || !view) return null

  const choose = async (tier: ExposureTier) => {
    setBusy(true)
    setError(null)
    try {
      const next = await gate.run((g) => depth.setExposure(itemId, tier === view.default_tier ? null : tier, g))
      setView(next)
      onNotice(`${TIER_WORDS[next.tier].name}: ${TIER_WORDS[next.tier].words}`)
    } catch (e) {
      setError(describeError(e))
    } finally {
      setBusy(false)
    }
  }

  const release = async (on: boolean) => {
    setBusy(true)
    setError(null)
    try {
      await gate.run((g) => depth.setLegacyOptIn(itemId, on, g))
      setLegacy(on)
      onNotice(on ? "Your emergency contacts get this item after their wait." : "This item stays out of emergency access.")
    } catch (e) {
      setError(describeError(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <section aria-label="Protection" className="flex flex-col gap-3 rounded-xl border p-3">
      <Field>
        <FieldLabel htmlFor={`exposure-${itemId}`}>Who can get its values</FieldLabel>
        <NativeSelect
          id={`exposure-${itemId}`}
          value={view.tier}
          onChange={(e) => void choose(e.target.value as ExposureTier)}
          disabled={busy}
        >
          {view.choices.map((t) => (
            <NativeSelectOption key={t} value={t}>
              {TIER_WORDS[t].name}
              {t === view.default_tier ? " (default)" : ""}
            </NativeSelectOption>
          ))}
        </NativeSelect>
        <FieldDescription>
          {TIER_WORDS[view.tier].words}
          {view.reason && view.tier !== "fill" ? ` ${view.reason}` : ""}
          {asksMore(view.default_tier, "fill") && view.tier === "fill" ? " This item usually asks first." : ""}
        </FieldDescription>
      </Field>
      {crownJewel && legacy !== null && (
        <Field orientation="horizontal">
          <FieldLabel htmlFor={`legacy-${itemId}`}>
            Include in emergency access
          </FieldLabel>
          <Switch id={`legacy-${itemId}`} checked={legacy} onCheckedChange={(v) => void release(v)} disabled={busy} />
        </Field>
      )}
      {error && <FieldError role="alert">{error}</FieldError>}
    </section>
  )
}
