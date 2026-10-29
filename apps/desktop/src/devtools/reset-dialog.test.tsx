import { act, render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import type { Status } from "@/core/types"

import { DevtoolsClientProvider } from "./devtools-client"
import { fakeDevtools } from "./fake-devtools"
import ResetMount from "./reset-mount"

const status = {} as Status

describe("Developer → Reset Lockdown…", () => {
  it("lists what goes and what stays, and runs only once reset is typed", async () => {
    const resetRun = vi.fn(() => Promise.resolve())
    const { client, pick } = fakeDevtools(status, { resetRun })
    render(
      <DevtoolsClientProvider value={client}>
        <ResetMount />
      </DevtoolsClientProvider>,
    )
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument()
    act(() => pick("reset"))
    const dialog = await screen.findByRole("dialog")
    expect(dialog).toHaveTextContent("Reset Lockdown on this computer?")
    expect(dialog).toHaveTextContent("It can't be undone.")
    expect(dialog).toHaveTextContent("An account on a server is not deleted")
    expect(await within(dialog).findByText("/home/sam/.local/share/snds/lockdown")).toBeInTheDocument()
    expect(dialog).toHaveTextContent("One entry in this computer's keychain under design.snds.lockdown")
    expect(dialog).toHaveTextContent("Everything this app's web view has stored.")
    expect(dialog).toHaveTextContent("Only Lockdown uses it, so it is erased")
    expect(dialog).toHaveTextContent("Exports and backups saved in folders you chose.")

    const go = within(dialog).getByRole("button", { name: "Erase and restart" })
    expect(go).toBeDisabled()
    const user = userEvent.setup()
    await user.type(within(dialog).getByLabelText("Type reset to confirm"), "rese")
    expect(go).toBeDisabled()
    await user.type(within(dialog).getByLabelText("Type reset to confirm"), "t")
    expect(go).toBeEnabled()
    await user.click(go)
    await waitFor(() => expect(resetRun).toHaveBeenCalledWith("reset"))
    expect(await within(dialog).findByRole("status")).toHaveTextContent("Erased. Lockdown is restarting…")
  })

  it("shows the shell's refusal and keeps the dialog", async () => {
    const { client, pick } = fakeDevtools(status, {
      resetRun: () => Promise.reject({ code: "refused", message: "the web view's storage wasn't cleared: busy" }),
    })
    render(
      <DevtoolsClientProvider value={client}>
        <ResetMount />
      </DevtoolsClientProvider>,
    )
    act(() => pick("reset"))
    const dialog = await screen.findByRole("dialog")
    const user = userEvent.setup()
    await user.type(await within(dialog).findByLabelText("Type reset to confirm"), "reset")
    await user.click(within(dialog).getByRole("button", { name: "Erase and restart" }))
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("the web view's storage wasn't cleared: busy")
  })

  it("starts empty each time it opens", async () => {
    const { client, pick } = fakeDevtools(status)
    render(
      <DevtoolsClientProvider value={client}>
        <ResetMount />
      </DevtoolsClientProvider>,
    )
    act(() => pick("reset"))
    const user = userEvent.setup()
    await user.type(await screen.findByLabelText("Type reset to confirm"), "reset")
    await user.click(screen.getByRole("button", { name: "Cancel" }))
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument())
    act(() => pick("reset"))
    expect(await screen.findByLabelText("Type reset to confirm")).toHaveValue("")
  })
})
