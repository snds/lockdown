import { act, render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useState } from "react"
import { afterEach, beforeEach, describe, expect, it } from "vitest"

import { TooltipProvider } from "@unicron/ui"

import { GateProvider } from "@/app/gate"
import { placeIdOf, type Nav } from "@/app/places"
import { SettingsProvider } from "@/app/settings"
import { AppShell } from "@/app/shell"
import { StatusProvider, type LiveStatus } from "@/app/status"
import { TemplatesProvider } from "@/app/templates"
import { ClientProvider } from "@/core/client"
import type { ItemSummary, VaultView as VaultInfo } from "@/core/types"
import { VaultsProvider } from "@/features/organization/vaults-context"
import { SharingClientProvider } from "@/features/sharing/sharing-client"
import { fakeClient } from "@/test/fake-client"
import { fakeSharing } from "@/test/fake-sharing"
import { TEMPLATES } from "@/test/templates-fixture"

import { groupByVault, narrowOf, recentlyUsed, searchLabel } from "./search-view"
import { SearchView } from "./search-view"
import { clearTyped, searchWith } from "./search-words"
import { VaultView } from "./vault-view"

// Search as a place (brief X4; Desktop / 11, Mobile / 14 and 15), in the real shell beside the real
// list, as the app mounts them: the field, its label, the chips, the groups, Escape, RECENTLY USED.

const TWO: VaultInfo[] = [
  { id: "v1", name: "Personal", primary: true, items: 3, trash_days: 30, folders: [] },
  { id: "v2", name: "Family", primary: false, items: 2, trash_days: 30, folders: [] },
]

const row = (id: string, over: Partial<ItemSummary> = {}): ItemSummary => ({
  id,
  kind: "login",
  state: "live",
  title: id,
  site: "",
  username: "",
  edited_at: 0,
  crown_jewel: false,
  methods: ["password"],
  passkeys: 0,
  codes: 0,
  expires_at: null,
  conflict_copy: false,
  warnings: [],
  views: [],
  subtitle: "",
  copy: { field_id: "pw", how: "secret", label: "password" },
  vault_id: "v1",
  folder: null,
  tags: [],
  favourite: false,
  ...over,
})

const ROWS: ItemSummary[] = [
  row("GitHub", { site: "https://github.com", username: "kestrel04", subtitle: "kestrel04", last_used: 300 }),
  row("GitLab", { site: "https://gitlab.com", username: "kestrel.04", subtitle: "kestrel.04", last_used: 100 }),
  row("Gitea at home", { site: "https://git.home.example", vault_id: "v2" }),
  row("Git seed", { kind: "crypto-wallet", crown_jewel: true, copy: null, vault_id: "v2", last_used: 200 }),
  row("Old git", { state: "archived", views: ["archived"] }),
]

const LIVE: LiveStatus = { idleLeft: null, onStay: () => {}, work: null }

function setWindow(width: number, height: number) {
  Object.defineProperty(window, "innerWidth", { configurable: true, value: width })
  Object.defineProperty(window, "innerHeight", { configurable: true, value: height })
  act(() => {
    window.dispatchEvent(new Event("resize"))
  })
}

function Harness({ rows, start }: { rows: ItemSummary[]; start: Nav }) {
  const [nav, setNav] = useState<Nav>(start)
  const [selected, setSelected] = useState<string | null>(null)
  return (
    <>
      <output data-testid="place">{placeIdOf(nav)}</output>
      <AppShell nav={nav} onNavigate={setNav} rows={rows} onLock={() => {}} onSaved={() => {}}>
        {nav.area === "vault" && <VaultView view={nav.view} rows={rows} error={null} onChanged={() => {}} onNotice={() => {}} selected={selected} onSelect={setSelected} onPalette={() => {}} scope={nav.scope} />}
        {nav.area === "search" && <SearchView rows={rows} selected={selected} onSelect={setSelected} onChanged={() => {}} onNotice={() => {}} />}
        {nav.area !== "vault" && nav.area !== "search" && <p>elsewhere</p>}
      </AppShell>
    </>
  )
}

function mount({ rows = ROWS, vaults = TWO, start = { area: "search", view: "all" } as Nav } = {}) {
  render(
    <ClientProvider value={fakeClient({ templates: () => Promise.resolve(TEMPLATES), userTemplates: () => Promise.resolve([]), securityReport: () => Promise.reject({ code: "refused", message: "no report here" }) })}>
      <TooltipProvider>
        <SettingsProvider>
          <TemplatesProvider>
            <GateProvider>
              <SharingClientProvider value={fakeSharing({})}>
                <VaultsProvider value={{ vaults, reload: () => {} }}>
                  <StatusProvider live={LIVE}>
                    <Harness rows={rows} start={start} />
                  </StatusProvider>
                </VaultsProvider>
              </SharingClientProvider>
            </GateProvider>
          </TemplatesProvider>
        </SettingsProvider>
      </TooltipProvider>
    </ClientProvider>,
  )
}

const where = () => screen.getByTestId("place").textContent
const field = () => screen.getByRole("searchbox", { name: /^Search (your vault|Personal and Family|\d+ vaults)/ })
const section = (name: RegExp) => screen.getByRole("region", { name })
/** The item rows inside `el` (the sidebar's rows are list items too). */
const itemRows = (el: HTMLElement = document.body) => within(el).queryAllByRole("listitem").filter((li) => li.getAttribute("data-slot") === "list-row" || li.querySelector('[data-slot="list-row"]'))
/** A row's title as written, without its marks' words. */
const titleOf = (li: HTMLElement) => {
  const title = li.querySelector('[data-slot="list-row-title"]')!.cloneNode(true) as HTMLElement
  title.querySelector('[data-slot="list-row-marks"]')?.remove()
  return title.textContent
}
const titles = (el?: HTMLElement) => itemRows(el).map(titleOf)

beforeEach(() => {
  window.localStorage.clear()
  setWindow(1440, 900)
})
afterEach(() => {
  clearTyped()
  setWindow(1024, 768)
})

describe("the words Search is made of", () => {
  it("names the field for the vaults it looks in", () => {
    expect(searchLabel(["Personal"])).toBe("Search your vault")
    expect(searchLabel(["Personal", "Family"])).toBe("Search Personal and Family")
    expect(searchLabel(["Personal", "Family", "Work"])).toBe("Search Personal, Family and Work")
    expect(searchLabel(["A", "B", "C", "D", "E", "F", "G"])).toBe("Search 7 vaults")
  })

  it("keeps recently used to five, newest first, and nothing never used", () => {
    const used = Array.from({ length: 7 }, (_, i) => row(`r${i}`, { last_used: i + 1 }))
    expect(recentlyUsed([...used, row("never")]).map((r) => r.id)).toEqual(["r6", "r5", "r4", "r3", "r2"])
    expect(recentlyUsed([row("never")])).toEqual([])
  })

  it("groups by vault in the vaults' order, a vault the app doesn't list last", () => {
    const groups = groupByVault([row("a", { vault_id: "v2" }), row("b"), row("c", { vault_id: "gone" })], TWO)
    expect(groups.map((g) => [g.vault?.name ?? null, g.rows.map((r) => r.id)])).toEqual([
      ["Personal", ["b"]],
      ["Family", ["a"]],
      [null, ["c"]],
    ])
  })

  it("offers a chip for a narrower list only, Archived and Recently deleted chosen", () => {
    expect(narrowOf({ area: "vault", view: "all" }, TWO)).toBeNull()
    expect(narrowOf({ area: "codes", view: "codes" }, TWO)).toBeNull()
    expect(narrowOf({ area: "vault", view: "all", scope: { vault: "v2" } }, TWO)).toMatchObject({ label: "Family", chosen: false })
    expect(narrowOf({ area: "vault", view: "all", scope: { tag: "work" } }, TWO)).toMatchObject({ label: "#work", chosen: false })
    expect(narrowOf({ area: "vault", view: "card" }, TWO)).toMatchObject({ label: "Cards", chosen: false })
    expect(narrowOf({ area: "vault", view: "archived" }, TWO)).toMatchObject({ label: "Archived", chosen: true })
    expect(narrowOf({ area: "vault", view: "deleted" }, TWO)).toMatchObject({ label: "Recently deleted", chosen: true })
  })
})

describe("Search (brief X4)", () => {
  it("focuses its field, named for the vaults, and shows RECENTLY USED before typing", async () => {
    mount()
    expect(await screen.findByRole("searchbox", { name: "Search Personal and Family" })).toHaveFocus()
    const recent = section(/^Recently used/)
    expect(within(recent).getByRole("heading", { name: /Recently used/ })).toBeInTheDocument()
    // Newest first; the crown jewel by its title alone (D113), the archived item never.
    expect(titles(recent)).toEqual(["GitHub", "Git seed", "GitLab"])
    const seed = itemRows(recent)[1]!
    expect(seed.querySelector('[data-slot="list-row-meta"]')).toBeNull()
  })

  it("shows nothing before typing when nothing was used on this device", async () => {
    mount({ rows: ROWS.map((r) => ({ ...r, last_used: null })) })
    await screen.findByRole("searchbox", { name: "Search Personal and Family" })
    expect(screen.queryByRole("region", { name: /^Recently used/ })).not.toBeInTheDocument()
    expect(itemRows()).toHaveLength(0)
  })

  it("groups results by vault with counts, marks the words, a crown jewel's in its title only, and says how many", async () => {
    mount()
    const user = userEvent.setup()
    await user.type(await screen.findByRole("searchbox", { name: "Search Personal and Family" }), "git")
    const personal = section(/^Personal, 2 results/)
    const family = section(/^Family, 2 results/)
    expect(titles(personal)).toEqual(["GitHub", "GitLab"])
    expect(titles(family)).toEqual(["Git seed", "Gitea at home"])
    expect(within(personal).getByRole("heading", { name: /Personal/ })).toHaveTextContent(/02/)
    // Archived items stay out of Search (D149).
    expect(screen.queryByText("Old git")).not.toBeInTheDocument()
    const github = itemRows(personal)[0]!
    expect(Array.from(github.querySelectorAll("mark")).map((m) => m.textContent)).toEqual(["Git", "git"])
    const seed = itemRows(family)[0]!
    expect(Array.from(seed.querySelectorAll("mark")).map((m) => m.textContent)).toEqual(["Git"])
    expect(seed.querySelector('[data-slot="list-row-meta"]')).toBeNull()
    // The status line: 04 RESULTS (the line sets the capitals).
    expect(await screen.findByText(/^04 results$/i)).toBeInTheDocument()
  })

  it("draws no vault headers with one vault", async () => {
    mount({ vaults: [TWO[0]!], rows: ROWS.filter((r) => r.vault_id === "v1") })
    const user = userEvent.setup()
    await user.type(await screen.findByRole("searchbox", { name: "Search your vault" }), "git")
    await waitFor(() => expect(itemRows()).toHaveLength(2))
    expect(screen.queryByRole("heading", { name: /^Personal/ })).not.toBeInTheDocument()
  })

  it("says SEARCH // 00 when nothing matches, and NEW LOGIN starts a login with the words as its website", async () => {
    mount()
    const user = userEvent.setup()
    await user.type(await screen.findByRole("searchbox", { name: "Search Personal and Family" }), "netflix")
    expect(await screen.findByText("No item for netflix.")).toBeInTheDocument()
    expect(screen.getByText(/^Search \/\/ 00$/i)).toBeInTheDocument()
    expect(screen.getByText(/^Nothing in Personal or Family matches\./)).toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: "New login" }))
    const editor = await screen.findByRole("dialog")
    expect(within(editor).getByDisplayValue("netflix")).toBeInTheDocument()
  })

  it("Escape clears the field first, then leaves for where the person was", async () => {
    mount({ start: { area: "vault", view: "all", scope: { vault: "v2" } } })
    const user = userEvent.setup()
    await screen.findByRole("heading", { name: "Family", level: 1 })
    await user.keyboard("/")
    await waitFor(() => expect(where()).toBe("search"))
    await user.type(field(), "git")
    await user.keyboard("{Escape}")
    expect(field()).toHaveValue("")
    expect(where()).toBe("search")
    await user.keyboard("{Escape}")
    await waitFor(() => expect(where()).toBe("vault:v2"))
  })

  it("opened from a vault, offers EVERYWHERE and that vault; the vault narrows it, and the crumb says so", async () => {
    mount({ start: { area: "vault", view: "all", scope: { vault: "v2" } } })
    const user = userEvent.setup()
    await screen.findByRole("heading", { name: "Family", level: 1 })
    await user.keyboard("/")
    const chips = await screen.findByRole("radiogroup", { name: "Search in" })
    const everywhere = within(chips).getByRole("radio", { name: /Everywhere/ })
    const family = within(chips).getByRole("radio", { name: /Family/ })
    expect(everywhere).toHaveAttribute("aria-checked", "true")
    await user.type(field(), "git")
    await waitFor(() => expect(itemRows()).toHaveLength(4))
    await user.click(family)
    expect(family).toHaveAttribute("aria-checked", "true")
    await waitFor(() => expect(itemRows()).toHaveLength(2))
    // One vault chosen: no vault headers; the breadcrumb reads SEARCH // FAMILY.
    expect(screen.queryByRole("region", { name: /^Personal/ })).not.toBeInTheDocument()
    await waitFor(() => expect(screen.getByRole("navigation", { name: /breadcrumb/i })).toHaveTextContent(/Search.*Family/i))
  })

  it("opened from Archived, starts on ARCHIVED, which Search otherwise leaves out", async () => {
    mount({ start: { area: "vault", view: "archived" } })
    const user = userEvent.setup()
    await screen.findByRole("heading", { name: "Archived", level: 1 })
    await user.keyboard("/")
    const chips = await screen.findByRole("radiogroup", { name: "Search in" })
    expect(within(chips).getByRole("radio", { name: /Archived/ })).toHaveAttribute("aria-checked", "true")
    await user.type(field(), "git")
    await waitFor(() => expect(titles()).toEqual(["Old git"]))
  })

  it("opened from All items or another place, offers no chips", async () => {
    mount({ start: { area: "vault", view: "all" } })
    const user = userEvent.setup()
    await screen.findByRole("searchbox", { name: "Search" })
    await user.keyboard("/")
    await waitFor(() => expect(where()).toBe("search"))
    expect(screen.queryByRole("radiogroup", { name: "Search in" })).not.toBeInTheDocument()
  })

  it("takes words handed to it while it shows (Quick access's last row)", async () => {
    mount()
    const field = await screen.findByRole("searchbox", { name: "Search Personal and Family" })
    act(() => searchWith("gitea"))
    expect(field).toHaveValue("gitea")
    await waitFor(() => expect(itemRows()).toHaveLength(1))
  })

  it("opens a result beside the list: the row is chosen", async () => {
    mount()
    const user = userEvent.setup()
    await user.type(await screen.findByRole("searchbox", { name: "Search Personal and Family" }), "gitlab")
    await user.click(await screen.findByRole("button", { name: /^GitLab/ }))
    await waitFor(() => expect(itemRows()[0]).toHaveAttribute("data-selected"))
  })
})
