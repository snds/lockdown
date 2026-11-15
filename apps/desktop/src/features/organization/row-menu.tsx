import { DropdownMenuGroup, DropdownMenuItem, DropdownMenuLabel, DropdownMenuSeparator, Icon, type SwipeRowAction } from "@unicron/ui"

import { useGate } from "@/app/gate"
import { archivedWords, deletedWords, notify } from "@/app/notify"
import { describeError, useClient } from "@/core/client"
import type { ItemSummary } from "@/core/types"
import { useCodeCopy, useQuickCopy } from "@/features/vault/quick-copy"

import { rowMenuOffers } from "./org-model"
import { useVaults } from "./vaults-context"

/** What a row's menu tells its list. */
export interface RowMenuHandlers {
  /** Opens the organize prompt (folder, tags, favorite) for this row. */
  onOrganize: (row: ItemSummary) => void
  /** Something changed: reload. `moved` is the item's new id after a move. The words are a toast's. */
  onChanged: (notice: string, moved?: string) => void
  onError: (text: string) => void
  /** Opens the item's history (one dialog for the whole list: `useRowMenuLayers`). */
  onHistory: (row: ItemSummary) => void
  /** Asks before an item in Recently deleted goes for good (`useRowMenuLayers`). */
  onPurge: (row: ItemSummary) => void
  /**
   * The row swipes (a phone, `useRowSwipe`), and unicron's `ListRow` lists
   * the swipe's actions at the top of this same menu: the items they repeat
   * (favorite, unarchive, restore) are left out here, so none shows twice.
   */
  swiped?: boolean
}

/**
 * A row's own menu (brief V1): the items behind the row's ⋯, its right
 * click, the Menu key and Shift+F10, which unicron's `ListRow` opens. A
 * live or archived item: Add to favorites, Folder and tags…, Move to vault,
 * Archive, History, Move to Recently deleted. An item in Recently deleted:
 * Restore, History and Delete for good. Nothing for a right that may not
 * change the item (view, fill only), as the detail's ⋯. Share… waits for
 * `ItemShare` to open from a menu (edges-8). Each is one call to the core;
 * a crown jewel's move asks it's you first through the gate. Moving to
 * Recently deleted and archiving say so in a toast with UNDO (Q14).
 */
export function RowMenuItems({ row, onOrganize, onChanged, onError, onHistory, onPurge, swiped = false }: { row: ItemSummary } & RowMenuHandlers) {
  const client = useClient()
  const gate = useGate()
  const { vaults } = useVaults()
  const title = row.title || "this item"
  const others = vaults.filter((v) => v.id !== row.vault_id)
  const archived = row.state === "archived"
  const run = (p: Promise<unknown>, notice: string) =>
    p.then((r) => onChanged(notice, typeof r === "string" ? r : undefined)).catch((e) => onError(describeError(e)))
  // Undoable, so it asks nothing: the toast offers UNDO, which puts it back (cross-app decisions §10).
  const undoable = (p: Promise<unknown>, words: string, undo: () => Promise<unknown>, undone: string) =>
    p
      .then(() => {
        notify(words, { action: { label: "Undo", onClick: () => void undo().then(() => onChanged(undone)).catch((e) => onError(describeError(e))) } })
        onChanged("")
      })
      .catch((e) => onError(describeError(e)))

  if (!rowMenuOffers(row)) return null
  if (row.state === "trashed") {
    return (
      <>
        {!swiped && (
          <DropdownMenuItem onClick={() => void run(client.restore(row.id), `Restored ${title}`)}>
            <Icon name="undo" />
            Restore
          </DropdownMenuItem>
        )}
        <DropdownMenuItem onClick={() => onHistory(row)}>
          <Icon name="recently-viewed" />
          History
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        <DropdownMenuItem variant="destructive" onClick={() => onPurge(row)}>
          <Icon name="trash-can" />
          Delete for good
        </DropdownMenuItem>
      </>
    )
  }
  return (
    <>
      {/* An archived row's swipe has no FAVORITE, so its menu keeps the item. */}
      {!(swiped && !archived) && (
        <DropdownMenuItem onClick={() => void run(client.organize(row.id, { favourite: !row.favourite }), favouriteWords(row))}>
          <Icon name={row.favourite ? "star--filled" : "star"} />
          {row.favourite ? "Remove from favorites" : "Add to favorites"}
        </DropdownMenuItem>
      )}
      {!archived && (
        <DropdownMenuItem onClick={() => onOrganize(row)}>
          <Icon name="tag" />
          Folder and tags…
        </DropdownMenuItem>
      )}
      {others.length > 0 && (
        // A group, not a submenu: the same rows wherever the menu opens, a phone's narrow column included.
        <DropdownMenuGroup>
          <DropdownMenuLabel>Move to vault</DropdownMenuLabel>
          {others.map((v) => (
            <DropdownMenuItem key={v.id} onClick={() => void run(gate.run((g) => client.moveItem(row.id, v.id, g)), `Moved ${title} to ${v.name}`)}>
              <Icon name="folder--move-to" />
              {v.name}
            </DropdownMenuItem>
          ))}
        </DropdownMenuGroup>
      )}
      {archived ? (
        !swiped && (
          <DropdownMenuItem onClick={() => void run(client.unarchive(row.id), `Unarchived ${title}`)}>
            <Icon name="reset" />
            Unarchive
          </DropdownMenuItem>
        )
      ) : (
        <DropdownMenuItem onClick={() => void undoable(client.archive(row.id), archivedWords(title), () => client.unarchive(row.id), `Unarchived ${title}`)}>
          <Icon name="archive" />
          Archive
        </DropdownMenuItem>
      )}
      <DropdownMenuItem onClick={() => onHistory(row)}>
        <Icon name="recently-viewed" />
        History
      </DropdownMenuItem>
      <DropdownMenuSeparator />
      <DropdownMenuItem variant="destructive" onClick={() => void undoable(client.trash(row.id), deletedWords(title), () => client.restore(row.id), `Restored ${title}`)}>
        <Icon name="trash-can" />
        Move to Recently deleted
      </DropdownMenuItem>
    </>
  )
}

/** A favorite's toast: "Added Steam to favorites", "Removed Steam from favorites". */
function favouriteWords(row: ItemSummary): string {
  const title = row.title || "this item"
  return row.favourite ? `Removed ${title} from favorites` : `Added ${title} to favorites`
}

/** What a phone row's swipe tells its list: the row menu's own, plus where a copy's toast goes. */
export interface RowSwipeHandlers {
  /** Something changed: reload. The words are a toast's. */
  onChanged: (notice: string) => void
  onError: (text: string) => void
  /** A copy's words (X2: "Copied the password for Steam, clears in 30 s"), or why nothing was copied. */
  onNotice: (text: string) => void
}

/** The tiles a row offers, by the brief V1 swipe table: names only, so a test can read them without a client. */
export interface SwipeOffer {
  leading: "favorite" | "unfavorite" | null
  /** From the row outwards; the last is the outermost, the one a full swipe runs. */
  trailing: ("more" | "code" | "copy" | "unarchive" | "restore")[]
  /** A full swipe runs the outermost tile (it is reversible: a copy, unarchive, restore). */
  full: boolean
}

/**
 * What a phone row's swipe offers (brief V1's table, rules.md 7.1, contract
 * 6.4). A live item: MORE, CODE when it holds codes, then its quick copy
 * (outermost; a full swipe runs it); FAVORITE or UNFAVORITE on the right.
 * Nothing to copy: MORE only, and no full swipe. Archived: MORE, UNARCHIVE.
 * Recently deleted: MORE, RESTORE, never a copy. A fill-only share: no
 * swipe at all. A view-only share has no menu, so no MORE and no FAVORITE:
 * only its copies. A crown jewel offers the same tiles: the copy goes
 * through the gate, which asks before any value moves.
 */
export function swipeOffer(row: ItemSummary): SwipeOffer {
  const none: SwipeOffer = { leading: null, trailing: [], full: false }
  if (row.access === "fill-only") return none
  const menu = rowMenuOffers(row)
  const more = menu ? (["more"] as const) : []
  if (row.state === "trashed") return menu ? { leading: null, trailing: [...more, "restore"], full: true } : none
  if (row.state === "archived") return menu ? { leading: null, trailing: [...more, "unarchive"], full: true } : none
  // A code item's quick copy is its code: one CODE tile, not two.
  const code = row.codes > 0 && row.copy?.how !== "code" ? (["code"] as const) : []
  const copy = row.copy ? (["copy"] as const) : []
  return { leading: menu ? (row.favourite ? "unfavorite" : "favorite") : null, trailing: [...more, ...code, ...copy], full: copy.length > 0 }
}

/** "password" → "Password": the tile sets the capitals, the menu shows it as written. */
const sentence = (s: string) => s.charAt(0).toUpperCase() + s.slice(1)

/**
 * A phone row's swipe actions (unicron's `SwipeRow`), built from
 * `swipeOffer`. Each runs the same call as its row-menu item, with the same
 * toast. MORE opens the row's own menu (`openMenu`). A copy is declared
 * reversible (it moves nothing for good) and runs `useQuickCopy`, which goes
 * through the core's gate: the swipe alone never releases a value. `null`
 * when the row does not swipe (no handlers, or nothing to offer).
 */
export function useRowSwipe(row: ItemSummary, handlers: RowSwipeHandlers | null, openMenu: () => void): { leading?: SwipeRowAction; trailing: SwipeRowAction[] } | null {
  const client = useClient()
  const noop = () => {}
  const quickCopy = useQuickCopy(handlers?.onNotice ?? noop)
  const codeCopy = useCodeCopy(handlers?.onNotice ?? noop)
  const offer = swipeOffer(row)
  const { onChanged, onError } = handlers ?? { onChanged: noop, onError: noop }
  const title = row.title || "this item"
  if (!handlers || (offer.trailing.length === 0 && !offer.leading)) return null
  const run = (p: Promise<unknown>, notice: string) => void p.then(() => onChanged(notice)).catch((e) => onError(describeError(e)))
  const tile = (name: SwipeOffer["trailing"][number]): SwipeRowAction => {
    switch (name) {
      case "more":
        return { label: "More", icon: "overflow-menu--horizontal", tone: "neutral", onAction: openMenu }
      case "code":
        return { label: "Code", icon: "time", tone: "neutral", reversible: true, onAction: () => void codeCopy(row) }
      case "copy":
        return { label: sentence(row.copy?.label ?? "copy"), icon: "copy", reversible: true, onAction: () => void quickCopy(row) }
      case "unarchive":
        return { label: "Unarchive", icon: "reset", reversible: true, onAction: () => run(client.unarchive(row.id), `Unarchived ${title}`) }
      case "restore":
        return { label: "Restore", icon: "undo", reversible: true, onAction: () => run(client.restore(row.id), `Restored ${title}`) }
    }
  }
  const leading: SwipeRowAction | undefined = offer.leading
    ? {
        label: offer.leading === "favorite" ? "Favorite" : "Unfavorite",
        icon: offer.leading === "favorite" ? "star" : "star--filled",
        reversible: true,
        onAction: () => run(client.organize(row.id, { favourite: offer.leading === "favorite" }), favouriteWords(row)),
      }
    : undefined
  // A row with nothing to copy has no full swipe: its outermost tile (MORE) only opens.
  return { leading, trailing: offer.trailing.map(tile) }
}
