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

/** The crown jewels a backup leaves out (D121), or `null`. */
export function crownLine(s: BackupStatus): string | null {
  if (!s.crown_jewels_left_out) return null
  const n = s.crown_jewels_left_out
  return `${n} crown ${n === 1 ? "jewel isn't" : "jewels aren't"} in your backups. To include one, open it and turn on "Include in exports and backups".`
}
