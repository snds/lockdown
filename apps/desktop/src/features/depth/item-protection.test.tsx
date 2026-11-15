import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import type { ReactNode } from "react"
import { fakeDepth } from "@/test/fake-depth"
import { DepthClientProvider, type DepthClient, type ExposureView } from "./depth-client"
import { EmergencyAccess, ItemProtection } from "./item-protection"

function mount(depth: DepthClient, ui: ReactNode) {
  return render(<DepthClientProvider value={depth}>{ui}</DepthClientProvider>)
}

describe("an item's protection (brief I4)", () => {
  const shop: ExposureView = { id: "i1", tier: "fill", source: "default", default_tier: "fill", reason: null, choices: ["app-only", "ask", "fill"] }

  it("WHO CAN GET ITS VALUES opens a picker of the tiers, each with its sentence, and keeps an item to the app", async () => {
    const setExposure = vi.fn(() => Promise.resolve<ExposureView>({ ...shop, tier: "app-only", source: "chosen" }))
    mount(fakeDepth({ exposure: () => Promise.resolve(shop), setExposure }), <ItemProtection itemId="i1" />)
    const row = await screen.findByRole("group", { name: "Who can get its values" })
    expect(row).toHaveTextContent("Fill freely")
    const user = userEvent.setup()
    await user.click(within(row).getByRole("button", { name: "Change" }))
    const picker = await screen.findByRole("radiogroup", { name: "Who can get its values" })
    expect(within(picker).getAllByRole("radio")).toHaveLength(3)
    expect(picker).toHaveTextContent("Never offered to a browser; only shown here.")
    await user.click(within(picker).getByRole("radio", { name: /^Keep to the app/ }))
    await waitFor(() => expect(setExposure).toHaveBeenCalledWith("i1", "app-only", {}))
    // Saving is silent: the row's value is the answer.
    await waitFor(() => expect(row).toHaveTextContent("Keep to the app"))
  })

  it("going back to the default clears the choice; the core's reason shows while it is the default", async () => {
    const setExposure = vi.fn(() => Promise.resolve<ExposureView>({ ...shop, tier: "ask", source: "high-risk", default_tier: "ask", reason: "It signs in to a bank." }))
    mount(
      fakeDepth({ exposure: () => Promise.resolve<ExposureView>({ ...shop, tier: "app-only", source: "chosen", default_tier: "ask", reason: "It signs in to a bank." }), setExposure }),
      <ItemProtection itemId="i1" />,
    )
    const row = await screen.findByRole("group", { name: "Who can get its values" })
    expect(row).not.toHaveTextContent("It signs in to a bank.")
    const user = userEvent.setup()
    await user.click(within(row).getByRole("button", { name: "Change" }))
    await user.click(await screen.findByRole("radio", { name: /^Ask each time/ }))
    await waitFor(() => expect(setExposure).toHaveBeenCalledWith("i1", null, {}))
    await waitFor(() => expect(row).toHaveTextContent("It signs in to a bank."))
  })

  it("is not drawn for someone who cannot change the item", async () => {
    const exposure = vi.fn(() => Promise.resolve(shop))
    mount(fakeDepth({ exposure }), <ItemProtection itemId="i1" canEdit={false} />)
    await waitFor(() => expect(exposure).toHaveBeenCalled())
    expect(screen.queryByRole("group", { name: "Who can get its values" })).not.toBeInTheDocument()
  })

  it("lets a crown jewel into emergency access through the gate", async () => {
    const setLegacyOptIn = vi.fn(() => Promise.resolve(3))
    mount(fakeDepth({ legacyOptIn: () => Promise.resolve(false), setLegacyOptIn }), <EmergencyAccess itemId="i1" />)
    const user = userEvent.setup()
    await user.click(await screen.findByRole("switch", { name: "Include in emergency access" }))
    await waitFor(() => expect(setLegacyOptIn).toHaveBeenCalledWith("i1", true, {}))
    await waitFor(() => expect(screen.getByRole("switch", { name: "Include in emergency access" })).toBeChecked())
  })
})
