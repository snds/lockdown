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
