import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { vi } from "vitest"

import { GateProvider } from "@/app/gate"
import { SettingsProvider } from "@/app/settings"
import { ClientProvider, type Client } from "@/core/client"
import type { Access, FieldView, ItemDetail } from "@/core/types"
import { fakeClient } from "@/test/fake-client"
import { fakeSharing } from "@/test/fake-sharing"

import { AccessNote, FILL_ONLY_WORDS, ItemShare, StopSharing } from "./item-share"
import { SharingClientProvider, type SharingClient, type SharingOverview } from "./sharing-client"

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

const overview: SharingOverview = {
  online: true,
  fingerprint: "",
  fingerprint_qr: "",
  contacts: [
    { account_id: "acc-bob", login: "bob@example.com", persona: "p", fingerprint: "1", qr: "q", verified: true, devices: 1, log_index: 1 },
  ],
  vaults: [],
  households: [],
  sent: [],
  received: [],
  emergency: [],
  waiting_on_you: 0,
}

function mount(detail: ItemDetail, sharing: SharingClient = fakeSharing({}), extra: Partial<Client> = {}) {
  const client = fakeClient({ ...extra })
  return render(
    <ClientProvider value={client}>
      <SettingsProvider>
        <GateProvider>
          <SharingClientProvider value={sharing}>
            <ItemShare item={detail} title="Router" onNotice={() => {}} />
          </SharingClientProvider>
        </GateProvider>
      </SettingsProvider>
    </ClientProvider>,
  )
}

function Shared({ detail, sharing, open, onOpenChange, onNotice = () => {} }: { detail: ItemDetail; sharing: SharingClient; open?: boolean; onOpenChange?: (o: boolean) => void; onNotice?: (t: string) => void }) {
  return (
    <ClientProvider value={fakeClient({})}>
      <SettingsProvider>
        <GateProvider>
          <SharingClientProvider value={sharing}>
            <ItemShare item={detail} title="Router" onNotice={onNotice} open={open} onOpenChange={onOpenChange} />
          </SharingClientProvider>
        </GateProvider>
      </SettingsProvider>
    </ClientProvider>
  )
}

describe("sharing an item", () => {
  it("with someone: a Form with who, View or Fill only as rows, the caption, and SHARE through the gate; then a toast naming both", async () => {
    const shareItem = vi.fn(() => Promise.resolve({ share_id: "s1", item_id: "i1", login: "bob@example.com", right: "view" as const, created_at: 0 }))
    const notices: string[] = []
    render(<Shared detail={login()} sharing={fakeSharing({ overview: () => Promise.resolve(overview), shareItem })} onNotice={(t) => notices.push(t)} />)
    const user = userEvent.setup({ pointerEventsCheck: 0 })
    await user.click(await screen.findByRole("button", { name: "Share" }))
    await user.click(await screen.findByRole("menuitem", { name: "With someone…" }))
    const dialog = await screen.findByRole("dialog", { name: "Share Router" })
    await within(dialog).findByRole("option", { name: "bob@example.com" })
    expect(within(dialog).getByText("Only this item goes, locked to their key. You can stop at any time.")).toBeInTheDocument()
    await user.click(within(dialog).getByRole("radio", { name: /^Fill only/ }))
    await user.click(within(dialog).getByRole("button", { name: "Share" }))
    await waitFor(() => expect(shareItem).toHaveBeenCalledWith("i1", "bob@example.com", "fill-only", {}))
    expect(notices).toEqual(["Shared Router with bob@example.com"])
  })

  it("someone not compared yet reads // not compared in the With select", async () => {
    const unverified = { ...overview.contacts[0]!, verified: false }
    mount(login(), fakeSharing({ overview: () => Promise.resolve({ ...overview, contacts: [unverified] }) }))
    const user = userEvent.setup({ pointerEventsCheck: 0 })
    await user.click(await screen.findByRole("button", { name: "Share" }))
    await user.click(await screen.findByRole("menuitem", { name: "With someone…" }))
    expect(await screen.findByRole("option", { name: "bob@example.com // not compared" })).toBeInTheDocument()
  })

  it("nobody pinned: an Empty State with OPEN PEOPLE, and no Share", async () => {
    mount(login(), fakeSharing({ overview: () => Promise.resolve({ ...overview, contacts: [] }) }))
    const user = userEvent.setup({ pointerEventsCheck: 0 })
    await user.click(await screen.findByRole("button", { name: "Share" }))
    await user.click(await screen.findByRole("menuitem", { name: "With someone…" }))
    const dialog = await screen.findByRole("dialog", { name: "Share Router" })
    expect(await within(dialog).findByText("Look someone up first.")).toBeInTheDocument()
    expect(within(dialog).getByRole("button", { name: "Open people" })).toBeInTheDocument()
    expect(within(dialog).queryByRole("button", { name: "Share" })).not.toBeInTheDocument()
  })

  it("as a link: options, CREATE LINK through the gate, then the link once with COPY LINK as the one primary and DONE secondary", async () => {
    const link = "https://sync.example/s/abc#k"
    const sendItem = vi.fn(() => Promise.resolve(new TextEncoder().encode(link)))
    const copyLink = vi.fn(() => Promise.resolve({ copy: 1, clears_in_secs: 45 }))
    mount(login(), fakeSharing({ sendItem, copyLink }))
    const user = userEvent.setup({ pointerEventsCheck: 0 })
    await user.click(await screen.findByRole("button", { name: "Share" }))
    await user.click(await screen.findByRole("menuitem", { name: "As a link…" }))
    const dialog = await screen.findByRole("dialog", { name: "A link to Router" })
    expect(within(dialog).getByText("Anyone with the link can open a copy of this item as it is now.")).toBeInTheDocument()
    await user.selectOptions(within(dialog).getByLabelText("Lasts"), "3")
    await user.selectOptions(within(dialog).getByLabelText("Opens at most"), "")
    await user.click(within(dialog).getByRole("button", { name: "Create link" }))
    expect(await within(dialog).findByTestId("new-link")).toHaveTextContent(link)
    expect(sendItem).toHaveBeenCalledWith("i1", 3, null, null, {})
    // One primary on this step: COPY LINK. DONE is secondary; CREATE LINK has gone.
    const copy = within(dialog).getByRole("button", { name: "Copy link" })
    expect(copy).toHaveAttribute("data-variant", "default")
    expect(within(dialog).getByRole("button", { name: "Done" })).toHaveAttribute("data-variant", "secondary")
    expect(within(dialog).queryByRole("button", { name: "Create link" })).not.toBeInTheDocument()
    await user.click(copy)
    expect(await within(dialog).findByText("Copied. The clipboard clears in 45 seconds.")).toBeInTheDocument()
    await user.click(within(dialog).getByRole("button", { name: "Done" }))
    await waitFor(() => expect(screen.queryByTestId("new-link")).not.toBeInTheDocument())
  })

  it("opened from the item's ⋯ (open and onOpenChange): no button of its own, two rows, and a row opens its Form", async () => {
    const changes: boolean[] = []
    render(<Shared detail={login()} sharing={fakeSharing({ overview: () => Promise.resolve(overview) })} open onOpenChange={(o) => changes.push(o)} />)
    const user = userEvent.setup({ pointerEventsCheck: 0 })
    const chooser = await screen.findByRole("dialog", { name: "Share Router" })
    expect(screen.queryByRole("button", { name: "Share" })).not.toBeInTheDocument()
    await user.click(within(chooser).getByRole("button", { name: /^As a link…/ }))
    expect(changes).toEqual([false])
    expect(await screen.findByRole("dialog", { name: "A link to Router" })).toBeInTheDocument()
  })

  it("offers nothing for an item shared with the person, whatever the right, or for a crown jewel", () => {
    for (const access of ["fill-only", "view", "edit"] as const) {
      const { unmount } = mount(login(access))
      expect(screen.queryByRole("button", { name: "Share" })).not.toBeInTheDocument()
      unmount()
    }
    mount({ ...login(), crown_jewel: true })
    expect(screen.queryByRole("button", { name: "Share" })).not.toBeInTheDocument()
  })
})

describe("stopping a share", () => {
  it("asks first, says the item moves to a new key, and stops with no gate", async () => {
    const onStop = vi.fn(() => Promise.resolve())
    render(<StopSharing share={{ share_id: "s1", login: "bob@example.com" }} title="Router" onClose={() => {}} onStop={onStop} />)
    const question = await screen.findByRole("dialog", { name: "Stop sharing with bob@example.com?" })
    expect(within(question).getByText("The item moves to a new key. What they already saw, they may have kept.")).toBeInTheDocument()
    expect(onStop).not.toHaveBeenCalled()
    await userEvent.setup().click(within(question).getByRole("button", { name: "Stop sharing" }))
    await waitFor(() => expect(onStop).toHaveBeenCalledWith("s1", "bob@example.com"))
  })
})

describe("an item shared with you", () => {
  it("says Shared with you and what the right allows; a fill-only one is never shown or copied", () => {
    const { unmount } = render(<AccessNote access="view" />)
    expect(screen.getByText("Shared with you")).toBeInTheDocument()
    expect(screen.getByText("View: see, copy and fill")).toBeInTheDocument()
    unmount()
    render(<AccessNote access="fill-only" />)
    expect(screen.getByText(/never shown or copied/)).toBeInTheDocument()
    expect(FILL_ONLY_WORDS).toBe("Fills in the browser only; never shown or copied.")
  })
})
