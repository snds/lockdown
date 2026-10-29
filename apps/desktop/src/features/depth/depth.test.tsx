import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import type { ReactNode } from "react"

import { ClientProvider } from "@/core/client"
import { FirstRunClientProvider } from "@/features/unlock/first-run-client"
import { UnlockScreen } from "@/features/unlock/unlock-screen"
import { fakeFirstRun, readyServer } from "@/test/fake-first-run"
import { fakeClient } from "@/test/fake-client"
import { fakeDepth } from "@/test/fake-depth"

import { AddDevice } from "./add-device"
import { DepthClientProvider, isLinkQr, linkCode, phraseDigits, promptFor, type DepthClient, type ExposureView, type LinkView, type PhraseCheckView, type RemindersView } from "./depth-client"
import { ItemProtection } from "./item-protection"
import { JoinWithDevice } from "./join-device"
import { PhraseInvite } from "./key-check"
import { QrCode } from "./qr-code"
import { RemindersPrompt } from "./reminders"

function mount(depth: DepthClient, ui: ReactNode) {
  return render(<DepthClientProvider value={depth}>{ui}</DepthClientProvider>)
}

const SLOW = { timeout: 4000 }

const waiting: LinkView = { state: "waiting", code: "7KQ2M-9XH4P", qr: "lockdown:link:v1?c=7KQ2M9XH4P", qr_rows: ["111", "101", "111"], expires_at: 1 }

const reminders = (over: { password?: Partial<RemindersView["password"]>; drill?: Partial<RemindersView["drill"]> }): RemindersView => ({
  password: { applies: true, due: false, last_at: 1, every_days: 14, next_at: 2, ...over.password },
  drill: { kit: "drilled", last_drilled_at: 1, next_at: 2, due: false, kit_test_pending: false, ...over.drill },
})

describe("the security-depth helpers", () => {
  it("reads codes and phrases the way the core does", () => {
    expect(linkCode("7kq2m 9xh4p")).toBe("7KQ2M-9XH4P")
    expect(linkCode("OIL23-45678")).toBe("01123-45678")
    expect(linkCode("7KQ2M")).toBeNull()
    expect(linkCode("UUUUU-UUUUU")).toBeNull()
    expect(phraseDigits("123 456-789")).toBe("123456789")
    expect(phraseDigits("12345678")).toBeNull()
    expect(isLinkQr(" lockdown:link:v1?c=X")).toBe(true)
    expect(isLinkQr("https://example.com")).toBe(false)
  })

  it("asks the kit test first, then the password, then the drill", () => {
    expect(promptFor(null)).toBeNull()
    expect(promptFor(reminders({}))).toBeNull()
    expect(promptFor(reminders({ drill: { kit_test_pending: true, due: true }, password: { due: true } }))).toBe("kit-test")
    expect(promptFor(reminders({ drill: { due: true }, password: { due: true } }))).toBe("password")
    expect(promptFor(reminders({ drill: { due: true }, password: { due: true, applies: false } }))).toBe("drill")
    expect(promptFor(reminders({ drill: { due: true, kit: "none" } }))).toBeNull()
  })

  it("draws a QR code from the core's rows, dark modules only", () => {
    const { container } = render(<QrCode rows={["101", "010", "101"]} label="code" />)
    const d = container.querySelector("path")?.getAttribute("d") ?? ""
    expect(d.match(/M/g)).toHaveLength(5)
    expect(screen.getByRole("img", { name: "code" })).toBeInTheDocument()
  })
})

describe("adding a device", () => {
  it("shows the code, then the digits, and approves through the gate", async () => {
    const linkPoll = vi.fn(() => Promise.resolve<LinkView>({ ...waiting, state: "compare", sas: "123 456", device_name: "Phone" }))
    const linkApprove = vi.fn(() => Promise.resolve(["Phone is now one of your devices."]))
    const onNotice = vi.fn()
    const onChanged = vi.fn()
    mount(fakeDepth({ linkOffer: () => Promise.resolve(waiting), linkPoll, linkApprove }), <AddDevice onNotice={onNotice} onChanged={onChanged} />)
    const user = userEvent.setup()
    await user.click(screen.getByRole("button", { name: "Add a device" }))
    expect(await screen.findByLabelText("Device code")).toHaveTextContent("7KQ2M-9XH4P")
    expect(screen.getByRole("img", { name: "Device link QR code" })).toBeInTheDocument()
    expect(await screen.findByLabelText("Digits to compare", {}, SLOW)).toHaveTextContent("123 456")
    await user.click(screen.getByRole("button", { name: "Approve" }))
    await waitFor(() => expect(onNotice).toHaveBeenCalledWith("Phone is now one of your devices."))
    expect(linkApprove).toHaveBeenCalledWith({})
    expect(onChanged).toHaveBeenCalled()
  })

  it("declines when the digits don't match", async () => {
    const linkDecline = vi.fn(() => Promise.resolve())
    mount(
      fakeDepth({ linkOffer: () => Promise.resolve(waiting), linkPoll: () => Promise.resolve<LinkView>({ ...waiting, state: "compare", sas: "111 111" }), linkDecline }),
      <AddDevice onNotice={() => {}} onChanged={() => {}} />,
    )
    const user = userEvent.setup()
    await user.click(screen.getByRole("button", { name: "Add a device" }))
    await screen.findByLabelText("Digits to compare", {}, SLOW)
    await user.click(screen.getByRole("button", { name: "They don't match" }))
    expect(linkDecline).toHaveBeenCalled()
  })
})

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

describe("an item's protection", () => {
  const shop: ExposureView = { id: "i1", tier: "fill", source: "default", default_tier: "fill", reason: null, choices: ["app-only", "ask", "fill"] }

  it("keeps an item to the app, and lets a crown jewel into emergency access through the gate", async () => {
    const setExposure = vi.fn(() => Promise.resolve<ExposureView>({ ...shop, tier: "app-only", source: "chosen" }))
    const setLegacyOptIn = vi.fn(() => Promise.resolve(3))
    const onNotice = vi.fn()
    mount(
      fakeDepth({ exposure: () => Promise.resolve(shop), legacyOptIn: () => Promise.resolve(false), setExposure, setLegacyOptIn }),
      <ItemProtection itemId="i1" crownJewel onNotice={onNotice} />,
    )
    const user = userEvent.setup()
    await user.selectOptions(await screen.findByLabelText("Who can get its values"), "app-only")
    await waitFor(() => expect(setExposure).toHaveBeenCalledWith("i1", "app-only", {}))
    expect(onNotice).toHaveBeenCalledWith(expect.stringContaining("Keep to the app"))
    await user.click(screen.getByRole("switch", { name: "Include in emergency access" }))
    await waitFor(() => expect(setLegacyOptIn).toHaveBeenCalledWith("i1", true, {}))
  })

  it("going back to the default clears the choice", async () => {
    const setExposure = vi.fn(() => Promise.resolve(shop))
    mount(
      fakeDepth({ exposure: () => Promise.resolve<ExposureView>({ ...shop, tier: "ask", source: "chosen" }), setExposure }),
      <ItemProtection itemId="i1" crownJewel={false} onNotice={() => {}} />,
    )
    const user = userEvent.setup()
    await user.selectOptions(await screen.findByLabelText("Who can get its values"), "fill")
    await waitFor(() => expect(setExposure).toHaveBeenCalledWith("i1", null, {}))
    expect(screen.queryByLabelText("Include in emergency access")).not.toBeInTheDocument()
  })
})

describe("the reminders", () => {
  it("skips the first-run kit test only with D251's words on the button", async () => {
    const kitTestSkip = vi.fn((understood: boolean) =>
      understood ? Promise.resolve(reminders({})) : Promise.reject({ code: "refused", message: "confirm", detail: "confirm_skip" }),
    )
    mount(fakeDepth({ reminders: () => Promise.resolve(reminders({ drill: { kit: "created", last_drilled_at: null, kit_test_pending: true, due: true } })), kitTestSkip }), <RemindersPrompt onNotice={() => {}} onChanged={() => {}} />)
    const user = userEvent.setup()
    expect(await screen.findByRole("heading", { name: "Test your recovery kit" })).toBeInTheDocument()
    expect(screen.queryByRole("button", { name: "Skip for now" })).not.toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: "I understand an untested kit can fail" }))
    await waitFor(() => expect(kitTestSkip).toHaveBeenCalledWith(true))
    expect(kitTestSkip).toHaveBeenCalledTimes(1)
    await waitFor(() => expect(screen.queryByRole("heading", { name: "Test your recovery kit" })).not.toBeInTheDocument())
  })

  it("asks for the master password when it's due and checks it", async () => {
    const passwordCheck = vi.fn(() => Promise.resolve(reminders({})))
    mount(fakeDepth({ reminders: () => Promise.resolve(reminders({ password: { due: true } })), passwordCheck }), <RemindersPrompt onNotice={() => {}} onChanged={() => {}} />)
    const user = userEvent.setup()
    await user.type(await screen.findByLabelText("Master password"), "a long password")
    await user.click(screen.getByRole("button", { name: "Check" }))
    await waitFor(() => expect(passwordCheck).toHaveBeenCalledWith("a long password"))
  })
})

describe("checking keys with a phrase", () => {
  it("shows the phrase to say and reports the match", async () => {
    const open: PhraseCheckView = { verify_id: "v1", role: "inviter", login: "bob@example.com", account_id: "a", phrase: "123 456 789", state: "waiting", words: "waiting", expires_at: 1 }
    const onVerified = vi.fn()
    mount(
      fakeDepth({
        phraseInvite: () => Promise.resolve(open),
        phrasePoll: () => Promise.resolve<PhraseCheckView>({ ...open, phrase: null, state: "matched", words: "bob@example.com's keys are verified" }),
      }),
      <PhraseInvite login="bob@example.com" onVerified={onVerified} />,
    )
    const user = userEvent.setup()
    await user.click(screen.getByRole("button", { name: "Check with a phrase" }))
    expect(await screen.findByLabelText("Phrase")).toHaveTextContent("123 456 789")
    expect(await screen.findByRole("status", {}, SLOW)).toHaveTextContent("verified")
    expect(onVerified).toHaveBeenCalled()
  })
})

/** Discovery finds the server; the master password is set after the sign-in. */
const joinFirstRun = fakeFirstRun({
  candidates: () => Promise.resolve([readyServer("https://vault.example")]),
  setMasterPassword: () => Promise.resolve({ lines: [], page: new Uint8Array() }),
})

describe("a sign-in that waits for a security key", () => {
  it("opens the server's page and finishes once the key answered", async () => {
    const accountLogin = vi.fn(() => Promise.reject({ code: "approval_needed", message: "use your key", detail: "security_key" }))
    const openVault = vi.fn(() => Promise.resolve({ devices: 2 } as never))
    const secondFactorOpen = vi.fn(() => Promise.resolve())
    const secondFactorWait = vi.fn(() => Promise.resolve(["enrolled"]))
    const onOpened = vi.fn()
    render(
      <ClientProvider
        value={fakeClient({
          accountLogin,
          openVault,
          status: () => Promise.resolve({ devices: 2 } as never),
          probe: () => Promise.resolve({ root: "/r", data: "/d", identity_present: false, device_key_present: false, account_present: false, device_name: "test" }),
        })}
      >
        <DepthClientProvider value={fakeDepth({ secondFactorOpen, secondFactorWait })}>
          <FirstRunClientProvider value={joinFirstRun}>
            <UnlockScreen onOpened={onOpened} />
          </FirstRunClientProvider>
        </DepthClientProvider>
      </ClientProvider>,
    )
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "I already use Lockdown" }))
    await user.click(await screen.findByRole("radio", { name: /vault\.example/ }))
    await user.type(screen.getByLabelText("Login"), "me@example.com")
    await user.type(screen.getByLabelText("Master password"), "a long master password")
    await user.click(screen.getByRole("button", { name: "Continue" }))
    await user.click(await screen.findByRole("button", { name: "Connect" }))
    expect(await screen.findByText(/asks for a security key/)).toBeInTheDocument()
    expect(secondFactorOpen).toHaveBeenCalledWith("https://vault.example")
    await waitFor(() => expect(onOpened).toHaveBeenCalled(), SLOW)
    expect(openVault).toHaveBeenCalledWith(true)
  })

  it("waits for the emailed approval link without opening a page", async () => {
    const accountLogin = vi.fn(() =>
      Promise.reject({
        code: "approval_needed",
        message: "the server emailed a link to s•••@example.com: open it and choose Approve, and this device carries on by itself",
        detail: "email",
      }),
    )
    const openVault = vi.fn(() => Promise.resolve({ devices: 2 } as never))
    const secondFactorOpen = vi.fn(() => Promise.resolve())
    const secondFactorWait = vi.fn(() => Promise.resolve(["enrolled"]))
    const onOpened = vi.fn()
    render(
      <ClientProvider
        value={fakeClient({
          accountLogin,
          openVault,
          status: () => Promise.resolve({ devices: 2 } as never),
          probe: () => Promise.resolve({ root: "/r", data: "/d", identity_present: false, device_key_present: false, account_present: false, device_name: "test" }),
        })}
      >
        <DepthClientProvider value={fakeDepth({ secondFactorOpen, secondFactorWait })}>
          <FirstRunClientProvider value={joinFirstRun}>
            <UnlockScreen onOpened={onOpened} />
          </FirstRunClientProvider>
        </DepthClientProvider>
      </ClientProvider>,
    )
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "I already use Lockdown" }))
    await user.click(await screen.findByRole("radio", { name: /vault\.example/ }))
    await user.type(screen.getByLabelText("Login"), "sean@example.com")
    await user.type(screen.getByLabelText("Master password"), "a long master password")
    await user.click(screen.getByRole("button", { name: "Continue" }))
    await user.click(await screen.findByRole("button", { name: "Connect" }))
    expect(await screen.findByText(/^The server emailed a link to s•••@example\.com/)).toBeInTheDocument()
    expect(screen.queryByRole("button", { name: "Open the page again" })).not.toBeInTheDocument()
    await waitFor(() => expect(onOpened).toHaveBeenCalled(), SLOW)
    expect(secondFactorOpen).not.toHaveBeenCalled()
  })
})
