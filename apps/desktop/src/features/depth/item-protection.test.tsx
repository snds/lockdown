import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import type { ReactNode } from "react"
import { fakeDepth } from "@/test/fake-depth"
import { DepthClientProvider, type DepthClient, type ExposureView } from "./depth-client"
import { ItemProtection } from "./item-protection"

function mount(depth: DepthClient, ui: ReactNode) {
  return render(<DepthClientProvider value={depth}>{ui}</DepthClientProvider>)
}

describe("an item's protection", () => {
  const shop: ExposureView = { id: "i1", tier: "fill", source: "default", default_tier: "fill", reason: null, choices: ["app-only", "ask", "fill"] }

  it("keeps an item to the app, and lets a crown jewel into emergency access through the gate", async () => {
    const setExposure = vi.fn(() => Promise.resolve<ExposureView>({ ...shop, tier: "app-only", source: "chosen" }))
    const setLegacyOptIn = vi.fn(() => Promise.resolve(3))
    const onNotice = vi.fn()
    mount(
      fakeDepth({ exposure: () => Promise.resolve(shop), legacyOptIn: () => Promise.resolve(false), setExposure, setLegacyOptIn }),
      <ItemProtection itemId="i1" crownJewel onNotice={onNotice} />,
    )
    const user = userEvent.setup()
    await user.selectOptions(await screen.findByLabelText("Who can get its values"), "app-only")
    await waitFor(() => expect(setExposure).toHaveBeenCalledWith("i1", "app-only", {}))
    expect(onNotice).toHaveBeenCalledWith(expect.stringContaining("Keep to the app"))
    await user.click(screen.getByRole("switch", { name: "Include in emergency access" }))
    await waitFor(() => expect(setLegacyOptIn).toHaveBeenCalledWith("i1", true, {}))
  })

  it("going back to the default clears the choice", async () => {
    const setExposure = vi.fn(() => Promise.resolve(shop))
    mount(
      fakeDepth({ exposure: () => Promise.resolve<ExposureView>({ ...shop, tier: "ask", source: "chosen" }), setExposure }),
      <ItemProtection itemId="i1" crownJewel={false} onNotice={() => {}} />,
    )
    const user = userEvent.setup()
    await user.selectOptions(await screen.findByLabelText("Who can get its values"), "fill")
    await waitFor(() => expect(setExposure).toHaveBeenCalledWith("i1", null, {}))
    expect(screen.queryByLabelText("Include in emergency access")).not.toBeInTheDocument()
  })
})
