import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { PasskeyClientProvider, type PasskeyClient, type PasskeyView } from "./passkey-client"
import { PasskeysBlock } from "./passkeys-block"
import { dateLocal, fullDate } from "@/lib/format"

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
    upgrades: () => Promise.resolve([]),
    upgradeSeen: () => Promise.resolve(0),
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

const details = async (user: ReturnType<typeof userEvent.setup>, rp = "example.com") => {
  await user.click(await screen.findByRole("button", { name: `Details of the passkey for ${rp}` }))
  return screen.findByRole("dialog", { name: rp })
}

describe("the passkeys on an item", () => {
  it("a row as the frames draw it (who made it, when, its account); the details name the provider and its ID, the backup flag, the dates, and never a key", async () => {
    mount(fake({ list: () => Promise.resolve([view()]) }))
    const row = await screen.findByRole("listitem", { name: "Passkey for example.com" })
    expect(row).toHaveTextContent(`Lockdown // ${fullDate(1_760_000_000)}`)
    expect(row).toHaveTextContent("sean@example.com (Sean)")
    const dialog = await details(userEvent.setup())
    expect(dialog).toHaveTextContent("Lockdown")
    expect(dialog).toHaveTextContent("07017dfd-2717-4c97-92c2-b3d6513f46ce")
    expect(dialog).toHaveTextContent("With your vault")
    expect(dialog).toHaveTextContent("Never on this computer")
    expect(dialog).toHaveTextContent("ES256 (P-256)")
    expect(dialog).toHaveTextContent(/kept by the provider, never shown/)
  })

  it("syncs: no device names and no count (W9), only whether it syncs", async () => {
    mount(fake({ list: () => Promise.resolve([view({ backed_up: false, last_used: 1_760_000_500 })]) }))
    const dialog = await details(userEvent.setup())
    expect(dialog).toHaveTextContent("This device only")
    expect(dialog).toHaveTextContent(`${fullDate(1_760_000_500)} // on this computer`)
    expect(dialog).not.toHaveTextContent(/devices/)
  })

  it("a passkey a site hid shows why, and Undo offers it again", async () => {
    const restore = vi.fn(() => Promise.resolve(3))
    const onChanged = mount(fake({ list: () => Promise.resolve([view({ hidden: { reason: "The site said it no longer knows this passkey", at: 1_760_000_100 } })]), restore }))
    const row = await screen.findByRole("listitem", { name: "Passkey for example.com" })
    expect(within(row).getByText("Hidden by the site")).toBeInTheDocument()
    expect(row).toHaveTextContent("Lockdown hid it rather than deleting it")
    await userEvent.setup().click(within(row).getByRole("button", { name: /^Undo/ }))
    await waitFor(() => expect(restore).toHaveBeenCalledWith("i1", "Y3JlZC0x"))
    expect(onChanged).toHaveBeenCalledWith("The passkey for example.com is offered again.")
  })

  it("deleting the only way in warns first, and goes ahead only on Delete anyway", async () => {
    const remove = vi.fn(() => Promise.resolve(4))
    const onChanged = mount(fake({ list: () => Promise.resolve([view({ only_sign_in: true })]), remove }))
    const user = userEvent.setup()
    await details(user)
    await user.click(screen.getByRole("button", { name: /^Delete passkey/ }))
    expect(await screen.findByText(/This is the only way into "Example" that Lockdown holds/)).toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: "Cancel" }))
    expect(remove).not.toHaveBeenCalled()
    await user.click(screen.getByRole("button", { name: /^Delete passkey/ }))
    await user.click(await screen.findByRole("button", { name: "Delete anyway" }))
    await waitFor(() => expect(remove).toHaveBeenCalledWith("i1", "Y3JlZC0x", {}))
    expect(onChanged).toHaveBeenCalledWith("The passkey for example.com was deleted.")
  })

  it("an ordinary delete says the site keeps it until removed there; no provider ID when there is none", async () => {
    mount(fake({ list: () => Promise.resolve([view({ provider: "Another provider (imported)", aaguid: null, created: null })]) }))
    const user = userEvent.setup()
    const dialog = await details(user)
    expect(dialog).not.toHaveTextContent("Provider ID")
    await user.click(within(dialog).getByRole("button", { name: /^Delete passkey/ }))
    expect(await screen.findByText(/The site still lists it until you remove it there/)).toBeInTheDocument()
  })
})

describe("a passkey the site added by itself after a password sign-in", () => {
  it("says so on the item until it is opened, and opening it tells the core", async () => {
    const at = 1_761_700_000
    const upgradeSeen = vi.fn(() => Promise.resolve(1))
    mount(fake({ list: () => Promise.resolve([view({ rp_id: "github.com", upgrade: { site: "github.com", at } })]), upgradeSeen }))
    const row = await screen.findByRole("listitem", { name: "Passkey for github.com" })
    expect(within(row).getByText("Added by the site")).toBeInTheDocument()
    expect(row).toHaveTextContent(`Added by github.com on ${dateLocal(at)}, after a password sign-in.`)
    await waitFor(() => expect(upgradeSeen).toHaveBeenCalledWith("i1"))
    expect(upgradeSeen).toHaveBeenCalledTimes(1)
  })

  it("says nothing, and tells the core nothing, for a passkey made any other way", async () => {
    const upgradeSeen = vi.fn(() => Promise.resolve(0))
    mount(fake({ list: () => Promise.resolve([view()]), upgradeSeen }))
    const row = await screen.findByRole("listitem", { name: "Passkey for example.com" })
    expect(row).not.toHaveTextContent("Added by")
    expect(upgradeSeen).not.toHaveBeenCalled()
  })
})

describe("a passkey on an item shared without edit", () => {
  it("offers neither Undo nor Delete, and still lists it", async () => {
    mount(fake({ list: () => Promise.resolve([view({ hidden: { reason: "The site said it no longer knows this passkey", at: 1_760_000_100 } })]) }), vi.fn(), false)
    const row = await screen.findByRole("listitem", { name: "Passkey for example.com" })
    expect(within(row).getByText("Hidden by the site")).toBeInTheDocument()
    expect(within(row).queryByRole("button", { name: /^Undo/ })).not.toBeInTheDocument()
    const dialog = await details(userEvent.setup())
    expect(within(dialog).queryByRole("button", { name: /^Delete passkey/ })).not.toBeInTheDocument()
  })
})
