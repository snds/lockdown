import { useCallback } from "react"

import { useGate } from "@/app/gate"
import { useSettings } from "@/app/settings"
import { describeError, isAppError, useClient } from "@/core/client"
import type { ItemSummary } from "@/core/types"

/**
 * A row's one copy (the palette and the list's copy button): the item's
 * primary secret through `copy_secret`, or its code through `copy_code`,
 * by id and field, on the Rust clipboard path with the settings' timer.
 * The value never reaches the webview. A crown jewel's copy goes through
 * the core's `Use` gate, so it asks first (D101); backing out of the ask
 * copies nothing and says so.
 */
export function useQuickCopy(onNotice: (text: string) => void) {
  const client = useClient()
  const gate = useGate()
  const { settings } = useSettings()
  return useCallback(
    async (row: ItemSummary) => {
      const target = row.copy
      if (!target) return
      const secs = settings.clipboard_clear_secs
      try {
        const clears =
          target.how === "code"
            ? await gate.run((g) => client.copyCode(row.id, target.field_id, secs, "code", g))
            : await gate.run((g) => client.copySecret(row.id, target.field_id, secs, g))
        onNotice(`Copied the ${target.label}. The clipboard clears in ${clears} seconds.`)
      } catch (e) {
        onNotice(isAppError(e) && e.code === "refused" && e.message.startsWith("Cancelled") ? "Nothing was copied." : describeError(e))
      }
    },
    [client, gate, settings.clipboard_clear_secs, onNotice],
  )
}
