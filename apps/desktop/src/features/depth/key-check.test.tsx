import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import type { ReactNode } from "react"
import { fakeDepth } from "@/test/fake-depth"
import { DepthClientProvider, type DepthClient, type PhraseCheckView, type PhraseIncoming } from "./depth-client"
import { PhraseIncomingList, PhraseInvite } from "./key-check"

function mount(depth: DepthClient, ui: ReactNode) {
  return render(<DepthClientProvider value={depth}>{ui}</DepthClientProvider>)
}

const SLOW = { timeout: 4000 }

describe("checking keys with a phrase", () => {
  it("shows the nine digits to say in groups and reports the match", async () => {
    const open: PhraseCheckView = { verify_id: "v1", role: "inviter", login: "bob@example.com", account_id: "a", phrase: "123 456 789", state: "waiting", words: "Waiting for bob@example.com to type it.", expires_at: 1 }
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
    const dialog = await screen.findByRole("dialog", { name: "Tell bob@example.com this phrase" })
    // DigitGroups: three groups, read one digit at a time with a pause at each group.
    expect(await within(dialog).findByText("Phrase: 1 2 3, 4 5 6, 7 8 9")).toBeInTheDocument()
    expect(dialog).toHaveTextContent("never in a message")
    // While it waits, the line says so; then it says the core's words.
    expect(within(dialog).getByRole("status")).toHaveTextContent("Waiting for bob@example.com")
    await waitFor(() => expect(within(dialog).getByRole("status")).toHaveTextContent("verified"), SLOW)
    expect(onVerified).toHaveBeenCalled()
  })

  it("closing a check that still waits calls it off", async () => {
    const open: PhraseCheckView = { verify_id: "v2", role: "inviter", login: "bob@example.com", account_id: "a", phrase: "123 456 789", state: "waiting", words: "Waiting.", expires_at: 1 }
    const phraseCancel = vi.fn(() => Promise.resolve())
    mount(fakeDepth({ phraseInvite: () => Promise.resolve(open), phrasePoll: () => Promise.resolve(open), phraseCancel }), <PhraseInvite login="bob@example.com" onVerified={() => {}} />)
    const user = userEvent.setup()
    await user.click(screen.getByRole("button", { name: "Check with a phrase" }))
    const dialog = await screen.findByRole("dialog")
    await within(dialog).findByText("Phrase: 1 2 3, 4 5 6, 7 8 9")
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }))
    expect(phraseCancel).toHaveBeenCalledWith("v2")
  })

  it("a check someone started: KEY CHECKS FOR YOU, one try at their phrase", async () => {
    const incoming: PhraseIncoming = { verify_id: "v3", login: "ann@example.com", account_id: "acc-ann", expires_at: 1 }
    const phraseAnswer = vi.fn(() => Promise.resolve<PhraseCheckView>({ verify_id: "v3", role: "joiner", login: "ann@example.com", account_id: "acc-ann", phrase: null, state: "matched", words: "ann@example.com's keys are verified", expires_at: 1 }))
    const onVerified = vi.fn()
    mount(fakeDepth({ phraseIncoming: () => Promise.resolve([incoming]), phraseAnswer }), <PhraseIncomingList online onVerified={onVerified} />)
    const user = userEvent.setup()
    const list = await screen.findByRole("list", { name: "Key checks for you" })
    await user.click(within(list).getByRole("button", { name: "Type their phrase" }))
    const dialog = await screen.findByRole("dialog", { name: "Check keys with ann@example.com" })
    expect(dialog).toHaveTextContent("You get one try. If it's wrong, they start a new check.")
    // Not nine digits: said under the field, and nothing is sent.
    await user.type(within(dialog).getByLabelText("Their phrase"), "12")
    await user.click(within(dialog).getByRole("button", { name: "Check" }))
    expect(await within(dialog).findByText("The phrase is nine digits.")).toBeInTheDocument()
    expect(phraseAnswer).not.toHaveBeenCalled()
    await user.clear(within(dialog).getByLabelText("Their phrase"))
    await user.type(within(dialog).getByLabelText("Their phrase"), "123-456-789")
    await user.click(within(dialog).getByRole("button", { name: "Check" }))
    await waitFor(() => expect(phraseAnswer).toHaveBeenCalledWith("v3", "123456789"))
    expect(await within(dialog).findByRole("status")).toHaveTextContent("verified")
  })
})
