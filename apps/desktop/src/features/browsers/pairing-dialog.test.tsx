import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { BrowserClientProvider, type BrowserClient, type PendingPairing } from "./browser-client"
import { PairingDialog } from "./pairing-dialog"

const request: PendingPairing = {
  request_id: "de997f97bf3d8fc75d48e1d6cbd86b12",
  code: "653540",
  name: "Chrome on Linux",
  family: "chromium",
  extension_id: "dbdblnolpiehhlabpmhobmkbjipmebpe",
  expires_in_secs: 110,
}

function fake(overrides: Partial<BrowserClient>): BrowserClient {
  return {
    status: () => Promise.resolve({ listening: true, problem: null, host_binary: "/opt/lockdown/lockdown-native-host", browsers: [] }),
    pending: () => Promise.resolve([]),
    answer: () => Promise.resolve(),
    paired: () => Promise.resolve([]),
    unpair: () => Promise.resolve(),
    install: () => Promise.resolve([]),
    ...overrides,
  }
}

describe("pairing a browser", () => {
  it("shows the code the extension shows and pairs only when the person approves", async () => {
    let waiting = [request]
    const answer = vi.fn(() => {
      waiting = []
      return Promise.resolve()
    })
    const notice = vi.fn()
    render(
      <BrowserClientProvider value={fake({ pending: () => Promise.resolve(waiting), answer })}>
        <PairingDialog onNotice={notice} />
      </BrowserClientProvider>,
    )
    expect(await screen.findByRole("dialog")).toHaveTextContent("Connect Chrome on Linux to Lockdown?")
    expect(screen.getByText("653 540")).toBeInTheDocument()
    await userEvent.setup().click(screen.getByRole("button", { name: "Pair" }))
    await waitFor(() => expect(answer).toHaveBeenCalledWith(request.request_id, true, {}))
    expect(notice).toHaveBeenCalledWith("Chrome on Linux is paired with Lockdown.")
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument())
  })

  it("declines when the codes differ", async () => {
    const answer = vi.fn(() => Promise.resolve())
    render(
      <BrowserClientProvider value={fake({ pending: () => Promise.resolve([request]), answer })}>
        <PairingDialog onNotice={() => {}} />
      </BrowserClientProvider>,
    )
    await userEvent.setup().click(await screen.findByRole("button", { name: "The codes differ" }))
    await waitFor(() => expect(answer).toHaveBeenCalledWith(request.request_id, false, {}))
  })
})
