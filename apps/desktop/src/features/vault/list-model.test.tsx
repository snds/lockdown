import type { ItemSummary } from "@/core/types"

import { baseTitle, buildList, conflictPair, countViews, groupRows, rowsIn, sortRows } from "./list-model"

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
    expect(countViews(rows)).toEqual({ all: 5, passkeys: 1, codes: 1, "wi-fi": 1, expiring: 1, attention: 1, deleted: 1, favourites: 0, archived: 0, contacts: 0 })
  })

  it("searches titles, sites and usernames inside the view", () => {
    const hit = buildList(rows, "all", "WI-FI", "title", "none")
    expect(hit[0]!.rows.map((r) => r.id)).toEqual(["3"])
    expect(groupRows([], "kind")).toEqual([])
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
