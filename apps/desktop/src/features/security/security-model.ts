import type { IconName } from "@unicron/ui"

import type { Finding, SecurityReport, Severity } from "@/core/types"

/** The three groups, most pressing first, in words a person acts on. */
export const SEVERITIES: Array<{ severity: Severity; heading: string; hint: string }> = [
  { severity: "urgent", heading: "Fix now", hint: "Someone could already be using these." },
  { severity: "important", heading: "Fix soon", hint: "These make an account easier to break into." },
  { severity: "suggested", heading: "Worth doing", hint: "Extra protection for when something goes wrong." },
]

/** The text role each severity's words take (app-extension.css roles; colour never carries the meaning alone). */
export const SEVERITY_TEXT: Record<Severity, string> = {
  urgent: "text-destructive",
  important: "text-attention-text",
  suggested: "",
}

/** A finding's kind as the row's right-hand word (brief H1): mono caps, set by the row. */
export const KIND_WORDS: Record<Finding["kind"], string> = {
  weak: "Weak",
  reused: "Reused",
  breached: "Breached",
  missing_2fa: "No second step",
  lookalike: "Look-alike",
  new_sign_in: "New sign-in",
  exposed_email: "Exposed email",
}

/** A finding's kind in words, for its detail's overline (`HEALTH // WEAK PASSWORD`, brief H2). */
export const KIND_NAMES: Record<Finding["kind"], string> = {
  weak: "Weak password",
  reused: "Reused password",
  breached: "Breached password",
  missing_2fa: "No second step",
  lookalike: "Look-alike site",
  new_sign_in: "New sign-in",
  exposed_email: "Exposed email",
}

/** One icon per kind of finding (decorative; the words carry the meaning). */
export const KIND_ICONS: Record<Finding["kind"], IconName> = {
  breached: "rule--cancelled",
  reused: "copy",
  weak: "password",
  missing_2fa: "user--access-locked",
  lookalike: "warning--hex",
  new_sign_in: "devices",
  exposed_email: "email",
}

/** Open findings by severity, in the core's order. */
export function groupBySeverity(findings: Finding[]): Record<Severity, Finding[]> {
  const out: Record<Severity, Finding[]> = { urgent: [], important: [], suggested: [] }
  for (const f of findings) out[f.severity].push(f)
  return out
}

/**
 * How many things there are to fix (redesign brief H1): the findings to fix
 * now and soon, and the rows in the Needs attention list. From the cached
 * report and the loaded rows only; `null` while no report has been read
 * counts the list alone. The nav marks and the Health tab read this.
 */
export function toFixCount(report: SecurityReport | null, rows: ReadonlyArray<{ views: readonly string[] }>): number {
  const findings = report ? report.counts.urgent + report.counts.important : 0
  return findings + rows.filter((r) => r.views.includes("attention")).length
}

/**
 * The Metrics under the hero (brief H1, Q8): the kinds the core reports,
 * counted from the open findings. No "Old": the core keeps no password age.
 * `exposed` is `null` while the breach check is off (it reads "—").
 */
export function kindCounts(report: SecurityReport): { weak: number; reused: number; exposed: number | null; noSecondStep: number } {
  const n = (k: Finding["kind"]) => report.findings.filter((f) => f.kind === k).length
  return { weak: n("weak"), reused: n("reused"), exposed: report.breach.enabled ? n("breached") : null, noSecondStep: n("missing_2fa") }
}

/** Where a finding stands in the fix lists (`FIX 1 OF 3` on the status line): its place among the findings to fix, in list order. */
export function placeOf(findings: Finding[], id: string): { at: number; of: number } | null {
  const ordered = SEVERITIES.flatMap(({ severity }) => findings.filter((f) => f.severity === severity))
  const at = ordered.findIndex((f) => f.id === id)
  return at < 0 ? null : { at: at + 1, of: ordered.length }
}

/** The finding after this one in list order, to open once it is skipped; `null` at the end. */
export function nextAfter(findings: Finding[], id: string): Finding | null {
  const ordered = SEVERITIES.flatMap(({ severity }) => findings.filter((f) => f.severity === severity))
  const at = ordered.findIndex((f) => f.id === id)
  return at < 0 ? null : (ordered[at + 1] ?? ordered[at - 1] ?? null)
}

/** A site as a row's mono line shows it: the host, without the scheme and the path. */
export function hostOf(site: string | null | undefined): string {
  if (!site) return ""
  try {
    return new URL(/^[a-z][a-z0-9+.-]*:\/\//i.test(site) ? site : `https://${site}`).host
  } catch {
    return site
  }
}

/** Why an item is in Needs attention, as the row's right-hand word (brief H1). */
export function attentionWord(row: { conflict_copy: boolean; warnings: readonly string[]; views: readonly string[] }): string {
  if (row.conflict_copy) return "Conflict"
  if (row.warnings.includes("passkey-hidden-by-site")) return "Passkey hidden"
  if (row.warnings.includes("signed-in-elsewhere")) return "Sign out due"
  if (row.views.includes("expiring")) return "Expires soon"
  return "Needs attention"
}

/** The one-line summary at the top of the screen. */
export function summaryWords(report: SecurityReport): string {
  const { urgent, important, suggested } = report.counts
  if (urgent + important + suggested === 0) return "Nothing needs your attention right now."
  const parts: string[] = []
  if (urgent) parts.push(`${urgent} to fix now`)
  if (important) parts.push(`${important} to fix soon`)
  if (suggested) parts.push(`${suggested} worth doing`)
  const last = parts.pop()
  return `${parts.length ? `${parts.join(", ")} and ${last}` : last}.`
}

/** The snooze choices: a week, a month. */
export const SNOOZES: Array<{ label: string; days: number }> = [
  { label: "Remind me in a week", days: 7 },
  { label: "Remind me in a month", days: 30 },
]

/** When a snooze of `days` from `nowSecs` ends. */
export function snoozeUntil(days: number, nowSecs: number): number {
  return nowSecs + days * 86_400
}

/** A dismissal in words, for the dismissed list. */
export function dismissedWords(f: Finding, when: (secs: number) => string): string {
  if (!f.dismissed) return ""
  return f.dismissed.until ? `Snoozed until ${when(f.dismissed.until)}` : "Dismissed"
}

/** The open findings about one item, most pressing first (the item's own panel). */
export function findingsForItem(report: SecurityReport, itemId: string): Finding[] {
  return report.findings.filter((f) => f.item_id === itemId)
}

/** Whether "Open item" applies: findings about an item, not about the account. */
export function opensItem(f: Finding): f is Finding & { item_id: string } {
  return f.item_id !== null
}
