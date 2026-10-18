import { useMemo, useRef, useState } from "react"

import { Button, Icon, Item, ItemActions, ItemContent, ItemDescription, ItemTitle } from "@unicron/ui"
import { SecretField, type SecretFieldReferenceHost, type SecretFormat } from "@unicron/ui/security/secret-field"

import { useGate } from "@/app/gate"
import { useLocked } from "@/app/locked"
import { useSettings } from "@/app/settings"
import { describeError, isAppError, useClient } from "@/core/client"
import type { Control, FieldView } from "@/core/types"

import { protectFor, useHygieneClient } from "@/features/hygiene/hygiene-client"

import { ShowFieldLargeType } from "./large-type"

export type { SecretFormat }

interface ItemFieldProps {
  itemId: string
  field: FieldView
  /** A plain copy's toast (hidden values announce themselves through the component). */
  onNotice: (text: string) => void
  /** How a value is drawn: as is, or in groups of four (cards, brief §3.1). */
  format?: SecretFormat
  /** Offer "Show large type" for this hidden field, named so ("Wi-Fi password · Home"). */
  largeType?: string
  /** The template's control, when the field is in one: how a plain value is drawn (yes / no, paragraphs). */
  control?: Control
  /**
   * Shared with this person to fill only (docs/plan/18-sharing.md): a
   * hidden field offers no reveal and no copy, only the words. The core
   * refuses both anyway.
   */
  fillOnly?: boolean
}

function formatPlain(text: string, format: SecretFormat): string {
  if (format !== "groups-of-4") return text
  return text.replace(/\s+/g, "").replace(/(.{4})(?=.)/g, "$1 ")
}

/** The core's sentence for a failed reveal or copy, in place of the component's default words. */
const coreWords = (e: unknown): string | undefined => (isAppError(e) ? e.message : undefined)

/**
 * One field of an item. A plain field shows its value with Copy or Open.
 * A hidden field is unicron's `SecretField`, bound to this host as a
 * `SecretFieldReferenceHost` (@unicron/ui 0.7.0):
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
 *   its timer, `locked` the core's lock state.
 *
 * A standard field needs no gate (the core gates nothing on it); inside a
 * `CrownJewelSurface` the surface gates use and reveal (`crown-jewel.tsx`)
 * and the component makes the reveal a tap with the crown-jewel lifetime
 * on its own (a hold cannot outlive the gate's dialog or the OS prompt).
 */
export function ItemField({ itemId, field, onNotice, format = "plain", largeType, control, fillOnly = false }: ItemFieldProps) {
  const client = useClient()
  const gate = useGate()
  const { settings } = useSettings()
  const locked = useLocked()
  const [error, setError] = useState<string | null>(null)
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

  if (field.hidden && fillOnly) {
    return (
      <Item variant="outline" size="sm" data-row="">
        <ItemContent>
          <ItemDescription>{field.label}</ItemDescription>
          <ItemTitle>
            <span className="text-muted-foreground">Fills in the browser only; never shown or copied.</span>
          </ItemTitle>
        </ItemContent>
      </Item>
    )
  }
  if (field.hidden) {
    return (
      <SecretField
        label={field.label}
        getSecret={getSecret}
        host={host}
        revealMode={settings.reveal}
        clipboardTimer={settings.clipboard_clear_secs}
        format={format}
        locked={locked}
        errorText={coreWords}
      >
        {largeType && <ShowFieldLargeType itemId={itemId} fieldId={fieldId} title={largeType} />}
      </SecretField>
    )
  }

  const boolean = control === "boolean" || field.kind === "boolean"
  const multiline = control === "multiline" || (field.value ?? "").includes("\n")
  const value = boolean ? (field.value === "true" ? "Yes" : "No") : formatPlain(field.value ?? "", format)
  const copyPlain = async () => {
    if (!field.value) return
    try {
      await navigator.clipboard.writeText(field.value)
      onNotice("Copied.")
    } catch (e) {
      setError(describeError(e))
    }
  }
  return (
    <Item variant="outline" size="sm" data-row="">
      <ItemContent>
        <ItemDescription>{field.label}</ItemDescription>
        <ItemTitle>{multiline ? <span className="whitespace-pre-wrap">{value}</span> : boolean ? value : <code>{value}</code>}</ItemTitle>
        {error && <ItemDescription role="alert">{error}</ItemDescription>}
      </ItemContent>
      <ItemActions>
        {field.kind === "url" && field.value ? (
          <Button variant="ghost" size="sm" onClick={() =>
              client
                .openSite(field.value!)
                .then(() => client.noteUse(itemId).catch(() => {}))
                .catch((e) => setError(describeError(e)))
            }
          >
            <Icon name="open_in_new" data-icon="inline-start" />
            Open
          </Button>
        ) : (
          field.value &&
          !boolean && (
            <Button variant="ghost" size="sm" onClick={copyPlain}>
              <Icon name="content_copy" data-icon="inline-start" />
              Copy
            </Button>
          )
        )}
      </ItemActions>
    </Item>
  )
}
