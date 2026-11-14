import { useState } from "react"

import { Badge, FieldRow, FieldRowAction, FieldRowValue, Icon } from "@unicron/ui"

import { useGate } from "@/app/gate"
import { useSettings } from "@/app/settings"
import { describeError, useClient } from "@/core/client"
import type { FieldView } from "@/core/types"

import { ItemField, type RowSize } from "./item-field"

interface LinkedFieldProps {
  itemId: string
  field: FieldView
  onNotice: (text: string) => void
  fillOnly?: boolean
  /** Field Row's layout. */
  size?: RowSize
}

/**
 * A field that points at another item's field (D235), as a Field Row
 * (brief I3). It names the other item and its field, never shows a value
 * of its own: a secret there is a `SecretField` that reveals and copies
 * through this item's field, which the core resolves to the other item and
 * gates as that item is gated (a link to a crown jewel asks as the crown
 * jewel does); a code copies the code valid now (COPY CODE); a plain value
 * says where it lives. A link whose target is gone says so, with BROKEN.
 */
export function LinkedField({ itemId, field, onNotice, fillOnly = false, size = "default" }: LinkedFieldProps) {
  const client = useClient()
  const gate = useGate()
  const { settings } = useSettings()
  const [error, setError] = useState<string | null>(null)
  const [copied, setCopied] = useState(false)
  const link = field.link
  const from = link && !link.missing ? `${link.label} from ${link.title || "another item"}` : ""

  if (!link || link.missing) {
    return (
      <FieldRow size={size} label={field.label} badge={<Badge tone="critical">Broken</Badge>}>
        <FieldRowValue truncate="wrap">Points at an item or field that is gone.</FieldRowValue>
      </FieldRow>
    )
  }
  if (link.secret && !link.code) {
    return (
      <ItemField
        itemId={itemId}
        field={{ ...field, label: `${field.label} · ${from}`, hidden: true, value: null, mask: "••••••••" }}
        onNotice={onNotice}
        fillOnly={fillOnly}
        size={size}
        largeType={fillOnly ? undefined : `${field.label} · ${from}`}
      />
    )
  }
  const copyCode = async () => {
    setError(null)
    try {
      await gate.run((g) => client.copyCode(itemId, field.field_id, settings.clipboard_clear_secs, "code", g))
      setCopied(true)
      setTimeout(() => setCopied(false), 2000)
    } catch (e) {
      setError(describeError(e))
    }
  }
  return (
    <FieldRow
      size={size}
      label={field.label}
      description={error ? <span role="alert">{error}</span> : undefined}
      action={
        link.code && !fillOnly ? (
          <FieldRowAction onClick={() => void copyCode()} aria-live="polite">
            {copied ? "Copied" : "Copy code"}
          </FieldRowAction>
        ) : undefined
      }
    >
      <span className="inline-flex items-center gap-2">
        <Icon name="link" size="sm" />
        {from}
      </span>
    </FieldRow>
  )
}
