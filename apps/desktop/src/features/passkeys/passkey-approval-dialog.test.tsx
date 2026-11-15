import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { PasskeyClientProvider, type PasskeyClient } from "./passkey-client"
import { PasskeyApprovalDialog } from "./passkey-approval-dialog"

function fake(overrides: Partial<PasskeyClient>): PasskeyClient {
  return {
    list: () => Promise.resolve([]),
    remove: () => Promise.resolve(2),
    restore: () => Promise.resolve(2),
    pending: () => Promise.resolve([]),
    answer: () => Promise.resolve(),
    upgrades: () => Promise.resolve([]),
    upgradeSeen: () => Promise.resolve(0),
    ...overrides,
  }
}

describe("a passkey request waiting for the app", () => {
  it("asks here, in the app, and answers through the gate", async () => {
    let waiting = [{ request_id: "r1", browser: "Chrome on Linux", text: "Sign in to github.com as sean", expires_in_secs: 110 }]
    const answer = vi.fn(() => {
      waiting = []
      return Promise.resolve()
    })
    const notice = vi.fn()
    render(
      <PasskeyClientProvider value={fake({ pending: () => Promise.resolve(waiting), answer })}>
        <PasskeyApprovalDialog onNotice={notice} />
      </PasskeyClientProvider>,
    )
    const dialog = await screen.findByRole("dialog")
    expect(dialog).toHaveTextContent("Sign in to github.com as sean?")
    expect(dialog).toHaveTextContent("Chrome on Linux is asking to use your passkey")
    await userEvent.setup().click(within(dialog).getByRole("button", { name: "Approve" }))
    await waitFor(() => expect(answer).toHaveBeenCalledWith("r1", true, {}))
    expect(notice).toHaveBeenCalledWith("Approved: Sign in to github.com as sean.")
  })

  it("Decline says no", async () => {
    const answer = vi.fn(() => Promise.resolve())
    render(
      <PasskeyClientProvider value={fake({ pending: () => Promise.resolve([{ request_id: "r2", browser: "Firefox on Linux", text: "Save a passkey for example.com as me", expires_in_secs: 90 }]), answer })}>
        <PasskeyApprovalDialog onNotice={() => {}} />
      </PasskeyClientProvider>,
    )
    await userEvent.setup().click(await screen.findByRole("button", { name: "Decline" }))
    await waitFor(() => expect(answer).toHaveBeenCalledWith("r2", false, {}))
  })
})
