import { createContext, useContext } from "react"

import type { IconName } from "@unicron/ui"

import type { VaultView } from "@/core/types"
import type { Scope } from "@/features/organization/org-model"
import { KIND_ICONS, KIND_VIEWS, VIEW_LABELS, type ListView } from "@/features/vault/list-model"

/**
 * Every place in Lockdown, in one table (redesign brief §3 and X1, doc 64).
 * The sidebar, the rail, the phone's tab bar, the chip row, the MORE sheet,
 * the Settings section list and the breadcrumb all read it, so a place has
 * one name, one icon and one address everywhere. A lane moves with
 * `go(placeId)` and never builds a `Nav` by hand.
 */

/** Which build draws the place: the desktop app, the web vault, or both. */
export type Surface = "desktop" | "web"

/** The areas of the window. Devices, Recovery and Status are Settings sections; People is a Sharing tab (D721, D722). */
export type Area = "vault" | "codes" | "security" | "sharing" | "search" | "settings"

/** The tabs of Sharing. The web vault has People and Links only. */
export type SharingTab = "people" | "vaults" | "links" | "emergency"

/** The sections of Settings, in the order the section list shows them (brief T0). */
export type SettingsSection =
  | "this-device"
  | "browsers"
  | "quick-access"
  | "appearance"
  | "devices"
  | "recovery"
  | "status"
  | "server"
  | "import-export"
  | "backups"
  | "aliases"
  | "codes"
  | "security-checks"
  | "about"

export interface Nav {
  area: Area
  /** Which list the vault shows; only read while `area` is `vault`. */
  view: ListView
  /** A vault, folder or tag the list is narrowed to (features/organization). */
  scope?: Scope
  /** A device the Devices section brings into view (the Health screen's post-quantum line). */
  device?: string
  /** The open Settings section. Left out: the first one on a desktop, the list itself on a phone. */
  section?: SettingsSection
  /** The open Sharing tab. Left out: the first one. */
  tab?: SharingTab
  /** One shared vault's members: the vault's id. Only read while `area` is `sharing`. */
  members?: string
}

/** Where a place sits in the navigation. */
export type PlaceGroup = "search" | "items" | "more" | "aside" | "tools" | "health" | "sharing" | "settings"

/** The Settings groups, as the section list's overlines. `device` is named for the device ("This Mac"). */
export type SettingsGroup = "device" | "account" | "vault" | "lockdown"

export interface Place {
  /** An id that outlives the label: what `go` takes, and what tests and capture steps name. */
  id: string
  label: string
  icon: IconName
  group: PlaceGroup
  surfaces: readonly Surface[]
  /** The breadcrumb, outermost first, as written (the status line sets the capitals). */
  crumbs: readonly string[]
  /** The key that goes there, as drawn in a `Kbd` ("/"). */
  key?: string
  /** Where it goes. */
  nav: Nav
  /** A Settings section's group. */
  settingsGroup?: SettingsGroup
  /** A Settings section's one line under its title. */
  description?: string
}

const BOTH: readonly Surface[] = ["desktop", "web"]
const DESKTOP: readonly Surface[] = ["desktop"]

/** The six views the sidebar and the chip row always offer after All items (brief 3.2). */
export const MAIN_VIEWS: readonly ListView[] = ["favourites", "login", "passkeys", "codes", "card", "note"]
/** The kinds under More, in the code's order (brief 3.2), then what is kept aside. */
export const MORE_VIEWS: readonly ListView[] = ["wi-fi", ...KIND_VIEWS.filter((k) => !MAIN_VIEWS.includes(k)), "contacts"]
export const ASIDE_VIEWS: readonly ListView[] = ["archived", "deleted"]
/** The two lists Health holds (D724): today's views, reached from Health. */
export const HEALTH_VIEWS: readonly ListView[] = ["expiring", "attention"]

const VIEW_ICONS: Partial<Record<ListView, IconName>> = {
  all: "folder",
  favourites: "star",
  login: "password",
  passkeys: "locked",
  codes: "time",
  card: "wallet",
  note: "document",
  "wi-fi": "wifi",
  contacts: "user--profile",
  archived: "archive",
  deleted: "trash-can",
  expiring: "event--schedule",
  attention: "warning--alt",
}

export function viewIcon(view: ListView): IconName {
  return VIEW_ICONS[view] ?? KIND_ICONS[view] ?? "categories"
}

const view = (v: ListView, group: PlaceGroup): Place => ({
  id: v,
  label: VIEW_LABELS[v],
  icon: viewIcon(v),
  group,
  surfaces: BOTH,
  crumbs: group === "health" ? ["Tools", "Health", VIEW_LABELS[v]] : ["Items", v === "all" ? "All" : VIEW_LABELS[v]],
  nav: v === "codes" ? { area: "codes", view: "codes" } : { area: "vault", view: v },
})

const SHARING_TABS: ReadonlyArray<[SharingTab, string, IconName, readonly Surface[]]> = [
  ["people", "People", "user--multiple", BOTH],
  ["vaults", "Vaults", "folder", DESKTOP],
  ["links", "Links", "link", BOTH],
  ["emergency", "Emergency", "health-cross", DESKTOP],
]

const SECTIONS: ReadonlyArray<[SettingsSection, string, IconName, SettingsGroup, string, readonly Surface[]]> = [
  ["this-device", "This device", "laptop", "device", "How Lockdown locks, copies and reveals here.", BOTH],
  ["browsers", "Browsers", "earth", "device", "The browsers this app fills in.", DESKTOP],
  ["quick-access", "Quick access", "flash", "device", "The shortcut that opens Lockdown from anywhere.", DESKTOP],
  ["appearance", "Appearance", "color-palette", "device", "Theme and density, kept on this device.", BOTH],
  ["devices", "Devices", "devices", "account", "Every device signed in to your account.", BOTH],
  ["recovery", "Recovery", "health-cross", "account", "Your recovery kit and the ways back in.", DESKTOP],
  ["status", "Status", "security", "account", "What this device can prove about your account.", DESKTOP],
  ["server", "Server", "bare-metal-server", "account", "Where your vault syncs.", BOTH],
  ["import-export", "Import and export", "document--import", "vault", "Bring items in, or take all of them out.", DESKTOP],
  ["backups", "Backups", "data-backup", "vault", "Encrypted copies of your vault, made on a schedule.", DESKTOP],
  ["aliases", "Email aliases", "email", "vault", "The service that makes an address for each login.", DESKTOP],
  ["codes", "Codes", "time", "vault", "The clock one-time codes are worked out from.", DESKTOP],
  ["security-checks", "Security checks", "security", "vault", "What Health checks, and with whom.", DESKTOP],
  ["about", "About", "information", "lockdown", "The version, updates and licenses.", DESKTOP],
]

/** Every place with a fixed address. A vault, a folder, a tag and a shared vault's members are made by `placeFor`. */
export const PLACES: readonly Place[] = [
  { id: "search", label: "Search", icon: "search", group: "search", surfaces: BOTH, crumbs: ["Search"], key: "/", nav: { area: "search", view: "all" } },
  view("all", "items"),
  ...MAIN_VIEWS.map((v) => view(v, "items")),
  ...MORE_VIEWS.map((v) => view(v, "more")),
  ...ASIDE_VIEWS.map((v) => view(v, "aside")),
  { id: "health", label: "Health", icon: "checkmark--outline", group: "tools", surfaces: BOTH, crumbs: ["Tools", "Health"], nav: { area: "security", view: "all" } },
  ...HEALTH_VIEWS.map((v) => view(v, "health")),
  { id: "sharing", label: "Sharing", icon: "share", group: "sharing", surfaces: BOTH, crumbs: ["Vaults", "Sharing"], nav: { area: "sharing", view: "all" } },
  ...SHARING_TABS.map(
    ([tab, label, icon, surfaces]): Place => ({ id: `sharing-${tab}`, label, icon, group: "sharing", surfaces, crumbs: ["Vaults", "Sharing", label], nav: { area: "sharing", view: "all", tab } }),
  ),
  { id: "settings", label: "Settings", icon: "settings", group: "settings", surfaces: BOTH, crumbs: ["Settings"], key: "⌘,", nav: { area: "settings", view: "all" } },
  ...SECTIONS.map(
    ([section, label, icon, settingsGroup, description, surfaces]): Place => ({
      id: `settings-${section}`,
      label,
      icon,
      group: "settings",
      surfaces,
      crumbs: ["Settings", label],
      nav: { area: "settings", view: "all", section },
      settingsGroup,
      description,
    }),
  ),
]

const BY_ID = new Map(PLACES.map((p) => [p.id, p]))

/** What this device is called in Settings (cross-app decisions §5): its real kind. */
export function deviceName(surface: Surface, mac: boolean): string {
  return surface === "web" ? "This browser" : mac ? "This Mac" : "This PC"
}

/** The places a build draws. */
export function placesFor(surface: Surface): Place[] {
  return PLACES.filter((p) => p.surfaces.includes(surface))
}

/** The Sharing tabs a build has, in order. */
export function sharingTabs(surface: Surface): SharingTab[] {
  return SHARING_TABS.filter(([, , , surfaces]) => surfaces.includes(surface)).map(([tab]) => tab)
}

/** The Settings sections a build has, in order. */
export function settingsSections(surface: Surface): Place[] {
  return placesFor(surface).filter((p) => p.settingsGroup !== undefined)
}

/** The ids of the places that are made from data. */
export const vaultPlace = (id: string) => `vault:${id}`
export const folderPlace = (id: string) => `folder:${id}`
export const tagPlace = (tag: string) => `tag:${tag}`
export const membersPlace = (vaultId: string) => `members:${vaultId}`

/** A fixed place by its id; undefined for an id the table doesn't hold. */
export function place(id: string): Place | undefined {
  return BY_ID.get(id)
}

/** Where a place id goes. A vault, a folder, a tag and a vault's members are read from the id itself. */
export function navFor(id: string): Nav | null {
  const fixed = BY_ID.get(id)
  if (fixed) return fixed.nav
  const at = id.indexOf(":")
  if (at < 0) return null
  const kind = id.slice(0, at)
  const value = id.slice(at + 1)
  if (!value) return null
  if (kind === "vault") return { area: "vault", view: "all", scope: { vault: value } }
  if (kind === "folder") return { area: "vault", view: "all", scope: { folder: value } }
  if (kind === "tag") return { area: "vault", view: "all", scope: { tag: value } }
  if (kind === "members") return { area: "sharing", view: "all", tab: "vaults", members: value }
  return null
}

const scoped = (nav: Nav) => Boolean(nav.scope && (nav.scope.vault || nav.scope.folder || nav.scope.tag))

/** The id of the place a `Nav` is: the deepest one. */
export function placeIdOf(nav: Nav): string {
  switch (nav.area) {
    case "search":
      return "search"
    case "codes":
      return "codes"
    case "security":
      return "health"
    case "sharing":
      return nav.members ? membersPlace(nav.members) : nav.tab ? `sharing-${nav.tab}` : "sharing"
    case "settings":
      return nav.section ? `settings-${nav.section}` : "settings"
    case "vault":
      if (nav.scope?.folder) return folderPlace(nav.scope.folder)
      if (nav.scope?.tag) return tagPlace(nav.scope.tag)
      if (nav.scope?.vault) return vaultPlace(nav.scope.vault)
      return nav.view
  }
}

/**
 * The one row that shows as current (brief 3.2): the deepest real place.
 * A view across all vaults is its own row; a vault, folder or tag is its row
 * and no view is; Health holds its two lists; Sharing holds its tabs and a
 * vault's members; Settings holds its sections.
 */
export function currentRow(nav: Nav): string {
  switch (nav.area) {
    case "sharing":
      return "sharing"
    case "settings":
      return "settings"
    case "vault":
      if (scoped(nav)) return placeIdOf(nav)
      return HEALTH_VIEWS.includes(nav.view) ? "health" : nav.view
    default:
      return placeIdOf(nav)
  }
}

/** The phone's tab for a place: Items, Codes or Health; none on Search, Sharing and Settings. */
export type Section = "items" | "codes" | "health"
export function sectionOf(nav: Nav): Section | null {
  if (nav.area === "codes") return "codes"
  if (nav.area === "security") return "health"
  if (nav.area === "vault") return !scoped(nav) && HEALTH_VIEWS.includes(nav.view) ? "health" : "items"
  return null
}

/** The three sections, as the tab bar and the rail draw them (Mobile / 01). */
export const SECTIONS_BAR: ReadonlyArray<{ id: Section; label: string; icon: IconName; place: string }> = [
  { id: "items", label: "Items", icon: "password", place: "all" },
  { id: "codes", label: "Codes", icon: "time", place: "codes" },
  { id: "health", label: "Health", icon: "checkmark--outline", place: "health" },
]

/** One part of the breadcrumb: its words, and the place it leads back to (none for the last). */
export interface Crumb {
  label: string
  place?: string
}

/** A folder's own name and its parents', outermost first. */
function folderTrail(vault: VaultView, id: string): Array<{ id: string; name: string }> {
  const out: Array<{ id: string; name: string }> = []
  const seen = new Set<string>()
  let at = vault.folders.find((f) => f.id === id)
  while (at && !seen.has(at.id)) {
    seen.add(at.id)
    out.unshift({ id: at.id, name: at.name })
    const parent = at.parent
    at = parent ? vault.folders.find((f) => f.id === parent) : undefined
  }
  return out
}

/**
 * The breadcrumb for where you are (brief 3.2), outermost first. Every part
 * but the last leads back to its place. `extra` is what the open screen adds
 * through `useCrumb` (an item opened in Health, a finding).
 */
export function crumbsOf(nav: Nav, vaults: VaultView[], surface: Surface, mac: boolean, extra: readonly string[] = []): Crumb[] {
  const out: Crumb[] = []
  if (nav.area === "vault" && scoped(nav)) {
    out.push({ label: "Vaults", place: "all" })
    if (nav.scope?.folder) {
      const vault = vaults.find((v) => v.folders.some((f) => f.id === nav.scope?.folder))
      if (vault) {
        out.push({ label: vault.name, place: vaultPlace(vault.id) })
        for (const f of folderTrail(vault, nav.scope.folder)) out.push({ label: f.name, place: folderPlace(f.id) })
      } else out.push({ label: "Folder" })
    } else if (nav.scope?.tag) {
      const parts = nav.scope.tag.split("/")
      parts.forEach((part, i) => out.push({ label: i === 0 ? `#${part}` : part, place: tagPlace(parts.slice(0, i + 1).join("/")) }))
    } else if (nav.scope?.vault) {
      out.push({ label: vaults.find((v) => v.id === nav.scope?.vault)?.name ?? "Vault", place: vaultPlace(nav.scope.vault) })
    }
    if (nav.view !== "all") out.push({ label: VIEW_LABELS[nav.view] })
  } else if (nav.area === "sharing" && nav.members) {
    out.push({ label: "Vaults", place: "all" }, { label: "Sharing", place: "sharing" }, { label: vaults.find((v) => v.id === nav.members)?.name ?? "Vault", place: membersPlace(nav.members) })
  } else {
    const p = BY_ID.get(placeIdOf(nav))
    const words = p ? [...p.crumbs] : ["Items"]
    if (p?.id === "settings-this-device") words[1] = deviceName(surface, mac)
    // The way back from each part: the group's first place, then the place's parent.
    const ROOT: Record<string, string> = { Items: "all", Tools: "health", Vaults: "all", Settings: "settings", Search: "search" }
    words.forEach((label, i) => {
      const parent = i === 0 ? ROOT[label] : i === 1 && words[0] === "Tools" ? "health" : i === 1 && label === "Sharing" ? "sharing" : undefined
      out.push({ label, place: i === words.length - 1 ? p?.id : parent })
    })
  }
  for (const label of extra) out.push({ label })
  // The last part is where you are, not a way back.
  const last = out[out.length - 1]
  if (last) out[out.length - 1] = { label: last.label }
  return out
}

/** What the shell gives every screen: where you are, and the way to any place. */
export interface PlacesValue {
  nav: Nav
  surface: Surface
  /** Goes to a place by its id. An id the table doesn't know does nothing. */
  go: (placeId: string) => void
  /** Goes to an exact `Nav` (a device brought into view, a view inside a vault). */
  navigate: (nav: Nav) => void
}

const PlacesContext = createContext<PlacesValue>({ nav: { area: "vault", view: "all" }, surface: "desktop", go: () => {}, navigate: () => {} })

export const PlacesProvider = PlacesContext.Provider

/** Where you are and the way to any place: `const { go } = usePlaces(); go("settings-devices")`. */
export function usePlaces(): PlacesValue {
  return useContext(PlacesContext)
}
