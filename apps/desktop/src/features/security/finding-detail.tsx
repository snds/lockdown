import { useEffect, useMemo, useState } from "react"

import { AdaptiveMenu, AdaptiveMenuItem, AdaptiveMenuSeparator, Banner, Button, FieldRow, FieldRowAction, FloatButton, Icon, useAppFrame } from "@unicron/ui"

import { useDetailBottomRow } from "@/app/detail-bottom-row"
import { useDetailTopBar } from "@/components/master-detail"
import { useClient } from "@/core/client"
import type { Finding, ItemDetail, Playbook, VersionView } from "@/core/types"
import { fullDate } from "@/lib/format"

import { SetupGuideBlock } from "@/features/bridges/setup-guide"
import { ItemField } from "@/features/vault/item-field"

import { SkipMenu, fixFloat, fixSolid, useFix } from "./finding-actions"
import { dismissedLine } from "./finding-row"
import { PlaybookSteps } from "./playbook-dialog"
import { KIND_NAMES, SNOOZES, hostOf, snoozeUntil } from "./security-model"

/**
 * When the field last changed, from the item's history only (brief H2): the
 * newest version that changed a field with this label, or the first version
 * when none did since. `null` when the history does not say.
 */
export function lastChanged(versions: readonly VersionView[], label: string): number | null {
  const sorted = [...versions].sort((a, b) => b.n - a.n)
  const changed = sorted.find((v) => v.changed.includes(label))
  if (changed) return changed.edited_at
  const first = sorted.find((v) => v.n === 1)
  return first ? first.edited_at : null
}

interface FindingDetailProps {
  finding: Finding
  playbook: Playbook | null
  /** Its place in the fix lists, for the phone's overline (`HEALTH // FIX 1 OF 3`). */
  place: { at: number; of: number } | null
  busy: boolean
  onDismiss: (f: Finding, until: number | null) => void
  onRestore: (f: Finding) => void
  /** Opens an item inside Health (OPEN ITEM, ALSO USED ON). */
  onOpenItem: (id: string) => void
  onDevices: () => void
  onNotice: (text: string) => void
}

/**
 * A finding of any kind (brief H2; Mobile / 10, Desktop / 08): the kind in
 * words over the item's title and site; a Banner with the finding's own
 * words (critical when urgent); the field it is about as a SecretField with
 * REVEAL, when it last changed (from history) and where else it is used;
 * WHAT TO DO in place; the site's set-up guide when it has one; then one
 * primary, SKIP FOR NOW and OPEN ITEM. On a phone the primary is the bottom
 * row's float button and the other two sit behind the top bar's ⋯.
 */
export function FindingDetail({ finding: f, playbook, place, busy, onDismiss, onRestore, onOpenItem, onDevices, onNotice }: FindingDetailProps) {
  const client = useClient()
  const frame = useAppFrame()
  const [item, setItem] = useState<ItemDetail | null>(null)
  const [versions, setVersions] = useState<VersionView[] | null>(null)
  useEffect(() => {
    if (!f.item_id) return
    let live = true
    client
      .show(f.item_id)
      .then((d) => live && setItem(d))
      .catch(() => live && setItem(null))
    client
      .history(f.item_id)
      .then((v) => live && setVersions(v))
      .catch(() => live && setVersions(null))
    return () => {
      live = false
    }
  }, [client, f.item_id])

  const fix = useFix(f.dismissed ? null : f, { onNotice, onDevices, onOpenItem })
  const title = f.item_title ?? f.title
  const host = hostOf(f.site)
  const field = f.field_id && item && !item.crown_jewel ? (item.fields.find((x) => x.field_id === f.field_id) ?? null) : null
  const changedAt = field && versions ? lastChanged(versions, field.label) : null
  const opensItem = f.item_id !== null && f.kind !== "lookalike"
  const size = frame.phone ? "stacked" : "default"

  // A phone: the fix is the bottom row's primary (Mobile / 10), SKIP FOR NOW and OPEN ITEM the top bar's ⋯.
  const float = useMemo(() => (fix ? fixFloat(fix) : undefined), [fix])
  const inRow = useDetailBottomRow({ end: float })
  const more = useMemo(
    () =>
      f.dismissed ? null : (
        <AdaptiveMenu title="More for this finding" overline={title} trigger={<FloatButton size="l" icon="overflow-menu--horizontal" label="More for this finding" />}>
          {f.alert === null &&
            SNOOZES.map((s) => (
              <AdaptiveMenuItem key={s.days} icon="time" onSelect={() => onDismiss(f, snoozeUntil(s.days, Math.floor(Date.now() / 1000)))}>
                {s.label}
              </AdaptiveMenuItem>
            ))}
          <AdaptiveMenuItem icon="checkmark" onSelect={() => onDismiss(f, null)}>
            {f.kind === "missing_2fa" ? "I already use two-step sign-in here" : "Dismiss, I've dealt with it"}
          </AdaptiveMenuItem>
          {opensItem && <AdaptiveMenuSeparator />}
          {opensItem && (
            <AdaptiveMenuItem icon="arrow--right" onSelect={() => f.item_id && onOpenItem(f.item_id)}>
              Open item
            </AdaptiveMenuItem>
          )}
        </AdaptiveMenu>
      ),
    [f, title, opensItem, onDismiss, onOpenItem],
  )
  const inTopBar = useDetailTopBar(more)

  const overline = frame.phone && place ? `Health // fix ${place.at} of ${place.of}` : `Health // ${KIND_NAMES[f.kind]}`
  return (
    <article aria-labelledby="finding-title" className="flex flex-col gap-6 pb-12">
      <header className="flex flex-col gap-1">
        <p className="type-overline text-muted-foreground">{overline}</p>
        <h2 id="finding-title" className={`min-w-0 break-words text-foreground ${frame.phone ? "type-display-compact" : "type-display"}`}>
          {title}
        </h2>
        {host && <p className="type-meta text-muted-foreground">{host}</p>}
      </header>

      {f.dismissed ? (
        <Banner
          tone="info"
          title={`${dismissedLine(f)}.`}
          body={f.detail}
          action={
            <Button variant="secondary" size="sm" disabled={busy} onClick={() => onRestore(f)}>
              Bring back
            </Button>
          }
        />
      ) : (
        <Banner tone={f.severity === "urgent" || f.kind === "lookalike" ? "critical" : "warning"} title={f.title} body={f.detail} />
      )}

      {(field || changedAt !== null || f.related.length > 0 || f.field_id) && (
        <div className="flex flex-col">
          {field && <ItemField itemId={f.item_id!} field={field} onNotice={onNotice} size={size} />}
          {changedAt !== null && (
            <FieldRow size={size} label="Last changed">
              {fullDate(changedAt)}
            </FieldRow>
          )}
          {f.field_id && f.related.length === 0 && (
            <FieldRow size={size} label="Also used on">
              None
            </FieldRow>
          )}
          {f.related.map((r) => (
            <FieldRow
              key={r.id}
              size={size}
              label={f.kind === "lookalike" ? "Looks like" : "Also used on"}
              action={<FieldRowAction onClick={() => onOpenItem(r.id)}>Open</FieldRowAction>}
            >
              {r.title}
            </FieldRow>
          ))}
        </div>
      )}

      <PlaybookSteps playbook={playbook} />

      {f.kind === "missing_2fa" && f.item_id && <SetupGuideBlock itemId={f.item_id} onNotice={onNotice} />}

      {!f.dismissed && !inTopBar && (
        <div className="flex flex-wrap items-center gap-4">
          {fix && !inRow && fixSolid(fix)}
          <SkipMenu finding={f} onDismiss={onDismiss} busy={busy} />
          {opensItem && (
            <Button variant="ghost" onClick={() => f.item_id && onOpenItem(f.item_id)}>
              <Icon name="arrow--right" data-icon="inline-start" />
              Open item
            </Button>
          )}
        </div>
      )}
    </article>
  )
}
