import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import type { ReactNode } from "react"

import { GateProvider } from "@/app/gate"
import { SettingsProvider } from "@/app/settings"
import { TemplatesProvider } from "@/app/templates"
import { ClientProvider, type Client } from "@/core/client"
import type { FieldTemplateView, ItemDetail, Templates, VaultView } from "@/core/types"
import { fakeClient } from "@/test/fake-client"
import { TEMPLATES } from "@/test/templates-fixture"
import { PlacePanel } from "./place-panel"
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

const jewel: ItemDetail = {
  id: "j1",
  kind: "login",
  state: "live",
  version: 3,
  edited_at: 0,
  crown_jewel: true,
  fields: [],
  vault_id: "v1",
  organization: { folder: "f-clients", tags: ["work"], favourite: true },
  attachments: [],
  crown_jewel_fixed: false,
}

describe("where an item is, in Details (brief I4)", () => {
  it("shows its vault, folder and tags as rows, and moves a crown jewel to another vault only after asking", async () => {
    const moveItem = vi.fn<Client["moveItem"]>((_id, _to, gate) =>
      gate?.confirmed ? Promise.resolve("moved-id") : Promise.reject({ code: "approval_needed", message: "confirm in this window", detail: "window" }),
    )
    const onReplaced = vi.fn()
    mount(fakeClient({ ...base, moveItem }), <PlacePanel item={jewel} title="Bank" rows={[]} onChanged={() => {}} onReplaced={onReplaced} onError={() => {}} />)
    expect(screen.getByRole("group", { name: "Vault" })).toHaveTextContent("Personal")
    expect(screen.getByRole("group", { name: "Folder" })).toHaveTextContent("Work / Clients")
    expect(within(screen.getByRole("list", { name: "Tags" })).getByText("#work")).toBeInTheDocument()
    const user = userEvent.setup()
    await user.click(within(screen.getByRole("group", { name: "Vault" })).getByRole("button", { name: "Move" }))
    await user.click(await screen.findByRole("menuitem", { name: "Family" }))
    // The core asked; the window's confirm stands in on a device with nothing else.
    await user.click(within(await screen.findByRole("dialog", { name: "Go ahead?" })).getByRole("button", { name: "Continue" }))
    await waitFor(() => expect(onReplaced).toHaveBeenCalledWith("moved-id", "Moved Bank to Family"))
    expect(moveItem).toHaveBeenLastCalledWith("j1", "v2", { confirmed: true })
  })

  it("CHANGE opens Folder and tags; saving files and tags it in one version", async () => {
    const organize = vi.fn<Client["organize"]>(() => Promise.resolve(4))
    const onChanged = vi.fn()
    const plain: ItemDetail = { ...jewel, crown_jewel: false, organization: { folder: null, tags: [], favourite: false } }
    mount(fakeClient({ ...base, organize }), <PlacePanel item={plain} title="Bank" rows={[]} onChanged={onChanged} onReplaced={() => {}} onError={() => {}} />)
    expect(screen.getByRole("group", { name: "Folder" })).toHaveTextContent("None")
    const user = userEvent.setup()
    await user.click(within(screen.getByRole("group", { name: "Tags" })).getByRole("button", { name: "Change" }))
    const form = await screen.findByRole("dialog", { name: "Folder and tags" })
    await user.selectOptions(within(form).getByLabelText("Folder"), "f-work")
    await user.type(within(form).getByLabelText("Tags"), "clients{Enter}Clients{Enter}")
    // The same tag in other letters is kept once.
    expect(within(form).getAllByRole("button", { name: /^Remove tag/ })).toHaveLength(1)
    await user.click(within(form).getByRole("switch", { name: "Favorite" }))
    await user.click(within(form).getByRole("button", { name: "Save" }))
    await waitFor(() => expect(organize).toHaveBeenCalledWith("j1", { folder: "f-work", tags: ["clients"], favourite: true }))
    expect(onChanged).toHaveBeenCalledWith("Saved Bank")
  })

  it("draws nothing to press on a share this person cannot change, and no MOVE with one vault", () => {
    mount(fakeClient(base), <PlacePanel item={jewel} title="Bank" rows={[]} canEdit={false} onChanged={() => {}} onReplaced={() => {}} onError={() => {}} />)
    expect(screen.getByRole("group", { name: "Vault" })).toBeInTheDocument()
    expect(screen.queryByRole("button")).not.toBeInTheDocument()
  })
})
