import { act, render, screen, waitFor, within } from "@testing-library/react"
import userEvent, { type UserEvent } from "@testing-library/user-event"
import { useState } from "react"

import { TooltipProvider } from "@unicron/ui"

import { GateProvider } from "@/app/gate"
import { SettingsProvider } from "@/app/settings"
import { TemplatesProvider } from "@/app/templates"
import { MasterDetail, PageColumn, PaneHeader } from "@/components/master-detail"
import { ClientProvider } from "@/core/client"
import type { ItemSummary, VaultView } from "@/core/types"
import { VaultsProvider } from "@/features/organization/vaults-context"
import { SettingsPlaces } from "@/features/settings/settings-view"
import { SharingClientProvider, type SharingOverview } from "@/features/sharing/sharing-client"
import { SharingView } from "@/features/sharing/sharing-view"
import { VIEW_LABELS } from "@/features/vault/list-model"
import { fakeClient } from "@/test/fake-client"
import { fakeSharing } from "@/test/fake-sharing"
import { TEMPLATES } from "@/test/templates-fixture"

import { useCrumb } from "./crumbs"
import { ASIDE_VIEWS, MAIN_VIEWS, MORE_VIEWS, folderPlace, membersPlace, placeIdOf, settingsSections, sharingTabs, tagPlace, usePlaces, vaultPlace, type Nav, type Surface } from "./places"
import { AppShell } from "./shell"

/*
 * The shell at its three widths (doc 64): every place is reached with the
 * keyboard, and the navigation says where you are, once. jsdom lays nothing
 * out, so the width is the window's own numbers, which is what unicron's
 * frame reads to pick its shell.
 */

const VAULTS: VaultView[] = [
  { id: "v1", name: "Personal", primary: true, items: 0, trash_days: 30, folders: [{ id: "f-money", name: "Finance", parent: null, path: "Finance" }] },
  { id: "v2", name: "Family", primary: false, items: 0, trash_days: 30, folders: [], sharing: { kind: "family", right: "edit", owned: true, owner: null, people: 3 } as VaultView["sharing"] },
]

let n = 0
const row = (kind: string, over: Partial<ItemSummary> = {}): ItemSummary => ({
  id: `i${++n}`,
  kind,
  state: "live",
  title: `${kind} ${n}`,
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

// One live item of every kind, so every view has a row, a chip or an entry under More.
const KINDS = ["login", "card", "note", "ssh-key", "api-credential", "document", "identity", "bank-account", "crypto-wallet", "server", "database", "software-licence", "membership", "custom"]
const ROWS: ItemSummary[] = [
  ...KINDS.map((k) => row(k)),
  row("login", { views: ["favourites"], favourite: true, folder: "f-money", tags: ["work/clients"] }),
  row("passkey", { views: ["passkeys"], vault_id: "v2" }),
  row("code", { views: ["codes"] }),
  row("wi-fi", { views: ["wi-fi"] }),
  row("contact", { views: ["contacts"] }),
  row("card", { views: ["expiring"] }),
  row("login", { views: ["attention"] }),
  row("note", { state: "archived", views: ["archived"] }),
  row("note", { state: "trashed", views: ["deleted"] }),
]

const OVERVIEW: SharingOverview = { online: true, fingerprint: "11111 22222", fingerprint_qr: "", contacts: [], vaults: [], households: [], sent: [], received: [], emergency: [], waiting_on_you: 0 }

/** A screen for every area: enough of each to carry its head, and the real Sharing tabs and Settings frame. */
function Screen() {
  const { nav, go } = usePlaces()
  // A screen may name its deeper place.
  useCrumb(nav.area === "security" ? ["Ticketmaster"] : null)
  if (nav.area === "vault" || nav.area === "search") {
    const title = nav.area === "search" ? "Search" : VIEW_LABELS[nav.view]
    return <MasterDetail header={<PaneHeader title={title} />} title={title} list={<p>rows</p>} detail={null} onBack={() => {}} emptyLabel="Pick an item." />
  }
  if (nav.area === "settings") return <SettingsPlaces overview={<p>this device's rows</p>}>{(section) => <p>section {section}</p>}</SettingsPlaces>
  if (nav.area === "sharing")
    return (
      <PageColumn>
        <SharingView rows={[]} section={nav.tab ?? "people"} vault={nav.members} onSection={(tab) => go(`sharing-${tab}`)} onOpenItem={() => {}} onChanged={() => {}} onNotice={() => {}} />
      </PageColumn>
    )
  return (
    <PageColumn>
      <PaneHeader title={nav.area === "codes" ? "Codes" : "Health"}>
        {nav.area === "security" && <button onClick={() => go("expiring")}>Expiring</button>}
        {nav.area === "security" && <button onClick={() => go("attention")}>Needs attention</button>}
      </PaneHeader>
    </PageColumn>
  )
}

function Harness({ surface = "desktop", onLock = () => {} }: { surface?: Surface; onLock?: () => void }) {
  const [nav, setNav] = useState<Nav>({ area: "vault", view: "all" })
  return (
    <ClientProvider value={fakeClient({ templates: () => Promise.resolve(TEMPLATES), userTemplates: () => Promise.resolve([]) })}>
      <TooltipProvider>
        <SettingsProvider>
          <TemplatesProvider>
            <GateProvider>
              <SharingClientProvider value={fakeSharing({ overview: () => Promise.resolve(OVERVIEW) })}>
                <VaultsProvider value={{ vaults: VAULTS, reload: () => {} }}>
                  <output data-testid="place">{placeIdOf(nav)}</output>
                  <AppShell surface={surface} nav={nav} onNavigate={setNav} rows={ROWS} onLock={onLock} toFix={3} sharingMark={1} onSaved={() => {}}>
                    <Screen />
                  </AppShell>
                </VaultsProvider>
              </SharingClientProvider>
            </GateProvider>
          </TemplatesProvider>
        </SettingsProvider>
      </TooltipProvider>
    </ClientProvider>
  )
}

function setWindow(width: number, height: number) {
  Object.defineProperty(window, "innerWidth", { configurable: true, value: width })
  Object.defineProperty(window, "innerHeight", { configurable: true, value: height })
  act(() => {
    window.dispatchEvent(new Event("resize"))
  })
}

const where = () => screen.getByTestId("place").textContent
const sidebar = () => screen.getByRole("navigation", { name: "Sidebar" })
const esc = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
/** A name that starts with these words (a row before its count). */
const starts = (s: string) => new RegExp(`^${esc(s)}(,|$)`)

/** Reaches a control with the keyboard alone: it takes focus, and Enter (or Space) runs it. No pointer. */
async function press(user: UserEvent, target: HTMLElement | (() => HTMLElement), key = "{Enter}") {
  // A layer that has just opened draws its rows again as it settles, and puts focus on its own first
  // control a moment later; so the control is looked up, and asked to take focus, until it holds.
  await waitFor(() => {
    const el = typeof target === "function" ? target() : target
    expect(el.closest("[inert], [aria-hidden='true']"), `${el.getAttribute("aria-label") ?? el.textContent} is out of reach`).toBeNull()
    expect(el).not.toBeDisabled()
    act(() => el.focus())
    expect(el).toHaveFocus()
  })
  await user.keyboard(key)
}

type Width = "expanded" | "medium" | "compact"

/** The way to each place, as a person at that width would go with the keyboard. */
function ways(user: UserEvent, width: Width) {
  // A sidebar row: a tree item in ITEMS and VAULTS, a button elsewhere and in the rail.
  const sideRow = (name: RegExp | string) => within(sidebar()).queryByRole("treeitem", { name }) ?? within(sidebar()).getByRole("button", { name })
  const side = (name: RegExp | string) => press(user, sideRow(name))
  const sideButton = side
  const tab = async (name: RegExp) => {
    // A pushed screen (Settings, Sharing) has no tab bar: Back first.
    while (!screen.queryByRole("tablist", { name: "Sections" })) await press(user, screen.getByRole("button", { name: "Back" }))
    await press(user, screen.getByRole("tab", { name }))
  }
  const items = () => (width === "expanded" ? side(starts("All items")) : width === "medium" ? side(starts("Items")) : tab(/^Items/))
  const section = (label: string) => (width === "compact" ? tab(starts(label)) : side(starts(label)))
  const layer = () => screen.findByRole("dialog")
  const openSwitcher = async () => {
    await items()
    await press(user, screen.getByRole("button", { name: /Switch vault$/ }))
    return layer()
  }
  /** A row of the vaults tree, by the names on the way to it: `treeRow("Personal", "Finance")`. */
  const treeRow = async (...names: string[]) => {
    if (width === "expanded") {
      for (const name of names) {
        const el = within(sidebar()).getByRole("treeitem", { name: starts(name) })
        // A branch that is already open (Tags) is left open.
        if (name === "Tags" && el.getAttribute("aria-expanded") === "true") continue
        await press(user, el)
      }
      return
    }
    // In the switcher the rows are plain buttons, and a chosen row closes it: so a vault on the way to
    // its folder is opened with its own "Expand" button.
    const sheet = await openSwitcher()
    for (const name of names) {
      const last = name === names[names.length - 1]
      const el = () => within(within(sheet).getByRole("list", { name: "Vaults" })).getByRole("button", { name: last || name === "Tags" ? starts(name) : `Expand ${name}` })
      if (name === "Tags" && (await waitFor(el)).getAttribute("aria-expanded") === "true") continue
      await press(user, el)
    }
  }
  const moreRow = async (label: string) => {
    if (width === "expanded") {
      const more = within(sidebar()).getByRole("treeitem", { name: "More" })
      if (more.getAttribute("aria-expanded") === "false") await press(user, more)
      await side(starts(label))
      return
    }
    await items()
    await press(user, screen.getByRole("button", { name: "More kinds and lists" }))
    const list = width === "medium" ? await screen.findByRole("menu") : await layer()
    await press(user, () => within(list).getByRole(width === "medium" ? "menuitem" : "button", { name: starts(label) }))
  }
  const view = async (label: string) => {
    if (width === "expanded") return side(starts(label))
    await items()
    // A radio of the group named Show: Space chooses it.
    await press(user, within(screen.getByRole("radiogroup", { name: "Show" })).getByRole("radio", { name: new RegExp(`^${esc(label)}`) }), " ")
  }
  const settings = () => (width === "compact" ? press(user, screen.getByRole("button", { name: "Settings" })) : sideButton(starts("Settings")))
  const sharing = async () => {
    if (width === "expanded") return side(starts("Sharing"))
    const sheet = await openSwitcher()
    await press(user, () => within(sheet).getByRole("button", { name: starts("Sharing") }))
  }
  return { side, items, section, treeRow, moreRow, view, settings, sharing, openSwitcher, sideButton }
}

/** How many things in the navigation say "you are here". */
function currents(width: Width): string[] {
  if (width === "compact") {
    const bar = screen.queryByRole("tablist", { name: "Sections" })
    const tabs = bar ? within(bar).queryAllByRole("tab", { selected: true }).map((t) => t.getAttribute("aria-label") ?? "") : []
    const buttons = Array.from(document.querySelectorAll<HTMLElement>("button[aria-current]")).filter((b) => !b.closest("[inert], [aria-hidden='true'], [role='dialog']"))
    return [...tabs, ...buttons.map((b) => b.getAttribute("aria-label") ?? "")]
  }
  return Array.from(sidebar().querySelectorAll<HTMLElement>("[aria-current='page']")).map((el) => el.getAttribute("aria-label") ?? "")
}

const SIZES: Array<[Width, number, number]> = [
  ["expanded", 1440, 900],
  ["medium", 800, 1024],
  ["compact", 390, 844],
]

describe.each(SIZES)("every place, by keyboard, at the %s width (%i px)", (width, w, h) => {
  beforeEach(() => {
    window.localStorage.clear()
    setWindow(w, h)
  })

  /** Where the navigation must say you are, for a place. */
  const expectCurrent = (label: RegExp | null) => {
    const now = currents(width)
    if (label === null) expect(now, `at ${where()}`).toEqual([])
    else {
      // Exactly one row is current: the deepest real place (brief 3.2).
      expect(now, `at ${where()}`).toHaveLength(1)
      expect(now[0]).toMatch(label)
    }
  }
  // Where the sidebar's groups don't show, the section stands for the places inside it.
  const itemsRow = (own: RegExp) => (width === "expanded" ? own : /^Items/)

  it("reaches every view and every kind", async () => {
    const user = userEvent.setup()
    render(<Harness />)
    const go = ways(user, width)
    expect(where()).toBe("all")
    expectCurrent(itemsRow(/^All items, 21 items$/))
    for (const v of MAIN_VIEWS) {
      if (v === "codes") {
        await go.section("Codes")
        expect(where()).toBe("codes")
        expectCurrent(/^Codes/)
        continue
      }
      await go.view(VIEW_LABELS[v])
      expect(where()).toBe(v)
      expectCurrent(itemsRow(starts(VIEW_LABELS[v])))
    }
    for (const v of [...MORE_VIEWS, ...ASIDE_VIEWS]) {
      await go.moreRow(VIEW_LABELS[v])
      await waitFor(() => expect(where()).toBe(v))
      expectCurrent(itemsRow(starts(VIEW_LABELS[v])))
    }
    await go.items()
    expect(where()).toBe("all")
  }, 120_000)

  it("reaches every vault, folder and tag, Sharing's tabs and a shared vault's members", async () => {
    const user = userEvent.setup()
    render(<Harness />)
    const go = ways(user, width)
    await go.treeRow("Family")
    await waitFor(() => expect(where()).toBe(vaultPlace("v2")))
    expectCurrent(itemsRow(starts("Family")))
    // A vault's folders open with the vault.
    await go.treeRow("Personal", "Finance")
    await waitFor(() => expect(where()).toBe(folderPlace("f-money")))
    expectCurrent(itemsRow(starts("Finance")))
    await go.treeRow("Tags", "work")
    await waitFor(() => expect(where()).toBe(tagPlace("work")))
    expectCurrent(itemsRow(starts("work")))
    await go.treeRow("Tags", "clients")
    await waitFor(() => expect(where()).toBe(tagPlace("work/clients")))

    await go.sharing()
    await waitFor(() => expect(where()).toBe("sharing"))
    // Sharing is a sidebar row; where the groups don't show, no section is active on it.
    expectCurrent(width === "expanded" ? /^Sharing, 1 waiting on you$/ : null)
    // People is the tab Sharing opens on; so it is pressed last, from another tab.
    const [first, ...others] = sharingTabs("desktop")
    expect(await screen.findByRole("tab", { name: new RegExp(`^${first}$`, "i") })).toHaveAttribute("aria-selected", "true")
    for (const tab of [...others, first!]) {
      await press(user, await screen.findByRole("tab", { name: new RegExp(`^${tab}$`, "i") }))
      await waitFor(() => expect(where()).toBe(`sharing-${tab}`))
      expectCurrent(width === "expanded" ? /^Sharing/ : null)
    }
    if (width === "expanded") {
      // One shared vault's members: from the vault's own menu.
      await press(user, screen.getByRole("button", { name: "Family options" }))
      await press(user, () => within(screen.getByRole("menu")).getByRole("menuitem", { name: "Members" }))
      await waitFor(() => expect(where()).toBe(membersPlace("v2")))
      expectCurrent(/^Sharing/)
    } else {
      // One shared vault's members: its SHARED WITH row in the switcher.
      const sheet = await go.openSwitcher()
      await press(user, () => within(within(sheet).getByRole("list", { name: "Shared with" })).getByRole("button", { name: /^Family/ }))
      await waitFor(() => expect(where()).toBe(membersPlace("v2")))
      expectCurrent(null)
    }
  }, 120_000)

  it("reaches Search, Health and its lists, and every Settings section", async () => {
    const user = userEvent.setup()
    render(<Harness />)
    const go = ways(user, width)
    await (width === "compact" ? press(user, screen.getByRole("button", { name: "Search" })) : go.sideButton(starts("Search")))
    expect(where()).toBe("search")
    // Search included: its row, its rail item, or the phone's search button, and no tab.
    expectCurrent(/^Search$/)
    // `/` goes there from anywhere but a field.
    await go.items()
    await user.keyboard("/")
    expect(where()).toBe("search")

    await go.section("Health")
    expect(where()).toBe("health")
    expectCurrent(/^Health, 3 to fix$/)
    for (const list of ["Expiring", "Needs attention"]) {
      await go.section("Health")
      await press(user, screen.getByRole("button", { name: list }))
      expect(where()).toBe(list === "Expiring" ? "expiring" : "attention")
      expectCurrent(/^Health/)
    }

    await go.settings()
    expect(where()).toBe("settings")
    expectCurrent(width === "compact" ? null : /^Settings$/)
    const sections = within(screen.getByRole("navigation", { name: "Settings sections" }))
    for (const p of settingsSections("desktop")) {
      const label = p.nav.section === "this-device" ? /^This (Mac|PC)$/ : starts(p.label)
      if (p.nav.section === "this-device" && width !== "expanded") {
        // Narrower than two columns the device's rows lead the list itself.
        expect(screen.getByText("this device's rows")).toBeInTheDocument()
        continue
      }
      if (width !== "expanded" && where() !== "settings") await press(user, screen.getByRole("button", { name: "Back" }))
      await press(user, sections.getByRole("button", { name: label }))
      expect(where()).toBe(p.id)
      expect(screen.getByText(`section ${p.nav.section}`)).toBeInTheDocument()
      expectCurrent(width === "compact" ? null : /^Settings$/)
    }
    // ⌘, opens Settings from anywhere.
    await go.items()
    await user.keyboard("{Meta>},{/Meta}")
    expect(where()).toBe("settings")
  }, 120_000)
})

describe("the sidebar (brief 3.2)", () => {
  beforeEach(() => {
    window.localStorage.clear()
    setWindow(1440, 900)
  })

  it("keeps its name, its groups with their counts, Search with its key, and Settings beside Lock at the foot", async () => {
    const onLock = vi.fn()
    const user = userEvent.setup()
    render(<Harness onLock={onLock} />)
    const nav = within(sidebar())
    expect(nav.getByRole("button", { name: "New item" })).toBeInTheDocument()
    expect(nav.getByRole("button", { name: "Search" })).toHaveAttribute("aria-keyshortcuts", "/")
    // ITEMS // NN TYPES: the kinds that hold an item that is neither archived nor deleted.
    expect(nav.getByRole("tree", { name: "Items // 18 types" })).toBeInTheDocument()
    expect(nav.getByRole("tree", { name: /^Vaults, 2$/ })).toBeInTheDocument()
    expect(nav.getByRole("group", { name: "Tools" })).toBeInTheDocument()
    // A count is said as the number and its noun, and written in two digits.
    const all = nav.getByRole("treeitem", { name: "All items, 21 items" })
    expect(all).toHaveTextContent("21")
    expect(nav.getByRole("treeitem", { name: "Favorites, 1 item" })).toHaveTextContent("01")
    expect(nav.getByRole("button", { name: "Health, 3 to fix" })).toHaveTextContent("03")
    expect(nav.getByRole("treeitem", { name: "Sharing, 1 waiting on you" })).toBeInTheDocument()
    await press(user, nav.getByRole("button", { name: "Lock" }))
    expect(onLock).toHaveBeenCalledTimes(1)
  })

  it("opens More in place, remembers it on this device, and keeps it open while the place showing is inside it", async () => {
    const user = userEvent.setup()
    const first = render(<Harness />)
    const more = () => within(sidebar()).getByRole("treeitem", { name: "More" })
    expect(more()).toHaveAttribute("aria-expanded", "false")
    expect(within(sidebar()).queryByRole("treeitem", { name: starts("Archived") })).toBeNull()
    // The tree's keys (C4): Right opens, Down moves in, Left closes.
    act(() => more().focus())
    await user.keyboard("{ArrowRight}")
    expect(more()).toHaveAttribute("aria-expanded", "true")
    expect(within(sidebar()).getByRole("treeitem", { name: starts("Wi-Fi") })).toHaveAttribute("aria-level", "2")
    await user.keyboard("{ArrowDown}")
    expect(within(sidebar()).getByRole("treeitem", { name: starts("Wi-Fi") })).toHaveFocus()
    // A new start on this device finds it open.
    first.unmount()
    const second = render(<Harness />)
    expect(more()).toHaveAttribute("aria-expanded", "true")
    act(() => more().focus())
    await user.keyboard("{ArrowLeft}")
    expect(more()).toHaveAttribute("aria-expanded", "false")
    second.unmount()
    render(<Harness />)
    expect(more()).toHaveAttribute("aria-expanded", "false")
  })

  it("⌘\\ and Ctrl+\\ collapse it to the rail and back, kept on this device", async () => {
    const user = userEvent.setup()
    const first = render(<Harness />)
    expect(sidebar()).toHaveAttribute("data-state", "expanded")
    await user.keyboard("{Meta>}\\{/Meta}")
    expect(sidebar()).toHaveAttribute("data-state", "rail")
    // The rail: the three sections, with Health's count, between Search and Settings; the page head is the switcher now.
    expect(within(sidebar()).getByRole("button", { name: "Health, 3 to fix" })).toBeInTheDocument()
    expect(within(sidebar()).queryByRole("tree")).toBeNull()
    expect(screen.getByRole("button", { name: /Switch vault$/ })).toBeInTheDocument()
    first.unmount()
    render(<Harness />)
    expect(sidebar()).toHaveAttribute("data-state", "rail")
    await user.keyboard("{Control>}\\{/Control}")
    expect(sidebar()).toHaveAttribute("data-state", "expanded")
    expect(screen.queryByRole("button", { name: /Switch vault$/ })).toBeNull()
  })

  it("draws the breadcrumb as links back, with what the open screen added", async () => {
    const user = userEvent.setup()
    render(<Harness />)
    const crumbs = () => within(screen.getByRole("navigation", { name: "Breadcrumb" }))
    expect(crumbs().getByText("ALL")).toHaveAttribute("aria-current", "page")
    await press(user, within(sidebar()).getByRole("button", { name: starts("Health") }))
    // The screen named its deeper place through useCrumb.
    expect(screen.getByRole("navigation", { name: "Breadcrumb" })).toHaveTextContent("TOOLS // HEALTH // TICKETMASTER")
    expect(crumbs().getByText("TICKETMASTER")).toHaveAttribute("aria-current", "page")
    await press(user, within(sidebar()).getByRole("treeitem", { name: starts("Personal") }))
    await press(user, within(sidebar()).getByRole("treeitem", { name: starts("Finance") }))
    expect(screen.getByRole("navigation", { name: "Breadcrumb" })).toHaveTextContent("VAULTS // PERSONAL // FINANCE")
    await press(user, crumbs().getByRole("button", { name: "PERSONAL" }))
    expect(where()).toBe(vaultPlace("v1"))
  })

  it("in the web vault lists the web's places only", async () => {
    const user = userEvent.setup()
    render(<Harness surface="web" />)
    await press(user, within(sidebar()).getByRole("button", { name: starts("Settings") }))
    const sections = within(screen.getByRole("navigation", { name: "Settings sections" }))
    expect(sections.getAllByRole("button").map((b) => b.getAttribute("aria-label"))).toEqual(["This browser", "Appearance", "Devices", "Server"])
  })
})

describe("the phone's rows (brief 3.3)", () => {
  beforeEach(() => {
    window.localStorage.clear()
    setWindow(390, 844)
  })

  it("has the lockup and the profile button on top; Search, three tabs and create at the foot", async () => {
    render(<Harness />)
    expect(screen.queryByRole("navigation", { name: "Sidebar" })).toBeNull()
    expect(screen.getByRole("button", { name: "Settings" })).toBeInTheDocument()
    const tabs = within(screen.getByRole("tablist", { name: "Sections" })).getAllByRole("tab")
    expect(tabs.map((t) => t.getAttribute("aria-label"))).toEqual(["Items", "Codes", "Health, 3 to fix"])
    expect(screen.getByRole("button", { name: "Search" })).toBeInTheDocument()
    expect(screen.getByRole("button", { name: "New item" })).toHaveAttribute("aria-keyshortcuts", "Shift+Enter")
  })

  it("makes the whole page head the switcher, with the scope and type count over the view's name and its count", async () => {
    const user = userEvent.setup()
    render(<Harness />)
    const head = screen.getByRole("button", { name: /Switch vault$/ })
    expect(head).toHaveAccessibleName("Items, Switch vault")
    expect(screen.getByText(/^All vaults \/\/ 18 types$/i)).toBeInTheDocument()
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("Items")
    // The arrow keys on the head step the scope: All vaults, then each vault.
    act(() => head.focus())
    await user.keyboard("{ArrowRight}")
    expect(where()).toBe(vaultPlace("v1"))
    expect(screen.getByText(/^Personal \/\/ \d+ types$/i)).toBeInTheDocument()
    await user.keyboard("{ArrowRight}")
    expect(where()).toBe(vaultPlace("v2"))
    await user.keyboard("{ArrowLeft}{ArrowLeft}")
    expect(where()).toBe("all")
  })

  it("shows the chips as one group named Show, with the count on the active chip only, and MORE last", async () => {
    render(<Harness />)
    const group = screen.getByRole("radiogroup", { name: "Show" })
    const chips = within(group).getAllByRole("radio")
    expect(chips.map((c) => c.textContent?.replace(/\s+/g, " ").trim())).toEqual(["All 21", "Favorites", "Logins", "Passkeys", "Cards", "Notes"])
    expect(chips[0]).toHaveAttribute("aria-checked", "true")
    expect(screen.getByRole("button", { name: "More kinds and lists" })).toHaveTextContent("More")
  })

  it("opens the list of kinds from create, and quick create from its long-press key", async () => {
    const user = userEvent.setup()
    render(<Harness />)
    const create = screen.getByRole("button", { name: "New item" })
    await press(user, create)
    const sheet = await screen.findByRole("dialog", { name: "New item" })
    expect(await within(sheet).findByRole("group", { name: "Sign-ins and money" })).toBeInTheDocument()
    await user.keyboard("{Escape}")
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "New item" })).toBeNull())
    // Quick create is never gesture-only: Shift+Enter opens it.
    await press(user, screen.getByRole("button", { name: "New item" }), "{Shift>}{Enter}{/Shift}")
    const quick = await screen.findByRole("menu", { name: "Quick create" })
    expect((await within(quick).findAllByRole("menuitem")).map((m) => m.textContent)).toEqual(TEMPLATES.kinds.filter((t) => !t.variant && ["login", "card", "note", "code"].includes(t.kind)).sort((a, b) => ["login", "card", "note", "code"].indexOf(a.kind) - ["login", "card", "note", "code"].indexOf(b.kind)).map((t) => t.name))
  })

  it("puts Lock in Settings' top bar, and Back returns to the section the person came from", async () => {
    const onLock = vi.fn()
    const user = userEvent.setup()
    render(<Harness onLock={onLock} />)
    await press(user, screen.getByRole("tab", { name: /^Codes/ }))
    await press(user, screen.getByRole("button", { name: "Settings" }))
    expect(where()).toBe("settings")
    // A pushed screen has no tab bar.
    expect(screen.queryByRole("tablist", { name: "Sections" })).toBeNull()
    await press(user, screen.getByRole("button", { name: "Lock" }))
    expect(onLock).toHaveBeenCalledTimes(1)
    await press(user, screen.getByRole("button", { name: "Back" }))
    expect(where()).toBe("codes")
  })

  it("a second press on the active tab goes back to its root and fetches again", async () => {
    const user = userEvent.setup()
    const onRefresh = vi.fn()
    function WithRefresh() {
      const [nav, setNav] = useState<Nav>({ area: "vault", view: "card" })
      return (
        <ClientProvider value={fakeClient({ templates: () => Promise.resolve(TEMPLATES), userTemplates: () => Promise.resolve([]) })}>
          <TooltipProvider>
            <SettingsProvider>
              <TemplatesProvider>
                <GateProvider>
                  <VaultsProvider value={{ vaults: VAULTS, reload: () => {} }}>
                    <output data-testid="place">{placeIdOf(nav)}</output>
                    <AppShell nav={nav} onNavigate={setNav} rows={ROWS} onLock={() => {}} onSaved={() => {}} onRefresh={onRefresh}>
                      <Screen />
                    </AppShell>
                  </VaultsProvider>
                </GateProvider>
              </TemplatesProvider>
            </SettingsProvider>
          </TooltipProvider>
        </ClientProvider>
      )
    }
    render(<WithRefresh />)
    expect(where()).toBe("card")
    await user.click(screen.getByRole("tab", { name: "Items" }))
    expect(where()).toBe("all")
    expect(onRefresh).toHaveBeenCalledTimes(1)
  })
})
