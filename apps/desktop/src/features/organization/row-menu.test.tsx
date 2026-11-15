import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import type { ReactNode } from "react"

import { GateProvider } from "@/app/gate"
import { SettingsProvider } from "@/app/settings"
import { TemplatesProvider } from "@/app/templates"
import { ClientProvider, type Client } from "@/core/client"
import type { FieldTemplateView, ItemSummary, Templates, VaultView } from "@/core/types"
import { fakeClient } from "@/test/fake-client"
import { TEMPLATES } from "@/test/templates-fixture"
import { RowMenu } from "./row-menu"
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

describe("assigning from the list and the detail", () => {
  it("a row's menu stars, files, moves and archives through the core", async () => {
    const organize = vi.fn<Client["organize"]>(() => Promise.resolve(2))
    const moveItem = vi.fn<Client["moveItem"]>(() => Promise.resolve("new-id"))
    const archive = vi.fn<Client["archive"]>(() => Promise.resolve())
    const onChanged = vi.fn()
    mount(fakeClient({ ...base, organize, moveItem, archive }), <RowMenu row={row("a")} onOrganize={() => {}} onChanged={onChanged} onError={() => {}} />)
    // The menu fades out after a pick; the next open doesn't wait for it.
    const user = userEvent.setup({ pointerEventsCheck: 0 })
    const open = () => user.click(screen.getByRole("button", { name: "More for a" }))
    await open()
    await user.click(await screen.findByRole("menuitem", { name: "Add to favourites" }))
    await waitFor(() => expect(organize).toHaveBeenCalledWith("a", { favourite: true }))
    await open()
    await user.click(await screen.findByRole("menuitem", { name: "Archive" }))
    await waitFor(() => expect(archive).toHaveBeenCalledWith("a"))
    await open()
    const sub = await screen.findByRole("menuitem", { name: "Move to vault" })
    await user.click(sub)
    await user.keyboard("{ArrowRight}")
    // The submenu opened from the keyboard highlights its first vault; Enter picks it.
    expect(await screen.findByRole("menuitem", { name: "Family" })).toHaveAttribute("data-highlighted")
    await user.keyboard("{Enter}")
    await waitFor(() => expect(moveItem).toHaveBeenCalledWith("a", "v2", {}))
    expect(onChanged).toHaveBeenLastCalledWith("Moved to Family.", "new-id")
  })
})
