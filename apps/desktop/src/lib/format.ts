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
 * A count as every list, mark and overline writes one (cross-app decisions
 * §13): two digits (`04`), three from 100 (`248`), a comma from 1,000
 * (`1,204`). `cap` stops at `99+`, for a mark at an icon's corner.
 */
export function count(n: number, { cap = false }: { cap?: boolean } = {}): string {
  const whole = Math.max(0, Math.floor(n))
  if (cap && whole > 99) return "99+"
  if (whole < 100) return String(whole).padStart(2, "0")
  return whole.toLocaleString("en-US")
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
