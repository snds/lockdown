import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { ClientProvider, type Client } from "@/core/client"
import type { CodeView } from "@/core/types"

import { CodesView } from "./codes-view"

function fakeClient(overrides: Partial<Client>): Client {
  const refuse = () => Promise.reject({ code: "refused", message: "not in this test" })
  const base = new Proxy({} as Client, { get: () => refuse })
  return Object.assign(Object.create(base), overrides)
}

const code = (over: Partial<CodeView>): CodeView => ({
  item_id: "i1",
  field_id: "code.0",
  title: "GitHub (sean)",
  issuer: "GitHub",
  account: "sean",
  variant: "totp",
  digits: 6,
  period: 30,
  current: "482917",
  next: "330558",
  seconds_left: 8,
  warnings: [],
  ...over,
})

describe("the Codes view on unicron's CodeDisplay", () => {
  it("shows the current code and, in its last seconds, the next one from the command output", async () => {
    const codes = vi.fn(() => Promise.resolve([code({})]))
    render(
      <ClientProvider value={fakeClient({ codes })}>
        <CodesView rows={[]} onChanged={() => {}} />
      </ClientProvider>,
    )
    const list = await screen.findByRole("list", { name: "One-time codes" })
    await waitFor(() => expect(list.querySelector('[data-slot="code-display"]')).toBeInTheDocument())
    expect(list.textContent).toContain("482")
    expect(list.textContent).toContain("917")
    const next = list.querySelector('[data-slot="code-display-next"]')
    expect(next).toBeInTheDocument()
    expect(next!.textContent).toContain("330")
    expect(next!.textContent).toContain("558")
    expect(list.textContent).not.toContain("secret")
  })

  it("copies through the core's clipboard path (`copy_code`), never the value through the webview", async () => {
    const copyCode = vi.fn(() => Promise.resolve(30))
    const writeText = vi.fn()
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true })
    render(
      <ClientProvider value={fakeClient({ codes: () => Promise.resolve([code({ seconds_left: 20 })]), copyCode })}>
        <CodesView rows={[]} onChanged={() => {}} />
      </ClientProvider>,
    )
    const list = await screen.findByRole("list", { name: "One-time codes" })
    await waitFor(() => expect(list.querySelector('[data-slot="code-display-copy"]')).toBeInTheDocument())
    await userEvent.setup().click(list.querySelector('[data-slot="code-display-copy"]') as HTMLElement)
    await waitFor(() => expect(copyCode).toHaveBeenCalledWith("i1", "code.0", 30, "code", {}))
    expect(writeText).not.toHaveBeenCalled()
  })
})
