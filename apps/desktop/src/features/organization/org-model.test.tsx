import type { ItemSummary, VaultView } from "@/core/types"

import { expiryBucket, expiryWords, folderTree, inScope, parseTags, rankRows, scopeTitle, tagTree, tagWithin, useScore } from "./org-model"

const row = (id: string, over: Partial<ItemSummary> = {}): ItemSummary => ({
  id,
  kind: "login",
  state: "live",
  title: id,
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
  vault_id: "v1",
  folder: null,
  tags: [],
  ...over,
})

const VAULTS: VaultView[] = [
  {
    id: "v1",
    name: "Personal",
    primary: true,
    items: 3,
    trash_days: 30,
    folders: [
      { id: "f-work", name: "Work", parent: null, path: "Work" },
      { id: "f-clients", name: "Clients", parent: "f-work", path: "Work / Clients" },
      { id: "f-home", name: "Home", parent: null, path: "Home" },
    ],
  },
  { id: "v2", name: "Family", primary: false, items: 1, trash_days: 7, folders: [] },
]

describe("organization: scopes, trees and ranking", () => {
  const rows = [
    row("a", { folder: "f-work", tags: ["work"] }),
    row("b", { folder: "f-clients", tags: ["work/clients", "Daily"] }),
    row("c", { folder: "f-home" }),
    row("d", { vault_id: "v2", tags: ["family"] }),
  ]

  it("a folder holds the folders inside it; a tag holds the tags nested under it", () => {
    expect(inScope(rows, { folder: "f-work" }, VAULTS).map((r) => r.id)).toEqual(["a", "b"])
    expect(inScope(rows, { folder: "f-clients" }, VAULTS).map((r) => r.id)).toEqual(["b"])
    expect(inScope(rows, { tag: "work" }, VAULTS).map((r) => r.id)).toEqual(["a", "b"])
    expect(inScope(rows, { tag: "WORK/clients" }, VAULTS).map((r) => r.id)).toEqual(["b"])
    expect(inScope(rows, { vault: "v2" }, VAULTS).map((r) => r.id)).toEqual(["d"])
    expect(tagWithin("workshop", "work")).toBe(false)
    expect(scopeTitle({ folder: "f-clients" }, VAULTS)).toBe("Personal / Work / Clients")
    expect(scopeTitle({ tag: "work" }, VAULTS)).toBe("#work")
  })

  it("the tag tree lists parents even when no item carries them alone, with counts", () => {
    const tree = tagTree([...rows, row("e", { state: "archived", tags: ["work"] })])
    expect(tree.map((t) => [t.tag, t.depth, t.count])).toEqual([
      ["Daily", 0, 1],
      ["family", 0, 1],
      ["work", 0, 2],
      ["work/clients", 1, 1],
    ])
  })

  it("the folder tree puts children under their parent, alphabetically", () => {
    expect(folderTree(VAULTS[0]!.folders).map((f) => `${f.depth}:${f.name}`)).toEqual(["0:Home", "0:Work", "1:Clients"])
  })

  it("favourites rank first, then what this device used most lately, then A–Z", () => {
    const now = 100 * 86_400
    const ranked = rankRows(
      [
        row("zeta"),
        row("old", { uses: 30, last_used: now - 90 * 86_400 }),
        row("fresh", { uses: 3, last_used: now - 3600 }),
        row("star", { favourite: true }),
        row("alpha"),
      ],
      now,
    )
    expect(ranked.map((r) => r.id)).toEqual(["star", "fresh", "old", "alpha", "zeta"])
    expect(useScore(row("x"), now)).toBe(0)
  })

  it("expiry reads in sections and words", () => {
    expect([expiryBucket(-2), expiryBucket(0), expiryBucket(7), expiryBucket(20), expiryBucket(90), expiryBucket(null)]).toEqual([
      "lapsed",
      "week",
      "week",
      "month",
      "later",
      "later",
    ])
    expect(expiryWords(-3)).toBe("expired 3 days ago")
    expect(expiryWords(0)).toBe("expires today")
    expect(expiryWords(12)).toBe("expires in 12 days")
  })

  it("tags typed in one box split on commas and lines, trimmed, without repeats", () => {
    expect(parseTags(" work / clients, family\nFamily,, ")).toEqual(["work/clients", "family"])
  })
})
