import type { VaultView } from "@/core/types"
import { KIND_VIEWS, VIEW_LABELS, type ListView } from "@/features/vault/list-model"

import {
  ASIDE_VIEWS,
  HEALTH_VIEWS,
  MAIN_VIEWS,
  MORE_VIEWS,
  PLACES,
  SECTIONS_BAR,
  crumbsOf,
  currentRow,
  deviceName,
  folderPlace,
  membersPlace,
  navFor,
  place,
  placeIdOf,
  placesFor,
  sectionOf,
  settingsSections,
  sharingTabs,
  tagPlace,
  vaultPlace,
  type Area,
  type Nav,
} from "./places"

const VAULTS: VaultView[] = [
  {
    id: "v1",
    name: "Personal",
    primary: true,
    items: 2,
    trash_days: 30,
    folders: [
      { id: "f-money", name: "Finance", parent: null, path: "Finance" },
      { id: "f-tax", name: "Taxes", parent: "f-money", path: "Finance / Taxes" },
    ],
  },
  { id: "v2", name: "Family", primary: false, items: 1, trash_days: 7, folders: [], sharing: { kind: "family", right: "edit", owned: true, owner: null, people: 3 } as VaultView["sharing"] },
]

const words = (nav: Nav, extra: string[] = []) => crumbsOf(nav, VAULTS, "desktop", true, extra).map((c) => c.label)
const at = (id: string): Nav => {
  const nav = navFor(id)
  if (!nav) throw new Error(`no place ${id}`)
  return nav
}

describe("the table of places (doc 64)", () => {
  it("names every place once, with a label, an icon and a breadcrumb", () => {
    const ids = PLACES.map((p) => p.id)
    expect(new Set(ids).size).toBe(ids.length)
    for (const p of PLACES) {
      expect(p.label).not.toBe("")
      expect(p.icon).not.toBe("")
      expect(p.crumbs.length).toBeGreaterThan(0)
      expect(p.surfaces.length).toBeGreaterThan(0)
      // Names use "and", never "&" (cross-app decisions §5).
      expect(p.label).not.toContain("&")
    }
  })

  it("has a place for every area, every view and every kind", () => {
    const areas = new Set(PLACES.map((p) => p.nav.area))
    expect([...areas].sort()).toEqual((["codes", "search", "security", "settings", "sharing", "vault"] satisfies Area[]).sort())
    // Every value of ListView is a place. Codes is its own screen.
    for (const view of Object.keys(VIEW_LABELS) as ListView[]) {
      const p = place(view)
      expect(p, view).toBeDefined()
      expect(p!.nav).toEqual(view === "codes" ? { area: "codes", view: "codes" } : { area: "vault", view })
    }
    // Each kind is in exactly one of the sidebar's lists: the six, More, or a smart view of its own.
    for (const kind of KIND_VIEWS) expect([...MAIN_VIEWS, ...MORE_VIEWS].filter((v) => v === kind)).toHaveLength(1)
    expect(MAIN_VIEWS).toEqual(["favourites", "login", "passkeys", "codes", "card", "note"])
    expect(ASIDE_VIEWS).toEqual(["archived", "deleted"])
    expect(HEALTH_VIEWS).toEqual(["expiring", "attention"])
  })

  it("goes to every place by its id, and reads the id back from where it went", () => {
    for (const p of PLACES) {
      expect(navFor(p.id), p.id).toEqual(p.nav)
      expect(placeIdOf(p.nav), p.id).toBe(p.id)
    }
    // The places made from data.
    expect(at(vaultPlace("v2"))).toEqual({ area: "vault", view: "all", scope: { vault: "v2" } })
    expect(at(folderPlace("f-tax"))).toEqual({ area: "vault", view: "all", scope: { folder: "f-tax" } })
    expect(at(tagPlace("work/clients"))).toEqual({ area: "vault", view: "all", scope: { tag: "work/clients" } })
    expect(at(membersPlace("v2"))).toEqual({ area: "sharing", view: "all", tab: "vaults", members: "v2" })
    for (const id of [vaultPlace("v2"), folderPlace("f-tax"), tagPlace("work/clients"), membersPlace("v2")]) expect(placeIdOf(at(id))).toBe(id)
    // An id the table doesn't know goes nowhere.
    expect(navFor("nowhere")).toBeNull()
    expect(navFor("vault:")).toBeNull()
  })

  it("moves Devices, Recovery and Status into Settings, and People into Sharing (D721, D722)", () => {
    expect(at("settings-devices")).toEqual({ area: "settings", view: "all", section: "devices" })
    expect(at("settings-recovery").section).toBe("recovery")
    expect(at("settings-status").section).toBe("status")
    expect(at("sharing-people")).toEqual({ area: "sharing", view: "all", tab: "people" })
    expect(place("health")!.label).toBe("Health")
    expect(place("health")!.nav.area).toBe("security")
  })

  it("lists 14 Settings sections in four groups on the desktop and four sections in the web vault", () => {
    const desktop = settingsSections("desktop")
    expect(desktop.map((p) => p.label)).toEqual(["This device", "Browsers", "Quick access", "Appearance", "Devices", "Recovery", "Status", "Server", "Import and export", "Backups", "Email aliases", "Codes", "Security checks", "About"])
    expect(desktop.map((p) => p.settingsGroup)).toEqual(["device", "device", "device", "device", "account", "account", "account", "account", "vault", "vault", "vault", "vault", "vault", "lockdown"])
    expect(settingsSections("web").map((p) => p.nav.section)).toEqual(["this-device", "appearance", "devices", "server"])
    expect([deviceName("desktop", true), deviceName("desktop", false), deviceName("web", true)]).toEqual(["This Mac", "This PC", "This browser"])
  })

  it("gives the web vault its own places: People and Links in Sharing, no Recovery or Status", () => {
    expect(sharingTabs("desktop")).toEqual(["people", "vaults", "links", "emergency"])
    expect(sharingTabs("web")).toEqual(["people", "links"])
    const web = placesFor("web").map((p) => p.id)
    for (const id of ["all", "codes", "health", "search", "sharing", "sharing-people", "sharing-links", "settings", "settings-devices"]) expect(web).toContain(id)
    for (const id of ["sharing-vaults", "sharing-emergency", "settings-recovery", "settings-status", "settings-backups", "settings-about"]) expect(web).not.toContain(id)
  })
})

describe("the one current row (brief 3.2)", () => {
  it("is the view's own row across all vaults", () => {
    for (const v of ["all", ...MAIN_VIEWS, ...MORE_VIEWS, ...ASIDE_VIEWS]) expect(currentRow(at(v))).toBe(v)
  })

  it("is the vault's, folder's or tag's row, and no view's, inside one", () => {
    expect(currentRow(at(vaultPlace("v2")))).toBe("vault:v2")
    expect(currentRow({ area: "vault", view: "card", scope: { vault: "v2" } })).toBe("vault:v2")
    expect(currentRow(at(folderPlace("f-tax")))).toBe("folder:f-tax")
    expect(currentRow(at(tagPlace("work")))).toBe("tag:work")
  })

  it("is Health for Health and the two lists it holds; Sharing for its tabs and a vault's members; Settings for every section", () => {
    for (const id of ["health", "expiring", "attention"]) expect(currentRow(at(id))).toBe("health")
    for (const id of ["sharing", "sharing-people", "sharing-vaults", "sharing-links", "sharing-emergency", membersPlace("v2")]) expect(currentRow(at(id))).toBe("sharing")
    for (const p of settingsSections("desktop")) expect(currentRow(p.nav)).toBe("settings")
    expect(currentRow(at("settings"))).toBe("settings")
    expect(currentRow(at("search"))).toBe("search")
    expect(currentRow(at("codes"))).toBe("codes")
  })

  it("puts each place under one of the three sections, or none", () => {
    expect(SECTIONS_BAR.map((s) => [s.id, s.label, s.icon])).toEqual([
      ["items", "Items", "password"],
      ["codes", "Codes", "time"],
      ["health", "Health", "checkmark--outline"],
    ])
    expect(sectionOf(at("all"))).toBe("items")
    expect(sectionOf(at("deleted"))).toBe("items")
    expect(sectionOf(at(vaultPlace("v1")))).toBe("items")
    expect(sectionOf(at("codes"))).toBe("codes")
    expect(sectionOf(at("health"))).toBe("health")
    expect(sectionOf(at("expiring"))).toBe("health")
    // Active state must be true: on a place that is no section, no tab shows active.
    for (const id of ["search", "sharing", "sharing-links", "settings", "settings-about", membersPlace("v2")]) expect(sectionOf(at(id))).toBeNull()
  })
})

describe("the breadcrumb (brief 3.2)", () => {
  it("names a view, a vault, a folder inside folders, and a tag", () => {
    expect(words(at("all"))).toEqual(["Items", "All"])
    expect(words(at("card"))).toEqual(["Items", "Cards"])
    expect(words(at("codes"))).toEqual(["Items", "Codes"])
    expect(words(at(vaultPlace("v2")))).toEqual(["Vaults", "Family"])
    expect(words(at(folderPlace("f-tax")))).toEqual(["Vaults", "Personal", "Finance", "Taxes"])
    expect(words(at(tagPlace("work/clients")))).toEqual(["Vaults", "#work", "clients"])
    expect(words({ area: "vault", view: "card", scope: { vault: "v2" } })).toEqual(["Vaults", "Family", "Cards"])
  })

  it("names Health, Sharing, Search and Settings, with the device's real kind", () => {
    expect(words(at("health"))).toEqual(["Tools", "Health"])
    expect(words(at("attention"))).toEqual(["Tools", "Health", "Needs attention"])
    expect(words(at("sharing-people"))).toEqual(["Vaults", "Sharing", "People"])
    expect(words(at(membersPlace("v2")))).toEqual(["Vaults", "Sharing", "Family"])
    expect(words(at("search"))).toEqual(["Search"])
    expect(words(at("settings-backups"))).toEqual(["Settings", "Backups"])
    expect(words(at("settings-this-device"))).toEqual(["Settings", "This Mac"])
    expect(crumbsOf(at("settings-this-device"), VAULTS, "web", false).map((c) => c.label)).toEqual(["Settings", "This browser"])
  })

  it("takes what the open screen adds, and makes every part but the last a way back", () => {
    expect(words(at("health"), ["Ticketmaster"])).toEqual(["Tools", "Health", "Ticketmaster"])
    const crumbs = crumbsOf(at(folderPlace("f-tax")), VAULTS, "desktop", true)
    expect(crumbs.map((c) => c.place)).toEqual(["all", "vault:v1", "folder:f-money", undefined])
    // With something added, the place itself becomes a way back.
    const deeper = crumbsOf(at("health"), VAULTS, "desktop", true, ["Ticketmaster"])
    expect(deeper.map((c) => c.place)).toEqual(["health", "health", undefined])
    const tab = crumbsOf(at("sharing-links"), VAULTS, "desktop", true)
    expect(tab.map((c) => c.place)).toEqual(["all", "sharing", undefined])
    // Every way back is a real place.
    for (const p of PLACES) for (const c of crumbsOf(p.nav, VAULTS, "desktop", true, ["x"])) if (c.place) expect(navFor(c.place), `${p.id} → ${c.place}`).not.toBeNull()
  })
})
