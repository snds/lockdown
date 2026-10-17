import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useState } from "react"

import { GateProvider } from "@/app/gate"
import { SettingsProvider } from "@/app/settings"
import { TemplatesProvider } from "@/app/templates"
import { ClientProvider, type Client } from "@/core/client"
import type { GateInput, ItemSummary } from "@/core/types"
import { fakeClient } from "@/test/fake-client"
import { TEMPLATES } from "@/test/templates-fixture"

import { matches, searchWords } from "./palette-model"
import { QuickPalette } from "./quick-palette"

const row = (over: Partial<ItemSummary> & Pick<ItemSummary, "id" | "kind" | "title">): ItemSummary => ({
  state: "live",
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
  ...over,
})

const ROWS: ItemSummary[] = [
  row({ id: "l1", kind: "login", title: "Forum", site: "https://forum.example.com/login", username: "me", subtitle: "me", copy: { field_id: "pw", how: "secret", label: "password" } }),
  row({ id: "w1", kind: "wi-fi", title: "Home", subtitle: "home-net", copy: { field_id: "wifi.passphrase", how: "secret", label: "Wi-Fi password" } }),
  row({ id: "c1", kind: "code", title: "Bank code", copy: { field_id: "code.0", how: "code", label: "code" } }),
  row({ id: "j1", kind: "crypto-wallet", title: "Cold wallet", crown_jewel: true, copy: null }),
  row({ id: "t1", kind: "note", title: "Old plans", state: "trashed" }),
]

function Harness({ client, onOpen, onNotice }: { client: Client; onOpen: (id: string) => void; onNotice: (t: string) => void }) {
  const [open, setOpen] = useState(false)
  return (
    <ClientProvider value={client}>
      <SettingsProvider>
        <TemplatesProvider>
          <GateProvider>
            <p>window</p>
            <QuickPalette open={open} onOpenChange={setOpen} rows={ROWS} onOpen={onOpen} onNotice={onNotice} />
          </GateProvider>
        </TemplatesProvider>
      </SettingsProvider>
    </ClientProvider>
  )
}

describe("the quick-access palette (D111)", () => {
  it("finds by title, username, website host and kind, from the summaries only", () => {
    const words = (id: string) => searchWords(ROWS.find((r) => r.id === id)!, "")
    expect(matches("forum.example", words("l1"))).toBe(true)
    expect(matches("me forum", words("l1"))).toBe(true)
    expect(matches("home-net", words("w1"))).toBe(true)
    expect(matches("wi-fi", words("w1"))).toBe(true)
    expect(matches("cold", words("j1"))).toBe(true)
    expect(matches("login", words("w1"))).toBe(false)
  })

  it("opens on Ctrl+K, filters, opens an item on Enter, and leaves trashed items out", async () => {
    const onOpen = vi.fn()
    render(<Harness client={fakeClient({ templates: () => Promise.resolve(TEMPLATES) })} onOpen={onOpen} onNotice={() => {}} />)
    const user = userEvent.setup()
    await user.keyboard("{Control>}k{/Control}")
    const input = await screen.findByPlaceholderText("Search titles, usernames, websites, kinds")
    expect(screen.queryByText("Old plans")).not.toBeInTheDocument()
    await user.type(input, "home")
    await waitFor(() => expect(screen.queryByText("Forum")).not.toBeInTheDocument())
    expect(screen.getByText("Home")).toBeInTheDocument()
    await user.keyboard("{Enter}")
    expect(onOpen).toHaveBeenCalledWith("w1")
  })

  it("Ctrl+Enter copies the highlighted result's password through the core, by id and field", async () => {
    const copySecret = vi.fn<Client["copySecret"]>(() => Promise.resolve(30))
    const onNotice = vi.fn()
    render(<Harness client={fakeClient({ templates: () => Promise.resolve(TEMPLATES), copySecret })} onOpen={() => {}} onNotice={onNotice} />)
    const user = userEvent.setup()
    await user.keyboard("{Control>}k{/Control}")
    await user.type(await screen.findByPlaceholderText(/Search titles/), "forum")
    await user.keyboard("{Control>}{Enter}{/Control}")
    await waitFor(() => expect(copySecret).toHaveBeenCalledWith("l1", "pw", 30, {}))
    expect(onNotice).toHaveBeenCalledWith("Copied the password. The clipboard clears in 30 seconds.")
  })

  it("a crown jewel's copy asks first (the core's gate)", async () => {
    const copySecret = vi.fn<Client["copySecret"]>((_id, _f, _s, gate?: GateInput) =>
      gate?.confirmed ? Promise.resolve(30) : Promise.reject({ code: "approval_needed", detail: "window", message: "confirm in this window to copy this protected value" }),
    )
    const rows = ROWS.map((r): ItemSummary => (r.id === "j1" ? { ...r, copy: { field_id: "seed", how: "secret", label: "recovery phrase" } } : r))
    const onNotice = vi.fn()
    function Many() {
      const [open, setOpen] = useState(true)
      return <QuickPalette open={open} onOpenChange={setOpen} rows={rows} onOpen={() => {}} onNotice={onNotice} />
    }
    render(
      <ClientProvider value={fakeClient({ templates: () => Promise.resolve(TEMPLATES), copySecret, approvalRoute: () => Promise.resolve({ route: "window", method: "window" }) })}>
        <SettingsProvider>
          <TemplatesProvider>
            <GateProvider>
              <Many />
            </GateProvider>
          </TemplatesProvider>
        </SettingsProvider>
      </ClientProvider>,
    )
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "Copy recovery phrase" }))
    expect(await screen.findByRole("heading", { name: "Are you sure?" })).toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: "Confirm" }))
    await waitFor(() => expect(copySecret).toHaveBeenLastCalledWith("j1", "seed", 30, { confirmed: true }))
    expect(onNotice).toHaveBeenCalledWith("Copied the recovery phrase. The clipboard clears in 30 seconds.")
  })
})
