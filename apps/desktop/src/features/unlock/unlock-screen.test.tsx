import { render, screen, waitFor } from "@testing-library/react"
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

describe("the unlock screen's error path", () => {
  it("shows the core's words when joining fails, and keeps the form", async () => {
    const accountLogin = vi.fn(() =>
      Promise.reject({
        code: "refused",
        message: "the password and Secret Key didn't open the account slot: check the Secret Key against the page",
      }),
    )
    const openVault = vi.fn()
    render(
      <ClientProvider value={fakeClient({ accountLogin, openVault })}>
        <UnlockScreen onOpened={() => {}} />
      </ClientProvider>,
    )
    const user = userEvent.setup()
    await user.click(await screen.findByRole("tab", { name: "Join" }))
    await user.type(screen.getByLabelText("Server"), "https://vault.example")
    await user.type(screen.getByLabelText("Login name"), "me@example.com")
    await user.type(screen.getByLabelText("Account password"), "a long account password")
    await user.type(screen.getByLabelText("Secret Key"), "A3-WRONG")
    await user.click(screen.getByRole("button", { name: "Join" }))

    const alert = await screen.findByRole("alert")
    expect(alert).toHaveTextContent("didn't open the account slot")
    expect(accountLogin).toHaveBeenCalledWith({
      server: "https://vault.example",
      login: "me@example.com",
      password: "a long account password",
      secretKey: "A3-WRONG",
    })
    // The vault was never opened, and the person can try again.
    expect(openVault).not.toHaveBeenCalled()
    expect(screen.getByRole("button", { name: "Join" })).toBeEnabled()
  })

  it("shows the core's words when an enrolled device fails to unlock", async () => {
    const unlock = vi.fn(() => Promise.reject({ code: "refused", message: "Rollback attempt blocked: the vault file is older than this device remembers" }))
    render(
      <ClientProvider
        value={fakeClient({
          probe: () => Promise.resolve({ ...probe, identity_present: true, device_key_present: true }),
          unlock,
        })}
      >
        <UnlockScreen reason="idle" onOpened={() => {}} />
      </ClientProvider>,
    )
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "Unlock" }))
    await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent("Rollback attempt blocked"))
    expect(screen.getByText(/without you/)).toBeInTheDocument()
  })
})
