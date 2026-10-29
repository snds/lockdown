import { act, render, screen, within } from "@testing-library/react"

import { GateProvider } from "@/app/gate"
import { LockedProvider } from "@/app/locked"
import { SettingsProvider } from "@/app/settings"
import { TemplatesProvider } from "@/app/templates"
import { ClientProvider, type Client } from "@/core/client"
import type { FieldView, Finding, ItemDetail, SecurityReport } from "@/core/types"
import { DepthClientProvider, type ExposureView } from "@/features/depth/depth-client"
import { fakeClient } from "@/test/fake-client"
import { fakeDepth } from "@/test/fake-depth"
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

describe("one Security box (design review R2)", () => {
  it("draws the item's Security box once, however often the lock poll redraws the detail", async () => {
    const finding = {
      id: "f1",
      kind: "weak",
      severity: "important",
      title: "Weak password",
      detail: "It could be guessed.",
      key: "weak:pw",
      item_id: "i1",
      item_title: "Thing",
      item_kind: "login",
      field_id: "login.password",
      site: null,
      related: [],
      setup_url: null,
      change_password: true,
      playbook: "weak",
      alert: null,
    } as unknown as Finding
    const report = { findings: [finding], dismissed: [], counts: { urgent: 0, important: 1, suggested: 0 } } as unknown as SecurityReport
    const client = fakeClient({
      templates: () => Promise.resolve(TEMPLATES),
      show: () => Promise.resolve(item("login", [plain("login.username", "username", "sam")])),
      securityReport: () => Promise.resolve(report),
      securityPlaybooks: () => Promise.resolve([]),
    })
    const depth = fakeDepth({
      exposure: () => Promise.resolve({ id: "i1", tier: "fill", source: "default", default_tier: "fill", reason: null, choices: ["fill", "ask", "app-only"] } satisfies ExposureView),
    })
    const tree = (locked: boolean) => (
      <ClientProvider value={client}>
        <DepthClientProvider value={depth}>
          <LockedProvider value={locked}>
            <SettingsProvider>
              <TemplatesProvider>
                <GateProvider>
                  <DetailPane id="i1" rows={[]} onEdit={() => {}} onTrashed={() => {}} onChanged={() => {}} onNotice={() => {}} />
                </GateProvider>
              </TemplatesProvider>
            </SettingsProvider>
          </LockedProvider>
        </DepthClientProvider>
      </ClientProvider>
    )
    const { rerender } = render(tree(false))
    await screen.findByRole("region", { name: "Security" })
    await screen.findByRole("region", { name: "Protection" })
    for (let poll = 0; poll < 3; poll += 1) {
      rerender(tree(false))
      await act(() => Promise.resolve())
    }
    expect(screen.getAllByRole("region", { name: "Security" })).toHaveLength(1)
    expect(screen.getAllByRole("region", { name: "Protection" })).toHaveLength(1)
  })
})
