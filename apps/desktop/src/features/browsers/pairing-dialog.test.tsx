import { act, render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { toast } from "@unicron/ui"

import { ARMED_MS, RequestHost } from "@/features/approvals/test-host"

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

function mount(client: BrowserClient, notice = vi.fn()) {
  render(
    <BrowserClientProvider value={client}>
      <RequestHost>
        <PairingDialog onNotice={notice} />
      </RequestHost>
    </BrowserClientProvider>,
  )
  return notice
}

const armed = () => act(() => new Promise((r) => setTimeout(r, ARMED_MS)))

async function openReview() {
  const user = userEvent.setup()
  expect(await screen.findByText("Chrome on Linux wants to pair.")).toBeInTheDocument()
  expect(screen.getByText(/^Pairing request \/\/ \d\d:\d\d left$/)).toBeInTheDocument()
  await user.click(screen.getByRole("button", { name: "Review" }))
  return { user, dialog: await screen.findByRole("dialog") }
}

afterEach(() => {
  act(() => {
    toast.dismiss()
  })
})

describe("pairing a browser (brief A1, X3 row 2)", () => {
  it("waits as the needs-you; the review shows the code the extension shows and pairs only on THEY MATCH", async () => {
    let waiting = [request]
    const answer = vi.fn(() => {
      waiting = []
      return Promise.resolve()
    })
    const notice = mount(fake({ pending: () => Promise.resolve(waiting), answer }))
    const { user, dialog } = await openReview()
    expect(dialog).toHaveTextContent("Pair Chrome on Linux?")
    expect(dialog).toHaveTextContent("Browser")
    expect(dialog).toHaveTextContent("Kind")
    expect(dialog).toHaveTextContent("Chromium")
    expect(dialog).toHaveTextContent("Approve only if the Lockdown extension in your browser shows this code.")
    expect(dialog).toHaveTextContent("A paired browser can ask to fill the sign-ins, codes and cards on this computer. It never sees your vault.")
    // The code in DigitGroups, in its two groups.
    const code = dialog.querySelector('[data-slot="approval-sheet-match-code"]')!
    expect(code).toHaveTextContent(/^653\s*540/)
    await armed()
    await user.click(within(dialog).getByRole("button", { name: "They match" }))
    await waitFor(() => expect(answer).toHaveBeenCalledWith(request.request_id, true, {}))
    expect(notice).toHaveBeenCalledWith("Paired Chrome on Linux")
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument())
  })

  it("declines when the codes differ", async () => {
    const answer = vi.fn(() => Promise.resolve())
    mount(fake({ pending: () => Promise.resolve([request]), answer }))
    const { user, dialog } = await openReview()
    await armed()
    await user.click(within(dialog).getByRole("button", { name: "Codes differ" }))
    await waitFor(() => expect(answer).toHaveBeenCalledWith(request.request_id, false, {}))
  })

  it("DECLINE on the needs-you says no at once", async () => {
    const answer = vi.fn(() => Promise.resolve())
    const notice = mount(fake({ pending: () => Promise.resolve([request]), answer }))
    await userEvent.setup().click(await screen.findByRole("button", { name: "Decline" }))
    await waitFor(() => expect(answer).toHaveBeenCalledWith(request.request_id, false, {}))
    expect(notice).toHaveBeenCalledWith("Declined: Chrome on Linux")
  })

  it("never opens by itself, and closing the review pairs nothing and declines nothing", async () => {
    const answer = vi.fn(() => Promise.resolve())
    mount(fake({ pending: () => Promise.resolve([request]), answer }))
    expect(await screen.findByText("Chrome on Linux wants to pair.")).toBeInTheDocument()
    await act(() => new Promise((r) => setTimeout(r, 1700)))
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument()
    const { user } = await openReview()
    await armed()
    await user.keyboard("{Escape}")
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument())
    expect(answer).not.toHaveBeenCalled()
    expect(await screen.findByText("Chrome on Linux wants to pair.")).toBeInTheDocument()
  }, 10_000)

  it("a pairing request outranks nothing above it: a sign-in request would show first, this one counts as // +1", async () => {
    // Two pairing requests: one block, the second counted.
    mount(fake({ pending: () => Promise.resolve([request, { ...request, request_id: "second", name: "Firefox on Linux", family: "firefox" }]) }))
    expect(await screen.findByText(/^Pairing request \/\/ \d\d:\d\d left/)).toBeInTheDocument()
    expect(document.querySelectorAll('[data-slot="needs-you"]')).toHaveLength(1)
    expect(document.querySelector('[data-slot="needs-you"]')).toHaveTextContent("+1")
  })
})
