// What crosses from lockdown-app (crates/app/src/types.rs), field for field.

export type ErrorCode = "locked" | "not_set_up" | "refused" | "stopped"

export interface AppError {
  code: ErrorCode
  message: string
}

export interface Probe {
  root: string
  data: string
  identity_present: boolean
  device_key_present: boolean
  account_present: boolean
  device_name: string
}

export type LockReason = "requested" | "idle" | "hidden" | "sleep"

export interface LockState {
  locked: boolean
  reason: LockReason | null
  idle_secs: number
  idle_timeout_secs: number | null
  hidden_timeout_secs: number | null
}

export interface PolicyInput {
  idle_secs: number | null
  hidden_secs: number | null
  sleep_gap_secs: number | null
}

export interface ItemSummary {
  id: string
  kind: string
  state: string
  title: string
  site: string
  username: string
  edited_at: number
}

export interface FieldView {
  field_id: string
  label: string
  kind: string
  hidden: boolean
  value: string | null
  mask: string | null
}

export interface ItemDetail {
  id: string
  kind: string
  state: string
  version: number
  edited_at: number
  crown_jewel: boolean
  fields: FieldView[]
}

export interface AccountStatus {
  server: string
  login: string
  synced_seq: number
  synced_rev: number
  last_sync_at: number | null
  session_expires_at: number
}

export interface Status {
  root: string
  data: string
  profile_id: string
  manager: string | null
  recovery: string
  device_id: string
  device_name: string
  persona_id: string
  persona_label: string
  vault_id: string
  head_seq: number
  head_hash: string
  live_items: number
  devices: number
  devices_revoked: number
  identity_seq: number
  account: AccountStatus | null
}

export interface DeviceInfo {
  id: string
  name: string
  platform: string
  enrolled_at: number
  revoked_at: number | null
  this_device: boolean
}

export interface SyncOutcome {
  pushed: number
  pulled: number
  merged: number
  conflict_copies: string[]
  head_seq: number
  rounds: number
  lines: string[]
}

/** A page (the recovery kit) that is shown once: notes, and the page's bytes. */
export interface OncePage {
  lines: string[]
  /** The kit page, as bytes the caller wipes after showing. Empty when none was made. */
  page: Uint8Array
}
