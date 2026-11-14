import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import type { ReactNode } from "react"
import { fakeDepth } from "@/test/fake-depth"

import { AddDevice } from "./add-device"
import { DepthClientProvider, type DepthClient, type LinkView } from "./depth-client"

function mount(depth: DepthClient, ui: ReactNode) {
  return render(<DepthClientProvider value={depth}>{ui}</DepthClientProvider>)
}

const SLOW = { timeout: 4000 }

const waiting: LinkView = { state: "waiting", code: "7KQ2M-9XH4P", qr: "lockdown:link:v1?c=7KQ2M9XH4P", qr_rows: ["111", "101", "111"], expires_at: 1 }

describe("adding a device", () => {
  it("shows the code, then the digits, and approves through the gate", async () => {
    const linkPoll = vi.fn(() => Promise.resolve<LinkView>({ ...waiting, state: "compare", sas: "123 456", device_name: "Phone" }))
    const linkApprove = vi.fn(() => Promise.resolve(["Phone is now one of your devices."]))
    const onNotice = vi.fn()
    const onChanged = vi.fn()
    mount(fakeDepth({ linkOffer: () => Promise.resolve(waiting), linkPoll, linkApprove }), <AddDevice onNotice={onNotice} onChanged={onChanged} />)
    const user = userEvent.setup()
    await user.click(screen.getByRole("button", { name: "Add a device" }))
    expect(await screen.findByLabelText("Device code")).toHaveTextContent("7KQ2M-9XH4P")
    expect(screen.getByRole("img", { name: "Device link QR code" })).toBeInTheDocument()
    expect(await screen.findByLabelText("Digits to compare", {}, SLOW)).toHaveTextContent("123 456")
    await user.click(screen.getByRole("button", { name: "Approve" }))
    await waitFor(() => expect(onNotice).toHaveBeenCalledWith("Phone is now one of your devices."))
    expect(linkApprove).toHaveBeenCalledWith({})
    expect(onChanged).toHaveBeenCalled()
  })

  it("declines when the digits don't match", async () => {
    const linkDecline = vi.fn(() => Promise.resolve())
    mount(
      fakeDepth({ linkOffer: () => Promise.resolve(waiting), linkPoll: () => Promise.resolve<LinkView>({ ...waiting, state: "compare", sas: "111 111" }), linkDecline }),
      <AddDevice onNotice={() => {}} onChanged={() => {}} />,
    )
    const user = userEvent.setup()
    await user.click(screen.getByRole("button", { name: "Add a device" }))
    await screen.findByLabelText("Digits to compare", {}, SLOW)
    await user.click(screen.getByRole("button", { name: "They don't match" }))
    expect(linkDecline).toHaveBeenCalled()
  })
})
