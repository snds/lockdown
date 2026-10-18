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

/** What the person is asked to confirm: the button's words. */
export function confirmWords(p: ImportPreview): string {
  const keeping = p.new + p.updated
  if (keeping === 0) return "Nothing to bring in"
  return `Bring in ${plural(keeping, "item")}`
}

/** The report, in one sentence. */
export function doneLine(r: ImportReport): string {
  const parts: string[] = []
  if (r.imported) parts.push(`${plural(r.imported, "item")} brought in`)
  if (r.updated) parts.push(`${plural(r.updated, "item")} updated from the backup`)
  if (r.unchanged) parts.push(`${plural(r.unchanged, "item")} already here`)
  return parts.length ? `${parts.join(", ")}.` : "Nothing changed."
}
