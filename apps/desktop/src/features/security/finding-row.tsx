import { Button, ListRow } from "@unicron/ui"

import type { Finding } from "@/core/types"
import { fullDate } from "@/lib/format"

import { KIND_WORDS, hostOf } from "./security-model"

interface FindingRowProps {
  finding: Finding
  /** The finding open beside the list: primary-soft with the 2 px edge. */
  selected?: boolean
  /** Opens the finding (brief H2). */
  onOpen: (f: Finding) => void
  /** A dismissed or snoozed finding's BRING BACK. */
  onRestore?: (f: Finding) => void
  busy?: boolean
}

/** What a dismissed finding's mono line says: when, and until when for a snooze. */
export function dismissedLine(f: Finding): string {
  if (!f.dismissed) return ""
  return f.dismissed.until ? `Snoozed until ${fullDate(f.dismissed.until)}` : `Dismissed ${fullDate(f.dismissed.at)}`
}

/**
 * One finding in Health's lists (brief H1): the item's title (an account
 * alert's own title), its site as the mono line, the kind as the word at
 * the end. A press opens it beside the list, or as a pushed screen. A
 * dismissed finding says when, and offers BRING BACK in the row.
 */
export function FindingRow({ finding: f, selected = false, onOpen, onRestore, busy = false }: FindingRowProps) {
  const title = f.item_title ?? f.title
  const host = hostOf(f.site)
  const meta = f.dismissed ? [host, dismissedLine(f)].filter(Boolean).join(" // ") : host || undefined
  return (
    <ListRow
      layout="inline"
      metaStyle="plain"
      render={<li />}
      selected={selected}
      title={title}
      meta={meta}
      status={KIND_WORDS[f.kind]}
      onClick={() => onOpen(f)}
      action={
        f.dismissed && onRestore ? (
          <Button size="sm" variant="ghost" disabled={busy} onClick={() => onRestore(f)} aria-label={`Bring back: ${title}`}>
            Bring back
          </Button>
        ) : undefined
      }
    />
  )
}
