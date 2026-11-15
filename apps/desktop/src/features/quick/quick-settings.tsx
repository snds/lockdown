import { useEffect, useState } from "react"

import { FieldRow, Kbd, SectionHeader, ToggleRow } from "@unicron/ui"

import { describeError } from "@/core/client"
import { useHygieneClient, type MemoryReport } from "@/features/hygiene/hygiene-client"
import { isMac } from "@/features/palette/palette-model"
import { SaveError, useGroupLevel, useRowSize } from "@/features/settings/setting-rows"

import { useQuickClient, type HotkeyStatus } from "./quick-client"

/** What the core keeps out of swap, as one row's value and line (brief T6). */
export function memoryWords(memory: MemoryReport): { value: string; line: string | null } {
  if (!memory.supported) return { value: "Not on this build", line: "This build can't keep memory out of swap." }
  const pages = memory.pages_locked === 1 ? "1 page of keys" : `${memory.pages_locked} pages of keys`
  const line = [memory.refused ? `This computer refused ${memory.refused}.` : null, memory.core_dumps_off ? "Crash dumps are off." : null].filter(Boolean).join(" ")
  return { value: pages, line: line || null }
}

/**
 * Settings → Quick access (brief T6, doc 80; docs/plan/26-desktop-finish.md
 * §2, §7): the system-wide shortcut as a switch with the shortcut as a Kbd,
 * the core's reason in critical text when this computer won't give it (a
 * Wayland session, a shortcut taken), and in Details what the core keeps
 * out of swap. A shell with neither shows nothing.
 */
export function QuickSettings({ onNotice: _onNotice }: { onNotice: (text: string) => void }) {
  const quick = useQuickClient()
  const hygiene = useHygieneClient()
  const level = useGroupLevel()
  const size = useRowSize()
  const [status, setStatus] = useState<HotkeyStatus | null>(null)
  const [memory, setMemory] = useState<MemoryReport | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  useEffect(() => {
    quick.hotkeyStatus().then(setStatus).catch(() => setStatus(null))
    hygiene.memory().then(setMemory).catch(() => setMemory(null))
  }, [quick, hygiene])
  if (!status && !memory) return null

  // A switch saves silently (brief T0); a refusal keeps the old state with one critical line.
  const toggle = async (on: boolean) => {
    setBusy(true)
    setError(null)
    try {
      setStatus(await quick.setHotkey(on))
    } catch (e) {
      setError(describeError(e))
    } finally {
      setBusy(false)
    }
  }
  const kept = memory ? memoryWords(memory) : null

  return (
    <div className="flex flex-col gap-8">
      {status && (
        <div className="flex flex-col">
          <ToggleRow
            label={
              <>
                Open quick access from anywhere <Kbd>{status.label}</Kbd>
              </>
            }
            description={status.problem ? <span className="text-critical-text">{status.problem}</span> : undefined}
            checked={status.on}
            disabled={busy}
            onCheckedChange={(on) => void toggle(on)}
          />
          <SaveError text={error} />
          <p className="type-caption text-text-secondary pt-3">
            <Kbd>{isMac() ? "⌘K" : "Ctrl+K"}</Kbd> inside Lockdown opens it too.
          </p>
        </div>
      )}
      {kept && (
        <SectionHeader level={level} title="Details" collapsible summary={kept.value}>
          <FieldRow size={size} label="Out of swap" description={kept.line ?? undefined}>
            {kept.value}
          </FieldRow>
        </SectionHeader>
      )}
    </div>
  )
}
