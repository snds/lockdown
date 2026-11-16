import { useSyncExternalStore } from "react"

import { countName, fullDate } from "@/lib/format"

import type { EmergencyState, EmergencyView } from "./sharing-client"

/*
 * Emergency access as brief S5 shows it (doc 91): the word for each state,
 * the line under a grant's row, what each side can do, and which grant is
 * on screen, so needs-you row 4 knows when its solid action is STOP IT.
 */

/** The right-hand word of a grant's row (brief S5). */
export const STATE_WORDS: Record<EmergencyState, string> = {
  invited: "Invited",
  accepted: "Accepted",
  requested: "Asked for access",
  ready: "Ready",
  released: "Open",
  declined: "Declined",
  revoked: "Ended",
}

/** `7 days`, `1 day`. */
export const daysWords = (n: number) => `${n} ${n === 1 ? "day" : "days"}`

/** A row's mono line: `wait 7 days // 2 vaults` (a count with its noun is words: a plain number). */
export function grantLine(g: EmergencyView): string {
  // Someone who named you: the vaults are theirs to pick, and none show until they are sealed to you.
  if (g.vaults.length === 0) return `wait ${daysWords(g.wait_days)}`
  return `wait ${daysWords(g.wait_days)} // ${countName(g.vaults.length, "vault")}`
}

/** `NOV 20`, as a needs-you title writes a date ahead (brief X3 row 4). */
export function monthDay(secs: number): string {
  return new Date(secs * 1000).toLocaleDateString("en-US", { month: "short", day: "2-digit" }).toUpperCase()
}

/** `2026.11.20 14:00`, a date and its local time. */
export function dateTime(secs: number): string {
  const d = new Date(secs * 1000)
  return `${fullDate(secs)} ${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`
}

/** Someone you named asked for your vaults, and the wait runs: needs-you row 4. */
export const asksForYours = (g: EmergencyView) => g.mine && g.state === "requested"

/** The needs-you's overline for row 4: `Emergency access // opens NOV 20`. */
export function requestTitle(g: EmergencyView): string {
  return g.release_at ? `Emergency access // opens ${monthDay(g.release_at)}` : "Emergency access // asked"
}

/** Row 4's line. */
export const requestLine = (g: EmergencyView) => `${g.other} asked for your vaults.`

/** The toast once a request is stopped. */
export const stoppedWords = (g: EmergencyView) => `Stopped. ${g.other} can ask again later`

/** What a grant's detail says about where it stands, one line, or null. */
export function standingLine(g: EmergencyView): string | null {
  if (g.mine) {
    switch (g.state) {
      case "invited":
        return `Waiting for ${g.other} to agree.`
      case "accepted":
        return `${g.other} can ask for your vaults. You'd have ${daysWords(g.wait_days)} to stop it.`
      case "requested":
        return g.release_at ? `They open your vaults ${dateTime(g.release_at)} unless you stop it.` : "They asked for your vaults. Stop it if you didn't expect this."
      case "ready":
        return "The wait ran out. They can open your vaults now."
      case "released":
        return "They opened your vaults."
      case "declined":
        return `${g.other} said no or stepped down.`
      case "revoked":
        return "You ended this."
    }
  }
  switch (g.state) {
    case "invited":
      return `${g.other} named you an emergency contact.`
    case "accepted":
      return `You can ask for their vaults. ${g.other} has ${daysWords(g.wait_days)} to stop it.`
    case "requested":
      return g.release_at ? `It opens ${dateTime(g.release_at)}.` : "You asked. It opens after the wait."
    case "ready":
      return "The wait ran out. You can open their vaults now."
    case "released":
      return "You can read their vaults, in the sidebar."
    case "declined":
      return "You stepped down."
    case "revoked":
      return `${g.other} ended this.`
  }
}

/** One thing a person can do with a grant, and how loud it is (brief S5's table). */
export interface GrantAction {
  action: "accept" | "decline" | "request" | "stop" | "revoke" | "open"
  label: string
  /** primary: the detail's one solid; secondary; text; critical: a critical text button. */
  emphasis: "primary" | "secondary" | "text" | "critical"
  /** Asks a Question first. */
  asks?: boolean
}

/**
 * What each side can do by the grant's state, as the server allows it.
 * STOP IT on a request is the needs-you's solid action (row 4), so it is
 * not repeated in the detail; it acts at once, with no gate and no
 * Question (D196, D221). Ending asks first; nothing here takes the gate.
 */
export function grantActions(g: EmergencyView): GrantAction[] {
  if (g.state === "declined" || g.state === "revoked") return []
  const end: GrantAction = { action: "revoke", label: "End it", emphasis: "critical", asks: true }
  if (g.mine) {
    if (g.state === "ready") return [{ action: "stop", label: "Stop it", emphasis: "secondary" }, end]
    return [end]
  }
  const stepDown: GrantAction = { action: "decline", label: "Step down", emphasis: "text", asks: true }
  switch (g.state) {
    case "invited":
      return [
        { action: "accept", label: "Accept", emphasis: "primary" },
        { action: "decline", label: "Decline", emphasis: "text" },
      ]
    case "accepted":
      return [{ action: "request", label: "Ask for access", emphasis: "secondary", asks: true }, stepDown]
    case "requested":
      return [stepDown]
    case "ready":
      return [{ action: "open", label: "Open their vaults", emphasis: "primary" }]
    default:
      return []
  }
}

// --- which grant is open, and which one a REVIEW asked for ------------------------

let shown: string | null = null
let wanted: string | null = null
const listeners = new Set<() => void>()
const changed = () => listeners.forEach((l) => l())
const subscribe = (l: () => void) => {
  listeners.add(l)
  return () => {
    listeners.delete(l)
  }
}

/** The Emergency tab says which grant's detail is on screen now (null: none). */
export function setShownGrant(id: string | null) {
  if (shown === id) return
  shown = id
  changed()
}

/** The grant whose detail is on screen. */
export function useShownGrant(): string | null {
  return useSyncExternalStore(subscribe, () => shown, () => shown)
}

/** A REVIEW from somewhere else: the Emergency tab opens this grant when it shows. */
export function wantGrant(id: string | null) {
  if (wanted === id) return
  wanted = id
  changed()
}

/** The grant a REVIEW asked for and the tab has not opened yet. */
export function useWantedGrant(): string | null {
  return useSyncExternalStore(subscribe, () => wanted, () => wanted)
}
