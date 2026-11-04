import { whenLocal } from "@/lib/format"
import type { BackupStatus } from "@/core/types"

export const INTERVALS: Array<[string, number]> = [
  ["Every hour", 1],
  ["Every 6 hours", 6],
  ["Every 12 hours", 12],
  ["Every day", 24],
  ["Every week", 168],
]

export const KEEP: number[] = [3, 5, 10, 20, 50]

/** "every day", for the status line. */
export function intervalWords(hours: number): string {
  const found = INTERVALS.find(([, h]) => h === hours)
  return found ? found[0].toLowerCase() : `every ${hours} hours`
}

/** The status in one sentence. */
export function statusLine(s: BackupStatus): string {
  if (!s.enabled || !s.folder) return "Automatic backups are off."
  return `On: ${intervalWords(s.interval_hours)} to ${s.folder}, keeping the newest ${s.keep_last}.`
}

/** When the last one worked, and when the next is due. */
export function whenLine(s: BackupStatus): string {
  const last = s.last_success_at ? `Last backup: ${whenLocal(s.last_success_at)}.` : "No backup yet."
  if (!s.enabled) return last
  const next = s.next_due_at ? ` Next: ${whenLocal(s.next_due_at)}, while Lockdown is unlocked.` : ""
  return `${last}${next}`
}

/** A file's size in plain words. */
export function sizeWords(bytes: number): string {
  if (bytes < 1024) return `${bytes} bytes`
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}

/** "Include crown jewels" (D138 as decided, doc 48): on unless the core says off. */
export function includesCrownJewels(s: BackupStatus): boolean {
  return s.include_crown_jewels !== false
}

/** The switch's one-line explanation, for its state. */
export function crownSwitchHelp(on: boolean): string {
  return on
    ? "Recovery phrases, recovery codes and your other crown jewels go into every locked backup, protected by its password."
    : "Crown jewels stay out of locked backups unless you include them one by one."
}

/** Said once after the switch starts carrying them: the newest backup lacks them, the next one won't. `null` otherwise. */
export function waitingLine(s: BackupStatus): string | null {
  const n = s.crown_jewels_waiting ?? 0
  if (!includesCrownJewels(s) || n <= 0) return null
  return `Your next backup will be the first to include your ${n} crown ${n === 1 ? "jewel" : "jewels"}.`
}

/** The toast after the switch changes. */
export function crownSwitchNotice(on: boolean): string {
  return on ? "Crown jewels will be in your next backup." : "Crown jewels stay out of new backups. Backups you already have keep what they hold."
}

/** The crown jewels a backup leaves out (D121), or `null`. */
export function crownLine(s: BackupStatus): string | null {
  if (includesCrownJewels(s) || !s.crown_jewels_left_out) return null
  const n = s.crown_jewels_left_out
  return `${n} crown ${n === 1 ? "jewel isn't" : "jewels aren't"} in your backups. To include one, open it and turn on "Include in exports and backups".`
}
