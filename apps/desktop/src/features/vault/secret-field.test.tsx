import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { ClientProvider, type Client } from "@/core/client"
import type { FieldView } from "@/core/types"

import { SecretField } from "./secret-field"

const field: FieldView = { field_id: "pw", label: "password", kind: "hidden", hidden: true, value: null, mask: "••••••••" }

function fakeClient(overrides: Partial<Client>): Client {
  const refuse = () => Promise.reject({ code: "refused", message: "not in this test" })
  const base = new Proxy({} as Client, { get: () => refuse })
  return Object.assign(Object.create(base), overrides)
}

const secret = () => Promise.resolve(new TextEncoder().encode("correct horse"))

describe("the reveal gesture (D97)", () => {
  it("hold: the value shows while the pointer is down and masks on release", async () => {
    const reveal = vi.fn(secret)
    render(
      <ClientProvider value={fakeClient({ reveal })}>
        <SecretField itemId="i1" field={field} onNotice={() => {}} gesture="hold" />
      </ClientProvider>,
    )
    const user = userEvent.setup()
    const button = screen.getByRole("button", { name: /Hold to reveal/ })
    expect(screen.getByText("••••••••")).toBeInTheDocument()

    await user.pointer({ keys: "[MouseLeft>]", target: button })
    await screen.findByText("correct horse")
    expect(button).toHaveAttribute("aria-pressed", "true")

    await user.pointer({ keys: "[/MouseLeft]", target: button })
    await waitFor(() => expect(screen.queryByText("correct horse")).not.toBeInTheDocument())
    expect(screen.getByText("••••••••")).toBeInTheDocument()
    expect(reveal).toHaveBeenCalledTimes(1)
  })

  it("hold: Space held shows, Space released hides", async () => {
    render(
      <ClientProvider value={fakeClient({ reveal: secret })}>
        <SecretField itemId="i1" field={field} onNotice={() => {}} gesture="hold" />
      </ClientProvider>,
    )
    const user = userEvent.setup()
    const button = screen.getByRole("button", { name: /Hold to reveal/ })
    button.focus()
    await user.keyboard("{ >}")
    await screen.findByText("correct horse")
    await user.keyboard("{/ }")
    await waitFor(() => expect(screen.queryByText("correct horse")).not.toBeInTheDocument())
  })

  it("toggle: a tap shows and the value stays until the next tap", async () => {
    render(
      <ClientProvider value={fakeClient({ reveal: secret })}>
        <SecretField itemId="i1" field={field} onNotice={() => {}} gesture="toggle" />
      </ClientProvider>,
    )
    const user = userEvent.setup()
    await user.click(screen.getByRole("button", { name: "Reveal" }))
    await screen.findByText("correct horse")
    // Nothing is held; the value stays.
    await new Promise((r) => setTimeout(r, 50))
    expect(screen.getByText("correct horse")).toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: "Hide" }))
    expect(screen.queryByText("correct horse")).not.toBeInTheDocument()
  })

  it("shows the core's words when a reveal is refused", async () => {
    const reveal = vi.fn(() => Promise.reject({ code: "refused", message: "'pw' isn't a hidden field" }))
    render(
      <ClientProvider value={fakeClient({ reveal })}>
        <SecretField itemId="i1" field={field} onNotice={() => {}} gesture="toggle" />
      </ClientProvider>,
    )
    const user = userEvent.setup()
    await user.click(screen.getByRole("button", { name: "Reveal" }))
    expect(await screen.findByRole("alert")).toHaveTextContent("isn't a hidden field")
  })
})
