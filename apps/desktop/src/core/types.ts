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
export type SmartView = "passkeys" | "codes" | "wi-fi" | "expiring" | "attention" | "deleted" | "favourites" | "archived"

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
  // Organization (docs/plan/15-organization.md). Always sent by the core;
  // optional here so older fixtures still type-check.
  /** The vault the item is in. */
  vault_id?: string
  /** The folder it is filed in (an id of its vault's folders). */
  folder?: string | null
  /** Its tags, sorted; `/` nests one under another. */
  tags?: string[]
  favourite?: boolean
  /** When this device last used it (copy, reveal, site opened); never synced. */
  last_used?: number | null
  /** How many times this device used it. */
  uses?: number
  /** Days until its expiry date lapses; negative once it has. */
  expires_in_days?: number | null
  /** How many files are attached. */
  attachments?: number
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
  /** A web address's match rule (`exact_host`, `base_domain`, `equivalent`, `starts_with`, `never`). */
  match_rule?: string
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
  /** A crown jewel the person opted in to exports and backups (D121); the core always sends it. */
  export_opt_in?: boolean
  fields: FieldView[]
  /**
   * The bridge tier an item carries, from its signed manifest. Not in the
   * Rust `ItemDetail` yet: the vault model has no tier, so no item sends
   * one; the detail draws `RiskTierBadge` the moment one arrives.
   */
  tier?: RiskTier | null
  /** The manifest check's answer; left out while it runs. */
  tier_verified?: boolean
  /** The vault it is in. */
  vault_id?: string
  /** Its folder, tags and favourite (never among `fields`). */
  organization?: OrganizationView
  /** Its files' facts, never their bytes. */
  attachments?: AttachmentView[]
  /** A crown jewel by its kind or a field: it can never leave the class. */
  crown_jewel_fixed?: boolean
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
  /** A backup's items already here that got the backup's copy as a new version. */
  updated: number
  /** A backup's items already here exactly as the backup has them. */
  unchanged: number
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
  /*
   * The security center's three settings. The core always sends them;
   * optional here so a settings object built before they existed still
   * type-checks, and absent means the core's default.
   */
  /** The breached-password check (five hex characters of each password's hash leave the device). Default on. */
  breach_check?: boolean
  /** An `https://` range API or `file:///folder`; `null` is the public Pwned Passwords API. */
  breach_endpoint?: string | null
  /** Whether "Change password" asks the site for its change-password page first. Default on. */
  change_password_check?: boolean
  /** Automatic backups; changed only through the backup calls. The core always sends it. */
  backup?: BackupSettings
}

export interface BackupSettings {
  enabled: boolean
  folder: string | null
  interval_hours: number
  keep_last: number
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
  /** May be added more than once (`url.0`, `url.1`, …). */
  repeats?: boolean
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
  /** With a web address's `text`: how it matches pages. */
  match_rule?: string
}

// --- organization (docs/plan/15-organization.md) --------------------------------

export interface OrganizationView {
  folder: string | null
  tags: string[]
  favourite: boolean
}

export interface AttachmentView {
  id: string
  name: string
  mime: string
  size: number
}

export interface FolderView {
  id: string
  name: string
  parent: string | null
  /** Names from the top, joined by " / ". */
  path: string
}

export interface VaultView {
  id: string
  name: string
  /** The first vault: new items go here unless another is picked. */
  primary: boolean
  items: number
  trash_days: number
  folders: FolderView[]
}

export interface DuplicateGroup {
  reason: "same-login" | "same-password" | "same-code"
  words: string
  ids: string[]
  mergeable: boolean
}

export interface VersionView {
  n: number
  edited_at: number
  edited_by: string
  /** Labels of the fields this version changed; never a value. */
  changed: string[]
  readable: boolean
  current: boolean
}

export interface ExpiryReminder {
  id: string
  title: string
  kind: string
  date: string
  days: number
}

// --- the security center (crates/app/src/security.rs) ----------------------------

export type FindingKind = "breached" | "reused" | "weak" | "missing_2fa" | "lookalike" | "new_sign_in" | "exposed_email"
export type Severity = "urgent" | "important" | "suggested"

/** A finding dismissed (`until: null`) or snoozed until a time; a setting on the item, so it syncs. */
export interface Dismissal {
  at: number
  until: number | null
}

export interface RelatedItem {
  id: string
  title: string
}

/** One finding, in words: never a password, a hash or a reuse key. */
export interface Finding {
  id: string
  kind: FindingKind
  severity: Severity
  title: string
  detail: string
  /** What a dismissal is filed under on the item (`weak:pw`). */
  key: string
  item_id: string | null
  item_title: string | null
  item_kind: string | null
  field_id: string | null
  /** The item's website as typed; never a crown jewel's. */
  site: string | null
  related: RelatedItem[]
  /** The site's own page on turning on two-step sign-in. */
  setup_url: string | null
  change_password: boolean
  playbook: string
  /** An account alert's number. */
  alert: number | null
  at: number | null
  dismissed: Dismissal | null
}

export interface SecurityReport {
  findings: Finding[]
  dismissed: Finding[]
  counts: { urgent: number; important: number; suggested: number }
  passwords: number
  breach: { enabled: boolean; source: string; checked_at: number | null; checked: number; unchecked: number; message: string | null }
  alerts: { account: boolean; checked_at: number | null; message: string | null }
  directory: { source: string; licence: string; date: string; sites: number }
  generated_at: number
}

/** What to do about one kind of finding; the last step confirms the fix. */
export interface Playbook {
  id: string
  title: string
  summary: string
  steps: string[]
}

/** The page "Change password" opened. */
export interface ChangeLink {
  url: string
  kind: "well_known" | "site_root"
  checked: boolean
}

// --- import, export and backups (docs/plan/14-import-export.md) -----------------

/** One format the import wizard offers. */
export interface ImportFormatView {
  id: string
  name: string
  how: string
  may_need_password: boolean
}

export interface KindCount {
  kind: string
  name: string
  count: number
}

/** What a file holds before anything is kept: counts and words, never a value. */
export interface ImportPreview {
  token: number
  format: string
  format_name: string
  total: number
  by_kind: KindCount[]
  attachments: number
  crown_jewels: number
  new: number
  updated: number
  unchanged: number
  problems: string[]
  item_notes: string[]
}

export interface LeftOutView {
  id: string
  title: string
  kind: string
  reason: string
}

/** What an export carries and leaves, in counts and words. */
export interface ExportPreview {
  format: "lockdown" | "cxf" | "csv" | "kdbx"
  items: number
  attachments: number
  crown_jewels_included: number
  crown_jewels_left_out: number
  left_out: LeftOutView[]
  dropped: string[]
  encrypted: boolean
}

export interface ExportDone {
  path: string
  bytes: number
  preview: ExportPreview
}

/** A place the person chose in the OS's dialog, held by the shell under `token`. */
export interface Chosen {
  token: number
  path: string
  name: string
}

export interface BackupFile {
  name: string
  bytes: number
  made_at: number
}

export interface BackupStatus {
  enabled: boolean
  folder: string | null
  interval_hours: number
  keep_last: number
  key_set: boolean
  last_success_at: number | null
  last_error: string | null
  next_due_at: number | null
  crown_jewels_left_out: number
  backups: BackupFile[]
}
