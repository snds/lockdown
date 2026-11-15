import { render, screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import type { ReactNode } from "react"

import { GateProvider } from "@/app/gate"
import { DEFAULT_SETTINGS, SettingsProvider } from "@/app/settings"
import { ClientProvider, type Client } from "@/core/client"
import { fakeClient } from "@/test/fake-client"
import { BridgesProvider, type BridgesClient } from "./bridges-client"
import { SetupGuideBlock } from "./setup-guide"
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

describe("the set-up guide, read in a finding", () => {
  it("says nothing where the host has no bridges client (the web vault)", () => {
    const { container } = mount(<SetupGuideBlock itemId="i1" onNotice={vi.fn()} />, null)
    expect(container.textContent).toBe("")
  })

  it("says nothing for a site with no guide", async () => {
    const setupGuide = vi.fn(() => Promise.resolve(null))
    const { container } = mount(<SetupGuideBlock itemId="i1" onNotice={vi.fn()} />, fakeBridges({ setupGuide }))
    await vi.waitFor(() => expect(setupGuide).toHaveBeenCalledWith("i1"))
    await vi.waitFor(() => expect(container.textContent).toBe(""))
  })

  it("says what the site offers, quotes its note, lists the steps and opens its pages through the shell", async () => {
    const openSite = vi.fn(() => Promise.resolve())
    const bridges = fakeBridges({
      setupGuide: () =>
        Promise.resolve({
          site: "Harbor Health",
          methods: ["an authenticator app", "a security key"],
          app_codes: true,
          passkeys: null,
          setup_url: "https://harbor.example/2fa",
          passkey_url: null,
          recovery_url: "https://harbor.example/recover",
          notes: "Codes by text message are for US numbers only.",
          site_apps: [],
          steps: ["Open the set-up page.", "Scan the code with Lockdown."],
          attribution: [{ name: "2fa.directory", licence: "MIT", url: "https://2fa.directory" }],
        }),
    })
    mount(<SetupGuideBlock itemId="i1" onNotice={vi.fn()} />, bridges, { openSite })
    const block = await screen.findByRole("region", { name: "Set-up guide" })
    expect(within(block).getByText("Harbor Health offers an authenticator app, a security key.")).toBeInTheDocument()
    expect(within(block).getByText(/US numbers only/)).toBeInTheDocument()
    expect(within(within(block).getByRole("list", { name: "Set-up steps" })).getAllByRole("listitem")).toHaveLength(2)
    await userEvent.setup().click(within(block).getByRole("button", { name: "Account recovery page" }))
    expect(openSite).toHaveBeenCalledWith("https://harbor.example/recover")
  })
})
