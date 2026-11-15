import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import type { ReactNode } from "react"

import { GateProvider } from "@/app/gate"
import { SettingsProvider } from "@/app/settings"
import { TemplatesProvider } from "@/app/templates"
import { ClientProvider, type Client } from "@/core/client"
import type { FieldTemplateView, Templates, VaultView } from "@/core/types"
import { ItemEditor } from "@/features/vault/item-editor"
import { fakeClient } from "@/test/fake-client"
import { TEMPLATES } from "@/test/templates-fixture"
import { VaultsProvider } from "@/features/organization/vaults-context"

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

describe("the editor files, tags and keeps several websites", () => {
  it("adds a second website with its own match rule, into the chosen vault", async () => {
    const addItemIn = vi.fn<Client["addItemIn"]>(() => Promise.resolve("l1"))
    const onSaved = vi.fn()
    // The first website as Search's "New login" passes it.
    mount(fakeClient({ ...base, addItemIn }), <ItemEditor target={{ kind: "login", item: null, website: "shop.example.com" }} onClose={() => {}} onSaved={onSaved} />)
    const user = userEvent.setup()
    expect(await screen.findByRole("heading", { name: "shop.example.com" })).toBeInTheDocument()
    // The overline names the vault the item goes to; its row offers CHANGE (brief E2).
    expect(screen.getByText("New login // Personal vault")).toBeInTheDocument()
    // Everything past the essentials, the organization block included, is under More options.
    expect(screen.queryByLabelText("Folder")).not.toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: /More options/ }))
    await user.click(screen.getByRole("button", { name: "Add website" }))
    await user.type(screen.getAllByLabelText("Website")[1]!, "b.example")
    await user.selectOptions(screen.getAllByLabelText("Where website is offered")[0]!, "base_domain")
    await user.click(screen.getByRole("button", { name: "Save login" }))
    await waitFor(() => expect(onSaved).toHaveBeenCalledWith("l1", false))
    const [vault, input] = addItemIn.mock.calls[0]!
    expect(vault).toBe("v1")
    expect(input.fields).toEqual(
      expect.arrayContaining([
        { field_id: "url.0", action: "set", text: "shop.example.com", match_rule: "base_domain" },
        { field_id: "url.1", action: "set", text: "b.example", match_rule: "exact_host" },
      ]),
    )
  })

  it("files it in a folder with a tag, from More options' ORGANIZE", async () => {
    const addItemIn = vi.fn<Client["addItemIn"]>(() => Promise.resolve("l1"))
    const onSaved = vi.fn()
    mount(fakeClient({ ...base, addItemIn }), <ItemEditor target={{ kind: "login", item: null }} onClose={() => {}} onSaved={onSaved} />)
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: /More options/ }))
    await user.selectOptions(screen.getByLabelText("Folder"), "f-clients")
    await user.type(screen.getByLabelText("Tags"), "shop{Enter}")
    await user.click(screen.getByRole("button", { name: "Save login" }))
    await waitFor(() => expect(onSaved).toHaveBeenCalledWith("l1", false))
    const input = addItemIn.mock.calls[0]![1]
    expect(input.fields).toEqual(
      expect.arrayContaining([
        { field_id: "org.folder", action: "set", text: "f-clients" },
        { field_id: "org.tags", action: "set", text: "shop" },
      ]),
    )
    expect(input.fields.some((f) => f.field_id === "org.favourite")).toBe(false)
  })

  it("changes the vault from the VAULT row, which starts the item unfiled", async () => {
    const addItemIn = vi.fn<Client["addItemIn"]>(() => Promise.resolve("l2"))
    mount(fakeClient({ ...base, addItemIn }), <ItemEditor target={{ kind: "login", item: null, vaultId: "v1", folder: "f-work" }} onClose={() => {}} onSaved={() => {}} />)
    const user = userEvent.setup()
    await screen.findByRole("heading", { name: "New login" })
    await user.click(screen.getByRole("button", { name: "Change vault" }))
    await user.click(await screen.findByRole("menuitemradio", { name: /Family/ }))
    expect(screen.getByText("New login // Family vault")).toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: "Save login" }))
    await waitFor(() => expect(addItemIn).toHaveBeenCalled())
    const [vault, input] = addItemIn.mock.calls[0]!
    expect(vault).toBe("v2")
    expect(input.fields.some((f) => f.field_id === "org.folder")).toBe(false)
  })
})
