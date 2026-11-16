import { act, render, screen, waitFor, within } from "@testing-library/react"
import userEvent, { type UserEvent } from "@testing-library/user-event"
import { useState, type ReactNode } from "react"

import { TooltipProvider } from "@unicron/ui"

import { GateProvider } from "@/app/gate"
import { SettingsProvider } from "@/app/settings"
import { TemplatesProvider } from "@/app/templates"
import { AppShell, type Nav } from "@/app/shell"
import { ClientProvider, type Client } from "@/core/client"
import type { FieldTemplateView, ItemSummary, Templates, VaultView } from "@/core/types"
import { fakeClient } from "@/test/fake-client"
import { TEMPLATES } from "@/test/templates-fixture"
import { MasterDetail, PaneHeader } from "@/components/master-detail"
import { OrganizationNav } from "./organization-nav"
import { VaultsProvider } from "./vaults-context"

const VAULTS: VaultView[] = [
  {
    id: "v1",
    name: "Personal",
    primary: true,
    items: 2,
    trash_days: 30,
    folders: [
      { id: "f-work", name: "Work", parent: null, path: "Work" },
      { id: "f-clients", name: "Clients", parent: "f-work", path: "Work / Clients" },
    ],
  },
  { id: "v2", name: "Family", primary: false, items: 1, trash_days: 7, folders: [] },
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

// The core's Login template: its websites repeat.
const LOGIN_TEMPLATES: Templates = {
  ...TEMPLATES,
  kinds: TEMPLATES.kinds.map((t) =>
    t.kind === "login" ? { ...t, fields: t.fields.map((f): FieldTemplateView => (f.field_id === "url.0" ? { ...f, repeats: true } : f)) } : t,
  ),
}

function mount(client: Client, ui: ReactNode, vaults = VAULTS) {
  return render(
    <ClientProvider value={client}>
      <SettingsProvider>
        <TemplatesProvider>
          <GateProvider>
            <VaultsProvider value={{ vaults, reload: () => {} }}>{ui}</VaultsProvider>
          </GateProvider>
        </TemplatesProvider>
      </SettingsProvider>
    </ClientProvider>,
  )
}

const base = { templates: () => Promise.resolve(LOGIN_TEMPLATES), approvalRoute: () => Promise.resolve({ route: "window" as const, method: "window" }) }

describe("vaults, folders and tags in the sidebar", () => {
  it("lists each vault with its folder tree and the tags, and narrows the list to one", async () => {
    const onNavigate = vi.fn<(n: Nav) => void>()
    const rows = [row("a", { folder: "f-clients", tags: ["work/clients"] }), row("b", { vault_id: "v2", tags: ["family"] })]
    mount(fakeClient(base), <OrganizationNav rows={rows} nav={{ area: "vault", view: "all" }} onNavigate={onNavigate} />)
    const user = userEvent.setup()
    // One tree, named for the group and its count: Up and Down move, Right and Left open and close (doc 64).
    const tree = screen.getByRole("tree", { name: /^Vaults/ })
    expect(within(tree).getByRole("treeitem", { name: /^Personal/ })).toBeInTheDocument()
    expect(within(tree).getByRole("treeitem", { name: /^Family/ })).toBeInTheDocument()
    // A vault's folders are closed until it is opened; a vault with none has nothing to open.
    expect(within(tree).queryByRole("treeitem", { name: /^Clients/ })).toBeNull()
    expect(within(tree).getByRole("treeitem", { name: /^Personal/ })).toHaveAttribute("aria-expanded", "false")
    expect(within(tree).getByRole("treeitem", { name: /^Family/ })).not.toHaveAttribute("aria-expanded")
    within(tree).getByRole("treeitem", { name: /^Personal/ }).focus()
    await user.keyboard("{ArrowRight}")
    expect(within(tree).getByRole("treeitem", { name: /^Work/ })).toHaveAttribute("aria-level", "2")
    expect(within(tree).getByRole("treeitem", { name: /^Clients/ })).toHaveAttribute("aria-level", "3")
    await user.click(within(tree).getByRole("treeitem", { name: /^Clients/ }))
    expect(onNavigate).toHaveBeenLastCalledWith({ area: "vault", view: "all", scope: { folder: "f-clients" } })
    // Tags is one more row with the tag tree inside.
    await user.click(within(tree).getByRole("treeitem", { name: "Tags" }))
    expect(within(tree).getAllByRole("treeitem", { name: /^(work|clients|family),/ })).toHaveLength(3)
    await user.click(within(tree).getByRole("treeitem", { name: /^clients,/ }))
    expect(onNavigate).toHaveBeenLastCalledWith({ area: "vault", view: "all", scope: { tag: "work/clients" } })
    // Every row keeps its menu, by the same name. In the tree the ⋯ is for a pointer: a tree owns
    // only its items, so the button is out of the tab order and hidden from assistive tech (the
    // row's Menu key opens the same menu, below).
    for (const name of ["Personal options", "#work/clients options"]) {
      const options = screen.getByLabelText(name, { selector: "button" })
      expect(options).toHaveAttribute("aria-hidden", "true")
      expect(options).toHaveAttribute("tabindex", "-1")
    }
    // Sharing is the group's last row.
    await user.click(within(tree).getByRole("treeitem", { name: "Sharing" }))
    expect(onNavigate).toHaveBeenLastCalledWith({ area: "sharing", view: "all" })
  })

  it("makes a vault with the name given", async () => {
    const createVault = vi.fn<Client["createVault"]>(() => Promise.resolve("v3"))
    mount(fakeClient({ ...base, createVault }), <OrganizationNav rows={[]} nav={{ area: "vault", view: "all" }} onNavigate={() => {}} />)
    const user = userEvent.setup()
    await user.click(screen.getByRole("button", { name: "New vault" }))
    await user.type(await screen.findByLabelText("Name"), "Work")
    await user.click(screen.getByRole("button", { name: "Create vault" }))
    await waitFor(() => expect(createVault).toHaveBeenCalledWith("Work"))
  })
})

/*
 * Brief V2: the vault, folder and tag menus at the three widths (doc 71).
 * The real shell draws the rows: the sidebar's tree at 1440, the page
 * head's popover at 800, the Switch vault sheet at 390. Every menu is
 * reached with the keyboard alone.
 */

const SHELL_VAULTS: VaultView[] = [
  ...VAULTS.map((v) => (v.id === "v2" ? { ...v, sharing: { kind: "family" as const, right: "manage" as const, owned: true, owner: null, people: 3 } } : v)),
  { id: "v3", name: "Book club", primary: false, items: 1, trash_days: 30, folders: [], sharing: { kind: "shared", right: "view", owned: false, owner: "robin@club.example", people: 4 } },
]
const SHELL_ROWS = [row("a", { folder: "f-clients", tags: ["work/clients"] }), row("b", { vault_id: "v2", tags: ["family"] }), row("c", { vault_id: "v3" })]

function ShellScreen({ nav }: { nav: Nav }) {
  if (nav.area !== "vault") return <p>elsewhere</p>
  return <MasterDetail header={<PaneHeader title="All items" />} title="All items" list={<p>rows</p>} detail={null} onBack={() => {}} emptyLabel="Pick an item." />
}

function ShellHarness({ client, onPlace }: { client: Client; onPlace?: (nav: Nav) => void }) {
  const [nav, setNav] = useState<Nav>({ area: "vault", view: "all" })
  const navigate = (next: Nav) => {
    onPlace?.(next)
    setNav(next)
  }
  return (
    <ClientProvider value={client}>
      <TooltipProvider>
        <SettingsProvider>
          <TemplatesProvider>
            <GateProvider>
              <VaultsProvider value={{ vaults: SHELL_VAULTS, reload: () => {} }}>
                <AppShell surface="desktop" nav={nav} onNavigate={navigate} rows={SHELL_ROWS} onLock={() => {}} onSaved={() => {}}>
                  <ShellScreen nav={nav} />
                </AppShell>
              </VaultsProvider>
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

/** Reaches a control with the keyboard alone: it takes focus, and Enter runs it. */
async function press(user: UserEvent, target: () => HTMLElement, key = "{Enter}") {
  await waitFor(() => {
    const el = target()
    expect(el.closest("[inert], [aria-hidden='true']"), `${el.getAttribute("aria-label") ?? el.textContent} is out of reach`).toBeNull()
    act(() => el.focus())
    expect(el).toHaveFocus()
  })
  await user.keyboard(key)
}

/** The rows of the menu that is open: menu items beside the sidebar and in the popover, buttons in a phone's sheet. */
function openMenuRows(): string[] {
  const menus = screen.queryAllByRole("menu")
  if (menus.length) return Array.from(menus[menus.length - 1]!.querySelectorAll<HTMLElement>('[role^="menuitem"]')).map((el) => el.textContent?.trim() ?? "")
  const sheets = document.querySelectorAll<HTMLElement>('[data-adaptive="menu"]')
  const sheet = sheets[sheets.length - 1]
  if (!sheet) throw new Error("no menu is open")
  return Array.from(sheet.querySelectorAll<HTMLElement>('[data-slot="adaptive-menu-item"], [role="radio"]')).map((el) => el.textContent?.trim() ?? "")
}

const WIDTHS = [
  ["1440", 1440, 900],
  ["800", 800, 1024],
  ["390", 390, 844],
] as const

const startsName = (s: string) => new RegExp(`^${s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}(,|$)`)

describe.each(WIDTHS)("the vault, folder and tag menus at %s px", (_name, width, height) => {
  const wide = width >= 1024
  beforeEach(() => setWindow(width, height))
  afterEach(() => setWindow(1024, 768))

  /** Where the rows are: the sidebar's tree, or the switcher the page head opens. */
  async function rowsAt(user: UserEvent) {
    if (wide) return screen.getByRole("tree", { name: /^Vaults/ })
    await press(user, () => screen.getByRole("button", { name: /Switch vault$/ }))
    const layer = await screen.findByRole("dialog")
    return within(layer).getByRole("list", { name: "Vaults" })
  }
  /** Opens a branch: Right on the tree item beside the sidebar, its Expand button in the switcher. */
  async function openBranch(user: UserEvent, rows: HTMLElement, name: string) {
    if (wide) {
      const item = within(rows).getByRole("treeitem", { name: startsName(name) })
      if (item.getAttribute("aria-expanded") === "true") return
      await press(user, () => item, "{ArrowRight}")
    } else if (name === "Tags") await press(user, () => within(rows).getByRole("button", { name: "Tags" }))
    else await press(user, () => within(rows).getByRole("button", { name: `Expand ${name}` }))
  }
  async function openMenu(user: UserEvent, rows: HTMLElement, label: string) {
    // Beside the sidebar the ⋯ is for a pointer; the keyboard opens the same menu from the row (Shift+F10).
    // A tag's row is named by its last part ("#work/clients" is the row "clients").
    if (wide) await press(user, () => within(rows).getByRole("treeitem", { name: startsName(label.replace(/ options$/, "").replace(/^#(.*\/)?/, "")) }), "{Shift>}{F10}{/Shift}")
    else await press(user, () => within(rows).getByRole("button", { name: label }))
    await waitFor(() => expect(openMenuRows().length).toBeGreaterThan(0))
  }
  async function closeMenu(user: UserEvent) {
    await user.keyboard("{Escape}")
    await waitFor(() => expect(screen.queryByRole("menu") ?? document.querySelectorAll('[data-adaptive="menu"]').length > 1 ? "open" : null).toBeNull())
  }

  it("offers every vault, folder and tag action, by keyboard", async () => {
    const user = userEvent.setup()
    render(<ShellHarness client={fakeClient(base)} />)
    const rows = await rowsAt(user)

    await openMenu(user, rows, "Personal options")
    expect(openMenuRows()).toEqual(["New folder", "Rename", "Keep deleted items for…", "Share…"])
    await closeMenu(user)
    await openMenu(user, rows, "Family options")
    expect(openMenuRows()).toEqual(["New folder", "Rename", "Keep deleted items for…", "Members"])
    await closeMenu(user)
    // A vault shared with you to view: who has it, and nothing its right doesn't allow.
    await openMenu(user, rows, "Book club options")
    expect(openMenuRows()).toEqual(["Who has it"])
    await closeMenu(user)

    await openBranch(user, rows, "Personal")
    await openMenu(user, rows, "Work options")
    expect(openMenuRows()).toEqual(["New folder inside", "Rename", "Remove folder"])
    await closeMenu(user)
    await openMenu(user, rows, "Clients options")
    expect(openMenuRows()).toEqual(["New folder inside", "Rename", "Move to the top", "Remove folder"])
    await closeMenu(user)

    await openBranch(user, rows, "Tags")
    await openMenu(user, rows, "#work/clients options")
    expect(openMenuRows()).toEqual(["Rename", "Take off every item"])
  }, 60_000)

  it("keeps deleted items for the time picked, from the vault's menu", async () => {
    const setTrashDays = vi.fn<Client["setTrashDays"]>(() => Promise.resolve())
    const user = userEvent.setup()
    render(<ShellHarness client={fakeClient({ ...base, setTrashDays })} />)
    const rows = await rowsAt(user)
    await openMenu(user, rows, "Personal options")
    const leads = () => (screen.queryByRole("menu") ? screen.getByRole("menuitem", { name: "Keep deleted items for…" }) : screen.getByRole("button", { name: "Keep deleted items for…" }))
    await press(user, leads)
    // The choices open as a menu of their own (Lockdown 20), not a second page of the first: the vault's own time checked.
    await waitFor(() => expect(openMenuRows()).toEqual(["7 days", "14 days", "30 days", "60 days", "90 days", "A year"]))
    expect(screen.queryByRole("menuitem", { name: "Rename" })).toBeNull()
    const choice = (name: string) => screen.getByRole(screen.queryByRole("menu") ? "menuitemradio" : "radio", { name })
    expect(choice("30 days")).toHaveAttribute("aria-checked", "true")
    await press(user, () => choice("60 days"))
    // Saved on pick, and the menu goes away.
    await waitFor(() => expect(setTrashDays).toHaveBeenCalledWith("v1", 60))
    await waitFor(() => expect(screen.queryByRole("menuitemradio", { name: "60 days" })).toBeNull())
  }, 60_000)

  it("asks before it removes a folder, and Cancel removes nothing", async () => {
    const removeFolder = vi.fn<Client["removeFolder"]>(() => Promise.resolve(2))
    const user = userEvent.setup()
    render(<ShellHarness client={fakeClient({ ...base, removeFolder })} />)
    const rows = await rowsAt(user)
    await openBranch(user, rows, "Personal")
    await openMenu(user, rows, "Work options")
    await press(user, () => (screen.queryByRole("menu") ? screen.getByRole("menuitem", { name: "Remove folder" }) : screen.getByRole("button", { name: "Remove folder" })))
    const question = await screen.findByRole("dialog", { name: 'Remove "Work"?' })
    // Focus starts on Cancel, so Enter never removes by accident.
    await waitFor(() => expect(within(question).getByRole("button", { name: "Cancel" })).toHaveFocus())
    await user.keyboard("{Enter}")
    await waitFor(() => expect(screen.queryByRole("dialog", { name: 'Remove "Work"?' })).toBeNull())
    expect(removeFolder).not.toHaveBeenCalled()
  }, 60_000)
})

describe("the name prompts", () => {
  it("renames a vault, and keeps the prompt open with the core's words when it can't", async () => {
    let refuse = true
    const renameVault = vi.fn<Client["renameVault"]>(() => (refuse ? Promise.reject({ code: "refused", message: "A vault named Family already exists." }) : Promise.resolve()))
    mount(fakeClient({ ...base, renameVault }), <OrganizationNav rows={[]} nav={{ area: "vault", view: "all" }} onNavigate={() => {}} />)
    const user = userEvent.setup()
    await user.click(screen.getByLabelText("Personal options", { selector: "button" }))
    await user.click(await screen.findByRole("menuitem", { name: "Rename" }))
    const field = await screen.findByRole("textbox", { name: "Name" })
    expect(field).toHaveValue("Personal")
    // One field: the question size, 480 on a desktop (Lockdown 20).
    expect(field.closest('[data-slot="dialog-content"]')).toHaveAttribute("data-size", "question")
    await user.clear(field)
    await user.type(field, "Family{Enter}")
    // The error is the field's own, under it; the prompt stays.
    expect(await screen.findByText("A vault named Family already exists.")).toBeInTheDocument()
    expect(field).toHaveAttribute("aria-invalid", "true")
    refuse = false
    await user.clear(field)
    await user.type(field, "Home")
    await user.click(screen.getByRole("button", { name: "Rename" }))
    await waitFor(() => expect(renameVault).toHaveBeenLastCalledWith("v1", "Home"))
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Rename vault" })).toBeNull())
  })

  it("says what is missing when the name is empty, and saves nothing", async () => {
    const addFolder = vi.fn<Client["addFolder"]>(() => Promise.resolve("f1"))
    mount(fakeClient({ ...base, addFolder }), <OrganizationNav rows={[]} nav={{ area: "vault", view: "all" }} onNavigate={() => {}} />)
    const user = userEvent.setup()
    await user.click(screen.getByLabelText("Family options", { selector: "button" }))
    await user.click(await screen.findByRole("menuitem", { name: "New folder" }))
    await screen.findByRole("dialog", { name: "New folder in Family" })
    await user.click(screen.getByRole("button", { name: "Create folder" }))
    expect(await screen.findByText("Type a name.")).toBeInTheDocument()
    expect(addFolder).not.toHaveBeenCalled()
    await user.type(screen.getByRole("textbox", { name: "Name" }), "School{Enter}")
    await waitFor(() => expect(addFolder).toHaveBeenCalledWith("v2", "School", null))
  })

  it("takes a tag off every item only after asking", async () => {
    const renameTag = vi.fn<Client["renameTag"]>(() => Promise.resolve(3))
    const rows = [row("a", { tags: ["travel"] })]
    mount(fakeClient({ ...base, renameTag }), <OrganizationNav rows={rows} nav={{ area: "vault", view: "all", scope: { tag: "travel" } }} onNavigate={() => {}} />)
    const user = userEvent.setup()
    await user.click(screen.getByLabelText("#travel options", { selector: "button" }))
    await user.click(await screen.findByRole("menuitem", { name: "Take off every item" }))
    const question = await screen.findByRole("dialog", { name: "Take #travel off every item?" })
    expect(renameTag).not.toHaveBeenCalled()
    await user.click(within(question).getByRole("button", { name: "Take off" }))
    await waitFor(() => expect(renameTag).toHaveBeenCalledWith("travel", null))
  })
})

describe("keys", () => {
  it("opens a row's menu with Shift+F10 and the Menu key", async () => {
    mount(fakeClient(base), <OrganizationNav rows={[]} nav={{ area: "vault", view: "all" }} onNavigate={() => {}} />)
    const user = userEvent.setup()
    const tree = screen.getByRole("tree", { name: /^Vaults/ })
    within(tree).getByRole("treeitem", { name: /^Family/ }).focus()
    await user.keyboard("{Shift>}{F10}{/Shift}")
    expect(await screen.findByRole("menu", { name: "Family" })).toBeInTheDocument()
    await user.keyboard("{Escape}")
    await waitFor(() => expect(screen.queryByRole("menu")).toBeNull())
    // Closing gives focus back to the row, never to the hidden ⋯.
    await waitFor(() => expect(within(tree).getByRole("treeitem", { name: /^Family/ })).toHaveFocus())
    within(tree).getByRole("treeitem", { name: /^Personal/ }).focus()
    await user.keyboard("{ContextMenu}")
    expect(await screen.findByRole("menu", { name: "Personal" })).toBeInTheDocument()
  })

  it("keeps the tree to its items: Tab leaves the tree in one stop, past every ⋯", async () => {
    mount(fakeClient(base), <OrganizationNav rows={[]} nav={{ area: "vault", view: "all" }} onNavigate={() => {}} />)
    const tree = screen.getByRole("tree", { name: /^Vaults/ })
    // The tree's only children are its items: no button stands among them for assistive tech.
    expect(within(tree).queryAllByRole("button")).toEqual([])
    const stops = Array.from(tree.querySelectorAll<HTMLElement>("button, [tabindex]")).filter((el) => el.tabIndex >= 0)
    expect(stops).toHaveLength(1)
    expect(stops[0]).toHaveAttribute("role", "treeitem")
  })

  it("moves through the switcher's rows with the tree's arrow keys", async () => {
    const rows = [row("a", { folder: "f-work", tags: ["travel"] })]
    mount(fakeClient(base), <OrganizationNav variant="switcher" rows={rows} nav={{ area: "vault", view: "all" }} onNavigate={() => {}} />)
    const user = userEvent.setup()
    const list = screen.getByRole("list", { name: "Vaults" })
    const rowNamed = (name: RegExp) => within(list).getByRole("button", { name })
    rowNamed(/^All vaults(,|$)/).focus()
    await user.keyboard("{ArrowDown}")
    expect(rowNamed(/^Personal(,|$)/)).toHaveFocus()
    // Right opens a closed branch, then goes to its first row; Left goes back up, then closes it.
    await user.keyboard("{ArrowRight}")
    expect(rowNamed(/^Work(,|$)/)).toBeInTheDocument()
    expect(rowNamed(/^Personal(,|$)/)).toHaveFocus()
    await user.keyboard("{ArrowRight}")
    expect(rowNamed(/^Work(,|$)/)).toHaveFocus()
    await user.keyboard("{ArrowLeft}")
    expect(rowNamed(/^Personal(,|$)/)).toHaveFocus()
    await user.keyboard("{ArrowLeft}")
    expect(within(list).queryByRole("button", { name: /^Work(,|$)/ })).toBeNull()
    await user.keyboard("{End}")
    expect(rowNamed(/^Tags(,|$)/)).toHaveFocus()
    await user.keyboard("{Home}")
    expect(rowNamed(/^All vaults(,|$)/)).toHaveFocus()
    // The switcher ends with its add row, after the vaults, the shared ones and Sharing (cross-app §7).
    const buttons = screen.getAllByRole("button")
    expect(buttons[buttons.length - 1]).toHaveAccessibleName("New vault")
  })
})
