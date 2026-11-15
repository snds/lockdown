import type { FolderView, ItemSummary, OrganizationView, VaultView } from "@/core/types"

/**
 * Where the list is looking, beyond its smart view: one vault, one folder
 * of a vault (with the folders inside it), or one tag (with the tags
 * nested under it). Empty means everywhere.
 */
export interface Scope {
  vault?: string
  folder?: string
  tag?: string
}

/** The folder ids at or under `folder` in its vault. */
export function folderAndBelow(folders: FolderView[], folder: string): Set<string> {
  const out = new Set([folder])
  let grew = true
  while (grew) {
    grew = false
    for (const f of folders) {
      if (f.parent && out.has(f.parent) && !out.has(f.id)) {
        out.add(f.id)
        grew = true
      }
    }
  }
  return out
}

/** Whether `tag` is `scope` or nested under it (`work` holds `work/clients`), case ignored. */
export function tagWithin(tag: string, scope: string): boolean {
  const t = tag.toLowerCase()
  const s = scope.toLowerCase()
  return t === s || t.startsWith(`${s}/`)
}

/** The rows the scope holds. */
export function inScope(rows: ItemSummary[], scope: Scope, vaults: VaultView[]): ItemSummary[] {
  let out = rows
  if (scope.vault) out = out.filter((r) => r.vault_id === scope.vault)
  if (scope.folder) {
    const vault = vaults.find((v) => v.folders.some((f) => f.id === scope.folder))
    const ids = folderAndBelow(vault?.folders ?? [], scope.folder)
    out = out.filter((r) => r.folder != null && ids.has(r.folder))
  }
  if (scope.tag) {
    const tag = scope.tag
    out = out.filter((r) => (r.tags ?? []).some((t) => tagWithin(t, tag)))
  }
  return out
}

/** The words the scope is named by in the list's header. */
export function scopeTitle(scope: Scope, vaults: VaultView[]): string | null {
  if (scope.folder) {
    for (const v of vaults) {
      const f = v.folders.find((x) => x.id === scope.folder)
      if (f) return vaults.length > 1 ? `${v.name} / ${f.path}` : f.path
    }
    return "Folder"
  }
  if (scope.tag) return `#${scope.tag}`
  if (scope.vault) return vaults.find((v) => v.id === scope.vault)?.name ?? "Vault"
  return null
}

/** Every folder's path by id, for search words and row captions. */
export function folderPaths(vaults: VaultView[]): Map<string, string> {
  const out = new Map<string, string>()
  for (const v of vaults) for (const f of v.folders) out.set(f.id, f.path)
  return out
}

/** One node of the tag tree in the sidebar. */
export interface TagNode {
  /** The full tag (`work/clients`). */
  tag: string
  /** Its last part (`clients`). */
  name: string
  depth: number
  /** Items carrying it or a tag under it (archived and trashed left out). */
  count: number
}

/**
 * Every tag the rows carry, and each tag's parents (`work` for
 * `work/clients`) even when no item carries the parent alone, in
 * alphabetical order with children under their parent.
 */
export function tagTree(rows: ItemSummary[]): TagNode[] {
  const counts = new Map<string, Set<string>>()
  const names = new Map<string, string>()
  for (const r of rows) {
    if (r.state !== "live") continue
    for (const t of r.tags ?? []) {
      const parts = t.split("/")
      for (let i = 1; i <= parts.length; i++) {
        const prefix = parts.slice(0, i).join("/")
        const key = prefix.toLowerCase()
        if (!names.has(key)) names.set(key, prefix)
        const set = counts.get(key) ?? new Set<string>()
        set.add(r.id)
        counts.set(key, set)
      }
    }
  }
  return [...names.entries()]
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([key, tag]) => {
      const parts = tag.split("/")
      return { tag, name: parts[parts.length - 1] ?? tag, depth: parts.length - 1, count: counts.get(key)?.size ?? 0 }
    })
}

/** A vault's folders as a tree in display order, each with its depth. */
export function folderTree(folders: FolderView[]): Array<FolderView & { depth: number }> {
  const byParent = new Map<string | null, FolderView[]>()
  const ids = new Set(folders.map((f) => f.id))
  for (const f of folders) {
    const p = f.parent && ids.has(f.parent) ? f.parent : null
    const list = byParent.get(p) ?? []
    list.push(f)
    byParent.set(p, list)
  }
  const out: Array<FolderView & { depth: number }> = []
  const walk = (parent: string | null, depth: number, seen: Set<string>) => {
    const kids = [...(byParent.get(parent) ?? [])].sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" }))
    for (const k of kids) {
      if (seen.has(k.id)) continue
      seen.add(k.id)
      out.push({ ...k, depth })
      walk(k.id, depth + 1, seen)
    }
  }
  walk(null, 0, new Set())
  return out
}

/**
 * How strongly this device's use ranks a row now, as the core ranks it
 * (`lockdown_app::recency::score`): the use count halved for every two
 * weeks since the last use.
 */
export function useScore(row: ItemSummary, nowSecs: number): number {
  if (!row.uses || row.last_used == null) return 0
  const weeks = Math.max(0, nowSecs - row.last_used) / (14 * 86_400)
  return row.uses / 2 ** weeks
}

/** Favourites first, then by use, then by title: the palette's order. */
export function rankRows(rows: ItemSummary[], nowSecs: number): ItemSummary[] {
  return [...rows].sort(
    (a, b) =>
      Number(Boolean(b.favourite)) - Number(Boolean(a.favourite)) ||
      useScore(b, nowSecs) - useScore(a, nowSecs) ||
      a.title.localeCompare(b.title, undefined, { sensitivity: "base" }) ||
      a.id.localeCompare(b.id),
  )
}

/** The Expiring view's sections, by how soon (D-row in 15-organization §5). */
export type ExpiryBucket = "lapsed" | "week" | "month" | "later"

export const EXPIRY_LABELS: Record<ExpiryBucket, string> = {
  lapsed: "Expired",
  week: "Within a week",
  month: "Within a month",
  later: "Later",
}

export function expiryBucket(days: number | null | undefined): ExpiryBucket {
  if (days == null) return "later"
  if (days < 0) return "lapsed"
  if (days <= 7) return "week"
  if (days <= 31) return "month"
  return "later"
}

/** When it lapses, in words: "expired 3 days ago", "expires today", "expires in 12 days". */
export function expiryWords(days: number | null | undefined): string {
  if (days == null) return ""
  if (days < -1) return `expired ${-days} days ago`
  if (days === -1) return "expired yesterday"
  if (days === 0) return "expires today"
  if (days === 1) return "expires tomorrow"
  return `expires in ${days} days`
}

/** A web address's match rules, in plain words (02-model §10). */
export const MATCH_RULES: Array<[string, string]> = [
  ["exact_host", "This address only"],
  ["base_domain", "Anywhere on this site"],
  ["equivalent", "This site and its sister sites"],
  ["starts_with", "Pages starting with this address"],
  ["never", "Never offer here"],
]

export function matchRuleWords(rule: string | undefined): string {
  return MATCH_RULES.find(([id]) => id === rule)?.[1] ?? "This address only"
}

/** A file size in words: "512 bytes", "12 KB", "3.4 MB". */
export function sizeWords(bytes: number): string {
  if (bytes < 1024) return `${bytes} bytes`
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}

/** Tags typed into one box, split on commas or new lines, trimmed, without repeats. */
export function parseTags(text: string): string[] {
  const out: string[] = []
  for (const raw of text.split(/[,\n]/)) {
    const t = raw
      .split("/")
      .map((p) => p.trim())
      .join("/")
      .trim()
    if (t && !out.some((o) => o.toLowerCase() === t.toLowerCase())) out.push(t)
  }
  return out
}

/** What the organize prompt files: one item, its vault and where it is now. */
export interface OrganizeTarget {
  id: string
  title: string
  vaultId: string
  organization: OrganizationView
}

/** The organize prompt's target for a list row. */
export function targetOf(row: ItemSummary): OrganizeTarget {
  return {
    id: row.id,
    title: row.title,
    vaultId: row.vault_id ?? "",
    organization: { folder: row.folder ?? null, tags: row.tags ?? [], favourite: Boolean(row.favourite) },
  }
}

/** Whether a row's menu has anything to offer: only what the person's right allows (docs/plan/18-sharing.md). */
export function rowMenuOffers(row: ItemSummary): boolean {
  return !row.access || row.access === "edit" || row.access === "manage"
}

// --- the vault, folder and tag menus (brief V2) -------------------------------------------

/** How long a vault keeps what was deleted before every device purges it, in days (the core's choices). */
export const TRASH_DAYS = [7, 14, 30, 60, 90, 365] as const

/** A choice of `TRASH_DAYS` in words: "30 days", "A year". */
export function trashDaysWords(days: number): string {
  return days === 365 ? "A year" : `${days} days`
}

/**
 * Whether the person may change a vault: name it, file in it, set how long it keeps deleted items.
 * Theirs, or shared with them to manage (docs/plan/18-sharing.md: nothing the right doesn't allow).
 */
export function canManageVault(v: VaultView): boolean {
  return !v.sharing || v.sharing.owned || v.sharing.right === "manage"
}

/** Whether the person may make, rename, move and remove the folders of a vault: theirs, or shared to edit or manage. */
export function canFileInVault(v: VaultView): boolean {
  return !v.sharing || v.sharing.owned || v.sharing.right === "edit" || v.sharing.right === "manage"
}

/** One row of a vault's ⋯ menu. */
export type VaultMenuRow = "new-folder" | "rename" | "trash-days" | "share" | "members" | "who-has-it"

/**
 * A vault's ⋯ menu, in order (brief V2): a vault you can manage offers New folder, Rename, Keep
 * deleted items for… and its sharing, which is Members once it is shared and Share… before; a vault
 * shared with you offers Who has it. `sharing` is false where Sharing has no place (no account).
 */
export function vaultMenuOffers(v: VaultView, sharing: boolean): VaultMenuRow[] {
  const shared = Boolean(v.sharing) && v.sharing?.kind !== "received"
  if (!canManageVault(v)) return [...(canFileInVault(v) ? (["new-folder"] as const) : []), ...(sharing ? (["who-has-it"] as const) : [])]
  return ["new-folder", "rename", "trash-days", ...(sharing ? [shared ? ("members" as const) : ("share" as const)] : [])]
}

/** One row of a folder's ⋯ menu, in order (brief V2). Move to the top only for a folder inside another. */
export function folderMenuOffers(folder: FolderView): Array<"new-folder" | "rename" | "move-to-top" | "remove"> {
  return ["new-folder", "rename", ...(folder.parent ? (["move-to-top"] as const) : []), "remove"]
}
