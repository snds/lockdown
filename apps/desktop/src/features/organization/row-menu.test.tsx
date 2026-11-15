import { act, render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useState, type ReactNode } from "react"

import { TooltipProvider } from "@unicron/ui"

import { GateProvider } from "@/app/gate"
import type { Nav } from "@/app/places"
import { SettingsProvider } from "@/app/settings"
import { AppShell } from "@/app/shell"
import { TemplatesProvider } from "@/app/templates"
import { ClientProvider, type Client } from "@/core/client"
import type { FieldTemplateView, ItemSummary, Templates, VaultView } from "@/core/types"
import { SharingClientProvider } from "@/features/sharing/sharing-client"
import { VaultView as Vault } from "@/features/vault/vault-view"
import { fakeClient } from "@/test/fake-client"
import { fakeSharing } from "@/test/fake-sharing"
import { TEMPLATES } from "@/test/templates-fixture"

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

const base: Partial<Client> = {
  templates: () => Promise.resolve(LOGIN_TEMPLATES),
  userTemplates: () => Promise.resolve([]),
  approvalRoute: () => Promise.resolve({ route: "window" as const, method: "window" }),
  securityReport: () => Promise.reject({ code: "refused", message: "no report here" }),
}

function setWindow(width: number, height: number) {
  Object.defineProperty(window, "innerWidth", { configurable: true, value: width })
  Object.defineProperty(window, "innerHeight", { configurable: true, value: height })
  act(() => {
    window.dispatchEvent(new Event("resize"))
  })
}

/** The real shell around the real list, as the app mounts them. */
function Harness({ rows, start = { area: "vault", view: "all" } }: { rows: ItemSummary[]; start?: Nav }) {
  const [nav, setNav] = useState<Nav>(start)
  return (
    <AppShell nav={nav} onNavigate={setNav} rows={rows} onLock={() => {}} onSaved={() => {}}>
      {nav.area === "vault" ? (
        <Vault view={nav.view} rows={rows} error={null} onChanged={() => {}} onNotice={() => {}} selected={null} onSelect={() => {}} onPalette={() => {}} scope={nav.scope} />
      ) : (
        <p>elsewhere</p>
      )}
    </AppShell>
  )
}

function mount(client: Client, ui: ReactNode) {
  return render(
    <ClientProvider value={client}>
      <TooltipProvider>
        <SettingsProvider>
          <TemplatesProvider>
            <GateProvider>
              <SharingClientProvider value={fakeSharing({})}>
                <VaultsProvider value={{ vaults: VAULTS, reload: () => {} }}>{ui}</VaultsProvider>
              </SharingClientProvider>
            </GateProvider>
          </TemplatesProvider>
        </SettingsProvider>
      </TooltipProvider>
    </ClientProvider>,
  )
}

const LIVE = ["Add to favorites", "Folder and tags…", "Family", "Archive", "History", "Move to Recently deleted"]

beforeEach(() => {
  window.localStorage.clear()
})

afterEach(() => {
  setWindow(1024, 768)
})

describe("a row's menu (brief V1, cross-app decisions §10)", () => {
  it.each([
    [390, 844],
    [800, 1024],
    [1440, 900],
  ])("offers every action at %i px from the row's ⋯", async (width, height) => {
    setWindow(width, height)
    mount(fakeClient(base), <Harness rows={[row("a"), row("b")]} />)
    const user = userEvent.setup({ pointerEventsCheck: 0 })
    await user.click(await screen.findByRole("button", { name: "More for a" }))
    const menu = await screen.findByRole("menu")
    // A phone's row swipes, and its swipe's FAVORITE heads the menu in place of Add to favorites (doc 72).
    const names = width < 600 ? ["Favorite", "More", ...LIVE.filter((n) => n !== "Add to favorites")] : LIVE
    for (const name of names) expect(within(menu).getByRole("menuitem", { name }), `${name} at ${width}`).toBeInTheDocument()
    expect(within(menu).getByText("Move to vault")).toBeInTheDocument()
  })

  it("opens the same menu from the Menu key and Shift+F10 on the row", async () => {
    setWindow(1440, 900)
    mount(fakeClient(base), <Harness rows={[row("a"), row("b")]} />)
    const user = userEvent.setup({ pointerEventsCheck: 0 })
    ;(await screen.findByRole("button", { name: "b" })).focus()
    await user.keyboard("{ContextMenu}")
    const keyed = await screen.findByRole("menu")
    for (const name of LIVE) expect(within(keyed).getByRole("menuitem", { name })).toBeInTheDocument()
    await user.keyboard("{Escape}")
    await waitFor(() => expect(screen.queryByRole("menu")).not.toBeInTheDocument())
    screen.getByRole("button", { name: "a" }).focus()
    await user.keyboard("{Shift>}{F10}{/Shift}")
    expect(await screen.findByRole("menu")).toBeInTheDocument()
  })

  it("stars and archives through the core", async () => {
    const organize = vi.fn<Client["organize"]>(() => Promise.resolve(2))
    const archive = vi.fn<Client["archive"]>(() => Promise.resolve())
    setWindow(1440, 900)
    mount(fakeClient({ ...base, organize, archive }), <Harness rows={[row("a")]} />)
    // The menu fades out after a pick; the next open doesn't wait for it.
    const user = userEvent.setup({ pointerEventsCheck: 0 })
    const open = async () => {
      await user.click(await screen.findByRole("button", { name: "More for a" }))
      return screen.findByRole("menu")
    }
    await user.click(within(await open()).getByRole("menuitem", { name: "Add to favorites" }))
    await waitFor(() => expect(organize).toHaveBeenCalledWith("a", { favourite: true }))
    await user.click(within(await open()).getByRole("menuitem", { name: "Archive" }))
    await waitFor(() => expect(archive).toHaveBeenCalledWith("a"))
  })

  it("moves to another vault and to Recently deleted through the core, asking nothing for the undoable one", async () => {
    const moveItem = vi.fn<Client["moveItem"]>(() => Promise.resolve("new-id"))
    const trash = vi.fn<Client["trash"]>(() => Promise.resolve())
    setWindow(1440, 900)
    mount(fakeClient({ ...base, moveItem, trash }), <Harness rows={[row("a")]} />)
    const user = userEvent.setup({ pointerEventsCheck: 0 })
    const open = async () => {
      await user.click(await screen.findByRole("button", { name: "More for a" }))
      return screen.findByRole("menu")
    }
    await user.click(within(await open()).getByRole("menuitem", { name: "Family" }))
    await waitFor(() => expect(moveItem).toHaveBeenCalledWith("a", "v2", {}))
    // Undoable, so no question: straight to Recently deleted.
    await user.click(within(await open()).getByRole("menuitem", { name: "Move to Recently deleted" }))
    await waitFor(() => expect(trash).toHaveBeenCalledWith("a"))
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument()
  })

  it("offers Restore and Delete for good in Recently deleted, and asks before deleting for good, Cancel first", async () => {
    const purge = vi.fn<Client["purge"]>(() => Promise.resolve())
    const restore = vi.fn<Client["restore"]>(() => Promise.resolve())
    setWindow(1440, 900)
    mount(fakeClient({ ...base, purge, restore }), <Harness rows={[row("gone", { state: "trashed", views: ["deleted"] })]} start={{ area: "vault", view: "deleted" }} />)
    const user = userEvent.setup({ pointerEventsCheck: 0 })
    await user.click(await screen.findByRole("button", { name: "More for gone" }))
    const menu = await screen.findByRole("menu")
    expect(within(menu).getAllByRole("menuitem").map((m) => m.textContent)).toEqual(["Restore", "History", "Delete for good"])
    await user.click(within(menu).getByRole("menuitem", { name: "Delete for good" }))
    const ask = await screen.findByRole("dialog", { name: 'Delete "gone" for good?' })
    await waitFor(() => expect(within(ask).getByRole("button", { name: "Cancel" })).toHaveFocus())
    await user.click(within(ask).getByRole("button", { name: "Delete for good" }))
    await waitFor(() => expect(purge).toHaveBeenCalledWith("gone", {}))
  })

  it("offers nothing on an item shared to view or to fill only", async () => {
    setWindow(1440, 900)
    mount(fakeClient(base), <Harness rows={[row("seen", { access: "view" }), row("filled", { access: "fill-only" }), row("mine")]} />)
    expect(await screen.findByRole("button", { name: "More for mine" })).toBeInTheDocument()
    expect(screen.queryByRole("button", { name: "More for seen" })).not.toBeInTheDocument()
    expect(screen.queryByRole("button", { name: "More for filled" })).not.toBeInTheDocument()
  })
})
