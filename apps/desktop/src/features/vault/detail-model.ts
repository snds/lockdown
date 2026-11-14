import type { Control, CopyTarget, FieldTemplateView, FieldView, ItemDetail, TemplateView } from "@/core/types"
import { rowDate } from "@/lib/format"

const isCustom = (f: FieldView) => f.field_id.startsWith("custom.")

/**
 * The fields under their template's section headings, in the template's
 * order; then the fields outside it ("Other fields"), then those added by
 * hand ("Your fields"); notes last, as in the editor.
 */
export function sectionsOf(fields: FieldView[], template: TemplateView | null): { name: string; fields: FieldView[] }[] {
  const out: { name: string; fields: FieldView[] }[] = []
  const push = (name: string, f: FieldView) => {
    const s = out.find((x) => x.name === name)
    if (s) s.fields.push(f)
    else out.push({ name, fields: [f] })
  }
  const known = (f: FieldView): FieldTemplateView | undefined => template?.fields.find((t) => t.field_id === f.field_id)
  const ordered = [...fields].sort((a, b) => {
    const i = (f: FieldView) => {
      const at = template?.fields.findIndex((t) => t.field_id === f.field_id) ?? -1
      return at < 0 ? Number.MAX_SAFE_INTEGER : at
    }
    return i(a) - i(b)
  })
  for (const f of ordered.filter((f) => known(f)?.section !== "Notes")) {
    const t = known(f)
    // A template's catch-all "Details" would share its name with the item's own Details at the foot (R18).
    const section = t?.section === "Details" ? "More details" : t?.section
    push(section ?? (isCustom(f) ? "Your fields" : "Other fields"), f)
  }
  for (const f of ordered.filter((f) => known(f)?.section === "Notes")) push("Notes", f)
  return out
}

/** The match rule a website has when nobody chose one (crates/vault `DEFAULT_MATCH_RULE`): its row says nothing about it. */
export const DEFAULT_MATCH_RULE = "exact_host"

/** A field holds something to draw (brief I2: a field with no value is not drawn). */
export function filled(f: FieldView): boolean {
  if (f.hidden) return true
  if (f.kind === "linked") return true
  return (f.value ?? "").trim() !== ""
}

/** An address as a row draws it: no scheme, no trailing slash ("store.harriergames.example"). */
export function siteWords(url: string): string {
  return url
    .trim()
    .replace(/^[a-z][a-z0-9+.-]*:\/\//i, "")
    .replace(/\/$/, "")
}

/** An address's host, for the mono line under the title; the address itself when it is not a URL. */
export function hostOf(url: string): string {
  try {
    return new URL(url.includes("://") ? url : `https://${url}`).hostname || siteWords(url)
  } catch {
    return siteWords(url)
  }
}

/** A date or a month as the frames write one: `2029.08.14`, `08/29`. Anything else as typed. */
export function dateWords(raw: string, control: Control | undefined): string {
  const t = raw.trim()
  const day = /^(\d{4})-(\d{2})-(\d{2})$/.exec(t)
  if (day) return `${day[1]}.${day[2]}.${day[3]}`
  const month = /^(\d{4})-(\d{2})$/.exec(t)
  if (month && control === "month") return `${month[2]}/${month[1]!.slice(2)}`
  return t
}

/** Each kind's word, singular, for the overline (brief I2). */
const KIND_WORDS: Record<string, string> = {
  login: "Login",
  passkey: "Passkey",
  code: "Code",
  "wi-fi": "Wi-Fi",
  "ssh-key": "SSH key",
  "api-credential": "API key",
  document: "Document",
  identity: "Identity",
  card: "Card",
  "bank-account": "Bank account",
  note: "Secure note",
  "crypto-wallet": "Crypto wallet",
  server: "Server",
  database: "Database",
  "software-licence": "Software license",
  membership: "Membership",
  custom: "Custom",
}

/** The field that tells one item of a kind apart, after the kind (brief I2: `CARD // VISA`, `SSH KEY // ED25519`). */
const TELLS: Record<string, string> = {
  card: "card.type",
  "wi-fi": "wifi.security",
  "ssh-key": "ssh.key_type",
  "api-credential": "api.type",
  database: "db.type",
  "bank-account": "bank.name",
  "software-licence": "licence.publisher",
  membership: "member.organization",
  server: "server.host",
  document: "doc.type",
}

/** What the overline knows besides the item: the parts the detail loads beside it. */
export interface OverlineFacts {
  passkeys: boolean
  codes: boolean
  /** The code's kind, for a one-time code item (`CODE // STEAM`). */
  code?: { variant: "totp" | "hotp" | "steam"; digits: number } | null
  /** The list says this is the second copy of a conflict (D58). */
  conflict?: boolean
  /** Seconds now, for a note's `EDITED NOV 08`. */
  now?: number
}

/**
 * The detail's overline (brief I1 #1): the kind, then what tells this one
 * apart (a login's methods, a variant, a brand), then the state words.
 * Typed in sentence case; the overline role sets the capitals.
 */
export function overlineOf(item: ItemDetail, template: TemplateView | null, facts: OverlineFacts): string {
  const value = (id: string) => item.fields.find((f) => f.field_id === id && !f.hidden)?.value?.trim() || ""
  const own = template?.id?.startsWith("user:")
  const parts: string[] = [own && template ? template.name : (KIND_WORDS[item.kind] ?? template?.name ?? item.kind)]
  if (!own && template?.variant) parts.push(template.variant)
  if (item.kind === "login") {
    const hasPassword = item.fields.some((f) => f.field_id === "pw")
    const methods = [facts.passkeys && "passkey", facts.codes && "code"].filter(Boolean) as string[]
    if (methods.length) parts.push(methods.join(" + "))
    if (!hasPassword && methods.length) parts.push("no password")
  } else if (item.kind === "passkey") {
    parts.push("no password")
  } else if (item.kind === "code" && facts.code) {
    parts.push(facts.code.variant === "steam" ? "Steam" : facts.code.variant === "hotp" ? "counter" : `${facts.code.digits} digits`)
  } else if (item.kind === "note") {
    parts.push(`edited ${rowDate(item.edited_at, facts.now)}`)
  } else if (TELLS[item.kind]) {
    const tell = value(TELLS[item.kind]!)
    if (tell) parts.push(tell)
  }
  if (item.crown_jewel) parts.push("crown jewel")
  if (item.access) parts.push("shared with you", item.access === "fill-only" ? "fill only" : item.access)
  if (item.state === "archived") parts.push("archived")
  if (item.state === "trashed") parts.push("recently deleted")
  if (facts.conflict) parts.push("conflict copy")
  return parts.join(" // ")
}

/** The words a card's art shows, from its real fields only (brief I2; no last four: wish W1). */
export interface CardFace {
  brand: string
  holder: string
  expires: string
  hasNumber: boolean
  hasCvc: boolean
}

export function cardFaceOf(item: ItemDetail): CardFace {
  const v = (id: string) => item.fields.find((f) => f.field_id === id && !f.hidden)?.value?.trim() || ""
  return {
    brand: v("card.type"),
    holder: v("card.holder"),
    expires: dateWords(v("card.expiry"), "month"),
    hasNumber: item.fields.some((f) => f.field_id === "card.number"),
    hasCvc: item.fields.some((f) => f.field_id === "card.cvc"),
  }
}

/** Public keys read and copy as code (brief I2: `CodeBlock` for SSH and PGP public keys). */
export const CODE_BLOCK_FIELDS = new Set(["ssh.public_key", "pgp.public_key"])

/** What the phone's bottom row holds for an item (brief I1), chosen by the row's quick copy (`ItemSummary.copy`). */
export interface BottomRowPlan {
  /** The eye: shows the copied secret's own row, as the Reveal setting says. */
  start: "reveal" | null
  /** The primary: copies the secret or the code; in Recently deleted, restores. */
  end: "copy-secret" | "copy-code" | "restore" | null
}

/**
 * A secret: the eye at the start and its copy at the end. A code: its copy
 * only. Nothing to copy (a passkey-only login, a note with nothing secret):
 * no row; the fingerprint button the frame draws is left out (Q17). In
 * Recently deleted: Restore, for someone who may change the item. A
 * fill-only share is never shown or copied, so it has no row.
 */
export function bottomRowOf(item: Pick<ItemDetail, "state" | "access">, copy: CopyTarget | null, canEdit: boolean): BottomRowPlan {
  if (item.state === "trashed") return { start: null, end: canEdit ? "restore" : null }
  if (item.access === "fill-only" || !copy) return { start: null, end: null }
  return copy.how === "secret" ? { start: "reveal", end: "copy-secret" } : { start: null, end: "copy-code" }
}
