import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import type { ReactNode } from "react"

import { GateProvider } from "@/app/gate"
import { SettingsProvider } from "@/app/settings"
import { TemplatesProvider } from "@/app/templates"
import { ClientProvider, type Client } from "@/core/client"
import type { FieldTemplateView, ItemSummary, Templates, VaultView } from "@/core/types"
import { fakeClient } from "@/test/fake-client"
import { TEMPLATES } from "@/test/templates-fixture"

import { SecurityView } from "@/features/security/security-view"

import { DuplicateDetail } from "./duplicates-review"
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

describe("Duplicates", () => {
  it("asks Keep which?, the newest chosen first, and merges into the copy kept after asking", async () => {
    const mergeDuplicates = vi.fn<Client["mergeDuplicates"]>(() => Promise.resolve(4))
    const onChanged = vi.fn()
    mount(
      fakeClient({ ...base, mergeDuplicates }),
      <DuplicateDetail
        group={{ reason: "same-login", words: "2 logins for the same website and username", ids: ["a", "b"], mergeable: true }}
        rows={[row("a", { title: "Forum", edited_at: 100 }), row("b", { title: "Forum old", edited_at: 50 })]}
        onChanged={onChanged}
        onOpen={() => {}}
      />,
    )
    const keep = screen.getByRole("radiogroup", { name: /Keep which/ })
    expect(within(keep).getByRole("radio", { name: "Forum" })).toBeChecked()
    const user = userEvent.setup()
    await user.click(within(keep).getByRole("radio", { name: "Forum old" }))
    await user.click(screen.getByRole("button", { name: "Merge" }))
    const ask = await screen.findByRole("dialog")
    expect(ask).toHaveTextContent('Merge into "Forum old"?')
    // Focus starts on Cancel: Enter never merges by accident.
    expect(within(ask).getByRole("button", { name: "Cancel" })).toHaveFocus()
    expect(mergeDuplicates).not.toHaveBeenCalled()
    await user.click(within(ask).getByRole("button", { name: "Merge" }))
    await waitFor(() => expect(mergeDuplicates).toHaveBeenCalledWith("b", ["a"], {}))
    expect(onChanged).toHaveBeenCalled()
  })

  it("a group that cannot merge offers OPEN on each copy and DELETE on every copy but the first", async () => {
    const trash = vi.fn<Client["trash"]>(() => Promise.resolve())
    const onOpen = vi.fn()
    mount(
      fakeClient({ ...base, trash }),
      <DuplicateDetail
        group={{ reason: "same-password", words: "Forum and Shop share a password.", ids: ["a", "b"], mergeable: false }}
        rows={[row("a", { title: "Forum" }), row("b", { title: "Shop" })]}
        onChanged={() => {}}
        onOpen={onOpen}
      />,
    )
    expect(screen.queryByRole("button", { name: "Merge" })).not.toBeInTheDocument()
    expect(screen.queryByRole("button", { name: "Delete Forum" })).not.toBeInTheDocument()
    const user = userEvent.setup()
    await user.click(screen.getByRole("button", { name: "Open Forum" }))
    expect(onOpen).toHaveBeenCalledWith("a")
    await user.click(screen.getByRole("button", { name: "Delete Shop" }))
    await waitFor(() => expect(trash).toHaveBeenCalledWith("b"))
  })

  it("is reached from Health in the web vault, whose client finds and merges duplicates", async () => {
    const duplicates = vi.fn<Client["duplicates"]>(() => Promise.resolve([{ reason: "same-login", words: "2 logins for forum.example", ids: ["a", "b"], mergeable: true }]))
    mount(
      fakeClient({
        ...base,
        duplicates,
        securityPlaybooks: () => Promise.resolve([]),
        list: () => Promise.resolve([row("a", { title: "Forum" }), row("b", { title: "Forum old" })]),
        status: () => Promise.reject({ code: "refused", message: "not here" }),
      }),
      <SecurityView surface="web" onDevices={() => {}} />,
    )
    expect(await screen.findByText("Checks for weak, reused and breached passwords are in the desktop app.")).toBeInTheDocument()
    // No hero, no metrics, no Fix groups on the web.
    expect(screen.queryByText("to fix")).not.toBeInTheDocument()
    expect(screen.queryByRole("list", { name: "Fix now" })).not.toBeInTheDocument()
    const row1 = screen.getByRole("group", { name: "Duplicates" })
    expect(row1).toHaveTextContent("Not checked")
    const user = userEvent.setup()
    await user.click(within(row1).getByRole("button", { name: "Find duplicates" }))
    const list = await screen.findByRole("list", { name: "Possible duplicates" })
    await user.click(within(list).getByRole("button", { name: "2 logins for forum.example" }))
    expect(await screen.findByRole("radiogroup", { name: /Keep which/ })).toBeInTheDocument()
  })
})
