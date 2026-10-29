import { render, screen, within } from "@testing-library/react"

import { GateProvider } from "@/app/gate"
import { SettingsProvider } from "@/app/settings"
import { TemplatesProvider } from "@/app/templates"
import { ClientProvider } from "@/core/client"
import type { ItemSummary } from "@/core/types"
import { repeatedLandmarks } from "@/test/a11y"
import { fakeClient } from "@/test/fake-client"
import { TEMPLATES } from "@/test/templates-fixture"

import { VaultView } from "./vault-view"

const row = (id: string, kind: string, title: string): ItemSummary => ({
  id,
  kind,
  state: "live",
  title,
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
  vault_id: "v1",
  folder: null,
  tags: [],
  favourite: false,
})

function mount(rows: ItemSummary[] | null) {
  return render(
    <ClientProvider value={fakeClient({ templates: () => Promise.resolve(TEMPLATES) })}>
      <SettingsProvider>
        <TemplatesProvider>
          <GateProvider>
            <VaultView view="all" rows={rows} error={null} onChanged={() => {}} onNotice={() => {}} selected={null} onSelect={() => {}} onPalette={() => {}} />
          </GateProvider>
        </TemplatesProvider>
      </SettingsProvider>
    </ClientProvider>,
  )
}

describe("the vault list's structure (design review R18)", () => {
  it("while loading, says so in a status with its words inside, not a label on a plain div", () => {
    mount(null)
    const status = screen.getByRole("status")
    expect(status).toHaveTextContent("Loading items")
    expect(status).not.toHaveAttribute("aria-label")
  })

  it("names each kind's group with its count, so no group shares a name with a section of the open item", () => {
    const { container } = mount([row("n1", "note", "Passport checklist"), row("n2", "note", "Alarm codes"), row("l1", "login", "Mail")])
    const notes = screen.getByRole("region", { name: "Notes, 2 items" })
    expect(within(notes).getAllByRole("listitem")).toHaveLength(2)
    expect(screen.getByRole("region", { name: "Logins, 1 item" })).toBeInTheDocument()
    // The item's own "Notes" block is a region named "Notes"; the list's is not.
    expect(screen.queryByRole("region", { name: "Notes" })).not.toBeInTheDocument()
    expect(repeatedLandmarks(container)).toEqual([])
  })
})
