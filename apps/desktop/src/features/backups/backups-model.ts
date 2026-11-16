import type { BackupStatus } from "@/core/types"
import { rowDate } from "@/lib/format"

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

const two = (n: number) => String(n).padStart(2, "0")
const clock = (d: Date) => `${two(d.getHours())}:${two(d.getMinutes())}`

/**
 * The LAST BACKUP row's words (brief T8; Lockdown 11, the cross-app clock
 * rule): "Backed up Today 09:02", or "No backup yet". How long ago is the
 * row's tip (`ClockTime`), never the row's words.
 */
export function lastBackupLine(s: BackupStatus, nowSecs: number = Date.now() / 1000): string {
  if (!s.last_success_at) return "No backup yet"
  return `Backed up ${rowDate(s.last_success_at, nowSecs)}`
}

/** When the next is due, under the LAST BACKUP row; `null` when backups are off. */
export function nextLine(s: BackupStatus, nowSecs: number = Date.now() / 1000): string | null {
  if (!s.enabled || !s.next_due_at) return null
  const when = "while Lockdown is unlocked"
  if (s.next_due_at <= nowSecs) return `Due now, ${when}.`
  const d = new Date(s.next_due_at * 1000)
  const today = new Date(nowSecs * 1000).toDateString() === d.toDateString()
  const at = today ? `at ${clock(d)}` : `${d.getFullYear()}.${two(d.getMonth() + 1)}.${two(d.getDate())} ${clock(d)}`
  return `Next ${at}, ${when}.`
}

/** A choice of HOW OFTEN, keyed by hours, for the CHANGE menu. */
export const INTERVAL_CHOICES = INTERVALS.map(([label, hours]) => [String(hours), { label, value: label }] as const)

/** A choice of KEEP, for the CHANGE menu. */
export const KEEP_CHOICES = KEEP.map((n) => [String(n), { label: `The newest ${n}`, value: `The newest ${n}` }] as const)

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
    : "Crown jewels stay out of locked backups unless you include them one by one. Backups you already have keep what they hold."
}

/** Said once after the switch starts carrying them: the newest backup lacks them, the next one won't. `null` otherwise. */
export function waitingLine(s: BackupStatus): string | null {
  const n = s.crown_jewels_waiting ?? 0
  if (!includesCrownJewels(s) || n <= 0) return null
  return `Your next backup will be the first to include your ${n} crown ${n === 1 ? "jewel" : "jewels"}.`
}

/** The toast after the switch changes. */
export function crownSwitchNotice(on: boolean): string {
  return on ? "Crown jewels go in your next backup" : "Crown jewels stay out of new backups"
}

/** The crown jewels a backup leaves out (D121), or `null`. */
export function crownLine(s: BackupStatus): string | null {
  if (includesCrownJewels(s) || !s.crown_jewels_left_out) return null
  const n = s.crown_jewels_left_out
  return `${n} crown ${n === 1 ? "jewel isn't" : "jewels aren't"} in your backups. To include one, open it and turn on "Include in exports and backups".`
}
