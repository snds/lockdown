import { render, screen, waitFor } from "@testing-library/react"
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

describe("assigning from the list and the detail", () => {

  it("the detail shows where the item is and takes it out of the crown-jewel class only after asking", async () => {
    const setCrownJewel = vi.fn<Client["setCrownJewel"]>((_id, _on, gate) =>
      gate?.confirmed ? Promise.resolve("plain-id") : Promise.reject({ code: "approval_needed", message: "confirm in this window", detail: "window" }),
    )
    const onReplaced = vi.fn()
    const item: ItemDetail = {
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
    mount(fakeClient({ ...base, setCrownJewel }), <PlacePanel item={item} title="Bank" rows={[]} onChanged={() => {}} onReplaced={onReplaced} onError={() => {}} />)
    expect(screen.getByText("Work / Clients")).toBeInTheDocument()
    expect(screen.getByText("#work")).toBeInTheDocument()
    expect(screen.getByRole("button", { name: "Remove from favourites" })).toBeInTheDocument()
    const user = userEvent.setup()
    await user.click(screen.getByRole("button", { name: "More for this item" }))
    await user.click(await screen.findByRole("menuitem", { name: "Stop keeping as a crown jewel" }))
    await user.click(await screen.findByRole("button", { name: "Stop protecting it" }))
    // The core asked; the window's confirm stands in on a device with nothing else.
    await user.click(await screen.findByRole("button", { name: /Confirm/ }))
    await waitFor(() => expect(onReplaced).toHaveBeenCalledWith("plain-id", "No longer a crown jewel."))
  })
})
