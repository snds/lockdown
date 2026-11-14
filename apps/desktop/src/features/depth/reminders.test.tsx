import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import type { ReactNode } from "react"
import { fakeDepth } from "@/test/fake-depth"
import { DepthClientProvider, type DepthClient, type RemindersView } from "./depth-client"
import { RemindersPrompt } from "./reminders"

function mount(depth: DepthClient, ui: ReactNode) {
  return render(<DepthClientProvider value={depth}>{ui}</DepthClientProvider>)
}

const reminders = (over: { password?: Partial<RemindersView["password"]>; drill?: Partial<RemindersView["drill"]> }): RemindersView => ({
  password: { applies: true, due: false, last_at: 1, every_days: 14, next_at: 2, ...over.password },
  drill: { kit: "drilled", last_drilled_at: 1, next_at: 2, due: false, kit_test_pending: false, ...over.drill },
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
