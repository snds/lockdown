import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import type { ReactNode } from "react"

import { GateProvider } from "@/app/gate"
import { DEFAULT_SETTINGS, SettingsProvider } from "@/app/settings"
import { ClientProvider, type Client } from "@/core/client"
import type { GateInput } from "@/core/types"
import { fakeClient } from "@/test/fake-client"

import { AliasField } from "./alias-field"
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

  it("the alias field says nothing where the host has no bridges client (the web vault)", () => {
    const { container } = mount(<AliasField site="https://x.example" onAddress={vi.fn()} />, null)
    expect(container.textContent).toBe("")
  })

  it("an alias comes through the gate into the user name", async () => {
    const aliasGenerate = vi.fn((_item: string, _site: string, gate?: GateInput) =>
      gate?.confirmed
        ? Promise.resolve({ address: "shop.x1@simplelogin.com", service: "simple_login", service_name: "SimpleLogin", monitoring_problem: null })
        : Promise.reject({ code: "approval_needed", message: "confirm in this window to use your alias service's key", detail: "window" }),
    )
    const bridges = fakeBridges({
      aliasOverview: () =>
        Promise.resolve({
          services: [],
          configured: [{ item_id: "k1", title: "SimpleLogin API key", service: "simple_login", service_name: "SimpleLogin", domain: null, base: null, made: 0 }],
        }),
      aliasGenerate,
    })
    const onAddress = vi.fn()
    mount(<AliasField site="https://Shop.example/signup" onAddress={onAddress} />, bridges)
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "New alias from SimpleLogin" }))
    const confirm = await screen.findByRole("button", { name: /confirm/i })
    await user.click(confirm)
    await waitFor(() => expect(onAddress).toHaveBeenCalledWith("shop.x1@simplelogin.com"))
    expect(aliasGenerate).toHaveBeenLastCalledWith("k1", "shop.example", expect.objectContaining({ confirmed: true }))
  })
})
