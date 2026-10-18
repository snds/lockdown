import { useEffect, useState } from "react"

import { Field, FieldDescription, FieldLabel, Switch } from "@unicron/ui"

import { describeError } from "@/core/client"
import { useHygieneClient, type MemoryReport } from "@/features/hygiene/hygiene-client"

import { useQuickClient, type HotkeyStatus } from "./quick-client"

/**
 * Settings → Quick access and memory (docs/plan/26-desktop-finish.md §2, §7):
 * the system-wide shortcut, on by default, with the reason when this
 * computer won't give it (a Wayland session), and what the core keeps out
 * of swap. A shell with neither shows nothing.
 */
export function QuickSettings({ onNotice }: { onNotice: (text: string) => void }) {
  const quick = useQuickClient()
  const hygiene = useHygieneClient()
  const [status, setStatus] = useState<HotkeyStatus | null>(null)
  const [memory, setMemory] = useState<MemoryReport | null>(null)
  useEffect(() => {
    quick.hotkeyStatus().then(setStatus).catch(() => setStatus(null))
    hygiene.memory().then(setMemory).catch(() => setMemory(null))
  }, [quick, hygiene])
  if (!status && !memory) return null
  const toggle = async (on: boolean) => {
    try {
      setStatus(await quick.setHotkey(on))
    } catch (e) {
      onNotice(describeError(e))
    }
  }
  return (
    <section className="flex flex-col gap-3 rounded-xl border bg-card p-4" aria-labelledby="quick-settings">
      <h2 id="quick-settings" className="font-medium">
        Quick access
      </h2>
      {status && (
        <Field orientation="horizontal">
          <FieldLabel htmlFor="quick-hotkey">Open quick access from anywhere with {status.label}</FieldLabel>
          <Switch id="quick-hotkey" checked={status.on} onCheckedChange={(on) => void toggle(on)} />
        </Field>
      )}
      {status?.problem && (
        <p role="status" className="text-sm text-destructive">
          {status.problem}
        </p>
      )}
      {status && !status.problem && <FieldDescription>Ctrl+K inside Lockdown opens it too.</FieldDescription>}
      {memory && (
        <FieldDescription>
          {memory.supported
            ? `Lockdown keeps ${memory.pages_locked} ${memory.pages_locked === 1 ? "page" : "pages"} of keys out of swap now${memory.refused ? `; this computer refused ${memory.refused}` : ""}. ${memory.core_dumps_off ? "Crash dumps are off." : ""}`
            : "This build can't keep memory out of swap."}
        </FieldDescription>
      )}
    </section>
  )
}
