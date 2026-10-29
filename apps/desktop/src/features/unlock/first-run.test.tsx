import { render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { ClientProvider, type Client } from "@/core/client"
import type { Probe } from "@/core/types"

import { UnlockScreen } from "./unlock-screen"

const probe: Probe = {
  root: "/tmp/snds/identity",
  data: "/tmp/snds/lockdown",
  identity_present: false,
  device_key_present: false,
  account_present: false,
  device_name: "Lockdown on test",
}

function fakeClient(overrides: Partial<Client>): Client {
  const refuse = () => Promise.reject({ code: "refused", message: "not in this test" })
  const base = new Proxy({} as Client, { get: () => refuse })
  return Object.assign(Object.create(base), { probe: () => Promise.resolve(probe) }, overrides)
}

describe("the first run (D94)", () => {
  it("says one sentence per screen and keeps the detail behind Learn more", async () => {
    render(
      <ClientProvider value={fakeClient({})}>
        <UnlockScreen onOpened={() => {}} />
      </ClientProvider>,
    )
    const user = userEvent.setup()
    // Screen 1: the tagline (once in the app) and one sentence.
    expect(await screen.findByRole("heading", { name: /Every way you sign in/ })).toBeInTheDocument()
    expect(screen.getByText(/One vault for passwords, passkeys and codes/)).toBeInTheDocument()
    // The OPAQUE explanation is not in the flow.
    expect(screen.queryByText(/encrypted on this device/)).not.toBeInTheDocument()
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument()

    // "Learn more" is a link in the sentence, not plain text (design review R20).
    const learnMore = screen.getByRole("button", { name: "Learn more" })
    expect(learnMore.parentElement?.tagName).toBe("P")
    expect(learnMore.parentElement?.className).toContain("[&>[data-slot=button]]:underline")
    // The Create door's button says what it makes; the tab alone says "Create".
    expect(screen.getByRole("tab", { name: "Create" })).toBeInTheDocument()
    expect(screen.queryByRole("button", { name: "Create" })).not.toBeInTheDocument()
    await user.click(learnMore)
    const dialog = await screen.findByRole("dialog")
    expect(dialog).toHaveTextContent(/encrypted on this device/)
    await user.click(screen.getByRole("button", { name: "Got it" }))

    // Screen 2: the Secret Key and the kit, one sentence, its own Learn more.
    await user.click(screen.getByRole("button", { name: "Create my vault" }))
    expect(await screen.findByRole("heading", { name: /Secret Key and recovery kit/ })).toBeInTheDocument()
    expect(screen.getByText(/syncs your vault between devices and makes the one page/)).toBeInTheDocument()
    expect(screen.queryByText(/OPAQUE/)).not.toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: "Learn more" }))
    expect(await screen.findByRole("dialog")).toHaveTextContent(/OPAQUE/)
  })

  it("shows the proof moment once, after an account was created, then opens the vault (D95)", async () => {
    const page = new TextEncoder().encode("Secret Key: A1-TEST\nRecovery code: X\n")
    const status = { live_items: 0 } as never
    const client = fakeClient({
      openVault: vi.fn(() => Promise.resolve(status)),
      accountRegister: vi.fn(() => Promise.resolve({ lines: ["registered"], page })),
      status: vi.fn(() => Promise.resolve(status)),
    })
    const onOpened = vi.fn()
    render(
      <ClientProvider value={client}>
        <UnlockScreen onOpened={onOpened} />
      </ClientProvider>,
    )
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "Create my vault" }))
    await user.type(screen.getByLabelText("Server"), "https://vault.example")
    await user.type(screen.getByLabelText("Login name"), "me@example.com")
    await user.type(screen.getByLabelText("Account password"), "a long account password")
    await user.click(screen.getByRole("button", { name: "Create account" }))

    // The kit page, shown once.
    const kit = await screen.findByRole("dialog")
    expect(kit).toHaveTextContent("Secret Key: A1-TEST")
    await user.click(screen.getByRole("button", { name: "I have it" }))

    // Screen 3: recovery picks; Continue waits for the kit to be kept.
    expect(await screen.findByRole("heading", { name: /Keep the kit somewhere safe/ })).toBeInTheDocument()
    expect(screen.getByRole("button", { name: "Continue" })).toBeDisabled()
    await user.click(screen.getByRole("checkbox", { name: "I printed or wrote down the kit" }))
    await user.click(screen.getByRole("button", { name: "Continue" }))

    // The proof moment: three lines, once.
    expect(await screen.findByRole("heading", { name: "Locked, even from us" })).toBeInTheDocument()
    expect(screen.getAllByRole("listitem")).toHaveLength(3)
    expect(onOpened).not.toHaveBeenCalled()
    await user.click(screen.getByRole("button", { name: "Open the vault" }))
    expect(onOpened).toHaveBeenCalledTimes(1)
  })
})
