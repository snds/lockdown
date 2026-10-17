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

  it("copy goes core → clipboard (`copy_secret` with the settings' timer); nothing reaches navigator.clipboard", async () => {
    const copySecret = vi.fn(() => Promise.resolve(45))
    const writeText = vi.fn()
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true })
    mount(fakeClient({ reveal: secret, copySecret, getSettings: () => Promise.resolve(settingsWith("toggle")) }))
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "Copy Password" }))
    await waitFor(() => expect(copySecret).toHaveBeenCalledWith("i1", "pw", 45, {}))
    expect(writeText).not.toHaveBeenCalled()
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("Copied. Clipboard clears in 45 seconds."))
  })

  it("a reveal that crosses the bridge (slower than a microtask) still shows: the patched SecretField keeps its value span mounted", async () => {
    const reveal = () => new Promise<Uint8Array>((r) => setTimeout(() => r(new TextEncoder().encode(SECRET)), 30))
    const { container } = mount(fakeClient({ reveal, getSettings: () => Promise.resolve(settingsWith("toggle")) }))
    await userEvent.setup().click(await screen.findByRole("button", { name: "Show Password" }))
    await waitFor(() => expect(container.textContent).toContain(SECRET))
  })

  it("shows the core's words under the field when a reveal is refused", async () => {
    const reveal = vi.fn(() => Promise.reject({ code: "refused", message: "'pw' isn't a hidden field" }))
    const { container } = mount(fakeClient({ reveal, getSettings: () => Promise.resolve(settingsWith("toggle")) }))
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "Show Password" }))
    expect(await screen.findByRole("alert")).toHaveTextContent("Couldn't read this field")
    await waitFor(() => expect(container.textContent).toContain("isn't a hidden field"))
  })
})
