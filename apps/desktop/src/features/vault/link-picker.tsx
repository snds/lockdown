import { useEffect, useState } from "react"

import { Field, FieldDescription, FieldError, FieldLabel, NativeSelect, NativeSelectOption } from "@unicron/ui"

import { describeError, useClient } from "@/core/client"
import type { LinkTargetView } from "@/core/types"

import { linkText, parseLink } from "./depth-model"

interface LinkPickerProps {
  id: string
  label: string
  /** The stored `<item>#<field>`, or empty. */
  value: string
  onChange: (value: string) => void
  /** Links stay inside one vault. */
  vaultId: string
  /** The item being edited, which can't link to itself. */
  except?: string
  required?: boolean
}

/**
 * A field that points at another item's field (D235): pick the item, then
 * one of its fields, by title and label only. The core reads the value
 * from there when it is used, through that item's gates, so a link to a
 * crown jewel asks as the crown jewel does.
 */
export function LinkPicker({ id, label, value, onChange, vaultId, except, required }: LinkPickerProps) {
  const client = useClient()
  const [targets, setTargets] = useState<LinkTargetView[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const chosen = parseLink(value)

  useEffect(() => {
    if (!vaultId) return
    let live = true
    client
      .linkTargets(vaultId, except)
      .then((t) => live && setTargets(t))
      .catch((e) => live && setError(describeError(e)))
    return () => {
      live = false
    }
  }, [client, vaultId, except])

  const item = targets?.find((t) => t.item_id === chosen.itemId)
  return (
    <Field>
      <FieldLabel htmlFor={`${id}-item`}>{label}</FieldLabel>
      <NativeSelect id={`${id}-item`} aria-label={`${label}: the item`} value={chosen.itemId} required={required} onChange={(e) => onChange(linkText(e.target.value, targets?.find((t) => t.item_id === e.target.value)?.fields[0]?.field_id ?? ""))}>
        <NativeSelectOption value="">{targets ? "Choose an item" : "Loading the items…"}</NativeSelectOption>
        {(targets ?? []).map((t) => (
          <NativeSelectOption key={t.item_id} value={t.item_id}>
            {t.title || t.kind}
          </NativeSelectOption>
        ))}
      </NativeSelect>
      {item && (
        <NativeSelect id={`${id}-field`} aria-label={`${label}: the field`} value={chosen.fieldId} onChange={(e) => onChange(linkText(item.item_id, e.target.value))}>
          {item.fields.map((f) => (
            <NativeSelectOption key={f.field_id} value={f.field_id}>
              {f.label}
              {f.secret ? " (secret)" : ""}
            </NativeSelectOption>
          ))}
        </NativeSelect>
      )}
      <FieldDescription>Shows that item's value here; using it asks what that item asks.</FieldDescription>
      {error && <FieldError role="alert">{error}</FieldError>}
    </Field>
  )
}
