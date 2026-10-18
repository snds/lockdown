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

/** One icon per kind of finding (decorative; the words carry the meaning). */
export const KIND_ICONS: Record<Finding["kind"], IconName> = {
  breached: "gpp_bad",
  reused: "content_copy",
  weak: "password",
  missing_2fa: "lock_person",
  lookalike: "report",
  new_sign_in: "devices",
  exposed_email: "mail",
}

/** Open findings by severity, in the core's order. */
export function groupBySeverity(findings: Finding[]): Record<Severity, Finding[]> {
  const out: Record<Severity, Finding[]> = { urgent: [], important: [], suggested: [] }
  for (const f of findings) out[f.severity].push(f)
  return out
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
