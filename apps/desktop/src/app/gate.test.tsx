import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useState } from "react"

import { TooltipProvider } from "@unicron/ui"

import { ClientProvider } from "@/core/client"
import type { GateInput } from "@/core/types"
import { fakeClient } from "@/test/fake-client"

import { GateProvider, LOCKED_WORDS, gateWords, useGate, wrongPasswordWords } from "./gate"

/*
 * Brief X5, the gate: unicron's CrownJewelGate on the password route, a
 * question on the window route that says nothing is checked and whose yes is
 * the action's own verb; three tries, then the vault locks; Escape cancels
 * and never confirms; a lock closes it at once.
 */

const confirmButton = () => within(screen.getByRole("dialog")).getByRole("button", { name: "Confirm" })

/** The core: asks with `message` until `ok` says the gate input passes. */
const core = (message: string, detail: "password" | "window", ok: (g: GateInput) => Promise<string> | null) =>
  vi.fn((g: GateInput): Promise<string> => ok(g) ?? Promise.reject({ code: "approval_needed", message, detail }))

function Asker({ check }: { detail: "password" | "window"; check: (gate: GateInput) => Promise<string> }) {
  const { run, dismiss } = useGate()
  const [how, setHow] = useState("")
  return (
    <>
      <button onClick={() => void run(check).then(setHow, (e: { message: string }) => setHow(e.message))}>
        Go
      </button>
      <button onClick={dismiss}>Lock now</button>
      <output data-testid="how">{how}</output>
    </>
  )
}

function mount(props: Parameters<typeof Asker>[0], lock = vi.fn(() => Promise.resolve())) {
  render(
    <ClientProvider value={fakeClient({ lock, approvalRoute: () => Promise.resolve({ route: props.detail, method: props.detail }) })}>
      <TooltipProvider>
        <GateProvider>
          <Asker {...props} />
        </GateProvider>
      </TooltipProvider>
    </ClientProvider>,
  )
  return { lock, user: userEvent.setup() }
}

describe("the gate's words", () => {
  it("reads the core's message: the reason, the question and the verb", () => {
    expect(gateWords("your master password is needed to export the vault")).toEqual({ reason: "To export the vault.", question: "Export the vault?", verb: "Export", what: null })
    expect(gateWords("confirm in this window to copy this protected code").verb).toBe("Copy")
    expect(gateWords("confirm in this window to copy this protected code").what).toBe("copied")
    expect(gateWords("confirm in this window to let them back in").verb).toBe("Let them back in")
    expect(gateWords("confirm in this window to back up the vault to a folder").verb).toBe("Back up")
    expect(wrongPasswordWords(2)).toBe("That isn't the master password. 2 tries left.")
    expect(wrongPasswordWords(1)).toBe("That isn't the master password. 1 try left.")
  })
})

describe("the password route", () => {
  it("three wrong tries lock the vault, with the reason", async () => {
    // The first call (no password) is the core asking; every typed one is wrong.
    const check = core("your master password is needed to export the vault", "password", (g) => (g.password ? Promise.reject({ code: "refused", message: "the password didn't match" }) : null))
    const { lock, user } = mount({ detail: "password", check })
    await user.click(screen.getByRole("button", { name: "Go" }))
    const gate = await screen.findByRole("dialog", { name: "Confirm it's you" })
    expect(gate).toHaveTextContent("To export the vault.")
    for (const left of [2, 1]) {
      await user.type(screen.getByLabelText("Account password"), "hunter2")
      await waitFor(() => expect(confirmButton()).toBeEnabled())
      await user.click(confirmButton())
      expect(await screen.findByText(wrongPasswordWords(left))).toBeInTheDocument()
    }
    await user.type(screen.getByLabelText("Account password"), "hunter2")
    await waitFor(() => expect(confirmButton()).toBeEnabled())
    await user.click(confirmButton())
    await waitFor(() => expect(screen.getByTestId("how")).toHaveTextContent(LOCKED_WORDS))
    expect(lock).toHaveBeenCalledTimes(1)
    expect(check.mock.calls.filter(([g]) => g.password)).toHaveLength(3)
  })

  it("a lock closes it at once, as a cancel", async () => {
    const { user } = mount({ detail: "password", check: core("your master password is needed to export the vault", "password", () => null) })
    await user.click(screen.getByRole("button", { name: "Go" }))
    await screen.findByRole("dialog", { name: "Confirm it's you" })
    // The lock comes from the shell, not from the gate's own buttons.
    screen.getByRole("button", { name: "Lock now", hidden: true }).click()
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull())
    expect(screen.getByTestId("how")).toHaveTextContent("Cancelled; nothing happened.")
  })
})

describe("the window route", () => {
  it("says plainly that nothing is checked, focuses Cancel, and its yes is the action's verb", async () => {
    const check = core("confirm in this window to export the vault", "window", (g) => (g.confirmed ? Promise.resolve("done") : null))
    const { user } = mount({ detail: "window", check })
    await user.click(screen.getByRole("button", { name: "Go" }))
    const question = await screen.findByRole("dialog", { name: "Export the vault?" })
    expect(question).toHaveTextContent(/nothing is checked here/)
    expect(within(question).queryByRole("button", { name: "Confirm" })).toBeNull()
    await waitFor(() => expect(within(question).getByRole("button", { name: "Cancel" })).toHaveFocus())
    await user.click(within(question).getByRole("button", { name: "Export" }))
    await waitFor(() => expect(screen.getByTestId("how")).toHaveTextContent("done"))
    expect(check).toHaveBeenLastCalledWith({ confirmed: true })
  })

  it("Escape cancels, even with the yes in focus", async () => {
    const check = core("confirm in this window to export the vault", "window", (g) => (g.confirmed ? Promise.resolve("done") : null))
    const { user } = mount({ detail: "window", check })
    await user.click(screen.getByRole("button", { name: "Go" }))
    const question = await screen.findByRole("dialog", { name: "Export the vault?" })
    within(question).getByRole("button", { name: "Export" }).focus()
    await user.keyboard("{Escape}")
    await waitFor(() => expect(screen.getByTestId("how")).toHaveTextContent("Cancelled; nothing happened."))
    expect(check).not.toHaveBeenCalledWith({ confirmed: true })
  })
})
