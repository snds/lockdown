import { act, render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { Dialog, DialogContent, DialogHeader, DialogTitle, toast } from "@unicron/ui"

import { ClientProvider } from "@/core/client"
import type { ApprovalRoute } from "@/core/types"
import { ARMED_MS, RequestHost } from "@/features/approvals/test-host"
import { fakeClient } from "@/test/fake-client"

import { PasskeyClientProvider, type PasskeyApproval, type PasskeyClient } from "./passkey-client"
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

const github: PasskeyApproval = { request_id: "r1", browser: "Chrome on Linux", text: "Sign in to github.com as sean", expires_in_secs: 110 }

const armed = () => act(() => new Promise((r) => setTimeout(r, ARMED_MS)))

function mount(client: PasskeyClient, { route = { route: "password", method: "password" } as ApprovalRoute, notice = vi.fn(), phone = false, extra = null as React.ReactNode } = {}) {
  render(
    <ClientProvider value={fakeClient({ approvalRoute: () => Promise.resolve(route) })}>
      <PasskeyClientProvider value={client}>
        <RequestHost phone={phone}>
          <PasskeyApprovalDialog onNotice={notice} />
          {extra}
        </RequestHost>
      </PasskeyClientProvider>
    </ClientProvider>,
  )
  return notice
}

/** The needs-you block, once the poll found the request. */
const block = () => screen.findByText("Sign-in request // 01:50 left", { exact: false })

afterEach(() => {
  act(() => {
    toast.dismiss()
  })
})

describe("a passkey request waiting for the app (brief A1, X3 row 1)", () => {
  it("waits as the needs-you; REVIEW opens the review, and Approve answers through the gate", async () => {
    let waiting = [github]
    const answer = vi.fn(() => {
      waiting = []
      return Promise.resolve()
    })
    const notice = mount(fake({ pending: () => Promise.resolve(waiting), answer }))
    await block()
    expect(screen.getByText("Sign in to github.com as sean")).toBeInTheDocument()
    const user = userEvent.setup()
    await user.click(screen.getByRole("button", { name: "Review" }))
    const dialog = await screen.findByRole("dialog")
    expect(dialog).toHaveTextContent("Sign in to github.com as sean")
    expect(dialog).toHaveTextContent("Browser")
    expect(dialog).toHaveTextContent("Chrome on Linux")
    // Nothing the request does not carry: no account, device, place or time row.
    for (const word of ["Account", "Device", "Near", "ago", "locks this login"]) expect(dialog).not.toHaveTextContent(word)
    await armed()
    await user.click(within(dialog).getByRole("button", { name: "Approve" }))
    await waitFor(() => expect(answer).toHaveBeenCalledWith("r1", true, {}))
    expect(notice).toHaveBeenCalledWith("Approved: Sign in to github.com as sean")
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument())
  })

  it("DECLINE on the needs-you says no at once, with no review", async () => {
    const answer = vi.fn(() => Promise.resolve())
    const notice = mount(fake({ pending: () => Promise.resolve([{ request_id: "r2", browser: "Firefox on Linux", text: "Save a passkey for example.com as me", expires_in_secs: 90 }]), answer }))
    await userEvent.setup().click(await screen.findByRole("button", { name: "Decline" }))
    await waitFor(() => expect(answer).toHaveBeenCalledWith("r2", false, {}))
    expect(notice).toHaveBeenCalledWith("Declined: Save a passkey for example.com as me")
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument()
  })

  it("never opens a review by itself", async () => {
    mount(fake({ pending: () => Promise.resolve([github]) }))
    await block()
    // Two more polls (1.5 s each): still only the needs-you.
    await act(() => new Promise((r) => setTimeout(r, 3200)))
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument()
  }, 10_000)

  it("closing the review, with its close button or Escape, sends no answer, and the request keeps waiting", async () => {
    const answer = vi.fn(() => Promise.resolve())
    mount(fake({ pending: () => Promise.resolve([github]), answer }))
    const user = userEvent.setup()
    await block()
    await user.click(screen.getByRole("button", { name: "Review" }))
    await screen.findByRole("dialog")
    await armed()
    await user.click(screen.getByRole("button", { name: "Close" }))
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument())
    await user.click(await screen.findByRole("button", { name: "Review" }))
    await screen.findByRole("dialog")
    await armed()
    await user.keyboard("{Escape}")
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument())
    expect(answer).not.toHaveBeenCalled()
    expect(await screen.findByRole("button", { name: "Review" })).toBeInTheDocument()
  })

  it("arms the yes a moment after it opens, and while the answer is on its way the no waits too", async () => {
    let finish: () => void = () => {}
    const answer = vi.fn(() => new Promise<void>((r) => (finish = r)))
    mount(fake({ pending: () => Promise.resolve([github]), answer }))
    const user = userEvent.setup()
    await block()
    await user.click(screen.getByRole("button", { name: "Review" }))
    const dialog = await screen.findByRole("dialog")
    expect(within(dialog).getByRole("button", { name: /^Approve/ })).toBeDisabled()
    await armed()
    const yes = within(dialog).getByRole("button", { name: /^Approve/ })
    expect(yes).toBeEnabled()
    await user.click(yes)
    await waitFor(() => expect(dialog).toHaveTextContent("Waiting for the answer"))
    // The yes keeps its label and shows that it works; the no is disabled meanwhile.
    expect(within(dialog).getByRole("button", { name: /Approve/ })).toHaveAttribute("aria-busy", "true")
    expect(within(dialog).getByRole("button", { name: "Decline" })).toBeDisabled()
    await act(async () => finish())
  })

  it("names the yes by the gate's route: Approve with Touch ID only for the platform check", async () => {
    mount(fake({ pending: () => Promise.resolve([github]) }), { route: { route: "platform", method: "Touch ID" } })
    const user = userEvent.setup()
    await block()
    await user.click(screen.getByRole("button", { name: "Review" }))
    const dialog = await screen.findByRole("dialog")
    expect(await within(dialog).findByRole("button", { name: /Approve with Touch ID/ })).toBeInTheDocument()
    // No DECLINE in the yes's place: the no stays the secondary.
    expect(within(dialog).getByRole("button", { name: "Decline" })).toHaveAttribute("data-variant", "secondary")
  })

  it("on the password route the yes is plain Approve; the gate asks after it is pressed", async () => {
    mount(fake({ pending: () => Promise.resolve([github]) }), { route: { route: "password", method: "password" } })
    const user = userEvent.setup()
    await block()
    await user.click(screen.getByRole("button", { name: "Review" }))
    const dialog = await screen.findByRole("dialog")
    await armed()
    expect(within(dialog).getByRole("button", { name: "Approve" })).toBeInTheDocument()
    expect(within(dialog).queryByRole("button", { name: /Touch ID/ })).toBeNull()
  })

  it("backing out of the gate returns to the review: nothing approved, no error", async () => {
    const answer = vi.fn(() => Promise.reject({ code: "refused", message: "Cancelled; nothing happened." }))
    mount(fake({ pending: () => Promise.resolve([github]), answer }))
    const user = userEvent.setup()
    await block()
    await user.click(screen.getByRole("button", { name: "Review" }))
    const dialog = await screen.findByRole("dialog")
    await armed()
    await user.click(within(dialog).getByRole("button", { name: "Approve" }))
    await waitFor(() => expect(answer).toHaveBeenCalledTimes(1))
    expect(screen.getByRole("dialog")).toBeInTheDocument()
    expect(within(screen.getByRole("dialog")).queryByRole("alert")).toBeNull()
    expect(within(screen.getByRole("dialog")).getByRole("button", { name: "Approve" })).toBeEnabled()
  })

  it("an answer that failed shows the core's sentence in the review", async () => {
    const answer = vi.fn(() => Promise.reject({ code: "refused", message: "The browser stopped waiting." }))
    mount(fake({ pending: () => Promise.resolve([github]), answer }))
    const user = userEvent.setup()
    await block()
    await user.click(screen.getByRole("button", { name: "Review" }))
    await armed()
    await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Approve" }))
    expect(await within(await screen.findByRole("dialog")).findByRole("alert")).toHaveTextContent("The browser stopped waiting.")
  })

  it("a request that runs out while its review is open says so, with Close only", async () => {
    let waiting = [{ ...github, expires_in_secs: 1 }]
    const answer = vi.fn(() => Promise.resolve())
    mount(fake({ pending: () => Promise.resolve(waiting), answer }))
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "Review" }))
    const dialog = await screen.findByRole("dialog")
    waiting = []
    expect(await within(dialog).findByText("This request ran out.", {}, { timeout: 3000 })).toBeInTheDocument()
    expect(within(dialog).queryByRole("button", { name: /^Approve/ })).toBeNull()
    expect(answer).not.toHaveBeenCalled()
  }, 10_000)

  it("arriving under an open dialog: the compact line at its top, and REVIEW opens the review over it", async () => {
    mount(fake({ pending: () => Promise.resolve([github]) }), {
      extra: (
        <Dialog open>
          <DialogContent>
            <DialogHeader>
              <DialogTitle>New login</DialogTitle>
            </DialogHeader>
          </DialogContent>
        </Dialog>
      ),
    })
    const line = await waitFor(() => {
      const el = document.querySelector<HTMLElement>('[data-needs-you-layer] [data-slot="needs-you"]')
      expect(el).not.toBeNull()
      return el!
    })
    expect(line).toHaveTextContent(/Sign-in request/i)
    await userEvent.setup().click(within(line).getByRole("button", { name: "Review" }))
    const review = await screen.findByRole("dialog", { name: "Sign in to github.com as sean" })
    expect(review).toBeInTheDocument()
    // The editor under it stays open: the review sits over it.
    expect(screen.getByRole("dialog", { name: "New login", hidden: true })).toBeInTheDocument()
  })

  it("on a phone, the request is the strip; its REVIEW opens the sheet", async () => {
    mount(fake({ pending: () => Promise.resolve([github]) }), { phone: true })
    const strip = await waitFor(() => {
      const el = document.querySelector<HTMLElement>('[data-slot="accessory"][data-tone="needs-you"]')
      expect(el).not.toBeNull()
      return el!
    })
    expect(strip).toHaveTextContent("Sign in to github.com as sean")
    expect(document.querySelector('[data-slot="needs-you"]')).toBeNull()
    await userEvent.setup().click(within(strip).getAllByRole("button", { name: "Review" })[0]!)
    expect(await screen.findByRole("dialog")).toHaveTextContent("Chrome on Linux")
  })
})
