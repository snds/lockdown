import { useState } from "react"

import { Button, Icon, Item, ItemActions, ItemContent, ItemDescription, ItemTitle } from "@unicron/ui"

import { useGate } from "@/app/gate"
import { useSettings } from "@/app/settings"
import { describeError, useClient } from "@/core/client"
import type { FieldView } from "@/core/types"

import { ItemField } from "./item-field"

interface LinkedFieldProps {
  itemId: string
  field: FieldView
  onNotice: (text: string) => void
  fillOnly?: boolean
}

/**
 * A field that points at another item's field (D235). It names the other
 * item and its field, never shows a value of its own: a secret there is
 * revealed and copied through this item's field, which the core resolves
 * to the other item and gates as that item is gated (a link to a crown
 * jewel asks as the crown jewel does); a code copies the code valid now; a
 * plain value says where it lives. A link whose target is gone says so.
 */
export function LinkedField({ itemId, field, onNotice, fillOnly = false }: LinkedFieldProps) {
  const client = useClient()
  const gate = useGate()
  const { settings } = useSettings()
  const [error, setError] = useState<string | null>(null)
  const link = field.link
  const from = link && !link.missing ? `${link.label} from ${link.title || "another item"}` : ""

  if (!link || link.missing) {
    return (
      <Item variant="outline" size="sm" data-row="">
        <ItemContent>
          <ItemDescription>{field.label}</ItemDescription>
          <ItemTitle>
            <span className="text-muted-foreground">Points at an item or field that is gone (in the trash, or removed).</span>
          </ItemTitle>
        </ItemContent>
      </Item>
    )
  }
  if (link.secret && !link.code) {
    return <ItemField itemId={itemId} field={{ ...field, label: `${field.label} · ${from}`, hidden: true, value: null, mask: "••••••••" }} onNotice={onNotice} fillOnly={fillOnly} largeType={fillOnly ? undefined : `${field.label} · ${from}`} />
  }
  const copyCode = async () => {
    setError(null)
    try {
      await gate.run((g) => client.copyCode(itemId, field.field_id, settings.clipboard_clear_secs, "code", g))
      onNotice("Copied the code.")
    } catch (e) {
      setError(describeError(e))
    }
  }
  return (
    <Item variant="outline" size="sm" data-row="">
      <ItemContent>
        <ItemDescription>{field.label}</ItemDescription>
        <ItemTitle>
          <Icon name="link" data-icon="inline-start" />
          {from}
        </ItemTitle>
        {error && <ItemDescription role="alert">{error}</ItemDescription>}
      </ItemContent>
      {link.code && !fillOnly && (
        <ItemActions>
          <Button variant="ghost" size="sm" onClick={() => void copyCode()}>
            <Icon name="content_copy" data-icon="inline-start" />
            Copy code
          </Button>
        </ItemActions>
      )}
    </Item>
  )
}
