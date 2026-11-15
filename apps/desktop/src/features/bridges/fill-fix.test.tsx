import { render, screen } from "@testing-library/react"
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

  it("a login's fill fix is set and shown in words", async () => {
    const siteFixSet = vi.fn((_id: string, fix: { never: boolean; fields: unknown[] }) => Promise.resolve(fix as never))
    const bridges = fakeBridges({ siteFix: () => Promise.resolve({ never: false, fields: [] }), siteFixSet })
    mount(<FillFixPanel itemId="i1" crownJewel={false} canEdit onNotice={vi.fn()} />, bridges)
    const user = userEvent.setup()
    await user.type(await screen.findByLabelText("Field name on the page"), "login_id")
    await user.click(screen.getByRole("button", { name: "Add fix" }))
    expect(siteFixSet).toHaveBeenCalledWith("i1", { never: false, fields: [{ match: { name: "login_id" }, role: "username" }] })
    expect(await screen.findByText('The field "login_id" is the user name')).toBeInTheDocument()
    await user.click(screen.getByRole("switch", { name: "Never fill on this site" }))
    expect(siteFixSet).toHaveBeenLastCalledWith("i1", expect.objectContaining({ never: true }))
  })
})
