import { createContext, useContext } from "react"

import type { OncePage, Status } from "@/core/types"

/**
 * The first run, the master password and finding a server, as the views
 * see them (docs/plan/39-first-run.md §1.2–§1.11). One method per command
 * in `src-tauri/src/first_run.rs`. The desktop provides it
 * (`app/tauri-first-run.ts`); the web vault, whose first run stays
 * server-backed (D176), provides none, so these views compile there and
 * show nothing of it.
 */
export interface FirstRunClient {
  /** Makes the local account; the kit page comes back once. */
  create(name: string, password: string): Promise<OncePage>
  unlockWithPassword(password: string): Promise<Status>
  /** The migration for an install made before the master password (§1.6). */
  setMasterPassword(input: MasterPasswordInput): Promise<OncePage>
  /** "Forgot your password?" with the recovery kit (§1.10). */
  resetWithKit(input: { secretKey: string; recoveryCode: string; password: string }): Promise<Status>
  stage(stage: OnboardingStage): Promise<void>
  setName(name: string): Promise<string>
  /** Refuses a master password under "fair", in the core's words. */
  checkMasterPassword(password: string, name?: string): Promise<void>
  quickUnlock(): Promise<QuickUnlock>
  candidates(network: boolean): Promise<Candidate[]>
  check(address: string): Promise<Candidate>
  domain(login: string): Promise<Candidate[]>
  confirm(address: string): Promise<Candidate>
  forget(address: string): Promise<void>
}

export interface MasterPasswordInput {
  name?: string
  password: string
  secretKey?: string
  recoveryCode?: string
  newKit?: boolean
}

export type OnboardingStage = "account_made" | "kit_saved" | "quick_offered" | "import_done" | "done"

/** Touch ID or Windows Hello unlock, as this build offers it (Q1). */
export interface QuickUnlock {
  kind: "touch_id" | "windows_hello" | "none"
  available: boolean
  why: string | null
}

export type Found =
  | { kind: "policy" }
  | { kind: "settings" }
  | { kind: "this_computer" }
  | { kind: "network" }
  | { kind: "domain"; domain: string }
  | { kind: "typed" }

export type CandidateState = "ready" | "unreachable" | "needs_https" | "not_lockdown" | "vaultwarden" | "wrong_key"

/** A server discovery found (`lockdown_app::discovery::Candidate`). */
export interface Candidate {
  address: string
  sources: Found[]
  state: CandidateState
  reason: string | null
  fingerprint: string | null
  version: string | null
  policy_key: boolean
  locked: boolean
  in_use: boolean
}

/** Where a candidate was found, in words. */
export function foundWords(found: Found): string {
  switch (found.kind) {
    case "policy":
      return "Set by your organization"
    case "settings":
      return "From your settings"
    case "this_computer":
      return "On this computer"
    case "network":
      return "On your network"
    case "domain":
      return `From ${found.domain}`
    case "typed":
      return "Typed by you"
  }
}

/** A candidate's state, in words. */
export function stateWords(state: CandidateState): string {
  switch (state) {
    case "ready":
      return "Ready"
    case "unreachable":
      return "Can't reach it"
    case "needs_https":
      return "Needs https"
    case "not_lockdown":
      return "Not a Lockdown server"
    case "vaultwarden":
      return "This looks like Vaultwarden or Bitwarden"
    case "wrong_key":
      return "Its key changed"
  }
}

/** The address without its scheme, for a row's title. */
export function hostOf(address: string): string {
  return address.replace(/^https?:\/\//i, "")
}

/** Adds a checked candidate to a list, replacing one at the same address. */
export function withCandidate(list: Candidate[], c: Candidate): Candidate[] {
  return [...list.filter((x) => x.address.toLowerCase() !== c.address.toLowerCase()), c]
}

const FirstRunContext = createContext<FirstRunClient | null>(null)

export const FirstRunClientProvider = FirstRunContext.Provider

/** The host's first-run client, or `null` where there is none (the web vault). */
export function useFirstRunClient(): FirstRunClient | null {
  return useContext(FirstRunContext)
}
