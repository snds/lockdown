import { useCallback, useState, type ReactNode } from "react"

import { AdaptiveMenu, AdaptiveMenuRadioGroup, AdaptiveMenuRadioItem, FieldRow, FieldRowAction, useAppFrame } from "@unicron/ui"

import { useSettings } from "@/app/settings"
import { describeError } from "@/core/client"
import type { Settings } from "@/core/types"

/**
 * How every setting saves (brief T0, cross-app decisions §5): at once, and
 * silently. The core writes `settings.cbor`, and only an answer from it
 * changes what the control shows, so a save that fails leaves the control
 * on its old value by itself; the row then shows one critical line in the
 * core's words until the next try.
 */
export function useSettingSave(after?: () => Promise<unknown> | void) {
  const { settings, update } = useSettings()
  const [errors, setErrors] = useState<Record<string, string>>({})
  const [busy, setBusy] = useState<string | null>(null)

  const save = useCallback(
    async (row: string, change: Partial<Settings>) => {
      setBusy(row)
      setErrors(({ [row]: _gone, ...rest }) => rest)
      try {
        await update(change)
        await after?.()
      } catch (e) {
        setErrors((all) => ({ ...all, [row]: describeError(e) }))
      } finally {
        setBusy(null)
      }
    },
    [update, after],
  )

  return { settings, save, busy, errorOf: (row: string): string | null => errors[row] ?? null }
}

/** The one critical line under a row whose save failed, in the core's words. */
export function SaveError({ text }: { text: string | null }) {
  if (!text) return null
  return (
    <p role="alert" className="type-caption text-critical-text pt-1 pb-3">
      {text}
    </p>
  )
}

/** A row's size: Figma draws Settings' rows stacked on a phone and in one line on a desktop. */
export function useRowSize(): "stacked" | "default" {
  return useAppFrame().phone ? "stacked" : "default"
}

/**
 * A group's heading level: under the section head's `h2` on a desktop, and
 * straight under the page's title on a phone, where the drawn rows have no
 * section head.
 */
export function useGroupLevel(): 2 | 3 {
  return useAppFrame().phone ? 2 : 3
}

export interface Choice {
  /** The menu's words for it ("5 minutes"). */
  label: string
  /** What the row's value reads while it is the one ("After 5 minutes"). */
  value: string
}

interface ChangeSettingProps {
  /** The row's label, which also titles the menu ("Auto-lock"). */
  label: string
  /** The choices, by a key of the host's: five or more, so they sit behind CHANGE (cross-app §5). */
  choices: ReadonlyArray<readonly [string, Choice]>
  /** The current one's key. */
  current: string
  onPick: (key: string) => void
  description?: ReactNode
  busy?: boolean
  error?: string | null
}

/**
 * A setting of five or more choices (brief T0): its value in the row and
 * CHANGE at the end, which opens a menu on a desktop and a sheet on a
 * phone with the current one marked. Picking one saves it.
 */
export function ChangeSetting({ label, choices, current, onPick, description, busy, error }: ChangeSettingProps) {
  const size = useRowSize()
  const shown = choices.find(([key]) => key === current)?.[1].value ?? ""
  return (
    <div>
      <FieldRow
        size={size}
        label={label}
        description={description}
        action={
          <AdaptiveMenu title={label} overline="Change" trigger={<FieldRowAction disabled={busy}>Change</FieldRowAction>}>
            <AdaptiveMenuRadioGroup label={label} value={current} onValueChange={(key) => key !== current && onPick(key)}>
              {choices.map(([key, choice]) => (
                <AdaptiveMenuRadioItem key={key} value={key}>
                  {choice.label}
                </AdaptiveMenuRadioItem>
              ))}
            </AdaptiveMenuRadioGroup>
          </AdaptiveMenu>
        }
      >
        {shown}
      </FieldRow>
      <SaveError text={error ?? null} />
    </div>
  )
}

/** Auto-lock's choices (brief T1), keyed by seconds; the web has no Never. */
export function idleChoices(surface: "desktop" | "web"): Array<[string, Choice]> {
  const out: Array<[string, Choice]> = [
    ["60", { label: "1 minute", value: "After 1 minute" }],
    ["300", { label: "5 minutes", value: "After 5 minutes" }],
    ["600", { label: "10 minutes", value: "After 10 minutes" }],
    ["1800", { label: "30 minutes", value: "After 30 minutes" }],
    ["3600", { label: "1 hour", value: "After 1 hour" }],
    ["14400", { label: "4 hours", value: "After 4 hours" }],
  ]
  if (surface === "desktop") out.push(["never", { label: "Never", value: "Never" }])
  return out
}

/** The key of an idle time in `idleChoices`. */
export const idleKey = (secs: number | null): string => (secs === null ? "never" : String(secs))
/** The seconds of a key from `idleChoices`. */
export const idleSecs = (key: string): number | null => (key === "never" ? null : Number(key))

/** Clipboard's choices (brief T1), keyed by seconds. */
export const CLIPBOARD_CHOICES: ReadonlyArray<[string, Choice]> = [10, 20, 30, 45, 60, 90].map((s) => [String(s), { label: `${s} seconds`, value: `Clears after ${s} s` }])
