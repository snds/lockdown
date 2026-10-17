import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { toast } from "sonner"

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
  counter: null,
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
    const copyCode = vi.fn<Client["copyCode"]>(() => Promise.resolve({ clears_in_secs: 30, copy: 1 }))
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

  it("a refused copy starts no countdown and says the core's words (CodeDisplay 0.7: a rejected onCopy announces nothing)", async () => {
    const copyCode = vi.fn<Client["copyCode"]>(() => Promise.reject({ code: "locked", message: "The vault is locked." }))
    const shown = vi.spyOn(toast, "error")
    render(
      <ClientProvider value={fakeClient({ codes: () => Promise.resolve([code({ seconds_left: 20 })]), copyCode })}>
        <CodesView rows={[]} onChanged={() => {}} />
      </ClientProvider>,
    )
    const list = await screen.findByRole("list", { name: "One-time codes" })
    await waitFor(() => expect(list.querySelector('[data-slot="code-display-copy"]')).toBeInTheDocument())
    await userEvent.setup().click(list.querySelector('[data-slot="code-display-copy"]') as HTMLElement)
    await waitFor(() => expect(shown).toHaveBeenCalledWith("The vault is locked."))
    expect(list.querySelector("[data-copied]")).not.toBeInTheDocument()
    expect(list.textContent).not.toContain("Copied")
    shown.mockRestore()
  })

  it("at the countdown's end the clear names the shell's copy number, so it runs for that copy only", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true })
    try {
      const copyCode = vi.fn<Client["copyCode"]>(() => Promise.resolve({ clears_in_secs: 30, copy: 7 }))
      const clipboardClear = vi.fn<Client["clipboardClear"]>(() => Promise.resolve())
      render(
        <ClientProvider value={fakeClient({ codes: () => Promise.resolve([code({ seconds_left: 25 })]), copyCode, clipboardClear })}>
          <CodesView rows={[]} onChanged={() => {}} />
        </ClientProvider>,
      )
      const list = await screen.findByRole("list", { name: "One-time codes" })
      await waitFor(() => expect(list.querySelector('[data-slot="code-display-copy"]')).toBeInTheDocument())
      await userEvent.setup({ advanceTimers: vi.advanceTimersByTime }).click(list.querySelector('[data-slot="code-display-copy"]') as HTMLElement)
      await waitFor(() => expect(copyCode).toHaveBeenCalled())
      expect(clipboardClear).not.toHaveBeenCalled()
      await vi.advanceTimersByTimeAsync(31_000)
      await waitFor(() => expect(clipboardClear).toHaveBeenCalledWith(7))
      expect(clipboardClear).toHaveBeenCalledTimes(1)
    } finally {
      vi.useRealTimers()
    }
  })

  it("HOTP: Next code asks the core to move the counter on and shows the code it answers with", async () => {
    const hotp = code({ item_id: "h1", variant: "hotp", current: "969429", next: null, seconds_left: null, counter: 3 })
    const advanceCode = vi.fn<Client["advanceCode"]>(() => Promise.resolve({ code: "338314", counter: 4, version: 2 }))
    const copyCode = vi.fn<Client["copyCode"]>(() => Promise.resolve({ clears_in_secs: 30, copy: 1 }))
    render(
      <ClientProvider value={fakeClient({ codes: () => Promise.resolve([hotp]), advanceCode, copyCode })}>
        <CodesView rows={[]} onChanged={() => {}} />
      </ClientProvider>,
    )
    const list = await screen.findByRole("list", { name: "One-time codes" })
    const next = await screen.findByRole("button", { name: "Next code" })
    expect(next).toBeEnabled()
    expect(list.textContent).toContain("969")
    expect(list.querySelector('[data-slot="code-display-counter"]')).toHaveTextContent("#3")
    const user = userEvent.setup()
    await user.click(next)
    await waitFor(() => expect(advanceCode).toHaveBeenCalledWith("h1", "code.0", {}))
    await waitFor(() => expect(list.textContent).toContain("338"))
    expect(list.textContent).toContain("314")
    expect(list.querySelector('[data-slot="code-display-counter"]')).toHaveTextContent("#4")
    // The copy after it goes by id, so the core copies the code for the stored counter.
    await user.click(list.querySelector('[data-slot="code-display-copy"]') as HTMLElement)
    await waitFor(() => expect(copyCode).toHaveBeenCalledWith("h1", "code.0", 30, "code", {}))
  })

  it("HOTP: a refused step says the core's words and leaves the code as it was", async () => {
    const hotp = code({ item_id: "h1", variant: "hotp", current: "969429", next: null, seconds_left: null, counter: 3 })
    const advanceCode = vi.fn<Client["advanceCode"]>(() => Promise.reject({ code: "locked", message: "The vault is locked." }))
    const shown = vi.spyOn(toast, "error")
    render(
      <ClientProvider value={fakeClient({ codes: () => Promise.resolve([hotp]), advanceCode })}>
        <CodesView rows={[]} onChanged={() => {}} />
      </ClientProvider>,
    )
    const list = await screen.findByRole("list", { name: "One-time codes" })
    await userEvent.setup().click(await screen.findByRole("button", { name: "Next code" }))
    await waitFor(() => expect(shown).toHaveBeenCalledWith("The vault is locked."))
    expect(list.textContent).toContain("969")
    expect(screen.getByRole("button", { name: "Next code" })).toBeEnabled()
    shown.mockRestore()
  })
})
