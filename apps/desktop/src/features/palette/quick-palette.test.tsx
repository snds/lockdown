import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useState } from "react"

import { GateProvider } from "@/app/gate"
import { SettingsProvider } from "@/app/settings"
import { TemplatesProvider } from "@/app/templates"
import { ClientProvider, type Client } from "@/core/client"
import type { GateInput, ItemSummary } from "@/core/types"
import { fakeClient } from "@/test/fake-client"
import { fakeQuick } from "@/test/fake-quick"
import { HygieneClientProvider, type HygieneClient } from "@/features/hygiene/hygiene-client"
import { QuickClientProvider, type QuickClient } from "@/features/quick/quick-client"
import { TEMPLATES } from "@/test/templates-fixture"

import { matches, searchWords } from "./palette-model"
import { QuickPalette } from "./quick-palette"

const row = (over: Partial<ItemSummary> & Pick<ItemSummary, "id" | "kind" | "title">): ItemSummary => ({
  state: "live",
  site: "",
  username: "",
  edited_at: 0,
  crown_jewel: false,
  methods: [],
  passkeys: 0,
  codes: 0,
  expires_at: null,
  conflict_copy: false,
  warnings: [],
  views: [],
  subtitle: "",
  copy: null,
  ...over,
})

const ROWS: ItemSummary[] = [
  row({ id: "l1", kind: "login", title: "Forum", site: "https://forum.example.com/login", username: "me", subtitle: "me", copy: { field_id: "pw", how: "secret", label: "password" } }),
  row({ id: "w1", kind: "wi-fi", title: "Home", subtitle: "home-net", copy: { field_id: "wifi.passphrase", how: "secret", label: "Wi-Fi password" } }),
  row({ id: "c1", kind: "code", title: "Bank code", copy: { field_id: "code.0", how: "code", label: "code" } }),
  row({ id: "j1", kind: "crypto-wallet", title: "Cold wallet", crown_jewel: true, copy: null }),
  row({ id: "t1", kind: "note", title: "Old plans", state: "trashed" }),
]

function Harness({
  client,
  onOpen,
  onNotice,
  rows = ROWS,
  quick,
  hygiene,
}: {
  client: Client
  onOpen: (id: string) => void
  onNotice: (t: string) => void
  rows?: ItemSummary[]
  quick?: QuickClient
  hygiene?: HygieneClient
}) {
  const [open, setOpen] = useState(false)
  const palette = <QuickPalette open={open} onOpenChange={setOpen} rows={rows} onOpen={onOpen} onNotice={onNotice} />
  return (
    <ClientProvider value={client}>
      <SettingsProvider>
        <TemplatesProvider>
          <GateProvider>
            <QuickClientProvider value={quick ?? fakeQuick(rows)}>
              <p>window</p>
              {hygiene ? <HygieneClientProvider value={hygiene}>{palette}</HygieneClientProvider> : palette}
            </QuickClientProvider>
          </GateProvider>
        </TemplatesProvider>
      </SettingsProvider>
    </ClientProvider>
  )
}

describe("the quick-access palette (D111)", () => {
  it("finds by title, username, website host and kind, from the summaries only", () => {
    const words = (id: string) => searchWords(ROWS.find((r) => r.id === id)!, "")
    expect(matches("forum.example", words("l1"))).toBe(true)
    expect(matches("me forum", words("l1"))).toBe(true)
    expect(matches("home-net", words("w1"))).toBe(true)
    expect(matches("wi-fi", words("w1"))).toBe(true)
    expect(matches("cold", words("j1"))).toBe(true)
    expect(matches("login", words("w1"))).toBe(false)
  })

  // The palette keeps the vault list's rule (every word a plain substring, any order) over each
  // item's keywords, not cmdk's fuzzy ranking: Ctrl+Enter copies the highlighted result, and a
  // scattered-letter match ranked first would copy the wrong item's password (10-desktop-shell §12).
  it("matches on keywords alone: a title word, a host, words in any order; never the id, never scattered letters", async () => {
    const ID = { github: "4f3a9c1e-0b7d-4e2a-9f10-1c2d3e4f5a6b", mail: "9b8c7d6e-5f4a-4b3c-8d2e-1f0a9b8c7d6e" }
    const rows = [
      row({ id: ID.github, kind: "login", title: "GitHub", site: "https://github.com/login", username: "sean@example.com", subtitle: "sean@example.com", copy: { field_id: "pw", how: "secret", label: "password" } }),
      row({ id: ID.mail, kind: "login", title: "Mail", site: "https://mail.google.com", username: "sean", subtitle: "sean", copy: { field_id: "pw", how: "secret", label: "password" } }),
    ]
    const copySecret = vi.fn<Client["copySecret"]>(() => Promise.resolve({ clears_in_secs: 30, copy: 1 }))
    render(<Harness client={fakeClient({ templates: () => Promise.resolve(TEMPLATES), copySecret })} rows={rows} onOpen={() => {}} onNotice={() => {}} />)
    const user = userEvent.setup()
    await user.keyboard("{Control>}k{/Control}")
    const input = await screen.findByPlaceholderText(/Search titles/)
    const shown = () => screen.queryAllByRole("option").map((o) => o.textContent ?? "")
    const search = async (text: string) => {
      await user.clear(input)
      if (text) await user.type(input, text)
    }

    await search("github") // a title word
    await waitFor(() => expect(shown()).toEqual([expect.stringContaining("GitHub")]))
    await search("mail.google") // the website's host
    await waitFor(() => expect(shown()).toEqual([expect.stringContaining("Mail")]))
    await search("example sean git") // words in any order
    await waitFor(() => expect(shown()).toEqual([expect.stringContaining("GitHub")]))
    await search("4f3a") // the id's letters: an id is a key, not a word
    await waitFor(() => expect(screen.getByText("No matches. Try fewer letters.")).toBeInTheDocument())
    // Scattered letters: cmdk's ranking puts GitHub first for "gml" (g…m…l across its keywords);
    // under the list's rule nothing matches, so Ctrl+Enter can't copy a near miss.
    await search("gml")
    await waitFor(() => expect(screen.getByText("No matches. Try fewer letters.")).toBeInTheDocument())
    await user.keyboard("{Control>}{Enter}{/Control}")
    expect(copySecret).not.toHaveBeenCalled()
    // An empty search lists both, each with an explicit value, its copy described with its key.
    await search("")
    await waitFor(() => expect(shown()).toHaveLength(2))
    for (const option of screen.getAllByRole("option")) {
      expect(option).toHaveAttribute("data-value", expect.stringMatching(/^[0-9a-f-]{36}$/))
      expect(option).toHaveAttribute("aria-keyshortcuts", "Control+Enter")
      expect(option).toHaveAccessibleDescription("Copy password, Control Enter")
    }
  })

  it("the copy control on a result copies without opening the item", async () => {
    const copySecret = vi.fn<Client["copySecret"]>(() => Promise.resolve({ clears_in_secs: 30, copy: 1 }))
    const onOpen = vi.fn()
    render(<Harness client={fakeClient({ templates: () => Promise.resolve(TEMPLATES), copySecret })} onOpen={onOpen} onNotice={() => {}} />)
    const user = userEvent.setup()
    await user.keyboard("{Control>}k{/Control}")
    await user.type(await screen.findByPlaceholderText(/Search titles/), "home")
    // The button is hidden from assistive tech (the result describes it); a pointer still clicks it.
    const home = await screen.findByRole("option", { description: "Copy Wi-Fi password, Control Enter" })
    await user.click(home.querySelector<HTMLElement>('[data-slot="command-item-action"]')!)
    await waitFor(() => expect(copySecret).toHaveBeenCalledWith("w1", "wifi.passphrase", 30, {}))
    expect(onOpen).not.toHaveBeenCalled()
  })

  it("opens on Ctrl+K, filters, opens an item on Enter, and leaves trashed items out", async () => {
    const onOpen = vi.fn()
    render(<Harness client={fakeClient({ templates: () => Promise.resolve(TEMPLATES) })} onOpen={onOpen} onNotice={() => {}} />)
    const user = userEvent.setup()
    await user.keyboard("{Control>}k{/Control}")
    const input = await screen.findByPlaceholderText("Search titles, usernames, websites, kinds")
    expect(screen.queryByText("Old plans")).not.toBeInTheDocument()
    await user.type(input, "home")
    await waitFor(() => expect(screen.queryByText("Forum")).not.toBeInTheDocument())
    expect(screen.getByText("Home")).toBeInTheDocument()
    await user.keyboard("{Enter}")
    expect(onOpen).toHaveBeenCalledWith("w1")
  })

  it("Ctrl+Enter copies the highlighted result's password through the core, by id and field", async () => {
    const copySecret = vi.fn<Client["copySecret"]>(() => Promise.resolve({ clears_in_secs: 30, copy: 1 }))
    const onNotice = vi.fn()
    render(<Harness client={fakeClient({ templates: () => Promise.resolve(TEMPLATES), copySecret })} onOpen={() => {}} onNotice={onNotice} />)
    const user = userEvent.setup()
    await user.keyboard("{Control>}k{/Control}")
    await user.type(await screen.findByPlaceholderText(/Search titles/), "forum")
    await user.keyboard("{Control>}{Enter}{/Control}")
    await waitFor(() => expect(copySecret).toHaveBeenCalledWith("l1", "pw", 30, {}))
    expect(onNotice).toHaveBeenCalledWith("Copied the password. The clipboard clears in 30 seconds.")
  })

  it("a crown jewel's copy asks first (the core's gate)", async () => {
    const copySecret = vi.fn<Client["copySecret"]>((_id, _f, _s, gate?: GateInput) =>
      gate?.confirmed ? Promise.resolve({ clears_in_secs: 30, copy: 2 }) : Promise.reject({ code: "approval_needed", detail: "window", message: "confirm in this window to copy this protected value" }),
    )
    const rows = ROWS.map((r): ItemSummary => (r.id === "j1" ? { ...r, copy: { field_id: "seed", how: "secret", label: "recovery phrase" } } : r))
    const onNotice = vi.fn()
    function Many() {
      const [open, setOpen] = useState(true)
      return <QuickPalette open={open} onOpenChange={setOpen} rows={rows} onOpen={() => {}} onNotice={onNotice} />
    }
    render(
      <ClientProvider value={fakeClient({ templates: () => Promise.resolve(TEMPLATES), copySecret, approvalRoute: () => Promise.resolve({ route: "window", method: "window" }) })}>
        <SettingsProvider>
          <TemplatesProvider>
            <GateProvider>
              <QuickClientProvider value={fakeQuick(rows)}>
                <Many />
              </QuickClientProvider>
            </GateProvider>
          </TemplatesProvider>
        </SettingsProvider>
      </ClientProvider>,
    )
    const user = userEvent.setup()
    const wallet = await screen.findByRole("option", { description: "Copy recovery phrase, Control Enter" })
    await user.click(wallet.querySelector<HTMLElement>('[data-slot="command-item-action"]')!)
    expect(await screen.findByRole("heading", { name: "Are you sure?" })).toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: "Confirm" }))
    await waitFor(() => expect(copySecret).toHaveBeenLastCalledWith("j1", "seed", 30, { confirmed: true }))
    expect(onNotice).toHaveBeenCalledWith("Copied the recovery phrase. The clipboard clears in 30 seconds.")
  })

  it("has its search in focus when it opens, and says when a result is a close match", async () => {
    render(<Harness client={fakeClient({ templates: () => Promise.resolve(TEMPLATES) })} onOpen={() => {}} onNotice={() => {}} quick={fakeQuick(ROWS, {}, { forun: ["l1"] })} />)
    const user = userEvent.setup()
    await user.keyboard("{Control>}k{/Control}")
    const input = await screen.findByPlaceholderText(/Search titles/)
    await waitFor(() => expect(input).toHaveFocus())
    await user.type(input, "forun")
    await waitFor(() => expect(screen.getAllByRole("option")).toHaveLength(1))
    expect(screen.getByRole("option", { name: /Forum/ })).toHaveTextContent("Close match")
  })

  it("Alt+Enter shows a code in place, kept out of screen captures, through the core", async () => {
    const quickCode = vi.fn<QuickClient["quickCode"]>(() => Promise.resolve(new TextEncoder().encode("482913")))
    const protect = vi.fn<HygieneClient["protect"]>(() => Promise.resolve(true))
    render(
      <Harness
        client={fakeClient({ templates: () => Promise.resolve(TEMPLATES) })}
        onOpen={() => {}}
        onNotice={() => {}}
        quick={fakeQuick(ROWS, { quickCode })}
        hygiene={{ protect, memory: () => Promise.resolve(null) }}
      />,
    )
    const user = userEvent.setup()
    await user.keyboard("{Control>}k{/Control}")
    await user.type(await screen.findByPlaceholderText(/Search titles/), "bank")
    await waitFor(() => expect(screen.getAllByRole("option")).toHaveLength(1))
    await user.keyboard("{Alt>}{Enter}{/Alt}")
    expect(await screen.findByLabelText("Code 4 8 2 9 1 3")).toBeInTheDocument()
    expect(quickCode).toHaveBeenCalledWith("c1", "code.0", {})
    expect(protect).toHaveBeenCalledWith(true)
    // The action bar offers it by pointer too; a login has no code to show.
    expect(screen.getByRole("button", { name: /Show code/ })).toBeInTheDocument()
  })

  it("Shift+Enter fills a login in the browser; never a crown jewel", async () => {
    const quickFill = vi.fn<QuickClient["quickFill"]>(() => Promise.resolve({ url: "https://forum.example.com/login", host: "forum.example.com", waits_secs: 120 }))
    const onNotice = vi.fn()
    render(<Harness client={fakeClient({ templates: () => Promise.resolve(TEMPLATES) })} onOpen={() => {}} onNotice={onNotice} quick={fakeQuick(ROWS, { quickFill })} />)
    const user = userEvent.setup()
    await user.keyboard("{Control>}k{/Control}")
    await user.type(await screen.findByPlaceholderText(/Search titles/), "forum")
    await waitFor(() => expect(screen.getAllByRole("option")).toHaveLength(1))
    await screen.findByRole("button", { name: /Fill in the browser/ })
    await user.keyboard("{Shift>}{Enter}{/Shift}")
    await waitFor(() => expect(quickFill).toHaveBeenCalledWith("l1"))
    expect(onNotice).toHaveBeenCalledWith(expect.stringContaining("Opened forum.example.com"))
    await user.keyboard("{Control>}k{/Control}")
    await user.type(await screen.findByPlaceholderText(/Search titles/), "cold")
    await waitFor(() => expect(screen.getAllByRole("option")).toHaveLength(1))
    expect(screen.queryByRole("button", { name: /Fill in the browser/ })).not.toBeInTheDocument()
  })

  it("the quick-access window draws it in place; Escape hides the window", async () => {
    const hide = vi.fn<QuickClient["hide"]>(() => Promise.resolve())
    const openMain = vi.fn<QuickClient["openMain"]>(() => Promise.resolve())
    render(
      <ClientProvider value={fakeClient({ templates: () => Promise.resolve(TEMPLATES) })}>
        <SettingsProvider>
          <TemplatesProvider>
            <GateProvider>
              <QuickClientProvider value={fakeQuick(ROWS, { hide, openMain })}>
                <QuickPalette inline open onOpenChange={() => {}} rows={ROWS} onOpen={() => {}} onNotice={() => {}} />
              </QuickClientProvider>
            </GateProvider>
          </TemplatesProvider>
        </SettingsProvider>
      </ClientProvider>,
    )
    const user = userEvent.setup()
    const input = await screen.findByPlaceholderText(/Search titles/)
    await waitFor(() => expect(input).toHaveFocus())
    await user.type(input, "home")
    await waitFor(() => expect(screen.getAllByRole("option")).toHaveLength(1))
    await user.keyboard("{Enter}")
    await waitFor(() => expect(openMain).toHaveBeenCalledWith("w1"))
    expect(hide).toHaveBeenCalled()
    await user.type(input, "x")
    await user.keyboard("{Escape}")
    expect(hide).toHaveBeenCalledTimes(2)
  })
})
