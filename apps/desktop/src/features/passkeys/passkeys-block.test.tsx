import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { PasskeyClientProvider, type PasskeyClient, type PasskeyView } from "./passkey-client"
import { PasskeyApprovalDialog } from "./passkey-approval-dialog"
import { PasskeysBlock } from "./passkeys-block"

const view = (over: Partial<PasskeyView> = {}): PasskeyView => ({
  credential_id: "Y3JlZC0x",
  rp_id: "example.com",
  user_name: "sean@example.com",
  user_display_name: "Sean",
  provider: "Lockdown",
  aaguid: "07017dfd-2717-4c97-92c2-b3d6513f46ce",
  algorithm: "ES256 (P-256)",
  backup_eligible: true,
  backed_up: true,
  created: 1_760_000_000,
  last_used: null,
  hidden: null,
  only_sign_in: false,
  ...over,
})

function fake(overrides: Partial<PasskeyClient>): PasskeyClient {
  return {
    list: () => Promise.resolve([]),
    remove: () => Promise.resolve(2),
    restore: () => Promise.resolve(2),
    pending: () => Promise.resolve([]),
    answer: () => Promise.resolve(),
    ...overrides,
  }
}

function mount(client: PasskeyClient, onChanged = vi.fn(), canEdit = true) {
  render(
    <PasskeyClientProvider value={client}>
      <PasskeysBlock itemId="i1" title="Example" fields={[]} onChanged={onChanged} canEdit={canEdit} />
    </PasskeyClientProvider>,
  )
  return onChanged
}

describe("the passkeys on an item", () => {
  it("names the provider and its ID, the backup flags, the dates, and never a key", async () => {
    mount(fake({ list: () => Promise.resolve([view()]) }))
    const row = await screen.findByRole("listitem", { name: "Passkey for example.com" })
    expect(row).toHaveTextContent("sean@example.com (Sean)")
    expect(row).toHaveTextContent("Lockdown · ES256 (P-256) · synced with your vault")
    expect(row).toHaveTextContent("last used on this computer: never")
    expect(row).toHaveTextContent("Provider ID 07017dfd-2717-4c97-92c2-b3d6513f46ce")
    expect(screen.getByText(/kept by the provider, never shown/)).toBeInTheDocument()
  })

  it("a passkey a site hid shows why, and Undo offers it again", async () => {
    const restore = vi.fn(() => Promise.resolve(3))
    const onChanged = mount(fake({ list: () => Promise.resolve([view({ hidden: { reason: "The site said it no longer knows this passkey", at: 1_760_000_100 } })]), restore }))
    const row = await screen.findByRole("listitem", { name: "Passkey for example.com" })
    expect(within(row).getByText("hidden by the site")).toBeInTheDocument()
    expect(row).toHaveTextContent("Lockdown hid it rather than deleting it")
    await userEvent.setup().click(within(row).getByRole("button", { name: "Undo" }))
    await waitFor(() => expect(restore).toHaveBeenCalledWith("i1", "Y3JlZC0x"))
    expect(onChanged).toHaveBeenCalledWith("The passkey for example.com is offered again.")
  })

  it("deleting the only way in warns first, and goes ahead only on Delete anyway", async () => {
    const remove = vi.fn(() => Promise.resolve(4))
    const onChanged = mount(fake({ list: () => Promise.resolve([view({ only_sign_in: true })]), remove }))
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "Delete" }))
    const dialog = await screen.findByRole("dialog")
    expect(dialog).toHaveTextContent('This is the only way into "Example" that Lockdown holds')
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }))
    expect(remove).not.toHaveBeenCalled()
    await user.click(screen.getByRole("button", { name: "Delete" }))
    await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Delete anyway" }))
    await waitFor(() => expect(remove).toHaveBeenCalledWith("i1", "Y3JlZC0x", {}))
    expect(onChanged).toHaveBeenCalledWith("The passkey for example.com was deleted.")
  })

  it("an ordinary delete says the site keeps it until removed there", async () => {
    mount(fake({ list: () => Promise.resolve([view({ provider: "Another provider (imported)", aaguid: null, created: null })]) }))
    const row = await screen.findByRole("listitem", { name: "Passkey for example.com" })
    expect(row).not.toHaveTextContent("Provider ID")
    await userEvent.setup().click(within(row).getByRole("button", { name: "Delete" }))
    expect(await screen.findByRole("dialog")).toHaveTextContent("The site still lists it until you remove it there")
  })
})

describe("a passkey on an item shared without edit", () => {
  it("offers neither Undo nor Delete, and still lists it", async () => {
    mount(fake({ list: () => Promise.resolve([view({ hidden: { reason: "The site said it no longer knows this passkey", at: 1_760_000_100 } })]) }), vi.fn(), false)
    const row = await screen.findByRole("listitem", { name: "Passkey for example.com" })
    expect(within(row).getByText("hidden by the site")).toBeInTheDocument()
    expect(within(row).queryByRole("button", { name: "Undo" })).not.toBeInTheDocument()
    expect(within(row).queryByRole("button", { name: "Delete" })).not.toBeInTheDocument()
  })
})

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
