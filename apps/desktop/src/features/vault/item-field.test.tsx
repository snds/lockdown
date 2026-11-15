import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { SettingsProvider } from "@/app/settings"
import { ClientProvider, type Client } from "@/core/client"
import type { FieldView, Settings } from "@/core/types"

import { SecretFieldRemoteButton, useSecretFieldRemote } from "@unicron/ui/security/secret-field"

import { ItemField } from "./item-field"

const field: FieldView = { field_id: "pw", label: "Password", kind: "hidden", hidden: true, value: null, mask: "••••••••" }
const SECRET = "correct horse"

function fakeClient(overrides: Partial<Client>): Client {
  const refuse = () => Promise.reject({ code: "refused", message: "not in this test" })
  const base = new Proxy({} as Client, { get: () => refuse })
  return Object.assign(Object.create(base), overrides)
}

const settingsWith = (reveal: Settings["reveal"]): Settings => ({
  v: 1,
  density: "comfortable",
  reveal,
  clipboard_clear_secs: 45,
  idle_lock_secs: 600,
  hidden_lock_secs: 300,
  server: null,
})

const secret = () => Promise.resolve(new TextEncoder().encode(SECRET))

function mount(client: Client) {
  return render(
    <ClientProvider value={client}>
      <SettingsProvider>
        <ItemField itemId="i1" field={field} onNotice={() => {}} />
      </SettingsProvider>
    </ClientProvider>,
  )
}

describe("ItemField on unicron's SecretField (the host contract)", () => {
  it("hold (the default setting): the value shows while the pointer is down and masks on release", async () => {
    const reveal = vi.fn(secret)
    const { container } = mount(fakeClient({ reveal, getSettings: () => Promise.resolve(settingsWith("hold")) }))
    const user = userEvent.setup()
    const button = await screen.findByRole("button", { name: "Hold to show Password" })
    expect(container.textContent).not.toContain(SECRET)

    await user.pointer({ keys: "[MouseLeft>]", target: button })
    await waitFor(() => expect(container.textContent).toContain(SECRET))
    expect(reveal).toHaveBeenCalledWith("i1", "pw", {})

    await user.pointer({ keys: "[/MouseLeft]", target: button })
    await waitFor(() => expect(container.textContent).not.toContain(SECRET))
    expect(container.querySelector('[data-slot="secret-field"]')).toHaveAttribute("data-state", "masked")
  })

  it("toggle (the setting): a tap shows and the value stays until the next tap", async () => {
    const { container } = mount(fakeClient({ reveal: secret, getSettings: () => Promise.resolve(settingsWith("toggle")) }))
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "Show Password" }))
    await waitFor(() => expect(container.textContent).toContain(SECRET))
    await new Promise((r) => setTimeout(r, 50))
    expect(container.textContent).toContain(SECRET)
    await user.click(screen.getByRole("button", { name: "Hide Password" }))
    await waitFor(() => expect(container.textContent).not.toContain(SECRET))
  })

  it("copy goes core → clipboard by reference (`copy_secret` with the settings' timer): no reveal, no getSecret, nothing through navigator.clipboard", async () => {
    const copySecret = vi.fn<Client["copySecret"]>(() => Promise.resolve({ clears_in_secs: 45, copy: 7 }))
    const reveal = vi.fn(secret)
    const writeText = vi.fn()
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true })
    const { container } = mount(fakeClient({ reveal, copySecret, getSettings: () => Promise.resolve(settingsWith("toggle")) }))
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "Copy Password" }))
    await waitFor(() => expect(copySecret).toHaveBeenCalledWith("i1", "pw", 45, {}))
    expect(reveal).not.toHaveBeenCalled()
    expect(writeText).not.toHaveBeenCalled()
    expect(container.textContent).not.toContain(SECRET)
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("Copied. Clipboard clears in 45 seconds."))
  })

  it("at expiry the clear asks the shell about its own copy number, and clears only when the shell says it still holds it", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true })
    try {
      const copySecret = vi.fn<Client["copySecret"]>(() => Promise.resolve({ clears_in_secs: 45, copy: 7 }))
      const clipboardHolds = vi.fn<Client["clipboardHolds"]>(() => Promise.resolve(true))
      const clipboardClear = vi.fn<Client["clipboardClear"]>(() => Promise.resolve(true))
      mount(fakeClient({ copySecret, clipboardHolds, clipboardClear, getSettings: () => Promise.resolve(settingsWith("toggle")) }))
      const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime })
      await user.click(await screen.findByRole("button", { name: "Copy Password" }))
      await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("Copied. Clipboard clears in 45 seconds."))
      await vi.advanceTimersByTimeAsync(46_000)
      await waitFor(() => expect(clipboardHolds).toHaveBeenCalledWith(7))
      await waitFor(() => expect(clipboardClear).toHaveBeenCalledTimes(1))
      expect(clipboardClear).toHaveBeenCalledWith(7)
      await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("Clipboard cleared."))
    } finally {
      vi.useRealTimers()
    }
  })

  it("leaves the clipboard alone when the shell's copy is no longer there", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true })
    try {
      const copySecret = vi.fn<Client["copySecret"]>(() => Promise.resolve({ clears_in_secs: 45, copy: 3 }))
      const clipboardHolds = vi.fn<Client["clipboardHolds"]>(() => Promise.resolve(false))
      const clipboardClear = vi.fn<Client["clipboardClear"]>(() => Promise.resolve(true))
      mount(fakeClient({ copySecret, clipboardHolds, clipboardClear, getSettings: () => Promise.resolve(settingsWith("toggle")) }))
      const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime })
      await user.click(await screen.findByRole("button", { name: "Copy Password" }))
      await waitFor(() => expect(copySecret).toHaveBeenCalled())
      await vi.advanceTimersByTimeAsync(46_000)
      await waitFor(() => expect(clipboardHolds).toHaveBeenCalledWith(3))
      expect(clipboardClear).not.toHaveBeenCalled()
    } finally {
      vi.useRealTimers()
    }
  })

  it("a reveal that crosses the bridge (slower than a microtask) still shows: SecretField 0.5.0 keeps its value span mounted while loading", async () => {
    const reveal = () => new Promise<Uint8Array>((r) => setTimeout(() => r(new TextEncoder().encode(SECRET)), 30))
    const { container } = mount(fakeClient({ reveal, getSettings: () => Promise.resolve(settingsWith("toggle")) }))
    await userEvent.setup().click(await screen.findByRole("button", { name: "Show Password" }))
    await waitFor(() => expect(container.textContent).toContain(SECRET))
  })

  it("shows the core's sentence in the error state when a reveal is refused (`errorText`)", async () => {
    const reveal = vi.fn(() => Promise.reject({ code: "refused", message: "'pw' isn't a hidden field" }))
    mount(fakeClient({ reveal, getSettings: () => Promise.resolve(settingsWith("toggle")) }))
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "Show Password" }))
    const alert = await screen.findByRole("alert")
    expect(alert).toHaveTextContent("'pw' isn't a hidden field")
    expect(alert).not.toHaveTextContent("Couldn't read this field")
  })

  it("shows the core's sentence when a copy is refused, and still fetches nothing", async () => {
    const reveal = vi.fn(secret)
    const copySecret = vi.fn<Client["copySecret"]>(() => Promise.reject({ code: "locked", message: "The vault is locked." }))
    mount(fakeClient({ reveal, copySecret, getSettings: () => Promise.resolve(settingsWith("toggle")) }))
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "Copy Password" }))
    const alert = await screen.findByRole("alert")
    expect(alert).toHaveTextContent("The vault is locked.")
    expect(alert).not.toHaveTextContent("Couldn't copy this field")
    expect(reveal).not.toHaveBeenCalled()
  })
})

describe("ItemField's other rows (brief I2, LA5)", () => {
  function WithRemote() {
    const remote = useSecretFieldRemote()
    return (
      <>
        <ItemField itemId="i1" field={field} onNotice={() => {}} remote={remote} />
        <SecretFieldRemoteButton remote={remote} action="reveal" />
        <SecretFieldRemoteButton remote={remote} action="copy" />
      </>
    )
  }
  const mountRemote = (client: Client) =>
    render(
      <ClientProvider value={client}>
        <SettingsProvider>
          <WithRemote />
        </SettingsProvider>
      </ClientProvider>,
    )

  it("the phone's eye follows the Reveal setting: hold", async () => {
    mountRemote(fakeClient({ reveal: secret, getSettings: () => Promise.resolve(settingsWith("hold")) }))
    await waitFor(() => expect(screen.getAllByRole("button", { name: "Hold to show Password" })).toHaveLength(2))
    expect(screen.getAllByRole("button", { name: "Copy Password" })).toHaveLength(2)
  })

  it("the phone's eye follows the Reveal setting: tap, and it shows the value in the field's own row", async () => {
    const { container } = mountRemote(fakeClient({ reveal: secret, getSettings: () => Promise.resolve(settingsWith("toggle")) }))
    await waitFor(() => expect(screen.getAllByRole("button", { name: "Show Password" })).toHaveLength(2))
    await userEvent.setup().click(screen.getAllByRole("button", { name: "Show Password" })[1]!)
    await waitFor(() => expect(container.querySelector('[data-slot="secret-field"]')).toHaveTextContent(SECRET))
  })

  it("Show large type is the first row of the hidden field's ⋯, and reads through the core", async () => {
    const reveal = vi.fn(secret)
    render(
      <ClientProvider value={fakeClient({ reveal, getSettings: () => Promise.resolve(settingsWith("hold")) })}>
        <SettingsProvider>
          <ItemField itemId="i1" field={field} largeType="Password · Thing" onNotice={() => {}} />
        </SettingsProvider>
      </ClientProvider>,
    )
    const user = userEvent.setup({ pointerEventsCheck: 0 })
    await user.click(await screen.findByRole("button", { name: "More actions for Password" }))
    const rows = await screen.findAllByRole("menuitem")
    expect(rows[0]).toHaveTextContent("Show large type")
    await user.click(rows[0]!)
    await waitFor(() => expect(reveal).toHaveBeenCalledWith("i1", "pw", {}))
  })

  it("a plain value says Copied in its own row and raises no toast; a yes or no is a word; a website has Open and Copy", async () => {
    const onNotice = vi.fn()
    const client = fakeClient({ getSettings: () => Promise.resolve(settingsWith("hold")) })
    render(
      <ClientProvider value={client}>
        <SettingsProvider>
          <ItemField itemId="i1" field={{ field_id: "user", label: "Username", kind: "text", hidden: false, value: "sam", mask: null }} onNotice={onNotice} />
          <ItemField itemId="i1" field={{ field_id: "h", label: "Hidden network", kind: "boolean", hidden: false, value: "false", mask: null }} control="boolean" onNotice={onNotice} />
          <ItemField itemId="i1" field={{ field_id: "url.0", label: "Website", kind: "url", hidden: false, value: "https://a.example.test", mask: null }} onNotice={onNotice} />
        </SettingsProvider>
      </ClientProvider>,
    )
    const user = userEvent.setup()
    // After user-event's own clipboard stub, so this one is what the row writes to.
    const writeText = vi.fn(() => Promise.resolve())
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true })
    await user.click(screen.getAllByRole("button", { name: /^Copy/ })[0]!)
    await waitFor(() => expect(writeText).toHaveBeenCalledWith("sam"))
    expect(await screen.findByRole("button", { name: /^Copied/ })).toBeInTheDocument()
    expect(onNotice).not.toHaveBeenCalled()
    expect(screen.getByText("No")).toBeInTheDocument()
    expect(screen.getByRole("button", { name: /^Open/ })).toBeInTheDocument()
  })
})
