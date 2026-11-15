import { useEffect, useRef, useState } from "react"

import { Button, ConfirmDialog, FileChip, Icon, IconButton, SectionHeader } from "@unicron/ui"

import { useGate } from "@/app/gate"
import { usePlaces } from "@/app/places"
import { describeError, useClient } from "@/core/client"
import type { AttachmentView, StorageView } from "@/core/types"

import { sizeWords } from "./org-model"

/**
 * The largest file this window reads itself and hands over (the path the
 * web vault and a test take). The desktop attaches from the disk instead,
 * through its own file dialog, a chunk at a time up to 1 GiB (D233).
 */
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
 * An item's files (02-model §6, brief I5): each sealed in 1 MiB chunks
 * under its own key, its name, type and size sealed with it. The Section
 * Header FILES with its count and ATTACH, then one FileChip per file (a
 * type tag, the name, the size). Pressing a chip saves the file where the
 * person chooses (a crown jewel's after its check); its trailing control
 * removes it after a Question: the item saves a new version without it and
 * the history keeps it. Under the chips, the room used on the server, when
 * the account has one. Nothing on a share this person cannot change but the
 * files, and nothing at all in the web vault, which keeps no files (W1).
 */
export function FilesPanel({ itemId, files, editable, onChanged, onNotice }: FilesPanelProps) {
  const client = useClient()
  const gate = useGate()
  const { surface } = usePlaces()
  const pick = useRef<HTMLInputElement>(null)
  const [busy, setBusy] = useState(false)
  const [saving, setSaving] = useState<string | null>(null)
  const [removing, setRemoving] = useState<AttachmentView | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [room, setRoom] = useState<StorageView | null>(null)
  const web = surface === "web"

  // The account's attachment room on its server (D234); nothing without an account.
  useEffect(() => {
    if (!editable || web) return
    let live = true
    Promise.resolve()
      .then(() => client.storageUsage())
      .then((r) => live && setRoom(r ?? null))
      .catch(() => live && setRoom(null))
    return () => {
      live = false
    }
  }, [client, editable, web, files.length])

  /**
   * Attach: the shell's own file dialog and the core reading the disk a
   * chunk at a time (D233), so this window never holds the file; where
   * that isn't offered (a test), the system's picker and the bytes.
   */
  const attachFromDisk = async () => {
    setError(null)
    let chosen
    try {
      chosen = await client.attachChoose()
    } catch {
      pick.current?.click()
      return
    }
    if (!chosen) return
    setBusy(true)
    try {
      await gate.run((g) => client.attachChosen(itemId, chosen.token, g))
      onChanged(`Attached ${chosen.name}`)
    } catch (e) {
      setError(describeError(e))
    } finally {
      setBusy(false)
    }
  }

  const add = async (file: File) => {
    setError(null)
    if (file.size > MAX_FILE_BYTES) {
      setError(`Files up to ${MAX_FILE_BYTES / (1024 * 1024)} MB attach here; ${file.name} is larger.`)
      return
    }
    setBusy(true)
    const bytes = new Uint8Array(await file.arrayBuffer())
    try {
      await gate.run((g) => client.attach(itemId, file.name, file.type, bytes, g))
      onChanged(`Attached ${file.name}`)
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
    // The desktop writes the file to disk itself, a chunk at a time, where the person chooses.
    let place
    try {
      place = await client.attachmentSaveChoose(f.name)
    } catch {
      place = undefined
    }
    if (place === null) return
    setSaving(f.id)
    try {
      if (place) {
        await gate.run((g) => client.attachmentSave(itemId, f.id, place.token, g))
        onNotice(`Saved ${f.name} to ${place.name}`)
        return
      }
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
      onNotice(`Saving ${f.name}`)
    } catch (e) {
      setError(describeError(e))
    } finally {
      setSaving(null)
    }
  }

  const remove = async () => {
    const f = removing
    if (!f) return
    try {
      await gate.run((g) => client.detach(itemId, f.id, g))
      onChanged(`Removed ${f.name}`)
    } catch (e) {
      setError(describeError(e))
    } finally {
      setRemoving(null)
    }
  }

  if (web || (!files.length && !editable)) return null
  return (
    <section aria-label="Files" className="flex flex-col gap-3">
      <SectionHeader
        title="Files"
        count={files.length || undefined}
        level={3}
        action={
          editable ? (
            <Button variant="ghost" size="sm" loading={busy} onClick={() => void attachFromDisk()}>
              <Icon name="attachment" data-icon="inline-start" />
              Attach
            </Button>
          ) : undefined
        }
      />
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
      {files.length > 0 ? (
        <ul aria-label="Files" className="flex flex-wrap gap-2">
          {files.map((f) => (
            <li key={f.id} className="flex min-w-0 max-w-full">
              <FileChip
                name={f.name}
                type={typeOf(f)}
                size={sizeWords(f.size)}
                busy={saving === f.id}
                onClick={() => void save(f)}
                mainRender={<button type="button" aria-label={`Save ${f.name}`} />}
                action={editable ? <IconButton variant="ghost" size="icon-sm" icon="close" label={`Remove ${f.name}`} onClick={() => setRemoving(f)} /> : undefined}
              />
            </li>
          ))}
        </ul>
      ) : (
        <p className="type-caption text-support-foreground">No files.</p>
      )}
      {room && (
        <p className="type-caption text-support-foreground">
          {room.quota !== null ? `${room.used_words} of ${room.quota_words} used for files on your server.` : `${room.used_words} of files on your server.`}
        </p>
      )}
      {error && (
        <p role="alert" className="type-caption text-critical-text">
          {error}
        </p>
      )}
      <ConfirmDialog
        open={removing !== null}
        onOpenChange={(o) => !o && setRemoving(null)}
        tone="destructive"
        title={`Remove ${removing?.name ?? "this file"}?`}
        description="The item saves a new version without it. Earlier versions in its history still hold the file."
        confirmLabel="Remove"
        onConfirm={() => remove()}
      />
    </section>
  )
}

/** The chip's type tag: the name's extension, else the media type's last part ("PDF", "PNG"). */
function typeOf(f: AttachmentView): string | undefined {
  const dot = f.name.lastIndexOf(".")
  if (dot > 0 && dot < f.name.length - 1) return f.name.slice(dot + 1).toUpperCase()
  const sub = f.mime.split("/")[1]?.split(/[+.;-]/)[0]
  return sub ? sub.toUpperCase() : undefined
}
