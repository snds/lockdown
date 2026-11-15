import { act, render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { DeviceApproval, DeviceRequests, type PendingApproval } from "./approval-sheet"
import { requestLine } from "./requests"
import { ARMED_MS, RequestHost } from "./test-host"

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

describe("a device request on the needs-you (brief X3 row 3, A1)", () => {
  it("waits as DEVICE REQUEST with REVIEW only; the review never opens by itself", async () => {
    render(
      <RequestHost>
        <DeviceRequests requests={[request({})]} onApprove={() => {}} onDeny={() => {}} onReport={() => {}} />
      </RequestHost>,
    )
    const block = document.querySelector<HTMLElement>('[data-slot="needs-you"]')!
    expect(block).toHaveTextContent(/^Device request/)
    expect(block).toHaveTextContent("Enrol a new device // Living-room laptop")
    expect(within(block).getAllByRole("button").map((b) => b.textContent)).toEqual(["Review"])
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument()
    await userEvent.setup().click(within(block).getByRole("button", { name: "Review" }))
    expect(await screen.findByRole("dialog")).toHaveTextContent("Approve a new device?")
  })

  it("closing the review sends no answer, and the request waits again", async () => {
    const onDeny = vi.fn()
    const onReport = vi.fn()
    render(
      <RequestHost>
        <DeviceRequests requests={[request({})]} onApprove={() => {}} onDeny={onDeny} onReport={onReport} />
      </RequestHost>,
    )
    const user = userEvent.setup()
    await user.click(screen.getByRole("button", { name: "Review" }))
    await screen.findByRole("dialog")
    await act(() => new Promise((r) => setTimeout(r, ARMED_MS)))
    await user.click(screen.getByRole("button", { name: "Close" }))
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument())
    expect(onDeny).not.toHaveBeenCalled()
    expect(onReport).not.toHaveBeenCalled()
    expect(await screen.findByRole("button", { name: "Review" })).toBeInTheDocument()
  })

  it("a pushed device code has no approve control from the needs-you either", async () => {
    render(
      <RequestHost>
        <DeviceRequests requests={[request({ kind: "device-code", origin: "push" })]} onApprove={() => {}} onDeny={() => {}} onReport={() => {}} />
      </RequestHost>,
    )
    await userEvent.setup().click(screen.getByRole("button", { name: "Review" }))
    const dialog = await screen.findByRole("dialog")
    await act(() => new Promise((r) => setTimeout(r, ARMED_MS)))
    expect(within(dialog).queryByRole("button", { name: /Approve/ })).toBeNull()
  })

  it("names the request by its own words, else its what and who", () => {
    expect(requestLine(request({ title: "Sam's new phone wants to join" }))).toBe("Sam's new phone wants to join")
    expect(requestLine(request({ context: [] }))).toBe("A device asks to be approved.")
  })
})
