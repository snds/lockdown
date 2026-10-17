import { useEffect, useState } from "react"

import {
  Badge,
  Button,
  Dialog,
  DialogAction,
  DialogActions,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  Icon,
  ItemGroup,
  Spinner,
} from "@unicron/ui"

import { describeError, useClient } from "@/core/client"
import type { ItemDetail } from "@/core/types"
import { whenLocal } from "@/lib/format"

import { SecretField } from "./secret-field"

interface DetailPaneProps {
  /** The parent keys this pane by item and version, so a change remounts it. */
  id: string
  onEdit: (item: ItemDetail) => void
  onTrashed: () => void
  onNotice: (text: string) => void
}

export function DetailPane({ id, onEdit, onTrashed, onNotice }: DetailPaneProps) {
  const client = useClient()
  const [item, setItem] = useState<ItemDetail | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [confirm, setConfirm] = useState(false)

  useEffect(() => {
    let live = true
    client
      .show(id)
      .then((d) => live && setItem(d))
      .catch((e) => live && setError(describeError(e)))
    return () => {
      live = false
    }
  }, [client, id])

  const trash = async () => {
    try {
      await client.trash(id)
      setConfirm(false)
      onTrashed()
    } catch (e) {
      setError(describeError(e))
    }
  }

  if (error) {
    return (
      <p role="alert" className="p-6 text-sm text-destructive">
        {error}
      </p>
    )
  }
  if (!item) {
    return (
      <div className="p-6">
        <Spinner />
      </div>
    )
  }
  const title = item.fields.find((f) => f.field_id === "title")?.value ?? item.kind
  return (
    <article className="flex flex-col gap-4 p-4 md:p-6">
      <header className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <h2 className="truncate text-xl font-semibold tracking-tight">{title}</h2>
          <p className="text-sm text-muted-foreground">
            {item.kind} · version {item.version} · edited {whenLocal(item.edited_at)}
          </p>
          {item.crown_jewel && (
            <Badge variant="outline" className="mt-1">
              <Icon name="lock" data-icon="inline-start" />
              crown jewel
            </Badge>
          )}
        </div>
        <div className="flex shrink-0 items-center gap-1">
          <Button variant="outline" size="sm" onClick={() => onEdit(item)}>
            <Icon name="edit" data-icon="inline-start" />
            Edit
          </Button>
          <Button variant="ghost-destructive" size="sm" onClick={() => setConfirm(true)}>
            <Icon name="delete" data-icon="inline-start" />
            Trash
          </Button>
        </div>
      </header>
      <ItemGroup className="gap-2">
        {item.fields.map((f) => (
          <SecretField key={f.field_id} itemId={item.id} field={f} onNotice={onNotice} />
        ))}
      </ItemGroup>
      <Dialog open={confirm} onOpenChange={setConfirm}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Move to the trash?</DialogTitle>
            <DialogDescription>
              "{title}" leaves the list and stays readable through its grace period. Nothing is destroyed yet.
            </DialogDescription>
          </DialogHeader>
          <DialogActions>
            <DialogAction kind="cancel" onClick={() => setConfirm(false)}>
              Cancel
            </DialogAction>
            <DialogAction kind="destructive" onClick={trash}>
              Trash
            </DialogAction>
          </DialogActions>
        </DialogContent>
      </Dialog>
    </article>
  )
}
