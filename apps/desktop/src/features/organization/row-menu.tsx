import { DropdownMenuGroup, DropdownMenuItem, DropdownMenuLabel, DropdownMenuSeparator, Icon } from "@unicron/ui"

import { useGate } from "@/app/gate"
import { archivedWords, deletedWords, notify } from "@/app/notify"
import { describeError, useClient } from "@/core/client"
import type { ItemSummary } from "@/core/types"

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
export function RowMenuItems({ row, onOrganize, onChanged, onError, onHistory, onPurge }: { row: ItemSummary } & RowMenuHandlers) {
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
        <DropdownMenuItem onClick={() => void run(client.restore(row.id), `Restored ${title}`)}>
          <Icon name="undo" />
          Restore
        </DropdownMenuItem>
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
      <DropdownMenuItem onClick={() => void run(client.organize(row.id, { favourite: !row.favourite }), row.favourite ? `Removed ${title} from favorites` : `Added ${title} to favorites`)}>
        <Icon name={row.favourite ? "star--filled" : "star"} />
        {row.favourite ? "Remove from favorites" : "Add to favorites"}
      </DropdownMenuItem>
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
        <DropdownMenuItem onClick={() => void run(client.unarchive(row.id), `Unarchived ${title}`)}>
          <Icon name="reset" />
          Unarchive
        </DropdownMenuItem>
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
