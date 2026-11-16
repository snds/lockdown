import { formatNavCount } from "@unicron/ui"

/** Seconds since the Unix epoch, as a short local date and time. */
export function whenLocal(secs: number | null | undefined): string {
  if (!secs) return "never"
  return new Date(secs * 1000).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" })
}

/** Seconds since the Unix epoch, as a local date. */
export function dateLocal(secs: number): string {
  return new Date(secs * 1000).toLocaleDateString(undefined, { dateStyle: "medium" })
}

/** The first eight characters of an id, for rows. */
export function shortId(id: string): string {
  return id.slice(0, 8)
}

/**
 * A count in an overline, a chip, a badge or a mark, as every list writes
 * one (the cross-app settlement, Unicron 13): three digits (`012`), a comma
 * from 1,000 (`1,204`), through unicron 1.1's own formatter so the app and
 * its parts never disagree. `cap` is a mark at an icon's corner: two digits,
 * stopping at `99+`. A count inside a sentence is a plain number (`countName`).
 */
export function count(n: number, { cap = false }: { cap?: boolean } = {}): string {
  return formatNavCount(Math.max(0, Math.floor(n)), { cap })
}

/** What a count is called aloud: the plain number and its noun ("1 item", "61 items", "3 to fix"). */
export function countName(n: number, one: string, many = `${one}s`): string {
  return `${n.toLocaleString("en-US")} ${n === 1 ? one : many}`
}

/** A countdown (cross-app decisions §13): `NN S` under a minute, `MM:SS` from one. Never below zero. */
export function countdown(secs: number): string {
  const left = Math.max(0, Math.ceil(secs))
  if (left < 60) return `${String(left).padStart(2, "0")} S`
  return `${String(Math.floor(left / 60)).padStart(2, "0")}:${String(left % 60).padStart(2, "0")}`
}

const two = (n: number) => String(n).padStart(2, "0")
const DAYS = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"]
const MONTHS = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"]
const dayStart = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime()

/**
 * A date in a row, as the frames draw them (brief 4.0 Data, until Sean
 * answers Q20): `Today 09:02`, `Yesterday 18:40`, within a week `MON 09:02`,
 * earlier this year `NOV 02`, before that `2026.10.02`. Local time; a date
 * ahead of now is written in full.
 */
export function rowDate(secs: number, nowSecs: number = Date.now() / 1000): string {
  const d = new Date(secs * 1000)
  const now = new Date(nowSecs * 1000)
  const days = Math.round((dayStart(now) - dayStart(d)) / 86_400_000)
  const clock = `${two(d.getHours())}:${two(d.getMinutes())}`
  if (days === 0) return `Today ${clock}`
  if (days === 1) return `Yesterday ${clock}`
  if (days > 1 && days < 7) return `${DAYS[d.getDay()]} ${clock}`
  if (days >= 7 && d.getFullYear() === now.getFullYear()) return `${MONTHS[d.getMonth()]} ${two(d.getDate())}`
  return fullDate(secs)
}

/** A date in full, as the frames write one: `2026.10.02`. */
export function fullDate(secs: number): string {
  const d = new Date(secs * 1000)
  return `${d.getFullYear()}.${two(d.getMonth() + 1)}.${two(d.getDate())}`
}

/**
 * How long ago, in words, for the tooltip beside a row's clock time (the
 * cross-app rule: rows say the clock time; the relative age is the tip):
 * `just now`, `5 minutes ago`, `3 hours ago`, `2 days ago`.
 */
export function ageWords(secs: number, nowSecs: number = Date.now() / 1000): string {
  const s = Math.max(0, nowSecs - secs)
  const say = (n: number, unit: string) => `${n} ${unit}${n === 1 ? "" : "s"} ago`
  if (s < 60) return "just now"
  if (s < 3600) return say(Math.floor(s / 60), "minute")
  if (s < 86_400) return say(Math.floor(s / 3600), "hour")
  return say(Math.floor(s / 86_400), "day")
}
