import { useNeedsYouSource, type NeedsYouEntry } from "@/app/needs-you"

/** Registers one request's needs-you row while it waits (brief X3 rows 1 to 3). */
export function RequestSource({ entry }: { entry: NeedsYouEntry | null }) {
  useNeedsYouSource(entry)
  return null
}
