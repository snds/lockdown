import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import type { ReactNode } from "react"

import { GateProvider } from "@/app/gate"
import { SettingsProvider } from "@/app/settings"
import { TemplatesProvider } from "@/app/templates"
import type { Nav } from "@/app/shell"
import { ClientProvider, type Client } from "@/core/client"
import type { FieldTemplateView, ItemDetail, ItemSummary, Templates, VaultView } from "@/core/types"
import { ItemEditor } from "@/features/vault/item-editor"
import { fakeClient } from "@/test/fake-client"
import { TEMPLATES } from "@/test/templates-fixture"

import { DuplicatesReview } from "./duplicates-review"
import { FilesPanel, MAX_FILE_BYTES } from "./files-panel"
import { OrganizationNav } from "./organization-nav"
import { PlacePanel } from "./place-panel"
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

describe("vaults, folders and tags in the sidebar", () => {
  it("lists each vault with its folder tree and the tags, and narrows the list to one", async () => {
    const onNavigate = vi.fn<(n: Nav) => void>()
    const rows = [row("a", { folder: "f-clients", tags: ["work/clients"] }), row("b", { vault_id: "v2", tags: ["family"] })]
    mount(fakeClient(base), <OrganizationNav rows={rows} nav={{ area: "vault", view: "all" }} onNavigate={onNavigate} />)
    const user = userEvent.setup()
    const vaults = screen.getByRole("list", { name: "Vaults" })
    expect(within(vaults).getByRole("button", { name: /^Personal(?! options)/ })).toBeInTheDocument()
    expect(within(vaults).getByRole("button", { name: /^Family(?! options)/ })).toBeInTheDocument()
    const folders = screen.getByRole("list", { name: "Folders in Personal" })
    await user.click(within(folders).getByRole("button", { name: /^Clients(?! options)/ }))
    expect(onNavigate).toHaveBeenLastCalledWith({ area: "vault", view: "all", scope: { folder: "f-clients" } })
    const tags = screen.getByRole("list", { name: "Tags" })
    expect(within(tags).getAllByRole("button", { name: /^(work|clients|family)(?! options)/ })).toHaveLength(3)
    await user.click(within(tags).getByRole("button", { name: /^clients(?! options)/ }))
    expect(onNavigate).toHaveBeenLastCalledWith({ area: "vault", view: "all", scope: { tag: "work/clients" } })
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

describe("files on an item", () => {
  it("attaches the picked file's bytes and saves one back on request", async () => {
    const attach = vi.fn<Client["attach"]>(() => Promise.resolve("att-1"))
    const attachment = vi.fn<Client["attachment"]>(() => Promise.resolve(new Uint8Array([104, 105])))
    const onChanged = vi.fn()
    const createObjectURL = vi.fn(() => "blob:x")
    Object.assign(URL, { createObjectURL, revokeObjectURL: () => {} })
    mount(
      fakeClient({ ...base, attach, attachment }),
      <FilesPanel itemId="i1" files={[{ id: "att-0", name: "scan.pdf", mime: "application/pdf", size: 2048 }]} editable onChanged={onChanged} onNotice={() => {}} />,
    )
    expect(screen.getByText("2 KB · application/pdf")).toBeInTheDocument()
    const user = userEvent.setup()
    const file = new File([new Uint8Array([1, 2, 3])], "receipt.txt", { type: "text/plain" })
    await user.upload(screen.getByLabelText("File to attach"), file)
    await waitFor(() => expect(attach).toHaveBeenCalled())
    const [id, name, mime, bytes] = attach.mock.calls[0]!
    expect([id, name, mime]).toEqual(["i1", "receipt.txt", "text/plain"])
    // The webview's copy is wiped once the core has it.
    expect(Array.from(bytes)).toEqual([0, 0, 0])
    expect(onChanged).toHaveBeenCalledWith('Attached "receipt.txt".')
    await user.click(screen.getByRole("button", { name: "Save scan.pdf" }))
    await waitFor(() => expect(attachment).toHaveBeenCalledWith("i1", "att-0", {}))
    expect(createObjectURL).toHaveBeenCalled()
  })

  it("refuses a file larger than the app attaches, before reading it", async () => {
    const attach = vi.fn<Client["attach"]>()
    mount(fakeClient({ ...base, attach }), <FilesPanel itemId="i1" files={[]} editable onChanged={() => {}} onNotice={() => {}} />)
    const big = new File(["x"], "huge.iso")
    Object.defineProperty(big, "size", { value: MAX_FILE_BYTES + 1 })
    await userEvent.setup().upload(screen.getByLabelText("File to attach"), big)
    expect(await screen.findByRole("alert")).toHaveTextContent("huge.iso")
    expect(attach).not.toHaveBeenCalled()
  })
})

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

describe("the editor files, tags and keeps several websites", () => {
  it("adds a second website with its own match rule, a folder and a tag, into the chosen vault", async () => {
    const addItemIn = vi.fn<Client["addItemIn"]>(() => Promise.resolve("l1"))
    const onSaved = vi.fn()
    mount(fakeClient({ ...base, addItemIn }), <ItemEditor target={{ kind: "login", item: null }} onClose={() => {}} onSaved={onSaved} />)
    const user = userEvent.setup()
    expect(await screen.findByRole("heading", { name: "New login" })).toBeInTheDocument()
    await user.type(screen.getByLabelText("Website"), "shop.example.com")
    await user.click(screen.getByRole("button", { name: "Add another website" }))
    const sites = screen.getAllByLabelText("Website")
    await user.type(sites[1]!, "checkout.example.net")
    const rules = screen.getAllByLabelText("Where website is offered")
    await user.selectOptions(rules[0]!, "base_domain")
    await user.selectOptions(screen.getByLabelText("Folder"), "f-clients")
    await user.type(screen.getByLabelText("Tags"), "shopping{Enter}")
    await user.click(screen.getByRole("button", { name: "Add" }))
    await waitFor(() => expect(onSaved).toHaveBeenCalledWith("l1", false))
    const [vault, input] = addItemIn.mock.calls[0]!
    expect(vault).toBe("v1")
    expect(input.fields).toEqual(
      expect.arrayContaining([
        { field_id: "url.0", action: "set", text: "shop.example.com", match_rule: "base_domain" },
        { field_id: "url.1", action: "set", text: "checkout.example.net", match_rule: "exact_host" },
        { field_id: "org.folder", action: "set", text: "f-clients" },
        { field_id: "org.tags", action: "set", text: "shopping" },
      ]),
    )
    expect(input.fields.some((f) => f.field_id === "org.favourite")).toBe(false)
  })
})
