import type { IconName } from "@unicron/ui"

import type { ItemSummary, SmartView } from "@/core/types"

/** What the list shows: everything, or one smart view (D93; brief §3). */
export type ListView = "all" | SmartView

export type SortKey = "title" | "changed"
export type GroupBy = "kind" | "none"

/** The defaults Sean decided (D93): alphabetical by title, grouped by kind. */
export const DEFAULT_SORT: SortKey = "title"
export const DEFAULT_GROUP: GroupBy = "kind"

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
  "software-licence": "Software licences",
  membership: "Memberships",
  custom: "Custom",
}

/** The group order: the hub type first, then the rest as people name them. */
const KIND_ORDER = Object.keys(KIND_LABELS)

export const KIND_ICONS: Record<string, IconName> = {
  login: "key",
  passkey: "passkey",
  code: "timer",
  "wi-fi": "wifi",
  "ssh-key": "terminal",
  "api-credential": "api",
  document: "description",
  identity: "badge",
  card: "credit_card",
  "bank-account": "account_balance",
  note: "sticky_note_2",
  "crypto-wallet": "account_balance_wallet",
  server: "dns",
  database: "database",
  "software-licence": "license",
  membership: "card_membership",
  custom: "category",
}

export function kindLabel(kind: string): string {
  return KIND_LABELS[kind] ?? kind
}

export function kindIcon(kind: string): IconName {
  return KIND_ICONS[kind] ?? "category"
}

/** The sidebar's words for each view. */
export const VIEW_LABELS: Record<ListView, string> = {
  all: "All items",
  passkeys: "Passkeys",
  codes: "Codes",
  "wi-fi": "Wi-Fi",
  expiring: "Expiring",
  attention: "Needs attention",
  deleted: "Deleted",
}

/** The rows a view holds. "All" leaves trashed items to the Deleted view. */
export function rowsIn(rows: ItemSummary[], view: ListView): ItemSummary[] {
  if (view === "all") return rows.filter((r) => r.state !== "trashed")
  return rows.filter((r) => r.views.includes(view))
}

/** How many rows each view holds, for the sidebar's counts. */
export function countViews(rows: ItemSummary[]): Record<ListView, number> {
  const counts: Record<ListView, number> = { all: 0, passkeys: 0, codes: 0, "wi-fi": 0, expiring: 0, attention: 0, deleted: 0 }
  for (const r of rows) {
    if (r.state !== "trashed") counts.all += 1
    for (const v of r.views) counts[v] += 1
  }
  return counts
}

/** A search over the words a row shows: title, site, username. */
export function searchRows(rows: ItemSummary[], query: string): ItemSummary[] {
  const q = query.trim().toLowerCase()
  if (!q) return rows
  return rows.filter((r) => [r.title, r.site, r.username].some((s) => s.toLowerCase().includes(q)))
}

const byTitle = (a: ItemSummary, b: ItemSummary) =>
  a.title.localeCompare(b.title, undefined, { sensitivity: "base" }) || a.id.localeCompare(b.id)

/** A stable sort: title A–Z, or newest change first (ties by title). */
export function sortRows(rows: ItemSummary[], sort: SortKey): ItemSummary[] {
  const out = [...rows]
  if (sort === "title") return out.sort(byTitle)
  return out.sort((a, b) => b.edited_at - a.edited_at || byTitle(a, b))
}

export interface RowGroup {
  /** The kind, or `all` when not grouping. */
  key: string
  label: string
  rows: ItemSummary[]
}

/** Groups sorted rows by kind, with a header and count per group, or one group. */
export function groupRows(rows: ItemSummary[], groupBy: GroupBy): RowGroup[] {
  if (groupBy === "none") return rows.length ? [{ key: "all", label: "", rows }] : []
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

/** The whole pipeline: view → search → sort → group. */
export function buildList(rows: ItemSummary[], view: ListView, query: string, sort: SortKey, groupBy: GroupBy): RowGroup[] {
  return groupRows(sortRows(searchRows(rowsIn(rows, view), query), sort), groupBy)
}
