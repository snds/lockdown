import { Icon, ListRow } from "@unicron/ui"
import { CrownJewelMark } from "@unicron/ui/security/crown-jewel"

import type { ReactNode } from "react"

import { useSettings } from "@/app/settings"
import type { ItemSummary } from "@/core/types"

import { monoLine, rowWords } from "./list-model"

interface ItemRowProps {
  row: ItemSummary
  selected: boolean
  onSelect: (id: string) => void
  /**
   * Kept for callers. The row draws no copy button any more (brief V1, as
   * the frames draw it): copying from a list is ⌘K on a desktop and the
   * detail's COPY; the phone's swipe comes with the swipes slice.
   */
  onCopy?: (row: ItemSummary) => void
  /** The row's menu: `DropdownMenuItem`s, behind its ⋯ named "More for {title}", the Menu key, Shift+F10 and a right click. */
  actions?: ReactNode
  /** The right-hand words (`rowWords`); the row's own methods or kind when left out. */
  words?: string
  /** For a list that draws only the rows in view (brief X6): where this row sits in the whole list. */
  position?: { index: number; of: number }
}

/**
 * One row of an item list (brief V1): unicron's `ListRow`, inline, with
 * plain meta (92 px; 56 at the compact density). The title as written,
 * then its marks: the crown-jewel mark, a filled star for a favorite. Under
 * it the mono line (`monoLine`); at the right the methods or the kind, then
 * the state words after `//`. No kind glyph, no method glyphs and no copy
 * button. The title is the row's one button: it opens the item; the ⋯
 * beside it is a separate control.
 */
export function ItemRow({ row, selected, onSelect, actions, words, position }: ItemRowProps) {
  const { settings } = useSettings()
  const title = row.title || row.site || row.id
  const marks =
    row.crown_jewel || row.favourite ? (
      <>
        {row.crown_jewel && <CrownJewelMark size="sm" />}
        {row.favourite && <Icon name="star--filled" label="Favorite" />}
      </>
    ) : undefined
  return (
    <ListRow
      layout="inline"
      metaStyle="plain"
      size={settings.density === "compact" ? "compact" : "default"}
      render={<li aria-posinset={position ? position.index + 1 : undefined} aria-setsize={position?.of} />}
      selected={selected}
      onClick={() => onSelect(row.id)}
      title={title}
      marks={marks}
      meta={monoLine(row) || undefined}
      status={words ?? rowWords(row, { view: "all" })}
      menu={actions}
      menuLabel="More for"
    />
  )
}
