import { render, screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import type { ReactNode } from "react"

import { GateProvider } from "@/app/gate"
import { DEFAULT_SETTINGS, SettingsProvider } from "@/app/settings"
import { ClientProvider, type Client } from "@/core/client"
import { fakeClient } from "@/test/fake-client"
import { BridgesProvider, type BridgesClient, type IncidentView } from "./bridges-client"
import { IncidentsSection } from "./incidents"
import { fakeBridges } from "./fake-bridges"

const NOW = Math.floor(Date.now() / 1000)

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

  it("the incidents says nothing where the host has no bridges client (the web vault)", () => {
    const { container } = mount(<IncidentsSection onOpenItem={vi.fn()} onAction={vi.fn()} onNotice={vi.fn()} />, null)
    expect(container.textContent).toBe("")
  })

  it("a guided flow asks what it is about, then ticks steps and items off", async () => {
    const steps = (done: boolean): IncidentView["steps"] => [
      { id: "change", title: "Change the password on that site", detail: "Use Change password.", action: "change_passwords", done, items: [{ item_id: "i1", title: "Shop", host: "shop.example", done: false }] },
      { id: "verify", title: "Check it worked", detail: "Choose Check now.", action: "check", done: false, items: [] },
    ]
    const view = (picked: string | null, done = false): IncidentView => ({
      id: "site-breach",
      title: "A site I use was breached",
      when: "The site told you.",
      scope: "item",
      picked,
      choices: [{ value: "i1", label: "Shop (shop.example)" }],
      started_at: picked ? NOW : null,
      steps: steps(done),
      finished: false,
    })
    const incident = vi.fn((_id: string, pick?: string | null) => Promise.resolve(view(pick ?? null)))
    const incidentMark = vi.fn((_id: string, _s: string, _i: string | null, done: boolean) => Promise.resolve(view("i1", done)))
    const bridges = fakeBridges({
      incidents: () => Promise.resolve([{ id: "site-breach", title: "A site I use was breached", when: "The site told you.", scope: "item", started_at: null, steps_done: 0, steps: 2 }]),
      incident,
      incidentMark,
    })
    const onAction = vi.fn()
    mount(<IncidentsSection onOpenItem={vi.fn()} onAction={onAction} onNotice={vi.fn()} />, bridges)
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "Start" }))
    const dialog = await screen.findByRole("dialog")
    const first = within(dialog).getByRole("checkbox", { name: "Done: Change the password on that site" })
    expect(first.getAttribute("aria-disabled") === "true" || first.hasAttribute("disabled") || first.hasAttribute("data-disabled")).toBe(true)
    await user.selectOptions(within(dialog).getByLabelText("Which site"), "i1")
    await user.click(within(dialog).getByRole("button", { name: "Start" }))
    expect(incident).toHaveBeenLastCalledWith("site-breach", "i1")
    await user.click(await within(dialog).findByRole("checkbox", { name: "Done: Change the password on that site" }))
    expect(incidentMark).toHaveBeenCalledWith("site-breach", "change", null, true)
    await user.click(within(dialog).getByRole("button", { name: "Check now" }))
    expect(onAction).toHaveBeenCalledWith("check")
  })
})
