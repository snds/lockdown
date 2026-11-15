import { useEffect, useMemo, useRef, useState } from "react"

import { Button, FieldRow, FileChip } from "@unicron/ui"
import { SecretField, type SecretFieldReferenceHost } from "@unicron/ui/security/secret-field"

import { useLocked } from "@/app/locked"
import { useSettings } from "@/app/settings"
import { describeError, isAppError, useClient } from "@/core/client"
import { decodeAndWipe } from "@/core/secret-bytes"

import { useSharingClient, type OpenedView } from "./sharing-client"
import { paneHeading } from "./sharing-panes"

interface OpenedPanelProps {
  opened: OpenedView
  onNotice: (text: string) => void
  /** KEEP IN MY VAULT is offered: left out where the build has no call for it (the web vault). */
  canKeep?: boolean
}

/** The size of a file as a chip says it: `84 KB`. */
const kb = (bytes: number) => `${Math.max(1, Math.round(bytes / 1024))} KB`

/**
 * What a share link or a request's answer held, opened on this device:
 * the Links tab's detail (brief S4). OPENED // SHARED WITH YOU, what it
 * is, then one row per value. The core keeps it until the next one is
 * opened or the vault locks; each value comes over as bytes when it is
 * shown or copied, never as a list. A secret is a `SecretField` copied on
 * the shell's clipboard path, one value at a time; plain text is a Field
 * Row; a file is a `FileChip` saved where the person picks. KEEP IN MY
 * VAULT (secondary) leaves once pressed, and a caption takes its place.
 */
export function OpenedPanel({ opened, onNotice, canKeep = true }: OpenedPanelProps) {
  const sharing = useSharingClient()
  const [error, setError] = useState<string | null>(null)
  const [saved, setSaved] = useState(false)
  const [keeping, setKeeping] = useState(false)
  const title = opened.title || "Shared with you"

  const keep = async () => {
    setError(null)
    setKeeping(true)
    try {
      await sharing.saveOpened()
      setSaved(true)
    } catch (e) {
      setError(describeError(e))
    } finally {
      setKeeping(false)
    }
  }
  const saveFile = async () => {
    const file = opened.file
    if (!file) return
    setError(null)
    try {
      const bytes = await sharing.openedFile()
      const blob = new Blob([bytes as BlobPart], { type: file.mime || "application/octet-stream" })
      bytes.fill(0)
      const url = URL.createObjectURL(blob)
      const a = document.createElement("a")
      a.href = url
      a.download = file.name
      document.body.appendChild(a)
      a.click()
      a.remove()
      window.setTimeout(() => URL.revokeObjectURL(url), 10_000)
      onNotice(`Saving ${file.name}`)
    } catch (e) {
      setError(describeError(e))
    }
  }

  return (
    <section aria-label={`Opened: ${title}`} className="flex flex-col gap-6">
      {paneHeading("Opened // shared with you", title)}
      <div role="list" aria-label={`What ${title} holds`} className="flex w-full flex-col">
        {opened.values.map((v, index) => (
          <div role="listitem" key={index}>
            {v.secret ? <OpenedSecret index={index} label={v.label} /> : <OpenedPlain index={index} label={v.label} first={index === 0} />}
          </div>
        ))}
        {opened.file && (
          <div role="listitem" className="pt-3">
            <FileChip
              name={opened.file.name}
              size={kb(opened.file.size)}
              action={
                <Button type="button" variant="ghost" size="sm" onClick={() => void saveFile()}>
                  Save
                </Button>
              }
            />
          </div>
        )}
      </div>
      {canKeep && !opened.file && (
        <div className="flex flex-col items-start gap-2">
          {saved ? (
            <p className="type-body text-text-body">Kept in your vault.</p>
          ) : (
            <Button type="button" variant="secondary" loading={keeping} onClick={() => void keep()}>
              Keep in my vault
            </Button>
          )}
        </div>
      )}
      <p className="type-caption text-text-secondary">Kept here until you open another link or lock.</p>
      {error && (
        <p role="alert" className="type-caption text-critical-text">
          {error}
        </p>
      )}
    </section>
  )
}

const coreWords = (e: unknown): string | undefined => (isAppError(e) ? e.message : undefined)

function OpenedSecret({ index, label }: { index: number; label: string }) {
  const sharing = useSharingClient()
  const client = useClient()
  const { settings } = useSettings()
  const locked = useLocked()
  const lastCopy = useRef<number | null>(null)
  const host = useMemo<SecretFieldReferenceHost>(
    () => ({
      copyRef: async () => {
        const { copy } = await sharing.copyOpened(index, settings.clipboard_clear_secs)
        lastCopy.current = copy
        return { hash: String(copy) }
      },
      clear: async () => {
        if (lastCopy.current !== null) await client.clipboardClear(lastCopy.current)
      },
      clipboardHolds: (hash) => {
        const copy = Number(hash)
        return Number.isSafeInteger(copy) ? client.clipboardHolds(copy) : Promise.resolve(false)
      },
    }),
    [sharing, client, index, settings.clipboard_clear_secs],
  )
  return (
    <SecretField
      label={label}
      getSecret={() => sharing.openedValue(index)}
      host={host}
      revealMode={settings.reveal}
      clipboardTimer={settings.clipboard_clear_secs}
      locked={locked}
      errorText={coreWords}
    />
  )
}

function OpenedPlain({ index, label, first }: { index: number; label: string; first: boolean }) {
  const sharing = useSharingClient()
  const [text, setText] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    let live = true
    sharing
      .openedValue(index)
      .then((b) => {
        const t = decodeAndWipe(b)
        if (live) setText(t)
      })
      .catch((e) => live && setError(describeError(e)))
    return () => {
      live = false
    }
  }, [sharing, index])
  return (
    <FieldRow size="stacked" label={label} rule={first ? "none" : undefined}>
      <span className="whitespace-pre-wrap break-words">{text ?? error ?? "…"}</span>
    </FieldRow>
  )
}
