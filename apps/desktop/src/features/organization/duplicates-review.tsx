import { useCallback, useEffect, useMemo, useState } from "react"

import { Button, ConfirmDialog, EmptyState, FloatButton, ListRow, RadioGroup, RadioGroupItem, SectionHeader, Skeleton } from "@unicron/ui"

import { useDetailBottomRow } from "@/app/detail-bottom-row"

import { useGate } from "@/app/gate"
import { notify, deletedWords } from "@/app/notify"
import { describeError, useClient } from "@/core/client"
import type { DuplicateGroup, ItemSummary } from "@/core/types"
import { count, countName, fullDate } from "@/lib/format"
import { hostOf } from "@/features/security/security-model"

import { useVaults, vaultName } from "./vaults-context"

/**
 * The duplicate finder's groups (02-model §11), read when Duplicates opens
 * and again after each merge or delete. The core decides what matches with a
 * keyed hash it throws away after the search, so no secret and no
 * fingerprint of one comes to this window. `null` until they arrive; it
 * runs once `enabled` (Duplicates opened), and the groups are kept after.
 */
export function useDuplicates(enabled = true) {
  const client = useClient()
  const [groups, setGroups] = useState<DuplicateGroup[] | null>(null)
  const [failed, setFailed] = useState<string | null>(null)
  const [round, setRound] = useState(0)
  useEffect(() => {
    if (!enabled) return
    let live = true
    client
      .duplicates()
      .then((g) => {
        if (!live) return
        setGroups(g)
        setFailed(null)
      })
      .catch((e) => live && setFailed(describeError(e)))
    return () => {
      live = false
    }
  }, [client, round, enabled])
  const reload = useCallback(() => setRound((r) => r + 1), [])
  return { groups, failed, reload }
}

/** A group's key: its reason and its items. */
export const groupKey = (g: DuplicateGroup): string => `${g.reason}:${g.ids.join(",")}`

/** Why the core put the items together, as the row's right-hand word. */
export const REASON_WORDS: Record<DuplicateGroup["reason"], string> = {
  "same-login": "Same login",
  "same-password": "Same password",
  "same-code": "Same code",
}

/** What the DUPLICATES row on Health says: "Not checked", "4 groups" or "None". */
export function duplicatesWords(groups: DuplicateGroup[] | null): string {
  if (groups === null) return "Not checked"
  if (groups.length === 0) return "None"
  return countName(groups.length, "group")
}

interface DuplicatesListProps {
  groups: DuplicateGroup[] | null
  failed: string | null
  chosen: string | null
  onChoose: (key: string) => void
  onTryAgain: () => void
}

/**
 * Duplicates, the list (brief V3): POSSIBLE DUPLICATES and one row per
 * group, its words as the title, how many items, why they match.
 */
export function DuplicatesList({ groups, failed, chosen, onChoose, onTryAgain }: DuplicatesListProps) {
  if (failed && !groups) return <EmptyState tone="critical" overline="Duplicates // not read" title="Couldn't look for duplicates." body={failed} actionLabel="Try again" onAction={onTryAgain} />
  if (!groups)
    return (
      <div aria-busy="true" aria-label="Looking for duplicates" className="flex flex-col">
        <Skeleton variant="row" />
        <Skeleton variant="row" />
      </div>
    )
  if (groups.length === 0) return <EmptyState overline={`Duplicates // ${count(0)}`} title="No duplicates." body="Nothing shares a website and username, a password or a code set-up." />
  return (
    <section aria-labelledby="possible-duplicates" className="flex flex-col">
      <SectionHeader id="possible-duplicates" title="Possible duplicates" count={groups.length} />
      <ul aria-label="Possible duplicates" className="flex flex-col">
        {groups.map((g) => (
          <ListRow
            key={groupKey(g)}
            render={<li />}
            metaStyle="plain"
            selected={chosen === groupKey(g)}
            title={g.words}
            meta={countName(g.ids.length, "item")}
            status={REASON_WORDS[g.reason]}
            onClick={() => onChoose(groupKey(g))}
          />
        ))}
      </ul>
    </section>
  )
}

interface DuplicateDetailProps {
  group: DuplicateGroup
  rows: ItemSummary[]
  /** Something was merged or deleted: read the groups and the list again. */
  onChanged: () => void
  /** Opens one of the copies inside Health. */
  onOpen: (id: string) => void
}

/**
 * One group (brief V3). A group that can merge: "Keep which?" as a radio
 * group of rows (the most recently changed chosen first), what joins it and
 * where the rest go, and one MERGE that asks first, through the gate when a
 * crown jewel is in it (D152). A merge cannot be undone: its toast has no
 * UNDO, and the others stay in Recently deleted. A group that cannot merge:
 * each copy with OPEN, and DELETE on every copy but the first, which moves
 * it to Recently deleted with UNDO.
 */
export function DuplicateDetail({ group, rows, onChanged, onOpen }: DuplicateDetailProps) {
  const client = useClient()
  const gate = useGate()
  const { vaults } = useVaults()
  const byId = useMemo(() => new Map(rows.map((r) => [r.id, r])), [rows])
  const newest = useMemo(() => [...group.ids].sort((a, b) => (byId.get(b)?.edited_at ?? 0) - (byId.get(a)?.edited_at ?? 0))[0] ?? "", [group.ids, byId])
  const [keep, setKeep] = useState(newest)
  const [asking, setAsking] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const titleOf = (id: string) => byId.get(id)?.title || "this item"
  const others = group.ids.filter((x) => x !== keep)

  const merge = async () => {
    setError(null)
    try {
      await gate.run((g) => client.mergeDuplicates(keep, others, g))
      const kept = byId.get(keep)
      notify(`Merged into ${kept?.site ? hostOf(kept.site) : titleOf(keep)}`)
      onChanged()
    } catch (e) {
      setError(describeError(e))
      throw e
    }
  }
  const remove = async (id: string) => {
    try {
      await client.trash(id)
      notify(deletedWords(titleOf(id)), {
        action: { label: "Undo", onClick: () => void client.restore(id).then(onChanged).catch((e) => notify(describeError(e))) },
        takeFocus: true,
      })
      onChanged()
    } catch (e) {
      notify(describeError(e))
    }
  }
  const line = (r: ItemSummary | undefined) =>
    r ? [vaults.length > 1 ? vaultName(vaults, r.vault_id) : null, `changed ${fullDate(r.edited_at)}`].filter(Boolean).join(" // ") : undefined

  // On a phone MERGE sits at the end of the pushed screen's bottom row (brief V3).
  const mergeFloat = useMemo(
    () => (group.mergeable ? <FloatButton kind="primary" size="l" extended icon="merge" label="Merge" onClick={() => setAsking(true)} /> : undefined),
    [group.mergeable],
  )
  const inRow = useDetailBottomRow({ end: mergeFloat })
  return (
    <article aria-labelledby="duplicates-title" className="flex flex-col gap-6">
      <header className="flex flex-col gap-1">
        <p className="type-overline text-muted-foreground">{`Duplicates // ${REASON_WORDS[group.reason]}`}</p>
        <h2 id="duplicates-title" className="type-title">
          {group.words}
        </h2>
      </header>
      {group.mergeable ? (
        <>
          <section aria-labelledby="keep-which" className="flex flex-col">
            <SectionHeader id="keep-which" title="Keep which?" count={group.ids.length} />
            <RadioGroup aria-labelledby="keep-which" value={keep} onValueChange={(v) => setKeep(String(v))}>
              <ul className="flex flex-col">
                {group.ids.map((id) => (
                  <ListRow
                    key={id}
                    render={<li />}
                    metaStyle="plain"
                    leading={<RadioGroupItem value={id} aria-label={titleOf(id)} />}
                    title={titleOf(id)}
                    meta={line(byId.get(id))}
                    action={
                      <Button size="sm" variant="ghost" onClick={() => onOpen(id)} aria-label={`Open ${titleOf(id)}`}>
                        Open
                      </Button>
                    }
                  />
                ))}
              </ul>
            </RadioGroup>
          </section>
          <p className="type-caption text-muted-foreground">Websites, codes, passkeys, notes and tags from the others join it. The others go to Recently deleted.</p>
          {!inRow && (
            <div className="flex flex-col gap-2">
              <div>
                <Button variant="secondary" onClick={() => setAsking(true)}>
                  Merge
                </Button>
              </div>
              {error && (
                <p role="alert" className="type-caption text-destructive">
                  {error}
                </p>
              )}
            </div>
          )}
          <ConfirmDialog
            open={asking}
            onOpenChange={setAsking}
            overline={REASON_WORDS[group.reason]}
            title={`Merge into "${titleOf(keep)}"?`}
            description={`Websites, codes, passkeys, notes and tags from ${others.length === 1 ? `"${titleOf(others[0] ?? "")}"` : `${others.length} copies`} are added to it, and ${others.length === 1 ? "that copy goes" : "they go"} to Recently deleted. Its own password stays; a different password on a copy stays readable there.`}
            confirmLabel="Merge"
            onConfirm={merge}
            error={error}
          />
        </>
      ) : (
        <section aria-labelledby="copies" className="flex flex-col">
          <SectionHeader id="copies" title="Copies" count={group.ids.length} />
          <ul className="flex flex-col">
            {group.ids.map((id, n) => (
              <ListRow
                key={id}
                render={<li />}
                metaStyle="plain"
                title={titleOf(id)}
                meta={line(byId.get(id))}
                action={
                  <span className="flex gap-1">
                    <Button size="sm" variant="ghost" onClick={() => onOpen(id)} aria-label={`Open ${titleOf(id)}`}>
                      Open
                    </Button>
                    {n > 0 && (
                      <Button size="sm" variant="ghost" onClick={() => void remove(id)} aria-label={`Delete ${titleOf(id)}`}>
                        Delete
                      </Button>
                    )}
                  </span>
                }
              />
            ))}
          </ul>
        </section>
      )}
    </article>
  )
}

