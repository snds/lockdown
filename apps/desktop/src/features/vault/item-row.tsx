import { Badge, Icon, Item, ItemContent, ItemDescription, ItemMedia, ItemTitle, type IconName } from "@unicron/ui"

import type { ItemSummary } from "@/core/types"

import { kindIcon } from "./list-model"

interface ItemRowProps {
  row: ItemSummary
  selected: boolean
  onSelect: (id: string) => void
}

/** The three sign-in methods a Login row can carry (brief §1 promise A). */
const METHODS: Record<string, [IconName, string]> = {
  password: ["password", "password"],
  passkey: ["passkey", "passkey"],
  code: ["timer", "one-time code"],
}

/**
 * One list row: kind glyph, title, subtitle, the Login's method glyphs
 * (each with its word for assistive tech), and the crown-jewel mark with
 * its word (brief §6.5: never an icon alone). Rows take the density knobs
 * through `data-row`.
 */
export function ItemRow({ row, selected, onSelect }: ItemRowProps) {
  const subtitle = [row.username, row.site].filter(Boolean).join(" · ")
  return (
    <Item
      role="listitem"
      size="sm"
      render={<button type="button" />}
      data-row=""
      data-selected={selected ? "true" : undefined}
      aria-current={selected ? "true" : undefined}
      onClick={() => onSelect(row.id)}
    >
      <ItemMedia variant="icon">
        <Icon name={kindIcon(row.kind)} />
      </ItemMedia>
      <ItemContent>
        <ItemTitle>
          <span className="truncate">{row.title || row.site || row.id}</span>
          {row.crown_jewel && (
            <Badge variant="outline">
              <Icon name="lock" data-icon="inline-start" />
              Protected
            </Badge>
          )}
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
    </Item>
  )
}
