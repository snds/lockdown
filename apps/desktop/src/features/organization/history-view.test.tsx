import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { GateProvider } from "@/app/gate"
import { ClientProvider, type Client } from "@/core/client"
import type { ItemDetail, VersionView } from "@/core/types"
import { fakeClient } from "@/test/fake-client"

import { HistoryView } from "./history-view"

const at = (d: number) => Date.UTC(2026, 10, d, 12) / 1000
const VERSIONS: VersionView[] = [
  { n: 3, edited_at: at(8), edited_by: "MacBook", changed: ["Password"], readable: true, current: true },
  { n: 2, edited_at: at(4), edited_by: "Pixel", changed: ["Password", "Website"], readable: true, current: false },
  { n: 1, edited_at: at(1), edited_by: "MacBook", changed: [], readable: false, current: false },
]
const OLD: ItemDetail = {
  id: "i1",
  kind: "login",
  state: "live",
  version: 2,
  edited_at: at(4),
  crown_jewel: false,
  fields: [
    { field_id: "title", label: "title", kind: "text", hidden: false, value: "Steam", mask: null },
    { field_id: "user", label: "Username", kind: "text", hidden: false, value: "sam", mask: null },
    { field_id: "pw", label: "Password", kind: "password", hidden: true, value: null, mask: "••••••••" },
  ],
}

function mount(over: Partial<Client>, editable = true) {
  const onRestored = vi.fn()
  const client = fakeClient({ approvalRoute: () => Promise.resolve({ route: "window", method: "window" }), history: () => Promise.resolve(VERSIONS), showVersion: () => Promise.resolve(OLD), ...over })
  render(
    <ClientProvider value={client}>
      <GateProvider>
        <HistoryView itemId="i1" title="Steam" editable={editable} onClose={() => {}} onRestored={onRestored} />
      </GateProvider>
    </ClientProvider>,
  )
  return { onRestored }
}

describe("an item's history (brief I6)", () => {
  it("lists every version newest first: CURRENT, when and where, what changed; a version without keys offers nothing", async () => {
    mount({})
    const dialog = await screen.findByRole("dialog", { name: "3 versions" })
    expect(dialog).toHaveTextContent("History // Steam")
    const rows = within(within(dialog).getByRole("list", { name: "Versions" })).getAllByRole("listitem")
    expect(rows.map((r) => within(r).getByText(/^Version \d$/).textContent)).toEqual(["Version 3", "Version 2", "Version 1"])
    expect(rows[0]).toHaveTextContent("Current")
    expect(rows[1]).toHaveTextContent(/2026\.11\.04 \d\d:\d\d \/\/ Pixel/)
    expect(rows[1]).toHaveTextContent("Changed: Password, Website")
    // The current version is not restored onto itself; a version deleted for good has no actions.
    expect(within(rows[0]!).queryByRole("button", { name: /^Restore/ })).not.toBeInTheDocument()
    expect(rows[2]).toHaveTextContent("Deleted for good")
    expect(within(rows[2]!).queryByRole("button")).not.toBeInTheDocument()
  })

  it("SHOW opens a version under its row with every hidden value still hidden", async () => {
    const reveal = vi.fn<Client["reveal"]>()
    mount({ reveal })
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "Show version 2" }))
    const shown = await screen.findByRole("region", { name: "Version 2" })
    expect(within(shown).getByRole("group", { name: "Username" })).toHaveTextContent("sam")
    expect(shown).toHaveTextContent("Hidden here. Restore this version to use it.")
    expect(within(shown).queryByRole("button", { name: /Show|Reveal|Copy/ })).not.toBeInTheDocument()
    expect(reveal).not.toHaveBeenCalled()
    await user.click(screen.getByRole("button", { name: "Hide version 2" }))
    expect(screen.queryByRole("region", { name: "Version 2" })).not.toBeInTheDocument()
  })

  it("a restore asks first, with Cancel focused, then saves the version as the newest and says so", async () => {
    const restoreVersion = vi.fn<Client["restoreVersion"]>(() => Promise.resolve(4))
    const { onRestored } = mount({ restoreVersion })
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "Restore version 2" }))
    const ask = await screen.findByRole("dialog", { name: "Restore version 2?" })
    await waitFor(() => expect(within(ask).getByRole("button", { name: "Cancel" })).toHaveFocus())
    expect(restoreVersion).not.toHaveBeenCalled()
    await user.click(within(ask).getByRole("button", { name: "Restore" }))
    await waitFor(() => expect(restoreVersion).toHaveBeenCalledWith("i1", 2, {}))
    expect(onRestored).toHaveBeenCalledWith("Version 2 of Steam is back as version 4")
  })

  it("without the right to change the item, versions can be read but not restored", async () => {
    mount({}, false)
    await screen.findByRole("button", { name: "Show version 2" })
    expect(screen.queryByRole("button", { name: /^Restore/ })).not.toBeInTheDocument()
  })
})
