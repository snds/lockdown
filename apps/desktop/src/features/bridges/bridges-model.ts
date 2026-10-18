import type { BackupStatus } from "@/core/types"

import type { FieldRole } from "./bridges-client"

/** The host of a website as typed, for an alias's note. */
export function siteHost(site: string): string {
  const t = site.trim()
  if (!t) return ""
  try {
    return new URL(t.includes("://") ? t : `https://${t}`).hostname
  } catch {
    return ""
  }
}

/** Backups late by more than this many intervals are called overdue. */
const LATE_INTERVALS = 2

/** "Backed up 3 hours ago", from seconds. */
export function agoWords(secs: number): string {
  if (secs < 90) return "just now"
  if (secs < 5400) return `${Math.round(secs / 60)} minutes ago`
  if (secs < 36 * 3600) return `${Math.round(secs / 3600)} hours ago`
  return `${Math.round(secs / 86400)} days ago`
}

/** The last-backup indicator's words and whether it needs attention. */
export function backupLine(status: BackupStatus, now: number): { text: string; late: boolean } {
  if (!status.enabled) return { text: "Backups are off", late: true }
  if (status.last_error) return { text: "The last backup failed", late: true }
  if (!status.last_success_at) return { text: "No backup yet", late: true }
  const age = now - status.last_success_at
  return { text: `Backed up ${agoWords(age)}`, late: age > status.interval_hours * 3600 * LATE_INTERVALS }
}

/** What each field role is called. */
export const ROLE_WORDS: Record<FieldRole, string> = {
  username: "the user name",
  password: "the password",
  new_password: "a new password",
  code: "the one-time code",
  ignore: "not for filling",
}
