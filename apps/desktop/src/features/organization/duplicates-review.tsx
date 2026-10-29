import { useCallback, useEffect, useState } from "react"

import {
  Badge,
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyTitle,
  FieldError,
  Icon,
  Spinner,
} from "@unicron/ui"

import { useGate } from "@/app/gate"
import { LabelledAction, LabelledActions } from "@/components/labelled-actions"
import { describeError, useClient } from "@/core/client"
import type { DuplicateGroup, ItemSummary } from "@/core/types"
import { whenLocal } from "@/lib/format"
import { kindLabel } from "@/features/vault/list-model"

import { useVaults, vaultName } from "./vaults-context"

interface DuplicatesReviewProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  rows: ItemSummary[]
  /** Something was merged or trashed: reload the list. */
  onChanged: (notice: string) => void
  /** Opens one item in the vault. */
  onOpen: (id: string) => void
}

type Pending = { kind: "merge"; keep: string; others: string[] } | { kind: "trash"; id: string }

/**
 * The duplicate finder (02-model §11): Logins for the same website and
 * username (merge them into one: websites, codes, notes and tags come
 * along, the rest go to the trash), and items sharing a password or a
 * code set-up (keep the one you want, trash the others). The core decides
 * what matches with a keyed hash it throws away after the search, so no
 * secret and no fingerprint of one comes to this window. Nothing is
 * destroyed: whatever goes is readable in Deleted through its grace period.
 */
export function DuplicatesReview({ open, onOpenChange, rows, onChanged, onOpen }: DuplicatesReviewProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent size="lg">{open && <ReviewBody onOpenChange={onOpenChange} rows={rows} onChanged={onChanged} onOpen={onOpen} />}</DialogContent>
    </Dialog>
  )
}

function ReviewBody({ onOpenChange, rows, onChanged, onOpen }: Omit<DuplicatesReviewProps, "open">) {
  const client = useClient()
  const gate = useGate()
  const { vaults } = useVaults()
  const [groups, setGroups] = useState<DuplicateGroup[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [pending, setPending] = useState<Pending | null>(null)
  const [round, setRound] = useState(0)

  useEffect(() => {
    let live = true
    client
      .duplicates()
      .then((g) => live && setGroups(g))
      .catch((e) => live && setError(describeError(e)))
    return () => {
      live = false
    }
  }, [client, round])
  const load = useCallback(() => setRound((r) => r + 1), [])

  const byId = new Map(rows.map((r) => [r.id, r]))
  const go = async () => {
    if (!pending) return
    try {
      if (pending.kind === "merge") {
        await gate.run((g) => client.mergeDuplicates(pending.keep, pending.others, g))
        onChanged(pending.others.length === 1 ? "Merged; the other copy is in the trash." : `Merged; ${pending.others.length} copies are in the trash.`)
      } else {
        await client.trash(pending.id)
        onChanged("Moved to the trash.")
      }
      setPending(null)
      load()
    } catch (e) {
      setPending(null)
      setError(describeError(e))
    }
  }

  const titleOf = (id: string) => byId.get(id)?.title || "this item"
  return (
    <>
        <DialogHeader>
          <DialogTitle>Possible duplicates</DialogTitle>
          <DialogDescription>
            Items that look like the same thing twice, often after an import. Merge Logins into one, or trash the copies you don't need; trashed items stay
            readable in Deleted for a while.
          </DialogDescription>
        </DialogHeader>
        {error && <FieldError role="alert">{error}</FieldError>}
        {!groups && !error && <Spinner />}
        {groups && groups.length === 0 && (
          <Empty>
            <EmptyHeader>
              <EmptyTitle>No duplicates</EmptyTitle>
              <EmptyDescription>Nothing shares a website and username, a password or a code set-up.</EmptyDescription>
            </EmptyHeader>
          </Empty>
        )}
        <div className="flex max-h-[60vh] flex-col gap-4 overflow-y-auto">
          {groups?.map((g) => (
            <section key={`${g.reason}:${g.ids.join(",")}`} aria-label={g.words} className="flex flex-col gap-2 rounded-xl border p-3">
              <p className="flex items-center gap-2 text-sm font-medium">
                <Badge variant="outline">{g.reason === "same-login" ? "Same login" : g.reason === "same-password" ? "Same password" : "Same code"}</Badge>
                {g.words}
              </p>
              <ul className="flex flex-col gap-2">
                {g.ids.map((id) => {
                  const r = byId.get(id)
                  return (
                    <li key={id} className="flex flex-wrap items-center justify-between gap-2 text-sm">
                      <span className="min-w-0">
                        <span className="font-medium">{r?.title || id}</span>
                        <span className="block text-xs text-muted-foreground">
                          {[r ? kindLabel(r.kind) : "", vaults.length > 1 ? vaultName(vaults, r?.vault_id) : "", r ? `changed ${whenLocal(r.edited_at)}` : ""].filter(Boolean).join(" · ")}
                        </span>
                      </span>
                      <span className="flex shrink-0 gap-1">
                        <Button size="sm" variant="ghost" onClick={() => onOpen(id)}>
                          Open
                        </Button>
                        {g.mergeable && (
                          <Button size="sm" variant="outline" onClick={() => setPending({ kind: "merge", keep: id, others: g.ids.filter((x) => x !== id) })}>
                            <Icon name="merge" data-icon="inline-start" />
                            Keep this, merge the rest
                          </Button>
                        )}
                        <Button size="sm" variant="ghost-destructive" onClick={() => setPending({ kind: "trash", id })}>
                          Trash
                        </Button>
                      </span>
                    </li>
                  )
                })}
              </ul>
            </section>
          ))}
        </div>
        <LabelledActions>
          <LabelledAction kind="cancel" onClick={() => onOpenChange(false)}>
            Done
          </LabelledAction>
        </LabelledActions>
        <Dialog open={pending !== null} onOpenChange={(o) => !o && setPending(null)}>
          <DialogContent>
            <DialogHeader>
              <DialogTitle>{pending?.kind === "merge" ? `Merge into "${titleOf(pending.keep)}"?` : "Move to the trash?"}</DialogTitle>
              <DialogDescription>
                {pending?.kind === "merge"
                  ? `Websites, codes, passkeys, notes and tags from ${pending.others.length === 1 ? `"${titleOf(pending.others[0] ?? "")}"` : `${pending.others.length} copies`} are added to it, and ${pending.others.length === 1 ? "that copy goes" : "they go"} to the trash. Its own password stays; a different password on a copy stays readable in the trash.`
                  : `"${pending ? titleOf(pending.id) : ""}" stays readable in Deleted through its grace period.`}
              </DialogDescription>
            </DialogHeader>
            <LabelledActions>
              <LabelledAction kind="cancel" onClick={() => setPending(null)}>
                Cancel
              </LabelledAction>
              <LabelledAction kind={pending?.kind === "merge" ? "confirm" : "destructive"} onClick={() => void go()}>
                {pending?.kind === "merge" ? "Merge" : "Trash"}
              </LabelledAction>
            </LabelledActions>
          </DialogContent>
        </Dialog>
    </>
  )
}
