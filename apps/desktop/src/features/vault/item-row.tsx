import { Badge, Icon, IconButton, Item, ItemActions, ItemContent, ItemDescription, ItemMain, ItemMedia, ItemTitle, type IconName } from "@unicron/ui"
import { CrownJewelMark } from "@unicron/ui/security/crown-jewel"

import type { ItemSummary } from "@/core/types"

import { kindIcon } from "./list-model"

interface ItemRowProps {
  row: ItemSummary
  selected: boolean
  onSelect: (id: string) => void
  /** The row's own copy button (password, code, Wi-Fi password, card number), through the core. */
  onCopy?: (row: ItemSummary) => void
}

/** The three sign-in methods a Login row can carry (brief §1 promise A). */
const METHODS: Record<string, [IconName, string]> = {
  password: ["password", "password"],
  passkey: ["passkey", "passkey"],
  code: ["timer", "one-time code"],
}

/**
 * One list row: kind glyph, title, subtitle, the Login's method glyphs
 * (each with its word for assistive tech), and unicron's CrownJewelMark
 * (a filled lock with "Crown jewel" for assistive tech). The row's main
 * action is unicron's `ItemMain` (it opens the item), so the row's own
 * copy button stays a separate control beside it. Rows take the density
 * knobs through `data-row`.
 */
export function ItemRow({ row, selected, onSelect, onCopy }: ItemRowProps) {
  const subtitle = row.subtitle || [row.username, row.site].filter(Boolean).join(" · ")
  const copy = onCopy && row.copy && row.state !== "trashed" ? row.copy : null
  return (
    <Item role="listitem" size="sm" data-row="" data-selected={selected ? "true" : undefined}>
      <ItemMain onClick={() => onSelect(row.id)} aria-current={selected ? "true" : undefined}>
        <ItemMedia variant="icon">
          <Icon name={kindIcon(row.kind)} />
        </ItemMedia>
        <ItemContent>
          <ItemTitle>
            <span className="truncate">{row.title || row.site || row.id}</span>
            {row.crown_jewel && <CrownJewelMark size="sm" />}
            {row.conflict_copy && <Badge variant="secondary">Conflict copy</Badge>}
            {row.state === "trashed" && <Badge variant="secondary">In the trash</Badge>}
          </ItemTitle>
          {(subtitle || row.methods.length > 0) && (
            <ItemDescription>
              {row.methods.length > 0 && (
                <span className="mr-2 inline-flex items-center gap-1 align-middle">
                  {row.methods.map((m) => {
                    const entry = METHODS[m]
                    return entry ? <Icon key={m} name={entry[0]} label={entry[1]} /> : null
                  })}
                </span>
              )}
              {subtitle}
            </ItemDescription>
          )}
        </ItemContent>
      </ItemMain>
      {copy && (
        <ItemActions>
          <IconButton label={`Copy ${copy.label}`} icon="content_copy" size="icon-sm" onClick={() => onCopy?.(row)} />
        </ItemActions>
      )}
    </Item>
  )
}
