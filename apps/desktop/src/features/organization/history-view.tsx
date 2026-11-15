import { useEffect, useState } from "react"

import { AdaptiveDialog, Badge, Button, ConfirmDialog, FieldRow, FieldRowValue, ListRow, Skeleton } from "@unicron/ui"
import { SecretField } from "@unicron/ui/security/secret-field"

import { useGate } from "@/app/gate"
import { describeError, useClient } from "@/core/client"
import type { ItemDetail, VersionView } from "@/core/types"
import { countName, fullDate } from "@/lib/format"

interface HistoryViewProps {
  /** The item, or `null` when closed. */
  itemId: string | null
  title: string
  /** Whether going back a version is allowed (a live or archived item). */
  editable: boolean
  onClose: () => void
  /** A version was restored: reload the item. */
  onRestored: (notice: string) => void
}

const two = (n: number) => String(n).padStart(2, "0")
/** When a version was saved, as the brief writes it: `2026.11.08 14:02`, local time. */
function savedAt(secs: number): string {
  const d = new Date(secs * 1000)
  return `${fullDate(secs)} ${two(d.getHours())}:${two(d.getMinutes())}`
}

/**
 * An item's history (02-model §7, brief I6): a form-size dialog (a sheet
 * on a phone) with every saved version, newest first: VERSION n with
 * CURRENT on the newest, when and which device, and which fields it changed
 * (labels only, never a value). SHOW opens that version under its row as
 * Field Rows, every hidden value a SecretField that never reveals or
 * copies; RESTORE asks first (a Question over the dialog), then copies the
 * version into a new one, so nothing is rewritten and the restore is itself
 * in the history. A crown jewel's restore asks it's you first (the gate). A
 * version whose keys are gone says DELETED FOR GOOD and offers nothing.
 */
export function HistoryView({ itemId, title, editable, onClose, onRestored }: HistoryViewProps) {
  const [count, setCount] = useState<number | null>(null)
  return (
    <AdaptiveDialog
      open={itemId !== null}
      onOpenChange={(o) => !o && onClose()}
      size="form"
      height="full"
      overline={`History // ${title}`}
      title={count === null ? "Versions" : countName(count, "version")}
      description="Every save is a version. Going back saves the old one again as the newest."
    >
      {itemId && <HistoryBody key={itemId} itemId={itemId} title={title} editable={editable} onClose={onClose} onRestored={onRestored} onCount={setCount} />}
    </AdaptiveDialog>
  )
}

function HistoryBody({ itemId, title, editable, onRestored, onCount }: HistoryViewProps & { itemId: string; onCount: (n: number) => void }) {
  const client = useClient()
  const gate = useGate()
  const [versions, setVersions] = useState<VersionView[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [shown, setShown] = useState<ItemDetail | null>(null)
  const [showing, setShowing] = useState<number | null>(null)
  const [restoring, setRestoring] = useState<VersionView | null>(null)
  const [restoreError, setRestoreError] = useState<string | null>(null)

  useEffect(() => {
    let live = true
    client
      .history(itemId)
      .then((v) => {
        if (!live) return
        setVersions(v)
        onCount(v.length)
      })
      .catch((e) => live && setError(describeError(e)))
    return () => {
      live = false
    }
  }, [client, itemId, onCount])

  const show = async (v: VersionView) => {
    setError(null)
    if (shown?.version === v.n) return setShown(null)
    setShowing(v.n)
    try {
      setShown(await client.showVersion(itemId, v.n))
    } catch (e) {
      setError(describeError(e))
    } finally {
      setShowing(null)
    }
  }

  const restore = async () => {
    const v = restoring
    if (!v) return
    setRestoreError(null)
    try {
      const n = await gate.run((g) => client.restoreVersion(itemId, v.n, g))
      setRestoring(null)
      onRestored(`Version ${v.n} of ${title} is back as version ${n}`)
    } catch (e) {
      setRestoreError(describeError(e))
      throw e
    }
  }

  if (!versions && !error) {
    return (
      <div role="status" aria-busy="true" aria-label="Reading the history" className="flex flex-col">
        {[0, 1, 2].map((n) => (
          <Skeleton key={n} variant="row" />
        ))}
      </div>
    )
  }
  return (
    <div className="flex flex-col gap-3">
      {error && (
        <p role="alert" className="type-caption text-critical-text">
          {error}
        </p>
      )}
      <ol aria-label="Versions" className="flex flex-col">
        {versions?.map((v) => (
          <li key={v.n} className="flex flex-col">
            <ListRow
              render={<div />}
              title={`Version ${v.n}`}
              badge={v.current ? <Badge>Current</Badge> : !v.readable ? <Badge tone="critical">Deleted for good</Badge> : undefined}
              meta={`${savedAt(v.edited_at)} // ${v.edited_by}`}
              metaStyle="plain"
              detail={v.changed.length > 0 ? `Changed: ${v.changed.join(", ")}` : undefined}
              action={
                v.readable ? (
                  <span className="flex items-center gap-1">
                    <Button type="button" variant="ghost" size="sm" loading={showing === v.n} aria-expanded={shown?.version === v.n} aria-label={`${shown?.version === v.n ? "Hide" : "Show"} version ${v.n}`} onClick={() => void show(v)}>
                      {shown?.version === v.n ? "Hide" : "Show"}
                    </Button>
                    {editable && !v.current && (
                      <Button type="button" variant="ghost" size="sm" aria-label={`Restore version ${v.n}`} onClick={() => setRestoring(v)}>
                        Restore
                      </Button>
                    )}
                  </span>
                ) : undefined
              }
            />
            {shown?.version === v.n && <VersionShown version={shown} />}
          </li>
        ))}
      </ol>
      <ConfirmDialog
        open={restoring !== null}
        onOpenChange={(o) => {
          if (o) return
          setRestoring(null)
          setRestoreError(null)
        }}
        overline={`${title} // version ${restoring?.n ?? ""}`}
        title={`Restore version ${restoring?.n ?? ""}?`}
        description="Its fields are saved again as the newest version. The current one stays in the history; where the item is filed stays as it is."
        confirmLabel="Restore"
        error={restoreError ?? undefined}
        onConfirm={() => restore()}
      />
    </div>
  )
}

/** A passkey's parts: the item's passkey rows draw them, never a version's list. */
const isPasskey = (field_id: string, kind: string) => field_id.startsWith("passkey.") || kind === "passkey"

/** One earlier version, read only: plain values as Field Rows, hidden ones never shown here. */
function VersionShown({ version }: { version: ItemDetail }) {
  const fields = version.fields.filter((f) => !isPasskey(f.field_id, f.kind) && (f.hidden || f.value))
  return (
    <section aria-label={`Version ${version.version}`} className="flex flex-col pb-4">
      <div role="list" className="flex flex-col">
        {fields.map((f) => (
          <div role="listitem" key={f.field_id}>
            {f.hidden ? (
              <SecretField size="stacked" label={f.field_id === "title" ? "Title" : f.label} fillOnly fillOnlyText="Hidden here. Restore this version to use it." />
            ) : (
              <FieldRow size="stacked" label={f.field_id === "title" ? "Title" : f.label}>
                <FieldRowValue truncate={f.kind === "url" ? "middle" : "wrap"}>{f.value ?? ""}</FieldRowValue>
              </FieldRow>
            )}
          </div>
        ))}
      </div>
      <p className="type-caption pt-2 text-support-foreground">Hidden values stay hidden here; restore the version to use them.</p>
    </section>
  )
}
