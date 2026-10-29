import { useState } from "react"

import {
  Button,
  Chip,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
  Icon,
  IconButton,
} from "@unicron/ui"

import { useGate } from "@/app/gate"
import { LabelledAction, LabelledActions } from "@/components/labelled-actions"
import { describeError, useClient } from "@/core/client"
import type { ItemDetail, ItemSummary } from "@/core/types"

import { HistoryView } from "./history-view"
import type { OrganizeTarget } from "./org-model"
import { OrganizePrompt } from "./organize-prompt"
import { useVaults } from "./vaults-context"

interface PlacePanelProps {
  item: ItemDetail
  title: string
  rows: ItemSummary[]
  /** Reload the item and the list. */
  onChanged: (notice: string) => void
  /** The item was sealed again under a new id (a move, a class change). */
  onReplaced: (id: string, notice: string) => void
  onError: (text: string) => void
}

type Ask = "jewel-on" | "jewel-off" | null

/**
 * Where an item is and what can move it: its vault, folder and tags, the
 * favourite star, and a menu to archive it, move it to another vault, put
 * it into or take it out of the crown-jewel class, and see its history. A
 * move and a change of class seal the item again under new keys (its
 * history and files come along) and give it a new id; taking it out of the
 * class asks for the account password, as a reveal does.
 */
export function PlacePanel({ item, title, rows, onChanged, onReplaced, onError }: PlacePanelProps) {
  const client = useClient()
  const gate = useGate()
  const { vaults } = useVaults()
  const [organizing, setOrganizing] = useState<OrganizeTarget | null>(null)
  const [history, setHistory] = useState(false)
  const [ask, setAsk] = useState<Ask>(null)
  const org = item.organization ?? { folder: null, tags: [], favourite: false }
  const vault = vaults.find((v) => v.id === item.vault_id)
  const folder = vault?.folders.find((f) => f.id === org.folder)
  const others = vaults.filter((v) => v.id !== item.vault_id)
  const trashed = item.state === "trashed"
  const archived = item.state === "archived"
  const fail = (e: unknown) => onError(describeError(e))

  const star = () =>
    client
      .organize(item.id, { favourite: !org.favourite })
      .then(() => onChanged(org.favourite ? "Removed from favourites." : "Added to favourites."))
      .catch(fail)
  const move = (to: string, name: string) =>
    gate
      .run((g) => client.moveItem(item.id, to, g))
      .then((id) => onReplaced(id, `Moved to ${name}.`))
      .catch(fail)
  const jewel = (on: boolean) => {
    setAsk(null)
    gate
      .run((g) => client.setCrownJewel(item.id, on, g))
      .then((id) => onReplaced(id, on ? "Now kept as a crown jewel." : "No longer a crown jewel."))
      .catch(fail)
  }
  const archive = () =>
    (archived ? client.unarchive(item.id) : client.archive(item.id))
      .then(() => onChanged(archived ? "Back in the list." : "Archived. It stays in your exports and syncs; find it under Archived."))
      .catch(fail)

  return (
    <section aria-label="Where it is" className="flex flex-wrap items-center gap-2 text-sm">
      {!trashed && (
        <IconButton
          label={org.favourite ? "Remove from favourites" : "Add to favourites"}
          icon="star"
          fill={org.favourite}
          size="icon-sm"
          onClick={() => void star()}
        />
      )}
      {vaults.length > 1 && vault && (
        <Chip variant="outline">
          <Icon name="shelves" />
          {vault.name}
        </Chip>
      )}
      {folder && (
        <Chip variant="outline">
          <Icon name="folder" />
          {folder.path}
        </Chip>
      )}
      {org.tags.map((t) => (
        <Chip key={t} variant="secondary">
          #{t}
        </Chip>
      ))}
      {!trashed && !archived && (
        <Button
          variant="ghost"
          size="sm"
          onClick={() => setOrganizing({ id: item.id, title, vaultId: item.vault_id ?? "", organization: org })}
        >
          <Icon name="sell" data-icon="inline-start" />
          {folder || org.tags.length ? "Change folder and tags" : "Add a folder or tags"}
        </Button>
      )}
      <span className="flex-1" />
      <DropdownMenu>
        <DropdownMenuTrigger render={<IconButton label="More for this item" icon="more_horiz" size="icon-sm" />} />
        <DropdownMenuContent align="end">
          <DropdownMenuItem onClick={() => setHistory(true)}>
            <Icon name="history" />
            History
          </DropdownMenuItem>
          {!trashed && (
            <>
              <DropdownMenuItem onClick={() => void archive()}>
                <Icon name={archived ? "unarchive" : "archive"} />
                {archived ? "Unarchive" : "Archive"}
              </DropdownMenuItem>
              {others.length > 0 && (
                <DropdownMenuSub>
                  <DropdownMenuSubTrigger>
                    <Icon name="drive_file_move" />
                    Move to vault
                  </DropdownMenuSubTrigger>
                  <DropdownMenuSubContent>
                    {others.map((v) => (
                      <DropdownMenuItem key={v.id} onClick={() => void move(v.id, v.name)}>
                        <Icon name="shelves" />
                        {v.name}
                      </DropdownMenuItem>
                    ))}
                  </DropdownMenuSubContent>
                </DropdownMenuSub>
              )}
              <DropdownMenuSeparator />
              {!item.crown_jewel && (
                <DropdownMenuItem onClick={() => setAsk("jewel-on")}>
                  <Icon name="lock" />
                  Keep as a crown jewel
                </DropdownMenuItem>
              )}
              {item.crown_jewel && !item.crown_jewel_fixed && (
                <DropdownMenuItem onClick={() => setAsk("jewel-off")}>
                  <Icon name="lock_open" />
                  Stop keeping as a crown jewel
                </DropdownMenuItem>
              )}
            </>
          )}
        </DropdownMenuContent>
      </DropdownMenu>
      <OrganizePrompt
        target={organizing}
        rows={rows}
        onClose={() => setOrganizing(null)}
        onSaved={() => {
          setOrganizing(null)
          onChanged("Saved.")
        }}
      />
      <HistoryView
        itemId={history ? item.id : null}
        title={title}
        editable={!trashed}
        onClose={() => setHistory(false)}
        onRestored={(notice) => {
          setHistory(false)
          onChanged(notice)
        }}
      />
      <Dialog open={ask !== null} onOpenChange={(o) => !o && setAsk(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{ask === "jewel-on" ? "Keep as a crown jewel?" : "Stop keeping it as a crown jewel?"}</DialogTitle>
            <DialogDescription>
              {ask === "jewel-on"
                ? "Showing or using it will ask it's you first, it is never filled into a page, and lists show only its name. It is sealed again under the crown-jewel key. You can stop later from this menu, with your account password."
                : "It will be shown, copied and filled like any other item. Your account password is asked for first, and it is sealed again under the vault's key."}
            </DialogDescription>
          </DialogHeader>
          <LabelledActions>
            <LabelledAction kind="cancel" onClick={() => setAsk(null)}>
              Cancel
            </LabelledAction>
            <LabelledAction kind={ask === "jewel-off" ? "destructive" : "confirm"} onClick={() => jewel(ask === "jewel-on")}>
              {ask === "jewel-on" ? "Keep as a crown jewel" : "Stop protecting it"}
            </LabelledAction>
          </LabelledActions>
        </DialogContent>
      </Dialog>
    </section>
  )
}
