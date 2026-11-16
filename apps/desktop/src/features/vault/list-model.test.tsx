import type { ItemSummary } from "@/core/types"

import type { Finding } from "@/core/types"
import { count } from "@/lib/format"

import {
  baseTitle,
  buildList,
  conflictPair,
  countViews,
  defaultGroup,
  findingWords,
  groupFor,
  groupRows,
  letterOf,
  methodWords,
  monoLine,
  rowWords,
  rowsIn,
  scopeChips,
  sortRows,
  viewOfKind,
  KIND_VIEWS,
  VIEW_LABELS,
  countTypes,
} from "./list-model"

function row(over: Partial<ItemSummary> & { id: string; title: string; kind: string }): ItemSummary {
  return {
    state: "live",
    site: "",
    username: "",
    edited_at: 0,
    crown_jewel: false,
    methods: [],
    passkeys: 0,
    codes: 0,
    expires_at: null,
    conflict_copy: false,
    warnings: [],
    views: [],
    subtitle: "",
    copy: null,
    ...over,
  }
}

const rows: ItemSummary[] = [
  row({ id: "1", title: "zeta mail", kind: "login", edited_at: 10, methods: ["password", "code"], codes: 1, views: ["codes"] }),
  row({ id: "2", title: "Alpha bank", kind: "login", edited_at: 30, methods: ["password", "passkey"], passkeys: 1, views: ["passkeys"] }),
  row({ id: "3", title: "Home Wi-Fi", kind: "wi-fi", edited_at: 20, views: ["wi-fi"] }),
  row({ id: "4", title: "Passport", kind: "document", edited_at: 5, expires_at: "2026-11-01", views: ["expiring", "attention"] }),
  row({ id: "5", title: "Old thing", kind: "note", edited_at: 40, state: "trashed", views: ["deleted"] }),
  row({ id: "6", title: "beta", kind: "note", edited_at: 1 }),
]

describe("the vault list (D93)", () => {
  it("defaults to title order, grouped by kind with Logins first and counts per group", () => {
    const groups = buildList(rows, "all", "", "title", "kind")
    expect(groups.map((g) => [g.label, g.rows.length])).toEqual([
      ["Logins", 2],
      ["Wi-Fi", 1],
      ["Documents", 1],
      ["Notes", 1],
    ])
    expect(groups[0]!.rows.map((r) => r.title)).toEqual(["Alpha bank", "zeta mail"])
    // A Login keeps its codes and passkeys inside its own row.
    expect(groups[0]!.rows[1]!.methods).toEqual(["password", "code"])
  })

  it("sorts by recent change when asked, and drops the groups when asked", () => {
    const flat = buildList(rows, "all", "", "changed", "none")
    expect(flat).toHaveLength(1)
    expect(flat[0]!.rows.map((r) => r.id)).toEqual(["2", "3", "1", "4", "6"])
    expect(sortRows(rows, "title").map((r) => r.title)).toEqual(["Alpha bank", "beta", "Home Wi-Fi", "Old thing", "Passport", "zeta mail"])
  })

  it("keeps trashed items out of All and in Deleted, and cuts smart views across kinds", () => {
    expect(rowsIn(rows, "all").map((r) => r.id)).not.toContain("5")
    expect(rowsIn(rows, "deleted").map((r) => r.id)).toEqual(["5"])
    expect(rowsIn(rows, "codes").map((r) => r.id)).toEqual(["1"])
    expect(rowsIn(rows, "attention").map((r) => r.id)).toEqual(["4"])
    const counts = countViews(rows)
    const { all, passkeys, codes, "wi-fi": wifi, expiring, attention, deleted, favourites, archived, contacts } = counts
    expect({ all, passkeys, codes, "wi-fi": wifi, expiring, attention, deleted, favourites, archived, contacts }).toEqual({ all: 5, passkeys: 1, codes: 1, "wi-fi": 1, expiring: 1, attention: 1, deleted: 1, favourites: 0, archived: 0, contacts: 0 })
  })

  it("has a view for every kind that is not already a smart view, counted from the live rows (doc 64)", () => {
    const counts = countViews(rows)
    for (const kind of KIND_VIEWS) {
      const live = rows.filter((r) => r.kind === kind && r.state !== "trashed" && r.state !== "archived")
      expect(rowsIn(rows, kind).map((r) => r.id)).toEqual(live.map((r) => r.id))
      expect(counts[kind]).toBe(live.length)
      expect(VIEW_LABELS[kind]).toBeTruthy()
    }
    // The kinds in the list, not counting what is trashed or archived.
    expect(countTypes(rows)).toBe(new Set(rowsIn(rows, "all").map((r) => r.kind)).size)
    // US spelling in the words people read; the ids keep the core's.
    expect(VIEW_LABELS.favourites).toBe("Favorites")
    expect(VIEW_LABELS.deleted).toBe("Recently deleted")
  })

  it("searches titles, sites and usernames inside the view", () => {
    const hit = buildList(rows, "all", "WI-FI", "title", "none")
    expect(hit[0]!.rows.map((r) => r.id)).toEqual(["3"])
    expect(groupRows([], "kind")).toEqual([])
  })
})

describe("letter groups and the sort and group menu (brief V1, D780)", () => {
  it("groups title order by first letter, accents folded, anything else under #, with each letter's count", () => {
    const named = ["Steam", "spotify", "Émile's bank", "Stripe", "42 club", "tailscale", "Ticketmaster"].map((title, i) => row({ id: String(i), title, kind: "login" }))
    const groups = buildList(named, "all", "", "title", "letter")
    expect(groups.map((g) => [g.label, g.rows.length])).toEqual([
      ["#", 1],
      ["E", 1],
      ["S", 3],
      ["T", 2],
    ])
    expect(groups.find((g) => g.label === "S")!.rows.map((r) => r.title)).toEqual(["spotify", "Steam", "Stripe"])
    // A header's count is written as every count is (decisions §13): `S // 03`.
    expect(count(groups.find((g) => g.label === "S")!.rows.length)).toBe("003")
    expect(letterOf("  émile")).toBe("E")
    expect(letterOf("")).toBe("#")
  })

  it("offers letters only in A to Z order, and starts each device on its drawn grouping", () => {
    expect(groupFor("title", "letter")).toBe("letter")
    expect(groupFor("changed", "letter")).toBe("none")
    expect(groupFor("used", "kind")).toBe("kind")
    // With another sort, the letter grouping falls to one list rather than letters out of order.
    expect(buildList(rows, "all", "", "changed", "letter")).toHaveLength(1)
    expect(defaultGroup(false)).toBe("letter")
    expect(defaultGroup(true)).toBe("none")
  })

  it("sorts by recent use on this device, newest first, never-used last in title order", () => {
    const used = [row({ id: "a", title: "Alpha", kind: "login", last_used: 50 }), row({ id: "b", title: "Bravo", kind: "login" }), row({ id: "c", title: "Charlie", kind: "login", last_used: 90 }), row({ id: "d", title: "Able", kind: "login" })]
    expect(sortRows(used, "used").map((r) => r.id)).toEqual(["c", "a", "d", "b"])
  })
})

describe("kind views and a scope's chips (brief X1, V1)", () => {
  it("opens each kind as its own view, or as the smart view that lists it", () => {
    expect(viewOfKind("card")).toBe("card")
    expect(viewOfKind("wi-fi")).toBe("wi-fi")
    expect(viewOfKind("passkey")).toBe("passkeys")
    expect(viewOfKind("code")).toBe("codes")
    expect(viewOfKind("contact")).toBeNull()
  })

  it("chips only the kinds a scope holds, live ones, in the sidebar's order", () => {
    const scoped = [row({ id: "1", title: "a", kind: "note" }), row({ id: "2", title: "b", kind: "login" }), row({ id: "3", title: "c", kind: "wi-fi" }), row({ id: "4", title: "d", kind: "card", state: "trashed" })]
    expect(scopeChips(scoped)).toEqual(["wi-fi", "login", "note"])
    expect(scopeChips([])).toEqual([])
  })
})

describe("what a row says (brief V1)", () => {
  const login = row({ id: "s", title: "Steam", kind: "login", site: "https://store.steam.example/login", methods: ["password", "passkey", "code"], vault_id: "v1" })

  it("says a login's ways in as the frames do", () => {
    expect(methodWords(login)).toBe("Passkey + code")
    expect(methodWords({ ...login, methods: ["passkey"] })).toBe("Passkey")
    expect(methodWords({ ...login, methods: ["password"] })).toBe("Login")
    expect(methodWords({ ...login, methods: ["password", "code"] })).toBe("Login + code")
    expect(methodWords({ ...login, methods: [] })).toBe("Login")
  })

  it("puts the site, a note's edit, or the template's plain field on the mono line, and nothing for a crown jewel (D113)", () => {
    expect(monoLine(login)).toBe("store.steam.example")
    const now = new Date(2026, 10, 15, 12).getTime() / 1000
    const edited = new Date(2026, 10, 8, 9).getTime() / 1000
    expect(monoLine(row({ id: "n", title: "Door", kind: "note", edited_at: edited }), now)).toBe("edited NOV 08")
    expect(monoLine(row({ id: "w", title: "Home", kind: "wi-fi", subtitle: "Rivera-Home" }))).toBe("Rivera-Home")
    expect(monoLine(row({ id: "j", title: "Cold wallet", kind: "crypto-wallet", crown_jewel: true, subtitle: "" }))).toBe("")
  })

  it("names the kind, or the vault in a kind view across vaults, then the state words", () => {
    const vaults = [
      { id: "v1", name: "Personal", primary: true, items: 1, trash_days: 30, folders: [] },
      { id: "v2", name: "Family", primary: false, items: 1, trash_days: 7, folders: [] },
    ]
    const card = row({ id: "c", title: "Everyday", kind: "card", vault_id: "v2" })
    expect(rowWords(card, { view: "all", vaults })).toBe("Card")
    expect(rowWords(card, { view: "card", vaults })).toBe("Family")
    // Inside one vault the vault goes without saying.
    expect(rowWords(card, { view: "card", vaults, oneVault: true })).toBe("Card")
    expect(rowWords(login, { view: "all", vaults })).toBe("Passkey + code")
    expect(rowWords({ ...login, conflict_copy: true, access: "view" }, { view: "all" })).toBe("Passkey + code // conflict // shared")
    expect(rowWords({ ...card, state: "archived" }, { view: "favourites" })).toBe("Card // archived")
    // A state the view already names is said once.
    expect(rowWords({ ...card, state: "archived" }, { view: "archived" })).toBe("Card")
    expect(rowWords({ ...card, state: "trashed" }, { view: "deleted" })).toBe("Card")
  })

  it("adds the worst finding of the cached report, and none that was dismissed", () => {
    const f = (kind: Finding["kind"], item: string, dismissed = false) => ({ kind, item_id: item, dismissed: dismissed ? { at: 1 } : null }) as unknown as Finding
    const words = findingWords([f("weak", "s"), f("breached", "s"), f("reused", "x"), f("weak", "y", true), f("missing_2fa", "z")])
    expect(words.get("s")).toBe("breached")
    expect(words.get("x")).toBe("reused")
    expect(words.has("y")).toBe(false)
    expect(words.has("z")).toBe(false)
    expect(rowWords(login, { view: "all", findings: words })).toBe("Passkey + code // breached")
    expect(findingWords(null).size).toBe(0)
  })
})

describe("conflict pairs (D58)", () => {
  const bank = row({ id: "b", title: "Bank", kind: "login", edited_at: 10 })
  const copy1 = row({ id: "c1", title: "Bank (conflict copy from laptop, 2026-10-16)", kind: "login", conflict_copy: true, edited_at: 20 })
  const copy2 = row({ id: "c2", title: "Bank (conflict copy from phone, 2026-10-17)", kind: "login", conflict_copy: true, edited_at: 30 })
  const trashedCopy = row({ id: "c3", title: "Bank (conflict copy from tv, 2026-10-17)", kind: "login", conflict_copy: true, state: "trashed", edited_at: 40 })
  const other = row({ id: "o", title: "Bank", kind: "note" })

  it("strips the marker to the shared title", () => {
    expect(baseTitle(copy1.title)).toBe("Bank")
    expect(baseTitle("Bank")).toBe("Bank")
  })

  it("pairs a copy with its original and an original with its newest live copy, never across kinds or the trash", () => {
    const all = [bank, copy1, copy2, trashedCopy, other]
    expect(conflictPair(all, "c1")?.id).toBe("b")
    expect(conflictPair(all, "b")?.id).toBe("c2")
    expect(conflictPair(all, "o")).toBeNull()
    expect(conflictPair([copy1], "c1")).toBeNull()
    expect(conflictPair(all, "missing")).toBeNull()
  })
})
