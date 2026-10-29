import { act, render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { ClientProvider, type Client } from "@/core/client"
import type { Probe } from "@/core/types"
import { PaperClientProvider, type PaperClient } from "@/features/paper/paper-client"
import { KIT_PAGE, fakeFirstRun, opened, readyServer } from "@/test/fake-first-run"

import { setDemoEntry } from "./demo-entry"
import { FirstRunClientProvider, type FirstRunClient } from "./first-run-client"
import { UnlockScreen } from "./unlock-screen"

const NOTHING: Probe = {
  root: "/tmp/snds/identity",
  data: "/tmp/snds/lockdown",
  identity_present: false,
  device_key_present: false,
  account_present: false,
  device_name: "Lockdown on test",
}

function fakeClient(overrides: Partial<Client>): Client {
  const refuse = () => Promise.reject({ code: "refused", message: "not in this test" })
  const base = new Proxy({} as Client, { get: () => refuse })
  return Object.assign(
    Object.create(base),
    {
      probe: () => Promise.resolve(NOTHING),
      passwordStrength: (candidate: number[]) => Promise.resolve(candidate.length > 12 ? 4 : 1),
      status: () => Promise.resolve(opened),
      importFormats: () =>
        Promise.resolve([
          { id: "1pux", name: "1Password (.1pux)", how: "In 1Password: File, Export, and pick the 1PUX format.", may_need_password: false },
          { id: "otpauth", name: "A list of otpauth:// links", how: "One per line.", may_need_password: false },
        ]),
    },
    overrides,
  )
}

function mount(frc: FirstRunClient, client: Partial<Client> = {}, paper?: PaperClient, onOpened = vi.fn()) {
  render(
    <ClientProvider value={fakeClient(client)}>
      <PaperClientProvider value={paper ?? { print: () => Promise.resolve("done") }}>
        <FirstRunClientProvider value={frc}>
          <UnlockScreen onOpened={onOpened} />
        </FirstRunClientProvider>
      </PaperClientProvider>
    </ClientProvider>,
  )
  return onOpened
}

const PASSWORD = "orbit lantern pickle meadow"

describe("the first run (doc 39 §1.9)", () => {
  it("walks Welcome, the master password, the kit, passwords and the proof, with no server words", async () => {
    const create = vi.fn(() => Promise.resolve({ lines: ["Your vault is ready."], page: new TextEncoder().encode(KIT_PAGE) }))
    const stage = vi.fn(() => Promise.resolve())
    const saveKitPdf = vi.fn(() => Promise.resolve("saved" as const))
    const frc = fakeFirstRun({ create, stage })
    const onOpened = mount(frc, {}, { print: () => Promise.resolve("done"), saveKitPdf })
    const user = userEvent.setup()
    const noServerWords = () => expect(document.body.textContent ?? "").not.toMatch(/server|sync/i)

    // Welcome.
    expect(await screen.findByRole("heading", { level: 1, name: "Every way you sign in. Locked down, even from us." })).toBeInTheDocument()
    expect(screen.getByText("Lockdown keeps your passwords, passkeys and one-time codes in one vault on this computer.", { exact: false })).toBeInTheDocument()
    noServerWords()
    await user.click(screen.getByRole("button", { name: "Create a new vault" }))

    // Your name and master password: the expectations, plainly.
    expect(await screen.findByRole("heading", { level: 1, name: "Choose your master password" })).toBeInTheDocument()
    expect(screen.getByText("Step 1 of 3")).toBeInTheDocument()
    expect(screen.getByText(/the one password to remember: it opens your vault, and Lockdown can't see it or reset it/)).toBeInTheDocument()
    expect(screen.getByText(/A long phrase beats a clever word/)).toBeInTheDocument()
    expect(screen.getByText(/only your recovery kit gets you back in/)).toBeInTheDocument()
    noServerWords()
    await user.click(screen.getByRole("button", { name: "Learn more" }))
    const learn = await screen.findByRole("dialog")
    expect(learn).toHaveTextContent("Your master password")
    expect(learn).toHaveTextContent("nobody, Lockdown included, can tell you what it was or reset it")
    await user.click(within(learn).getByRole("button", { name: "Got it" }))

    // Checked before anything is made.
    await user.click(screen.getByRole("button", { name: "Continue" }))
    expect(await screen.findByText("Type a name. It's what the lock screen shows.")).toBeInTheDocument()
    await user.type(screen.getByLabelText("Your name"), "Sean")
    await user.type(screen.getByLabelText("Master password"), "short")
    expect(await screen.findByText("Strength: Weak")).toBeInTheDocument()
    await user.type(screen.getByLabelText("Type it again"), "short")
    await user.click(screen.getByRole("button", { name: "Continue" }))
    expect(await screen.findByText(/needs to be at least fair/)).toBeInTheDocument()
    await user.clear(screen.getByLabelText("Master password"))
    await user.type(screen.getByLabelText("Master password"), PASSWORD)
    expect(await screen.findByText("Strength: Strong")).toBeInTheDocument()
    await user.clear(screen.getByLabelText("Type it again"))
    await user.type(screen.getByLabelText("Type it again"), "orbit lantern")
    await user.click(screen.getByRole("button", { name: "Continue" }))
    expect(await screen.findByText("The two passwords don't match yet.")).toBeInTheDocument()
    expect(create).not.toHaveBeenCalled()
    await user.type(screen.getByLabelText("Type it again"), " pickle meadow")
    await user.click(screen.getByRole("button", { name: "Continue" }))
    await waitFor(() => expect(create).toHaveBeenCalledWith("Sean", PASSWORD))

    // The kit: saved, and Continue only once the person says so.
    expect(await screen.findByRole("heading", { level: 1, name: "Save your recovery kit" })).toBeInTheDocument()
    expect(screen.getByText("Step 2 of 3")).toBeInTheDocument()
    expect(screen.getByRole("heading", { level: 2, name: "Recovery kit for Sean" })).toBeInTheDocument()
    expect(screen.getByText(/such as a drawer at home or with your important papers/)).toBeInTheDocument()
    expect(screen.getByRole("button", { name: "Continue" })).toBeDisabled()
    await user.click(screen.getByRole("button", { name: "Save as PDF" }))
    expect(saveKitPdf).toHaveBeenCalledWith(["Your vault is ready."], KIT_PAGE)
    expect(await screen.findByText(/A PDF on this computer alone won't help if this computer is lost/)).toBeInTheDocument()
    noServerWords()
    await user.click(screen.getByRole("checkbox", { name: "I saved, printed or wrote down the kit" }))
    await user.click(screen.getByRole("button", { name: "Continue" }))
    expect(stage).toHaveBeenCalledWith("kit_saved")

    // Bring your passwords: only importers that exist, and a way to skip.
    expect(await screen.findByRole("heading", { level: 1, name: "Bring your passwords" })).toBeInTheDocument()
    expect(screen.getByText("Step 3 of 3")).toBeInTheDocument()
    expect(await screen.findByText("1Password (.1pux)")).toBeInTheDocument()
    expect(screen.queryByText("A list of otpauth:// links")).not.toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: "Skip for now" }))

    // The proof moment, once (D95), then the vault.
    expect(await screen.findByRole("heading", { level: 1, name: "Locked, even from us" })).toBeInTheDocument()
    expect(screen.getByText("Your master password never leaves this computer.")).toBeInTheDocument()
    noServerWords()
    await user.click(screen.getByRole("button", { name: "Open the vault" }))
    await waitFor(() => expect(onOpened).toHaveBeenCalledWith(opened))
    expect(stage).toHaveBeenCalledWith("done")
  })

  it("puts the core's floor under the password field", async () => {
    const create = vi.fn(() =>
      Promise.reject({ code: "refused", message: "This password protects everything else, so it needs to be at least fair. Add another word or two." }),
    )
    mount(fakeFirstRun({ create }))
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "Create a new vault" }))
    await user.type(screen.getByLabelText("Your name"), "Sean")
    await user.type(screen.getByLabelText("Master password"), PASSWORD)
    await user.type(screen.getByLabelText("Type it again"), PASSWORD)
    await user.click(screen.getByRole("button", { name: "Continue" }))
    expect(await screen.findByRole("alert")).toHaveTextContent("at least fair")
    expect(screen.getByRole("heading", { level: 1, name: "Choose your master password" })).toBeInTheDocument()
  })

  it("says what discovery found on Welcome, and the demo only while one is installed", async () => {
    mount(fakeFirstRun({ candidates: (network) => Promise.resolve(network ? [readyServer("https://nas.local")] : []) }))
    expect(await screen.findByRole("button", { name: "We found a Lockdown server on your network: nas.local" })).toBeInTheDocument()
    expect(screen.queryByRole("button", { name: "Explore the demo" })).not.toBeInTheDocument()
    const explore = vi.fn()
    act(() => setDemoEntry({ explore, busy: false }))
    await userEvent.setup().click(await screen.findByRole("button", { name: "Explore the demo" }))
    expect(explore).toHaveBeenCalled()
    act(() => setDemoEntry(null))
    expect(screen.queryByRole("button", { name: "Explore the demo" })).not.toBeInTheDocument()
  })
})
