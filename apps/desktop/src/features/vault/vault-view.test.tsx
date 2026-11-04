import { render, screen, within } from "@testing-library/react"

import { GateProvider } from "@/app/gate"
import { SettingsProvider } from "@/app/settings"
import { TemplatesProvider } from "@/app/templates"
import { ClientProvider } from "@/core/client"
import type { ItemSummary, VaultView as VaultInfo } from "@/core/types"
import { VaultsProvider } from "@/features/organization/vaults-context"
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

const LINE = "Some recent changes from bob@example.com couldn't be verified, so Lockdown left them out. You don't need to do anything."

const vault = (id: string, unverified: string | null): VaultInfo => ({ id, name: id === "v1" ? "Family" : "Personal", primary: id !== "v1", items: 1, trash_days: 30, folders: [], unverified })

function mountIn(scope: { vault: string } | undefined, vaults: VaultInfo[]) {
  return render(
    <ClientProvider value={fakeClient({ templates: () => Promise.resolve(TEMPLATES) })}>
      <SettingsProvider>
        <TemplatesProvider>
          <GateProvider>
            <VaultsProvider value={{ vaults, reload: () => {} }}>
              <VaultView view="all" rows={[row("n1", "note", "Door code")]} error={null} onChanged={() => {}} onNotice={() => {}} selected={null} onSelect={() => {}} onPalette={() => {}} scope={scope} />
            </VaultsProvider>
          </GateProvider>
        </TemplatesProvider>
      </SettingsProvider>
    </ClientProvider>,
  )
}

describe("a shared vault with changes held back (docs/plan/49-shared-vault-quiet.md)", () => {
  it("says so in one calm line on that vault, with nothing to click", () => {
    mountIn({ vault: "v1" }, [vault("v0", null), vault("v1", LINE)])
    const note = screen.getByRole("note")
    expect(note).toHaveTextContent(LINE)
    expect(within(note).queryByRole("button")).not.toBeInTheDocument()
    expect(screen.queryByRole("alert")).not.toBeInTheDocument()
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument()
  })

  it("shows nothing on a vault with nothing held back, or outside a vault", () => {
    mountIn({ vault: "v0" }, [vault("v0", null), vault("v1", LINE)])
    expect(screen.queryByText(LINE)).not.toBeInTheDocument()
  })

  it("shows nothing in All items", () => {
    mountIn(undefined, [vault("v1", LINE)])
    expect(screen.queryByText(LINE)).not.toBeInTheDocument()
  })
})
