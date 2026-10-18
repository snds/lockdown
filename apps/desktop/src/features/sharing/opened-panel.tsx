import { useEffect, useMemo, useRef, useState } from "react"

import { Button, Icon, Item, ItemActions, ItemContent, ItemDescription, ItemGroup, ItemTitle } from "@unicron/ui"
import { SecretField, type SecretFieldReferenceHost } from "@unicron/ui/security/secret-field"

import { useLocked } from "@/app/locked"
import { useSettings } from "@/app/settings"
import { describeError, isAppError, useClient } from "@/core/client"
import { decodeAndWipe } from "@/core/secret-bytes"

import { useSharingClient, type OpenedView } from "./sharing-client"

interface OpenedPanelProps {
  opened: OpenedView
  onNotice: (text: string) => void
}

/**
 * What a share link or a request's answer held, opened on this device.
 * The core keeps it until the next one is opened or the vault locks; each
 * value comes over as bytes when it is shown or copied, never as a list.
 * A secret is a `SecretField` copied on the shell's clipboard path; a
 * file is saved where the person picks; the rest can be kept as a note.
 */
export function OpenedPanel({ opened, onNotice }: OpenedPanelProps) {
  const sharing = useSharingClient()
  const [error, setError] = useState<string | null>(null)
  const [saved, setSaved] = useState(false)

  const keep = async () => {
    setError(null)
    try {
      await sharing.saveOpened()
      setSaved(true)
      onNotice(`Kept "${opened.title || "Shared with me"}" as a note in your vault.`)
    } catch (e) {
      setError(describeError(e))
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
      onNotice(`Saving "${file.name}".`)
    } catch (e) {
      setError(describeError(e))
    }
  }

  return (
    <section aria-label={`Opened: ${opened.title || "shared with you"}`} className="flex flex-col gap-2 rounded-xl border p-3">
      <h3 className="text-sm font-semibold">{opened.title || "Shared with you"}</h3>
      <ItemGroup>
        {opened.values.map((v, index) =>
          v.secret ? <OpenedSecret key={index} index={index} label={v.label} /> : <OpenedPlain key={index} index={index} label={v.label} />,
        )}
        {opened.file && (
          <Item variant="outline" size="sm">
            <ItemContent>
              <ItemDescription>A file</ItemDescription>
              <ItemTitle>
                {opened.file.name} · {Math.max(1, Math.round(opened.file.size / 1024))} KB
              </ItemTitle>
            </ItemContent>
            <ItemActions>
              <Button variant="outline" size="sm" onClick={() => void saveFile()}>
                <Icon name="download" data-icon="inline-start" />
                Save
              </Button>
            </ItemActions>
          </Item>
        )}
      </ItemGroup>
      {!opened.file && (
        <div>
          <Button variant="outline" size="sm" disabled={saved} onClick={() => void keep()}>
            <Icon name="inventory_2" data-icon="inline-start" />
            {saved ? "Kept in your vault" : "Keep in my vault"}
          </Button>
        </div>
      )}
      {error && (
        <p role="alert" className="text-sm text-destructive">
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

function OpenedPlain({ index, label }: { index: number; label: string }) {
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
    <Item variant="outline" size="sm" data-row="">
      <ItemContent>
        <ItemDescription>{label}</ItemDescription>
        <ItemTitle>
          <span className="whitespace-pre-wrap">{text ?? error ?? "…"}</span>
        </ItemTitle>
      </ItemContent>
    </Item>
  )
}
