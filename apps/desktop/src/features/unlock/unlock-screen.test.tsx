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

function mount(frc: FirstRunClient, client: Partial<Client> = {}, reason: "idle" | "hidden" | null = null, triesLocked = false) {
  const onOpened = vi.fn()
  render(
    <ClientProvider value={fakeClient(client)}>
      <FirstRunClientProvider value={frc}>
        <UnlockScreen reason={reason} onOpened={onOpened} triesLocked={triesLocked} />
      </FirstRunClientProvider>
    </ClientProvider>,
  )
  return onOpened
}

const IDLE = { locked: true, reason: "idle" as const, idle_secs: 300, idle_timeout_secs: 300, hidden_timeout_secs: null }

describe("the unlock screen (doc 39 §1.10)", () => {
  it("shows the reason and the name in the overline, refuses a wrong password in words, and opens with the right one", async () => {
    const unlockWithPassword = vi.fn((p: string) => (p === PASSWORD ? Promise.resolve(opened) : Promise.reject(WRONG)))
    const onOpened = mount(fakeFirstRun({ unlockWithPassword }), { lockState: () => Promise.resolve(IDLE) }, "idle")
    const user = userEvent.setup()
    // Brief F1 and Q15: the frame's title, the person's name in the small line above, the timer that fired.
    expect(await screen.findByRole("heading", { level: 1, name: "Lockdown is locked." })).toBeInTheDocument()
    expect(await screen.findByText("Locked // Idle 05:00 // Sean")).toBeInTheDocument()
    // Focus is in the field.
    expect(screen.getByLabelText("Master password")).toHaveFocus()
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

  it("says the core's words when it can't start, and tries again", async () => {
    const probe = vi.fn<Client["probe"]>().mockRejectedValueOnce({ code: "refused", message: "the identity root isn't readable" }).mockResolvedValue(SEALED)
    mount(fakeFirstRun(), { probe })
    expect(await screen.findByRole("heading", { level: 1, name: "Lockdown can't start." })).toBeInTheDocument()
    expect(screen.getByRole("alert")).toHaveTextContent("the identity root isn't readable")
    await userEvent.click(screen.getByRole("button", { name: "Try again" }))
    expect(await screen.findByRole("heading", { level: 1, name: "Lockdown is locked." })).toBeInTheDocument()
    expect(probe).toHaveBeenCalledTimes(2)
  })

  it("draws the lockup and a skeleton bar while the core starts, and nothing else", async () => {
    mount(fakeFirstRun(), { probe: () => new Promise(() => {}) })
    expect(screen.getByRole("heading", { level: 1, name: "Lockdown is starting" })).toBeInTheDocument()
    expect(document.querySelector('[data-slot="app-lockup"]')).not.toBeNull()
    expect(document.querySelectorAll('[data-slot="skeleton"]')).toHaveLength(1)
    expect(screen.queryByRole("textbox")).toBeNull()
    expect(screen.queryByRole("button")).toBeNull()
  })

  it("explains a device the identity doesn't list, with no action", async () => {
    mount(fakeFirstRun(), { probe: () => Promise.resolve({ ...SEALED, device_key_present: false }) })
    expect(await screen.findByRole("heading", { level: 1, name: "This device isn't enrolled yet." })).toBeInTheDocument()
    expect(screen.getByText(SEALED.root)).toBeInTheDocument()
    expect(screen.queryByRole("button")).toBeNull()
  })

  it("keeps Unlock's label while it works (cross-app decisions §9)", async () => {
    let finish: (s: Status) => void = () => {}
    const unlockWithPassword = vi.fn(() => new Promise<Status>((resolve) => (finish = resolve)))
    const onOpened = mount(fakeFirstRun({ unlockWithPassword }))
    const user = userEvent.setup()
    await user.type(await screen.findByLabelText("Master password"), PASSWORD)
    await user.click(screen.getByRole("button", { name: "Unlock" }))
    const busy = screen.getByRole("button", { name: /Unlock$/ })
    expect(busy).toHaveAttribute("aria-busy", "true")
    expect(busy).toHaveTextContent("Unlock")
    finish(opened)
    await waitFor(() => expect(onOpened).toHaveBeenCalledWith(opened))
  })

  it("asks for the password in words when Unlock is pressed with an empty field", async () => {
    const unlockWithPassword = vi.fn()
    mount(fakeFirstRun({ unlockWithPassword }))
    await userEvent.click(await screen.findByRole("button", { name: "Unlock" }))
    expect(screen.getByRole("alert")).toHaveTextContent("Type your master password.")
    expect(unlockWithPassword).not.toHaveBeenCalled()
  })

  it("says Caps Lock is on in a compact Banner under the field", async () => {
    mount(fakeFirstRun())
    const field = await screen.findByLabelText("Master password")
    await userEvent.click(field)
    await userEvent.keyboard("{CapsLock}a")
    expect(await screen.findByText("Caps Lock is on.")).toBeInTheDocument()
    expect(document.querySelector('[data-slot="banner"]')).not.toBeNull()
  })

  it("offers Touch ID only where this build has quick unlock, and then never as the primary", async () => {
    // Not offered on this build (the fake's default: it needs the signed app): no biometric button at all.
    mount(fakeFirstRun())
    await screen.findByLabelText("Master password")
    await waitFor(() => expect(screen.queryByRole("button", { name: /Touch ID|Windows Hello/ })).toBeNull())
  })

  it("shows quick unlock that isn't ready as disabled, with the core's reason as a visible line", async () => {
    mount(fakeFirstRun({ quickUnlock: () => Promise.resolve({ kind: "windows_hello", available: true, why: "Windows Hello unlock needs the signed Lockdown app." }) }))
    const hello = await screen.findByRole("button", { name: "Unlock with Windows Hello" })
    expect(hello).toHaveAttribute("aria-disabled", "true")
    expect(hello).toHaveAccessibleDescription("Windows Hello unlock needs the signed Lockdown app.")
    expect(screen.getByText("Windows Hello unlock needs the signed Lockdown app.")).toBeVisible()
    // Unlock stays the working primary.
    expect(screen.getByRole("button", { name: "Unlock" })).toHaveAttribute("data-variant", "default")
  })

  it("says the gate locked it after three wrong passwords, in place of the usual sentence", async () => {
    mount(fakeFirstRun(), {}, null, true)
    expect(await screen.findByText("The password was wrong three times, so the vault locked.")).toBeInTheDocument()
    expect(screen.queryByText(/Your master password opens it/)).toBeNull()
  })

  it("says why it locked without a timer it doesn't know", async () => {
    mount(fakeFirstRun(), {}, "hidden")
    expect(await screen.findByText("Locked // Out of view // Sean")).toBeInTheDocument()
  })

  // Replaces the wash's test (brief §4, Q15): behind Locked nothing readable is drawn, and nothing of the vault is read.
  it("renders nothing readable behind Locked", async () => {
    const asked: string[] = []
    const client = fakeClient({})
    const watched = new Proxy(client, {
      get(target, key, receiver) {
        if (typeof key === "string") asked.push(key)
        return Reflect.get(target, key, receiver)
      },
    })
    render(
      <ClientProvider value={watched}>
        <FirstRunClientProvider value={fakeFirstRun()}>
          <UnlockScreen reason="idle" onOpened={vi.fn()} />
        </FirstRunClientProvider>
      </ClientProvider>,
    )
    await screen.findByRole("heading", { level: 1, name: "Lockdown is locked." })
    // The lock screen asks the core what is on this computer and why it locked, and nothing about the vault.
    expect(new Set(asked)).toEqual(new Set(["probe", "lockState"]))
    // No list, no navigation, no rows or their skeletons, no item names: the frame and the form alone.
    expect(screen.queryByRole("navigation")).toBeNull()
    expect(screen.queryByRole("list")).toBeNull()
    expect(document.querySelector('[data-slot="skeleton"], [data-slot="list-row"], [data-slot="locked-wash"], [data-slot="app-frame-sidebar"]')).toBeNull()
    expect(screen.getAllByRole("heading")).toHaveLength(1)
    // The only words are the frame's own.
    const words = document.body.textContent ?? ""
    expect(words).toContain("Lockdown is locked.")
    expect(words).not.toMatch(/Personal|Family|vault names/)
  })
})
