import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import type { ReactNode } from "react"

import { GateProvider } from "@/app/gate"
import { DEFAULT_SETTINGS, SettingsProvider } from "@/app/settings"
import { ClientProvider, type Client } from "@/core/client"
import { fakeClient } from "@/test/fake-client"
import { BridgesProvider, type BridgesClient } from "./bridges-client"
import { ClockSkew, ClockSetting } from "./clock"
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
    const { container } = mount(<ClockSkew />, null)
    expect(container.textContent).toBe("")
  })

  it("the clock line shows only when the clocks disagree", async () => {
    const clock = { checked_at: NOW, offset_secs: 45, verdict: "corrected" as const, correcting: true, words: "This computer's clock is 45 seconds behind your server's." }
    mount(<ClockSkew />, fakeBridges({ clockStatus: () => Promise.resolve(null), clockCheck: () => Promise.resolve(clock) }))
    expect(await screen.findByText(/45 seconds behind/)).toBeInTheDocument()
    expect(screen.getByRole("button", { name: "Clock settings" })).toBeInTheDocument()
  })

  it("the clock line is a compact Banner: warning while codes are corrected, critical when the clock is too far off (brief C1)", async () => {
    const corrected = { checked_at: NOW, offset_secs: 45, verdict: "corrected" as const, correcting: true, words: "This computer's clock is 45 seconds behind your server's." }
    const { unmount } = mount(<ClockSkew />, fakeBridges({ clockStatus: () => Promise.resolve(corrected) }))
    const line = await screen.findByRole("status", { name: "Clock" })
    expect(line).toHaveAttribute("data-slot", "banner")
    expect(line).toHaveAttribute("data-tone", "warning")
    unmount()
    const far = { ...corrected, offset_secs: 900, verdict: "too_far" as const, correcting: false, words: "This computer's clock is 15 minutes behind your server's." }
    mount(<ClockSkew />, fakeBridges({ clockStatus: () => Promise.resolve(far) }))
    const alert = await screen.findByRole("alert", { name: "Clock" })
    expect(alert).toHaveAttribute("data-tone", "critical")
    expect(alert).toHaveTextContent("15 minutes behind")
  })

  it("nothing shows while the clocks agree", async () => {
    const agree = { checked_at: NOW, offset_secs: 1, verdict: "in_step" as const, correcting: false, words: "Within 2 seconds." }
    const clockStatus = vi.fn(() => Promise.resolve(agree))
    const { container } = mount(<ClockSkew />, fakeBridges({ clockStatus }))
    await waitFor(() => expect(clockStatus).toHaveBeenCalled())
    expect(container.textContent).toBe("")
  })

  it("Settings → Codes: one Toggle Row that names its switch and saves the choice", async () => {
    const setSettings = vi.fn<Client["setSettings"]>((s) => Promise.resolve(s))
    mount(<ClockSetting />, fakeBridges({}), { setSettings })
    const toggle = await screen.findByRole("switch", { name: "Correct for a wrong clock" })
    expect(toggle.closest('[data-slot="toggle-row"]')).toBeInTheDocument()
    expect(toggle).toBeChecked()
    await userEvent.setup().click(toggle)
    await waitFor(() => expect(setSettings).toHaveBeenCalledWith(expect.objectContaining({ code_clock_correction: false })))
  })
})
