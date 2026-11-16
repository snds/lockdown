import { useEffect, useMemo, useState } from "react"

import { AdaptiveDialog, FieldRow, FieldRowValue, Kbd, KbdGroup } from "@unicron/ui"

import { isMac } from "@/features/palette/palette-model"
import { useQuickClient, type HotkeyStatus } from "@/features/quick/quick-client"

import { shortcuts, type QuickWindowKey } from "./keys"

interface KeysDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}

/**
 * The keys list (brief X6, in Vale X7's form): `?` opens it. A form-sized
 * dialog with the overline KEYS, the title "Keyboard shortcuts", and a
 * Field Row for every key, the key itself as a `Kbd` in the label. It asks
 * nothing, so it has no primary: the close button and Escape put it away.
 *
 * The system-wide shortcut is read from the core when the list opens, so
 * the list says the key this computer uses, and says when it is off.
 */
export function KeysDialog({ open, onOpenChange }: KeysDialogProps) {
  const quick = useQuickClient()
  const [quickWindow, setQuickWindow] = useState<QuickWindowKey | null>(null)
  useEffect(() => {
    if (!open) return
    let live = true
    quick
      .hotkeyStatus()
      .then((s) => live && setQuickWindow(s ? { label: s.label, problem: hotkeyProblem(s) } : null))
      .catch(() => live && setQuickWindow(null))
    return () => {
      live = false
    }
  }, [open, quick])
  const rows = useMemo(() => shortcuts({ mac: isMac(), quickWindow }), [quickWindow])
  return (
    <AdaptiveDialog open={open} onOpenChange={onOpenChange} size="form" overline="Keys" title="Keyboard shortcuts">
      <div role="list" aria-label="Keys">
        {rows.map((s) => (
          <div role="listitem" key={s.id}>
            <FieldRow
              label={
                <KbdGroup>
                  {s.keys.map((k) => (
                    <Kbd key={k}>{k}</Kbd>
                  ))}
                </KbdGroup>
              }
              description={s.note}
              rule={s.id === "quick-access" ? "none" : "top"}
            >
              {/* What a key does is reading text, in the body role; only the key is code, in mono (Lockdown 12). */}
              <FieldRowValue truncate="wrap">
                <span className="type-body text-text-body">{s.does}</span>
              </FieldRowValue>
            </FieldRow>
          </div>
        ))}
      </div>
    </AdaptiveDialog>
  )
}

/** What stands between the system-wide shortcut and working, if anything. */
function hotkeyProblem(s: HotkeyStatus): string | undefined {
  if (!s.on) return "Off. Turn it on in Settings, Quick access."
  if (!s.registered) return s.problem ?? "This computer didn't give Lockdown the shortcut."
  return undefined
}
