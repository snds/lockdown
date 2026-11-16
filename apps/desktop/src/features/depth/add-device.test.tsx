import { act, render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import type { ReactNode } from "react"
import { fakeDepth } from "@/test/fake-depth"
import { fakeClient } from "@/test/fake-client"
import { ClientProvider, type Client } from "@/core/client"
import { ARMED_MS } from "@/features/approvals/requests"

import { ADD_DEVICE_SENTENCE, AddDevice } from "./add-device"
import { DepthClientProvider, type DepthClient, type LinkView } from "./depth-client"

function mount(depth: DepthClient, ui: ReactNode, client: Client = fakeClient({})) {
  return render(
    <ClientProvider value={client}>
      <DepthClientProvider value={depth}>{ui}</DepthClientProvider>
    </ClientProvider>,
  )
}

const SLOW = { timeout: 4000 }
const armed = () => act(() => new Promise((r) => setTimeout(r, ARMED_MS)))

const inTen = () => Math.floor(Date.now() / 1000) + 600
const waiting = (): LinkView => ({
  state: "waiting",
  code: "7KQ2M-9XH4P",
  qr: "lockdown:link:v1?c=7KQ2M9XH4P",
  qr_rows: ["111", "101", "111"],
  expires_at: inTen(),
})

describe("adding a device (brief T2, A1)", () => {
  it("shows the QR code, the code and the time left, then the six digits, and approves through the gate", async () => {
    const linkPoll = vi.fn(() =>
      Promise.resolve<LinkView>({
        ...waiting(),
        state: "compare",
        sas: "123 456",
        device_name: "Phone",
      }),
    )
    const linkApprove = vi.fn(() => Promise.resolve(["Phone is now one of your devices."]))
    const onNotice = vi.fn()
    const onChanged = vi.fn()
    mount(
      fakeDepth({
        linkOffer: () => Promise.resolve(waiting()),
        linkPoll,
        linkApprove,
      }),
      <AddDevice onNotice={onNotice} onChanged={onChanged} />,
    )
    const user = userEvent.setup()
    await user.click(screen.getByRole("button", { name: "Add device" }))
    const offer = await screen.findByRole("dialog", { name: "Add a device" })
    expect(offer).toHaveTextContent(ADD_DEVICE_SENTENCE)
    expect(within(offer).getByLabelText("Device code")).toHaveTextContent("7KQ2M-9XH4P")
    // unicron's QrCode, from the core's rows: one path of dark modules on a light square.
    const qr = within(offer).getByRole("img", {
      name: "QR code to add a device",
    })
    expect(qr.querySelectorAll("path")).toHaveLength(1)
    expect(
      within(offer).getByRole("progressbar", {
        name: /^Time left: (09:\d{2}|10:00)$/,
      }),
    ).toBeInTheDocument()
    expect(offer).toHaveTextContent(/Device link \/\/ (09:\d{2}|10:00) left/i)

    // The new device joined: the compare review on the ApprovalSheet.
    const review = await screen.findByRole("dialog", { name: "Add Phone?" }, SLOW)
    expect(within(review).getByLabelText("Digits to compare")).toHaveTextContent("123 456")
    expect(review).toHaveTextContent(/New device \/\/ \d{2}:\d{2} left/i)
    await armed()
    await user.click(within(review).getByRole("button", { name: "They match" }))
    await waitFor(() => expect(onNotice).toHaveBeenCalledWith("Phone is now one of your devices."))
    expect(linkApprove).toHaveBeenCalledWith({})
    expect(onChanged).toHaveBeenCalled()
  })

  it("declines when the digits don't match", async () => {
    const linkDecline = vi.fn(() => Promise.resolve())
    mount(
      fakeDepth({
        linkOffer: () => Promise.resolve(waiting()),
        linkPoll: () =>
          Promise.resolve<LinkView>({
            ...waiting(),
            state: "compare",
            sas: "111 111",
          }),
        linkDecline,
      }),
      <AddDevice onNotice={() => {}} onChanged={() => {}} />,
    )
    const user = userEvent.setup()
    await user.click(screen.getByRole("button", { name: "Add device" }))
    const review = await screen.findByRole("dialog", { name: "Add the new device?" }, SLOW)
    await armed()
    await user.click(within(review).getByRole("button", { name: "They don't match" }))
    await waitFor(() => expect(linkDecline).toHaveBeenCalled())
  })

  it("copies the code through the core's clipboard path only, and closing the offer declines the link", async () => {
    const linkDecline = vi.fn(() => Promise.resolve())
    const writeText = vi.fn(() => Promise.resolve())
    const copyLinkCode = vi.fn(() => Promise.resolve({ clears_in_secs: 30, copy: 7 }))
    mount(
      fakeDepth({
        linkOffer: () => Promise.resolve(waiting()),
        linkPoll: () => Promise.resolve(waiting()),
        linkDecline,
      }),
      <AddDevice onNotice={() => {}} onChanged={() => {}} />,
      fakeClient({ copyLinkCode }),
    )
    const user = userEvent.setup()
    // After user-event's setup, which puts its own clipboard in place.
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    })
    await user.click(screen.getByRole("button", { name: "Add device" }))
    const offer = await screen.findByRole("dialog", { name: "Add a device" })
    expect(within(offer).getByLabelText("Device code")).toHaveTextContent("7KQ2M-9XH4P")
    // COPY (Lockdown 1): the core copies its own code with the clear timer and the history marks (D316);
    // the webview sends no text, and never writes the system clipboard itself.
    await user.click(within(offer).getByRole("button", { name: "Copy" }))
    await waitFor(() => expect(copyLinkCode).toHaveBeenCalledWith(30))
    expect(copyLinkCode.mock.calls[0]).toHaveLength(1)
    expect(await within(offer).findByText("Copied. The clipboard clears in 30 seconds.")).toBeInTheDocument()
    expect(writeText).not.toHaveBeenCalled()
    await user.click(within(offer).getByRole("button", { name: "Cancel" }))
    await waitFor(() => expect(linkDecline).toHaveBeenCalled())
  })

  it("says why a copy was refused", async () => {
    mount(
      fakeDepth({ linkOffer: () => Promise.resolve(waiting()), linkPoll: () => Promise.resolve(waiting()) }),
      <AddDevice onNotice={() => {}} onChanged={() => {}} />,
      fakeClient({ copyLinkCode: () => Promise.reject({ code: "refused", message: "no device is being added" }) }),
    )
    const user = userEvent.setup()
    await user.click(screen.getByRole("button", { name: "Add device" }))
    const offer = await screen.findByRole("dialog", { name: "Add a device" })
    await user.click(within(offer).getByRole("button", { name: "Copy" }))
    expect(await within(offer).findByText("no device is being added")).toBeInTheDocument()
  })

  it("shows the core's refusal in words", async () => {
    mount(
      fakeDepth({
        linkOffer: () =>
          Promise.reject({
            code: "refused",
            message: "the account server can't be reached",
          }),
      }),
      <AddDevice onNotice={() => {}} onChanged={() => {}} />,
    )
    await userEvent.setup().click(screen.getByRole("button", { name: "Add device" }))
    expect(await screen.findByRole("alert")).toHaveTextContent("the account server can't be reached")
  })
})
