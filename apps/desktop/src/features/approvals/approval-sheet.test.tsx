import { act, render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { DeviceApproval, type PendingApproval } from "./approval-sheet"

const request = (over: Partial<PendingApproval>): PendingApproval => ({
  id: "r1",
  kind: "device",
  origin: "pairing",
  context: [
    { label: "What", value: "Enrol a new device" },
    { label: "Who", value: "Living-room laptop" },
    { label: "When", value: "just now" },
  ],
  expected: "42",
  digits: 2,
  ...over,
})

const approveButton = () => screen.queryByRole("button", { name: /Approve/ })

describe("the ApprovalSheet seam (D99, brief §6 rule 4)", () => {
  it("a device enrolment: the facts, the digits to match, Approve only after the match", async () => {
    const onApprove = vi.fn()
    render(<DeviceApproval request={request({})} onApprove={onApprove} onDeny={() => {}} onReport={() => {}} />)
    const dialog = await screen.findByRole("dialog")
    expect(dialog).toHaveTextContent("Approve a new device?")
    expect(dialog).toHaveTextContent("Living-room laptop")
    const cells = screen.getAllByRole("textbox", { name: /Digit \d of 2/ })
    expect(cells).toHaveLength(2)
    const user = userEvent.setup()
    await user.type(cells[0]!, "4")
    await user.type(cells[1]!, "2")
    // The actions arm half a second after open; a double click cannot land on Approve.
    await act(() => new Promise((r) => setTimeout(r, 600)))
    expect(approveButton()).toBeInTheDocument()
    await user.click(approveButton()!)
    expect(onApprove).toHaveBeenCalledWith("r1")
    expect(screen.getByRole("button", { name: "This wasn't me" })).toBeInTheDocument()
  })

  it("a device code that was not scanned from the screen in front of the person has no approve control at all", async () => {
    render(<DeviceApproval request={request({ kind: "device-code", origin: "push" })} onApprove={() => {}} onDeny={() => {}} onReport={() => {}} />)
    const dialog = await screen.findByRole("dialog")
    await act(() => new Promise((r) => setTimeout(r, 600)))
    expect(approveButton()).toBeNull()
    expect(dialog).toHaveTextContent("Device codes are approved only when you scan the screen in front of you.")
  })

  it("a scanned device code keeps the approve path", async () => {
    render(<DeviceApproval request={request({ kind: "device-code", origin: "scan" })} onApprove={() => {}} onDeny={() => {}} onReport={() => {}} />)
    const dialog = await screen.findByRole("dialog")
    expect(dialog).toHaveTextContent("Scanned from the screen in front of you.")
    expect(dialog).not.toHaveTextContent("Device codes are approved only when")
  })
})
