import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import type { ReactNode } from "react"
import { fakeDepth } from "@/test/fake-depth"
import { DepthClientProvider, type DepthClient, type PhraseCheckView } from "./depth-client"
import { PhraseInvite } from "./key-check"

function mount(depth: DepthClient, ui: ReactNode) {
  return render(<DepthClientProvider value={depth}>{ui}</DepthClientProvider>)
}

const SLOW = { timeout: 4000 }

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
    // While it waits, the spinner is the status ("Loading", since unicron 0.23); then the sentence is.
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("verified"), SLOW)
    expect(onVerified).toHaveBeenCalled()
  })
})
