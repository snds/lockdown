import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import type { ReactNode } from "react"

import { GateProvider } from "@/app/gate"
import { DEFAULT_SETTINGS, SettingsProvider } from "@/app/settings"
import { ClientProvider, type Client } from "@/core/client"
import { fakeClient } from "@/test/fake-client"
import { AliasSettings } from "./alias-settings"
import { BridgesProvider, type BridgesClient } from "./bridges-client"
import { fakeBridges } from "./fake-bridges"

function mount(ui: ReactNode, bridges: BridgesClient | null, over: Partial<Client> = {}) {
  const client = fakeClient({
    getSettings: () => Promise.resolve(DEFAULT_SETTINGS),
    approvalRoute: () => Promise.resolve({ route: "window", method: "this window" }),
    ...over,
  })
  return render(
    <ClientProvider value={client}>
      <SettingsProvider>
        <GateProvider>
          <BridgesProvider value={bridges}>{ui}</BridgesProvider>
        </GateProvider>
      </SettingsProvider>
    </ClientProvider>,
  )
}

const DUCK = {
  services: [{ id: "duck_duck_go", name: "DuckDuckGo", key_hint: "Copy the token.", needs_domain: false, takes_domain: false, self_hostable: false }],
  configured: [],
}

describe("the bridges views", () => {

  it("the alias settings says nothing where the host has no bridges client (the web vault)", () => {
    const { container } = mount(<AliasSettings onNotice={vi.fn()} />, null)
    expect(container.textContent).toBe("")
  })

  it("settings take an alias service's key as bytes in the Add service form, then switch monitoring on silently", async () => {
    const aliasServiceAdd = vi.fn((_input: { service: string; key: number[] }) => Promise.resolve("k1"))
    const aliasMonitoringSet = vi.fn((on: boolean) => Promise.resolve({ enabled: on, addresses: ["a@duck.com"], has_account: true }))
    const notices: string[] = []
    const bridges = fakeBridges({
      aliasOverview: () =>
        Promise.resolve({
          services: [{ id: "duck_duck_go", name: "DuckDuckGo", key_hint: "Copy the token.", needs_domain: false, takes_domain: false, self_hostable: false }],
          configured: [],
        }),
      aliasMonitoring: () => Promise.resolve({ enabled: false, addresses: ["a@duck.com"], has_account: true }),
      aliasServiceAdd,
      aliasMonitoringSet,
    })
    mount(<AliasSettings onNotice={(t) => notices.push(t)} />, bridges)
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "Add service" }))
    const form = await screen.findByRole("dialog")
    // A service that takes no domain and can't be self-hosted asks for neither.
    expect(within(form).queryByLabelText(/Your domain/)).toBeNull()
    expect(within(form).queryByLabelText(/Your own server/)).toBeNull()
    await user.type(within(form).getByLabelText("API key"), "tok-123")
    await user.click(within(form).getByRole("button", { name: "Set it up" }))
    await waitFor(() => expect(aliasServiceAdd).toHaveBeenCalled())
    const sent = aliasServiceAdd.mock.calls[0]?.[0]
    expect(sent?.service).toBe("duck_duck_go")
    // The key went as bytes, and those bytes were wiped after the call.
    expect(sent?.key.every((b) => b === 0)).toBe(true)
    // The form closed, and the field with it.
    await waitFor(() => expect(screen.queryByLabelText("API key")).toBeNull())
    expect(notices).toEqual(["DuckDuckGo's key is kept as a protected item"])
    await user.click(screen.getByRole("switch", { name: "Watch my aliases for data breaches" }))
    expect(aliasMonitoringSet).toHaveBeenCalledWith(true)
    await waitFor(() => expect(screen.getByRole("switch", { name: "Watch my aliases for data breaches" })).toBeChecked())
    expect(notices).toHaveLength(1)
  })

  it("an empty key is refused under the field, and nothing is sent", async () => {
    const aliasServiceAdd = vi.fn(() => Promise.resolve("k1"))
    mount(<AliasSettings onNotice={vi.fn()} />, fakeBridges({ aliasOverview: () => Promise.resolve(DUCK), aliasMonitoring: () => Promise.resolve({ enabled: false, addresses: [], has_account: true }), aliasServiceAdd }))
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "Add service" }))
    await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Set it up" }))
    expect(await screen.findByText("Paste the service's API key first.")).toBeInTheDocument()
    expect(aliasServiceAdd).not.toHaveBeenCalled()
  })

  it("lists the services set up; removing one asks first and moves its key to Recently deleted", async () => {
    const trash = vi.fn(() => Promise.resolve())
    const bridges = fakeBridges({
      aliasOverview: () =>
        Promise.resolve({
          services: [],
          configured: [{ item_id: "i-relay", title: "Relaymail", service: "relaymail", service_name: "Relaymail", domain: "relay.example", base: null, made: 12 }],
        }),
    })
    mount(<AliasSettings onNotice={vi.fn()} />, bridges, { trash })
    const user = userEvent.setup()
    const row = await screen.findByRole("group", { name: "Relaymail" })
    expect(row).toHaveTextContent("12 aliases made")
    expect(row).toHaveTextContent("relay.example")
    await user.click(within(row).getByRole("button", { name: /Remove/ }))
    const question = await screen.findByRole("dialog")
    expect(question).toHaveTextContent("Its key goes to Recently deleted.")
    expect(trash).not.toHaveBeenCalled()
    await user.click(within(question).getByRole("button", { name: "Remove" }))
    await waitFor(() => expect(trash).toHaveBeenCalledWith("i-relay"))
  })

  it("watching is disabled with its reason when there is no account (#37)", async () => {
    const aliasMonitoringSet = vi.fn((on: boolean) => Promise.resolve({ enabled: on, addresses: [], has_account: false }))
    mount(<AliasSettings onNotice={vi.fn()} />, fakeBridges({ aliasOverview: () => Promise.resolve(DUCK), aliasMonitoring: () => Promise.resolve({ enabled: false, addresses: [], has_account: false }), aliasMonitoringSet }))
    const user = userEvent.setup()
    const toggle = await screen.findByRole("switch", { name: "Watch my aliases for data breaches" })
    expect(screen.getByText("This needs an account.")).toBeInTheDocument()
    expect(toggle).toHaveAccessibleDescription(/This needs an account\./)
    await user.click(toggle)
    expect(aliasMonitoringSet).not.toHaveBeenCalled()
    expect(toggle).not.toBeChecked()
  })
})
