import { render, screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import type { ReactNode } from "react"

import { GateProvider } from "@/app/gate"
import { DEFAULT_SETTINGS, SettingsProvider } from "@/app/settings"
import { ClientProvider, type Client } from "@/core/client"
import { fakeClient } from "@/test/fake-client"
import { BridgesProvider, type BridgesClient, type FederationMap } from "./bridges-client"
import { FederationSection } from "./federation"
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

  it("the map says nothing where the host has no bridges client (the web vault)", () => {
    const { container } = mount(<FederationSection onOpenItem={vi.fn()} onChanged={vi.fn()} onNotice={vi.fn()} />, null)
    expect(container.textContent).toBe("")
  })

  it("the map shows what falls with each provider, what to fix, and asks about what the browser saw", async () => {
    const map: FederationMap = {
      providers: [
        {
          id: "google",
          name: "Google",
          account: { item_id: "g", title: "Google", username: "sean@gmail.com", host: "accounts.google.com", has_password: true, second_step: false, crown_jewel: false },
          candidates: [],
          sites: [{ item_id: "s1", title: "Shop", username: "", host: "shop.example", has_password: false, second_step: false, crown_jewel: false }],
          warnings: ["Your Google account has no two-step sign-in saved in Lockdown."],
        },
      ],
      pending: [{ id: 7, provider: "apple", provider_name: "Apple", host: "news.example", logins: [], confirmed: false }],
    }
    const federationAccept = vi.fn((_p: number, _i: string | null) => Promise.resolve("new"))
    const bridges = fakeBridges({ federationMap: () => Promise.resolve(map), federationAccept })
    const onOpenItem = vi.fn()
    mount(<FederationSection onOpenItem={onOpenItem} onChanged={vi.fn()} onNotice={vi.fn()} />, bridges)
    const google = await screen.findByRole("region", { name: "Google" })
    expect(within(google).getByText("1 site")).toBeInTheDocument()
    expect(within(google).getByText(/no two-step sign-in/)).toBeInTheDocument()
    const user = userEvent.setup()
    await user.click(within(google).getByRole("button", { name: /Shop · shop.example · no password of its own/ }))
    expect(onOpenItem).toHaveBeenCalledWith("s1")
    expect(screen.getByText("Did you sign in to news.example with Apple?")).toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: "Yes, save a login" }))
    expect(federationAccept).toHaveBeenCalledWith(7, null)
  })
})
