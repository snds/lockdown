import { useCallback } from "react"

import { useGate } from "@/app/gate"
import { cancelledWords, copiedWords } from "@/app/notify"
import { useSettings } from "@/app/settings"
import { describeError, isAppError, useClient } from "@/core/client"
import type { ItemSummary } from "@/core/types"

/** The core's answer when the person backed out of the gate: nothing moved, and the toast says so. */
const backedOut = (e: unknown) => isAppError(e) && e.code === "refused" && e.message.startsWith("Cancelled")

/**
 * A row's one copy (the palette, the detail's bottom row, a phone row's
 * swipe): the item's primary secret through `copy_secret`, or its code
 * through `copy_code`, by id and field, on the Rust clipboard path with the
 * settings' timer. The value never reaches the webview. A crown jewel's
 * copy, and an "Ask each time" item's, goes through the core's `Use` gate,
 * so it asks first (D101): the first call carries no proof, the core
 * refuses it, and the gate asks before the call that copies. Backing out of
 * the ask copies nothing and says so. The words are brief X2's toast:
 * "Copied the password for Steam, clears in 30 s".
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
        const { clears_in_secs: clears } =
          target.how === "code"
            ? await gate.run((g) => client.copyCode(row.id, target.field_id, secs, "code", g))
            : await gate.run((g) => client.copySecret(row.id, target.field_id, secs, g))
        onNotice(copiedWords(target.label, row.title, clears))
      } catch (e) {
        onNotice(backedOut(e) ? cancelledWords("copied") : describeError(e))
      }
    },
    [client, gate, settings.clipboard_clear_secs, onNotice],
  )
}

/**
 * A row's one-time code (a phone row's CODE swipe, brief V1): the code
 * valid now through `copy_code`, on the same clipboard path and timer, and
 * through the same gate as the quick copy. A row knows only how many codes
 * it holds, so the code's field comes from the core's codes list (as the
 * detail finds it); only its field id is kept, never a value. The toast is
 * X2's: "Copied the code for Steam, clears in 30 s".
 */
export function useCodeCopy(onNotice: (text: string) => void) {
  const client = useClient()
  const gate = useGate()
  const { settings } = useSettings()
  return useCallback(
    async (row: ItemSummary) => {
      const secs = settings.clipboard_clear_secs
      try {
        const field = (await client.codes()).find((c) => c.item_id === row.id)?.field_id
        if (!field) {
          onNotice(`No code to copy for ${row.title}`)
          return
        }
        const { clears_in_secs: clears } = await gate.run((g) => client.copyCode(row.id, field, secs, "code", g))
        onNotice(copiedWords("code", row.title, clears))
      } catch (e) {
        onNotice(backedOut(e) ? cancelledWords("copied") : describeError(e))
      }
    },
    [client, gate, settings.clipboard_clear_secs, onNotice],
  )
}
