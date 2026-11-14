import { render, screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import type { ReactNode } from "react"

import { GateProvider } from "@/app/gate"
import { DEFAULT_SETTINGS, SettingsProvider } from "@/app/settings"
import { ClientProvider, type Client } from "@/core/client"
import { fakeClient } from "@/test/fake-client"
import { BridgesProvider, type BridgesClient } from "./bridges-client"
import { EmergencySheetButton } from "./emergency-sheet"
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

  it("the emergency sheet says nothing where the host has no bridges client (the web vault)", () => {
    const { container } = mount(<EmergencySheetButton onNotice={vi.fn()} />, null)
    expect(container.textContent).toBe("")
  })

  it("the emergency sheet opens as a page to print", async () => {
    const bridges = fakeBridges({
      emergencySheet: () =>
        Promise.resolve({
          made_at: NOW,
          title: "Lockdown emergency sheet",
          intro: "Keep it with your recovery kit.",
          sections: [{ heading: "Your account", lines: ["Your Lockdown account is me@example.com."] }],
          blanks: ["Where I keep my recovery kit:"],
        }),
    })
    mount(<EmergencySheetButton onNotice={vi.fn()} />, bridges)
    const user = userEvent.setup()
    await user.click(screen.getByRole("button", { name: "Emergency sheet" }))
    const sheet = await screen.findByRole("article", { name: "Emergency sheet" })
    expect(within(sheet).getByText("Your account")).toBeInTheDocument()
    expect(within(sheet).getByText("Where I keep my recovery kit:")).toBeInTheDocument()
    // Printed alone, without the app behind it (index.css).
    expect(sheet.closest("[data-print-page]")).not.toBeNull()
    const print = vi.spyOn(window, "print").mockImplementation(() => {})
    await user.click(screen.getByRole("button", { name: "Print" }))
    expect(print).toHaveBeenCalled()
  })
})
