import type { ExportPreview } from "@/core/types"

/** One way out, as the export dialog offers it. */
export interface ExportChoice {
  id: string
  /** The core's format. */
  format: "lockdown" | "cxf" | "csv" | "kdbx"
  /** Whether the file is locked with a password. */
  sealed: boolean
  label: string
  detail: string
}

export const EXPORT_CHOICES: ExportChoice[] = [
  {
    id: "lockdown-sealed",
    format: "lockdown",
    sealed: true,
    label: "A Lockdown backup, locked with a password (recommended)",
    detail: "Everything: every kind of item, attached files and earlier versions. Opens in Lockdown on any computer with the password alone.",
  },
  {
    id: "kdbx",
    format: "kdbx",
    sealed: true,
    label: "A KeePass database, locked with a password",
    detail: "For KeePass, KeePassXC and the other apps that open .kdbx files. Your way out, whenever you want it.",
  },
  {
    id: "cxf",
    format: "cxf",
    sealed: false,
    label: "A CXF file for another password manager (not protected)",
    detail: "The industry's exchange format. Holds every kind of item, but not attached files or earlier versions.",
  },
  {
    id: "csv",
    format: "csv",
    sealed: false,
    label: "A spreadsheet of your logins (not protected)",
    detail: "Logins and their codes only, for apps that take nothing else.",
  },
  {
    id: "lockdown-plain",
    format: "lockdown",
    sealed: false,
    label: "A readable Lockdown file (not protected)",
    detail: "The same as the backup, as plain text you can read. Its format is published.",
  },
]

/** The shortest password the dialog takes for a locked file. */
export const MIN_PASSWORD = 8

/** Checks the two password boxes; `null` when they're fine. */
export function passwordProblem(first: string, second: string): string | null {
  if (first.length < MIN_PASSWORD) return `Use at least ${MIN_PASSWORD} characters.`
  if (first !== second) return "The two passwords don't match."
  return null
}

const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`

/** What the file will carry, in one line. */
export function carriesLine(p: ExportPreview): string {
  const parts = [plural(p.items, "item")]
  if (p.attachments) parts.push(plural(p.attachments, "attached file"))
  if (p.crown_jewels_included) parts.push(`${plural(p.crown_jewels_included, "crown jewel")} you chose to include`)
  return `This file will hold ${parts.join(", ")}.`
}

/** The crown jewels left out (D121), in one line, or `null`. */
export function leftOutLine(p: ExportPreview): string | null {
  if (!p.crown_jewels_left_out) return null
  const names = p.left_out.map((l) => l.title).join(", ")
  return `${plural(p.crown_jewels_left_out, "crown jewel")} stay${p.crown_jewels_left_out === 1 ? "s" : ""} out: ${names}. To include one, open it and turn on "Include in exports and backups".`
}
