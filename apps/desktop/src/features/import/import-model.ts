import { isAppError } from "@/core/client"
import type { ImportPreview, ImportReport } from "@/core/types"

/** The wizard's choice that lets the core tell the format from the file. */
export const AUTO = "auto"

/** Whether a refusal means the file has a password of its own and none (or the wrong one) was given. */
export function passwordAsked(e: unknown): "needed" | "wrong" | null {
  if (!isAppError(e)) return null
  if (/protected with a password/i.test(e.message)) return "needed"
  if (/password didn't open the file/i.test(e.message)) return "wrong"
  return null
}

const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`

/** One sentence on what the file holds, for the preview's heading. */
export function summaryLine(p: ImportPreview): string {
  if (p.total === 0) return "This file has nothing Lockdown can bring in."
  const restore = p.updated + p.unchanged > 0
  if (restore) {
    const parts = [`${p.new} new`]
    if (p.updated) parts.push(`${p.updated} changed since the backup was made`)
    if (p.unchanged) parts.push(`${p.unchanged} already here as they are`)
    return `${plural(p.total, "item")} in this file: ${parts.join(", ")}.`
  }
  return `${plural(p.total, "item")} ready to bring in.`
}

/** The kinds, largest first: "12 logins", "3 payment cards". */
export function kindLines(p: ImportPreview): string[] {
  return [...p.by_kind]
    .sort((a, b) => b.count - a.count || a.name.localeCompare(b.name))
    .map((k) => `${k.count} × ${k.name}`)
}

/** Facts worth a line of their own under the kinds. */
export function factLines(p: ImportPreview): string[] {
  const out: string[] = []
  if (p.attachments) out.push(`${plural(p.attachments, "attached file")} come along.`)
  if (p.crown_jewels)
    out.push(
      `${plural(p.crown_jewels, "item")} will be kept as ${p.crown_jewels === 1 ? "a crown jewel" : "crown jewels"}: extra protection, never filled into pages, left out of exports unless you include ${p.crown_jewels === 1 ? "it" : "them"}.`,
    )
  return out
}

/** How many items the yes brings in: the new ones and those a backup changes. */
export const keeping = (p: ImportPreview): number => p.new + p.updated

/** The yes, with how many it brings in (brief T7, no article): "Bring in 240 items"; "Bring in" when there is nothing. */
export function confirmWords(p: ImportPreview): string {
  const n = keeping(p)
  return n === 0 ? "Bring in" : `Bring in ${plural(n, "item")}`
}

/** Why the yes is off (#37), shown under it; `null` while there is something to bring in. */
export function nothingReason(p: ImportPreview): string | null {
  return keeping(p) === 0 ? "Nothing to bring in" : null
}

/** The wizard's place in its steps, for the overline: `Import // Step 01 // 03` (brief T7). */
export function stepOverline(step: 1 | 2 | 3): string {
  return `Import // Step ${String(step).padStart(2, "0")} // 03`
}

/** The report, in one sentence. */
export function doneLine(r: ImportReport): string {
  const parts: string[] = []
  if (r.imported) parts.push(`${plural(r.imported, "item")} brought in`)
  if (r.updated) parts.push(`${plural(r.updated, "item")} updated from the backup`)
  if (r.unchanged) parts.push(`${plural(r.unchanged, "item")} already here`)
  return parts.length ? `${parts.join(", ")}.` : "Nothing changed."
}

const two = (n: number) => String(n).padStart(2, "0")

/** A backup's time as the overline and the folder's rows write it: `2026.11.13 09:00`, local. */
export function madeAtWords(secs: number): string {
  const d = new Date(secs * 1000)
  return `${d.getFullYear()}.${two(d.getMonth() + 1)}.${two(d.getDate())} ${two(d.getHours())}:${two(d.getMinutes())}`
}

/** A restore's overline: `Restore // 2026.11.13 09:00`, or `Restore` when the backup's time isn't known. */
export function restoreOverline(madeAt?: number | null): string {
  return madeAt ? `Restore // ${madeAtWords(madeAt)}` : "Restore"
}
