import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import type { ReactNode } from "react"
import { vi } from "vitest"

import { AppFrame, spellDigitGroups, splitDigitGroups } from "@unicron/ui"

import { GateProvider } from "@/app/gate"
import { NeedsYouSlot, WaitingHost } from "@/app/needs-you"
import { SettingsProvider } from "@/app/settings"
import { ClientProvider } from "@/core/client"
import type { VaultView } from "@/core/types"
import { VaultsProvider } from "@/features/organization/vaults-context"
import { fakeClient } from "@/test/fake-client"
import { fakeSharing } from "@/test/fake-sharing"

import { grantActions } from "./emergency-model"
import { RESET_HINT, RESET_TITLE, letBackInNotice } from "./key-resets-words"
import { OFFLINE_REASON } from "./people-section"
import { SharingClientProvider, emergencyActions, type ContactView, type EmergencyView, type SharingClient, type SharingOverview, type VaultSharingView } from "./sharing-client"
import { PAUSED_TITLE } from "./sharing-paused"
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
const MY_FINGERPRINT = "11111 22222 33333 44444 55555 66666"
/** What a screen reader hears of a fingerprint in DigitGroups: one character at a time, a pause at each group. */
const heard = (label: string, code: string) => `${label}: ${spellDigitGroups(splitDigitGroups(code))}`

function overview(over: Partial<SharingOverview> = {}): SharingOverview {
  return {
    online: true,
    fingerprint: MY_FINGERPRINT,
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

const client = fakeClient({ clipboardClear: () => Promise.resolve(true), clipboardHolds: () => Promise.resolve(true) })

/** Sharing as the shell draws it at 1440: the frame, the gate, the needs-you host on Sharing and its block. */
function Screen({ sharing, vaults = [personal], onNotice = () => {}, children }: { sharing: SharingClient; vaults?: VaultView[]; onNotice?: (t: string) => void; children?: ReactNode }) {
  return (
    <ClientProvider value={client}>
      <SettingsProvider>
        <GateProvider>
          <AppFrame layout="expanded">
            <WaitingHost place={{ sharing: true, item: null }}>
              <VaultsProvider value={{ vaults, reload: () => {} }}>
                <SharingClientProvider value={sharing}>
                  <NeedsYouSlot />
                  <SharingView rows={[]} onOpenItem={() => {}} onChanged={() => {}} onNotice={onNotice} />
                  {children}
                </SharingClientProvider>
              </VaultsProvider>
            </WaitingHost>
          </AppFrame>
        </GateProvider>
      </SettingsProvider>
    </ClientProvider>
  )
}

function mount(sharing: SharingClient, vaults: VaultView[] = [personal]) {
  return render(<Screen sharing={sharing} vaults={vaults} />)
}

const empty = { sends: () => Promise.resolve([]), requests: () => Promise.resolve([]) }
const blocks = () => document.querySelectorAll('[data-slot="needs-you"]')

/** Opens a tab by its name (the count follows the name). */
async function openTab(user: ReturnType<typeof userEvent.setup>, name: string) {
  await user.click(await screen.findByRole("tab", { name: new RegExp(`^${name}`) }))
}

/** Opens a person's detail from the People list. */
async function openPerson(user: ReturnType<typeof userEvent.setup>, login: string) {
  const people = await screen.findByRole("list", { name: "People" })
  await user.click(within(people).getByRole("button", { name: login }))
  return screen.findByRole("region", { name: "People: open" })
}

/** Opens a vault's members from the Vaults tab. */
async function openVault(user: ReturnType<typeof userEvent.setup>, name: string) {
  await openTab(user, "Vaults")
  const list = await screen.findByRole("list", { name: "Vaults" })
  await user.click(within(list).getByRole("button", { name }))
  return screen.findByRole("region", { name: "Vaults: open" })
}

describe("the Sharing screen", () => {
  it("shows your fingerprint in groups and the people pinned here, with whether each was compared", async () => {
    mount(fakeSharing({ ...empty, overview: () => Promise.resolve(overview()) }))
    expect(await screen.findByText(heard("Your fingerprint", MY_FINGERPRINT))).toBeInTheDocument()
    expect(screen.getByLabelText("Your code to paste")).toHaveTextContent("lockdown:contact:v1:me:00")
    const people = screen.getByRole("list", { name: "People" })
    expect(within(people).getByText("bob@example.com")).toBeInTheDocument()
    expect(within(people).getByRole("listitem")).toHaveTextContent("Fingerprint not compared")
    expect(screen.getByRole("heading", { level: 1, name: "Sharing" })).toBeInTheDocument()
    expect(screen.getByText("Sharing // 01 person // 00 shared vaults")).toBeInTheDocument()
    expect(screen.queryByText("You're offline.")).not.toBeInTheDocument()
  })

  it("four tabs, People first; the detail says nothing is open until a person is picked", async () => {
    mount(fakeSharing({ ...empty, overview: () => Promise.resolve(overview()) }))
    const tabs = await screen.findAllByRole("tab")
    expect(tabs.map((t) => t.textContent)).toEqual(["People01", "Vaults01", "Links00", "Emergency00"])
    expect(screen.getByRole("tab", { name: /^People/ })).toHaveAttribute("aria-selected", "true")
    expect(within(screen.getByRole("region", { name: "People: open" })).getByText("Nothing open.")).toBeInTheDocument()
  })

  it("shows the code to compare as a QR code behind Show code", async () => {
    mount(fakeSharing({ ...empty, overview: () => Promise.resolve(overview()) }))
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: /^Show code/ }))
    const dialog = await screen.findByRole("dialog", { name: "Your code" })
    expect(within(dialog).getByRole("img", { name: /QR code of your fingerprint/ })).toBeInTheDocument()
    expect(within(dialog).getByText(heard("Your fingerprint", MY_FINGERPRINT))).toBeInTheDocument()
  })

  it("a split view: says what is paused and why, in the core's words, with See status", async () => {
    const paused =
      "Sharing is paused: this server's public log doesn't match a mirror, and a server showing two histories could swap someone's keys. Until it clears, Lockdown won't look anyone up or check their keys with a phrase, share a vault or an item, make share or request links, name an emergency contact or open emergency access. Your vault still opens and syncs."
    mount(fakeSharing({ ...empty, overview: () => Promise.resolve(overview({ paused })) }))
    const alert = await screen.findByText(/^Sharing is paused/)
    expect(alert.closest("[role=alert]")).toHaveTextContent(PAUSED_TITLE)
    expect(alert).toHaveTextContent("name an emergency contact")
    expect(within(alert.closest("[role=alert]") as HTMLElement).getByRole("button", { name: "See status" })).toBeInTheDocument()
    expect(screen.getByRole("button", { name: "Look up" })).toBeDisabled()
  })

  it("someone who reset their account is the needs-you on Sharing, never a dialog, until it is answered", async () => {
    const reset = { account_id: "acc-bob", login: "bob@example.com", words: "bob@example.com reset their account. Let them back in?", seen_at: 0 }
    const letBackIn = vi.fn(() => Promise.resolve("bob@example.com"))
    const notices: string[] = []
    const sharing = fakeSharing({ ...empty, overview: () => Promise.resolve(overview({ resets: [reset] })), letBackIn })
    const { unmount } = render(<Screen sharing={sharing} onNotice={(t) => notices.push(t)} />)
    const user = userEvent.setup({ pointerEventsCheck: 0 })
    await waitFor(() => expect(blocks()).toHaveLength(1))
    const block = blocks()[0] as HTMLElement
    expect(block).toHaveAttribute("data-needs-you-row", "6")
    expect(block).toHaveTextContent(RESET_TITLE)
    expect(block).toHaveTextContent("bob@example.com reset their account. Let them back in?")
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument()
    // Their detail says what yes means, so nobody says it by reflex.
    const detail = await openPerson(user, "bob@example.com")
    expect(within(detail).getByText(RESET_HINT)).toBeInTheDocument()
    // Not now: it goes from this screen, and nothing is answered.
    await user.click(within(block).getByRole("button", { name: "Not now" }))
    await waitFor(() => expect(blocks()).toHaveLength(0))
    expect(letBackIn).not.toHaveBeenCalled()
    // Opened again, it asks again; yes goes through the gate.
    unmount()
    render(<Screen sharing={sharing} onNotice={(t) => notices.push(t)} />)
    await waitFor(() => expect(blocks()).toHaveLength(1))
    await user.click(within(blocks()[0] as HTMLElement).getByRole("button", { name: "Let them back in" }))
    await waitFor(() => expect(letBackIn).toHaveBeenCalledWith("acc-bob", {}))
    await waitFor(() => expect(notices).toContain(letBackInNotice("bob@example.com")))
  })

  it("two resets: one needs-you with // +1, the other as an ordinary row under Waiting on you", async () => {
    const one = { account_id: "acc-bob", login: "bob@example.com", words: "bob@example.com reset their account. Let them back in?", seen_at: 0 }
    const two = { account_id: "acc-ann", login: "ann@example.com", words: "ann@example.com reset their account. Let them back in?", seen_at: 0 }
    mount(fakeSharing({ ...empty, overview: () => Promise.resolve(overview({ resets: [one, two] })) }))
    await waitFor(() => expect(blocks()).toHaveLength(1))
    expect(blocks()[0]).toHaveTextContent("bob@example.com reset their account")
    const rows = await screen.findByRole("list", { name: "Waiting on you" })
    expect(within(rows).getAllByRole("listitem")).toHaveLength(1)
    expect(rows).toHaveTextContent("ann@example.com reset their account")
    expect(within(rows).getByRole("button", { name: "Let them back in" })).toBeInTheDocument()
  })

  it("nobody waiting: no needs-you", async () => {
    mount(fakeSharing({ ...empty, overview: () => Promise.resolve(overview()) }))
    expect(await screen.findByText(heard("Your fingerprint", MY_FINGERPRINT))).toBeInTheDocument()
    expect(blocks()).toHaveLength(0)
    expect(screen.queryByRole("list", { name: "Waiting on you" })).not.toBeInTheDocument()
  })

  it("no split: no pause banner", async () => {
    mount(fakeSharing({ ...empty, overview: () => Promise.resolve(overview({ paused: null })) }))
    expect(await screen.findByText(heard("Your fingerprint", MY_FINGERPRINT))).toBeInTheDocument()
    expect(screen.queryByText(/Sharing is paused/)).not.toBeInTheDocument()
    expect(screen.queryByRole("alert")).not.toBeInTheDocument()
  })

  it("offline: says so, keeps the lists readable, and each control that needs the server says why it is off", async () => {
    const reset = { account_id: "acc-bob", login: "bob@example.com", words: "bob@example.com reset their account. Let them back in?", seen_at: 0 }
    mount(fakeSharing({ ...empty, overview: () => Promise.resolve(overview({ online: false, resets: [reset] })) }))
    expect(await screen.findByText("You're offline.")).toBeInTheDocument()
    expect(screen.getByText("Sharing needs the server.")).toBeInTheDocument()
    // The list stays.
    expect(within(screen.getByRole("list", { name: "People" })).getByText("bob@example.com")).toBeInTheDocument()
    // Look up: off, with its reason tied to the field.
    expect(screen.getByRole("button", { name: "Look up" })).toBeDisabled()
    const field = screen.getByLabelText("Look someone up by their sign-in")
    expect(field).toHaveAccessibleDescription(expect.stringContaining(OFFLINE_REASON))
    // A yes that needs the server would fail: no needs-you while offline.
    expect(blocks()).toHaveLength(0)
    const user = userEvent.setup()
    await openTab(user, "Links")
    // Off, and still reachable, each with its reason (#37).
    for (const name of ["Share text", "Ask for secret", "Open"]) {
      const button = await screen.findByRole("button", { name })
      expect(button).toHaveAttribute("aria-disabled", "true")
      expect(button).toHaveAccessibleDescription(OFFLINE_REASON)
    }
  })

  it("offline: a vault's Add, Change and Remove are off with the reason under them", async () => {
    const view: VaultSharingView = {
      vault_id: "v1",
      shared: true,
      kind: "shared",
      right: "manage",
      can_manage: true,
      members: [
        { account_id: "me", login: "me@example.com", right: "manage", right_words: "Manage", owner: true, you: true, verified: true },
        { account_id: "acc-ann", login: "ann@example.com", right: "view", right_words: "View", owner: false, you: false, verified: true },
      ],
      history: [],
      blocked: null,
    }
    const shared: VaultView = { ...personal, sharing: { kind: "shared", right: "manage", owned: true, owner: null, people: 1 } }
    mount(fakeSharing({ ...empty, overview: () => Promise.resolve(overview({ online: false })), vault: () => Promise.resolve(view) }), [shared])
    const user = userEvent.setup()
    const detail = await openVault(user, "Personal")
    for (const name of ["Change", "Remove", "Add"]) {
      const button = await within(detail).findByRole("button", { name })
      expect(button).toHaveAttribute("aria-disabled", "true")
      expect(button).toHaveAccessibleDescription(expect.stringContaining(OFFLINE_REASON))
    }
  })

  it("looks someone up, opens them, then compares fingerprints with what they read out", async () => {
    const lookup = vi.fn((_login: string) => Promise.resolve(bob))
    const verify = vi.fn(() => Promise.resolve({ ...bob, verified: true }))
    let contacts: ContactView[] = []
    mount(fakeSharing({ ...empty, overview: () => Promise.resolve(overview({ contacts })), lookup: (l) => ((contacts = [bob]), lookup(l)), verify }))
    const user = userEvent.setup()
    expect(await screen.findByText("Nobody yet.")).toBeInTheDocument()
    await user.type(screen.getByLabelText("Look someone up by their sign-in"), "bob@example.com")
    await user.click(screen.getByRole("button", { name: "Look up" }))
    expect(lookup).toHaveBeenCalledWith("bob@example.com")
    const dialog = await screen.findByRole("dialog", { name: /Compare fingerprints with bob@example.com/ })
    await user.type(within(dialog).getByLabelText("What they read out"), "123456789012345678901234567890")
    await user.click(within(dialog).getByRole("button", { name: "They match" }))
    expect(verify).toHaveBeenCalledWith("acc-bob", "123456789012345678901234567890")
  })

  it("a person's detail: Compare fingerprints is the one primary while they are not compared", async () => {
    mount(fakeSharing({ ...empty, overview: () => Promise.resolve(overview()) }))
    const user = userEvent.setup()
    const detail = await openPerson(user, "bob@example.com")
    expect(within(detail).getByText("Person // not compared")).toBeInTheDocument()
    expect(within(detail).getByRole("heading", { name: "bob@example.com" })).toBeInTheDocument()
    expect(within(detail).getByText(heard("bob@example.com's fingerprint", bob.fingerprint))).toBeInTheDocument()
    const solid = within(detail)
      .getAllByRole("button")
      .filter((b) => b.getAttribute("data-variant") === "default")
    expect(solid.map((b) => b.textContent)).toEqual(["Compare fingerprints"])
    expect(within(detail).getByRole("button", { name: "Check with a phrase" })).toBeInTheDocument()
    expect(within(detail).getByRole("button", { name: "Forget" })).toBeInTheDocument()
  })

  it("a refused comparison says why under the field and verifies nothing", async () => {
    const verify = vi.fn(() => Promise.reject({ code: "refused", message: "the fingerprints don't match; don't share with this account" }))
    const notices: string[] = []
    render(<Screen sharing={fakeSharing({ ...empty, overview: () => Promise.resolve(overview()), verify })} onNotice={(t) => notices.push(t)} />)
    const user = userEvent.setup()
    const detail = await openPerson(user, "bob@example.com")
    await user.click(within(detail).getByRole("button", { name: "Compare fingerprints" }))
    const dialog = await screen.findByRole("dialog")
    await user.type(within(dialog).getByLabelText("What they read out"), "999")
    await user.click(within(dialog).getByRole("button", { name: "They match" }))
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("the fingerprints don't match")
    expect(within(dialog).getByLabelText("What they read out")).toHaveAttribute("aria-invalid", "true")
    expect(notices.some((n) => n.includes("verified"))).toBe(false)
  })

  it("forget asks first, with Cancel focused, and the yes repeats the verb", async () => {
    const forget = vi.fn(() => Promise.resolve())
    mount(fakeSharing({ ...empty, overview: () => Promise.resolve(overview()), forget }))
    const user = userEvent.setup()
    const detail = await openPerson(user, "bob@example.com")
    await user.click(within(detail).getByRole("button", { name: "Forget" }))
    const question = await screen.findByRole("dialog", { name: "Forget bob@example.com?" })
    await waitFor(() => expect(within(question).getByRole("button", { name: "Cancel" })).toHaveFocus())
    expect(forget).not.toHaveBeenCalled()
    await user.click(within(question).getByRole("button", { name: "Forget" }))
    await waitFor(() => expect(forget).toHaveBeenCalledWith("acc-bob"))
  })

  it("a manager sees rights and Remove; removing asks and says the key changes", async () => {
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
    const detail = await openVault(user, "Personal")
    expect(within(detail).getByText("Vault // shared // 2 people")).toBeInTheDocument()
    const members = await within(detail).findByRole("list", { name: "Members" })
    expect(within(members).getByText("Not compared")).toBeInTheDocument()
    // History is shut until asked.
    await user.click(within(detail).getByRole("button", { name: /^History/ }))
    expect(await within(detail).findByText(/added bob@example.com/)).toBeInTheDocument()
    // Change: D187's four rights as RadioRows; a pick applies it.
    await user.click(within(members).getByRole("button", { name: "Change" }))
    const picker = await screen.findByRole("dialog", { name: "What can bob@example.com do?" })
    expect(within(picker).getByRole("radio", { name: "Edit" })).toHaveAccessibleDescription("see, fill and change items")
    await user.click(within(picker).getByRole("radio", { name: "Edit" }))
    await waitFor(() => expect(setRight).toHaveBeenCalledWith("v1", "acc-bob", "edit"))
    // Remove is a text button; the red is in the Question.
    const remove = within(members).getByRole("button", { name: "Remove" })
    expect(remove).toHaveAttribute("data-variant", "ghost")
    await user.click(remove)
    const confirm = await screen.findByRole("dialog", { name: "Remove bob@example.com?" })
    expect(confirm).toHaveTextContent(/moves to a new key/)
    await user.click(within(confirm).getByRole("button", { name: "Remove and change key" }))
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
    const detail = await openVault(user, "Ann's")
    expect(await within(detail).findByText("ann@example.com")).toBeInTheDocument()
    expect(within(detail).queryByRole("button", { name: "Remove" })).not.toBeInTheDocument()
    expect(within(detail).queryByRole("button", { name: "Change" })).not.toBeInTheDocument()
    expect(within(detail).queryByRole("combobox")).not.toBeInTheDocument()
    expect(within(detail).queryByRole("button", { name: "Add" })).not.toBeInTheDocument()
  })

  it("during a split, a held vault says new members wait, in the core's words, and hides its controls (D572, D573)", async () => {
    const held = "New members wait until the server's log matches its mirrors again."
    const view: VaultSharingView = {
      vault_id: "v1",
      shared: true,
      kind: "shared",
      right: "manage",
      can_manage: true,
      members: [
        { account_id: "me", login: "me@example.com", right: "manage", right_words: "Manage", owner: true, you: true, verified: true },
        { account_id: "acc-ann", login: "ann@example.com", right: "view", right_words: "View", owner: false, you: false, verified: true },
      ],
      history: [],
      blocked: null,
      held,
    }
    const shared: VaultView = { ...personal, sharing: { kind: "shared", right: "manage", owned: true, owner: null, people: 1 } }
    mount(fakeSharing({ ...empty, overview: () => Promise.resolve(overview()), vault: () => Promise.resolve(view) }), [shared])
    const user = userEvent.setup()
    const detail = await openVault(user, "Personal")
    expect((await within(detail).findByText(held)).closest("[role=status]")).not.toBeNull()
    expect(within(detail).getByText("ann@example.com")).toBeInTheDocument()
    expect(within(detail).queryByRole("button", { name: "Change" })).not.toBeInTheDocument()
    expect(within(detail).queryByRole("button", { name: "Remove" })).not.toBeInTheDocument()
    expect(within(detail).queryByRole("form", { name: "Add someone" })).not.toBeInTheDocument()
  })

  it("a vault with a crown jewel says why it can't be shared and offers no Add", async () => {
    const view: VaultSharingView = { vault_id: "v1", shared: false, kind: "shared", right: "manage", can_manage: true, members: [], history: [], blocked: "Move the crown jewels out first." }
    mount(fakeSharing({ ...empty, overview: () => Promise.resolve(overview()), vault: () => Promise.resolve(view) }))
    const user = userEvent.setup()
    const detail = await openVault(user, "Personal")
    expect(await within(detail).findByText("Move the crown jewels out first.")).toBeInTheDocument()
    expect(within(detail).queryByRole("button", { name: "Add" })).not.toBeInTheDocument()
  })

  it("a vault that is yours alone: Only you, and Add someone through the gate with View by default", async () => {
    const view: VaultSharingView = { vault_id: "v1", shared: false, kind: "shared", right: "manage", can_manage: true, members: [], history: [], blocked: null }
    const shareVault = vi.fn(() => Promise.resolve({ ...view, shared: true }))
    mount(fakeSharing({ ...empty, overview: () => Promise.resolve(overview()), vault: () => Promise.resolve(view), shareVault }))
    const user = userEvent.setup()
    const detail = await openVault(user, "Personal")
    expect(await within(detail).findByText("Only you.")).toBeInTheDocument()
    expect(within(detail).getByLabelText("They can")).toHaveValue("view")
    await user.click(within(detail).getByRole("button", { name: "Add" }))
    await waitFor(() => expect(shareVault).toHaveBeenCalledWith("v1", "bob@example.com", "view", {}))
  })

  it("makes a text share link in a Form, shows it once with COPY LINK as the one primary, and copies it on the shell's clipboard path", async () => {
    const link = "https://sync.example/s/abc#key"
    const sendText = vi.fn(() => Promise.resolve(new TextEncoder().encode(link)))
    const copyLink = vi.fn(() => Promise.resolve({ copy: 3, clears_in_secs: 30 }))
    mount(fakeSharing({ ...empty, overview: () => Promise.resolve(overview()), sendText, copyLink }))
    const user = userEvent.setup()
    await openTab(user, "Links")
    await user.click(await screen.findByRole("button", { name: "Share text" }))
    const dialog = await screen.findByRole("dialog", { name: "Share text" })
    await user.type(within(dialog).getByLabelText("What it is (they see this)"), "Door code")
    await user.type(within(dialog).getByLabelText("The text"), "4321")
    await user.click(within(dialog).getByRole("button", { name: "Create link" }))
    // Shown once: the whole link is the line's text, cut in the middle on screen.
    expect(await within(dialog).findByTestId("new-link")).toHaveTextContent(link)
    // Title, the text as bytes, 7 days, once, no password.
    expect(sendText).toHaveBeenCalledWith("Door code", expect.any(Array), 7, 1, null)
    expect(within(dialog).queryByRole("button", { name: "Create link" })).not.toBeInTheDocument()
    expect(within(dialog).getByText("Send it in a way you trust. It isn't shown again after you close this.")).toBeInTheDocument()
    await user.click(within(dialog).getByRole("button", { name: "Copy link" }))
    expect(copyLink).toHaveBeenCalledWith(Array.from(new TextEncoder().encode(link)), 30)
    expect(await within(dialog).findByText("Copied. The clipboard clears in 30 seconds.")).toBeInTheDocument()
    await user.click(within(dialog).getByRole("button", { name: "Done" }))
    // Closed, the link is gone: it is never shown again.
    await waitFor(() => expect(screen.queryByTestId("new-link")).not.toBeInTheDocument())
    await user.click(screen.getByRole("button", { name: "Share text" }))
    expect(within(await screen.findByRole("dialog", { name: "Share text" })).queryByTestId("new-link")).not.toBeInTheDocument()
  })

  it("the link's seconds come from the copy's own clears_in_secs", async () => {
    const link = "https://sync.example/r/xyz#pin"
    mount(fakeSharing({ ...empty, overview: () => Promise.resolve(overview()), createRequest: () => Promise.resolve(new TextEncoder().encode(link)), copyLink: () => Promise.resolve({ copy: 4, clears_in_secs: 12 }) }))
    const user = userEvent.setup()
    await openTab(user, "Links")
    await user.click(await screen.findByRole("button", { name: "Ask for secret" }))
    const dialog = await screen.findByRole("dialog", { name: "Ask for a secret" })
    await user.type(within(dialog).getByLabelText("What you're asking for (they see this)"), "The Wi-Fi password")
    await user.click(within(dialog).getByRole("button", { name: "Create link" }))
    expect(await within(dialog).findByText("Send it to them. It works once, until it expires.")).toBeInTheDocument()
    await user.click(within(dialog).getByRole("button", { name: "Copy link" }))
    expect(await within(dialog).findByText("Copied. The clipboard clears in 12 seconds.")).toBeInTheDocument()
  })

  it("lists the links and requests made here, in rows, and deleting one asks first", async () => {
    const day = 86_400
    const sends = [{ send_id: "s1", created_at: 1_790_000_000, expires_at: 1_790_000_000 + 7 * day, max_views: 3, views: 1, password: true }]
    const requests = [{ request_id: "r1", created_at: 1_790_000_000, expires_at: 1_790_000_000 + 7 * day, answered_at: null }]
    const deleteSend = vi.fn(() => Promise.resolve())
    mount(fakeSharing({ overview: () => Promise.resolve(overview()), sends: () => Promise.resolve(sends), requests: () => Promise.resolve(requests), deleteSend }))
    const user = userEvent.setup()
    await openTab(user, "Links")
    const links = await screen.findByRole("list", { name: "Share links" })
    expect(within(links).getByText(/opened 1 of 3 \/\/ password/)).toBeInTheDocument()
    expect(within(screen.getByRole("list", { name: "Requests" })).getByText(/^waiting \/\/ expires/)).toBeInTheDocument()
    await user.click(within(links).getByRole("button", { name: "Delete" }))
    const question = await screen.findByRole("dialog", { name: "Delete this link?" })
    // Asking deletes nothing.
    expect(deleteSend).not.toHaveBeenCalled()
    await user.click(within(question).getByRole("button", { name: "Delete" }))
    await waitFor(() => expect(deleteSend).toHaveBeenCalledWith("s1"))
  })

  it("no links yet: an Empty State under the field", async () => {
    mount(fakeSharing({ ...empty, overview: () => Promise.resolve(overview()) }))
    const user = userEvent.setup()
    await openTab(user, "Links")
    expect(await screen.findByText("No links yet.")).toBeInTheDocument()
  })

  it("opens a share link someone sent in the one field: the detail draws plain values, secrets masked until asked", async () => {
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
    const saveOpened = vi.fn(() => Promise.resolve("n1"))
    mount(fakeSharing({ ...empty, overview: () => Promise.resolve(overview()), openSend, openedValue, saveOpened }))
    const user = userEvent.setup()
    await openTab(user, "Links")
    await user.type(await screen.findByLabelText("Paste a share link or a request link"), "https://sync.example/s/abc#key")
    await user.type(screen.getByLabelText("Its password, if it has one"), "pw")
    await user.click(screen.getByRole("button", { name: "Open" }))
    expect(openSend).toHaveBeenCalledWith(Array.from(new TextEncoder().encode("https://sync.example/s/abc#key")), Array.from(new TextEncoder().encode("pw")))
    const panel = await screen.findByRole("region", { name: "Opened: Router" })
    expect(within(panel).getByText("Opened // shared with you")).toBeInTheDocument()
    expect(await within(panel).findByText("admin")).toBeInTheDocument()
    expect(within(panel).queryByText("hunter2")).not.toBeInTheDocument()
    expect(openedValue).not.toHaveBeenCalledWith(1)
    expect(within(panel).getByText("Kept here until you open another link or lock.")).toBeInTheDocument()
    await user.click(within(panel).getByRole("button", { name: "Keep in my vault" }))
    expect(await within(panel).findByText("Kept in your vault.")).toBeInTheDocument()
    expect(within(panel).queryByRole("button", { name: "Keep in my vault" })).not.toBeInTheDocument()
  })

  it("a link that will not open says why under the field, in the core's words", async () => {
    mount(fakeSharing({ ...empty, overview: () => Promise.resolve(overview()), openSend: () => Promise.reject(new Error("This link has expired.")) }))
    const user = userEvent.setup()
    await openTab(user, "Links")
    const field = await screen.findByLabelText("Paste a share link or a request link")
    await user.type(field, "https://sync.example/s/old#k")
    await user.click(screen.getByRole("button", { name: "Open" }))
    await waitFor(() => expect(field).toHaveAccessibleDescription(expect.stringContaining("This link has expired.")))
  })

  it("a request link pasted in the same field opens the answer Form, which sends the secret once", async () => {
    const requestLabel = vi.fn(() => Promise.resolve("The Wi-Fi password"))
    const answerRequest = vi.fn(() => Promise.resolve())
    const openSend = vi.fn()
    mount(fakeSharing({ ...empty, overview: () => Promise.resolve(overview()), requestLabel, answerRequest, openSend }))
    const user = userEvent.setup()
    await openTab(user, "Links")
    await user.type(await screen.findByLabelText("Paste a share link or a request link"), "https://sync.example/r/xyz#pin")
    await user.click(screen.getByRole("button", { name: "Open" }))
    const dialog = await screen.findByRole("dialog", { name: "Answer a request" })
    expect(within(dialog).getByText("They asked for: The Wi-Fi password")).toBeInTheDocument()
    expect(openSend).not.toHaveBeenCalled()
    await user.type(within(dialog).getByLabelText("The secret"), "s3cret")
    await user.click(within(dialog).getByRole("button", { name: "Send it" }))
    await waitFor(() => expect(answerRequest).toHaveBeenCalled())
    const [sentLink, , sentSecret] = (answerRequest.mock.calls as unknown as Array<[number[], string, number[]]>)[0] ?? [[], "", [1]]
    expect(sentLink).toEqual(Array.from(new TextEncoder().encode("https://sync.example/r/xyz#pin")))
    // The typed secret's bytes are wiped once the call is made.
    expect(sentSecret.every((b) => b === 0)).toBe(true)
  })

  it("someone asked for your vaults: needs-you row 4 on Sharing; REVIEW opens the grant, where STOP IT acts at once with no gate", async () => {
    const asked = grant({ state: "requested", release_at: 1_900_000_000 })
    const emergencyStep = vi.fn(() => Promise.resolve([grant({ state: "accepted" })]))
    const gate = vi.fn()
    mount(fakeSharing({ ...empty, overview: () => Promise.resolve(overview({ emergency: [asked], waiting_on_you: 1 })), emergencyStep, letBackIn: gate }))
    const user = userEvent.setup()
    // On People: the block, REVIEW, no text action.
    const block = await screen.findByRole("region", { name: /^Emergency access \/\/ opens/i })
    expect(blocks()).toHaveLength(1)
    expect(within(block).getByText("bob@example.com asked for your vaults.")).toBeInTheDocument()
    expect(within(block).getAllByRole("button")).toHaveLength(1)
    await user.click(within(block).getByRole("button", { name: "Review" }))
    // The Emergency tab, the grant open.
    const detail = await screen.findByRole("region", { name: "Emergency: open" })
    expect(within(detail).getByText(/They open your vaults .* unless you stop it\./)).toBeInTheDocument()
    expect(emergencyStep).not.toHaveBeenCalled()
    // On its own screen the block's solid is STOP IT: one press, no Question, no gate.
    const here = await screen.findByRole("region", { name: /^Emergency access \/\/ opens/i })
    await user.click(await within(here).findByRole("button", { name: "Stop it" }))
    expect(emergencyStep).toHaveBeenCalledWith("g1", "stop")
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument()
    expect(gate).not.toHaveBeenCalled()
    // Stopped: the block leaves.
    await waitFor(() => expect(blocks()).toHaveLength(0))
  })

  it("a grant you named: END IT asks first, and the released case says what to change", async () => {
    const released = grant({ state: "released", history: [{ at: 1_790_000_000, words: "bob@example.com opened your vaults." }] })
    const emergencyStep = vi.fn(() => Promise.resolve([grant({ state: "revoked" })]))
    mount(fakeSharing({ ...empty, overview: () => Promise.resolve(overview({ emergency: [released] })), emergencyStep }))
    const user = userEvent.setup()
    await openTab(user, "Emergency")
    const list = await screen.findByRole("list", { name: "Your emergency contacts" })
    expect(within(list).getByText(/wait 7 days \/\/ 01 vault/)).toBeInTheDocument()
    await user.click(within(list).getByRole("button", { name: /bob@example.com/ }))
    const detail = await screen.findByRole("region", { name: "Emergency: open" })
    expect(within(within(detail).getByRole("list", { name: "What happened" })).getByText("bob@example.com opened your vaults.")).toBeInTheDocument()
    await user.click(within(detail).getByRole("button", { name: "End it" }))
    const question = await screen.findByRole("dialog", { name: "End emergency access for bob@example.com?" })
    expect(within(question).getByText(/They already opened your vaults\. Ending it stops new access/)).toBeInTheDocument()
    expect(emergencyStep).not.toHaveBeenCalled()
    await user.click(within(question).getByRole("button", { name: "End it" }))
    await waitFor(() => expect(emergencyStep).toHaveBeenCalledWith("g1", "revoke"))
  })

  it("someone who trusts you: ACCEPT is the detail's primary, and OPEN THEIR VAULTS once the wait ran out", async () => {
    const invited = grant({ grant_id: "g2", mine: false, other: "alice@example.com", state: "invited" })
    const emergencyStep = vi.fn(() => Promise.resolve([{ ...invited, state: "ready" as const }]))
    const emergencyOpen = vi.fn(() => Promise.resolve([{ ...invited, state: "released" as const }]))
    const notices: string[] = []
    render(<Screen sharing={fakeSharing({ ...empty, overview: () => Promise.resolve(overview({ emergency: [invited] })), emergencyStep, emergencyOpen })} onNotice={(t) => notices.push(t)} />)
    const user = userEvent.setup()
    await openTab(user, "Emergency")
    await user.click(within(await screen.findByRole("list", { name: "People who trust you" })).getByRole("button", { name: /alice@example.com/ }))
    const detail = await screen.findByRole("region", { name: "Emergency: open" })
    await user.click(within(detail).getByRole("button", { name: "Accept" }))
    expect(emergencyStep).toHaveBeenCalledWith("g2", "accept")
    await user.click(await within(detail).findByRole("button", { name: "Open their vaults" }))
    await waitFor(() => expect(emergencyOpen).toHaveBeenCalledWith("g2"))
    expect(notices).toContain("alice@example.com's vaults are open to read, in the sidebar")
  })

  it("an emergency contact must have compared fingerprints first, with OPEN PEOPLE", async () => {
    mount(fakeSharing({ ...empty, overview: () => Promise.resolve(overview()) }))
    const user = userEvent.setup()
    await openTab(user, "Emergency")
    await user.click(await screen.findByRole("button", { name: "Add emergency contact" }))
    const dialog = await screen.findByRole("dialog")
    expect(within(dialog).getByText("Only someone whose fingerprint you compared can be an emergency contact.")).toBeInTheDocument()
    expect(within(dialog).queryByRole("button", { name: "Ask them" })).not.toBeInTheDocument()
    await user.click(within(dialog).getByRole("button", { name: "Open people" }))
    expect(await screen.findByRole("tab", { name: /^People/, selected: true })).toBeInTheDocument()
  })

  it("adds a verified emergency contact through the gate with the wait and vaults picked", async () => {
    const addEmergency = vi.fn(() => Promise.resolve([grant({})]))
    mount(fakeSharing({ ...empty, overview: () => Promise.resolve(overview({ contacts: [{ ...bob, verified: true }] })), addEmergency }))
    const user = userEvent.setup()
    await openTab(user, "Emergency")
    await user.click(await screen.findByRole("button", { name: "Add emergency contact" }))
    const dialog = await screen.findByRole("dialog", { name: "Add emergency contact" })
    await user.selectOptions(within(dialog).getByLabelText("Waiting period"), "14")
    await user.click(within(dialog).getByRole("button", { name: "Ask them" }))
    await waitFor(() => expect(addEmergency).toHaveBeenCalledWith("bob@example.com", 14, ["v1"], {}))
  })
})

describe("emergency steps by state, as the screen offers them (brief S5)", () => {
  const offered = (g: EmergencyView) => grantActions(g).map((a) => `${a.action}:${a.emphasis}`)
  it("your contact: END IT always; STOP IT is the needs-you's while asked, and secondary once ready", () => {
    expect(offered(grant({ state: "invited" }))).toEqual(["revoke:critical"])
    expect(offered(grant({ state: "requested" }))).toEqual(["revoke:critical"])
    expect(offered(grant({ state: "ready" }))).toEqual(["stop:secondary", "revoke:critical"])
    expect(offered(grant({ state: "released" }))).toEqual(["revoke:critical"])
    expect(offered(grant({ state: "revoked" }))).toEqual([])
  })
  it("someone who trusts you: ACCEPT, ASK FOR ACCESS, then OPEN THEIR VAULTS; stepping down stays", () => {
    const c = (state: EmergencyView["state"]) => grant({ mine: false, state })
    expect(offered(c("invited"))).toEqual(["accept:primary", "decline:text"])
    expect(offered(c("accepted"))).toEqual(["request:secondary", "decline:text"])
    expect(offered(c("requested"))).toEqual(["decline:text"])
    expect(offered(c("ready"))).toEqual(["open:primary"])
    expect(offered(c("declined"))).toEqual([])
  })
  it("only ending, asking and stepping down ask first; nothing is gated", () => {
    expect(grantActions(grant({ state: "ready" })).filter((a) => a.asks).map((a) => a.action)).toEqual(["revoke"])
    expect(grantActions(grant({ mine: false, state: "accepted" })).filter((a) => a.asks).map((a) => a.action)).toEqual(["request", "decline"])
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
