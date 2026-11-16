import { render, screen, within } from "@testing-library/react"
import { useState } from "react"
import userEvent from "@testing-library/user-event"
import type { ReactNode } from "react"

import { GateProvider } from "@/app/gate"
import { DEFAULT_SETTINGS, SettingsProvider } from "@/app/settings"
import { ClientProvider, type Client } from "@/core/client"
import { fakeClient } from "@/test/fake-client"
import { BridgesProvider, type BridgesClient, type FederationMap } from "./bridges-client"
import { AccountDetail, AccountsList, useFederation } from "./federation"
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

/** Health's Accounts you sign in with, as Health lays it out: the list, and the account chosen. */
function Accounts({ onOpenItem }: { onOpenItem: (id: string) => void }) {
  const f = useFederation(() => {}, () => {})
  const [chosen, setChosen] = useState<string | null>(null)
  const provider = f.map?.providers.find((p) => p.id === chosen) ?? null
  if (!f.available) return null
  return (
    <>
      <AccountsList
        map={f.map}
        failed={f.failed}
        busy={f.busy}
        chosen={chosen}
        onChoose={setChosen}
        onRecord={(p, item) => void f.run((b) => b.federationAccept(p.id, item), "Noted")}
        onForget={(p) => void f.run((b) => b.federationDismiss(p.id), "Forgotten")}
        onTryAgain={f.reload}
      />
      {provider && <AccountDetail provider={provider} busy={f.busy} onPick={() => {}} onOpenItem={onOpenItem} />}
    </>
  )
}

describe("Accounts you sign in with", () => {
  it("says nothing where the host has no bridges client (the web vault)", () => {
    const { container } = mount(<Accounts onOpenItem={vi.fn()} />, null)
    expect(container.textContent).toBe("")
  })

  it("lists each provider with what falls with it, opens one with its warnings and sites, and asks about what the browser saw", async () => {
    const map: FederationMap = {
      providers: [
        {
          id: "google",
          name: "Google",
          account: { item_id: "g", title: "Google", username: "sean@gmail.com", host: "accounts.google.com", has_password: true, second_step: false, crown_jewel: false },
          candidates: [],
          sites: [{ item_id: "s1", title: "Shop", username: "", host: "shop.example", has_password: false, second_step: false, crown_jewel: false }],
          warnings: ["Your Google account has no two-step sign-in saved in Lockdown."],
        },
      ],
      pending: [{ id: 7, provider: "apple", provider_name: "Apple", host: "news.example", logins: [], confirmed: false }],
    }
    const federationAccept = vi.fn((_p: number, _i: string | null) => Promise.resolve("new"))
    const bridges = fakeBridges({ federationMap: () => Promise.resolve(map), federationAccept })
    const onOpenItem = vi.fn()
    mount(<Accounts onOpenItem={onOpenItem} />, bridges)
    const accounts = await screen.findByRole("list", { name: "Accounts" })
    expect(within(accounts).getByText("1 site")).toBeInTheDocument()
    expect(within(accounts).getByText("No second step")).toBeInTheDocument()
    const user = userEvent.setup()
    await user.click(within(accounts).getByRole("button", { name: "Google" }))
    const detail = await screen.findByRole("article", { name: "Google" })
    expect(within(detail).getByText(/no two-step sign-in/)).toBeInTheDocument()
    expect(within(detail).getByRole("group", { name: "Your Google login" })).toHaveTextContent("sean@gmail.com")
    const sites = within(detail).getByRole("list", { name: "Sites you sign in to with Google" })
    await user.click(within(sites).getByRole("button", { name: "Shop" }))
    expect(onOpenItem).toHaveBeenCalledWith("s1")
    expect(screen.getByText("Did you sign in to news.example with Apple?")).toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: "Yes, save a login" }))
    expect(federationAccept).toHaveBeenCalledWith(7, null)
  })
})
