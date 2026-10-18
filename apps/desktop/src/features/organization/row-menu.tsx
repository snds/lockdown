import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
  Icon,
  IconButton,
} from "@unicron/ui"

import { useGate } from "@/app/gate"
import { describeError, useClient } from "@/core/client"
import type { ItemSummary } from "@/core/types"

import { folderTree } from "./org-model"
import { useVaults } from "./vaults-context"

interface RowMenuProps {
  row: ItemSummary
  /** Opens the organize prompt (folder, tags, favourite) for this row. */
  onOrganize: (row: ItemSummary) => void
  /** Something changed: reload. `moved` is the item's new id after a move. */
  onChanged: (notice: string, moved?: string) => void
  onError: (text: string) => void
}

/**
 * A row's own menu ("assign from the list"): star it, file it, tag it,
 * move it to another vault, archive it. Each is one call to the core; a
 * crown jewel's move asks it's you first through the gate.
 */
export function RowMenu({ row, onOrganize, onChanged, onError }: RowMenuProps) {
  const client = useClient()
  const gate = useGate()
  const { vaults } = useVaults()
  const vault = vaults.find((v) => v.id === row.vault_id)
  const others = vaults.filter((v) => v.id !== row.vault_id)
  const archived = row.state === "archived"
  const run = (p: Promise<unknown>, notice: string) =>
    p.then((r) => onChanged(notice, typeof r === "string" ? r : undefined)).catch((e) => onError(describeError(e)))

  if (row.state === "trashed") return null
  return (
    <DropdownMenu>
      <DropdownMenuTrigger render={<IconButton label={`More for ${row.title || "this item"}`} icon="more_horiz" size="icon-sm" />} />
      <DropdownMenuContent align="end">
        {!archived && (
          <DropdownMenuItem onClick={() => void run(client.organize(row.id, { favourite: !row.favourite }), row.favourite ? "Removed from favourites." : "Added to favourites.")}>
            <Icon name="star" />
            {row.favourite ? "Remove from favourites" : "Add to favourites"}
          </DropdownMenuItem>
        )}
        {!archived && vault && vault.folders.length > 0 && (
          <DropdownMenuSub>
            <DropdownMenuSubTrigger>
              <Icon name="folder" />
              File in folder
            </DropdownMenuSubTrigger>
            <DropdownMenuSubContent>
              <DropdownMenuRadioGroup value={row.folder ?? ""} onValueChange={(f) => void run(client.organize(row.id, { folder: String(f) }), f ? "Filed." : "Taken out of its folder.")}>
                <DropdownMenuRadioItem value="">No folder</DropdownMenuRadioItem>
                {folderTree(vault.folders).map((f) => (
                  <DropdownMenuRadioItem key={f.id} value={f.id}>
                    {f.path}
                  </DropdownMenuRadioItem>
                ))}
              </DropdownMenuRadioGroup>
            </DropdownMenuSubContent>
          </DropdownMenuSub>
        )}
        {!archived && (
          <DropdownMenuItem onClick={() => onOrganize(row)}>
            <Icon name="sell" />
            Folder and tags…
          </DropdownMenuItem>
        )}
        {others.length > 0 && (
          <DropdownMenuSub>
            <DropdownMenuSubTrigger>
              <Icon name="drive_file_move" />
              Move to vault
            </DropdownMenuSubTrigger>
            <DropdownMenuSubContent>
              {others.map((v) => (
                <DropdownMenuItem key={v.id} onClick={() => void run(gate.run((g) => client.moveItem(row.id, v.id, g)), `Moved to ${v.name}.`)}>
                  <Icon name="shelves" />
                  {v.name}
                </DropdownMenuItem>
              ))}
            </DropdownMenuSubContent>
          </DropdownMenuSub>
        )}
        <DropdownMenuSeparator />
        <DropdownMenuItem onClick={() => void run(archived ? client.unarchive(row.id) : client.archive(row.id), archived ? "Back in the list." : "Archived. Find it under Archived.")}>
          <Icon name={archived ? "unarchive" : "archive"} />
          {archived ? "Unarchive" : "Archive"}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
