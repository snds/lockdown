import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import type { ReactNode } from "react"

import { ClientProvider, type Client } from "@/core/client"
import type { Status } from "@/core/types"
import { DepthClientProvider, type DepthClient, type RemindersView } from "@/features/depth/depth-client"
import { malformedLists } from "@/test/a11y"
import { fakeClient } from "@/test/fake-client"
import { fakeDepth } from "@/test/fake-depth"

import { RecoveryView, kitValue } from "./recovery-view"

const ACCOUNT = { login: "sam@example.test" } as unknown as Status["account"]
const status = (recovery: string, account: Status["account"] = null) => ({ recovery, account }) as unknown as Status

// 2026-10-01, noon UTC.
const DRILLED_AT = Date.UTC(2026, 9, 1, 12) / 1000
const REMINDERS: RemindersView = {
  password: { applies: true, due: false, last_at: 1, every_days: 14, next_at: 2 },
  drill: { kit: "drilled", last_drilled_at: DRILLED_AT, next_at: 2, due: false, kit_test_pending: false },
}
const CHECKS = [
  { what: "kit", ok: true, words: "Your kit passed its last test 45 days ago." },
  { what: "secret-key", ok: true, words: "This device keeps your Secret Key." },
  { what: "contact", ok: true, words: "No emergency contact." },
  { what: "crown-jewels", ok: false, words: "2 crown jewels are left out of backups." },
]
const PAGE = new TextEncoder().encode("Secret Key: A1-7GH2K-M9PQR\nRecovery code: QRST-2345-VWXY\n")

function mount(ui: ReactNode, client: Partial<Client> = {}, depth: Partial<DepthClient> = {}) {
  return render(
    <ClientProvider value={fakeClient(client)}>
      <DepthClientProvider value={fakeDepth({ reminders: () => Promise.resolve(REMINDERS), drillChecks: () => Promise.resolve(CHECKS), ...depth })}>{ui}</DepthClientProvider>
    </ClientProvider>,
  )
}

const row = (label: string) => screen.getByRole("group", { name: label })

describe("Settings → Recovery (brief T3)", () => {
  it("says the kit's state in the row: none yet, made and not tested, or tested with its date", () => {
    expect(kitValue("none", null)).toBe("None yet")
    expect(kitValue("made", null)).toBe("Made, not tested")
    expect(kitValue("drilled", DRILLED_AT)).toBe("Tested 2026.10.01")
    expect(kitValue("drilled", null)).toBe("Tested")
  })

  it("draws the rows, the drill's date and the drill's checks as a list (design review R7)", async () => {
    const { container } = mount(<RecoveryView status={status("drilled")} onChanged={() => {}} onNotice={() => {}} />)
    await waitFor(() => expect(row("Recovery kit")).toHaveTextContent("Tested 2026.10.01"))
    expect(row("Recovery drill")).toHaveTextContent("Passed 2026.10.01")
    expect(row("Master password reminder")).toBeInTheDocument()
    const checks = await screen.findByRole("list", { name: "Drill checks" })
    expect(within(checks).getAllByRole("listitem")).toHaveLength(4)
    expect(within(checks).getAllByText("OK")).toHaveLength(3)
    expect(within(checks).getAllByText("Check")).toHaveLength(1)
    expect(screen.getByRole("heading", { name: /What the drill checks/ })).toHaveTextContent("04")
    expect(screen.queryByRole("heading", { level: 1 })).toBeNull()
    expect(malformedLists(container)).toEqual([])
  })

  it("points emergency access at Sharing, where it is built, and opens it (design review R5)", async () => {
    const onEmergency = vi.fn()
    mount(<RecoveryView status={status("made")} onChanged={() => {}} onNotice={() => {}} onEmergency={onEmergency} />)
    const access = row("Emergency access")
    expect(access).toHaveTextContent("Someone you trust can ask for your vaults, after a wait you can stop.")
    expect(access).not.toHaveTextContent("later")
    await userEvent.setup().click(within(access).getByRole("button", { name: /^Open/ }))
    expect(onEmergency).toHaveBeenCalledTimes(1)
  })

  it("replacing the kit asks first, and Cancel changes nothing", async () => {
    const kitCreate = vi.fn(() => Promise.resolve({ lines: ["a new kit"], page: PAGE.slice() }))
    const onChanged = vi.fn()
    mount(<RecoveryView status={status("drilled")} onChanged={onChanged} onNotice={() => {}} />, { kitCreate })
    const user = userEvent.setup()
    await user.click(within(row("Recovery kit")).getByRole("button", { name: /^New kit/ }))
    let question = await screen.findByRole("dialog", { name: "Replace your recovery kit?" })
    expect(question).toHaveTextContent("Your old kit page stops working.")
    await waitFor(() => expect(within(question).getByRole("button", { name: "Cancel" })).toHaveFocus())
    await user.click(within(question).getByRole("button", { name: "Cancel" }))
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull())
    expect(kitCreate).not.toHaveBeenCalled()

    // The yes is the default one, not a destructive red: replacing is a choice worth a second look (X5).
    await user.click(within(row("Recovery kit")).getByRole("button", { name: /^New kit/ }))
    question = await screen.findByRole("dialog", { name: "Replace your recovery kit?" })
    const yes = within(question).getByRole("button", { name: "Replace kit" })
    expect(yes).toHaveAttribute("data-variant", "default")
    await user.click(yes)
    await waitFor(() => expect(kitCreate).toHaveBeenCalledWith({ replace: true }))
    // Then the kit page, shown once.
    expect(await screen.findByRole("dialog", { name: "Your recovery kit" })).toHaveTextContent("Recovery kit // shown once")
    expect(onChanged).toHaveBeenCalled()
  })

  it("with an account, a new kit over the old one asks, then takes the master password", async () => {
    const kitCreate = vi.fn(() => Promise.resolve({ lines: [], page: PAGE.slice() }))
    mount(<RecoveryView status={status("made", ACCOUNT)} onChanged={() => {}} onNotice={() => {}} />, { kitCreate })
    const user = userEvent.setup()
    await user.click(within(row("Recovery kit")).getByRole("button", { name: /^New kit/ }))
    await user.click(within(await screen.findByRole("dialog", { name: "Replace your recovery kit?" })).getByRole("button", { name: "Replace kit" }))
    const form = await screen.findByRole("dialog", { name: "New recovery kit" })
    await user.type(within(form).getByLabelText("Master password"), "a long password")
    await user.click(within(form).getByRole("button", { name: "Create kit" }))
    await waitFor(() => expect(kitCreate).toHaveBeenCalledWith({ replace: true, password: "a long password" }))
    expect(await screen.findByRole("dialog", { name: "Your recovery kit" })).toBeInTheDocument()
  })

  it("with no kit, NEW KIT makes one at once, and the drill waits with its reason", async () => {
    const kitCreate = vi.fn(() => Promise.resolve({ lines: [], page: PAGE.slice() }))
    mount(<RecoveryView status={status("none")} onChanged={() => {}} onNotice={() => {}} />, { kitCreate })
    expect(row("Recovery kit")).toHaveTextContent("None yet")
    expect(row("Recovery drill")).toHaveTextContent("Make a kit first.")
    expect(within(row("Recovery drill")).getByRole("button", { name: /^Run drill/ })).toHaveAttribute("aria-disabled", "true")
    await userEvent.setup().click(within(row("Recovery kit")).getByRole("button", { name: /^New kit/ }))
    await waitFor(() => expect(kitCreate).toHaveBeenCalledWith({ replace: false }))
    expect(screen.queryByRole("dialog", { name: "Replace your recovery kit?" })).toBeNull()
    expect(await screen.findByRole("dialog", { name: "Your recovery kit" })).toBeInTheDocument()
  })

  it("a failed drill keeps the form, with the core's sentence under the field it names", async () => {
    const kitVerify = vi.fn(() => Promise.reject({ code: "refused", message: "the recovery code didn't open this identity: check it against the page, letter by letter" }))
    mount(<RecoveryView status={status("drilled")} onChanged={() => {}} onNotice={() => {}} />, { kitVerify })
    const user = userEvent.setup()
    await user.click(within(row("Recovery drill")).getByRole("button", { name: /^Run drill/ }))
    const form = await screen.findByRole("dialog", { name: "Type your recovery kit" })
    expect(within(form).getByRole("switch", { name: "Record a passing check as the drill" })).toBeChecked()
    await user.type(within(form).getByLabelText("Secret Key"), "A1-7GH2K")
    await user.type(within(form).getByLabelText("Recovery code"), "QRST-2345")
    await user.click(within(form).getByRole("button", { name: "Check kit" }))
    await waitFor(() => expect(kitVerify).toHaveBeenCalledWith({ secretKey: "A1-7GH2K", recoveryCode: "QRST-2345", password: undefined, drill: true }))
    const code = within(form).getByLabelText("Recovery code")
    await waitFor(() => expect(code).toHaveAccessibleDescription(/the recovery code didn't open this identity/))
    expect(within(form).getByLabelText("Secret Key")).not.toHaveAccessibleDescription(/didn't open/)
    expect(screen.getByRole("dialog", { name: "Type your recovery kit" })).toBeInTheDocument()
  })

  it("a passing drill closes the form and says the core's lines in a toast", async () => {
    const onNotice = vi.fn()
    const kitVerify = vi.fn(() => Promise.resolve(["recovery code: opens this identity", "drill recorded"]))
    mount(<RecoveryView status={status("made")} onChanged={() => {}} onNotice={onNotice} />, { kitVerify })
    const user = userEvent.setup()
    await user.click(within(row("Recovery drill")).getByRole("button", { name: /^Run drill/ }))
    const form = await screen.findByRole("dialog", { name: "Type your recovery kit" })
    await user.type(within(form).getByLabelText("Secret Key"), "A1-7GH2K")
    await user.type(within(form).getByLabelText("Recovery code"), "QRST-2345")
    await user.click(within(form).getByRole("button", { name: "Check kit" }))
    await waitFor(() => expect(onNotice).toHaveBeenCalledWith("recovery code: opens this identity drill recorded"))
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull())
  })
})
