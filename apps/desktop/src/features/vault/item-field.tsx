import { useCallback, useEffect, useMemo, useRef, useState } from "react"

import { AdaptiveMenuItem, FieldRow, FieldRowAction, FieldRowValue } from "@unicron/ui"
import { LargeTypeView } from "@unicron/ui/security/large-type-view"
import { SecretField, type SecretFieldReferenceHost, type SecretFieldRemote, type SecretFormat } from "@unicron/ui/security/secret-field"

import { useGate } from "@/app/gate"
import { useLocked } from "@/app/locked"
import { useSettings } from "@/app/settings"
import { describeError, isAppError, useClient } from "@/core/client"
import type { Control, FieldView } from "@/core/types"

import { protectFor, useCaptureProtect, useHygieneClient } from "@/features/hygiene/hygiene-client"

import { dateWords, siteWords } from "./detail-model"
import { LARGE_TYPE_LIFETIME_S, ShowFieldLargeType } from "./large-type"

export type { SecretFormat }

/** A Field Row's layout: label beside the value (desktop), or over it (a phone, a pane under 480 px). */
export type RowSize = "default" | "stacked"

interface ItemFieldProps {
  itemId: string
  field: FieldView
  /** Kept for the callers that still pass it: a plain copy says "Copied" in its own row and raises no toast (cross-app §1). */
  onNotice: (text: string) => void
  /** How a value is drawn: as is, or in groups of four (cards, brief §3.1). */
  format?: SecretFormat
  /**
   * "Show large type" for this hidden field, named so ("Wi-Fi password · Home"). It is the first row
   * of the field's ⋯ (D242, brief I9), unless `largeTypeAction` puts it on the row as a text action.
   */
  largeType?: string
  /** A Wi-Fi password keeps SHOW LARGE TYPE as a visible text action (brief I9). */
  largeTypeAction?: boolean
  /** The template's control, when the field is in one: how a plain value is drawn (yes / no, dates, paragraphs). */
  control?: Control
  /**
   * Shared with this person to fill only (docs/plan/18-sharing.md): a
   * hidden field offers no reveal and no copy, only the words. The core
   * refuses both anyway.
   */
  fillOnly?: boolean
  /** Field Row's layout. Default `default`. */
  size?: RowSize
  /** The phone's bottom-row eye and copy run this field's own reveal and copy (brief I1). */
  remote?: SecretFieldRemote
}

/** The value in groups of four, for a card number shown in the clear. */
function formatPlain(text: string, format: SecretFormat): string {
  if (format !== "groups-of-4") return text
  return text.replace(/\s+/g, "").replace(/(.{4})(?=.)/g, "$1 ")
}

/** The core's sentence for a failed reveal or copy, in place of the component's default words. */
const coreWords = (e: unknown): string | undefined => (isAppError(e) ? e.message : undefined)

/** How long a row's own COPY says "Copied" (cross-app §1). */
const COPIED_MS = 2000

/**
 * One field of an item, as a Field Row (brief I2). A plain field shows its
 * value in mono with COPY, a website with OPEN and COPY, a yes or no as a
 * word, a date as the frames write one, several lines as a paragraph.
 *
 * A hidden field is unicron's `SecretField` in the Field Row look, bound to
 * this host as a `SecretFieldReferenceHost`:
 *
 * - `getSecret` is the core's `reveal` (bytes, never a JSON string; the
 *   component decodes them for as long as they are drawn and zeroes them).
 *   Only a reveal the person asked for calls it;
 * - `host.copyRef` asks the core to copy by id and field (`copy_secret`),
 *   so the value goes core → clipboard on the Rust side (D85) and no
 *   bytes come to the webview for a copy. It resolves with the shell's
 *   number for that copy, which the component hands back to
 *   `host.clipboardHolds` at expiry; with `host.clear` that is the shell's
 *   own clipboard rule (`clipboard_holds`, then `clipboard_clear` with
 *   the same number), so "Cleared" is only ever said about, and the clear
 *   only ever runs for, our own copy;
 * - `errorText` is the core's sentence when a reveal or a copy fails;
 * - `revealMode` is the settings file's gesture (D97), `clipboardTimer`
 *   its timer, `locked` the core's lock state;
 * - `remote` lets the phone's bottom-row eye and copy run this field's own
 *   reveal and copy, with the same gate, hold, lifetime and timer;
 * - its ⋯ holds "Show large type" first (D242): unicron's LargeTypeView,
 *   fed by the same `reveal`, capture-protected while it is up.
 *
 * A standard field needs no gate (the core gates nothing on it); inside a
 * `CrownJewelSurface` the surface gates use and reveal (`crown-jewel.tsx`)
 * and the component makes the reveal a tap with the crown-jewel lifetime
 * on its own (a hold cannot outlive the gate's dialog or the OS prompt).
 */
export function ItemField({ itemId, field, onNotice: _onNotice, format = "plain", largeType, largeTypeAction = false, control, fillOnly = false, size = "default", remote }: ItemFieldProps) {
  const client = useClient()
  const gate = useGate()
  const { settings } = useSettings()
  const locked = useLocked()
  const [error, setError] = useState<string | null>(null)
  const [copied, setCopied] = useState(false)
  const [big, setBig] = useState(false)
  const fieldId = field.field_id

  // The shell's number for this field's last copy: the clear at the countdown's end names it, so it
  // runs only while that copy is still the shell's last write (clipboard_clear; D105, D115).
  const lastCopy = useRef<number | null>(null)

  // A reveal keeps the window out of screen captures for its lifetime (docs/plan/26-desktop-finish.md §6).
  const hygiene = useHygieneClient()
  const getSecret = useMemo(
    () => async () => {
      const bytes = await gate.run((g) => client.reveal(itemId, fieldId, g))
      protectFor(hygiene)
      return bytes
    },
    [client, gate, itemId, fieldId, hygiene],
  )
  // Large type reads through the same gate; the window stays capture-protected while it is up.
  // Held by `run`, which stays the same while the gate asks: a new function would make the viewer read again
  // once the gate closes, with no proof left, and ask a second time.
  const gateRun = gate.run
  const getLarge = useCallback(() => gateRun((g) => client.reveal(itemId, fieldId, g)), [client, gateRun, itemId, fieldId])
  // Every layer closes on lock (brief 4.0).
  const bigOpen = big && !locked
  useCaptureProtect(bigOpen)

  const host = useMemo<SecretFieldReferenceHost>(
    () => ({
      copyRef: async () => {
        const { copy } = await gate.run((g) => client.copySecret(itemId, fieldId, settings.clipboard_clear_secs, g))
        lastCopy.current = copy
        return { hash: String(copy) }
      },
      clear: async () => {
        if (lastCopy.current !== null) await client.clipboardClear(lastCopy.current)
      },
      clipboardHolds: (hash) => {
        const copy = Number(hash)
        return Number.isSafeInteger(copy) ? client.clipboardHolds(copy) : Promise.resolve(false)
      },
    }),
    [client, gate, itemId, fieldId, settings.clipboard_clear_secs],
  )

  useEffect(() => {
    if (!copied) return
    const t = setTimeout(() => setCopied(false), COPIED_MS)
    return () => clearTimeout(t)
  }, [copied])

  if (field.hidden && fillOnly) {
    return <SecretField fillOnly label={field.label} size={size} remote={remote} />
  }
  if (field.hidden) {
    const inMenu = largeType && !largeTypeAction
    return (
      <>
        <SecretField
          label={field.label}
          size={size}
          getSecret={getSecret}
          host={host}
          revealMode={settings.reveal}
          clipboardTimer={settings.clipboard_clear_secs}
          format={format}
          locked={locked}
          errorText={coreWords}
          remote={remote}
          menu={
            inMenu ? (
              <AdaptiveMenuItem icon="text--scale" disabled={gate.asking} onSelect={() => setBig(true)}>
                Show large type
              </AdaptiveMenuItem>
            ) : undefined
          }
        >
          {largeType && largeTypeAction && <ShowFieldLargeType itemId={itemId} fieldId={fieldId} title={largeType} />}
        </SecretField>
        {inMenu && <LargeTypeView open={bigOpen} onOpenChange={setBig} getSecret={getLarge} title={largeType} groupSize={3} lifetime={LARGE_TYPE_LIFETIME_S} />}
      </>
    )
  }

  const raw = field.value ?? ""
  const boolean = control === "boolean" || field.kind === "boolean"
  const multiline = control === "multiline" || raw.includes("\n")
  const url = field.kind === "url" || control === "url"

  // Several lines: a paragraph under its label, as the frames draw a note (Mobile / 02 "Protected").
  if (multiline && !boolean) {
    return (
      <div className="flex flex-col gap-1 py-3" data-row="" aria-label={field.label} role="group">
        <span className="type-overline text-muted-foreground">{field.label}</span>
        <p className="type-body whitespace-pre-wrap text-support-foreground">{raw}</p>
      </div>
    )
  }

  const value = boolean ? (raw === "true" ? "Yes" : "No") : control === "date" || control === "month" ? dateWords(raw, control) : formatPlain(raw, format)
  const copyPlain = async () => {
    if (!raw) return
    setError(null)
    try {
      await navigator.clipboard.writeText(raw)
      setCopied(true)
    } catch (e) {
      setError(describeError(e))
    }
  }
  const open = () =>
    client
      .openSite(raw)
      .then(() => client.noteUse(itemId).catch(() => {}))
      .catch((e) => setError(describeError(e)))
  const copyAction = raw && !boolean && (
    <FieldRowAction onClick={() => void copyPlain()} aria-live="polite">
      {copied ? "Copied" : "Copy"}
    </FieldRowAction>
  )
  return (
    <FieldRow
      size={size}
      label={field.label}
      description={error ? <span role="alert">{error}</span> : undefined}
      action={
        url && raw ? (
          <>
            <FieldRowAction onClick={() => void open()}>Open</FieldRowAction>
            {copyAction}
          </>
        ) : (
          copyAction || undefined
        )
      }
    >
      {url ? <FieldRowValue>{siteWords(raw)}</FieldRowValue> : value}
    </FieldRow>
  )
}
