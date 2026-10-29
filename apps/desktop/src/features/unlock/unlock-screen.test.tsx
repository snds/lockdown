import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { ClientProvider, type Client } from "@/core/client"
import type { Probe, Status } from "@/core/types"
import { KIT_PAGE, fakeFirstRun, opened } from "@/test/fake-first-run"

import { FirstRunClientProvider, type FirstRunClient } from "./first-run-client"
import { UnlockScreen } from "./unlock-screen"

const SEALED: Probe = {
  root: "/tmp/snds/identity",
  data: "/tmp/snds/lockdown",
  identity_present: true,
  device_key_present: true,
  account_present: true,
  device_name: "Lockdown on test",
  master_password: true,
  local_account: true,
  name: "Sean",
  stage: "done",
}

const PASSWORD = "orbit lantern pickle meadow"
const WRONG = { code: "refused", message: "That isn't the master password for this vault." }

function fakeClient(overrides: Partial<Client>): Client {
  const refuse = () => Promise.reject({ code: "refused", message: "not in this test" })
  const base = new Proxy({} as Client, { get: () => refuse })
  return Object.assign(
    Object.create(base),
    { probe: () => Promise.resolve(SEALED), passwordStrength: () => Promise.resolve(4), status: () => Promise.resolve(opened) },
    overrides,
  )
}

function mount(frc: FirstRunClient, client: Partial<Client> = {}, reason: "idle" | null = null) {
  const onOpened = vi.fn()
  render(
    <ClientProvider value={fakeClient(client)}>
      <FirstRunClientProvider value={frc}>
        <UnlockScreen reason={reason} onOpened={onOpened} />
      </FirstRunClientProvider>
    </ClientProvider>,
  )
  return onOpened
}

describe("the unlock screen (doc 39 §1.10)", () => {
  it("shows the name and the reason, refuses a wrong password in words, and opens with the right one", async () => {
    const unlockWithPassword = vi.fn((p: string) => (p === PASSWORD ? Promise.resolve(opened) : Promise.reject(WRONG)))
    const onOpened = mount(fakeFirstRun({ unlockWithPassword }), {}, "idle")
    const user = userEvent.setup()
    expect(await screen.findByRole("heading", { level: 1, name: "Sean" })).toBeInTheDocument()
    expect(screen.getByText("Locked after a while without you.")).toBeInTheDocument()
    // No server words, ever, on this screen.
    expect(document.body.textContent).not.toMatch(/server|sync/i)
    for (let i = 0; i < 3; i++) {
      await user.clear(screen.getByLabelText("Master password"))
      await user.type(screen.getByLabelText("Master password"), "not it")
      await user.click(screen.getByRole("button", { name: "Unlock" }))
      await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent("That isn't the master password for this vault."))
    }
    expect(screen.getByRole("button", { name: "Still not it? Use your recovery kit" })).toBeInTheDocument()
    await user.clear(screen.getByLabelText("Master password"))
    await user.type(screen.getByLabelText("Master password"), PASSWORD)
    await user.click(screen.getByRole("button", { name: "Unlock" }))
    await waitFor(() => expect(onOpened).toHaveBeenCalledWith(opened))
  })

  it("chooses a new master password with the recovery kit", async () => {
    const resetWithKit = vi
      .fn<FirstRunClient["resetWithKit"]>()
      .mockRejectedValueOnce({ code: "refused", message: "That kit doesn't open this vault. Check each character against the page." })
      .mockResolvedValueOnce(opened)
    const onOpened = mount(fakeFirstRun({ resetWithKit }))
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "Forgot your password? Use your recovery kit" }))
    expect(await screen.findByRole("heading", { level: 1, name: "Use your recovery kit" })).toBeInTheDocument()
    await user.type(screen.getByLabelText("Secret Key"), "A3-WRONG")
    await user.type(screen.getByLabelText("Recovery code"), "RC4F")
    await user.click(screen.getByRole("button", { name: "Check the kit" }))
    expect(await screen.findByRole("heading", { level: 1, name: "Choose a new master password" })).toBeInTheDocument()
    expect(screen.getByText("Your old password stops working once you save the new one.")).toBeInTheDocument()
    await user.type(screen.getByLabelText("Master password"), PASSWORD)
    await user.type(screen.getByLabelText("Type it again"), PASSWORD)
    await user.click(screen.getByRole("button", { name: "Save new password" }))
    // A kit that doesn't open the vault goes back to the kit, in the core's words.
    expect(await screen.findByRole("heading", { level: 1, name: "Use your recovery kit" })).toBeInTheDocument()
    expect(screen.getByRole("alert")).toHaveTextContent("That kit doesn't open this vault.")
    await user.click(screen.getByRole("button", { name: "Check the kit" }))
    await user.type(await screen.findByLabelText("Master password"), PASSWORD)
    await user.type(screen.getByLabelText("Type it again"), PASSWORD)
    await user.click(screen.getByRole("button", { name: "Save new password" }))
    await waitFor(() => expect(onOpened).toHaveBeenCalledWith(opened))
    expect(resetWithKit).toHaveBeenLastCalledWith({ secretKey: "A3-WRONG", recoveryCode: "RC4F", password: PASSWORD })
  })

  it("resumes a first run that stopped before the kit was saved, with a new kit", async () => {
    const kitCreate = vi.fn(() => Promise.resolve({ lines: [], page: new TextEncoder().encode(KIT_PAGE) }))
    mount(fakeFirstRun({ unlockWithPassword: () => Promise.resolve(opened) }), {
      probe: () => Promise.resolve({ ...SEALED, stage: "account_made" }),
      kitCreate,
    })
    const user = userEvent.setup()
    await user.type(await screen.findByLabelText("Master password"), PASSWORD)
    await user.click(screen.getByRole("button", { name: "Unlock" }))
    expect(await screen.findByRole("heading", { level: 1, name: "Save your recovery kit" })).toBeInTheDocument()
    expect(kitCreate).toHaveBeenCalledWith({ replace: true, password: PASSWORD })
  })

  it("moves an install from before the master password to one, with its kit (Q2: no Not now)", async () => {
    const legacy = { ...SEALED, master_password: false, local_account: false, name: null, stage: null }
    const unlock = vi.fn(() => Promise.resolve({ ...opened, account: null, recovery: "created" } as unknown as Status))
    const setMasterPassword = vi.fn(() => Promise.resolve({ lines: [], page: new Uint8Array() }))
    const onOpened = mount(fakeFirstRun({ setMasterPassword }), { probe: () => Promise.resolve(legacy), unlock })
    const user = userEvent.setup()
    expect(await screen.findByRole("heading", { level: 1, name: "Lock your vault with a master password" })).toBeInTheDocument()
    expect(unlock).toHaveBeenCalled()
    expect(screen.queryByRole("button", { name: /not now|skip/i })).not.toBeInTheDocument()
    await user.type(screen.getByLabelText("Your name"), "Sean")
    await user.type(screen.getByLabelText("Master password"), PASSWORD)
    await user.type(screen.getByLabelText("Type it again"), PASSWORD)
    await user.click(screen.getByRole("button", { name: "Continue" }))
    expect(await screen.findByRole("heading", { level: 1, name: "Type your recovery kit" })).toBeInTheDocument()
    await user.type(screen.getByLabelText("Secret Key"), "A3-K7QF2M")
    await user.type(screen.getByLabelText("Recovery code"), "RC4F-9K2M")
    await user.click(screen.getByRole("button", { name: "Continue" }))
    await waitFor(() => expect(onOpened).toHaveBeenCalled())
    expect(setMasterPassword).toHaveBeenCalledWith({ name: "Sean", password: PASSWORD, secretKey: "A3-K7QF2M", recoveryCode: "RC4F-9K2M" })
  })

  it("asks an install with an account on a server for that password once", async () => {
    const legacy = { ...SEALED, master_password: false, local_account: false, name: null, stage: null }
    const unlock = vi.fn(() =>
      Promise.resolve({ ...opened, recovery: "created", account: { server: "https://vault.example", login: "me@example.com", local: false } } as unknown as Status),
    )
    const setMasterPassword = vi.fn(() => Promise.resolve({ lines: [], page: new Uint8Array() }))
    const onOpened = mount(fakeFirstRun({ setMasterPassword }), { probe: () => Promise.resolve(legacy), unlock })
    const user = userEvent.setup()
    expect(await screen.findByRole("heading", { level: 1, name: "Unlock with your password from now on" })).toBeInTheDocument()
    expect(screen.getByText("The password you sign in to vault.example with.")).toBeInTheDocument()
    await user.type(screen.getByLabelText("Master password"), PASSWORD)
    await user.click(screen.getByRole("button", { name: "Continue" }))
    await waitFor(() => expect(onOpened).toHaveBeenCalled())
    expect(setMasterPassword).toHaveBeenCalledWith({ password: PASSWORD })
  })

  it("says the core's words when it can't start", async () => {
    mount(fakeFirstRun(), { probe: () => Promise.reject({ code: "refused", message: "the identity root isn't readable" }) })
    expect(await screen.findByRole("heading", { level: 1, name: "Lockdown can't start" })).toBeInTheDocument()
    expect(screen.getByRole("alert")).toHaveTextContent("the identity root isn't readable")
  })
})
