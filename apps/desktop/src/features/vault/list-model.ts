import type { IconName } from "@unicron/ui"

import type { Finding, ItemSummary, SmartView, VaultView } from "@/core/types"
import { rowDate } from "@/lib/format"

import { hostOf, kindWord } from "./detail-model"

/**
 * The kinds that are a view of their own (redesign brief X1): every kind
 * but the three a smart view already lists (`passkeys`, `codes`, `wi-fi`),
 * in the order the More list shows them.
 */
export const KIND_VIEWS = ["login", "card", "note", "ssh-key", "api-credential", "document", "identity", "bank-account", "crypto-wallet", "server", "database", "software-licence", "membership", "custom"] as const
export type KindView = (typeof KIND_VIEWS)[number]

/** What the list shows: everything, one smart view (D93; brief §3), or one kind. */
export type ListView = "all" | SmartView | KindView

/** Whether a view is one kind's (its value is the kind's own id). */
export function isKindView(view: ListView): view is KindView {
  return (KIND_VIEWS as readonly string[]).includes(view)
}

export type SortKey = "title" | "changed" | "used"
/** `letter`: A to Z headers (brief V1, changes D93's default); `kind`: the menu's "By type"; `none`. */
export type GroupBy = "letter" | "kind" | "none"

/** A to Z by title (D93, kept). */
export const DEFAULT_SORT: SortKey = "title"
/**
 * The grouping a device starts with (brief V1, D780): by letter on a
 * desktop, as Desktop / 01 draws it; none on a phone, as Mobile / 01 draws
 * it. D93's "by kind" stays in the menu as "By type".
 */
export function defaultGroup(phone: boolean): GroupBy {
  return phone ? "none" : "letter"
}
/** Kept for callers of D93's default: the desktop's. */
export const DEFAULT_GROUP: GroupBy = "letter"

/** Letter headers make sense only in title order: with another sort the list is one group. */
export function groupFor(sort: SortKey, group: GroupBy): GroupBy {
  return group === "letter" && sort !== "title" ? "none" : group
}

/** Plain words for every kind the vault knows (crates/vault ItemKind::as_str). */
export const KIND_LABELS: Record<string, string> = {
  login: "Logins",
  passkey: "Passkeys",
  code: "Codes",
  "wi-fi": "Wi-Fi",
  "ssh-key": "SSH keys",
  "api-credential": "API credentials",
  document: "Documents",
  identity: "Identities",
  card: "Cards",
  "bank-account": "Bank accounts",
  note: "Notes",
  "crypto-wallet": "Crypto wallets",
  server: "Servers",
  database: "Databases",
  "software-licence": "Software licenses",
  membership: "Memberships",
  custom: "Custom",
}

/** The group order: the hub type first, then the rest as people name them. */
const KIND_ORDER = Object.keys(KIND_LABELS)

export const KIND_ICONS: Record<string, IconName> = {
  login: "password",
  passkey: "locked",
  code: "time",
  "wi-fi": "wifi",
  "ssh-key": "terminal",
  "api-credential": "api",
  document: "document",
  identity: "user--identification",
  card: "wallet",
  "bank-account": "finance",
  note: "document",
  "crypto-wallet": "wallet",
  server: "bare-metal-server",
  database: "data--base",
  "software-licence": "license",
  membership: "credentials",
  custom: "categories",
}

export function kindLabel(kind: string): string {
  return KIND_LABELS[kind] ?? kind
}

export function kindIcon(kind: string): IconName {
  return KIND_ICONS[kind] ?? "categories"
}

/** The sidebar's words for each view. */
export const VIEW_LABELS: Record<ListView, string> = {
  all: "All items",
  passkeys: "Passkeys",
  codes: "Codes",
  "wi-fi": "Wi-Fi",
  expiring: "Expiring",
  attention: "Needs attention",
  deleted: "Recently deleted",
  favourites: "Favorites",
  archived: "Archived",
  contacts: "Contacts",
  login: "Logins",
  card: "Cards",
  note: "Notes",
  "ssh-key": "SSH keys",
  "api-credential": "API credentials",
  document: "Documents",
  identity: "Identities",
  "bank-account": "Bank accounts",
  "crypto-wallet": "Crypto wallets",
  server: "Servers",
  database: "Databases",
  "software-licence": "Software licenses",
  membership: "Memberships",
  custom: "Custom",
}

/** The rows a view holds. "All" leaves trashed and archived items to their own views. */
export function rowsIn(rows: ItemSummary[], view: ListView): ItemSummary[] {
  if (view === "all") return rows.filter((r) => r.state !== "trashed" && r.state !== "archived")
  if (isKindView(view)) return rows.filter((r) => r.kind === view && r.state !== "trashed" && r.state !== "archived")
  return rows.filter((r) => r.views.includes(view))
}

/** How many rows each view holds, for the sidebar's counts. */
export function countViews(rows: ItemSummary[]): Record<ListView, number> {
  const counts = Object.fromEntries((Object.keys(VIEW_LABELS) as ListView[]).map((v) => [v, 0])) as Record<ListView, number>
  for (const r of rows) {
    if (r.state !== "trashed" && r.state !== "archived") {
      counts.all += 1
      if (isKindView(r.kind as ListView)) counts[r.kind as KindView] += 1
    }
    for (const v of r.views) counts[v] += 1
  }
  return counts
}

/** How many kinds hold an item that is neither archived nor in Recently deleted: the `ITEMS // 09 TYPES` overline. */
export function countTypes(rows: ItemSummary[]): number {
  return new Set(rows.filter((r) => r.state !== "trashed" && r.state !== "archived").map((r) => r.kind)).size
}

/**
 * A search over the words a row shows: title, site, username, the subtitle
 * its template names (a network, a card's holder), and its tags and
 * folder (`folders`: a folder's path by id). A crown jewel is found by its
 * title alone (02-model §3: "never in search previews").
 */
export function searchRows(rows: ItemSummary[], query: string, folders?: Map<string, string>): ItemSummary[] {
  const q = query.trim().toLowerCase()
  if (!q) return rows
  return rows.filter((r) => {
    const words = r.crown_jewel
      ? [r.title]
      : [r.title, r.site, r.username, r.subtitle, ...(r.tags ?? []).map((t) => `#${t}`), (r.folder && folders?.get(r.folder)) || ""]
    return words.some((s) => s.toLowerCase().includes(q))
  })
}

const byTitle = (a: ItemSummary, b: ItemSummary) =>
  a.title.localeCompare(b.title, undefined, { sensitivity: "base" }) || a.id.localeCompare(b.id)

/** A stable sort: title A–Z, newest change first, or most recently used on this device first (ties by title). */
export function sortRows(rows: ItemSummary[], sort: SortKey): ItemSummary[] {
  const out = [...rows]
  if (sort === "title") return out.sort(byTitle)
  if (sort === "used") return out.sort((a, b) => (b.last_used ?? 0) - (a.last_used ?? 0) || byTitle(a, b))
  return out.sort((a, b) => b.edited_at - a.edited_at || byTitle(a, b))
}

export interface RowGroup {
  /** The kind, `letter:S` for a letter, or `all` when not grouping. */
  key: string
  label: string
  rows: ItemSummary[]
}

/** The letter a title files under: its first letter, without accents, in capitals; `#` for anything else. */
export function letterOf(title: string): string {
  const first = title.trim().normalize("NFD").replace(/[\u0300-\u036f]/g, "").charAt(0).toUpperCase()
  return /^[A-Z]$/.test(first) ? first : "#"
}

/**
 * Groups sorted rows: by first letter (in the order the rows come, so A
 * to Z when they are sorted by title; `#` first, as it sorts first), by
 * kind with the hub type first, or one group.
 */
export function groupRows(rows: ItemSummary[], groupBy: GroupBy): RowGroup[] {
  if (groupBy === "none") return rows.length ? [{ key: "all", label: "", rows }] : []
  if (groupBy === "letter") {
    const letters = new Map<string, ItemSummary[]>()
    for (const r of rows) {
      const l = letterOf(r.title)
      const list = letters.get(l)
      if (list) list.push(r)
      else letters.set(l, [r])
    }
    return [...letters.entries()].map(([l, list]) => ({ key: `letter:${l}`, label: l, rows: list }))
  }
  const groups = new Map<string, ItemSummary[]>()
  for (const r of rows) {
    const list = groups.get(r.kind)
    if (list) list.push(r)
    else groups.set(r.kind, [r])
  }
  const order = (k: string) => {
    const i = KIND_ORDER.indexOf(k)
    return i < 0 ? KIND_ORDER.length : i
  }
  return [...groups.entries()]
    .sort(([a], [b]) => order(a) - order(b) || a.localeCompare(b))
    .map(([key, list]) => ({ key, label: kindLabel(key), rows: list }))
}

/** The whole pipeline: view → search → sort → group (letters only in title order). */
export function buildList(rows: ItemSummary[], view: ListView, query: string, sort: SortKey, groupBy: GroupBy, folders?: Map<string, string>): RowGroup[] {
  return groupRows(sortRows(searchRows(rowsIn(rows, view), query, folders), sort), groupFor(sort, groupBy))
}

// --- what a row says (brief V1) ---------------------------------------------------------------

/**
 * The view a kind opens as, for the chip row of a scope (brief X1): its
 * own kind view, or the smart view that already lists it. `null` for a kind
 * no view lists by itself.
 */
export function viewOfKind(kind: string): ListView | null {
  if (isKindView(kind as ListView)) return kind as KindView
  if (kind === "wi-fi") return "wi-fi"
  if (kind === "passkey") return "passkeys"
  if (kind === "code") return "codes"
  return null
}

/** The views of the kinds a scope holds, in the sidebar's order: the chips after All. */
export function scopeChips(rows: ItemSummary[]): ListView[] {
  const views = new Set(rowsIn(rows, "all").map((r) => viewOfKind(r.kind)).filter((v): v is ListView => v !== null))
  return (Object.keys(VIEW_LABELS) as ListView[]).filter((v) => views.has(v))
}

/**
 * The mono line under a row's title: a login's site, a note's last edit
 * (`edited NOV 08`), any other kind's plain field (a network, a card's
 * holder, a server's address). Nothing for a crown jewel (D113).
 */
export function monoLine(row: ItemSummary, nowSecs?: number): string {
  if (row.crown_jewel) return ""
  if (row.kind === "login" && row.site) return hostOf(row.site)
  if (row.kind === "note") return `edited ${rowDate(row.edited_at, nowSecs)}`
  return row.subtitle
}

/** A login's ways in, as the frames write them: `Passkey + code`, `Login`, `Login + code`. */
export function methodWords(row: ItemSummary): string {
  const base = row.methods.includes("passkey") ? "Passkey" : "Login"
  return row.methods.includes("code") ? `${base} + code` : base
}

/** The findings a desktop row names, worst first (brief V1). */
const FINDING_WORDS: ReadonlyArray<[Finding["kind"], string]> = [
  ["breached", "breached"],
  ["reused", "reused"],
  ["weak", "weak"],
]

/** Each item's worst finding from the cached report (`Finding.item_id`): `weak`, `reused` or `breached`. */
export function findingWords(findings: readonly Finding[] | null | undefined): Map<string, string> {
  const out = new Map<string, string>()
  if (!findings) return out
  for (const [kind, word] of FINDING_WORDS) {
    for (const f of findings) if (f.kind === kind && f.item_id && !f.dismissed && !out.has(f.item_id)) out.set(f.item_id, word)
  }
  return out
}

/** What the row's right-hand words say besides the row itself. */
export interface RowWordsContext {
  view: ListView
  /** The vaults, for a kind view across them. */
  vaults?: readonly VaultView[]
  /** The list is narrowed to one vault (or a folder in it). */
  oneVault?: boolean
  /** Desktop only: each item's worst finding (`findingWords`). */
  findings?: Map<string, string>
}

/**
 * The row's right-hand words (brief V1), typed in sentence case; the
 * overline role sets the capitals. A login's methods or the kind's name; in
 * a kind view across vaults, the vault's name (Desktop / 03). Then the state
 * words after `//`: conflict, archived, deleted, shared, and on a desktop a
 * finding. A state the view already names is not said again.
 */
export function rowWords(row: ItemSummary, ctx: RowWordsContext): string {
  const { view, vaults = [], oneVault = false, findings } = ctx
  const kindView = isKindView(view) || view === "wi-fi" || view === "contacts"
  const vault = kindView && !oneVault && vaults.length > 1 ? vaults.find((v) => v.id === row.vault_id)?.name : undefined
  const parts = [vault ?? (row.kind === "login" ? methodWords(row) : kindWord(row.kind))]
  if (row.conflict_copy) parts.push("conflict")
  if (row.state === "archived" && view !== "archived") parts.push("archived")
  if (row.state === "trashed" && view !== "deleted") parts.push("deleted")
  if (row.access) parts.push("shared")
  const finding = findings?.get(row.id)
  if (finding) parts.push(finding)
  return parts.join(" // ")
}

/** The marker a conflict copy's title carries (`lockdown_app::views::CONFLICT_MARK`, D58). */
export const CONFLICT_MARK = " (conflict copy from "

/** The title both copies share: the marker and what follows it stripped. */
export function baseTitle(title: string): string {
  const at = title.indexOf(CONFLICT_MARK)
  return at < 0 ? title : title.slice(0, at)
}

/**
 * The other half of a conflict (D58): for a conflict copy, the live item it
 * was split from; for an item that has conflict copies, the newest of them.
 * `null` when the item is in no conflict, or its other half is gone.
 */
export function conflictPair(rows: ItemSummary[], id: string): ItemSummary | null {
  const me = rows.find((r) => r.id === id)
  if (!me) return null
  const live = rows.filter((r) => r.id !== id && r.state === "live" && r.kind === me.kind)
  if (me.conflict_copy) {
    const base = baseTitle(me.title)
    return live.find((r) => !r.conflict_copy && r.title === base) ?? null
  }
  const copies = live.filter((r) => r.conflict_copy && baseTitle(r.title) === me.title)
  return copies.sort((a, b) => b.edited_at - a.edited_at)[0] ?? null
}
