import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import type { ReactNode } from "react"

import { GateProvider } from "@/app/gate"
import { DEFAULT_SETTINGS, SettingsProvider } from "@/app/settings"
import { ClientProvider, type Client } from "@/core/client"
import { fakeClient } from "@/test/fake-client"
import { BridgesProvider, type BridgesClient } from "./bridges-client"
import { EmergencySheetRow } from "./emergency-sheet"
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

describe("the emergency sheet (brief T3)", () => {
  const SHEET = {
    made_at: NOW,
    title: "Lockdown emergency sheet",
    intro: "Keep it with your recovery kit.",
    sections: [{ heading: "Your account", lines: ["Your Lockdown account is me@example.com."] }],
    blanks: ["Where I keep my recovery kit:"],
  }

  it("says nothing where the host has no bridges client (the web vault)", () => {
    const { container } = mount(<EmergencySheetRow />, null)
    expect(container.textContent).toBe("")
  })

  it("is a row in Recovery whose OPEN shows the sheet as a page to print, on paper", async () => {
    mount(<EmergencySheetRow />, fakeBridges({ emergencySheet: () => Promise.resolve(SHEET) }))
    const user = userEvent.setup()
    expect(screen.getByText("One page for a future you. No secrets on it.")).toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: /^Open/ }))
    const dialog = await screen.findByRole("dialog", { name: "Lockdown emergency sheet" })
    expect(dialog).toHaveTextContent("Emergency sheet // no secrets on it")
    expect(dialog).toHaveTextContent("Keep it with your recovery kit.")
    const sheet = within(dialog).getByRole("article", { name: "Emergency sheet" })
    expect(within(sheet).getByRole("heading", { name: "Your account" })).toBeInTheDocument()
    expect(within(sheet).getByText("Where I keep my recovery kit:")).toBeInTheDocument()
    // Black on white in both themes: the stone roles, as unicron's QrCode scopes them.
    expect(sheet).toHaveClass("light")
    // Printed alone, without the app behind it (app-extension.css).
    expect(sheet.closest("[data-print-page]")).not.toBeNull()
    const print = vi.spyOn(window, "print").mockImplementation(() => {})
    await user.click(within(dialog).getByRole("button", { name: "Print" }))
    expect(print).toHaveBeenCalled()
    await user.click(within(dialog).getByRole("button", { name: "Done" }))
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull())
  })

  it("a sheet the core could not make is one critical line under the row, never a toast", async () => {
    mount(<EmergencySheetRow />, fakeBridges({ emergencySheet: () => Promise.reject({ code: "refused", message: "The vault is locked." }) }))
    await userEvent.setup().click(screen.getByRole("button", { name: /^Open/ }))
    expect(await screen.findByRole("alert")).toHaveTextContent("The vault is locked.")
    expect(screen.queryByRole("dialog")).toBeNull()
  })
})
