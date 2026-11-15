import { render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { GateProvider } from "@/app/gate"
import { SettingsProvider } from "@/app/settings"
import { TemplatesProvider } from "@/app/templates"
import { ClientProvider, type Client } from "@/core/client"
import type { Access, FieldView, ItemDetail } from "@/core/types"
import { fakeClient } from "@/test/fake-client"
import { fakeSharing } from "@/test/fake-sharing"
import { TEMPLATES } from "@/test/templates-fixture"

import { DetailPane } from "./detail-pane"

import { SharingClientProvider, type SharingClient } from "@/features/sharing/sharing-client"

const plain = (field_id: string, label: string, value: string, kind = "text"): FieldView => ({ field_id, label, kind, hidden: false, value, mask: null })
const hidden = (field_id: string, label: string, kind = "hidden"): FieldView => ({ field_id, label, kind, hidden: true, value: null, mask: "••••••••" })

function login(access?: Access): ItemDetail {
  return {
    id: "i1",
    kind: "login",
    state: "live",
    version: 1,
    edited_at: 0,
    crown_jewel: false,
    fields: [plain("title", "title", "Router"), plain("user", "username", "admin"), hidden("pw", "password", "password")],
    access,
  }
}

function mount(detail: ItemDetail, sharing: SharingClient = fakeSharing({}), extra: Partial<Client> = {}) {
  const client = fakeClient({ templates: () => Promise.resolve(TEMPLATES), show: () => Promise.resolve(detail), ...extra })
  return render(
    <ClientProvider value={client}>
      <SettingsProvider>
        <TemplatesProvider>
          <GateProvider>
            <SharingClientProvider value={sharing}>
              <DetailPane id="i1" rows={[]} onEdit={() => {}} onTrashed={() => {}} onChanged={() => {}} onNotice={() => {}} />
            </SharingClientProvider>
          </GateProvider>
        </TemplatesProvider>
      </SettingsProvider>
    </ClientProvider>,
  )
}

describe("an item by the person's right in it", () => {
  it("fill only: no reveal, no copy, no edit, no trash, no share; the right is said", async () => {
    mount(login("fill-only"))
    expect(await screen.findByText("Shared with you")).toBeInTheDocument()
    expect(screen.getByText(/Fill only: Lockdown fills it in the browser/)).toBeInTheDocument()
    expect(screen.getByText("Fills in the browser only; never shown or copied.")).toBeInTheDocument()
    expect(screen.queryByRole("button", { name: /Show/ })).not.toBeInTheDocument()
    expect(screen.queryByRole("button", { name: /Copy password/i })).not.toBeInTheDocument()
    for (const name of ["Edit", "Trash", "Share"]) expect(screen.queryByRole("button", { name })).not.toBeInTheDocument()
    // The plain username is still drawn, as the fill uses it.
    expect(screen.getByText("admin")).toBeInTheDocument()
  })

  it("view: the password can be shown, but nothing can be changed or passed on", async () => {
    mount(login("view"))
    expect(await screen.findByText("Shared with you")).toBeInTheDocument()
    expect(screen.queryByText("Fills in the browser only; never shown or copied.")).not.toBeInTheDocument()
    for (const name of ["Edit", "Trash", "Share"]) expect(screen.queryByRole("button", { name })).not.toBeInTheDocument()
  })

  it("edit (a member vault): Edit and Trash, but sharing stays with the owner", async () => {
    mount(login("edit"))
    expect(await screen.findByRole("button", { name: "Edit" })).toBeInTheDocument()
    expect(screen.getByRole("button", { name: "Trash" })).toBeInTheDocument()
    expect(screen.queryByRole("button", { name: "Share" })).not.toBeInTheDocument()
  })

  it("the person's own item offers Share from the detail: with someone, or as a share link", async () => {
    mount(login())
    const user = userEvent.setup({ pointerEventsCheck: 0 })
    await user.click(await screen.findByRole("button", { name: "Share" }))
    expect(await screen.findByRole("menuitem", { name: "With someone…" })).toBeInTheDocument()
    expect(screen.getByRole("menuitem", { name: "As a share link…" })).toBeInTheDocument()
  })

  it("a crown jewel offers no sharing at all", async () => {
    mount({ ...login(), crown_jewel: true })
    expect(await screen.findByRole("button", { name: "Trash" })).toBeInTheDocument()
    expect(screen.queryByRole("button", { name: "Share" })).not.toBeInTheDocument()
  })
})
