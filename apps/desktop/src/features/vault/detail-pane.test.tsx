import { render, screen, within } from "@testing-library/react"

import { GateProvider } from "@/app/gate"
import { SettingsProvider } from "@/app/settings"
import { TemplatesProvider } from "@/app/templates"
import { ClientProvider, type Client } from "@/core/client"
import type { FieldView, ItemDetail } from "@/core/types"
import { fakeClient } from "@/test/fake-client"
import { TEMPLATES } from "@/test/templates-fixture"

import { DetailPane } from "./detail-pane"

const plain = (field_id: string, label: string, value: string, kind = "text"): FieldView => ({ field_id, label, kind, hidden: false, value, mask: null })
const hidden = (field_id: string, label: string, kind = "hidden"): FieldView => ({ field_id, label, kind, hidden: true, value: null, mask: "••••••••" })
const item = (kind: string, fields: FieldView[]): ItemDetail => ({ id: "i1", kind, state: "live", version: 1, edited_at: 0, crown_jewel: false, fields: [plain("title", "title", "Thing"), ...fields] })

function mount(client: Client) {
  return render(
    <ClientProvider value={client}>
      <SettingsProvider>
        <TemplatesProvider>
          <GateProvider>
            <DetailPane id="i1" rows={[]} onEdit={() => {}} onTrashed={() => {}} onChanged={() => {}} onNotice={() => {}} />
          </GateProvider>
        </TemplatesProvider>
      </SettingsProvider>
    </ClientProvider>,
  )
}

const shows = (detail: ItemDetail) => fakeClient({ templates: () => Promise.resolve(TEMPLATES), show: () => Promise.resolve(detail) })

describe("detail layouts from the templates (D110)", () => {
  it("a Wi-Fi network: its section, the template's labels, yes or no, and large type on the password", async () => {
    mount(shows(item("wi-fi", [plain("wifi.ssid", "Network name", "home-net"), hidden("wifi.passphrase", "password"), plain("wifi.hidden", "hidden", "true", "boolean")])))
    const network = await screen.findByRole("region", { name: "Network" })
    expect(within(network).getByText("home-net")).toBeInTheDocument()
    expect(within(network).getByText("Hidden network")).toBeInTheDocument()
    expect(within(network).getByText("Yes")).toBeInTheDocument()
    expect(within(network).getByRole("button", { name: "Show large type" })).toBeInTheDocument()
    expect(screen.getByRole("button", { name: "Edit" })).toBeInTheDocument()
  })

  it("a card: the number and security code masked, the rest under its heading, the fill rule said", async () => {
    mount(shows(item("card", [hidden("card.number", "number"), plain("card.holder", "holder", "A. Person"), hidden("card.cvc", "security code"), plain("custom.0", "Bank phone", "+1 555 0100", "phone")])))
    const card = await screen.findByRole("region", { name: "Card" })
    expect(within(card).getAllByText("Card number").length).toBeGreaterThan(0)
    expect(within(card).getAllByText("Security code").length).toBeGreaterThan(0)
    expect(within(card).getByText("A. Person")).toBeInTheDocument()
    expect(screen.getByRole("region", { name: "Your fields" })).toHaveTextContent("Bank phone")
    expect(screen.getByText(/always takes a second press/)).toBeInTheDocument()
  })

  it("a passkey is view only: no Edit, and its key is never drawn", async () => {
    mount(shows(item("passkey", [plain("passkey.0.rp_id", "relying party", "example.com"), hidden("passkey.0.key", "private key", "passkey")])))
    expect(await screen.findByText(/kept by the provider, never shown/)).toBeInTheDocument()
    expect(screen.queryByRole("button", { name: "Edit" })).not.toBeInTheDocument()
  })

  it("an item with nothing filled in says so; a failed open names the item and the core's words", async () => {
    mount(shows(item("note", [])))
    expect(await screen.findByText(/An empty note/)).toBeInTheDocument()
  })

  it("names the item when it can't be opened", async () => {
    mount(fakeClient({ templates: () => Promise.resolve(TEMPLATES), show: () => Promise.reject({ code: "refused", message: "the field 'pw' was tampered with" }) }))
    expect(await screen.findByRole("alert")).toHaveTextContent("Couldn't open this item (i1): the field 'pw' was tampered with")
  })
})
