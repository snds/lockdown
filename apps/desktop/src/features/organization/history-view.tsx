import { useEffect, useState } from "react"

import {
  Badge,
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  FieldError,
  Spinner,
} from "@unicron/ui"

import { useGate } from "@/app/gate"
import { LabelledAction, LabelledActions } from "@/components/labelled-actions"
import { describeError, useClient } from "@/core/client"
import type { ItemDetail, VersionView } from "@/core/types"
import { whenLocal } from "@/lib/format"

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

/**
 * An item's history (02-model §7): every saved version, newest first,
 * with when, which device, and which fields it changed (never a value).
 * "Show" opens an earlier version masked, as the item itself is; "Restore"
 * copies it into a new version, so nothing is rewritten and the restore is
 * itself in the history. A crown jewel's restore asks it's you first.
 */
export function HistoryView({ itemId, title, editable, onClose, onRestored }: HistoryViewProps) {
  return (
    <Dialog open={itemId !== null} onOpenChange={(o) => !o && onClose()}>
      <DialogContent size="lg">
        {itemId && <HistoryBody key={itemId} itemId={itemId} title={title} editable={editable} onClose={onClose} onRestored={onRestored} />}
      </DialogContent>
    </Dialog>
  )
}

function HistoryBody({ itemId, title, editable, onClose, onRestored }: HistoryViewProps & { itemId: string }) {
  const client = useClient()
  const gate = useGate()
  const [versions, setVersions] = useState<VersionView[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [shown, setShown] = useState<ItemDetail | null>(null)
  const [restoring, setRestoring] = useState<VersionView | null>(null)

  useEffect(() => {
    let live = true
    client
      .history(itemId)
      .then((v) => live && setVersions(v))
      .catch((e) => live && setError(describeError(e)))
    return () => {
      live = false
    }
  }, [client, itemId])

  const show = async (v: VersionView) => {
    if (!itemId) return
    setError(null)
    try {
      setShown(await client.showVersion(itemId, v.n))
    } catch (e) {
      setError(describeError(e))
    }
  }

  const restore = async () => {
    const v = restoring
    setRestoring(null)
    if (!itemId || !v) return
    try {
      const n = await gate.run((g) => client.restoreVersion(itemId, v.n, g))
      onRestored(`Version ${v.n} is back, saved as version ${n}.`)
    } catch (e) {
      setError(describeError(e))
    }
  }

  return (
    <>
        <DialogHeader>
          <DialogTitle>History of "{title}"</DialogTitle>
          <DialogDescription>Every save is a version of its own. Going back saves the old one again as the newest; nothing is rewritten.</DialogDescription>
        </DialogHeader>
        {error && <FieldError role="alert">{error}</FieldError>}
        {!versions && !error && <Spinner />}
        <ol aria-label="Versions" className="flex max-h-[50vh] flex-col gap-2 overflow-y-auto">
          {versions?.map((v) => (
            <li key={v.n} className="flex flex-wrap items-start justify-between gap-2 rounded-lg border p-2 text-sm">
              <span className="min-w-0">
                <span className="flex items-center gap-2 font-medium">
                  Version {v.n}
                  {v.current && <Badge variant="secondary">Current</Badge>}
                  {!v.readable && <Badge variant="outline">Deleted for good</Badge>}
                </span>
                <span className="block text-xs text-muted-foreground">
                  {whenLocal(v.edited_at)} · {v.edited_by}
                </span>
                {v.changed.length > 0 && <span className="block text-xs text-muted-foreground">Changed: {v.changed.join(", ")}</span>}
              </span>
              {v.readable && (
                <span className="flex shrink-0 gap-1">
                  <Button size="sm" variant="ghost" onClick={() => void show(v)}>
                    Show
                  </Button>
                  {editable && !v.current && (
                    <Button size="sm" variant="outline" onClick={() => setRestoring(v)}>
                      Restore
                    </Button>
                  )}
                </span>
              )}
            </li>
          ))}
        </ol>
        {shown && (
          <section aria-label={`Version ${shown.version}`} className="flex flex-col gap-1 rounded-lg border p-3 text-sm">
            <h3 className="text-xs font-medium text-muted-foreground uppercase">
              Version {shown.version} · {whenLocal(shown.edited_at)}
            </h3>
            <dl className="grid grid-cols-[minmax(0,1fr)_minmax(0,2fr)] gap-x-3 gap-y-1">
              {shown.fields.map((f) => (
                <div key={f.field_id} className="contents">
                  <dt className="truncate text-muted-foreground">{f.field_id === "title" ? "Title" : f.label}</dt>
                  <dd className="truncate">{f.hidden ? (f.mask ?? "••••••••") : (f.value ?? "")}</dd>
                </div>
              ))}
            </dl>
            <p className="text-xs text-muted-foreground">Hidden values stay hidden here; restore the version to use them.</p>
          </section>
        )}
        <LabelledActions>
          <LabelledAction kind="cancel" onClick={onClose}>
            Done
          </LabelledAction>
        </LabelledActions>
        <Dialog open={restoring !== null} onOpenChange={(o) => !o && setRestoring(null)}>
          <DialogContent>
            <DialogHeader>
              <DialogTitle>Restore version {restoring?.n}?</DialogTitle>
              <DialogDescription>Its fields are saved again as the newest version. The current one stays in the history; where the item is filed stays as it is.</DialogDescription>
            </DialogHeader>
            <LabelledActions>
              <LabelledAction kind="cancel" onClick={() => setRestoring(null)}>
                Cancel
              </LabelledAction>
              <LabelledAction kind="confirm" onClick={() => void restore()}>
                Restore
              </LabelledAction>
            </LabelledActions>
          </DialogContent>
        </Dialog>
    </>
  )
}
