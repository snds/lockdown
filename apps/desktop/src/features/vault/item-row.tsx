import { Icon, ListRow, SwipeRow } from "@unicron/ui"
import { CrownJewelMark } from "@unicron/ui/security/crown-jewel"

import { useCallback, useRef, type ReactNode } from "react"

import { useSettings } from "@/app/settings"
import type { ItemSummary } from "@/core/types"
import { useRowSwipe, type RowSwipeHandlers } from "@/features/organization/row-menu"

import { monoLine, rowWords } from "./list-model"

interface ItemRowProps {
  row: ItemSummary
  selected: boolean
  onSelect: (id: string) => void
  /**
   * Kept for callers. The row draws no copy button any more (brief V1, as
   * the frames draw it): copying from a list is ⌘K on a desktop, the
   * detail's COPY, and a phone row's swipe (`swipe`).
   */
  onCopy?: (row: ItemSummary) => void
  /** The row's menu: `DropdownMenuItem`s, behind its ⋯ named "More for {title}", the Menu key, Shift+F10 and a right click. */
  actions?: ReactNode
  /** The right-hand words (`rowWords`); the row's own methods or kind when left out. */
  words?: string
  /** For a list that draws only the rows in view (brief X6): where this row sits in the whole list. */
  position?: { index: number; of: number }
  /**
   * A phone's list (brief V1, States / 01): the row swipes, with the
   * actions `swipeOffer` names. Every one of them is also at the top of
   * the row's menu (WCAG 2.5.7). Left out, the row does not swipe.
   */
  swipe?: RowSwipeHandlers | null
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
export function ItemRow({ row, selected, onSelect, actions, words, position, swipe = null }: ItemRowProps) {
  const { settings } = useSettings()
  const item = useRef<HTMLLIElement>(null)
  // MORE opens the row's own menu, as its right click does. After the tap's own events, when the row has closed.
  const openMenu = useCallback(() => {
    window.setTimeout(() => {
      item.current?.querySelector('[data-slot="swipe-row-content"] > *')?.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }))
    }, 0)
  }, [])
  const swipes = useRowSwipe(row, swipe, openMenu)
  const title = row.title || row.site || row.id
  const marks =
    row.crown_jewel || row.favourite ? (
      <>
        {row.crown_jewel && <CrownJewelMark size="sm" />}
        {row.favourite && <Icon name="star--filled" label="Favorite" />}
      </>
    ) : undefined
  const posinset = position ? position.index + 1 : undefined
  const listRow = (
    <ListRow
      layout="inline"
      metaStyle="plain"
      size={settings.density === "compact" ? "compact" : "default"}
      render={swipes ? <div /> : <li aria-posinset={posinset} aria-setsize={position?.of} />}
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
  if (!swipes) return listRow
  // The list item holds the swipe; the row inside it is the same ListRow.
  return (
    <li ref={item} aria-posinset={posinset} aria-setsize={position?.of}>
      <SwipeRow leading={swipes.leading} trailing={swipes.trailing}>
        {listRow}
      </SwipeRow>
    </li>
  )
}
