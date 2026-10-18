import { createContext, useContext } from "react"

import type { ExportDone, GateInput } from "@/core/types"

// --- clock (docs/plan/23-bridges.md §8) -----------------------------------------

/** This computer's clock against the account's server (crates/app/src/bridges/clock.rs). */
export interface ClockView {
  checked_at: number
  /** Seconds to add to this clock to reach the server's. */
  offset_secs: number
  verdict: "in_step" | "corrected" | "too_far"
  /** Codes are being worked out from the server's time now. */
  correcting: boolean
  words: string
}

// --- the federated sign-in map (§4) ---------------------------------------------

export interface MapLogin {
  item_id: string
  title: string
  username: string
  host: string
  has_password: boolean
  second_step: boolean
  crown_jewel: boolean
}

export interface ProviderView {
  /** `google`, or `other:<name>` for one Lockdown doesn't list. */
  id: string
  name: string
  account: MapLogin | null
  candidates: MapLogin[]
  /** The sites entered through it: what falls with it. */
  sites: MapLogin[]
  warnings: string[]
}

export interface PendingView {
  id: number
  provider: string
  provider_name: string
  host: string
  logins: MapLogin[]
  confirmed: boolean
}

export interface FederationMap {
  providers: ProviderView[]
  pending: PendingView[]
}

// --- fill fixes (§5) --------------------------------------------------------------

export type FieldRole = "username" | "password" | "new_password" | "code" | "ignore"

export interface FieldFix {
  match: { id?: string | null; name?: string | null; autocomplete?: string | null }
  role: FieldRole
}

export interface FillFix {
  never: boolean
  fields: FieldFix[]
}

// --- aliases and monitoring (§5, §9) --------------------------------------------------

export interface AliasServiceInfo {
  id: string
  name: string
  key_hint: string
  needs_domain: boolean
  takes_domain: boolean
  self_hostable: boolean
}

export interface AliasConfigured {
  item_id: string
  title: string
  service: string
  service_name: string
  domain: string | null
  base: string | null
  made: number
}

export interface AliasOverview {
  services: AliasServiceInfo[]
  configured: AliasConfigured[]
}

export interface AliasMade {
  address: string
  service: string
  service_name: string
  monitoring_problem: string | null
}

export interface MonitoringView {
  enabled: boolean
  addresses: string[]
  has_account: boolean
}

// --- incidents (§6) -------------------------------------------------------------------

export type IncidentScope = "none" | "item" | "email" | "provider"
export type StepAction = "read" | "change_passwords" | "two_step" | "reenrol_codes" | "sessions" | "devices" | "recovery_kit" | "check"

export interface IncidentSummary {
  id: string
  title: string
  when: string
  scope: IncidentScope
  started_at: number | null
  steps_done: number
  steps: number
}

export interface StepItem {
  item_id: string
  title: string
  host: string
  done: boolean
}

export interface StepView {
  id: string
  title: string
  detail: string
  action: StepAction
  done: boolean
  items: StepItem[]
}

export interface IncidentView {
  id: string
  title: string
  when: string
  scope: IncidentScope
  picked: string | null
  choices: { value: string; label: string }[]
  started_at: number | null
  steps: StepView[]
  finished: boolean
}

// --- set-up cards (§3) ----------------------------------------------------------------

export interface Attribution {
  name: string
  licence: string
  url: string
}

export interface SetupGuide {
  site: string
  methods: string[]
  app_codes: boolean
  passkeys: string | null
  setup_url: string | null
  passkey_url: string | null
  recovery_url: string | null
  /** The site's own notes, as 2fa.directory has them. */
  notes: string | null
  site_apps: string[]
  steps: string[]
  attribution: Attribution[]
}

export interface GuideSources {
  sources: Attribution[]
  version: number
  published: string
}

// --- the emergency sheet (§10) ---------------------------------------------------------

export interface EmergencySheet {
  made_at: number
  title: string
  intro: string
  sections: { heading: string; lines: string[] }[]
  blanks: string[]
}

/**
 * The bridges commands (apps/desktop/src-tauri/src/bridges.rs), apart from
 * the main client so this slice stays in its own folder. Nothing here
 * carries a key back: an alias service's key goes in once, as bytes.
 */
export interface BridgesClient {
  clockStatus(): Promise<ClockView | null>
  clockCheck(): Promise<ClockView>
  federationMap(): Promise<FederationMap>
  /** The person's yes to what the extension saw, on one of the site's logins or (`null`) a new one. */
  federationAccept(pending: number, itemId: string | null): Promise<string>
  federationDismiss(pending: number): Promise<void>
  federationSetAccount(provider: string, itemId: string): Promise<void>
  siteFix(itemId: string): Promise<FillFix>
  siteFixSet(itemId: string, fix: FillFix): Promise<FillFix>
  aliasOverview(): Promise<AliasOverview>
  /** The key as bytes the caller zeroes; it becomes a crown jewel. */
  aliasServiceAdd(input: { service: string; key: number[]; base?: string | null; domain?: string | null }): Promise<string>
  /** Makes an alias for `site` through the `Use` gate. */
  aliasGenerate(itemId: string, site: string, gate?: GateInput): Promise<AliasMade>
  aliasMonitoring(): Promise<MonitoringView>
  aliasMonitoringSet(on: boolean): Promise<MonitoringView>
  incidents(): Promise<IncidentSummary[]>
  incident(id: string, pick?: string | null): Promise<IncidentView>
  incidentMark(id: string, step: string, itemId: string | null, done: boolean): Promise<IncidentView>
  incidentReset(id: string): Promise<IncidentView>
  setupGuide(itemId: string): Promise<SetupGuide | null>
  guideSources(): Promise<GuideSources>
  emergencySheet(): Promise<EmergencySheet>
  /** The export locked to age keys, to a place from `exportChoose("age", true)`, after the `Reveal` gate. */
  exportSaveAge(input: { target: number; recipients: string[]; vault?: string | null }, gate?: GateInput): Promise<ExportDone>
}

/**
 * No bridges client by default: a host that has one provides it (the
 * desktop in `app/tauri-bridges.ts`). Without one these views hide, so
 * the web vault compiles them unchanged (D183).
 */
const BridgesContext = createContext<BridgesClient | null>(null)

/** The host's bridges client; tests provide a fake one. */
export const BridgesProvider = BridgesContext.Provider

export function useBridges(): BridgesClient | null {
  return useContext(BridgesContext)
}
