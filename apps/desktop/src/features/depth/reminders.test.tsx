import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { AppFrame } from "@unicron/ui"

import { WaitingHost, NeedsYouSlot } from "@/app/needs-you"
import { fakeDepth } from "@/test/fake-depth"
import { DepthClientProvider, type DepthClient, type RemindersView } from "./depth-client"
import { DrillSection, PasswordReminderSetting, RemindersPrompt, SKIP_WORDS, dueReminders, useRecoveryFacts } from "./reminders"

/*
 * The reminders after unlock are needs-you rows 7 to 9 (brief X3, Q4), with
 * every word and skip rule of D251, D252 and D253 kept: the kit test is
 * skipped only through the question whose one yes is "I understand an
 * untested kit can fail"; the password is typed in a one-field form, or put
 * off with Tomorrow; the drill runs, or waits with Next week.
 */

function mount(depth: DepthClient) {
  return render(
    <AppFrame layout="expanded">
      <WaitingHost place={{ sharing: false, home: true, item: null }}>
        <DepthClientProvider value={depth}>
          <NeedsYouSlot />
          <RemindersPrompt onNotice={() => {}} onChanged={() => {}} />
        </DepthClientProvider>
      </WaitingHost>
    </AppFrame>,
  )
}

const reminders = (over: { password?: Partial<RemindersView["password"]>; drill?: Partial<RemindersView["drill"]> }): RemindersView => ({
  password: { applies: true, due: false, last_at: 1, every_days: 14, next_at: 2, ...over.password },
  drill: { kit: "drilled", last_drilled_at: 1, next_at: 2, due: false, kit_test_pending: false, ...over.drill },
})
const kitPending = reminders({ drill: { kit: "created", last_drilled_at: null, kit_test_pending: true, due: true } })

describe("the reminders, as needs-you rows 7 to 9", () => {
  it("orders what is due as the table does: the kit test, the password, the drill", () => {
    expect(dueReminders(null)).toEqual([])
    expect(dueReminders(reminders({}))).toEqual([])
    expect(dueReminders(reminders({ drill: { kit_test_pending: true, due: true }, password: { due: true } }))).toEqual(["kit-test", "password", "drill"])
    expect(dueReminders(reminders({ drill: { due: true }, password: { due: true, applies: false } }))).toEqual(["drill"])
    expect(dueReminders(reminders({ drill: { due: true, kit: "none" } }))).toEqual([])
  })

  it("is a block, not a dialog, and shows one at a time with `// +1`", async () => {
    mount(fakeDepth({ reminders: () => Promise.resolve(reminders({ drill: { kit_test_pending: true, due: true } })) }))
    const block = await screen.findByRole("region", { name: /Recovery kit \/\/ not tested/ })
    expect(block).toHaveTextContent("// +1")
    expect(block).toHaveTextContent("Test your recovery kit.")
    expect(screen.queryByRole("dialog")).toBeNull()
  })

  it("skips the first-run kit test only through the question with D251's words", async () => {
    const kitTestSkip = vi.fn((understood: boolean) =>
      understood ? Promise.resolve(reminders({})) : Promise.reject({ code: "refused", message: "confirm", detail: "confirm_skip" }),
    )
    mount(fakeDepth({ reminders: () => Promise.resolve(kitPending), kitTestSkip }))
    const user = userEvent.setup()
    const block = await screen.findByRole("region", { name: /Recovery kit \/\/ not tested/ })
    expect(within(block).getByRole("button", { name: "Test it now" })).toBeInTheDocument()
    expect(screen.queryByRole("button", { name: "Skip for now" })).toBeNull()

    // SKIP asks; Cancel (where focus starts) changes nothing.
    await user.click(within(block).getByRole("button", { name: "Skip" }))
    let question = await screen.findByRole("dialog", { name: "Skip the kit test?" })
    expect(question).toHaveTextContent("Skip it, and Lockdown asks again in a week.")
    await waitFor(() => expect(within(question).getByRole("button", { name: "Cancel" })).toHaveFocus())
    await user.click(within(question).getByRole("button", { name: "Cancel" }))
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull())
    expect(kitTestSkip).not.toHaveBeenCalled()

    // The one yes is the sentence the person accepts.
    await user.click(within(screen.getByRole("region", { name: /Recovery kit/ })).getByRole("button", { name: "Skip" }))
    question = await screen.findByRole("dialog", { name: "Skip the kit test?" })
    await user.click(within(question).getByRole("button", { name: SKIP_WORDS }))
    await waitFor(() => expect(kitTestSkip).toHaveBeenCalledWith(true))
    expect(kitTestSkip).toHaveBeenCalledTimes(1)
    await waitFor(() => expect(screen.queryByRole("region", { name: /Recovery kit/ })).toBeNull())
  })

  it("asks for the master password in a one-field form and checks it", async () => {
    const passwordCheck = vi.fn(() => Promise.resolve(reminders({})))
    mount(fakeDepth({ reminders: () => Promise.resolve(reminders({ password: { due: true } })), passwordCheck }))
    const user = userEvent.setup()
    const block = await screen.findByRole("region", { name: /Master password \/\/ 14 days/ })
    await user.click(within(block).getByRole("button", { name: "Type it" }))
    const form = await screen.findByRole("dialog", { name: "Your master password, once more" })
    await user.type(within(form).getByLabelText("Master password"), "a long password")
    await user.click(within(form).getByRole("button", { name: "Check" }))
    await waitFor(() => expect(passwordCheck).toHaveBeenCalledWith("a long password"))
    await waitFor(() => expect(screen.queryByRole("region", { name: /Master password/ })).toBeNull())
  })

  it("puts the password off a day with Tomorrow, and the drill a week with Next week", async () => {
    const passwordSnooze = vi.fn(() => Promise.resolve(reminders({ drill: { due: true } })))
    const drillSnooze = vi.fn(() => Promise.resolve(reminders({})))
    mount(fakeDepth({ reminders: () => Promise.resolve(reminders({ password: { due: true }, drill: { due: true } })), passwordSnooze, drillSnooze }))
    const user = userEvent.setup()
    await user.click(within(await screen.findByRole("region", { name: /Master password/ })).getByRole("button", { name: "Tomorrow" }))
    const drill = await screen.findByRole("region", { name: /Recovery drill \/\/ due/ })
    expect(passwordSnooze).toHaveBeenCalledTimes(1)
    expect(within(drill).getByRole("button", { name: "Run drill" })).toBeInTheDocument()
    await user.click(within(drill).getByRole("button", { name: "Next week" }))
    await waitFor(() => expect(drillSnooze).toHaveBeenCalledTimes(1))
    await waitFor(() => expect(screen.queryByRole("region", { name: /Recovery drill/ })).toBeNull())
  })

  it("says a failed answer in the core's words, on the block", async () => {
    mount(fakeDepth({ reminders: () => Promise.resolve(reminders({ password: { due: true } })), passwordSnooze: () => Promise.reject({ code: "refused", message: "The vault is locked." }) }))
    const user = userEvent.setup()
    await user.click(within(await screen.findByRole("region", { name: /Master password/ })).getByRole("button", { name: "Tomorrow" }))
    await waitFor(() => expect(screen.getByRole("region", { name: /Master password/ })).toHaveTextContent("The vault is locked."))
  })
})

describe("the drill's part of Recovery (brief T3)", () => {
  function Drill() {
    const facts = useRecoveryFacts()
    return (
      <>
        <PasswordReminderSetting facts={facts} />
        <DrillSection facts={facts} />
      </>
    )
  }
  const mountDrill = (depth: DepthClient) =>
    render(
      <DepthClientProvider value={depth}>
        <Drill />
      </DepthClientProvider>,
    )
  const CHECKS = [
    { what: "kit", ok: true, words: "Your kit passed its last test 45 days ago." },
    { what: "crown-jewels", ok: false, words: "2 crown jewels are left out of backups." },
  ]

  it("lists what the drill checks, each with OK or CHECK in words, and counts them", async () => {
    mountDrill(fakeDepth({ reminders: () => Promise.resolve(reminders({})), drillChecks: () => Promise.resolve(CHECKS) }))
    const list = await screen.findByRole("list", { name: "Drill checks" })
    const [kit, crown] = within(list).getAllByRole("listitem")
    expect(kit).toHaveTextContent("Your kit passed its last test 45 days ago.")
    expect(kit).toHaveTextContent("OK")
    expect(crown).toHaveTextContent("Check")
    expect(screen.getByRole("heading", { name: /What the drill checks/ })).toHaveTextContent("02")
  })

  it("asks how often for the master password with three chips, and saves a pick at once", async () => {
    const setPasswordReminderDays = vi.fn((days: number) => Promise.resolve(reminders({ password: { every_days: days } })))
    mountDrill(fakeDepth({ reminders: () => Promise.resolve(reminders({})), drillChecks: () => Promise.resolve(CHECKS), setPasswordReminderDays }))
    const group = await screen.findByRole("radiogroup", { name: "Ask for the master password every" })
    expect(within(group).getAllByRole("radio").map((r) => r.textContent)).toEqual(["7 days", "14 days", "30 days"])
    expect(within(group).getByRole("radio", { name: "14 days" })).toBeChecked()
    await userEvent.setup().click(within(group).getByRole("radio", { name: "30 days" }))
    await waitFor(() => expect(setPasswordReminderDays).toHaveBeenCalledWith(30))
    await waitFor(() => expect(within(group).getByRole("radio", { name: "30 days" })).toBeChecked())
  })

  it("keeps the old interval with the core's words under the row when a save is refused", async () => {
    mountDrill(
      fakeDepth({
        reminders: () => Promise.resolve(reminders({})),
        drillChecks: () => Promise.resolve(CHECKS),
        setPasswordReminderDays: () => Promise.reject({ code: "refused", message: "The vault is locked." }),
      }),
    )
    const group = await screen.findByRole("radiogroup", { name: "Ask for the master password every" })
    await userEvent.setup().click(within(group).getByRole("radio", { name: "7 days" }))
    expect(await screen.findByRole("alert")).toHaveTextContent("The vault is locked.")
    expect(within(group).getByRole("radio", { name: "14 days" })).toBeChecked()
  })

  it("leaves the interval out where unlocking asks for the password anyway", async () => {
    mountDrill(fakeDepth({ reminders: () => Promise.resolve(reminders({ password: { applies: false } })), drillChecks: () => Promise.resolve(CHECKS) }))
    await screen.findByRole("list", { name: "Drill checks" })
    expect(screen.queryByRole("radiogroup")).toBeNull()
  })

  it("a failed read of the checks is a critical Banner with Try again, which reads them again", async () => {
    const drillChecks = vi.fn().mockRejectedValueOnce({ code: "refused", message: "The server didn't answer." }).mockResolvedValueOnce(CHECKS)
    mountDrill(fakeDepth({ reminders: () => Promise.resolve(reminders({})), drillChecks }))
    expect(await screen.findByRole("alert")).toHaveTextContent("The server didn't answer.")
    await userEvent.setup().click(screen.getByRole("button", { name: "Try again" }))
    expect(await screen.findByRole("list", { name: "Drill checks" })).toBeInTheDocument()
    expect(screen.queryByRole("alert")).toBeNull()
  })
})
