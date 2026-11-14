import { render, screen } from "@testing-library/react"
import type { ReactNode } from "react"

import { GateProvider } from "@/app/gate"
import { DEFAULT_SETTINGS, SettingsProvider } from "@/app/settings"
import { ClientProvider, type Client } from "@/core/client"
import { fakeClient } from "@/test/fake-client"
import { BridgesProvider, type BridgesClient } from "./bridges-client"
import { ClockBanner } from "./clock"
import { fakeBridges } from "./fake-bridges"

const NOW = Math.floor(Date.now() / 1000)

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
  it("the clock line says nothing where the host has no bridges client (the web vault)", () => {
    const { container } = mount(<ClockBanner />, null)
    expect(container.textContent).toBe("")
  })

  it("the clock line shows only when the clocks disagree", async () => {
    const clock = { checked_at: NOW, offset_secs: 45, verdict: "corrected" as const, correcting: true, words: "This computer's clock is 45 seconds behind your server's." }
    mount(<ClockBanner />, fakeBridges({ clockStatus: () => Promise.resolve(null), clockCheck: () => Promise.resolve(clock) }))
    expect(await screen.findByText(/45 seconds behind/)).toBeInTheDocument()
    expect(screen.getByRole("button", { name: "Clock settings" })).toBeInTheDocument()
  })
})
