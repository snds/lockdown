import { useState, type ReactNode } from "react"

import { ConfirmDialog } from "@unicron/ui"

import { useGate } from "@/app/gate"
import { describeError, useClient } from "@/core/client"
import type { ItemSummary } from "@/core/types"

import { HistoryView } from "./history-view"
import type { RowMenuHandlers } from "./row-menu"

/**
 * The layers every row's menu shares, drawn once for the list: the item's
 * history, and the question before deleting for good ("can't be undone",
 * so it asks; focus starts on Cancel). A crown jewel's purge asks it's you
 * through the gate as well.
 */
export function useRowMenuLayers({ onChanged, onError }: Pick<RowMenuHandlers, "onChanged" | "onError">): {
  onHistory: (row: ItemSummary) => void
  onPurge: (row: ItemSummary) => void
  layers: ReactNode
} {
  const client = useClient()
  const gate = useGate()
  const [history, setHistory] = useState<ItemSummary | null>(null)
  const [purging, setPurging] = useState<ItemSummary | null>(null)
  const title = (r: ItemSummary | null) => r?.title || "this item"
  const purge = async () => {
    const row = purging
    if (!row) return
    try {
      await gate.run((g) => client.purge(row.id, g))
      setPurging(null)
      onChanged(`Deleted ${title(row)} for good`)
    } catch (e) {
      setPurging(null)
      onError(describeError(e))
    }
  }
  const layers = (
    <>
      <HistoryView
        itemId={history?.id ?? null}
        title={title(history)}
        editable={history !== null && history.state !== "trashed"}
        onClose={() => setHistory(null)}
        onRestored={(notice) => {
          setHistory(null)
          onChanged(notice)
        }}
      />
      <ConfirmDialog
        open={purging !== null}
        onOpenChange={(o) => !o && setPurging(null)}
        tone="destructive"
        title={`Delete "${title(purging)}" for good?`}
        description="Its keys are destroyed now, here and on every device at the next sync, so no copy anywhere can be read again. This can't be undone."
        confirmLabel="Delete for good"
        onConfirm={() => purge()}
      />
    </>
  )
  return { onHistory: setHistory, onPurge: setPurging, layers }
}
