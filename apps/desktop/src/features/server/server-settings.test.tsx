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

describe("Settings → Server (brief T5, doc 82; doc 39 §1.11)", () => {
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
    expect(screen.getByText("On this computer only")).toBeInTheDocument()
    const ready = await screen.findByRole("radio", { name: /nas\.example:8443/ })
    // One that would talk plain http to another computer can't be chosen.
    // Unicron's radio is a focusable-off span: it says so with aria-disabled, not the disabled attribute.
    expect(screen.getByRole("radio", { name: /studio\.local/ })).toHaveAttribute("aria-disabled", "true")
    expect(screen.getByText(/Needs https/)).toBeInTheDocument()
    expect(screen.getByRole("heading", { name: /Servers found/ })).toHaveTextContent("02")
    await user.click(ready)
    const sheet = await screen.findByRole("dialog", { name: "Use nas.example:8443?" })
    // Moving the vault asks first: the server's key, focus on Cancel, the yes repeats the verb.
    expect(sheet).toHaveTextContent("Use nas.example:8443?")
    await waitFor(() => expect(within(sheet).getByRole("button", { name: "Cancel" })).toHaveFocus())
    // The server key in DigitGroups: its six groups, as the other end shows them.
    const groups = Array.from(sheet.querySelectorAll('[data-slot="digit-groups-group"]'), (g) => g.textContent)
    expect(groups.join(" ")).toBe("12345 67890 13579 24680 11223 34455")
    await user.click(within(sheet).getByRole("button", { name: "Use this server" }))
    expect(confirm).toHaveBeenCalledWith("https://nas.example:8443")

    const form = await screen.findByRole("dialog", { name: "Move your vault to nas.example:8443" })
    expect(moveToServer).not.toHaveBeenCalled()
    expect(within(form).getByLabelText("Login on this server")).toHaveValue("Sean")
    await user.clear(within(form).getByLabelText("Login on this server"))
    await user.type(within(form).getByLabelText("Login on this server"), "me@example.com")
    expect(within(form).getByRole("button", { name: "Move my vault" })).toHaveAttribute("aria-disabled", "true")
    await user.type(within(form).getByLabelText("Master password"), "orbit lantern pickle meadow")
    await user.click(within(form).getByRole("button", { name: "Move my vault" }))
    expect(await screen.findByRole("alert")).toHaveTextContent("already taken")
    await user.click(within(screen.getByRole("dialog", { name: "Move your vault to nas.example:8443" })).getByRole("button", { name: "Move my vault" }))
    expect(await screen.findByText("Your vault is on nas.example:8443 now")).toBeInTheDocument()
    expect(moveToServer).toHaveBeenLastCalledWith({ address: "https://nas.example:8443", login: "me@example.com", password: "orbit lantern pickle meadow" })
    await waitFor(() => expect(onChanged).toHaveBeenCalled())
  })
  it("asks before moving the vault: Cancel sends nothing and opens no form", async () => {
    const confirm = vi.fn((address: string) => Promise.resolve(readyServer(address)))
    const moveToServer = vi.fn()
    const frc = fakeFirstRun({ candidates: () => Promise.resolve([readyServer("https://nas.example:8443")]), confirm, moveToServer })
    render(
      <ClientProvider value={fakeClient()}>
        <SettingsProvider>
          <ServerSettings frc={frc} status={LOCAL} onNotice={() => {}} onChanged={() => {}} />
        </SettingsProvider>
      </ClientProvider>,
    )
    const user = userEvent.setup()
    await user.click(await screen.findByRole("radio", { name: /nas\.example:8443/ }))
    const question = await screen.findByRole("dialog", { name: "Use nas.example:8443?" })
    // Escape and Cancel never say yes (brief 4.0).
    await user.keyboard("{Escape}")
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument())
    expect(question).not.toBeInTheDocument()
    await user.click(screen.getByRole("radio", { name: /nas\.example:8443/ }))
    await user.click(within(await screen.findByRole("dialog", { name: "Use nas.example:8443?" })).getByRole("button", { name: "Cancel" }))
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument())
    expect(confirm).not.toHaveBeenCalled()
    expect(moveToServer).not.toHaveBeenCalled()
  })
})
