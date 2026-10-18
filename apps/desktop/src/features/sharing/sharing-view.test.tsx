import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { vi } from "vitest"

import { GateProvider } from "@/app/gate"
import { SettingsProvider } from "@/app/settings"
import { ClientProvider } from "@/core/client"
import type { VaultView } from "@/core/types"
import { VaultsProvider } from "@/features/organization/vaults-context"
import { fakeClient } from "@/test/fake-client"
import { fakeSharing } from "@/test/fake-sharing"

import { SharingClientProvider, emergencyActions, type ContactView, type EmergencyView, type SharingClient, type SharingOverview, type VaultSharingView } from "./sharing-client"
import { SharingView } from "./sharing-view"

const bob: ContactView = {
  account_id: "acc-bob",
  login: "bob@example.com",
  persona: "p-bob",
  fingerprint: "12345 67890 12345 67890 12345 67890",
  qr: "lockdown:contact:v1:acc-bob:00",
  verified: false,
  devices: 2,
  log_index: 4,
}

const personal: VaultView = { id: "v1", name: "Personal", primary: true, items: 3, trash_days: 30, folders: [] }

function overview(over: Partial<SharingOverview> = {}): SharingOverview {
  return {
    online: true,
    fingerprint: "11111 22222 33333 44444 55555 66666",
    fingerprint_qr: "lockdown:contact:v1:me:00",
    contacts: [bob],
    vaults: [],
    households: [],
    sent: [],
    received: [],
    emergency: [],
    waiting_on_you: 0,
    ...over,
  }
}

function grant(over: Partial<EmergencyView>): EmergencyView {
  return { grant_id: "g1", mine: true, other: "bob@example.com", state: "invited", wait_days: 7, vaults: ["v1"], release_at: null, history: [], ...over }
}

function mount(sharing: SharingClient, vaults: VaultView[] = [personal]) {
  const client = fakeClient({ clipboardClear: () => Promise.resolve(true), clipboardHolds: () => Promise.resolve(true) })
  return render(
    <ClientProvider value={client}>
      <SettingsProvider>
        <GateProvider>
          <VaultsProvider value={{ vaults, reload: () => {} }}>
            <SharingClientProvider value={sharing}>
              <SharingView rows={[]} onOpenItem={() => {}} onChanged={() => {}} onNotice={() => {}} />
            </SharingClientProvider>
          </VaultsProvider>
        </GateProvider>
      </SettingsProvider>
    </ClientProvider>,
  )
}

const empty = { sends: () => Promise.resolve([]), requests: () => Promise.resolve([]) }

describe("the Sharing screen", () => {
  it("shows your fingerprint and the people pinned here, and says when sharing needs an account", async () => {
    mount(fakeSharing({ ...empty, overview: () => Promise.resolve(overview()) }))
    expect(await screen.findByText("11111 22222 33333 44444 55555 66666")).toBeInTheDocument()
    const people = screen.getByRole("list", { name: "People" })
    expect(within(people).getByText("bob@example.com")).toBeInTheDocument()
    expect(within(people).getByText("Not compared yet")).toBeInTheDocument()
    expect(screen.queryByText(/needs an account/)).not.toBeInTheDocument()
  })

  it("offline: says so, and nothing that needs the server is offered", async () => {
    mount(fakeSharing({ ...empty, overview: () => Promise.resolve(overview({ online: false, contacts: [] })) }))
    expect(await screen.findByText(/Sharing needs an account/)).toBeInTheDocument()
    expect(screen.getByRole("button", { name: "Look up" })).toBeDisabled()
    expect(screen.getByRole("button", { name: "Share some text…" })).toBeDisabled()
  })

  it("looks someone up, then compares fingerprints with what they read out", async () => {
    const lookup = vi.fn(() => Promise.resolve(bob))
    const verify = vi.fn(() => Promise.resolve({ ...bob, verified: true }))
    mount(fakeSharing({ ...empty, overview: () => Promise.resolve(overview({ contacts: [] })), lookup, verify }))
    const user = userEvent.setup()
    await user.type(await screen.findByLabelText("Look someone up by their sign-in"), "bob@example.com")
    await user.click(screen.getByRole("button", { name: "Look up" }))
    expect(lookup).toHaveBeenCalledWith("bob@example.com")
    const dialog = await screen.findByRole("dialog", { name: /Compare fingerprints with bob@example.com/ })
    await user.type(within(dialog).getByLabelText("What they read out"), "123456789012345678901234567890")
    await user.click(within(dialog).getByRole("button", { name: "They match" }))
    expect(verify).toHaveBeenCalledWith("acc-bob", "123456789012345678901234567890")
  })

  it("a refused comparison says why and verifies nothing", async () => {
    const verify = vi.fn(() => Promise.reject({ code: "refused", message: "the fingerprints don't match; don't share with this account" }))
    mount(fakeSharing({ ...empty, overview: () => Promise.resolve(overview()), verify }))
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "Compare" }))
    const dialog = await screen.findByRole("dialog")
    await user.type(within(dialog).getByLabelText("What they read out"), "999")
    await user.click(within(dialog).getByRole("button", { name: "They match" }))
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("the fingerprints don't match")
  })

  it("a manager sees rights and Remove; removing confirms and says the key changes", async () => {
    const view: VaultSharingView = {
      vault_id: "v1",
      shared: true,
      kind: "shared",
      right: "manage",
      can_manage: true,
      members: [
        { account_id: "me", login: "me@example.com", right: "manage", right_words: "Manage", owner: true, you: true, verified: true },
        { account_id: "acc-bob", login: "bob@example.com", right: "view", right_words: "View", owner: false, you: false, verified: false },
      ],
      history: [{ at: 1_700_000_000, words: "me@example.com added bob@example.com (view)" }],
      blocked: null,
    }
    const removeMember = vi.fn(() => Promise.resolve({ ...view, members: view.members.slice(0, 1) }))
    const setRight = vi.fn(() => Promise.resolve(view))
    const shared: VaultView = { ...personal, sharing: { kind: "shared", right: "manage", owned: true, owner: null, people: 1 } }
    mount(fakeSharing({ ...empty, overview: () => Promise.resolve(overview()), vault: () => Promise.resolve(view), removeMember, setRight }), [shared])
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "Manage sharing" }))
    const dialog = await screen.findByRole("dialog", { name: 'Sharing "Personal"' })
    expect(await within(dialog).findByText(/added bob@example.com/)).toBeInTheDocument()
    await user.selectOptions(within(dialog).getByLabelText("What bob@example.com can do"), "edit")
    expect(setRight).toHaveBeenCalledWith("v1", "acc-bob", "edit")
    await user.click(within(dialog).getByRole("button", { name: "Remove" }))
    const confirm = await screen.findByRole("dialog", { name: "Remove bob@example.com?" })
    expect(confirm).toHaveTextContent(/moves to a new key/)
    await user.click(within(confirm).getByRole("button", { name: "Remove and change the key" }))
    await waitFor(() => expect(removeMember).toHaveBeenCalledWith("v1", "acc-bob"))
  })

  it("a member who can only view sees who has it and no controls", async () => {
    const view: VaultSharingView = {
      vault_id: "v2",
      shared: true,
      kind: "shared",
      right: "view",
      can_manage: false,
      members: [
        { account_id: "acc-ann", login: "ann@example.com", right: "manage", right_words: "Manage", owner: true, you: false, verified: true },
        { account_id: "me", login: "me@example.com", right: "view", right_words: "View", owner: false, you: true, verified: true },
      ],
      history: [],
      blocked: null,
    }
    const theirs: VaultView = { ...personal, id: "v2", name: "Ann's", primary: false, sharing: { kind: "shared", right: "view", owned: false, owner: "ann@example.com", people: 2 } }
    mount(fakeSharing({ ...empty, overview: () => Promise.resolve(overview()), vault: () => Promise.resolve(view) }), [theirs])
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "Who has it" }))
    const dialog = await screen.findByRole("dialog")
    expect(await within(dialog).findByText("ann@example.com")).toBeInTheDocument()
    expect(within(dialog).queryByRole("button", { name: "Remove" })).not.toBeInTheDocument()
    expect(within(dialog).queryByRole("combobox")).not.toBeInTheDocument()
    expect(within(dialog).queryByRole("button", { name: "Add" })).not.toBeInTheDocument()
  })

  it("a vault with a crown jewel says why it can't be shared and offers no Add", async () => {
    const view: VaultSharingView = { vault_id: "v1", shared: false, kind: "shared", right: "manage", can_manage: true, members: [], history: [], blocked: "Move the crown jewels out first." }
    mount(fakeSharing({ ...empty, overview: () => Promise.resolve(overview()), vault: () => Promise.resolve(view) }))
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "Manage sharing" }))
    const dialog = await screen.findByRole("dialog")
    expect(await within(dialog).findByText("Move the crown jewels out first.")).toBeInTheDocument()
    expect(within(dialog).queryByRole("button", { name: "Add" })).not.toBeInTheDocument()
  })

  it("makes a text share link, shows it once, and copies it on the shell's clipboard path", async () => {
    const link = "https://sync.example/s/abc#key"
    const sendText = vi.fn(() => Promise.resolve(new TextEncoder().encode(link)))
    const copyLink = vi.fn(() => Promise.resolve({ copy: 3, clears_in_secs: 30 }))
    mount(fakeSharing({ ...empty, overview: () => Promise.resolve(overview()), sendText, copyLink }))
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "Share some text…" }))
    const dialog = await screen.findByRole("dialog", { name: "Share some text" })
    await user.type(within(dialog).getByLabelText("What it is (they see this)"), "Door code")
    await user.type(within(dialog).getByLabelText("The text"), "4321")
    await user.click(within(dialog).getByRole("button", { name: "Make the link" }))
    expect(await within(dialog).findByDisplayValue(link)).toBeInTheDocument()
    // Title, the text as bytes, 7 days, once, no password.
    expect(sendText).toHaveBeenCalledWith("Door code", Array.from(new TextEncoder().encode("4321")), 7, 1, null)
    await user.click(within(dialog).getByRole("button", { name: "Copy the link" }))
    expect(copyLink).toHaveBeenCalledWith(Array.from(new TextEncoder().encode(link)), 30)
    expect(await within(dialog).findByText("Copied. The clipboard clears in 30 seconds.")).toBeInTheDocument()
    await user.click(within(dialog).getByRole("button", { name: "Done" }))
    expect(screen.queryByDisplayValue(link)).not.toBeInTheDocument()
  })

  it("opens a link someone sent: plain values drawn, secrets masked until asked", async () => {
    const openSend = vi.fn(() =>
      Promise.resolve({
        kind: "item" as const,
        title: "Router",
        values: [
          { label: "username", secret: false },
          { label: "password", secret: true },
        ],
        file: null,
      }),
    )
    const openedValue = vi.fn((i: number) => Promise.resolve(new TextEncoder().encode(i === 0 ? "admin" : "hunter2")))
    mount(fakeSharing({ ...empty, overview: () => Promise.resolve(overview()), openSend, openedValue }))
    const user = userEvent.setup()
    await user.type(await screen.findByLabelText("Open a share link someone sent you"), "https://sync.example/s/abc#key")
    await user.type(screen.getByLabelText("Its password, if it has one"), "pw")
    await user.click(screen.getByRole("button", { name: "Open" }))
    expect(openSend).toHaveBeenCalledWith(Array.from(new TextEncoder().encode("https://sync.example/s/abc#key")), Array.from(new TextEncoder().encode("pw")))
    const panel = await screen.findByRole("region", { name: "Opened: Router" })
    expect(await within(panel).findByText("admin")).toBeInTheDocument()
    expect(within(panel).queryByText("hunter2")).not.toBeInTheDocument()
    expect(openedValue).not.toHaveBeenCalledWith(1)
    expect(within(panel).getByRole("button", { name: "Keep in my vault" })).toBeInTheDocument()
  })

  it("answers someone's request link: shows what they ask for, sends the secret once", async () => {
    const requestLabel = vi.fn(() => Promise.resolve("The Wi-Fi password"))
    const answerRequest = vi.fn(() => Promise.resolve())
    mount(fakeSharing({ ...empty, overview: () => Promise.resolve(overview()), requestLabel, answerRequest }))
    const user = userEvent.setup()
    await user.type(await screen.findByLabelText("Answer someone's request link"), "https://sync.example/r/xyz#pin")
    await user.click(screen.getByRole("button", { name: "Continue" }))
    const dialog = await screen.findByRole("dialog", { name: "They ask for: The Wi-Fi password" })
    await user.type(within(dialog).getByLabelText("The secret"), "s3cret")
    await user.click(within(dialog).getByRole("button", { name: "Send it" }))
    await waitFor(() => expect(answerRequest).toHaveBeenCalled())
    const [sentLink, , sentSecret] = (answerRequest.mock.calls as unknown as Array<[number[], string, number[]]>)[0] ?? [[], "", [1]]
    expect(sentLink).toEqual(Array.from(new TextEncoder().encode("https://sync.example/r/xyz#pin")))
    // The typed secret's bytes are wiped once the call is made.
    expect(sentSecret.every((b) => b === 0)).toBe(true)
  })

  it("someone asked for emergency access: the screen says so and Stop is one press", async () => {
    const asked = grant({ state: "requested", release_at: 1_900_000_000 })
    const emergencyStep = vi.fn(() => Promise.resolve([grant({ state: "accepted" })]))
    mount(fakeSharing({ ...empty, overview: () => Promise.resolve(overview({ emergency: [asked], waiting_on_you: 1 })), emergencyStep }))
    expect(await screen.findByText(/Someone asked for emergency access/)).toBeInTheDocument()
    const user = userEvent.setup()
    await user.click(screen.getByRole("button", { name: "Stop it" }))
    expect(emergencyStep).toHaveBeenCalledWith("g1", "stop")
    expect(await screen.findByText(/can ask for access; you'd have 7 days/)).toBeInTheDocument()
  })

  it("an emergency contact must have compared fingerprints first", async () => {
    mount(fakeSharing({ ...empty, overview: () => Promise.resolve(overview()) }))
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "Add an emergency contact…" }))
    const dialog = await screen.findByRole("dialog")
    expect(within(dialog).getByText(/Only someone whose fingerprint you compared/)).toBeInTheDocument()
    expect(within(dialog).queryByRole("button", { name: "Ask them" })).not.toBeInTheDocument()
  })

  it("adds a verified emergency contact through the gate with the wait and vaults picked", async () => {
    const addEmergency = vi.fn(() => Promise.resolve([grant({})]))
    mount(fakeSharing({ ...empty, overview: () => Promise.resolve(overview({ contacts: [{ ...bob, verified: true }] })), addEmergency }))
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "Add an emergency contact…" }))
    const dialog = await screen.findByRole("dialog")
    await user.selectOptions(within(dialog).getByLabelText("Waiting period"), "14")
    await user.click(within(dialog).getByRole("button", { name: "Ask them" }))
    await waitFor(() => expect(addEmergency).toHaveBeenCalledWith("bob@example.com", 14, ["v1"], {}))
  })
})

describe("emergency steps by state", () => {
  const names = (g: EmergencyView) => emergencyActions(g).map((a) => a.action)
  it("the owner can stop a request until it is released, and end it any time", () => {
    expect(names(grant({ state: "invited" }))).toEqual(["revoke"])
    expect(names(grant({ state: "requested" }))).toEqual(["stop", "revoke"])
    expect(names(grant({ state: "ready" }))).toEqual(["stop", "revoke"])
    expect(names(grant({ state: "released" }))).toEqual(["revoke"])
    expect(names(grant({ state: "revoked" }))).toEqual([])
  })
  it("the contact agrees, asks, and opens only once the wait ran out", () => {
    const c = (state: EmergencyView["state"]) => grant({ mine: false, state })
    expect(names(c("invited"))).toEqual(["accept", "decline"])
    expect(names(c("accepted"))).toEqual(["request", "decline"])
    expect(names(c("requested"))).toEqual(["decline"])
    expect(names(c("ready"))).toEqual(["open"])
    expect(names(c("declined"))).toEqual([])
  })
})
