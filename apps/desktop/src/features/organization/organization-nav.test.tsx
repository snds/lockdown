import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import type { ReactNode } from "react"

import { GateProvider } from "@/app/gate"
import { SettingsProvider } from "@/app/settings"
import { TemplatesProvider } from "@/app/templates"
import type { Nav } from "@/app/shell"
import { ClientProvider, type Client } from "@/core/client"
import type { FieldTemplateView, ItemSummary, Templates, VaultView } from "@/core/types"
import { fakeClient } from "@/test/fake-client"
import { TEMPLATES } from "@/test/templates-fixture"
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
    // Every row keeps its menu, by the same name.
    expect(screen.getByRole("button", { name: "Personal options" })).toBeInTheDocument()
    expect(screen.getByRole("button", { name: "#work/clients options" })).toBeInTheDocument()
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
    await user.click(screen.getByRole("button", { name: "Make vault" }))
    await waitFor(() => expect(createVault).toHaveBeenCalledWith("Work"))
  })
})
