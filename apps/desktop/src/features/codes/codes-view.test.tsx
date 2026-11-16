import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { toast } from "@unicron/ui"

import { ClientProvider, type Client } from "@/core/client"
import type { CodeView } from "@/core/types"
import { malformedLists } from "@/test/a11y"

import { CodesView, SEARCH_FROM } from "./codes-view"

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
  it("with no codes, says so as the page's empty state with ADD CODE, and adds by paste, never by camera (brief C1; review R5)", async () => {
    render(
      <ClientProvider value={fakeClient({ codes: () => Promise.resolve([]) })}>
        <CodesView rows={[]} onChanged={() => {}} />
      </ClientProvider>,
    )
    expect(await screen.findByText("No codes yet.")).toBeInTheDocument()
    expect(screen.getAllByText("Codes // 00").length).toBeGreaterThan(0)
    expect(screen.getByText("Add one from the set-up link a site gives you.")).toBeInTheDocument()
    const adds = screen.getAllByRole("button", { name: "Add code" })
    await userEvent.setup().click(adds[adds.length - 1]!)
    const dialog = await screen.findByRole("dialog", { name: "Add a code" })
    expect(within(dialog).getByText("Paste the otpauth:// or steam:// link the site gave you.")).toBeInTheDocument()
    expect(within(dialog).queryByText(/scan|camera/i)).not.toBeInTheDocument()
  })

  it("the page head counts the codes through lib/format, with ADD CODE and SIGN IN TV", async () => {
    render(
      <ClientProvider value={fakeClient({ codes: () => Promise.resolve([code({}), code({ item_id: "i2", title: "Mail", issuer: "Mail" })]) })}>
        <CodesView rows={[]} onChanged={() => {}} />
      </ClientProvider>,
    )
    expect(await screen.findByText("Codes // 002")).toBeInTheDocument()
    expect(screen.getByRole("heading", { name: "Codes" })).toBeInTheDocument()
    expect(screen.getByRole("button", { name: "Add code" })).toBeInTheDocument()
    expect(screen.getByRole("button", { name: "Sign in TV" })).toBeInTheDocument()
  })

  it("reads a code's digits one by one, never as a number", async () => {
    render(
      <ClientProvider value={fakeClient({ codes: () => Promise.resolve([code({})]) })}>
        <CodesView rows={[]} onChanged={() => {}} />
      </ClientProvider>,
    )
    const list = await screen.findByRole("list", { name: "One-time codes" })
    await waitFor(() => expect(list.querySelector('[data-slot="code-display"]')).toBeInTheDocument())
    const named = list.querySelector('[aria-label*="Code for GitHub, sean:"]')
    expect(named?.getAttribute("aria-label")).toContain("4 8 2, 9 1 7")
    expect(named?.getAttribute("aria-label")).not.toContain("482917")
  })

  it("searches issuers and accounts from ten codes up, and says when nothing matches", async () => {
    const many = Array.from({ length: SEARCH_FROM }, (_, i) => code({ item_id: `i${i}`, issuer: i === 3 ? "Fastmail" : `Issuer ${i}`, title: `Item ${i}` }))
    render(
      <ClientProvider value={fakeClient({ codes: () => Promise.resolve(many) })}>
        <CodesView rows={[]} onChanged={() => {}} />
      </ClientProvider>,
    )
    const search = await screen.findByRole("searchbox", { name: "Search codes" })
    // `paste` puts the words in at once: typing them a key at a time is slow on a loaded machine.
    const user = userEvent.setup()
    await user.click(search)
    await user.paste("fastmail")
    const list = screen.getByRole("list", { name: "One-time codes" })
    expect(within(list).getAllByRole("listitem")).toHaveLength(1)
    await user.clear(search)
    await user.paste("nothing here")
    expect(await screen.findByText("No code for nothing here.")).toBeInTheDocument()
    expect(screen.getByText("Search // 00")).toBeInTheDocument()
  })

  it("under ten codes there is no search field", async () => {
    render(
      <ClientProvider value={fakeClient({ codes: () => Promise.resolve([code({})]) })}>
        <CodesView rows={[]} onChanged={() => {}} />
      </ClientProvider>,
    )
    await screen.findByRole("list", { name: "One-time codes" })
    expect(screen.queryByRole("textbox", { name: "Search codes" })).not.toBeInTheDocument()
  })

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

  it("is a list with one item per code, and CodeDisplayList's live region sits beside it, not in it (design review R7)", async () => {
    const { container } = render(
      <ClientProvider value={fakeClient({ codes: () => Promise.resolve([code({}), code({ item_id: "i2", title: "Mail", issuer: "Mail" })]) })}>
        <CodesView rows={[]} onChanged={() => {}} />
      </ClientProvider>,
    )
    const list = await screen.findByRole("list", { name: "One-time codes" })
    await waitFor(() => expect(within(list).getAllByRole("listitem")).toHaveLength(2))
    for (const item of within(list).getAllByRole("listitem")) expect(item.querySelector('[data-slot="code-display"]')).toBeInTheDocument()
    const live = container.querySelector('[data-slot="code-display-live"]')
    expect(live).toBeInTheDocument()
    expect(list.contains(live)).toBe(false)
    expect(malformedLists(container)).toEqual([])
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
      const clipboardClear = vi.fn<Client["clipboardClear"]>(() => Promise.resolve(true))
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
      await waitFor(() => expect(list.textContent).toContain("Cleared"))
    } finally {
      vi.useRealTimers()
    }
  })

  it("when the shell leaves the clipboard alone (the person copied something else), the code never says Cleared", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true })
    try {
      const copyCode = vi.fn<Client["copyCode"]>(() => Promise.resolve({ clears_in_secs: 30, copy: 8 }))
      const clipboardClear = vi.fn<Client["clipboardClear"]>(() => Promise.resolve(false))
      render(
        <ClientProvider value={fakeClient({ codes: () => Promise.resolve([code({ seconds_left: 25 })]), copyCode, clipboardClear })}>
          <CodesView rows={[]} onChanged={() => {}} />
        </ClientProvider>,
      )
      const list = await screen.findByRole("list", { name: "One-time codes" })
      await waitFor(() => expect(list.querySelector('[data-slot="code-display-copy"]')).toBeInTheDocument())
      await userEvent.setup({ advanceTimers: vi.advanceTimersByTime }).click(list.querySelector('[data-slot="code-display-copy"]') as HTMLElement)
      await waitFor(() => expect(copyCode).toHaveBeenCalled())
      await vi.advanceTimersByTimeAsync(31_000)
      await waitFor(() => expect(clipboardClear).toHaveBeenCalledWith(8))
      await vi.advanceTimersByTimeAsync(1_000)
      expect(list.textContent).not.toContain("Cleared")
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
    expect(list.querySelector('[data-slot="code-display-counter"]')).toHaveTextContent("Counter 3")
    const user = userEvent.setup()
    await user.click(next)
    await waitFor(() => expect(advanceCode).toHaveBeenCalledWith("h1", "code.0", {}))
    await waitFor(() => expect(list.textContent).toContain("338"))
    expect(list.textContent).toContain("314")
    expect(list.querySelector('[data-slot="code-display-counter"]')).toHaveTextContent("Counter 4")
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

  it("a code's ⋯ holds Open item, Show large type and Copy; its Copy is the code's own copy (C18)", async () => {
    const copyCode = vi.fn<Client["copyCode"]>(() => Promise.resolve({ clears_in_secs: 30, copy: 3 }))
    const onOpenItem = vi.fn()
    render(
      <ClientProvider value={fakeClient({ codes: () => Promise.resolve([code({ seconds_left: 20 })]), copyCode })}>
        <CodesView rows={[]} onChanged={() => {}} onOpenItem={onOpenItem} />
      </ClientProvider>,
    )
    const user = userEvent.setup()
    const more = await screen.findByRole("button", { name: /More actions for GitHub/ })
    await user.click(more)
    expect(await screen.findByRole("menuitem", { name: "Show large type" })).toBeInTheDocument()
    await user.click(screen.getByRole("menuitem", { name: "Copy" }))
    await waitFor(() => expect(copyCode).toHaveBeenCalledWith("i1", "code.0", 30, "code", {}))
    await user.click(screen.getByRole("button", { name: /More actions for GitHub/ }))
    await user.click(await screen.findByRole("menuitem", { name: "Open item" }))
    expect(onOpenItem).toHaveBeenCalledWith("i1")
  })

  it("without a way to open the item, the ⋯ offers no Open item", async () => {
    render(
      <ClientProvider value={fakeClient({ codes: () => Promise.resolve([code({})]) })}>
        <CodesView rows={[]} onChanged={() => {}} />
      </ClientProvider>,
    )
    await userEvent.setup().click(await screen.findByRole("button", { name: /More actions for GitHub/ }))
    expect(await screen.findByRole("menuitem", { name: "Show large type" })).toBeInTheDocument()
    expect(screen.queryByRole("menuitem", { name: "Open item" })).not.toBeInTheDocument()
  })
})
