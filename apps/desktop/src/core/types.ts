// What crosses from lockdown-app (crates/app/src/types.rs, settings.rs,
// approval.rs), field for field.

export type ErrorCode = "locked" | "not_set_up" | "refused" | "stopped" | "approval_needed"

export interface AppError {
  code: ErrorCode
  message: string
  /** With `approval_needed`: what to ask for, `password` or `window`. */
  detail?: string
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

/** The smart views (crates/app/src/views.rs), as `ItemSummary.views` names them. */
export type SmartView = "passkeys" | "codes" | "wi-fi" | "expiring" | "attention" | "deleted"

export interface ItemSummary {
  id: string
  kind: string
  state: string
  title: string
  site: string
  username: string
  edited_at: number
  crown_jewel: boolean
  /** A Login's sign-in methods: `password`, `passkey`, `code`. */
  methods: string[]
  passkeys: number
  codes: number
  expires_at: string | null
  conflict_copy: boolean
  warnings: string[]
  views: SmartView[]
}

export interface FieldView {
  field_id: string
  label: string
  kind: string
  hidden: boolean
  value: string | null
  mask: string | null
}

/** The bridge tiers' internal ids (D96); people see the badge's words, never these. */
export type RiskTier = "standard" | "official" | "tolerated"

export interface ItemDetail {
  id: string
  kind: string
  state: string
  version: number
  edited_at: number
  crown_jewel: boolean
  fields: FieldView[]
  /**
   * The bridge tier an item carries, from its signed manifest. Not in the
   * Rust `ItemDetail` yet: the vault model has no tier, so no item sends
   * one; the detail draws `RiskTierBadge` the moment one arrives.
   */
  tier?: RiskTier | null
  /** The manifest check's answer; left out while it runs. */
  tier_verified?: boolean
}

/** One one-time code now: values and timing, never the set-up. */
export interface CodeView {
  item_id: string
  field_id: string
  title: string
  issuer: string | null
  account: string | null
  variant: "totp" | "hotp" | "steam"
  digits: number
  period: number
  current: string
  next: string | null
  seconds_left: number | null
  warnings: string[]
}

export interface PersonaInfo {
  id: string
  label: string
  kind: string
  active: boolean
}

export interface ImportReport {
  imported: number
  notes: string[]
}

/** The server's key-transparency log as this device last verified it. */
export interface TransparencyView {
  log_size: number
  root_prefix: string
  audited: number
  checked_at: number | null
}

export interface AccountStatus {
  server: string
  login: string
  synced_seq: number
  synced_rev: number
  last_sync_at: number | null
  session_expires_at: number
  transparency: TransparencyView | null
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
  transparency: TransparencyView
  lines: string[]
}

/** A page (the recovery kit) that is shown once: notes, and the page's bytes. */
export interface OncePage {
  lines: string[]
  /** The kit page, as bytes the caller wipes after showing. Empty when none was made. */
  page: Uint8Array
}

// --- settings (settings.cbor, D100) --------------------------------------------

export type Density = "comfortable" | "compact"
export type RevealGesture = "hold" | "toggle"

export interface Settings {
  v: number
  density: Density
  reveal: RevealGesture
  clipboard_clear_secs: number
  idle_lock_secs: number | null
  hidden_lock_secs: number | null
  server: string | null
}

// --- approvals (D101) ------------------------------------------------------------

export type ApprovalKind = "use" | "reveal" | "approve"

/** What a gated call carries: the password the UI asked for, or the window's confirm. */
export interface GateInput {
  password?: string
  confirmed?: boolean
}

export interface ApprovalRoute {
  route: "platform" | "password" | "window"
  /** `Touch ID`, `Windows Hello`, `password` or `window`. */
  method: string
}
