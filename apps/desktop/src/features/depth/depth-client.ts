import { createContext, useContext } from "react"

import { isAppError } from "@/core/client"
import type { GateInput, OncePage, Status } from "@/core/types"

/** How far an item lets its values out (docs/plan/22-security-depth.md §1). */
export type ExposureTier = "app-only" | "ask" | "fill"

export interface ExposureView {
  id: string
  tier: ExposureTier
  /** `default`, `high-risk`, `crown-jewel` or `chosen`. */
  source: string
  default_tier: ExposureTier
  /** Why it starts above the everyday level, in words. */
  reason: string | null
  /** The levels it may be set to. */
  choices: ExposureTier[]
}

export interface PasswordReminder {
  applies: boolean
  due: boolean
  last_at: number | null
  every_days: number
  next_at: number | null
}

export interface DrillReminder {
  /** `none`, `created` or `drilled`. */
  kit: string
  last_drilled_at: number | null
  next_at: number | null
  due: boolean
  /** The kit was made but never tested: the first run's last step. */
  kit_test_pending: boolean
}

export interface RemindersView {
  password: PasswordReminder
  drill: DrillReminder
}

export interface DrillCheck {
  /** `kit`, `secret-key`, `contact`, `crown-jewels`. */
  what: string
  ok: boolean
  words: string
}

export interface PhraseCheckView {
  verify_id: string
  role: "inviter" | "joiner"
  login: string
  account_id: string
  /** The phrase to say, while the check is open (inviter only). */
  phrase: string | null
  /** `waiting`, `matched`, `wrong-phrase`, `keys-differ`, `failed`, `expired`. */
  state: string
  words: string
  expires_at: number
}

export interface PhraseIncoming {
  verify_id: string
  login: string
  account_id: string
  expires_at: number
}

export interface SecurityKeyInfo {
  id: string
  name: string
  alg: string
  created_at: number
  last_used: number | null
}

export interface SecurityKeyList {
  keys: SecurityKeyInfo[]
  rp_id: string
}

/** A link this device opened so another device can join. */
export interface LinkView {
  /** `waiting`, `compare`, `approved`, `declined`, `closed`. */
  state: string
  /** The code to type on the other device (`XXXXX-XXXXX`). */
  code: string
  /** The QR payload (`lockdown:link:v1?…`). */
  qr: string
  /** The same as QR module rows (`1` dark), while the link waits. */
  qr_rows?: string[]
  expires_at: number
  /** The six digits to compare. */
  sas?: string | null
  /** What the other device calls itself. */
  device_name?: string | null
}

/** This device joining another's link. */
export interface JoinView {
  /** `waiting`, `compare`, `approved`, `done`. */
  state: string
  sas?: string
  server: string
  /** `sent`, `opted-out`, `not-held`. */
  secret_key?: string
  words: string
}

export interface SecretKeyView {
  /** `kept`, `opted-out`, `not-kept`. */
  state: string
  words: string
}

/**
 * Security depth as the UI sees it: one method per command
 * (apps/desktop/src-tauri/src/depth.rs; the web vault's
 * crates/web/src/commands/depth.rs carries the same names). Feature code
 * reaches it through `DepthClientProvider` and never imports a host's API,
 * so the web vault compiles these views unchanged. A host without one of
 * the calls rejects it in words.
 */
export interface DepthClient {
  exposure(id: string): Promise<ExposureView>
  exposures(): Promise<ExposureView[]>
  /** `tier: null` goes back to the item's default. */
  setExposure(id: string, tier: ExposureTier | null, gate?: GateInput): Promise<ExposureView>
  legacyOptIn(id: string): Promise<boolean>
  setLegacyOptIn(id: string, on: boolean, gate?: GateInput): Promise<number>
  reminders(): Promise<RemindersView>
  passwordCheck(password: string): Promise<RemindersView>
  passwordSnooze(): Promise<RemindersView>
  setPasswordReminderDays(days: number): Promise<RemindersView>
  drillSnooze(): Promise<RemindersView>
  /** Refused with detail `confirm_skip` unless `understood`. */
  kitTestSkip(understood: boolean): Promise<RemindersView>
  drillChecks(): Promise<DrillCheck[]>
  phraseInvite(login: string): Promise<PhraseCheckView>
  phraseIncoming(): Promise<PhraseIncoming[]>
  phraseAnswer(verifyId: string, phrase: string): Promise<PhraseCheckView>
  phrasePoll(verifyId: string): Promise<PhraseCheckView>
  phraseCancel(verifyId: string): Promise<void>
  securityKeys(): Promise<SecurityKeyList>
  /** Opens the server's page for adding a key (the system browser, or this tab). */
  securityKeyAdd(): Promise<void>
  securityKeyRemove(id: string, gate?: GateInput): Promise<SecurityKeyList>
  linkOffer(): Promise<LinkView>
  linkPoll(): Promise<LinkView>
  linkApprove(gate?: GateInput): Promise<string[]>
  linkDecline(): Promise<void>
  secretKeyStatus(): Promise<SecretKeyView>
  /** A device with nothing on it joins another's link (code or QR payload). */
  joinStart(server: string, invite: string): Promise<JoinView>
  joinPoll(): Promise<JoinView>
  joinFinish(login: string, password: string): Promise<Status>
  joinCancel(): Promise<void>
  /** Opens the page where a waiting sign-in's security key is used. */
  secondFactorOpen(server: string): Promise<void>
  /** The waiting sign-in's lines once the key answered; `null` while it waits. */
  secondFactorWait(): Promise<string[] | null>
  /** Makes the account without a Secret Key; refused with detail `confirm_opt_out` unless `understood`. */
  accountRegisterOptedOut(input: { server: string; login: string; password: string }, understood: boolean): Promise<OncePage>
}

const DepthClientContext = createContext<DepthClient | null>(null)

/** The host's client; tests provide a fake one. */
export const DepthClientProvider = DepthClientContext.Provider

/** The host's client, or `null` where the host has none (the views hide). */
export function useDepthClient(): DepthClient | null {
  return useContext(DepthClientContext)
}

/** The words a tier is shown with. */
export const TIER_WORDS: Record<ExposureTier, { name: string; words: string }> = {
  fill: { name: "Fill freely", words: "Browsers and copies get it without asking." },
  ask: { name: "Ask each time", words: "Every copy, reveal and fill asks you first." },
  "app-only": { name: "Keep to the app", words: "Never offered to a browser; only shown here." },
}

/** Whether a tier asks more than another (`app-only` > `ask` > `fill`). */
export function asksMore(a: ExposureTier, b: ExposureTier): boolean {
  const rank: Record<ExposureTier, number> = { fill: 0, ask: 1, "app-only": 2 }
  return rank[a] > rank[b]
}

/** A phrase as typed, digits only, or `null` until it has nine. */
export function phraseDigits(typed: string): string | null {
  const digits = typed.replace(/[\s-]/g, "")
  return /^\d{9}$/.test(digits) ? digits : null
}

/** A ten-character link code as typed, normalised to `XXXXX-XXXXX`, or `null`. */
export function linkCode(typed: string): string | null {
  // Crockford's base32, as the core reads it: O is zero, I and L are one.
  const raw = typed.toUpperCase().replace(/[\s-]/g, "").replace(/O/g, "0").replace(/[IL]/g, "1")
  return /^[0-9A-HJKMNP-TV-Z]{10}$/.test(raw) ? `${raw.slice(0, 5)}-${raw.slice(5)}` : null
}

/** Whether a scanned or pasted text is a device-link QR payload. */
export function isLinkQr(text: string): boolean {
  return text.trim().startsWith("lockdown:link:v1?")
}

/** How often an open link or check asks the server what happened, in milliseconds. */
export const POLL_MS = 1500

/** The password reminder's intervals, in days. */
export const PASSWORD_DAYS = [7, 14, 30] as const

/** Which prompt comes first: the first run's kit test, the password, then the yearly drill. */
export function promptFor(r: RemindersView | null): "kit-test" | "password" | "drill" | null {
  if (!r) return null
  if (r.drill.kit_test_pending) return "kit-test"
  if (r.password.applies && r.password.due) return "password"
  if (r.drill.due && r.drill.kit !== "none") return "drill"
  return null
}

/** Whether an error is the core saying a sign-in waits for a security key. */
export function needsSecurityKey(e: unknown): boolean {
  return isAppError(e) && e.code === "approval_needed" && e.detail === "security_key"
}

/** Whether an error is the core saying the server emailed a link to approve this new device. */
export function needsEmailApproval(e: unknown): boolean {
  return isAppError(e) && e.code === "approval_needed" && e.detail === "email"
}
