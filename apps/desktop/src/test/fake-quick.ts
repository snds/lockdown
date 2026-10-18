import type { ItemSummary } from "@/core/types"
import { matches, searchWords } from "@/features/palette/palette-model"
import type { QuickClient, QuickHit } from "@/features/quick/quick-client"

/**
 * A stand-in for the core's quick-access index in component tests: live
 * rows, every word a substring of the summary's words. The real ranking and
 * typo tolerance are the core's, tested in Rust (`lockdown_vault::search`);
 * `typos` marks rows the fake should answer as close matches for a query.
 */
export function fakeQuick(rows: ItemSummary[], over: Partial<QuickClient> = {}, typos: Record<string, string[]> = {}): QuickClient {
  return {
    search: (query) => {
      const live = rows.filter((r) => r.state === "live")
      const exact: QuickHit[] = live.filter((r) => matches(query, searchWords(r, ""))).map((r) => ({ ...r, exact: true }))
      const close: QuickHit[] = live.filter((r) => (typos[query] ?? []).includes(r.id)).map((r) => ({ ...r, exact: false }))
      return Promise.resolve([...exact, ...close])
    },
    quickCode: () => Promise.resolve(new TextEncoder().encode("123456")),
    quickFill: (id) => {
      const row = rows.find((r) => r.id === id)
      return Promise.resolve({ url: row?.site ?? "", host: new URL(row?.site || "https://x.invalid").host, waits_secs: 120 })
    },
    canFill: true,
    hide: () => Promise.resolve(),
    openMain: () => Promise.resolve(),
    hotkeyStatus: () => Promise.resolve(null),
    setHotkey: () => Promise.resolve(null),
    ...over,
  }
}
