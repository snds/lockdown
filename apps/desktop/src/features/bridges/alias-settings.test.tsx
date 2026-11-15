import { render, screen, waitFor } from "@testing-library/react"
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

describe("the bridges views", () => {

  it("the alias settings says nothing where the host has no bridges client (the web vault)", () => {
    const { container } = mount(<AliasSettings onNotice={vi.fn()} />, null)
    expect(container.textContent).toBe("")
  })

  it("settings take an alias service's key as bytes and switch monitoring on", async () => {
    const aliasServiceAdd = vi.fn((_input: { service: string; key: number[] }) => Promise.resolve("k1"))
    const aliasMonitoringSet = vi.fn((on: boolean) => Promise.resolve({ enabled: on, addresses: ["a@duck.com"], has_account: true }))
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
    mount(<AliasSettings onNotice={vi.fn()} />, bridges)
    const user = userEvent.setup()
    await user.type(await screen.findByLabelText("API key"), "tok-123")
    await user.click(screen.getByRole("button", { name: "Set it up" }))
    await waitFor(() => expect(aliasServiceAdd).toHaveBeenCalled())
    const sent = aliasServiceAdd.mock.calls[0]?.[0]
    expect(sent?.service).toBe("duck_duck_go")
    // The key went as bytes, and those bytes were wiped after the call.
    expect(sent?.key.every((b) => b === 0)).toBe(true)
    expect((screen.getByLabelText("API key") as HTMLInputElement).value).toBe("")
    await user.click(screen.getByRole("switch", { name: "Watch my aliases for data breaches" }))
    expect(aliasMonitoringSet).toHaveBeenCalledWith(true)
  })
})
