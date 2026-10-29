import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { DEFAULT_SETTINGS, SettingsProvider } from "@/app/settings"
import { ClientProvider, type Client } from "@/core/client"
import type { Status } from "@/core/types"
import { fakeFirstRun, readyServer } from "@/test/fake-first-run"

import { ServerSettings } from "./server-settings"

function fakeClient(): Client {
  const refuse = () => Promise.reject({ code: "refused", message: "not in this test" })
  const base = new Proxy({} as Client, { get: () => refuse })
  return Object.assign(Object.create(base), {
    getSettings: () => Promise.resolve(DEFAULT_SETTINGS),
    setSettings: (s: unknown) => Promise.resolve(s),
  })
}

const LOCAL = { account: { local: true, login: "Sean", server: "lockdown-local://this-device" } } as unknown as Status

describe("Settings → Server (doc 39 §1.11)", () => {
  it("lists what was found, says the vault is kept here, and moves it to a confirmed server", async () => {
    const confirm = vi.fn((address: string) => Promise.resolve(readyServer(address)))
    const moveToServer = vi
      .fn()
      .mockRejectedValueOnce({ code: "refused", message: "The move stopped: me@example.com is already taken on that server. Choose another login. Your vault is unchanged and still on this computer. You can try again." })
      .mockResolvedValueOnce({ lines: [] })
    const onChanged = vi.fn()
    const frc = fakeFirstRun({
      candidates: () =>
        Promise.resolve([
          readyServer("https://nas.example:8443"),
          readyServer("http://studio.local:8787", { state: "needs_https", reason: "Lockdown only talks to a server on another computer over https.", fingerprint: null }),
        ]),
      confirm,
      moveToServer,
    })
    render(
      <ClientProvider value={fakeClient()}>
        <SettingsProvider>
          <ServerSettings frc={frc} status={LOCAL} onNotice={() => {}} onChanged={onChanged} />
        </SettingsProvider>
      </ClientProvider>,
    )
    const user = userEvent.setup()
    expect(screen.getByText(/Your vault is kept on this computer only/)).toBeInTheDocument()
    const ready = await screen.findByRole("radio", { name: /nas\.example:8443/ })
    // One that would talk plain http to another computer can't be chosen.
    expect(screen.getByRole("radio", { name: /studio\.local/ })).toBeDisabled()
    expect(screen.getByText(/Needs https/)).toBeInTheDocument()
    await user.click(ready)
    const sheet = await screen.findByRole("dialog")
    expect(sheet).toHaveTextContent("Move your vault to nas.example:8443?")
    expect(sheet).toHaveTextContent("12345 67890 13579 24680 11223 34455")
    await user.click(within(sheet).getByRole("button", { name: "Continue" }))
    expect(confirm).toHaveBeenCalledWith("https://nas.example:8443")

    const form = await screen.findByRole("dialog")
    expect(form).toHaveTextContent("Your login on nas.example:8443")
    expect(within(form).getByLabelText("Login on this server")).toHaveValue("Sean")
    await user.clear(within(form).getByLabelText("Login on this server"))
    await user.type(within(form).getByLabelText("Login on this server"), "me@example.com")
    await user.type(within(form).getByLabelText("Master password"), "orbit lantern pickle meadow")
    await user.click(within(form).getByRole("button", { name: "Move my vault" }))
    expect(await screen.findByRole("alert")).toHaveTextContent("already taken")
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Move my vault" }))
    expect(await screen.findByText("Your vault is on nas.example:8443 now")).toBeInTheDocument()
    expect(moveToServer).toHaveBeenLastCalledWith({ address: "https://nas.example:8443", login: "me@example.com", password: "orbit lantern pickle meadow" })
    await waitFor(() => expect(onChanged).toHaveBeenCalled())
  })
})
