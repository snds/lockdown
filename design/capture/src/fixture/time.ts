// The fixture's clock: every date in it is relative to the moment the page
// opened, so a review a month from now still shows "expires in 40 days".
// The capture pins the browser's clock, so its images are stable.

/** Seconds since the epoch when the harness started. */
export const NOW = Math.floor(Date.now() / 1000)

export const MIN = 60
export const HOUR = 60 * MIN
export const DAY = 24 * HOUR

/** Seconds since the epoch, `d` days ago (a fraction is fine). */
export const ago = (d: number) => NOW - Math.round(d * DAY)

/** Seconds since the epoch, `d` days from now. */
export const ahead = (d: number) => NOW + Math.round(d * DAY)

/** A local `YYYY-MM-DD`, `d` days from now. */
export function dateIn(d: number): string {
  const t = new Date((NOW + d * DAY) * 1000)
  return `${t.getFullYear()}-${String(t.getMonth() + 1).padStart(2, "0")}-${String(t.getDate()).padStart(2, "0")}`
}

/** A local `YYYY-MM`, `months` from now. */
export function monthIn(months: number): string {
  const t = new Date(NOW * 1000)
  t.setDate(1)
  t.setMonth(t.getMonth() + months)
  return `${t.getFullYear()}-${String(t.getMonth() + 1).padStart(2, "0")}`
}

/** Whole days until the end of the day or month `text` names (`lockdown_app::views::days_until`). */
export function daysUntil(text: string): number | null {
  const m = /^(\d{4})-(\d{2})(?:-(\d{2}))?$/.exec(text)
  if (!m) return null
  const y = Number(m[1])
  const mo = Number(m[2])
  const end = m[3] ? new Date(y, mo - 1, Number(m[3]), 23, 59, 59) : new Date(y, mo, 0, 23, 59, 59)
  return Math.floor((end.getTime() / 1000 - NOW) / DAY)
}
