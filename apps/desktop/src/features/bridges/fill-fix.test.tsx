import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import type { ReactNode } from "react"

import { GateProvider } from "@/app/gate"
import { DEFAULT_SETTINGS, SettingsProvider } from "@/app/settings"
import { ClientProvider, type Client } from "@/core/client"
import { fakeClient } from "@/test/fake-client"
import { BridgesProvider, type BridgesClient } from "./bridges-client"
import { FillFixPanel } from "./fill-fix"
import { fakeBridges } from "./fake-bridges"

function mount(ui: ReactNode, bridges: BridgesClient | null, over: Partial<Client> = {}) {
  const client = fakeClient({
    getSettings: () => Promise.resolve(DEFAULT_SETTINGS),
    approvalRoute: () => Promise.resolve({ route: "window", method: "this window" }),
    ...over,
  })
  return render(
    <ClientProvider value={client}>
      <SettingsProvider>
        <GateProvider>
          <BridgesProvider value={bridges}>{ui}</BridgesProvider>
        </GateProvider>
      </SettingsProvider>
    </ClientProvider>,
  )
}

describe("the bridges views", () => {

  it("the fill fix says nothing where the host has no bridges client (the web vault)", () => {
    const { container } = mount(<FillFixPanel itemId="i1" crownJewel={false} canEdit onNotice={vi.fn()} />, null)
    expect(container.textContent).toBe("")
  })

  it("FILLING ON THIS SITE says what is set, and its Form fixes a field by name and turns filling off (R9)", async () => {
    const siteFixSet = vi.fn((_id: string, fix: { never: boolean; fields: unknown[] }) => Promise.resolve(fix as never))
    const bridges = fakeBridges({ siteFix: () => Promise.resolve({ never: false, fields: [] }), siteFixSet })
    mount(<FillFixPanel itemId="i1" crownJewel={false} canEdit host="store.example.com" onNotice={vi.fn()} />, bridges)
    const row = await screen.findByRole("group", { name: "Filling on this site" })
    expect(row).toHaveTextContent("Lockdown decides")
    const user = userEvent.setup()
    await user.click(within(row).getByRole("button", { name: "Change" }))
    const form = await screen.findByRole("dialog", { name: "Filling on store.example.com" })
    await user.type(within(form).getByLabelText("Field name on the page"), "login_id")
    await user.click(within(form).getByRole("button", { name: "Add fix" }))
    expect(siteFixSet).toHaveBeenCalledWith("i1", { never: false, fields: [{ match: { name: "login_id" }, role: "username" }] })
    expect(await within(form).findByText('The field "login_id" is the user name')).toBeInTheDocument()
    expect(row).toHaveTextContent("1 field fixed")
    await user.click(within(form).getByRole("switch", { name: "Never fill on this site" }))
    expect(siteFixSet).toHaveBeenLastCalledWith("i1", expect.objectContaining({ never: true }))
    await waitFor(() => expect(row).toHaveTextContent("Never fills here"))
    await user.click(within(form).getByRole("button", { name: "Remove the fix for login_id" }))
    expect(siteFixSet).toHaveBeenLastCalledWith("i1", { never: true, fields: [] })
  })

  it("a crown jewel has no fill fix (never filled); without the right to edit there is no CHANGE", async () => {
    const siteFix = vi.fn(() => Promise.resolve({ never: true, fields: [] }))
    const { container, unmount } = mount(<FillFixPanel itemId="i1" crownJewel canEdit onNotice={vi.fn()} />, fakeBridges({ siteFix }))
    expect(container.textContent).toBe("")
    expect(siteFix).not.toHaveBeenCalled()
    unmount()
    mount(<FillFixPanel itemId="i1" crownJewel={false} canEdit={false} onNotice={vi.fn()} />, fakeBridges({ siteFix }))
    const row = await screen.findByRole("group", { name: "Filling on this site" })
    expect(row).toHaveTextContent("Never fills here")
    expect(within(row).queryByRole("button")).not.toBeInTheDocument()
  })
})
