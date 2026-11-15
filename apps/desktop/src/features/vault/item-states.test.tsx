// An item's states (brief I8) and large type on its hidden fields (brief I9, D242): the conflict as
// needs-you row 5 or a waiting row, the purge's Question, and Show large type by tap and by keyboard.

import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { GateProvider } from "@/app/gate"
import { NeedsYouSlot, WaitingHost, addNeedsYou } from "@/app/needs-you"
import { SettingsProvider } from "@/app/settings"
import { TemplatesProvider } from "@/app/templates"
import { ClientProvider, type Client } from "@/core/client"
import type { FieldView, ItemDetail, ItemSummary } from "@/core/types"
import { fakeClient } from "@/test/fake-client"
import { TEMPLATES } from "@/test/templates-fixture"

import { DetailPane } from "./detail-pane"

const plain = (field_id: string, label: string, value: string, kind = "text"): FieldView => ({ field_id, label, kind, hidden: false, value, mask: null })
const hidden = (field_id: string, label: string, kind = "hidden"): FieldView => ({ field_id, label, kind, hidden: true, value: null, mask: "••••••••" })
const item = (kind: string, fields: FieldView[], over: Partial<ItemDetail> = {}): ItemDetail => ({ id: "i1", kind, state: "live", version: 1, edited_at: 0, crown_jewel: false, fields: [plain("title", "title", "Tern Airlines"), ...fields], ...over })
const summary = (over: Partial<ItemSummary>): ItemSummary => ({ id: "i1", kind: "login", title: "Tern Airlines", subtitle: "", state: "live", codes: 0, conflict_copy: false, copy: null, edited_at: 0, ...over }) as ItemSummary

/** This copy (i1) and the other (i2), as the list has them (D58). */
const CONFLICT: ItemSummary[] = [
  summary({ id: "i1", title: "Tern Airlines (conflict copy from Sam's phone)", conflict_copy: true }),
  summary({ id: "i2", title: "Tern Airlines", edited_at: Date.UTC(2026, 10, 2, 12) / 1000 }),
]

function mount(client: Client, rows: ItemSummary[] = []) {
  return render(
    <ClientProvider value={client}>
      <SettingsProvider>
        <TemplatesProvider>
          <GateProvider>
            <WaitingHost place={{ sharing: false, item: "i1" }}>
              {/* The shell's slot at the top of the detail (components/master-detail.tsx). */}
              <NeedsYouSlot />
              <DetailPane id="i1" rows={rows} onEdit={() => {}} onTrashed={() => {}} onChanged={() => {}} onNotice={() => {}} />
            </WaitingHost>
          </GateProvider>
        </TemplatesProvider>
      </SettingsProvider>
    </ClientProvider>,
  )
}

const shows = (detail: ItemDetail, over: Partial<Client> = {}) => fakeClient({ templates: () => Promise.resolve(TEMPLATES), show: () => Promise.resolve(detail), ...over })
const blocks = () => document.querySelectorAll("[data-needs-you-row]")

describe("a conflict on the open item (brief I8, X3 row 5)", () => {
  it("is one needs-you on its item, and each answer is a Question that starts on Cancel", async () => {
    const resolveConflict = vi.fn<Client["resolveConflict"]>(() => Promise.resolve())
    mount(shows(item("login", [plain("user", "username", "sam")]), { resolveConflict }), CONFLICT)
    await screen.findByRole("heading", { name: "Tern Airlines" })
    await waitFor(() => expect(blocks()).toHaveLength(1))
    const block = blocks()[0] as HTMLElement
    expect(block.getAttribute("data-needs-you-row")).toBe("5")
    expect(block).toHaveTextContent("Conflict // 2 copies")
    expect(block).toHaveTextContent("Two devices changed this item at the same time.")
    expect(block).toHaveTextContent(/The other copy: edited 2026\.11\.0[23]/)
    // Not twice: the notice slot draws no second yellow shape and no warning Banner for it.
    expect(screen.queryByRole("region", { name: "Waiting on you" })).toBeNull()
    expect(screen.getAllByRole("button", { name: "Keep this copy" })).toHaveLength(1)

    const user = userEvent.setup()
    await user.click(within(block).getByRole("button", { name: "Keep other copy" }))
    const ask = await screen.findByRole("dialog")
    expect(ask).toHaveTextContent("Keep the other copy?")
    await waitFor(() => expect(within(ask).getByRole("button", { name: "Cancel" })).toHaveFocus())
    await user.click(within(ask).getByRole("button", { name: "Cancel" }))
    expect(resolveConflict).not.toHaveBeenCalled()

    await user.click(within(blocks()[0] as HTMLElement).getByRole("button", { name: "Keep this copy" }))
    const again = await screen.findByRole("dialog")
    expect(again).toHaveTextContent("Keep this copy?")
    await user.click(within(again).getByRole("button", { name: "Keep this copy" }))
    await waitFor(() => expect(resolveConflict).toHaveBeenCalledWith("i1", "i2"))
  })

  it("is a WAITING ON YOU row when a request outranks it, never a second block", async () => {
    const drop = addNeedsYou({ id: "req", priority: 1, title: "Sign-in request", line: "A browser asks to sign in.", action: { label: "Review", onClick: () => {} }, shows: "every" })
    try {
      mount(shows(item("login", [plain("user", "username", "sam")])), CONFLICT)
      await screen.findByRole("heading", { name: "Tern Airlines" })
      const waiting = await screen.findByRole("region", { name: "Waiting on you" })
      expect(blocks()).toHaveLength(1)
      expect((blocks()[0] as HTMLElement).getAttribute("data-needs-you-row")).toBe("1")
      expect(blocks()[0]).toHaveTextContent("// +1")
      expect(within(waiting).getByText("Conflict // 2 copies")).toBeInTheDocument()
      expect(within(waiting).getByRole("button", { name: "Keep this copy" })).toBeInTheDocument()
      expect(within(waiting).getByRole("button", { name: "Keep other copy" })).toBeInTheDocument()
    } finally {
      drop()
    }
  })

  it("is nothing on a copy in Recently deleted", async () => {
    mount(shows(item("login", [plain("user", "username", "sam")], { state: "trashed" })), [{ ...CONFLICT[0]!, state: "trashed" }, CONFLICT[1]!])
    expect(await screen.findByText(/In Recently deleted\. Deleted for good after \d+ days\./)).toBeInTheDocument()
    expect(blocks()).toHaveLength(0)
  })
})

describe("Recently deleted (brief I8)", () => {
  it("a purge asks first, critical, and nothing is destroyed on Cancel", async () => {
    const purge = vi.fn<Client["purge"]>(() => Promise.resolve())
    mount(shows(item("login", [plain("user", "username", "sam")], { state: "trashed" }), { purge }))
    await screen.findByRole("heading", { name: "Tern Airlines" })
    const user = userEvent.setup()
    await user.click(screen.getByRole("button", { name: "More for this item" }))
    const rows = await screen.findAllByRole("menuitem")
    expect(rows.at(-1)).toHaveTextContent("Delete for good")
    await user.click(rows.at(-1)!)
    const ask = await screen.findByRole("dialog")
    expect(ask).toHaveTextContent('Delete "Tern Airlines" for good?')
    await waitFor(() => expect(within(ask).getByRole("button", { name: "Cancel" })).toHaveFocus())
    await user.click(within(ask).getByRole("button", { name: "Cancel" }))
    expect(purge).not.toHaveBeenCalled()
  })
})

describe("Show large type on every hidden field (brief I9, D242)", () => {
  const FIELDS: FieldView[] = [hidden("pw", "password", "password"), hidden("pin", "PIN"), hidden("custom.0", "Door code")]

  // One field a test: each opens the ⋯, then the viewer, then closes it.
  it.each(["password", "PIN", "Door code"])("is the first row of the ⋯ of %s, reached by a tap", async (name) => {
    const reveal = vi.fn<Client["reveal"]>(() => Promise.resolve(new TextEncoder().encode("k7Hq3mWx")))
    mount(shows(item("login", FIELDS), { reveal }))
    await screen.findByRole("heading", { name: "Tern Airlines" })
    expect(await screen.findAllByRole("button", { name: /^More actions for / })).toHaveLength(FIELDS.length)
    const user = userEvent.setup({ pointerEventsCheck: 0 })
    await user.click(screen.getByRole("button", { name: new RegExp(`^More actions for ${name}`, "i") }))
    const rows = await screen.findAllByRole("menuitem")
    expect(rows[0]).toHaveTextContent("Show large type")
    await user.click(rows[0]!)
    expect(await screen.findByRole("dialog")).toHaveTextContent("Large type")
    await waitFor(() => expect(reveal).toHaveBeenCalledTimes(1))
  })

  it.each(["password", "PIN", "Door code"])("is reached on %s by the keyboard alone: focus the ⋯, Enter, Enter, then Escape hides it", async (name) => {
    const reveal = vi.fn<Client["reveal"]>(() => Promise.resolve(new TextEncoder().encode("k7Hq3mWx")))
    mount(shows(item("login", FIELDS), { reveal }))
    await screen.findByRole("heading", { name: "Tern Airlines" })
    const user = userEvent.setup()
    const trigger = await screen.findByRole("button", { name: new RegExp(`^More actions for ${name}`, "i") })
    trigger.focus()
    expect(trigger).toHaveFocus()
    await user.keyboard("{Enter}")
    const first = (await screen.findAllByRole("menuitem"))[0]!
    expect(first).toHaveTextContent("Show large type")
    await waitFor(() => expect(first).toHaveFocus())
    await user.keyboard("{Enter}")
    const viewer = await screen.findByRole("dialog")
    await waitFor(() => expect(reveal).toHaveBeenCalledTimes(1))
    await user.keyboard("{Escape}")
    await waitFor(() => expect(viewer).not.toBeInTheDocument())
  })

  it("is a visible action on a Wi-Fi password, next to the guest card", async () => {
    mount(shows(item("wi-fi", [plain("wifi.ssid", "Network name", "home-net"), hidden("wifi.passphrase", "password")])))
    await screen.findByRole("heading", { name: "Tern Airlines" })
    expect(await screen.findByRole("button", { name: "Show large type" })).toBeVisible()
    expect(screen.getByRole("button", { name: "Guest card" })).toBeInTheDocument()
  })
})
