// The fake core's state: the seed made into the shapes lockdown-app sends
// (crates/app/src/types.rs), derived the way the core derives them
// (crates/app/src/session.rs `list`, views.rs `classify`), and changed by
// the commands a reviewer clicks through. One world per page load.

import type {
  CodeView,
  FieldView,
  ItemDetail,
  ItemSummary,
  LockState,
  Settings,
  SmartView,
  Templates,
  TemplateView,
  VaultView,
} from "@/core/types"
import templates from "virtual:lockdown-templates"

import { uid } from "./ids"
import { ITEMS, PERSON, VAULTS, vaultId, type SeedItem } from "./seed"
import { NOW, ago, daysUntil } from "./time"

export const TEMPLATES = templates as unknown as Templates

/** One item as the fake core holds it. */
export interface WItem extends SeedItem {
  id: string
  version: number
}

const MASK = "••••••••••••"

/** The template an item is drawn from (D231). */
export function templateOf(item: SeedItem): TemplateView | null {
  const id = item.template ?? item.kind
  return TEMPLATES.kinds.find((t) => (t.id ?? t.kind) === id) ?? TEMPLATES.kinds.find((t) => t.kind === item.kind && !t.variant) ?? null
}

/** The storage kind a template control writes (`Control::storage`). */
function storage(control: string): string {
  switch (control) {
    case "secret":
    case "secret-multiline":
      return "hidden"
    case "url":
    case "email":
    case "phone":
    case "boolean":
    case "totp":
    case "passkey":
    case "linked":
      return control
    case "date":
    case "month":
      return "date"
    default:
      return "text"
  }
}

/** Every field the item has, in the core's `FieldView` shape, the title first. */
export function fieldsOf(item: WItem): FieldView[] {
  const t = templateOf(item)
  const out: FieldView[] = [{ field_id: "title", label: "Title", kind: "text", hidden: false, value: item.title, mask: null }]
  for (const [field_id, value] of Object.entries(item.fields)) {
    const ft = t?.fields.find((f) => f.field_id === field_id) ?? t?.fields.find((f) => f.repeats && field_id.startsWith(f.field_id.replace(/\.0$/, ".")))
    const kind = storage(ft?.control ?? "text")
    const hidden = kind === "hidden" || kind === "totp" || kind === "passkey"
    out.push({
      field_id,
      label: ft?.label ?? field_id,
      kind,
      hidden,
      value: hidden ? null : value,
      mask: hidden ? MASK : null,
      ...(kind === "url" ? { match_rule: item.rules?.[field_id] ?? "base_domain" } : {}),
    })
  }
  for (const c of item.custom ?? []) {
    const hidden = c.kind === "hidden"
    out.push({ field_id: c.field_id, label: c.label, kind: c.kind, hidden, value: hidden ? null : c.value, mask: hidden ? MASK : null })
  }
  // A Login's passkeys are fields of it (`passkey.N.*`), as the core keeps them.
  if (item.kind === "login") {
    item.passkeys?.forEach((p, i) => {
      out.push(
        { field_id: `passkey.${i}.rp_id`, label: "Website", kind: "text", hidden: false, value: p.rp_id, mask: null },
        { field_id: `passkey.${i}.user_name`, label: "User name", kind: "text", hidden: false, value: p.user_name, mask: null },
        { field_id: `passkey.${i}.key`, label: "Private key", kind: "passkey", hidden: true, value: null, mask: MASK },
      )
    })
  }
  return out
}

/** The plain value of any field, secret or not (what `reveal` hands back). */
export function valueOf(item: WItem, fieldId: string): string | null {
  if (fieldId === "title") return item.title
  return item.fields[fieldId] ?? item.custom?.find((c) => c.field_id === fieldId)?.value ?? null
}

const CONFLICT = " (conflict copy from "
const isConflict = (title: string) => title.includes(CONFLICT)

/** Lead days before an expiry counts (`views::expiry_lead_days`). */
const leadDays = (kind: string) => (kind === "document" || kind === "identity" ? 90 : kind === "card" ? 60 : kind === "api-credential" ? 14 : 30)

function expiryOf(item: SeedItem): string | null {
  return item.fields.expires_at ?? item.fields["card.expiry"] ?? null
}

/** The row the list draws: masked, never a value (`session.rs` `list`). */
export function summaryOf(item: WItem): ItemSummary {
  const t = templateOf(item)
  const warnings: string[] = []
  if (item.away?.some((a) => a.remind_at <= NOW)) warnings.push("signed-in-elsewhere")
  if (item.passkeys?.some((p) => p.hidden)) warnings.push("passkey-hidden-by-site")
  const codes = Object.keys(item.fields).filter((k) => /^code\.\d+$/.test(k)).length
  const code = item.fields["code.0"]
  if (code && /issuer=vault\.rivera\.example/.test(code)) warnings.push("issuer-equals-login")
  const methods: string[] = []
  const passkeys = item.kind === "login" || item.kind === "passkey" ? (item.passkeys?.length ?? 0) : 0
  if (item.kind === "login") {
    if (item.fields.pw) methods.push("password")
    if (passkeys > 0) methods.push("passkey")
    if (codes > 0) methods.push("code")
  }
  const expires_at = expiryOf(item)
  const expires_in_days = expires_at ? daysUntil(expires_at) : null
  const state = item.state ?? "live"
  const conflict_copy = isConflict(item.title)
  const views: SmartView[] = []
  if (state === "trashed") views.push("deleted")
  else if (state === "archived") views.push("archived")
  else {
    if (item.favourite) views.push("favourites")
    if (item.kind === "passkey" || passkeys > 0) views.push("passkeys")
    if (item.kind === "code" || codes > 0) views.push("codes")
    if (item.kind === "wi-fi") views.push("wi-fi")
    if (item.template === "identity/contact") views.push("contacts")
    const expiring = expires_in_days !== null && expires_in_days <= leadDays(item.kind)
    if (expiring) views.push("expiring")
    if (expiring || conflict_copy || warnings.length > 0) views.push("attention")
  }
  const site = Object.entries(item.fields).find(([k]) => /^url\.\d+$/.test(k) || k === "router.admin" || k === "server.admin")?.[1] ?? ""
  const username = item.fields.user ?? ""
  const sub = t?.subtitle ? item.fields[t.subtitle] : undefined
  const crown = item.crown ?? false
  const primary = t?.primary
  let copy: ItemSummary["copy"] = null
  if (item.kind === "code" || (item.kind === "login" && !item.fields.pw && code)) copy = { field_id: "code.0", how: "code", label: "code" }
  else if (primary && item.fields[primary] !== undefined) {
    const label = item.kind === "wi-fi" ? "Wi-Fi password" : item.kind === "card" ? "card number" : (t?.fields.find((f) => f.field_id === primary)?.label ?? "password").toLowerCase()
    copy = { field_id: primary, how: "secret", label }
  }
  if (item.access === "fill-only") copy = null
  return {
    id: item.id,
    kind: item.kind,
    state,
    title: item.title,
    site: crown ? "" : site,
    username: crown ? "" : username,
    edited_at: ago(item.edited),
    crown_jewel: crown,
    methods,
    passkeys,
    codes,
    expires_at,
    conflict_copy,
    warnings,
    views,
    subtitle: crown ? "" : (sub ?? username),
    template: item.template ?? item.kind,
    copy,
    vault_id: vaultId(item.vault),
    folder: item.folder?.id ?? null,
    tags: [...(item.tags ?? [])].sort(),
    favourite: item.favourite ?? false,
    last_used: item.used === undefined ? null : ago(item.used),
    uses: item.uses ?? 0,
    expires_in_days,
    attachments: item.attachments?.length ?? 0,
    ...(item.access ? { access: item.access } : {}),
  }
}

/** The item in full, secrets masked (`show`). */
export function detailOf(item: WItem): ItemDetail {
  return {
    id: item.id,
    kind: item.kind,
    state: item.state ?? "live",
    version: item.version,
    edited_at: ago(item.edited),
    crown_jewel: item.crown ?? false,
    export_opt_in: item.exportOptIn ?? false,
    fields: fieldsOf(item),
    vault_id: vaultId(item.vault),
    organization: { folder: item.folder?.id ?? null, tags: [...(item.tags ?? [])].sort(), favourite: item.favourite ?? false },
    attachments: item.attachments ?? [],
    crown_jewel_fixed: item.kind === "crypto-wallet",
    template: item.template ?? item.kind,
    away: item.away ?? [],
    reprompt: item.reprompt ?? false,
    ...(item.access ? { access: item.access } : {}),
    ...(item.tier ? { tier: item.tier, tier_verified: true } : {}),
  }
}

// --- codes ------------------------------------------------------------------------

function fnv(text: string): number {
  let h = 0x811c9dc5
  for (let i = 0; i < text.length; i++) h = Math.imul(h ^ text.charCodeAt(i), 0x01000193) >>> 0
  return h
}

const STEAM = "23456789BCDFGHJKMNPQRTVWXY"

/** A stand-in code for a slot: stable, looks like a code, not a real TOTP. */
function codeFor(secret: string, slot: number, digits: number, steam: boolean): string {
  let h = fnv(`${secret}:${slot}`)
  let out = ""
  for (let i = 0; i < digits; i++) {
    out += steam ? STEAM[h % STEAM.length] : String(h % 10)
    h = Math.floor(h / (steam ? STEAM.length : 10)) || fnv(out + secret)
  }
  return out
}

/** Every code the vault holds, now (`codes`). */
export function codesOf(items: WItem[], at = Date.now() / 1000): CodeView[] {
  const out: CodeView[] = []
  for (const item of items) {
    if ((item.state ?? "live") !== "live") continue
    for (const [field_id, uri] of Object.entries(item.fields)) {
      if (!/^code\.\d+$/.test(field_id)) continue
      const u = new URL(uri.replace(/^otpauth:\/\//, "https://otp/"))
      const steam = uri.startsWith("otpauth://steam/") || u.searchParams.get("encoder") === "steam"
      const hotp = uri.startsWith("otpauth://hotp/")
      const digits = Number(u.searchParams.get("digits") ?? (steam ? 5 : 6))
      const period = Number(u.searchParams.get("period") ?? 30)
      const secret = u.searchParams.get("secret") ?? ""
      const label = decodeURIComponent(u.pathname.split("/").pop() ?? "")
      const [issuerFromLabel, account] = label.includes(":") ? label.split(":") : [null, label]
      const counter = Number(u.searchParams.get("counter") ?? 0)
      const slot = hotp ? counter : Math.floor(at / period)
      const left = hotp ? null : period - Math.floor(at % period)
      out.push({
        item_id: item.id,
        field_id,
        title: item.title,
        issuer: u.searchParams.get("issuer") ?? issuerFromLabel ?? null,
        account: account ?? null,
        variant: steam ? "steam" : hotp ? "hotp" : "totp",
        digits,
        period,
        current: codeFor(secret, slot, digits, steam),
        next: hotp ? null : codeFor(secret, slot + 1, digits, steam),
        seconds_left: left,
        counter: hotp ? counter : null,
        warnings: summaryOf(item).warnings.filter((w) => w === "issuer-equals-login"),
      })
    }
  }
  return out
}

// --- the world --------------------------------------------------------------------------

export interface World {
  items: WItem[]
  settings: Settings
  lock: LockState
  /** What `probe` finds on this device. */
  device: {
    identity: boolean
    deviceKey: boolean
    /** The keys are sealed under a master password (doc 39 §1.3); false is an install from before it. */
    masterPassword?: boolean
    /** The account is the local one the app keeps itself (§1.1). */
    local?: boolean
    /** How far an unfinished first run got. */
    stage?: string | null
  }
  /** How many copies the shell has made, for `clipboard_holds`. */
  copies: number
  /** The web vault's tab: signed in or not, open or locked (`web_state`). */
  web: { signed_in: boolean; locked: boolean; login: string | null }
}

export function freshWorld(): World {
  return {
    items: ITEMS.map((s) => ({ ...s, id: uid("item", s.n), version: Math.max(1, Math.round(s.edited / 40)) })),
    settings: {
      v: 1,
      density: "comfortable",
      reveal: "hold",
      clipboard_clear_secs: 30,
      idle_lock_secs: 600,
      hidden_lock_secs: 300,
      server: PERSON.server,
      breach_check: true,
      breach_endpoint: null,
      change_password_check: true,
      backup: { enabled: true, folder: "/Users/sam/Backups/Lockdown", interval_hours: 24, keep_last: 10 },
      browser_passkeys: true,
      code_clock_correction: true,
      alias_monitoring: true,
      quick_access_hotkey: true,
    },
    lock: { locked: true, reason: null, idle_secs: 0, idle_timeout_secs: 600, hidden_timeout_secs: 300 },
    device: { identity: true, deviceKey: true },
    copies: 0,
    web: { signed_in: true, locked: true, login: PERSON.login },
  }
}

export function vaultViews(world: World): VaultView[] {
  return VAULTS.map((v) => ({
    id: v.id,
    name: v.name,
    primary: v.primary,
    items: world.items.filter((i) => i.vault === v.key && (i.state ?? "live") === "live").length,
    trash_days: v.trash_days,
    folders: v.folders,
    ...(v.key === "family"
      ? { sharing: { kind: "family" as const, right: "manage" as const, owned: true, owner: null, people: 3 } }
      : {}),
  }))
}
