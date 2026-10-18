// The shapes the extension and the desktop app exchange through the native
// host (docs/plan/16-browser-extension.md §4). The Rust side is
// apps/desktop/src-tauri/src/browser/handler.rs and crates/app/src/browser.rs.
// Every request is {id, op, ...}; every answer is {id, ok, result} or
// {id, ok: false, error: {code, message}}. The host also sends events of
// its own: {event: "ready" | "refused" | "no_app" | "untrusted_app" |
// "closed" | "unsupported", ...}.

export type Decision = "exact" | "base_domain" | "equivalent" | "lookalike" | "no_match"

export type FillWhat = "login" | "code" | "card" | "identity"

export interface LoginCandidate {
  item_id: string
  title: string
  username: string
  saved: string | null
  rule: string | null
  decision: Decision
  crown_jewel: boolean
  password: boolean
  code_field: string | null
  edited_at: number
}

export interface PageMatches {
  host: string
  logins: LoginCandidate[]
  lookalike_of: string[]
  secure: boolean
}

export interface WalletEntry {
  item_id: string
  kind: "card" | "identity"
  title: string
  subtitle: string
  crown_jewel: boolean
}

export interface FillValue {
  role: string
  value: string
}

export type FillAnswer =
  | { verdict: "fill"; values: FillValue[] }
  | { verdict: "confirm"; reason: string }
  | { verdict: "refuse"; reason: string }
  | { verdict: "none"; reason: string }

export interface SearchItem {
  item_id: string
  kind: string
  title: string
  username: string
  host: string
  subtitle: string
  crown_jewel: boolean
  copy: { label: string; how: string } | null
}

export interface SaveOffer {
  offer_id?: string
  kind: "new" | "update" | "same"
  title: string
  username?: string
}

export interface DesktopError {
  code: string
  message: string
}

/** What the background knows about its link to the app, for the popup. */
export interface LinkState {
  /** connecting, ready, no_app, refused, untrusted_app, closed, unsupported, error */
  link: string
  message: string
  paired: boolean
  locked: boolean
  /** A pairing in progress: the code the desktop gave, to compare. */
  pairing: { code: string } | null
  /** The app offers Lockdown as the passkey provider in pages. */
  passkeys: boolean
}

/** The menu's view of one field, built by the background for the frame. */
export interface MenuData {
  kind: FillWhat
  link: LinkState
  host: string
  logins: LoginCandidate[]
  wallet: WalletEntry[]
  lookalike_of: string[]
  /** Why there is nothing to offer, in plain words. */
  note: string | null
  /** Passkeys for a page that asked for one with conditional mediation,
   *  listed with its logins. */
  passkeys: PasskeyOption[]
}

export interface PickAnswer {
  done?: boolean
  confirm?: string
  refuse?: string
}

/** One passkey a page may be offered (no secret): from `passkey_options`. */
export interface PasskeyOption {
  item_id: string
  credential_id: string
  title: string
  user_name: string
  user_display_name: string
}

/** Where a new passkey would go: from `passkey_plan`. */
export interface PasskeyPlan {
  rp_id: string
  excluded: boolean
  item_id: string | null
  title: string
  user_name: string
}

/** What the passkey sheet shows. */
export interface SheetData {
  kind: "get" | "create" | "locked"
  host: string
  rp_id: string
  rp_name: string
  options: PasskeyOption[]
  plan: PasskeyPlan | null
}
