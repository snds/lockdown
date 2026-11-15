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

import { DuplicatesReview } from "./duplicates-review"
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

describe("the duplicate finder", () => {
  it("lists what the core found and merges into the copy kept", async () => {
    const duplicates = vi.fn<Client["duplicates"]>(() =>
      Promise.resolve([{ reason: "same-login", words: "2 logins for the same website and username", ids: ["a", "b"], mergeable: true }]),
    )
    const mergeDuplicates = vi.fn<Client["mergeDuplicates"]>(() => Promise.resolve(4))
    const onChanged = vi.fn()
    mount(
      fakeClient({ ...base, duplicates, mergeDuplicates }),
      <DuplicatesReview open onOpenChange={() => {}} rows={[row("a", { title: "Forum" }), row("b", { title: "Forum old" })]} onChanged={onChanged} onOpen={() => {}} />,
    )
    const user = userEvent.setup()
    expect(await screen.findByText("2 logins for the same website and username")).toBeInTheDocument()
    const keep = screen.getAllByRole("button", { name: "Keep this, merge the rest" })
    await user.click(keep[0]!)
    await user.click(await screen.findByRole("button", { name: "Merge" }))
    await waitFor(() => expect(mergeDuplicates).toHaveBeenCalledWith("a", ["b"], {}))
    expect(onChanged).toHaveBeenCalledWith("Merged; the other copy is in the trash.")
  })
})
