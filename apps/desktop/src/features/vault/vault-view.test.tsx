import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { TooltipProvider } from "@unicron/ui"

import { GateProvider } from "@/app/gate"
import { PlacesProvider, type Nav, type PlacesValue } from "@/app/places"
import { SettingsProvider } from "@/app/settings"
import { TemplatesProvider } from "@/app/templates"
import { ClientProvider, type Client } from "@/core/client"
import type { Finding, ItemSummary, SecurityReport, VaultView as VaultInfo } from "@/core/types"
import type { Scope } from "@/features/organization/org-model"
import { VaultsProvider } from "@/features/organization/vaults-context"
import { repeatedLandmarks } from "@/test/a11y"
import { fakeClient } from "@/test/fake-client"
import { TEMPLATES } from "@/test/templates-fixture"

import type { ListView } from "./list-model"
import { VaultView, WINDOW_FROM } from "./vault-view"

const row = (id: string, kind: string, title: string, over: Partial<ItemSummary> = {}): ItemSummary => ({
  id,
  kind,
  state: "live",
  title,
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
  favourite: false,
  ...over,
})

const VAULTS: VaultInfo[] = [
  { id: "v1", name: "Personal", primary: true, items: 1, trash_days: 30, folders: [] },
  { id: "v2", name: "Family", primary: false, items: 1, trash_days: 7, folders: [] },
]

interface Mount {
  rows: ItemSummary[] | null
  error?: string | null
  view?: ListView
  scope?: Scope
  vaults?: VaultInfo[]
  client?: Partial<Client>
  onChanged?: () => void
  places?: Partial<PlacesValue>
}

function mount({ rows, error = null, view = "all", scope, vaults = VAULTS, client = {}, onChanged = () => {}, places = {} }: Mount) {
  const nav: Nav = { area: "vault", view, scope }
  const value: PlacesValue = { nav, surface: "desktop", go: () => {}, navigate: () => {}, ...places }
  return render(
    <ClientProvider value={fakeClient({ templates: () => Promise.resolve(TEMPLATES), userTemplates: () => Promise.resolve([]), ...client })}>
      <TooltipProvider>
        <SettingsProvider>
          <TemplatesProvider>
            <GateProvider>
              <PlacesProvider value={value}>
                <VaultsProvider value={{ vaults, reload: () => {} }}>
                  <VaultView view={view} rows={rows} error={error} onChanged={onChanged} onNotice={() => {}} selected={null} onSelect={() => {}} onPalette={() => {}} scope={scope} />
                </VaultsProvider>
              </PlacesProvider>
            </GateProvider>
          </TemplatesProvider>
        </SettingsProvider>
      </TooltipProvider>
    </ClientProvider>,
  )
}

beforeEach(() => {
  window.localStorage.clear()
})

describe("the vault list's structure (design review R18)", () => {
  it("while loading, says so in a status with its words inside, draws skeleton rows and no count", () => {
    mount({ rows: null })
    const status = screen.getByRole("status")
    expect(status).toHaveTextContent("Loading items")
    expect(status).not.toHaveAttribute("aria-label")
    expect(status.querySelectorAll('[data-slot="skeleton"]').length).toBeGreaterThanOrEqual(5)
    expect(screen.getByRole("searchbox", { name: "Search" })).toHaveAttribute("placeholder", "Search")
  })

  it("groups All items by letter on a desktop, each a region named with its count, the letter's count beside it", () => {
    const { container } = mount({ rows: [row("n1", "note", "Passport checklist"), row("n2", "note", "Alarm codes"), row("l1", "login", "Mail"), row("l2", "login", "mail backup")] })
    const m = screen.getByRole("region", { name: "M, 2 items" })
    expect(within(m).getAllByRole("listitem")).toHaveLength(2)
    expect(within(m).getByRole("heading", { level: 2 })).toHaveTextContent(/M.*02/)
    expect(screen.getByRole("region", { name: "A, 1 item" })).toBeInTheDocument()
    expect(screen.getByRole("region", { name: "P, 1 item" })).toBeInTheDocument()
    // The pane's name is a heading only a screen reader reads (brief 4.0).
    expect(screen.getByRole("heading", { level: 1, name: "All items" })).toBeInTheDocument()
    expect(repeatedLandmarks(container)).toEqual([])
  })

  it("names each kind's group with its count when grouped by type, so no group shares a name with a section of the open item", async () => {
    window.localStorage.setItem("lockdown.list-order", JSON.stringify({ group: "kind" }))
    const { container } = mount({ rows: [row("n1", "note", "Passport checklist"), row("n2", "note", "Alarm codes"), row("l1", "login", "Mail")] })
    const notes = screen.getByRole("region", { name: "Notes, 2 items" })
    expect(within(notes).getAllByRole("listitem")).toHaveLength(2)
    expect(screen.getByRole("region", { name: "Logins, 1 item" })).toBeInTheDocument()
    expect(screen.queryByRole("region", { name: "Notes" })).not.toBeInTheDocument()
    expect(repeatedLandmarks(container)).toEqual([])
  })

  it("keeps the sort and group per device, and offers By letter only with A to Z", async () => {
    mount({ rows: [row("a", "login", "Alpha", { edited_at: 1 }), row("b", "login", "Bravo", { edited_at: 9 })] })
    const user = userEvent.setup({ pointerEventsCheck: 0 })
    await user.click(screen.getByRole("button", { name: "Sort and group" }))
    expect(await screen.findByRole("menuitemradio", { name: "By letter" })).toHaveAttribute("aria-checked", "true")
    await user.click(screen.getByRole("menuitemradio", { name: "Recently changed" }))
    await waitFor(() => expect(JSON.parse(window.localStorage.getItem("lockdown.list-order")!)).toEqual({ sort: "changed" }))
    // Newest change first, one group: no letters out of order.
    await waitFor(() => expect(screen.queryByRole("region", { name: /^B, / })).not.toBeInTheDocument())
    expect(screen.getAllByRole("listitem").map((li) => li.textContent)).toEqual([expect.stringMatching(/^Bravo/), expect.stringMatching(/^Alpha/)])
    if (!screen.queryByRole("menu")) await user.click(screen.getByRole("button", { name: "Sort and group" }))
    expect(await screen.findByRole("menuitemradio", { name: "None" })).toHaveAttribute("aria-checked", "true")
    expect(screen.queryByRole("menuitemradio", { name: "By letter" })).not.toBeInTheDocument()
  })

  it("opens Search with what is typed in All items' field (brief V1, X4)", async () => {
    const go = vi.fn()
    mount({ rows: [row("a", "login", "Alpha")], places: { go } })
    const user = userEvent.setup()
    const field = screen.getByRole("searchbox", { name: "Search" })
    expect(field).toHaveAttribute("placeholder", "Search 1 item")
    await user.type(field, "b")
    expect(go).toHaveBeenCalledWith("search")
  })
})

describe("what a row shows (brief V1)", () => {
  it("draws the title with its marks, the mono line and the right-hand words, and no kind glyph or copy button", async () => {
    const report = { findings: [{ kind: "weak", item_id: "t", dismissed: null } as unknown as Finding] } as SecurityReport
    mount({
      rows: [
        row("s", "login", "Steam", { site: "https://store.steam.example", methods: ["password", "passkey", "code"], favourite: true, copy: { field_id: "pw", how: "secret", label: "password" } }),
        row("t", "login", "Ticketmaster", { site: "ticketmaster.example", methods: ["password"] }),
        row("j", "crypto-wallet", "Cold wallet", { crown_jewel: true, subtitle: "" }),
      ],
      client: { securityReport: () => Promise.resolve(report) },
    })
    const steam = screen.getByRole("button", { name: "Steam" })
    expect(steam).toHaveAccessibleDescription(/store\.steam\.example.*Passkey \+ code/)
    expect(screen.getByRole("img", { name: "Favorite" })).toBeInTheDocument()
    expect(screen.queryByRole("button", { name: /^Copy/ })).not.toBeInTheDocument()
    // The cached report's finding, on a desktop.
    expect(await screen.findByText("Login // weak")).toBeInTheDocument()
    // A crown jewel: its title and mark, nothing under it (D113).
    const wallet = screen.getByRole("button", { name: "Cold wallet" })
    expect(wallet).toHaveAccessibleDescription("Crypto wallet")
    expect(screen.getByText("Crown jewel")).toBeInTheDocument()
  })

  it("names the vault in a kind view across vaults (Desktop / 03)", () => {
    mount({ view: "card", rows: [row("c1", "card", "Everyday card", { vault_id: "v1" }), row("c2", "card", "Family card", { vault_id: "v2" })] })
    expect(screen.getByRole("button", { name: "Everyday card" })).toHaveAccessibleDescription("Personal")
    expect(screen.getByRole("button", { name: "Family card" })).toHaveAccessibleDescription("Family")
    // A kind's list has its Section Header with the count and SORT; no search field.
    expect(screen.getByRole("heading", { level: 2, name: /Cards\s*002/ })).toBeInTheDocument()
    expect(screen.getByRole("button", { name: "Sort" })).toBeInTheDocument()
    expect(screen.queryByRole("searchbox")).not.toBeInTheDocument()
  })

  it("draws only the rows in view of a list over 200, each still saying where it sits", () => {
    const many = Array.from({ length: WINDOW_FROM + 50 }, (_, i) => row(`r${i}`, "login", `Row ${String(i).padStart(3, "0")}`))
    mount({ rows: many })
    const drawn = screen.getAllByRole("listitem")
    expect(drawn.length).toBeLessThan(many.length)
    expect(drawn[0]).toHaveAttribute("aria-setsize", String(many.length))
    expect(drawn[0]).toHaveAttribute("aria-posinset", "1")
  })
})

describe("a list that could not load (cross-app decisions §9)", () => {
  it("keeps the rows of a failed reload, with a critical Banner and Try again above them", async () => {
    const onChanged = vi.fn()
    mount({ rows: [row("a", "login", "Alpha")], error: "The server didn't answer.", onChanged })
    const alert = screen.getByRole("alert")
    expect(alert).toHaveTextContent("Couldn't load your items.")
    expect(alert).toHaveTextContent("The server didn't answer.")
    expect(screen.getByRole("button", { name: "Alpha" })).toBeInTheDocument()
    await userEvent.click(within(alert).getByRole("button", { name: "Try again" }))
    expect(onChanged).toHaveBeenCalled()
  })

  it("says so in a critical Empty State when nothing loaded", async () => {
    const onChanged = vi.fn()
    mount({ rows: null, error: "The vault's index on this device is damaged.", onChanged })
    expect(screen.queryByRole("status")).not.toBeInTheDocument()
    expect(screen.getByText("Couldn't load your items.")).toBeInTheDocument()
    expect(screen.getByText("The vault's index on this device is damaged.")).toBeInTheDocument()
    await userEvent.click(screen.getByRole("button", { name: "Try again" }))
    expect(onChanged).toHaveBeenCalled()
  })
})

describe("the empty states (brief 5.2)", () => {
  it.each<[ListView, string, string, string | null]>([
    // Beside the sidebar, its NEW ITEM is the way in; the Empty State repeats it only where the sidebar's groups don't show.
    ["all", "Items // 00", "Nothing saved yet.", null],
    ["favourites", "Favorites // 00", "No favorites yet.", null],
    ["passkeys", "Passkeys // 00", "No passkeys yet.", null],
    ["archived", "Archived // 00", "Nothing archived.", null],
    ["card", "Cards // 00", "No cards yet.", "New card"],
    ["contacts", "Contacts // 00", "No contacts yet.", null],
  ])("%s says %s, %s", async (view, overline, title, action) => {
    mount({ view, rows: [] })
    expect(screen.getByText(overline)).toBeInTheDocument()
    expect(screen.getByText(title)).toBeInTheDocument()
    if (action) expect(await screen.findByRole("button", { name: action })).toBeInTheDocument()
    // Contacts offers NEW CONTACT where its template is (the test's templates have none): never a button that does nothing.
    else expect(screen.queryByRole("button", { name: /^New / })).not.toBeInTheDocument()
  })

  it("reads Recently deleted's days from the vault, and says each vault keeps its own across vaults", () => {
    const { unmount } = mount({ view: "deleted", rows: [], scope: { vault: "v2" } })
    expect(screen.getByText("Nothing here.")).toBeInTheDocument()
    expect(screen.getByText("Deleted items stay here for 7 days, then go for good.")).toBeInTheDocument()
    unmount()
    mount({ view: "deleted", rows: [] })
    expect(screen.getByText("Each vault keeps deleted items for its own time.")).toBeInTheDocument()
  })

  it("captions Recently deleted with its vault's time when it holds something", () => {
    mount({ view: "deleted", rows: [row("x", "note", "Old", { state: "trashed", views: ["deleted"], vault_id: "v1" })], scope: { vault: "v1" } })
    expect(screen.getByText("Kept for 30 days, then deleted for good.")).toBeInTheDocument()
    // The rows offer no copy, and say nothing the view already says.
    expect(screen.getByRole("button", { name: "Old" })).toHaveAccessibleDescription(/Secure note$/)
  })

  it("says a scope has nothing filed, and a kind inside a scope names both", () => {
    const { unmount } = mount({ view: "all", rows: [row("a", "login", "Alpha", { vault_id: "v1" })], scope: { vault: "v2" } })
    expect(screen.getByText("Family // 00")).toBeInTheDocument()
    expect(screen.getByText("Nothing is filed here yet.")).toBeInTheDocument()
    unmount()
    mount({ view: "card", rows: [row("a", "login", "Alpha", { vault_id: "v2" })], scope: { vault: "v2" } })
    expect(screen.getByText("Family // Cards // 00")).toBeInTheDocument()
    expect(screen.getByText("No cards in Family.")).toBeInTheDocument()
  })

  it("chips the kinds a scope holds, on a desktop", async () => {
    const navigate = vi.fn()
    mount({ view: "all", rows: [row("a", "login", "Alpha", { vault_id: "v2" }), row("b", "card", "Bluefin", { vault_id: "v2" })], scope: { vault: "v2" }, places: { navigate } })
    const chips = screen.getByRole("radiogroup", { name: "Show" })
    expect(within(chips).getAllByRole("radio").map((c) => c.textContent)).toEqual([expect.stringMatching(/^All/), "Logins", "Cards"])
    await userEvent.click(within(chips).getByRole("radio", { name: "Cards" }))
    expect(navigate).toHaveBeenCalledWith(expect.objectContaining({ view: "card", scope: { vault: "v2" } }))
  })
})

const LINE = "Some recent changes from bob@example.com couldn't be verified, so Lockdown left them out. You don't need to do anything."

const vault = (id: string, unverified: string | null): VaultInfo => ({ id, name: id === "v1" ? "Family" : "Personal", primary: id !== "v1", items: 1, trash_days: 30, folders: [], unverified })

describe("a shared vault with changes held back (docs/plan/49-shared-vault-quiet.md, D573)", () => {
  it("says so in one calm compact Banner on that vault, with nothing to click", () => {
    mount({ rows: [row("n1", "note", "Door code")], scope: { vault: "v1" }, vaults: [vault("v0", null), vault("v1", LINE)] })
    const note = screen.getByRole("note")
    expect(note).toHaveTextContent(LINE)
    expect(within(note).queryByRole("button")).not.toBeInTheDocument()
    expect(screen.queryByRole("alert")).not.toBeInTheDocument()
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument()
  })

  it("shows nothing on a vault with nothing held back, or outside a vault", () => {
    mount({ rows: [row("n1", "note", "Door code")], scope: { vault: "v0" }, vaults: [vault("v0", null), vault("v1", LINE)] })
    expect(screen.queryByText(LINE)).not.toBeInTheDocument()
  })

  it("shows nothing in All items", () => {
    mount({ rows: [row("n1", "note", "Door code")], vaults: [vault("v1", LINE)] })
    expect(screen.queryByText(LINE)).not.toBeInTheDocument()
  })
})
