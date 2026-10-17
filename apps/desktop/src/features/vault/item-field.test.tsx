import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { SettingsProvider } from "@/app/settings"
import { ClientProvider, type Client } from "@/core/client"
import type { FieldView, Settings } from "@/core/types"

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
      const clipboardClear = vi.fn<Client["clipboardClear"]>(() => Promise.resolve())
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
      const clipboardClear = vi.fn<Client["clipboardClear"]>(() => Promise.resolve())
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
