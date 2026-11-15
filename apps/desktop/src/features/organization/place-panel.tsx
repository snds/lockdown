import { useState } from "react"

import { AdaptiveMenu, AdaptiveMenuItem, Chip, FieldRow, FieldRowAction } from "@unicron/ui"

import { useGate } from "@/app/gate"
import { describeError, useClient } from "@/core/client"
import type { ItemDetail, ItemSummary } from "@/core/types"

import type { OrganizeTarget } from "./org-model"
import { OrganizePrompt } from "./organize-prompt"
import { useVaults } from "./vaults-context"

interface PlacePanelProps {
  item: ItemDetail
  title: string
  rows: ItemSummary[]
  /** Reload the item and the list. */
  onChanged: (notice: string) => void
  /** The item was sealed again under a new id (a move). */
  onReplaced: (id: string, notice: string) => void
  onError: (text: string) => void
  /** Field Row's layout: `stacked` on a phone and in a pane under 480 px (brief 4.0). */
  size?: "default" | "stacked"
  /** Whether this person may change the item (their own, or a share with edit). Default true. */
  canEdit?: boolean
}

/**
 * Where an item is (brief I4, the first rows of Details): VAULT with MOVE
 * (a menu of the other vaults), FOLDER and TAGS with CHANGE (the Folder and
 * tags Form). A move seals the item again under the other vault's keys
 * (its history and files come along) and gives it a new id; the detail
 * follows it; a crown jewel's asks it's you first (the gate). Only what
 * applies is drawn: no MOVE with one vault, nothing to press in Recently
 * deleted or on a share this person cannot change, no CHANGE on an
 * archived item (unarchive it first, from its menu or its Banner).
 */
export function PlacePanel({ item, title, rows, onChanged, onReplaced, onError, size = "default", canEdit = true }: PlacePanelProps) {
  const client = useClient()
  const gate = useGate()
  const { vaults } = useVaults()
  const [organizing, setOrganizing] = useState<OrganizeTarget | null>(null)
  const org = item.organization ?? { folder: null, tags: [], favourite: false }
  const vault = vaults.find((v) => v.id === item.vault_id)
  const folder = vault?.folders.find((f) => f.id === org.folder)
  const others = vaults.filter((v) => v.id !== item.vault_id)
  const live = canEdit && item.state !== "trashed"
  const organizable = live && item.state !== "archived"

  const move = (to: string, name: string) =>
    void gate
      .run((g) => client.moveItem(item.id, to, g))
      .then((id) => onReplaced(id, `Moved ${title} to ${name}`))
      .catch((e) => onError(describeError(e)))
  const change = organizable ? (
    <FieldRowAction onClick={() => setOrganizing({ id: item.id, title, vaultId: item.vault_id ?? "", organization: org })}>Change</FieldRowAction>
  ) : undefined

  return (
    <>
      {vault && (
        <FieldRow
          size={size}
          label="Vault"
          action={
            live && others.length > 0 ? (
              <AdaptiveMenu title={`Move ${title} to`} align="end" trigger={<FieldRowAction>Move</FieldRowAction>}>
                {others.map((v) => (
                  <AdaptiveMenuItem key={v.id} icon="folder--move-to" onSelect={() => move(v.id, v.name)}>
                    {v.name}
                  </AdaptiveMenuItem>
                ))}
              </AdaptiveMenu>
            ) : undefined
          }
        >
          {vault.name}
        </FieldRow>
      )}
      {(organizable || folder) && (
        <FieldRow size={size} label="Folder" action={change}>
          {folder?.path ?? "None"}
        </FieldRow>
      )}
      {(organizable || org.tags.length > 0) && (
        <FieldRow size={size} label="Tags" action={change}>
          {org.tags.length ? (
            <span role="list" aria-label="Tags" className="flex min-w-0 flex-wrap gap-1">
              {org.tags.map((t) => (
                <span role="listitem" key={t}>
                  <Chip variant="outline">#{t}</Chip>
                </span>
              ))}
            </span>
          ) : (
            "None"
          )}
        </FieldRow>
      )}
      <OrganizePrompt
        target={organizing}
        rows={rows}
        onClose={() => setOrganizing(null)}
        onSaved={() => {
          setOrganizing(null)
          onChanged(`Saved ${title}`)
        }}
      />
    </>
  )
}
