import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import type { ReactNode } from "react"

import { ClientProvider, type Client } from "@/core/client"
import type { Probe } from "@/core/types"
import { FirstRunClientProvider, type FirstRunClient } from "@/features/unlock/first-run-client"
import { UnlockScreen } from "@/features/unlock/unlock-screen"
import { fakeFirstRun, readyServer } from "@/test/fake-first-run"
import { fakeClient } from "@/test/fake-client"
import { fakeDepth } from "@/test/fake-depth"
import { DepthClientProvider, type DepthClient } from "./depth-client"
import { JoinWithDevice } from "./join-device"

function mount(depth: DepthClient, ui: ReactNode) {
  return render(<DepthClientProvider value={depth}>{ui}</DepthClientProvider>)
}

const SLOW = { timeout: 4000 }

/** Discovery finds the server; the join is sealed by the core in one step. */
function joinFirstRun(over: Partial<FirstRunClient> = {}) {
  return fakeFirstRun({
    candidates: () => Promise.resolve([readyServer("https://vault.example")]),
    ...over,
  })
}

const EMPTY: Probe = { root: "/r", data: "/d", identity_present: false, device_key_present: false, account_present: false, device_name: "test" } as Probe

function mountJoin(frc: FirstRunClient, depth: Partial<DepthClient> = {}, client: Partial<Client> = {}) {
  const onOpened = vi.fn()
  render(
    <ClientProvider value={fakeClient({ probe: () => Promise.resolve(EMPTY), ...client })}>
      <DepthClientProvider value={fakeDepth(depth)}>
        <FirstRunClientProvider value={frc}>
          <UnlockScreen onOpened={onOpened} />
        </FirstRunClientProvider>
      </DepthClientProvider>
    </ClientProvider>,
  )
  return onOpened
}

async function signIn(login: string, way: RegExp = /Sign in with your login and password/) {
  const user = userEvent.setup()
  await user.click(await screen.findByRole("button", { name: "I already use Lockdown" }))
  await user.click(await screen.findByRole("radio", { name: way }))
  await user.click(await screen.findByRole("radio", { name: /vault\.example/ }))
  await user.type(screen.getByLabelText("Login"), login)
  await user.type(screen.getByLabelText("Master password"), "a long master password")
  return user
}

describe("joining from another device", () => {
  it("refuses a malformed code, then joins and finishes with the password", async () => {
    const joinStart = vi.fn(() => Promise.resolve({ state: "approved", server: "https://vault.example", secret_key: "sent", words: "Approved. Sign in to finish." }))
    const joinFinish = vi.fn(() => Promise.resolve({ devices: 2 } as never))
    const onOpened = vi.fn()
    mount(fakeDepth({ joinStart, joinFinish }), <JoinWithDevice onOpened={onOpened} />)
    const user = userEvent.setup()
    await user.type(screen.getByLabelText("Server"), "https://vault.example")
    await user.type(screen.getByLabelText("Code from the other device"), "7KQ2")
    await user.click(screen.getByRole("button", { name: "Join" }))
    expect(await screen.findByRole("alert")).toHaveTextContent("ten letters and digits")
    await user.clear(screen.getByLabelText("Code from the other device"))
    await user.type(screen.getByLabelText("Code from the other device"), "7kq2m9xh4p")
    await user.click(screen.getByRole("button", { name: "Join" }))
    expect(joinStart).toHaveBeenCalledWith("https://vault.example", "7KQ2M-9XH4P")
    await user.type(await screen.findByLabelText("Login name"), "me@example.com")
    await user.type(screen.getByLabelText("Master password"), "a long password")
    await user.click(screen.getByRole("button", { name: "Finish" }))
    await waitFor(() => expect(onOpened).toHaveBeenCalled())
    expect(joinFinish).toHaveBeenCalledWith("me@example.com", "a long password")
  })
})

describe("screen J's sealed join (doc 45)", () => {
  it("signs in and opens with the same password in one step, with no separate master password step", async () => {
    const accountLoginSealed = vi.fn(() => Promise.resolve(["enrolled"]))
    const openJoined = vi.fn(() => Promise.resolve({ devices: 2 } as never))
    const setMasterPassword = vi.fn(() => Promise.resolve({ lines: [], page: new Uint8Array() }))
    const openVault = vi.fn(() => Promise.resolve({ devices: 2 } as never))
    const accountLogin = vi.fn(() => Promise.resolve(["plain"]))
    const onOpened = mountJoin(joinFirstRun({ accountLoginSealed, openJoined, setMasterPassword }), {}, { openVault, accountLogin })
    const user = await signIn("me@example.com")
    await user.type(screen.getByLabelText("Secret Key"), "A1-KEY")
    await user.click(screen.getByRole("button", { name: "Continue" }))
    await user.click(await screen.findByRole("button", { name: "Connect" }))
    await waitFor(() => expect(onOpened).toHaveBeenCalledWith({ devices: 2 }))
    expect(accountLoginSealed).toHaveBeenCalledWith({ server: "https://vault.example", login: "me@example.com", password: "a long master password", secretKey: "A1-KEY" })
    expect(openJoined).toHaveBeenCalledWith("a long master password")
    // The two-step path is gone: no plain sign-in, no create, no seal after.
    expect(accountLogin).not.toHaveBeenCalled()
    expect(openVault).not.toHaveBeenCalled()
    expect(setMasterPassword).not.toHaveBeenCalled()
  })

  it("recovers with the kit, sealed, and shows a refusal in the core's words without opening", async () => {
    const recoverSealed = vi
      .fn()
      .mockRejectedValueOnce({ code: "refused", message: "the recovery code didn't open the identity" })
      .mockResolvedValueOnce(["recovered"])
    const openJoined = vi.fn(() => Promise.resolve({ devices: 2 } as never))
    const onOpened = mountJoin(joinFirstRun({ recoverSealed, openJoined }))
    const user = await signIn("me@example.com", /Use your recovery kit/)
    await user.type(screen.getByLabelText("Secret Key"), "A1-KEY")
    await user.type(screen.getByLabelText("Recovery code"), "RC-CODE")
    await user.click(screen.getByRole("button", { name: "Continue" }))
    await user.click(await screen.findByRole("button", { name: "Connect" }))
    expect(await screen.findByRole("alert")).toHaveTextContent("the recovery code didn't open the identity")
    expect(openJoined).not.toHaveBeenCalled()
    await user.click(screen.getByRole("button", { name: "Continue" }))
    await user.click(await screen.findByRole("button", { name: "Connect" }))
    await waitFor(() => expect(onOpened).toHaveBeenCalled())
    expect(recoverSealed).toHaveBeenLastCalledWith({
      server: "https://vault.example",
      login: "me@example.com",
      password: "a long master password",
      secretKey: "A1-KEY",
      recoveryCode: "RC-CODE",
    })
    expect(openJoined).toHaveBeenCalledWith("a long master password")
  })

  it("comes back to screen J after a join that stopped with its keys sealed", async () => {
    mountJoin(joinFirstRun(), {}, { probe: () => Promise.resolve({ ...EMPTY, device_key_present: true }) })
    expect(await screen.findByRole("heading", { name: "Add this computer to your Lockdown" })).toBeInTheDocument()
    expect(screen.queryByText(/isn't enrolled yet/)).not.toBeInTheDocument()
  })
})

describe("a sign-in that waits for a security key", () => {
  it("opens the server's page and finishes once the key answered, sealed with the same password", async () => {
    const accountLoginSealed = vi.fn(() => Promise.reject({ code: "approval_needed", message: "use your key", detail: "security_key" }))
    const openJoined = vi.fn(() => Promise.resolve({ devices: 2 } as never))
    const setMasterPassword = vi.fn(() => Promise.resolve({ lines: [], page: new Uint8Array() }))
    const openVault = vi.fn(() => Promise.resolve({ devices: 2 } as never))
    const secondFactorOpen = vi.fn(() => Promise.resolve())
    const secondFactorWait = vi.fn(() => Promise.resolve(["enrolled"]))
    const onOpened = mountJoin(joinFirstRun({ accountLoginSealed, openJoined, setMasterPassword }), { secondFactorOpen, secondFactorWait }, { openVault })
    const user = await signIn("me@example.com")
    await user.click(screen.getByRole("button", { name: "Continue" }))
    await user.click(await screen.findByRole("button", { name: "Connect" }))
    expect(await screen.findByText(/asks for a security key/)).toBeInTheDocument()
    expect(secondFactorOpen).toHaveBeenCalledWith("https://vault.example")
    await waitFor(() => expect(onOpened).toHaveBeenCalled(), SLOW)
    expect(openJoined).toHaveBeenCalledWith("a long master password")
    expect(openVault).not.toHaveBeenCalled()
    expect(setMasterPassword).not.toHaveBeenCalled()
  })

  it("waits for the emailed approval link without opening a page", async () => {
    const accountLoginSealed = vi.fn(() =>
      Promise.reject({
        code: "approval_needed",
        message: "the server emailed a link to s•••@example.com: open it and choose Approve, and this device carries on by itself",
        detail: "email",
      }),
    )
    const openJoined = vi.fn(() => Promise.resolve({ devices: 2 } as never))
    const secondFactorOpen = vi.fn(() => Promise.resolve())
    const secondFactorWait = vi.fn(() => Promise.resolve(["enrolled"]))
    const onOpened = mountJoin(joinFirstRun({ accountLoginSealed, openJoined }), { secondFactorOpen, secondFactorWait })
    const user = await signIn("sean@example.com")
    await user.click(screen.getByRole("button", { name: "Continue" }))
    await user.click(await screen.findByRole("button", { name: "Connect" }))
    expect(await screen.findByText(/^The server emailed a link to s•••@example\.com/)).toBeInTheDocument()
    expect(screen.queryByRole("button", { name: "Open the page again" })).not.toBeInTheDocument()
    await waitFor(() => expect(onOpened).toHaveBeenCalled(), SLOW)
    expect(secondFactorOpen).not.toHaveBeenCalled()
    expect(openJoined).toHaveBeenCalledWith("a long master password")
  })

  it("a cancelled wait opens nothing", async () => {
    const accountLoginSealed = vi.fn(() => Promise.reject({ code: "approval_needed", message: "use your key", detail: "security_key" }))
    const openJoined = vi.fn(() => Promise.resolve({ devices: 2 } as never))
    const secondFactorWait = vi.fn(() => Promise.resolve(null))
    const onOpened = mountJoin(joinFirstRun({ accountLoginSealed, openJoined }), { secondFactorOpen: () => Promise.resolve(), secondFactorWait })
    const user = await signIn("me@example.com")
    await user.click(screen.getByRole("button", { name: "Continue" }))
    await user.click(await screen.findByRole("button", { name: "Connect" }))
    await user.click(await screen.findByRole("button", { name: "Cancel" }))
    expect(await screen.findByRole("heading", { name: "Add this computer to your Lockdown" })).toBeInTheDocument()
    expect(openJoined).not.toHaveBeenCalled()
    expect(onOpened).not.toHaveBeenCalled()
  })
})
