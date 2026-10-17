import { useMemo, useRef, useState } from "react"

import { Button, Icon, Item, ItemActions, ItemContent, ItemDescription, ItemTitle } from "@unicron/ui"
import { useCrownJewel } from "@unicron/ui/security/crown-jewel"
import { SecretField, type SecretFieldHost, type SecretFormat } from "@unicron/ui/security/secret-field"

import { useGate } from "@/app/gate"
import { useLocked } from "@/app/locked"
import { useSettings } from "@/app/settings"
import { describeError, useClient } from "@/core/client"
import type { Control, FieldView } from "@/core/types"

import { CROWN_JEWEL_REVEAL_S } from "./crown-jewel"
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
}

function formatPlain(text: string, format: SecretFormat): string {
  if (format !== "groups-of-4") return text
  return text.replace(/\s+/g, "").replace(/(.{4})(?=.)/g, "$1 ")
}

/**
 * One field of an item. A plain field shows its value with Copy or Open.
 * A hidden field is unicron's `SecretField`, bound to this host:
 *
 * - `getSecret` is the core's `reveal` (bytes, never a JSON string; the
 *   component decodes them for as long as they are drawn and zeroes them);
 * - `host.copy` ignores the bytes it is handed (zeroed at once) and asks
 *   the core to copy by id and field (`copy_secret`), so the value goes
 *   core → clipboard on the Rust side as before (D85); the component's
 *   copy path still fetches the bytes first, which is a contract gap
 *   recorded in 10-desktop-shell §3;
 * - `host.clipboardHolds` and `host.clear` are the shell's own clipboard
 *   rule (`clipboard_holds`, `clipboard_clear`), so "Cleared" is only ever
 *   said about our own value;
 * - `revealMode` is the settings file's gesture (D97), `clipboardTimer`
 *   its timer, `locked` the core's lock state.
 *
 * A standard field needs no gate (the core gates nothing on it); inside a
 * `CrownJewelSurface` the surface gates use and reveal (`crown-jewel.tsx`),
 * the reveal is a tap with the crown-jewel lifetime (a hold cannot
 * outlive the gate's dialog or the OS prompt), and a copy fetches nothing
 * for the webview: the core copies by reference after its own `Use` gate.
 */
export function ItemField({ itemId, field, onNotice, format = "plain", largeType, control }: ItemFieldProps) {
  const client = useClient()
  const gate = useGate()
  const { settings } = useSettings()
  const locked = useLocked()
  const crown = useCrownJewel()
  const [error, setError] = useState<string | null>(null)
  /** True while the last copy went by reference (the component never had the bytes). */
  const byReference = useRef(false)
  const fieldId = field.field_id

  const getSecret = useMemo(
    () => async () => {
      setError(null)
      if (gate.pendingProof() === "use") {
        // A crown jewel's copy: the core copies by reference after its own
        // `Use` gate; nothing is fetched for the webview.
        byReference.current = true
        return new Uint8Array()
      }
      byReference.current = false
      try {
        return await gate.run((g) => client.reveal(itemId, fieldId, g))
      } catch (e) {
        setError(describeError(e))
        throw e
      }
    },
    [client, gate, itemId, fieldId],
  )

  const host = useMemo<SecretFieldHost>(
    () => ({
      copy: async (bytes) => {
        bytes.fill(0)
        try {
          await gate.run((g) => client.copySecret(itemId, fieldId, settings.clipboard_clear_secs, g))
        } catch (e) {
          setError(describeError(e))
          throw e
        }
      },
      clear: () => client.clipboardClear(),
      clipboardHolds: (hash) => client.clipboardHolds(byReference.current ? null : hash),
    }),
    [client, gate, itemId, fieldId, settings.clipboard_clear_secs],
  )

  if (field.hidden) {
    return (
      <SecretField
        label={field.label}
        getSecret={getSecret}
        host={host}
        revealMode={crown.gated ? "toggle" : settings.reveal}
        revealLifetime={crown.gated ? CROWN_JEWEL_REVEAL_S : undefined}
        clipboardTimer={settings.clipboard_clear_secs}
        format={format}
        locked={locked}
        description={error ?? undefined}
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
          <Button variant="ghost" size="sm" onClick={() => client.openSite(field.value!).catch((e) => setError(describeError(e)))}>
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
