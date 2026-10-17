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
  /** The template's plain field under the title, or the username; empty for a crown jewel (title only). */
  subtitle: string
  /** What the row's quick copy takes (palette, copy button), when the item holds it. */
  copy: CopyTarget | null
}

/** One value a row offers to copy, by id and field: never the value. */
export interface CopyTarget {
  field_id: string
  how: "secret" | "code"
  /** In plain words: `password`, `code`, `Wi-Fi password`, `card number`. */
  label: string
}

/**
 * A copy the shell made on the Rust side (`copy_secret`, `copy_code`):
 * the seconds until its clear, and its generation, the name the webview
 * holds for that copy when it asks `clipboard_holds`. Nothing in it is
 * derived from the value.
 */
export interface Copied {
  clears_in_secs: number
  copy: number
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
  /** The counter `current` was made from: HOTP only, `null` for the time-based kinds. */
  counter: number | null
  warnings: string[]
}

/** "Next code" on an HOTP code (`advance_code`): the new code, its counter, the item's new version. Never the set-up. */
export interface CodeAdvanced {
  code: string
  counter: number
  version: number
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

// --- templates (lockdown_vault::template, D110) ------------------------------------

export type Control =
  | "text"
  | "multiline"
  | "secret"
  | "secret-multiline"
  | "url"
  | "email"
  | "phone"
  | "number"
  | "date"
  | "month"
  | "select"
  | "boolean"
  | "totp"
  | "passkey"

export interface GeneratorView {
  length: number
  symbols: boolean
  unambiguous: boolean
  /** "20 characters: letters, digits and symbols". */
  words: string
}

export interface FieldTemplateView {
  field_id: string
  label: string
  control: Control
  /** Sent as bytes; comes back only through `reveal`. */
  secret: boolean
  required: boolean
  crown_jewel: boolean
  fill: "fill" | "confirm" | "never"
  generator: GeneratorView | null
  options: string[]
  section: string
  grouped: boolean
  hint: string
}

export interface TemplateView {
  kind: string
  /** Singular, plain words: "Wi-Fi network". */
  name: string
  summary: string
  creatable: boolean
  /** Always a crown jewel; the person cannot opt down. */
  crown_jewel: boolean
  autofill: boolean
  primary: string | null
  subtitle: string | null
  fields: FieldTemplateView[]
}

export interface CustomKindView {
  /** A `FieldView.kind`: `text`, `hidden`, `url`, … */
  kind: string
  label: string
}

export interface Templates {
  kinds: TemplateView[]
  custom_kinds: CustomKindView[]
}

/**
 * One field of an add or an edit (src-tauri/src/items.rs). An edit names
 * every field the item has: `keep` for a secret the person did not
 * retype, so it never crosses back. A secret is a byte array, never a string.
 */
export interface FieldChangeInput {
  /** Omitted for a field the person adds by hand. */
  field_id?: string
  label?: string
  /** For a field added by hand: a `CustomKindView.kind`. */
  kind?: string
  action: "keep" | "set" | "generate" | "remove"
  text?: string
  secret?: number[]
  flag?: boolean
}
