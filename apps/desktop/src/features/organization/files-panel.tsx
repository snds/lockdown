import { useRef, useState } from "react"

import {
  Button,
  Dialog,
  DialogAction,
  DialogActions,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  Icon,
  IconButton,
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemMedia,
  ItemTitle,
} from "@unicron/ui"

import { useGate } from "@/app/gate"
import { describeError, useClient } from "@/core/client"
import type { AttachmentView } from "@/core/types"

import { sizeWords } from "./org-model"

/** The largest file the app attaches (`lockdown_app` refuses past it too, until attachments stream). */
export const MAX_FILE_BYTES = 100 * 1024 * 1024

interface FilesPanelProps {
  itemId: string
  files: AttachmentView[]
  /** Whether files may be added or removed (a live or archived item). */
  editable: boolean
  /** A file was added or removed: reload the item. */
  onChanged: (notice: string) => void
  onNotice: (text: string) => void
}

/**
 * An item's attached files (02-model §6): each sealed in 1 MiB chunks
 * under its own key, its name, type and size sealed with it. Add picks a
 * file with the system's own picker; Save hands the file's bytes over
 * once (a crown jewel's after its check) and the system saves them where
 * the person chooses, then this window drops its copy. Remove makes a new
 * version without the file; the history keeps it.
 */
export function FilesPanel({ itemId, files, editable, onChanged, onNotice }: FilesPanelProps) {
  const client = useClient()
  const gate = useGate()
  const pick = useRef<HTMLInputElement>(null)
  const [busy, setBusy] = useState(false)
  const [removing, setRemoving] = useState<AttachmentView | null>(null)
  const [error, setError] = useState<string | null>(null)

  const add = async (file: File) => {
    setError(null)
    if (file.size > MAX_FILE_BYTES) {
      setError(`Files up to ${MAX_FILE_BYTES / (1024 * 1024)} MB attach here; "${file.name}" is larger.`)
      return
    }
    setBusy(true)
    const bytes = new Uint8Array(await file.arrayBuffer())
    try {
      await gate.run((g) => client.attach(itemId, file.name, file.type, bytes, g))
      onChanged(`Attached "${file.name}".`)
    } catch (e) {
      setError(describeError(e))
    } finally {
      bytes.fill(0)
      setBusy(false)
      if (pick.current) pick.current.value = ""
    }
  }

  const save = async (f: AttachmentView) => {
    setError(null)
    try {
      const bytes = await gate.run((g) => client.attachment(itemId, f.id, g))
      const blob = new Blob([bytes as BlobPart], { type: f.mime || "application/octet-stream" })
      bytes.fill(0)
      const url = URL.createObjectURL(blob)
      const a = document.createElement("a")
      a.href = url
      a.download = f.name
      document.body.appendChild(a)
      a.click()
      a.remove()
      window.setTimeout(() => URL.revokeObjectURL(url), 10_000)
      onNotice(`Saving "${f.name}".`)
    } catch (e) {
      setError(describeError(e))
    }
  }

  const remove = async () => {
    const f = removing
    setRemoving(null)
    if (!f) return
    try {
      await gate.run((g) => client.detach(itemId, f.id, g))
      onChanged(`Took off "${f.name}"; the item's history still holds it.`)
    } catch (e) {
      setError(describeError(e))
    }
  }

  if (!files.length && !editable) return null
  return (
    <section aria-label="Files" className="flex flex-col gap-2">
      <div className="flex items-center justify-between">
        <h3 className="text-xs font-medium text-muted-foreground uppercase">Files</h3>
        {editable && (
          <Button variant="ghost" size="sm" disabled={busy} onClick={() => pick.current?.click()}>
            <Icon name="attach_file" data-icon="inline-start" />
            {busy ? "Attaching…" : "Attach a file"}
          </Button>
        )}
      </div>
      <input
        ref={pick}
        type="file"
        hidden
        aria-label="File to attach"
        onChange={(e) => {
          const f = e.target.files?.[0]
          if (f) void add(f)
        }}
      />
      {files.length === 0 ? (
        <p className="text-sm text-muted-foreground">No files. Attach a scan, a receipt or a key file; it is encrypted with the item.</p>
      ) : (
        <ItemGroup>
          {files.map((f) => (
            <Item key={f.id} size="sm">
              <ItemMedia variant="icon">
                <Icon name="description" />
              </ItemMedia>
              <ItemContent>
                <ItemTitle>
                  <span className="truncate">{f.name}</span>
                </ItemTitle>
                <ItemDescription>{[sizeWords(f.size), f.mime].filter(Boolean).join(" · ")}</ItemDescription>
              </ItemContent>
              <ItemActions>
                <IconButton label={`Save ${f.name}`} icon="download" size="icon-sm" onClick={() => void save(f)} />
                {editable && <IconButton label={`Remove ${f.name}`} icon="delete" size="icon-sm" variant="ghost-destructive" onClick={() => setRemoving(f)} />}
              </ItemActions>
            </Item>
          ))}
        </ItemGroup>
      )}
      {error && (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      )}
      <Dialog open={removing !== null} onOpenChange={(o) => !o && setRemoving(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Take off "{removing?.name}"?</DialogTitle>
            <DialogDescription>The item saves a new version without it. Earlier versions in its history still hold the file.</DialogDescription>
          </DialogHeader>
          <DialogActions>
            <DialogAction kind="cancel" onClick={() => setRemoving(null)}>
              Cancel
            </DialogAction>
            <DialogAction kind="destructive" onClick={() => void remove()}>
              Take it off
            </DialogAction>
          </DialogActions>
        </DialogContent>
      </Dialog>
    </section>
  )
}
